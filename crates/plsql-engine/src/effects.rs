//! Source-backed routine effects and invocation closures.
//!
//! A missing call target or an unclassified construct is evidence of
//! uncertainty, never evidence of purity. Catalog validity and SYS allowlist
//! decisions belong to the downstream adapter.

use std::collections::{BTreeMap, BTreeSet};

use plsql_ir::{Expr, PackageUnitKind, Statement, expr::lower_expression, stmt::SqlVerb};
use serde::{Deserialize, Serialize};

pub const ROUTINE_EFFECTS_CONTRACT: &str = "RoutineEffectsV1";
pub const INVOCATION_CLOSURE_CONTRACT: &str = "InvocationClosureV1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum OperatorStatementClass {
    DefaultEditionFlip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum RoutineEffect {
    ReadDb,
    RowLock,
    SessionState,
    Dml,
    Ddl,
    Admin,
    SequenceAdvance,
    OperatorOnly(OperatorStatementClass),
    TxnControl,
    Autonomous,
    DynamicSql,
    ExternalIo,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum RoutineEffectsContract {
    #[default]
    #[serde(rename = "RoutineEffectsV1")]
    V1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineEffectsV1 {
    contract: RoutineEffectsContract,
    pub effects: BTreeSet<RoutineEffect>,
}

impl Default for RoutineEffectsV1 {
    fn default() -> Self {
        Self {
            contract: RoutineEffectsContract::V1,
            effects: BTreeSet::new(),
        }
    }
}

impl RoutineEffectsV1 {
    pub const CONTRACT: &'static str = ROUTINE_EFFECTS_CONTRACT;

    #[must_use]
    pub fn new(effects: impl IntoIterator<Item = RoutineEffect>) -> Self {
        Self {
            contract: RoutineEffectsContract::V1,
            effects: effects.into_iter().collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = RoutineEffect> + '_ {
        self.effects.iter().copied()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvocationShapeV1 {
    /// Exact canonical identity: unquoted components are upper-case and
    /// quoted components retain their original case.
    pub routine_id: String,
    pub positional_count: usize,
    /// Exact names supplied through named notation.
    pub named_arguments: BTreeSet<String>,
    /// Argument categories known at the call site. The source-only engine
    /// retains them but does not select an overload by type: Oracle's
    /// implicit conversions need catalog resolution to prove a match.
    pub argument_types: Vec<InvocationArgType>,
    /// Catalog `ALL_ARGUMENTS.OVERLOAD` when the caller supplied it.
    pub overload: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum InvocationArgType {
    Bind,
    Number,
    Text,
    Null,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum InvocationClosureContract {
    #[default]
    #[serde(rename = "InvocationClosureV1")]
    V1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationClosureV1 {
    contract: InvocationClosureContract,
    pub root: String,
    pub units: BTreeSet<String>,
    pub effects: RoutineEffectsV1,
    pub clean: bool,
    pub unknown_reasons: BTreeSet<String>,
}

impl InvocationClosureV1 {
    pub const CONTRACT: &'static str = INVOCATION_CLOSURE_CONTRACT;

    fn new(root: String) -> Self {
        Self {
            contract: InvocationClosureContract::V1,
            root,
            units: BTreeSet::new(),
            effects: RoutineEffectsV1::default(),
            clean: true,
            unknown_reasons: BTreeSet::new(),
        }
    }

    fn unknown(&mut self, reason: impl Into<String>) {
        self.effects.effects.insert(RoutineEffect::Unknown);
        self.unknown_reasons.insert(reason.into());
        self.clean = false;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParamShape {
    pub name: String,
    pub has_default: bool,
    pub type_text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectUnit {
    pub id: String,
    pub package: Option<String>,
    pub kind: Option<PackageUnitKind>,
    pub statements: Vec<Statement>,
    pub params: Vec<ParamShape>,
    pub source_complete: bool,
    pub source_reasons: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageShape {
    pub has_spec: bool,
    pub has_body: bool,
    pub complete: bool,
    pub spec_members: BTreeSet<String>,
    pub body_members: BTreeSet<String>,
    pub state_variables: BTreeSet<String>,
    pub source_reasons: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InvocationIndexV1 {
    pub units: BTreeMap<String, EffectUnit>,
    pub packages: BTreeMap<String, PackageShape>,
}

impl InvocationIndexV1 {
    pub fn insert_unit(&mut self, unit: EffectUnit) {
        if let Some(existing) = self.units.get_mut(&unit.id) {
            existing.statements.extend(unit.statements);
            if !unit.params.is_empty() {
                existing.params = unit.params;
            }
            existing.source_complete &= unit.source_complete;
            existing.source_reasons.extend(unit.source_reasons);
        } else {
            self.units.insert(unit.id.clone(), unit);
        }
    }

    /// Effects of one source unit before transitive call propagation. This
    /// feeds typed graph edges; invocation admission uses `closure` instead.
    #[must_use]
    pub fn local_effects(&self, id: &str) -> BTreeSet<RoutineEffect> {
        let Some(unit) = self.units.get(id) else {
            return BTreeSet::from([RoutineEffect::Unknown]);
        };
        let mut result = InvocationClosureV1::new(id.to_string());
        let mut calls = Vec::new();
        inspect_statements(&unit.statements, unit, self, 0, &mut result, &mut calls);
        result.effects.effects
    }

    #[must_use]
    pub fn closure(&self, shape: &InvocationShapeV1) -> InvocationClosureV1 {
        let mut out = InvocationClosureV1::new(shape.routine_id.clone());
        let mut visiting = BTreeSet::new();
        self.visit(shape, &mut out, &mut visiting);
        if out.effects.effects.contains(&RoutineEffect::Unknown) {
            out.clean = false;
        }
        out
    }

    fn visit(
        &self,
        shape: &InvocationShapeV1,
        out: &mut InvocationClosureV1,
        visiting: &mut BTreeSet<(String, usize, BTreeSet<String>, Vec<InvocationArgType>)>,
    ) {
        let candidates = self.resolve(shape);
        if candidates.len() != 1 {
            out.unknown(format!(
                "unresolved or ambiguous callee: {}",
                shape.routine_id
            ));
            return;
        }
        let id = candidates[0].clone();
        if id.contains("#?") {
            out.unknown(format!("ambiguous overload identity: {id}"));
        }
        let visit_key = (
            id.clone(),
            shape.positional_count,
            shape.named_arguments.clone(),
            shape.argument_types.clone(),
        );
        if !visiting.insert(visit_key) {
            // Recursion is safe only when every edge is known. A cycle is
            // closed by this visited set; earlier visits already supplied
            // that unit's effects.
            return;
        }
        let Some(unit) = self.units.get(&id) else {
            out.unknown(format!("source unavailable: {id}"));
            return;
        };
        out.units.insert(id.clone());
        if !unit.source_complete {
            out.unknown(format!("incomplete source: {id}"));
        }
        for reason in &unit.source_reasons {
            out.unknown(format!("{id}: {reason}"));
        }
        if let Some(package) = &unit.package {
            match self.packages.get(package) {
                Some(meta) if meta.complete && meta.has_spec && meta.has_body => {
                    if matches!(unit.kind, Some(PackageUnitKind::Member { .. }))
                        && meta.spec_members.contains(&id)
                        && !meta.body_members.contains(&id)
                    {
                        out.unknown(format!("member body unavailable: {id}"));
                    }
                    for reason in &meta.source_reasons {
                        out.unknown(format!("{package}: {reason}"));
                    }
                }
                _ => out.unknown(format!("incomplete package: {package}")),
            }
            // Package instantiation executes both parts' declaration
            // initializers and the body initialization section.
            for suffix in ["#spec_initializers", "#body_initializers", "#init"] {
                let init_id = format!("{package}{suffix}");
                if self.units.contains_key(&init_id) && init_id != id {
                    self.visit(
                        &InvocationShapeV1 {
                            routine_id: init_id,
                            ..InvocationShapeV1::default()
                        },
                        out,
                        visiting,
                    );
                }
            }
        }
        if matches!(unit.kind, Some(PackageUnitKind::Member { .. })) {
            for (position, param) in unit.params.iter().enumerate() {
                let supplied = position < shape.positional_count
                    || shape.named_arguments.contains(&param.name);
                if !supplied && param.has_default {
                    // Defaults may occur in either spec or body. Evaluating
                    // both is conservative if the two declarations differ.
                    for part in ["spec", "body"] {
                        let default_id = format!("{id}({})@{part}#default", param.name);
                        if self.units.contains_key(&default_id) {
                            self.visit(
                                &InvocationShapeV1 {
                                    routine_id: default_id,
                                    ..InvocationShapeV1::default()
                                },
                                out,
                                visiting,
                            );
                        }
                    }
                }
            }
        }
        let mut calls = Vec::new();
        inspect_statements(&unit.statements, unit, self, 0, out, &mut calls);
        for call in calls {
            self.visit(&call, out, visiting);
        }
    }

    fn resolve(&self, shape: &InvocationShapeV1) -> Vec<String> {
        if self.units.contains_key(&shape.routine_id) {
            let unit = &self.units[&shape.routine_id];
            if matches!(unit.kind, Some(PackageUnitKind::Member { .. })) {
                let supplied = shape.positional_count + shape.named_arguments.len();
                let required = unit.params.iter().filter(|p| !p.has_default).count();
                if supplied < required || supplied > unit.params.len() {
                    return Vec::new();
                }
            }
            if let Some(overload) = shape.overload
                && !shape.routine_id.ends_with(&format!("#{overload}"))
            {
                return Vec::new();
            }
            return vec![shape.routine_id.clone()];
        }
        let mut ids = Vec::new();
        for (id, unit) in &self.units {
            if !matches!(unit.kind, Some(PackageUnitKind::Member { .. })) {
                continue;
            }
            if id.split('#').next() != Some(shape.routine_id.as_str()) {
                continue;
            }
            if let Some(overload) = shape.overload
                && !id.ends_with(&format!("#{overload}"))
            {
                continue;
            }
            let required = unit.params.iter().filter(|p| !p.has_default).count();
            let supplied = shape.positional_count + shape.named_arguments.len();
            if supplied >= required
                && supplied <= unit.params.len()
                && shape.named_arguments.iter().all(|name| {
                    unit.params
                        .iter()
                        .enumerate()
                        .any(|(pos, p)| p.name == *name && pos >= shape.positional_count)
                })
            {
                ids.push(id.clone());
            }
        }
        if ids.is_empty()
            && shape.overload.is_none()
            && let Some((package, name)) = shape.routine_id.split_once('.')
            && self.packages.contains_key(package)
            && !self
                .units
                .keys()
                .any(|id| id.split('#').next() == Some(shape.routine_id.as_str()))
            && self.units.contains_key(name)
        {
            // A package member may call a standalone routine in its schema
            // when no member of that name exists in the package.
            ids.push(name.to_string());
        }
        ids
    }
}

fn inspect_statements(
    statements: &[Statement],
    unit: &EffectUnit,
    index: &InvocationIndexV1,
    depth: usize,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    if depth >= plsql_ir::MAX_RELOWER_DEPTH {
        out.unknown("statement recursion limit");
        return;
    }
    for stmt in statements {
        match stmt {
            Statement::Null | Statement::Raise { .. } => {}
            Statement::Assignment { target, rhs_text } => {
                let target_name = match lower_expression(target) {
                    Expr::Name(name) if name.parts.len() == 1 => Some(name.parts[0].clone()),
                    Expr::Name(name)
                        if name.parts.len() >= 2
                            && unit.package.as_deref() == Some(name.parts[0].as_str()) =>
                    {
                        Some(name.parts[1].clone())
                    }
                    Expr::Name(name) if name.parts.len() >= 2 => {
                        // A qualified write may target another package's
                        // session state. We cannot prove its owner here.
                        out.unknown(format!("qualified assignment target: {}", name.display));
                        None
                    }
                    _ => {
                        out.unknown(format!("unclassified assignment target: {target}"));
                        None
                    }
                };
                if let Some(package) = &unit.package
                    && index.packages.get(package).is_some_and(|p| {
                        target_name
                            .as_ref()
                            .is_some_and(|name| p.state_variables.contains(name))
                    })
                {
                    out.effects.effects.insert(RoutineEffect::SessionState);
                }
                if matches!(unit.kind, Some(PackageUnitKind::InitSection)) {
                    out.effects.effects.insert(RoutineEffect::SessionState);
                }
                inspect_expression(rhs_text, unit, out, calls);
            }
            Statement::Return { value_text } => {
                if let Some(text) = value_text {
                    inspect_expression(text, unit, out, calls);
                }
            }
            Statement::Exit { when_text } => {
                if let Some(text) = when_text {
                    inspect_expression(text, unit, out, calls);
                }
            }
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    inspect_expression(&arm.cond_text, unit, out, calls);
                    inspect_statements(
                        &plsql_ir::lower_statement_body(&arm.body_text),
                        unit,
                        index,
                        depth + 1,
                        out,
                        calls,
                    );
                }
                if let Some(body) = else_body_text {
                    inspect_statements(
                        &plsql_ir::lower_statement_body(body),
                        unit,
                        index,
                        depth + 1,
                        out,
                        calls,
                    );
                }
            }
            Statement::BareLoop { body_text } => {
                inspect_statements(
                    &plsql_ir::lower_statement_body(body_text),
                    unit,
                    index,
                    depth + 1,
                    out,
                    calls,
                );
            }
            Statement::ForLoop {
                range_text,
                body_text,
                ..
            } => {
                inspect_expression(range_text, unit, out, calls);
                inspect_statements(
                    &plsql_ir::lower_statement_body(body_text),
                    unit,
                    index,
                    depth + 1,
                    out,
                    calls,
                );
            }
            Statement::WhileLoop {
                cond_text,
                body_text,
            } => {
                inspect_expression(cond_text, unit, out, calls);
                inspect_statements(
                    &plsql_ir::lower_statement_body(body_text),
                    unit,
                    index,
                    depth + 1,
                    out,
                    calls,
                );
            }
            Statement::NestedBlock { body_text } => {
                let inner = body_text.trim();
                let stripped = inner
                    .strip_prefix("BEGIN")
                    .or_else(|| inner.strip_prefix("begin"));
                if let Some(inner) = stripped {
                    let inner = inner.trim_end_matches(';').trim_end();
                    let inner = inner
                        .strip_suffix("END")
                        .or_else(|| inner.strip_suffix("end"));
                    if let Some(inner) = inner {
                        inspect_statements(
                            &plsql_ir::lower_statement_body(inner),
                            unit,
                            index,
                            depth + 1,
                            out,
                            calls,
                        );
                    } else {
                        out.unknown("unrecognized nested block");
                    }
                } else {
                    out.unknown("unrecognized nested block");
                }
            }
            Statement::Sql { verb, raw_text } => {
                match verb {
                    SqlVerb::Select => {
                        out.effects.effects.insert(RoutineEffect::ReadDb);
                    }
                    SqlVerb::Insert | SqlVerb::Update | SqlVerb::Delete | SqlVerb::Merge => {
                        out.effects.effects.insert(RoutineEffect::Dml);
                    }
                }
                inspect_sql_text(raw_text, out);
                inspect_sql_calls(raw_text, unit, out, calls);
            }
            Statement::TransactionControl { .. } => {
                out.effects.effects.insert(RoutineEffect::TxnControl);
            }
            Statement::ExecuteImmediate {
                sql_literal,
                has_bind_variables,
            } => {
                if *has_bind_variables {
                    out.unknown("EXECUTE IMMEDIATE USING/INTO expressions not modeled");
                }
                match lower_expression(sql_literal) {
                    Expr::StringLit(text) => inspect_literal_sql(&text, unit, out, calls),
                    _ => {
                        out.effects.effects.insert(RoutineEffect::DynamicSql);
                        out.unknown("data-dependent EXECUTE IMMEDIATE");
                    }
                }
            }
            Statement::Unrecognized { raw_text, .. } => {
                let upper = raw_text.trim().to_ascii_uppercase();
                if upper.starts_with("PRAGMA AUTONOMOUS_TRANSACTION") {
                    out.effects.effects.insert(RoutineEffect::Autonomous);
                } else if upper.starts_with("LOCK TABLE") {
                    out.effects.effects.insert(RoutineEffect::RowLock);
                } else if upper.starts_with("SET TRANSACTION") {
                    out.effects.effects.insert(RoutineEffect::TxnControl);
                } else if upper.starts_with("ALTER SESSION") {
                    out.effects.effects.insert(RoutineEffect::SessionState);
                } else if upper.starts_with("EXECUTE IMMEDIATE") {
                    out.effects.effects.insert(RoutineEffect::DynamicSql);
                    out.unknown("unclassified EXECUTE IMMEDIATE");
                } else if upper.is_empty() {
                    out.unknown("statement source unavailable");
                } else {
                    let expr = lower_expression(raw_text);
                    if matches!(expr, Expr::Call { .. }) {
                        inspect_expr(&expr, unit, out, calls);
                    } else {
                        out.unknown(format!("unclassified statement: {raw_text}"));
                    }
                }
            }
        }
    }
}

fn inspect_expression(
    text: &str,
    unit: &EffectUnit,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    inspect_expr(&lower_expression(text), unit, out, calls);
}

fn inspect_expr(
    expr: &Expr,
    unit: &EffectUnit,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    match expr {
        Expr::Null
        | Expr::BoolLit(_)
        | Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::DateTimeLit { .. } => {}
        Expr::BindRef(_) | Expr::SubstitutionRef { .. } => {
            out.unknown("runtime expression input");
        }
        Expr::Name(name) => {
            if name.parts.last().is_some_and(|p| p == "NEXTVAL") {
                out.effects.effects.insert(RoutineEffect::SequenceAdvance);
            }
        }
        Expr::Call { callee, args } => {
            let parts = &callee.parts;
            if parts
                .iter()
                .any(|part| part.contains(['.', '#', '(', ')', '@']))
            {
                out.unknown(format!(
                    "callee identity needs structured components: {}",
                    callee.display
                ));
                for arg in args {
                    inspect_expr(arg, unit, out, calls);
                }
                return;
            }
            let name = parts.join(".");
            let last = parts.last().map(String::as_str).unwrap_or("");
            if matches!(last, "NEXTVAL") {
                out.effects.effects.insert(RoutineEffect::SequenceAdvance);
            } else if name.starts_with("DBMS_SQL") {
                out.effects.effects.insert(RoutineEffect::DynamicSql);
                out.unknown(format!("opaque external call: {name}"));
            } else if name.starts_with("UTL_")
                || name.starts_with("DBMS_SCHEDULER")
                || name.starts_with("DBMS_PIPE")
                || name.starts_with("DBMS_AQ")
                || name.starts_with("DBMS_SESSION")
            {
                out.effects.effects.insert(RoutineEffect::ExternalIo);
                if name.starts_with("DBMS_SESSION") {
                    out.effects.effects.insert(RoutineEffect::SessionState);
                }
                out.unknown(format!("opaque external call: {name}"));
            } else {
                // Built-in names are deliberately not treated as pure here:
                // lexical lookup may resolve to a user routine, and SYS
                // effects require the adapter's reviewed allowlist.
                let mut named_arguments = BTreeSet::new();
                let mut positional_count = 0;
                let mut argument_types = Vec::new();
                for arg in args {
                    if let Expr::Raw { text, .. } = arg
                        && let Some((name, _)) = text.split_once("=>")
                    {
                        named_arguments.insert(name.trim().to_ascii_uppercase());
                    } else {
                        positional_count += 1;
                        argument_types.push(match arg {
                            Expr::IntLit(_) | Expr::FloatLit(_) => InvocationArgType::Number,
                            Expr::StringLit(_) => InvocationArgType::Text,
                            Expr::Null => InvocationArgType::Null,
                            _ => InvocationArgType::Bind,
                        });
                    }
                }
                let routine_id = if parts.len() == 1 {
                    match &unit.package {
                        Some(pkg) => format!("{pkg}.{name}"),
                        None => name,
                    }
                } else {
                    name
                };
                calls.push(InvocationShapeV1 {
                    routine_id,
                    positional_count,
                    named_arguments,
                    argument_types,
                    overload: None,
                });
            }
            for arg in args {
                if let Expr::Raw { text, .. } = arg
                    && let Some((_, value)) = text.split_once("=>")
                {
                    inspect_expression(value, unit, out, calls);
                } else {
                    inspect_expr(arg, unit, out, calls);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            inspect_expr(lhs, unit, out, calls);
            inspect_expr(rhs, unit, out, calls);
        }
        Expr::Unary { operand, .. } => inspect_expr(operand, unit, out, calls),
        Expr::Raw { text, .. } if text.trim() == "*" => {}
        Expr::Raw { text, .. } => out.unknown(format!("unclassified expression: {text}")),
    }
}

fn inspect_sql_text(text: &str, out: &mut InvocationClosureV1) {
    let upper = text.to_ascii_uppercase();
    let words = upper.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.contains(" FOR UPDATE") || words.starts_with("LOCK TABLE") {
        out.effects.effects.insert(RoutineEffect::RowLock);
    }
    if upper.contains("NEXTVAL") {
        out.effects.effects.insert(RoutineEffect::SequenceAdvance);
    }
    if upper.contains("DBMS_SQL.") || upper.contains("UTL_") {
        out.unknown("opaque call in SQL expression");
    }
}

fn inspect_sql_calls(
    text: &str,
    unit: &EffectUnit,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    if text.contains('"')
        || text.contains("--")
        || text.contains("/*")
        || text.to_ascii_uppercase().contains("Q'")
    {
        out.unknown("SQL quoting or comments need structured call lowering");
        return;
    }
    let bytes = text.as_bytes();
    let sql_lead = text.trim_start().to_ascii_uppercase();
    let mut pos = 0;
    let mut in_string = false;
    let mut previous_word = String::new();
    while pos < bytes.len() {
        if bytes[pos] == b'\'' {
            if in_string && bytes.get(pos + 1) == Some(&b'\'') {
                pos += 2;
                continue;
            }
            in_string = !in_string;
            pos += 1;
            continue;
        }
        if in_string || !(bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_') {
            pos += 1;
            continue;
        }
        let start = pos;
        pos += 1;
        while pos < bytes.len()
            && (bytes[pos].is_ascii_alphanumeric()
                || matches!(bytes[pos], b'_' | b'$' | b'#' | b'.'))
        {
            pos += 1;
        }
        let name = &text[start..pos];
        let preceded_by_dml_target = matches!(previous_word.as_str(), "INTO" | "UPDATE")
            && (sql_lead.starts_with("INSERT")
                || sql_lead.starts_with("UPDATE")
                || sql_lead.starts_with("MERGE"));
        previous_word = name.to_ascii_uppercase();
        let mut open = pos;
        while bytes.get(open).is_some_and(u8::is_ascii_whitespace) {
            open += 1;
        }
        if bytes.get(open) != Some(&b'(')
            || preceded_by_dml_target
            || matches!(
                name.to_ascii_uppercase().as_str(),
                "VALUES" | "IN" | "ON" | "USING" | "TABLE"
            )
        {
            continue;
        }
        let mut depth = 0usize;
        let mut close = open;
        let mut quoted = false;
        while close < bytes.len() {
            match bytes[close] {
                b'\'' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        close += 1;
                        break;
                    }
                }
                _ => {}
            }
            close += 1;
        }
        if depth != 0 {
            out.unknown(format!("unclosed SQL call: {name}"));
            return;
        }
        inspect_expression(&text[start..close], unit, out, calls);
        pos = close;
    }
}

fn inspect_literal_sql(
    text: &str,
    unit: &EffectUnit,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    let upper = text.trim().to_ascii_uppercase();
    if upper.starts_with("ALTER DATABASE DEFAULT EDITION") {
        out.effects.effects.insert(RoutineEffect::OperatorOnly(
            OperatorStatementClass::DefaultEditionFlip,
        ));
    } else if upper.starts_with("ALTER SESSION") {
        out.effects.effects.insert(RoutineEffect::SessionState);
    } else if upper.starts_with("LOCK TABLE") {
        out.effects.effects.insert(RoutineEffect::RowLock);
    } else if upper.starts_with("ALTER SYSTEM")
        || upper.starts_with("ALTER DATABASE")
        || [
            "CREATE USER",
            "ALTER USER",
            "DROP USER",
            "CREATE ROLE",
            "DROP ROLE",
            "GRANT ",
            "REVOKE ",
        ]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
    {
        if parsed_literal_ddl(text) {
            out.effects.effects.insert(RoutineEffect::Admin);
        } else {
            out.effects.effects.insert(RoutineEffect::DynamicSql);
            out.unknown("literal DDL could not be parsed");
        }
    } else if [
        "CREATE ",
        "ALTER ",
        "DROP ",
        "TRUNCATE ",
        "GRANT ",
        "REVOKE ",
    ]
    .iter()
    .any(|prefix| upper.starts_with(prefix))
    {
        if parsed_literal_ddl(text) {
            out.effects.effects.insert(RoutineEffect::Ddl);
        } else {
            out.effects.effects.insert(RoutineEffect::DynamicSql);
            out.unknown("literal DDL could not be parsed");
        }
    } else if ["INSERT ", "UPDATE ", "DELETE ", "MERGE "]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
    {
        out.effects.effects.insert(RoutineEffect::Dml);
        inspect_sql_text(text, out);
        inspect_sql_calls(text, unit, out, calls);
    } else if upper.starts_with("SELECT ") {
        out.effects.effects.insert(RoutineEffect::ReadDb);
        inspect_sql_text(text, out);
        inspect_sql_calls(text, unit, out, calls);
    } else if ["COMMIT", "ROLLBACK", "SAVEPOINT", "SET TRANSACTION"]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
    {
        out.effects.effects.insert(RoutineEffect::TxnControl);
    } else {
        out.effects.effects.insert(RoutineEffect::DynamicSql);
        out.unknown(format!("unclassified literal SQL: {text}"));
    }
}

fn parsed_literal_ddl(text: &str) -> bool {
    use plsql_parser::ast::AstDecl;

    let source = format!("{};", text.trim().trim_end_matches(';'));
    let parsed = plsql_parser::parse_with_backend(
        &source,
        plsql_core::FileId::new(0),
        &plsql_parser_antlr::Antlr4RustBackend::new(),
        &plsql_parser::ParseOptions::default(),
    );
    !parsed.recovered
        && !parsed
            .diagnostics
            .iter()
            .any(|d| d.severity >= plsql_core::Severity::Error)
        && matches!(
            parsed.ast.root.declarations.as_slice(),
            [AstDecl::Ddl { .. }]
        )
}
