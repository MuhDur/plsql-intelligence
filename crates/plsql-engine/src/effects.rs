//! Source-backed routine effects and invocation closures.
//!
//! A missing call target or an unclassified construct is evidence of
//! uncertainty, never evidence of purity. Catalog validity and SYS allowlist
//! decisions belong to the downstream adapter.

use std::collections::{BTreeMap, BTreeSet};

use plsql_ir::{Expr, PackageUnitKind, Statement, expr::lower_expression, stmt::SqlVerb};
use plsql_parser_antlr::read_shape::{
    ShapeBranch, ShapeExpr, ShapeIdent, ShapePartition, ShapeQuery, ShapeSelectItem, ShapeSource,
    ShapeTableRef, StatementValueShape, statement_value_shape,
};
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
    /// Source expression for a standalone default. Package defaults have their
    /// own addressable units instead.
    #[serde(default)]
    pub default_text: Option<String>,
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
    /// Values proven visible before this unit starts; locals are introduced
    /// by ordered IR binding markers, not inferred from assignments.
    #[serde(default)]
    pub visible_values: BTreeSet<String>,
    #[serde(default)]
    pub visible_value_types: BTreeMap<String, String>,
    #[serde(default)]
    pub package_part: Option<plsql_ir::PackagePart>,
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
    #[serde(default)]
    pub spec_variables: BTreeSet<String>,
    #[serde(default)]
    pub spec_value_types: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
enum LocalValueShape {
    #[default]
    Opaque,
    Record(Vec<String>),
    Collection,
}

#[derive(Clone)]
struct ValueScope<'a> {
    unit: &'a EffectUnit,
    values: BTreeSet<String>,
    value_types: BTreeMap<String, LocalValueShape>,
    local_types: BTreeMap<String, LocalValueShape>,
    sql_calls: bool,
}

impl std::ops::Deref for ValueScope<'_> {
    type Target = EffectUnit;
    fn deref(&self) -> &Self::Target {
        self.unit
    }
}

impl<'a> ValueScope<'a> {
    fn new(unit: &'a EffectUnit, index: &InvocationIndexV1) -> Self {
        let mut values = unit.visible_values.clone();
        values.extend(unit.params.iter().map(|p| p.name.clone()));
        if unit.package_part == Some(plsql_ir::PackagePart::Body)
            && let Some(package) = unit.package.as_ref().and_then(|p| index.packages.get(p))
        {
            values.extend(package.spec_variables.iter().cloned());
        }
        let mut scope = Self {
            unit,
            values,
            value_types: BTreeMap::new(),
            local_types: BTreeMap::new(),
            sql_calls: false,
        };
        for (name, text) in &unit.visible_value_types {
            let shape = scope.resolve_type(text);
            scope.value_types.insert(name.clone(), shape);
        }
        if unit.package_part == Some(plsql_ir::PackagePart::Body)
            && let Some(package) = unit.package.as_ref().and_then(|p| index.packages.get(p))
        {
            for (name, text) in &package.spec_value_types {
                let shape = scope.resolve_type(text);
                scope.value_types.insert(name.clone(), shape);
            }
        }
        for param in &unit.params {
            let shape = scope.resolve_type(param.type_text.as_deref().unwrap_or(""));
            scope.value_types.insert(param.name.clone(), shape);
        }
        scope
    }

    fn resolve_type(&self, text: &str) -> LocalValueShape {
        let text = text.trim();
        let upper = text.to_ascii_uppercase();
        if upper.ends_with("%ROWTYPE") {
            return LocalValueShape::Record(Vec::new());
        }
        if upper.ends_with("%TYPE") {
            return match lower_expression(text[..text.len() - 5].trim()) {
                Expr::Name(name) if name.parts.len() == 1 => self
                    .value_types
                    .get(&name.parts[0])
                    .cloned()
                    .unwrap_or_default(),
                _ => LocalValueShape::Opaque,
            };
        }
        match lower_expression(text) {
            Expr::Name(name) if name.parts.len() == 1 => self
                .local_types
                .get(&name.parts[0])
                .cloned()
                .unwrap_or_default(),
            _ => LocalValueShape::Opaque,
        }
    }

    fn value_call(&self, parts: &[String], arity: usize) -> bool {
        if self.sql_calls {
            return false;
        }
        match parts {
            [name] => {
                matches!(
                    self.local_types.get(name),
                    Some(LocalValueShape::Collection)
                ) || (arity == 1
                    && matches!(
                        self.value_types.get(name),
                        Some(LocalValueShape::Collection)
                    ))
            }
            [name, member]
                if matches!(
                    self.value_types.get(name),
                    Some(LocalValueShape::Collection)
                ) =>
            {
                collection_method(member, arity)
            }
            _ => false,
        }
    }
}

fn collection_method(name: &str, arity: usize) -> bool {
    matches!(
        (name, arity),
        ("COUNT" | "FIRST" | "LAST" | "LIMIT", 0) | ("NEXT" | "PRIOR" | "EXISTS", 1)
    )
}

fn known_value(
    parts: &[String],
    values: &BTreeSet<String>,
    types: &BTreeMap<String, LocalValueShape>,
) -> bool {
    if values.contains(&parts.join(".")) {
        return true;
    }
    match parts {
        [name, _] if matches!(types.get(name), Some(LocalValueShape::Record(_))) => true,
        [name, member] if matches!(types.get(name), Some(LocalValueShape::Collection)) => {
            collection_method(member, 0)
        }
        _ => false,
    }
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
            if unit.package_part == Some(plsql_ir::PackagePart::Body)
                || existing.package_part.is_none()
            {
                existing.visible_values = unit.visible_values;
                existing.visible_value_types = unit.visible_value_types;
                existing.package_part = unit.package_part;
            }
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
        let scope = ValueScope::new(unit, self);
        inspect_statements(&unit.statements, &scope, self, 0, &mut result, &mut calls);
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
        let mut calls = Vec::new();
        let scope = ValueScope::new(unit, self);
        for (position, param) in unit.params.iter().enumerate() {
            let supplied =
                position < shape.positional_count || shape.named_arguments.contains(&param.name);
            if !supplied && param.has_default && unit.kind.is_none() {
                match &param.default_text {
                    Some(text) => inspect_default_expression(text, &scope, out, &mut calls),
                    None => {
                        out.unknown(format!("default source unavailable: {id}({})", param.name))
                    }
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
        inspect_statements(&unit.statements, &scope, self, 0, out, &mut calls);
        for call in calls {
            self.visit(&call, out, visiting);
        }
    }

    fn resolve(&self, shape: &InvocationShapeV1) -> Vec<String> {
        if self.units.contains_key(&shape.routine_id) {
            let unit = &self.units[&shape.routine_id];
            if unit.kind.is_none() || matches!(unit.kind, Some(PackageUnitKind::Member { .. })) {
                let supplied = shape.positional_count + shape.named_arguments.len();
                let required = unit.params.iter().filter(|p| !p.has_default).count();
                if supplied < required
                    || supplied > unit.params.len()
                    || !shape.named_arguments.iter().all(|name| {
                        unit.params
                            .iter()
                            .enumerate()
                            .any(|(pos, p)| p.name == *name && pos >= shape.positional_count)
                    })
                    || !unit.params.iter().enumerate().all(|(pos, p)| {
                        p.has_default
                            || pos < shape.positional_count
                            || shape.named_arguments.contains(&p.name)
                    })
                {
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
    scope: &ValueScope<'_>,
    index: &InvocationIndexV1,
    depth: usize,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    let mut owned_scope = (*scope).clone();
    let unit = &mut owned_scope;
    let mut scopes = Vec::new();
    if depth >= plsql_ir::MAX_RELOWER_DEPTH {
        out.unknown("statement recursion limit");
        return;
    }
    if unit
        .package
        .as_ref()
        .and_then(|package| index.packages.get(package))
        .is_some_and(|package| package.state_variables.contains("SYS"))
    {
        out.unknown("package state may shadow SYS qualifier");
    }
    for stmt in statements {
        match stmt {
            Statement::ValueBinding { name, type_text } => {
                let shape = unit.resolve_type(type_text);
                if let LocalValueShape::Record(defaults) = &shape {
                    for expr in defaults {
                        inspect_expression(expr, unit, out, calls);
                    }
                }
                unit.values.insert(name.clone());
                unit.value_types.insert(name.clone(), shape);
                unit.local_types.remove(name);
            }
            Statement::LocalTypeBinding {
                name,
                collection,
                expressions,
            } => {
                let shape = if *collection {
                    for expr in expressions {
                        inspect_expression(expr, unit, out, calls);
                    }
                    LocalValueShape::Collection
                } else {
                    LocalValueShape::Record(expressions.clone())
                };
                unit.local_types.insert(name.clone(), shape);
                unit.values.remove(name);
                unit.value_types.remove(name);
            }
            Statement::ValueScopeStart {
                parameters,
                default_expressions,
            } => {
                if scopes.len() >= plsql_ir::MAX_RELOWER_DEPTH {
                    out.unknown("lexical value scope recursion limit");
                    return;
                }
                // Nested subprogram bodies are already conservatively part of
                // this unit. Their possibly evaluated defaults must be too.
                for expression in default_expressions {
                    inspect_default_expression(expression, unit, out, calls);
                }
                scopes.push((
                    unit.values.clone(),
                    unit.value_types.clone(),
                    unit.local_types.clone(),
                ));
                for (name, ty) in parameters {
                    let shape = unit.resolve_type(ty);
                    unit.values.insert(name.clone());
                    unit.value_types.insert(name.clone(), shape);
                    unit.local_types.remove(name);
                }
            }
            Statement::ValueScopeEnd => {
                if let Some((values, value_types, local_types)) = scopes.pop() {
                    unit.values = values;
                    unit.value_types = value_types;
                    unit.local_types = local_types;
                } else {
                    out.unknown("unbalanced lexical value scope");
                }
            }
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
                iterator,
                range_text,
                body_text,
                ..
            } => {
                inspect_expression(range_text, unit, out, calls);
                let mut body_scope = (*unit).clone();
                body_scope.values.insert(iterator.clone());
                body_scope
                    .value_types
                    .insert(iterator.clone(), LocalValueShape::Opaque);
                inspect_statements(
                    &plsql_ir::lower_statement_body(body_text),
                    &body_scope,
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
            Statement::OpenCursor { target, query_text } => {
                // Opening a package-global cursor mutates session state just
                // like assigning the variable. Reuse the target analysis.
                inspect_statements(
                    &[Statement::Assignment {
                        target: target.clone(),
                        rhs_text: "NULL".into(),
                    }],
                    unit,
                    index,
                    depth + 1,
                    out,
                    calls,
                );
                out.effects.effects.insert(RoutineEffect::ReadDb);
                inspect_sql_text(query_text, out);
                inspect_sql_calls(query_text, unit, out, calls);
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
    if !scopes.is_empty() {
        out.unknown("unclosed lexical value scope");
    }
}

/// Dynamic-SQL sites in one unit's statements, for the run's
/// `CompletenessReport`. A site is an `EXECUTE IMMEDIATE`, an
/// `OPEN cursor FOR <string>` or a `DBMS_SQL.PARSE` call; it is opaque when
/// its SQL text is not a compile-time string literal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DynamicSqlCensus {
    pub(crate) sites: usize,
    pub(crate) opaque: usize,
    /// A nested body was abandoned at the re-lowering depth cap, so the
    /// counts are a lower bound, not a measurement.
    pub(crate) truncated: bool,
}

impl DynamicSqlCensus {
    fn site(&mut self, literal: bool) {
        self.sites += 1;
        if !literal {
            self.opaque += 1;
        }
    }
}

pub(crate) fn dynamic_sql_census(statements: &[Statement]) -> DynamicSqlCensus {
    let mut census = DynamicSqlCensus::default();
    census_statements(statements, 0, &mut census);
    census
}

fn census_statements(statements: &[Statement], depth: usize, census: &mut DynamicSqlCensus) {
    if depth >= plsql_ir::MAX_RELOWER_DEPTH {
        census.truncated = true;
        return;
    }
    let body = |text: &str, census: &mut DynamicSqlCensus| {
        census_statements(&plsql_ir::lower_statement_body(text), depth + 1, census);
    };
    for stmt in statements {
        match stmt {
            Statement::Assignment { rhs_text: text, .. }
            | Statement::Return {
                value_text: Some(text),
            }
            | Statement::Exit {
                when_text: Some(text),
            } => census_expr(&lower_expression(text), census),
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    census_expr(&lower_expression(&arm.cond_text), census);
                    body(&arm.body_text, census);
                }
                if let Some(text) = else_body_text {
                    body(text, census);
                }
            }
            Statement::WhileLoop {
                cond_text: text,
                body_text,
            }
            | Statement::ForLoop {
                range_text: text,
                body_text,
                ..
            } => {
                census_expr(&lower_expression(text), census);
                body(body_text, census);
            }
            Statement::BareLoop { body_text } => body(body_text, census),
            Statement::NestedBlock { body_text } => {
                let inner = body_text.trim();
                let inner = inner
                    .strip_prefix("BEGIN")
                    .or_else(|| inner.strip_prefix("begin"))
                    .map(|inner| inner.trim_end_matches(';').trim_end())
                    .and_then(|inner| {
                        inner
                            .strip_suffix("END")
                            .or_else(|| inner.strip_suffix("end"))
                    });
                match inner {
                    Some(inner) => body(inner, census),
                    // `inspect_statements` reports this block as unknown;
                    // the census cannot see into it either.
                    None => census.truncated = true,
                }
            }
            Statement::ExecuteImmediate { sql_literal, .. } => {
                census.site(matches!(lower_expression(sql_literal), Expr::StringLit(_)))
            }
            Statement::Unrecognized { raw_text, .. } => census_unrecognized(raw_text, census),
            Statement::ValueBinding { .. }
            | Statement::LocalTypeBinding { .. }
            | Statement::ValueScopeStart { .. }
            | Statement::ValueScopeEnd
            | Statement::Null
            | Statement::Raise { .. }
            | Statement::Return { value_text: None }
            | Statement::Exit { when_text: None }
            | Statement::Sql { .. }
            | Statement::OpenCursor { .. }
            | Statement::TransactionControl { .. } => {}
        }
    }
}

fn census_unrecognized(raw_text: &str, census: &mut DynamicSqlCensus) {
    let text = raw_text.trim();
    let upper = text.to_ascii_uppercase();
    if upper.starts_with("EXECUTE IMMEDIATE") {
        // The lowerer could not split out the SQL operand.
        census.site(false);
    } else if let Some(rest) = upper.strip_prefix("OPEN ")
        && let Some(at) = rest.find(" FOR ")
    {
        let start = text.len() - rest.len() + at + " FOR ".len();
        let query = text[start..].trim_start();
        let query_upper = query.to_ascii_uppercase();
        if !(query_upper.starts_with("SELECT")
            || query_upper.starts_with("WITH")
            || query_upper.starts_with('('))
        {
            let operand = match query_upper.find(" USING ") {
                Some(using) => &query[..using],
                None => query,
            };
            census.site(matches!(lower_expression(operand), Expr::StringLit(_)));
        }
    } else {
        census_expr(&lower_expression(text), census);
    }
}

fn census_expr(expr: &Expr, census: &mut DynamicSqlCensus) {
    match expr {
        Expr::Call { callee, args } => {
            if callee.parts.len() == 2
                && callee.parts[0] == "DBMS_SQL"
                && callee.parts[1] == "PARSE"
            {
                // `DBMS_SQL.PARSE(c, statement, language_flag)`, positional
                // or named.
                let named = args.iter().find_map(|arg| match arg {
                    Expr::Raw { text, .. } => text.split_once("=>").and_then(|(name, value)| {
                        name.trim()
                            .eq_ignore_ascii_case("STATEMENT")
                            .then(|| lower_expression(value))
                    }),
                    _ => None,
                });
                let literal = match named {
                    Some(statement) => matches!(statement, Expr::StringLit(_)),
                    None => matches!(args.get(1), Some(Expr::StringLit(_))),
                };
                census.site(literal);
            }
            for arg in args {
                census_expr(arg, census);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            census_expr(lhs, census);
            census_expr(rhs, census);
        }
        Expr::Unary { operand, .. } => census_expr(operand, census),
        _ => {}
    }
}

fn inspect_expression(
    text: &str,
    unit: &ValueScope<'_>,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    if matches!(unit.kind, Some(PackageUnitKind::ParamDefault { .. })) {
        inspect_default_expression(text, unit, out, calls);
        return;
    }
    inspect_expr(&lower_expression(text), unit, out, calls);
}

fn inspect_default_expression(
    text: &str,
    unit: &ValueScope<'_>,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    let expression = lower_expression(text);
    let mut pending = vec![&expression];
    while let Some(expr) = pending.pop() {
        match expr {
            // Oracle can invoke a zero-argument function without parentheses.
            // A source-only name here cannot be assumed to be a value.
            Expr::Name(name) if !name.parts.last().is_some_and(|part| part == "NEXTVAL") => {
                out.unknown("unresolved value or zero-argument call in default")
            }
            Expr::Binary { lhs, rhs, .. } => {
                pending.push(lhs);
                pending.push(rhs);
            }
            Expr::Unary { operand, .. } => pending.push(operand),
            Expr::Call { args, .. } => pending.extend(args),
            _ => {}
        }
    }
    inspect_expr(&expression, unit, out, calls);
}

fn inspect_expr(
    expr: &Expr,
    unit: &ValueScope<'_>,
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
            if known_value(&name.parts, &unit.values, &unit.value_types) {
                // Parser-proven value binding wins over a possible bare call.
            } else if name.parts.last().is_some_and(|p| p == "NEXTVAL") {
                out.effects.effects.insert(RoutineEffect::SequenceAdvance);
            } else {
                out.unknown(format!(
                    "unresolved value or zero-argument call: {}",
                    name.display
                ));
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
            if unit.value_call(parts, args.len()) {
                // Parser-proven local collection constructors, indexing and
                // built-in methods are values. Arguments are still inspected.
            } else if !unit.sql_calls
                && parts.first().is_some_and(|prefix| {
                    unit.values.contains(prefix) || unit.local_types.contains_key(prefix)
                })
            {
                // A bound value/type prefix cannot identify an unrelated schema
                // routine. Object member dispatch needs type resolution.
                out.unknown(format!(
                    "unproven call through a value or local type: {name}"
                ));
            } else if is_dbms_output_primitive(&name, args.len())
                && !args
                    .iter()
                    .any(|arg| matches!(arg, Expr::Raw { text, .. } if text.contains("=>")))
            {
                // Only fully qualified output producers. ENABLE/DISABLE and
                // reads are intentionally not admitted: capture is controlled
                // by the downstream session owner, with a bounded buffer.
                out.effects.effects.insert(RoutineEffect::SessionState);
            } else if matches!(last, "NEXTVAL") {
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
                        None => match unit.id.rsplit_once('.') {
                            Some((owner, _)) => format!("{owner}.{name}"),
                            None => name,
                        },
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

/// Exact supplied-package output producers modeled by this engine. This does
/// not resolve synonyms or lexical shadowing; source completeness and the
/// adapter's dictionary identity proof remain mandatory.
#[must_use]
pub fn is_dbms_output_primitive(name: &str, argument_count: usize) -> bool {
    matches!(
        (name, argument_count),
        ("SYS.DBMS_OUTPUT.PUT" | "SYS.DBMS_OUTPUT.PUT_LINE", 1) | ("SYS.DBMS_OUTPUT.NEW_LINE", 0)
    )
}

fn inspect_sql_text(text: &str, out: &mut InvocationClosureV1) {
    let upper = text.to_ascii_uppercase();
    let mut words = upper.split_whitespace();
    let mut previous = words.next();
    let mut row_lock = matches!(previous, Some("LOCK")) && words.next() == Some("TABLE");
    while !row_lock {
        let Some(word) = words.next() else {
            break;
        };
        row_lock = previous == Some("FOR") && word == "UPDATE";
        previous = Some(word);
    }
    if row_lock {
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
    unit: &ValueScope<'_>,
    out: &mut InvocationClosureV1,
    calls: &mut Vec<InvocationShapeV1>,
) {
    // Query-block bindings come from the existing ANTLR projection, including
    // SELECT INTO, OPEN's SELECT and DML value slots. The flat call walker below
    // still supplies exact call arguments; its accepted columns cannot erase an
    // uncertainty already found by this block-scoped walk.
    let mut values = SqlValueNames {
        bindings: &unit.values,
        value_types: &unit.value_types,
        out,
        accepted: BTreeSet::new(),
        call_names: BTreeSet::new(),
    };
    match statement_value_shape(text) {
        Ok(StatementValueShape::Query(query)) => {
            values.query(&query, &SqlValueScope::default(), &BTreeMap::new(), 0);
        }
        Ok(StatementValueShape::Dml {
            sources,
            values: expressions,
        }) => {
            let mut scope = SqlValueScope::default();
            for source in &sources {
                values.source(source, &mut scope, &BTreeMap::new(), 0);
            }
            for expr in &expressions {
                values.expression(expr, &scope, &BTreeMap::new(), 0);
            }
        }
        Err(reason) => values
            .out
            .unknown(format!("incomplete SQL value shape: {reason:?}")),
    }
    let mut sql_unit = unit.clone();
    sql_unit.sql_calls = true;
    sql_unit.values.extend(values.accepted);
    let call_names = values.call_names;
    let unit = &sql_unit;
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
            || !call_names.contains(&name.to_ascii_uppercase())
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

#[derive(Clone, Default)]
struct SqlColumns {
    names: BTreeSet<String>,
    // P0 oraclemcp-ncm0g supplies catalog column binding. Until then, bare
    // names in blocks with real-table columns remain potential columns.
    unresolved: bool,
}

#[derive(Clone, Default)]
struct SqlValueScope {
    qualifiers: BTreeMap<Vec<String>, SqlColumns>,
    columns: SqlColumns,
}

struct SqlValueNames<'a, 'b> {
    bindings: &'a BTreeSet<String>,
    value_types: &'a BTreeMap<String, LocalValueShape>,
    out: &'b mut InvocationClosureV1,
    accepted: BTreeSet<String>,
    call_names: BTreeSet<String>,
}

fn sql_ident(ident: &ShapeIdent) -> String {
    if ident.quoted {
        ident.text.clone()
    } else {
        ident.text.to_ascii_uppercase()
    }
}

impl SqlValueNames<'_, '_> {
    fn bounded(&mut self, depth: usize) -> bool {
        if depth > plsql_ir::MAX_RELOWER_DEPTH {
            self.out.unknown("SQL value binding depth limit");
            false
        } else {
            true
        }
    }

    fn query(
        &mut self,
        query: &ShapeQuery,
        outer: &SqlValueScope,
        ctes: &BTreeMap<String, SqlColumns>,
        depth: usize,
    ) -> SqlColumns {
        if !self.bounded(depth) {
            return SqlColumns::default();
        }
        let mut ctes = ctes.clone();
        for cte in &query.ctes {
            // A recursive CTE is a declared query source, not an unresolved
            // catalog table. Its declared columns cannot bind another bare F.
            ctes.insert(
                sql_ident(&cte.name),
                SqlColumns {
                    names: cte.columns.iter().map(sql_ident).collect(),
                    unresolved: false,
                },
            );
            let mut columns = self.query(&cte.query, outer, &ctes, depth + 1);
            if !cte.columns.is_empty() {
                columns = SqlColumns {
                    names: cte.columns.iter().map(sql_ident).collect(),
                    unresolved: false,
                };
            }
            ctes.insert(sql_ident(&cte.name), columns);
        }
        let (mut scope, columns) = self.branch(&query.first, outer, &ctes, depth + 1);
        for (_, branch) in &query.rest {
            self.branch(branch, outer, &ctes, depth + 1);
        }
        // SELECT aliases are values only in trailing query clauses; they
        // never bind a sibling select-list expression or a WHERE predicate.
        scope.columns.names.extend(columns.names.iter().cloned());
        for expr in query.order_by.iter().chain(&query.row_limit) {
            self.expression(expr, &scope, &ctes, depth + 1);
        }
        columns
    }

    fn branch(
        &mut self,
        branch: &ShapeBranch,
        outer: &SqlValueScope,
        ctes: &BTreeMap<String, SqlColumns>,
        depth: usize,
    ) -> (SqlValueScope, SqlColumns) {
        if !self.bounded(depth) {
            return (SqlValueScope::default(), SqlColumns::default());
        }
        let select = match branch {
            ShapeBranch::Query(query) => {
                return (outer.clone(), self.query(query, outer, ctes, depth + 1));
            }
            ShapeBranch::Select(select) => select,
        };
        // Qualified correlation names remain visible. Bare names are decided
        // against this block's sources, so a real table outside a DUAL-only
        // subquery does not turn an unresolved inner F into a column.
        let mut scope = SqlValueScope {
            qualifiers: outer.qualifiers.clone(),
            columns: SqlColumns::default(),
        };
        for item in &select.from {
            self.source(&item.source, &mut scope, ctes, depth + 1);
            for join in &item.joins {
                self.source(&join.table, &mut scope, ctes, depth + 1);
            }
        }
        let mut columns = SqlColumns::default();
        for item in &select.items {
            match item {
                ShapeSelectItem::Wildcard => {
                    columns.names.extend(scope.columns.names.iter().cloned());
                    columns.unresolved |= scope.columns.unresolved;
                }
                ShapeSelectItem::QualifiedWildcard(name) => {
                    let qualifier: Vec<_> = name.iter().map(sql_ident).collect();
                    if let Some(source) = scope.qualifiers.get(&qualifier) {
                        columns.names.extend(source.names.iter().cloned());
                        columns.unresolved |= source.unresolved;
                    } else {
                        self.out.unknown("unbound SQL wildcard qualifier");
                    }
                }
                ShapeSelectItem::Expr { expr, alias } => {
                    self.expression(expr, &scope, ctes, depth + 1);
                    if let Some(alias) = alias {
                        columns.names.insert(sql_ident(alias));
                    } else if let ShapeExpr::Name(name) = expr {
                        if let Some(last) = name.last() {
                            columns.names.insert(sql_ident(last));
                        }
                    }
                }
            }
        }
        for item in &select.from {
            for join in &item.joins {
                for expr in join.on.iter().chain(&join.partition_by) {
                    self.expression(expr, &scope, ctes, depth + 1);
                }
            }
        }
        for expr in select
            .where_clause
            .iter()
            .chain(&select.group_by)
            .chain(&select.having)
        {
            self.expression(expr, &scope, ctes, depth + 1);
        }
        if let Some(hierarchy) = &select.hierarchy {
            for expr in hierarchy
                .start_with
                .iter()
                .chain(std::iter::once(&hierarchy.connect_by))
            {
                self.expression(expr, &scope, ctes, depth + 1);
            }
        }
        scope.columns.names.extend(columns.names.iter().cloned());
        for expr in select.order_by.iter().chain(&select.row_limit) {
            self.expression(expr, &scope, ctes, depth + 1);
        }
        (scope, columns)
    }

    fn source(
        &mut self,
        source: &ShapeTableRef,
        scope: &mut SqlValueScope,
        ctes: &BTreeMap<String, SqlColumns>,
        depth: usize,
    ) {
        if !self.bounded(depth) {
            return;
        }
        let columns = match &source.source {
            ShapeSource::Derived(query) => self.query(query, scope, ctes, depth + 1),
            ShapeSource::Table {
                name,
                db_link,
                partition,
            } => {
                if let Some(ShapePartition::For(values)) = partition {
                    for expr in values {
                        self.expression(expr, scope, ctes, depth + 1);
                    }
                }
                let name: Vec<_> = name.iter().map(sql_ident).collect();
                if db_link.is_none()
                    && matches!(
                        name.iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                        ["DUAL"] | ["SYS", "DUAL"]
                    )
                {
                    SqlColumns {
                        names: BTreeSet::from(["DUMMY".into()]),
                        unresolved: false,
                    }
                } else if db_link.is_none() && name.len() == 1 && ctes.contains_key(&name[0]) {
                    ctes[&name[0]].clone()
                } else {
                    SqlColumns {
                        names: BTreeSet::new(),
                        unresolved: true,
                    }
                }
            }
        };
        scope.columns.names.extend(columns.names.iter().cloned());
        scope.columns.unresolved |= columns.unresolved;
        if let Some(alias) = &source.alias {
            scope.qualifiers.insert(vec![sql_ident(alias)], columns);
        } else if let ShapeSource::Table { name, .. } = &source.source {
            let name: Vec<_> = name.iter().map(sql_ident).collect();
            if let Some(last) = name.last() {
                scope.qualifiers.insert(vec![last.clone()], columns.clone());
            }
            scope.qualifiers.insert(name, columns);
        }
        for expr in &source.flashback {
            self.expression(expr, scope, ctes, depth + 1);
        }
    }

    fn expression(
        &mut self,
        expr: &ShapeExpr,
        scope: &SqlValueScope,
        ctes: &BTreeMap<String, SqlColumns>,
        depth: usize,
    ) {
        if !self.bounded(depth) {
            return;
        }
        match expr {
            ShapeExpr::Name(parts) => {
                let parts: Vec<_> = parts.iter().map(sql_ident).collect();
                let name = parts.join(".");
                let column = known_value(&parts, self.bindings, self.value_types)
                    || if parts.len() == 1 {
                        self.bindings.contains(&name)
                            || scope.columns.names.contains(&name)
                            || scope.columns.unresolved
                            || matches!(name.as_str(), "ROWNUM" | "LEVEL")
                    } else {
                        scope.qualifiers.contains_key(&parts[..parts.len() - 1])
                    };
                if parts.last().is_some_and(|p| p == "NEXTVAL") {
                    self.out
                        .effects
                        .effects
                        .insert(RoutineEffect::SequenceAdvance);
                } else if column {
                    self.accepted.insert(name);
                } else {
                    self.out.unknown(format!(
                        "unresolved SQL value or zero-argument call: {name}"
                    ));
                }
            }
            ShapeExpr::Call { name, args, .. } => {
                self.call_names
                    .insert(name.iter().map(sql_ident).collect::<Vec<_>>().join("."));
                for arg in args {
                    self.expression(arg, scope, ctes, depth + 1);
                }
            }
            ShapeExpr::Group(args) => {
                for arg in args {
                    self.expression(arg, scope, ctes, depth + 1);
                }
            }
            ShapeExpr::Subquery { query, .. } => {
                self.query(query, scope, ctes, depth + 1);
            }
            ShapeExpr::Literal | ShapeExpr::Star => {}
        }
    }
}

fn inspect_literal_sql(
    text: &str,
    unit: &ValueScope<'_>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_effect_token_window_preserves_multiline_and_conservative_text_matches() {
        for text in [
            "SELECT n FROM t FOR\nUPDATE",
            "SELECT n FROM t /* FOR UPDATE */",
        ] {
            let mut out = InvocationClosureV1::new("test".to_string());
            inspect_sql_text(text, &mut out);
            assert!(
                out.effects.effects.contains(&RoutineEffect::RowLock),
                "{text}"
            );
        }

        let mut quoted = InvocationClosureV1::new("test".to_string());
        inspect_sql_text("SELECT 'FOR UPDATE' FROM dual", &mut quoted);
        assert!(!quoted.effects.effects.contains(&RoutineEffect::RowLock));
    }
}
