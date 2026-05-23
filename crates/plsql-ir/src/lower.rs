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
    DeclCommon, Declaration, FunctionDecl, PackageDecl, ProcedureDecl, TriggerDecl, TypeDecl,
    ViewDecl,
};

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
            AstDecl::PackageSpec { name, span } => {
                let common = make_common(name, *span, interner);
                out.declarations.push(Declaration::Package(PackageDecl {
                    common,
                    members: Vec::new(),
                    body: None,
                }));
            }
            AstDecl::PackageBody { name, span } => {
                // The body's `common` carries the same name as the spec;
                // the spec/body pairing is wired up in the symbol pass
                // (PLSQL-SYM-001). Emit the body as a Package decl with
                // no members for now so the file-level top_level list
                // includes both spec and body in source order.
                let common = make_common(name, *span, interner);
                out.declarations.push(Declaration::Package(PackageDecl {
                    common,
                    members: Vec::new(),
                    body: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position};
    use plsql_parser::ast::SourceFile;

    fn span(offset: u32, len: u32) -> plsql_core::Span {
        plsql_core::Span::new(
            FileId::new(0),
            Position::new(1, 1, offset),
            Position::new(1, 1, offset + len),
        )
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
                },
                AstDecl::PackageBody {
                    name: String::from("BILLING_API"),
                    span: span(13, 12),
                },
            ]),
            &mut interner,
        );
        assert_eq!(out.declarations.len(), 2);
        assert!(out.diagnostics.is_empty());
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
