//! AST → IR lowering for top-level declarations.
//!
//! Walks `plsql_parser::Ast::root.declarations` and produces a
//! [`LoweredFile`] containing one [`Declaration`] per recognized
//! `AstDecl` variant plus a `Vec<Diagnostic>` for unclassified rows
//! (R13 — typed uncertainty, never silent drops).
//!
//! Pipeline:
//!
//! 1. Iterate `ast.root.declarations`.
//! 2. For each variant emit a [`Declaration`] with the source name
//!    interned into the supplied [`SymbolInterner`].
//! 3. `AstDecl::Unknown` becomes a typed `parser-recovery-region`
//!    diagnostic so the engine's `CompletenessReport` reflects the
//!    unclassified region instead of dropping it.
//! 4. `AstDecl::Ddl` is currently informational — the rule engine that
//!    classifies `CREATE / ALTER / DROP / GRANT` lives in the catalog +
//!    ChangeSet layers; here we record the verb in a diagnostic and let
//!    callers decide.

use plsql_core::{Diagnostic, Evidence, Severity, SymbolInterner};
use plsql_parser::Ast;
use plsql_parser::ast::AstDecl;
use tracing::instrument;

/// The evidence code + attribute key the USR-loop capture
/// (`plsql_accretion::gap::antlr_rule_path_of`
/// §2.1`) reads to recover the ANTLR grammar position a repairable
/// diagnostic arose at. Keeping the contract in one place means the
/// producer (here) and the consumer (capture) cannot drift.
const ANTLR_RULE_PATH_EVIDENCE_CODE: &str = "ANTLR_RULE_PATH";
const ANTLR_RULE_PATH_ATTR_KEY: &str = "antlr_rule_path";

/// Stamp the ANTLR `rule_path` (a `>`-joined path of *grammar rule
/// names* — never source text/identifiers, see
/// `plsql_parser::ast::AstDecl::Ddl`) onto `diag` as a structured
/// [`Evidence`] attribute, exactly where the USR-loop capture reads
/// it. A no-op when the declaration carried no rule path (text
/// scanner fallback), so signatures stay honest: a `None` here is a
/// real "no parse-tree position", not a fabricated one.
fn stamp_antlr_rule_path(diag: Diagnostic, rule_path: Option<&str>) -> Diagnostic {
    match rule_path {
        Some(path) if !path.is_empty() => diag.with_evidence(
            Evidence::new(
                ANTLR_RULE_PATH_EVIDENCE_CODE,
                "ANTLR grammar rule position of the unlowered declaration",
            )
            .with_attribute(
                ANTLR_RULE_PATH_ATTR_KEY,
                serde_json::Value::String(path.to_string()),
            ),
        ),
        _ => diag,
    }
}

use crate::decl::{
    DeclCommon, Declaration, FunctionDecl, OverloadIdentity, PackageCursor, PackageDecl,
    PackageInitSection, PackageInitializer, PackageMember, PackagePart, PackageUnits, ParamMode,
    ProcedureDecl, RoutineKind, RoutineParam, TriggerDecl, TypeDecl, TypeRef,
    UnattributedConstruct, ViewDecl,
};
use crate::expr::lower_expression;
use plsql_parser::ast::{AstOverload, AstPackageUnits, AstParamMode, AstRoutineKind};

/// Bundle of declarations + diagnostics produced by [`lower_top_level`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoweredFile {
    /// One [`Declaration`] per recognized `AstDecl` variant, in source
    /// order.
    pub declarations: Vec<Declaration>,
    /// Typed diagnostics for unclassified / informational rows. The
    /// engine merges these into the per-run `Diagnostic` stream.
    pub diagnostics: Vec<Diagnostic>,
}

impl LoweredFile {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty() && self.diagnostics.is_empty()
    }
}

/// Lower every top-level `AstDecl` in `ast` to an IR `Declaration`.
///
/// `interner` is mutated so the produced `Declaration::name`s are
/// re-resolvable through the same symbol table the rest of the engine
/// uses.
#[must_use]
#[instrument(level = "trace", skip(ast, interner))]
pub fn lower_top_level(ast: &Ast, interner: &mut SymbolInterner) -> LoweredFile {
    let mut out = LoweredFile::default();
    for decl in &ast.root.declarations {
        match decl {
            AstDecl::PackageSpec { name, span, units } => {
                let common = make_common(name, *span, interner);
                let units = lower_package_units(units, PackagePart::Spec);
                package_unit_diagnostics(name, *span, &units, &mut out.diagnostics);
                out.declarations.push(Declaration::Package(PackageDecl {
                    common,
                    members: Vec::new(),
                    body: None,
                    units,
                }));
            }
            AstDecl::PackageBody { name, span, units } => {
                // The body's `common` carries the same name as the spec;
                // the spec/body pairing (`members`/`body` DeclIds) is wired
                // up in the symbol pass (PLSQL-SYM-001). The body's own
                // executable content lives in `units`.
                let common = make_common(name, *span, interner);
                let units = lower_package_units(units, PackagePart::Body);
                package_unit_diagnostics(name, *span, &units, &mut out.diagnostics);
                out.declarations.push(Declaration::Package(PackageDecl {
                    common,
                    members: Vec::new(),
                    body: None,
                    units,
                }));
            }
            AstDecl::Procedure { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations.push(Declaration::Procedure(ProcedureDecl {
                    common,
                    params: Vec::new(),
                }));
            }
            AstDecl::Function { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations.push(Declaration::Function(FunctionDecl {
                    common,
                    params: Vec::new(),
                    return_type: None,
                }));
            }
            AstDecl::Trigger { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations
                    .push(Declaration::Trigger(TriggerDecl { common }));
            }
            AstDecl::View { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations.push(Declaration::View(ViewDecl {
                    common,
                    columns: Vec::new(),
                }));
            }
            AstDecl::TypeSpec { name, span } | AstDecl::TypeBody { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations
                    .push(Declaration::Type(TypeDecl { common }));
            }
            AstDecl::Ddl {
                kind,
                span,
                antlr_rule_path,
            } => {
                // CREATE / ALTER / DROP / GRANT lives in the changeset
                // path; record it informationally so the file-level
                // CompletenessReport reflects it.
                let mut diagnostic = Diagnostic::new(
                    "IR_DDL_NOT_LOWERED",
                    Severity::Info,
                    format!("DDL `{kind}` recorded but not lowered (handled by ChangeSet path)"),
                );
                diagnostic.primary_span = Some(*span);
                // USR-loop §2.1: stamp the ANTLR grammar position so
                // gap signatures are fine-grained (rule names only —
                // I-PRIVACY: never source text/identifiers).
                let diagnostic = stamp_antlr_rule_path(diagnostic, antlr_rule_path.as_deref());
                out.diagnostics.push(diagnostic);
            }
            AstDecl::Unknown {
                span,
                antlr_rule_path,
            } => {
                // R13: emit a typed parser-recovery diagnostic so the
                // engine's CompletenessReport sees the unclassified
                // region instead of dropping it.
                let mut diagnostic = Diagnostic::new(
                    "IR_UNCLASSIFIED_DECL",
                    Severity::Warn,
                    "AST classifier returned `Unknown` — declaration not lowered",
                );
                diagnostic.primary_span = Some(*span);
                diagnostic
                    .unknown_reasons
                    .push(plsql_core::UnknownReason::ParserRecoveryRegion);
                let diagnostic = stamp_antlr_rule_path(diagnostic, antlr_rule_path.as_deref());
                out.diagnostics.push(diagnostic);
            }
        }
    }
    out
}

/// Lower one package part's parse-tree units to IR units. A part the
/// parser did not lower (`lowered == false`) stays unlowered, so it can
/// never read as a complete package with no members.
fn lower_package_units(units: &AstPackageUnits, part: PackagePart) -> PackageUnits {
    PackageUnits {
        part,
        lowered: units.lowered,
        members: units
            .members
            .iter()
            .map(|m| PackageMember {
                name: m.name.clone(),
                kind: match m.kind {
                    AstRoutineKind::Procedure => RoutineKind::Procedure,
                    AstRoutineKind::Function => RoutineKind::Function,
                },
                overload: match m.overload {
                    AstOverload::NotOverloaded => OverloadIdentity::NotOverloaded,
                    AstOverload::Ordinal(n) => OverloadIdentity::Ordinal(n),
                    AstOverload::Ambiguous { position } => OverloadIdentity::Ambiguous { position },
                },
                params: m
                    .params
                    .iter()
                    .map(|p| RoutineParam {
                        name: p.name.clone(),
                        mode: match p.mode {
                            AstParamMode::In => ParamMode::In,
                            AstParamMode::Out => ParamMode::Out,
                            AstParamMode::InOut => ParamMode::InOut,
                        },
                        ty: (!p.type_text.is_empty())
                            .then(|| TypeRef::Unresolved(p.type_text.clone())),
                        default: p.default_text.as_deref().map(lower_expression),
                        default_text: p.default_text.clone(),
                        span: p.span,
                    })
                    .collect(),
                body: lower_ast_statements(&m.statements),
                span: m.span,
            })
            .collect(),
        initializers: units
            .initializers
            .iter()
            .map(|i| PackageInitializer {
                name: i.name.clone(),
                initializer: lower_expression(&i.initializer_text),
                initializer_text: i.initializer_text.clone(),
                span: i.span,
            })
            .collect(),
        cursors: units
            .cursors
            .iter()
            .map(|c| PackageCursor {
                name: c.name.clone(),
                query: if c.query_text.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![crate::Statement::Sql {
                        verb: crate::SqlVerb::Select,
                        raw_text: c.query_text.clone(),
                    }]
                },
                span: c.span,
            })
            .collect(),
        init_section: units.init_section.as_ref().map(|s| PackageInitSection {
            body: lower_ast_statements(&s.statements),
            span: s.span,
        }),
        unattributed: units
            .unattributed
            .iter()
            .map(|u| UnattributedConstruct {
                span: u.span,
                reason: u.reason.clone(),
            })
            .collect(),
    }
}

/// Typed diagnostics for everything that keeps a package part from being
/// complete (R13): an unlowered part, each unattributed construct, and each
/// overload whose `ALL_ARGUMENTS.OVERLOAD` identity is ambiguous. Each one
/// lowers completeness; an invocation closure over the package is `Unknown`.
fn package_unit_diagnostics(
    package: &str,
    span: plsql_core::Span,
    units: &PackageUnits,
    out: &mut Vec<Diagnostic>,
) {
    if !units.lowered {
        let mut d = Diagnostic::new(
            "IR_PACKAGE_NOT_LOWERED",
            Severity::Warn,
            format!("package `{package}` was not lowered into units; its closure is Unknown"),
        );
        d.primary_span = Some(span);
        d.unknown_reasons
            .push(plsql_core::UnknownReason::ParserRecoveryRegion);
        out.push(d);
    }
    for u in &units.unattributed {
        let reason = match u.reason.as_str() {
            "conditional_compilation" => plsql_core::UnknownReason::ConditionalCompilationBranch,
            "call_spec" => plsql_core::UnknownReason::UnsupportedDialectFeature,
            _ => plsql_core::UnknownReason::ParserRecoveryRegion,
        };
        let mut d = Diagnostic::new(
            "IR_PACKAGE_UNATTRIBUTED",
            Severity::Warn,
            format!(
                "package `{package}`: a `{}` construct could not be attributed to a member, \
                 initializer, cursor or the init section; the package closure is Unknown",
                u.reason
            ),
        );
        d.primary_span = Some(u.span);
        d.unknown_reasons.push(reason);
        out.push(d);
    }
    for m in &units.members {
        if let OverloadIdentity::Ambiguous { .. } = m.overload {
            let mut d = Diagnostic::new(
                "IR_PACKAGE_OVERLOAD_AMBIGUOUS",
                Severity::Warn,
                format!(
                    "package `{package}`: the OVERLOAD ordinal of `{}` cannot be established \
                     from source; its identity is Unknown until the catalog pins it",
                    m.name
                ),
            );
            d.primary_span = Some(m.span);
            d.unknown_reasons
                .push(plsql_core::UnknownReason::MissingCatalogObject);
            out.push(d);
        }
    }
}

fn make_common(name: &str, span: plsql_core::Span, interner: &mut SymbolInterner) -> DeclCommon {
    let interned = interner.intern(name).unwrap_or_else(|| {
        // SymbolInterner only fails on u64-overflow; in practice
        // intern() returns None when the interner has run out of slots.
        // We surface a 0 marker symbol so the caller can flag it
        // upstream via the diagnostic shoot at lower_top_level's end.
        plsql_core::SymbolId::new(0)
    });
    DeclCommon::new(interned, span)
}

/// Recover a typed [`crate::SqlVerb`] when the ANTLR lowerer tagged a SQL
/// statement with the generic `"SQL"` sentinel (anything that is not one of the
/// five DML verbs). Returns the matching verb ONLY if the raw statement text
/// genuinely *leads* with that DML keyword on a word boundary — so a true DML
/// statement the grammar failed to classify still yields correct table edges,
/// while a non-DML construct (`EXPLAIN PLAN FOR …`, `LOCK TABLE …`, `OPEN c FOR
/// …`, `COMMIT`, …) returns `None` and is routed to `Statement::Unrecognized`
/// by the caller. The word boundary is essential: a `DELETED_FLAG := …` LHS
/// must never be read as a `DELETE` verb. (oracle-j1ep.4)
#[must_use]
pub fn leading_dml_verb(raw_text: &str) -> Option<crate::SqlVerb> {
    use crate::SqlVerb;
    let trimmed = raw_text.trim_start();
    for (kw, verb) in [
        ("SELECT", SqlVerb::Select),
        ("INSERT", SqlVerb::Insert),
        ("UPDATE", SqlVerb::Update),
        ("DELETE", SqlVerb::Delete),
        ("MERGE", SqlVerb::Merge),
    ] {
        // Case-insensitive prefix match. `get(..kw.len())` is char-boundary
        // safe: `kw` is ASCII, so byte index `kw.len()` is a valid boundary
        // whenever it is `<= trimmed.len()`; a multibyte leading char simply
        // fails the ASCII prefix compare and falls through.
        if let Some(head) = trimmed.get(..kw.len()) {
            if head.eq_ignore_ascii_case(kw) {
                // Require a word boundary after the keyword: the next char (if
                // any) must NOT continue an identifier, else `UPDATES_LOG` /
                // `DELETED_FLAG` would masquerade as a verb.
                let boundary = match trimmed[kw.len()..].chars().next() {
                    None => true,
                    Some(c) => !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#')),
                };
                if boundary {
                    return Some(verb);
                }
            }
        }
    }
    None
}

/// Lower parse-tree statements ([`plsql_parser::ast::AstStatement`]) to
/// IR [`crate::Statement`]s.
#[must_use]
pub fn lower_ast_statements(
    ast_stmts: &[plsql_parser::ast::AstStatement],
) -> Vec<crate::Statement> {
    use crate::{SqlVerb, Statement, UnknownStatementReason};
    use plsql_parser::ast::AstStatement;

    ast_stmts
        .iter()
        .flat_map(|s| match s {
            AstStatement::Null { .. } => vec![Statement::Null],
            AstStatement::Assignment {
                target, rhs_text, ..
            } => vec![Statement::Assignment {
                target: target.clone(),
                rhs_text: rhs_text.clone(),
            }],
            AstStatement::Return { value_text, .. } => vec![Statement::Return {
                value_text: value_text.clone(),
            }],
            AstStatement::Raise { exception, .. } => vec![Statement::Raise {
                exception: exception.clone(),
            }],
            AstStatement::ExecuteImmediate {
                sql_text,
                has_using,
                ..
            } => vec![Statement::ExecuteImmediate {
                sql_literal: sql_text.clone(),
                has_bind_variables: *has_using,
            }],
            AstStatement::Sql { verb, raw_text, .. } => {
                // Map ONLY the five DML verbs to a typed `SqlVerb`. The ANTLR
                // lowerer (`tree_lower.rs`) tags every other SQL construct with
                // the sentinel verb `"SQL"` — cursor manipulation (OPEN/CLOSE/
                // FETCH), transaction control (COMMIT/ROLLBACK/LOCK TABLE), and
                // any `data_manipulation_language_statements` the grammar could
                // not classify (e.g. `EXPLAIN PLAN FOR SELECT … FROM t`).
                //
                // Coercing that sentinel to `SqlVerb::Select` (the old fallback)
                // both DROPPED the R13 typed-uncertainty signal AND minted
                // spurious `Reads` edges: `accesses_from_sql` scans a Select's
                // raw text for a whole-word `FROM`/`JOIN`, so an EXPLAIN PLAN
                // body invented a bogus `Reads t` (EXPLAIN writes PLAN_TABLE and
                // does not read `t`). We now route unknown verbs to
                // `Statement::Unrecognized`, matching the sibling text-scanner
                // (`crate::stmt::classify`) and preserving typed uncertainty.
                //
                // To keep the one beneficial case — an unclassified statement
                // whose text genuinely *leads* with a DML verb still yields
                // correct table edges — we recover the verb by word-boundary
                // matching the leading keyword of the raw text before falling
                // back to Unrecognized. (oracle-j1ep.4)
                let sql_verb = match verb.to_ascii_uppercase().as_str() {
                    "SELECT" => Some(SqlVerb::Select),
                    "INSERT" => Some(SqlVerb::Insert),
                    "UPDATE" => Some(SqlVerb::Update),
                    "DELETE" => Some(SqlVerb::Delete),
                    "MERGE" => Some(SqlVerb::Merge),
                    _ => leading_dml_verb(raw_text),
                };
                match sql_verb {
                    Some(verb) => vec![Statement::Sql {
                        verb,
                        raw_text: raw_text.clone(),
                    }],
                    None => vec![Statement::Unrecognized {
                        raw_text: raw_text.clone(),
                        unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
                    }],
                }
            }
            AstStatement::Call { callee, .. } => {
                // A call statement: emit as Unrecognized with raw_text of the
                // form "callee()" so that `lower_expression` recognises it as
                // an `Expr::Call` and `extract_call_sites` can resolve the
                // callee. If the callee already contains `(`, trust it as-is.
                let raw = if callee.contains('(') {
                    callee.clone()
                } else {
                    format!("{callee}()")
                };
                vec![Statement::Unrecognized {
                    raw_text: raw,
                    unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
                }]
            }
            // IF / LOOP: `cond_text` / `header_text` carry the *full*
            // `IF … END IF;` / `LOOP … END LOOP;` source slice
            // (the whole parse-tree node span, body included). Re-lower
            // it through the IR statement-body parser so the nested
            // DML becomes recursive `Statement::If`/`ForLoop`/… that
            // `extract_table_accesses` (PLSQL-DEP-003) walks — without
            // this the body's SELECT/INSERT/UPDATE/DELETE is invisible.
            AstStatement::If { cond_text, .. } => crate::lower_statement_body(cond_text),
            AstStatement::Loop { header_text, .. } => crate::lower_statement_body(header_text),
            AstStatement::Unknown { .. } => vec![Statement::Unrecognized {
                raw_text: String::new(),
                unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
            }],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an `AstStatement::Sql` carrying the ANTLR lowerer's generic
    /// `"SQL"` sentinel verb (the value emitted for any SQL construct that is
    /// not one of the five DML verbs — cursor manipulation, transaction
    /// control, or DML the grammar cannot classify such as `EXPLAIN PLAN`).
    fn sql_sentinel(raw_text: &str) -> plsql_parser::ast::AstStatement {
        use plsql_core::{FileId, Position, Span};
        let pos = Position::new(1, 1, 0);
        plsql_parser::ast::AstStatement::Sql {
            verb: "SQL".to_string(),
            raw_text: raw_text.to_string(),
            span: Span::new(FileId::new(0), pos, pos),
        }
    }

    /// oracle-j1ep.4 regression: the ANTLR `"SQL"` sentinel verb (e.g. an
    /// `EXPLAIN PLAN FOR SELECT … FROM t` body) must NOT be coerced to
    /// `SqlVerb::Select`. Coercion both dropped the R13 typed-uncertainty
    /// signal and minted a spurious `Reads t` edge (EXPLAIN writes PLAN_TABLE
    /// and does not read `t`). It must now lower to `Statement::Unrecognized`
    /// and yield zero table accesses — matching the sibling text-scanner.
    #[test]
    fn unknown_sql_verb_lowers_to_unrecognized_not_spurious_read() {
        use crate::{Statement, UnknownStatementReason, extract_table_accesses};

        for raw in [
            "EXPLAIN PLAN FOR SELECT col FROM t",
            "LOCK TABLE t IN EXCLUSIVE MODE",
            "OPEN c FOR SELECT col FROM t",
            "COMMIT",
            "ROLLBACK TO sp",
            "FETCH c INTO v",
            "CLOSE c",
        ] {
            let ir = lower_ast_statements(&[sql_sentinel(raw)]);
            assert_eq!(ir.len(), 1, "exactly one IR statement for `{raw}`");
            assert!(
                matches!(
                    ir[0],
                    Statement::Unrecognized {
                        unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
                        ..
                    }
                ),
                "`{raw}` must be Unrecognized, not coerced to a SqlVerb; got {:?}",
                ir[0]
            );
            assert!(
                extract_table_accesses(&ir).is_empty(),
                "`{raw}` must mint zero table accesses (no spurious Read), got {:?}",
                extract_table_accesses(&ir)
            );
        }
    }

    /// The beneficial case must survive: a SQL statement the grammar tagged
    /// generically but whose text genuinely *leads* with a DML verb still
    /// recovers the correct typed verb and table edges.
    #[test]
    fn unknown_sql_verb_with_leading_dml_keyword_recovers_verb_and_reads() {
        use crate::{AccessKind, SqlVerb, Statement, extract_table_accesses};

        let ir = lower_ast_statements(&[sql_sentinel("SELECT col FROM t")]);
        assert!(
            matches!(
                ir[0],
                Statement::Sql {
                    verb: SqlVerb::Select,
                    ..
                }
            ),
            "leading SELECT recovers SqlVerb::Select, got {:?}",
            ir[0]
        );
        let accesses = extract_table_accesses(&ir);
        assert_eq!(accesses.len(), 1, "one Read on T, got {accesses:?}");
        assert_eq!(accesses[0].table, "T");
        assert_eq!(accesses[0].access, AccessKind::Read);
    }

    /// Word-boundary discipline: a verb prefix on an identifier must NOT match.
    /// `DELETED_FLAG := …` and `UPDATES_LOG` are not DELETE / UPDATE verbs.
    #[test]
    fn leading_dml_verb_is_word_boundaried() {
        use crate::SqlVerb;
        assert_eq!(
            leading_dml_verb("SELECT 1 FROM dual"),
            Some(SqlVerb::Select)
        );
        assert_eq!(leading_dml_verb("  delete from t"), Some(SqlVerb::Delete));
        assert_eq!(
            leading_dml_verb("MERGE INTO t USING s ON (..)"),
            Some(SqlVerb::Merge)
        );
        // Identifier continuations must not be read as verbs.
        assert_eq!(leading_dml_verb("DELETED_FLAG := 1"), None);
        assert_eq!(leading_dml_verb("UPDATES_LOG.write(x)"), None);
        assert_eq!(leading_dml_verb("SELECTED := TRUE"), None);
        // Non-DML constructs.
        assert_eq!(leading_dml_verb("EXPLAIN PLAN FOR SELECT 1 FROM t"), None);
        assert_eq!(leading_dml_verb("COMMIT"), None);
        assert_eq!(leading_dml_verb(""), None);
        // Multibyte leading char must not panic and must not match.
        assert_eq!(leading_dml_verb("é := 1"), None);
    }
    use plsql_core::{FileId, Position};
    use plsql_parser::ast::SourceFile;

    fn span(offset: u32, len: u32) -> plsql_core::Span {
        plsql_core::Span::new(
            FileId::new(0),
            Position::new(1, 1, offset),
            Position::new(1, 1, offset + len),
        )
    }

    fn lowered_units() -> AstPackageUnits {
        AstPackageUnits {
            lowered: true,
            ..AstPackageUnits::default()
        }
    }

    fn ast_with(decls: Vec<AstDecl>) -> Ast {
        Ast {
            root: SourceFile {
                span: span(0, 0),
                declarations: decls,
            },
            source_map: plsql_parser::ast::SourceMap::new(),
            body_statements: Vec::new(),
        }
    }

    #[test]
    fn empty_ast_yields_empty_lowered_file() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(&ast_with(vec![]), &mut interner);
        assert!(out.is_empty());
    }

    #[test]
    fn package_spec_lowers_to_package_decl() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![AstDecl::PackageSpec {
                name: String::from("BILLING_API"),
                span: span(0, 12),
                units: lowered_units(),
            }]),
            &mut interner,
        );
        assert_eq!(out.declarations.len(), 1);
        assert!(matches!(out.declarations[0], Declaration::Package(_)));
        // The interner now resolves the name.
        let symbol = out.declarations[0].common().name;
        assert_eq!(interner.resolve(symbol), Some("BILLING_API"));
    }

    #[test]
    fn body_pairs_with_spec_in_source_order() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![
                AstDecl::PackageSpec {
                    name: String::from("BILLING_API"),
                    span: span(0, 12),
                    units: lowered_units(),
                },
                AstDecl::PackageBody {
                    name: String::from("BILLING_API"),
                    span: span(13, 12),
                    units: lowered_units(),
                },
            ]),
            &mut interner,
        );
        assert_eq!(out.declarations.len(), 2);
        assert!(out.diagnostics.is_empty());
    }

    /// A package the parser did not lower into units (the text-scanner
    /// fallback) must surface as incomplete — never as a complete package
    /// with no members.
    #[test]
    fn unlowered_package_is_incomplete_and_diagnosed() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![AstDecl::PackageBody {
                name: String::from("BILLING_API"),
                span: span(0, 12),
                units: AstPackageUnits::default(),
            }]),
            &mut interner,
        );
        let Declaration::Package(pkg) = &out.declarations[0] else {
            panic!("expected a package declaration");
        };
        assert!(!pkg.units.is_complete());
        assert!(out.diagnostics.iter().any(|d| {
            d.code == "IR_PACKAGE_NOT_LOWERED"
                && d.unknown_reasons
                    .contains(&plsql_core::UnknownReason::ParserRecoveryRegion)
        }));
    }

    #[test]
    fn procedure_function_trigger_view_each_lower() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![
                AstDecl::Procedure {
                    name: String::from("RESET_BALANCE"),
                    span: span(0, 8),
                },
                AstDecl::Function {
                    name: String::from("CURRENT_BALANCE"),
                    span: span(10, 8),
                },
                AstDecl::Trigger {
                    name: String::from("INVOICES_BIU"),
                    span: span(20, 8),
                },
                AstDecl::View {
                    name: String::from("V_BALANCE"),
                    span: span(30, 8),
                },
                AstDecl::TypeSpec {
                    name: String::from("ADDRESS_T"),
                    span: span(40, 8),
                },
                AstDecl::TypeBody {
                    name: String::from("ADDRESS_T"),
                    span: span(50, 8),
                },
            ]),
            &mut interner,
        );
        assert_eq!(out.declarations.len(), 6);
        // Verify variant types so a future refactor can't silently swap.
        assert!(matches!(out.declarations[0], Declaration::Procedure(_)));
        assert!(matches!(out.declarations[1], Declaration::Function(_)));
        assert!(matches!(out.declarations[2], Declaration::Trigger(_)));
        assert!(matches!(out.declarations[3], Declaration::View(_)));
        assert!(matches!(out.declarations[4], Declaration::Type(_)));
        assert!(matches!(out.declarations[5], Declaration::Type(_)));
    }

    #[test]
    fn ddl_emits_informational_diagnostic_no_declaration() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![AstDecl::Ddl {
                kind: String::from("CREATE TABLE"),
                span: span(0, 12),
                antlr_rule_path: None,
            }]),
            &mut interner,
        );
        assert!(out.declarations.is_empty());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code, "IR_DDL_NOT_LOWERED");
        assert_eq!(out.diagnostics[0].severity, Severity::Info);
        // No rule path supplied → no fabricated evidence (honest None).
        assert!(
            out.diagnostics[0]
                .evidence
                .iter()
                .all(|e| e.code != "ANTLR_RULE_PATH")
        );
    }

    #[test]
    fn ddl_rule_path_is_stamped_as_capture_evidence() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![AstDecl::Ddl {
                kind: String::from("CREATE SEQUENCE"),
                span: span(0, 15),
                antlr_rule_path: Some(String::from("unit_statement>create_sequence")),
            }]),
            &mut interner,
        );
        let diag = &out.diagnostics[0];
        assert_eq!(diag.code, "IR_DDL_NOT_LOWERED");
        // The capture contract: an `ANTLR_RULE_PATH` evidence whose
        // `antlr_rule_path` attribute is the verbatim grammar path.
        let ev = diag
            .evidence
            .iter()
            .find(|e| e.code == "ANTLR_RULE_PATH")
            .expect("rule-path evidence must be stamped");
        assert_eq!(
            ev.attributes
                .get("antlr_rule_path")
                .and_then(|v| v.as_str()),
            Some("unit_statement>create_sequence")
        );
    }

    #[test]
    fn unknown_decl_emits_typed_warning_with_unknown_reason() {
        let mut interner = SymbolInterner::new();
        let out = lower_top_level(
            &ast_with(vec![AstDecl::Unknown {
                span: span(0, 4),
                antlr_rule_path: None,
            }]),
            &mut interner,
        );
        assert!(out.declarations.is_empty());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code, "IR_UNCLASSIFIED_DECL");
        assert_eq!(out.diagnostics[0].severity, Severity::Warn);
        assert!(
            out.diagnostics[0]
                .unknown_reasons
                .contains(&plsql_core::UnknownReason::ParserRecoveryRegion)
        );
    }

    #[test]
    fn span_propagates_into_declcommon() {
        let mut interner = SymbolInterner::new();
        let in_span = span(42, 8);
        let out = lower_top_level(
            &ast_with(vec![AstDecl::Procedure {
                name: String::from("FOO"),
                span: in_span,
            }]),
            &mut interner,
        );
        assert_eq!(out.declarations[0].common().span, in_span);
    }
}
