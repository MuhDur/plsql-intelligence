//! Parse/symbol foundation tools:
//! `parse_file`, `get_symbol`, `compile_check`, `inspect_profile`.
//!
//! All four are pure, read-only, no-live-DB foundation-static
//! tools layered on the already-tested parser/IR/symbol surfaces.
//! Each takes source text (or nothing) and returns a small serde
//! summary — no project tree, no engine run.

use plsql_core::{AnalysisProfile, FileId, Severity, SymbolInterner};
use plsql_ir::lower_top_level;
use plsql_parser_antlr::lower::lower_source;
use plsql_symbols::DeclTable;
use serde::{Deserialize, Serialize};

use crate::{ToolDescriptor, ToolRegistry, ToolTier};

// --- parse_file -------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParseFileRequest {
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParseFileResponse {
    /// Top-level declarations the lowerer recognised.
    pub declaration_count: usize,
    /// `lower_source` is the text-scanning fallback lowerer with
    /// no error-recovery concept — reported honestly, not guessed.
    pub recovered: bool,
}

#[must_use]
pub fn run_parse_file(req: &ParseFileRequest) -> ParseFileResponse {
    let ast = lower_source(&req.source, FileId::new(1));
    ParseFileResponse {
        declaration_count: ast.root.declarations.len(),
        recovered: false,
    }
}

// --- get_symbol -------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetSymbolRequest {
    pub source: String,
    /// Bare (unqualified) declaration name to look up.
    pub symbol: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetSymbolResponse {
    /// `None` ⇒ no declaration with that name — a definite "not
    /// found", structurally distinct from an error (R13).
    pub found: Option<SymbolHit>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolHit {
    pub name: String,
    /// `DeclKind` debug name (Package / Procedure / Variable / …).
    pub kind: String,
    /// True iff the declaration has a parent (nested under a
    /// routine/package); false ⇒ schema-level.
    pub nested: bool,
}

#[must_use]
pub fn run_get_symbol(req: &GetSymbolRequest) -> GetSymbolResponse {
    let ast = lower_source(&req.source, FileId::new(1));
    let mut interner = SymbolInterner::new();
    let lowered = lower_top_level(&ast, &mut interner);
    let mut table = DeclTable::new();
    table.register_all(lowered.declarations);

    // Oracle unquoted identifiers are case-insensitive; the
    // lowerer interns the raw token, so probe the query as-is
    // plus its upper/lower foldings and union the candidates.
    let mut candidates: Vec<plsql_core::SymbolId> = Vec::new();
    for variant in [
        req.symbol.clone(),
        req.symbol.to_ascii_uppercase(),
        req.symbol.to_ascii_lowercase(),
    ] {
        if let Some(s) = interner.intern(variant.as_str()) {
            if !candidates.contains(&s) {
                candidates.push(s);
            }
        }
    }
    let hit = candidates
        .into_iter()
        .flat_map(|sym| table.by_name(sym))
        .find_map(|id| {
            table.get(id).map(|d| SymbolHit {
                name: req.symbol.clone(),
                kind: format!("{:?}", d.kind()),
                nested: d.common().parent.is_some(),
            })
        });
    GetSymbolResponse { found: hit }
}

// --- compile_check ----------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileCheckRequest {
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileCheckResponse {
    pub error_count: usize,
    pub warning_count: usize,
    /// Every diagnostic message, in lowering order (nothing
    /// summarised away — R13).
    pub diagnostics: Vec<String>,
    /// `true` iff zero error-severity diagnostics.
    pub clean: bool,
}

#[must_use]
pub fn run_compile_check(req: &CompileCheckRequest) -> CompileCheckResponse {
    let ast = lower_source(&req.source, FileId::new(1));
    let mut interner = SymbolInterner::new();
    let lowered = lower_top_level(&ast, &mut interner);
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut diagnostics = Vec::with_capacity(lowered.diagnostics.len());
    for d in &lowered.diagnostics {
        if d.severity >= Severity::Error {
            error_count += 1;
        } else if d.severity == Severity::Warn {
            warning_count += 1;
        }
        diagnostics.push(d.message.clone());
    }
    CompileCheckResponse {
        error_count,
        warning_count,
        diagnostics,
        clean: error_count == 0,
    }
}

// --- inspect_profile --------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InspectProfileResponse {
    pub oracle_version: String,
    pub compatibility: Option<String>,
    pub feature_policy: String,
}

/// Report the default [`AnalysisProfile`] the engine uses when a
/// request does not override it. Pure, infallible.
#[must_use]
pub fn run_inspect_profile() -> InspectProfileResponse {
    let p = AnalysisProfile::default();
    InspectProfileResponse {
        oracle_version: format!("{:?}", p.oracle_version),
        compatibility: p.compatibility.map(|c| format!("{c:?}")),
        feature_policy: format!("{:?}", p.feature_policy),
    }
}

/// The advertised argument JSON-Schema for a parse tool (oracle-da9j.1), so an
/// agent can construct a valid call first-try instead of probing -32602s.
fn parse_tool_schema(name: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    let source = json!({"type": "string", "description": "PL/SQL source text to lower."});
    match name {
        "parse_file" | "compile_check" => Some(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["source"],
            "properties": { "source": source },
        })),
        "get_symbol" => Some(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["source", "symbol"],
            "properties": {
                "source": source,
                "symbol": {"type": "string", "description": "Bare (unqualified) declaration name to look up."},
            },
        })),
        // inspect_profile takes no arguments.
        "inspect_profile" => {
            Some(json!({"type": "object", "additionalProperties": false, "properties": {}}))
        }
        _ => None,
    }
}

/// Register the four descriptors. Foundation-static tier.
pub fn register_parse_tools(registry: &mut ToolRegistry) {
    for (name, summary) in [
        (
            "parse_file",
            "Lower a PL/SQL source string and report the recognised top-level declaration \
             count + recovery status. No project, no DB.",
        ),
        (
            "get_symbol",
            "Lower a source string and look up a bare declaration name; returns kind + \
             nested/schema-level, or found:null (a definite not-found, not an error).",
        ),
        (
            "compile_check",
            "Lower a source string and return error/warning counts + every diagnostic \
             message verbatim; clean=true iff zero error-severity diagnostics.",
        ),
        (
            "inspect_profile",
            "Report the default AnalysisProfile (oracle_version, compatibility floor, \
             feature_policy) the engine applies when a request does not override it.",
        ),
    ] {
        let mut d = ToolDescriptor::new(name, ToolTier::FoundationStatic, summary);
        if let Some(schema) = parse_tool_schema(name) {
            d = d.with_input_schema(schema);
        }
        registry.register(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKG: &str = "CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n";

    #[test]
    fn parse_file_counts_declarations() {
        let r = run_parse_file(&ParseFileRequest {
            source: PKG.to_string(),
        });
        assert!(r.declaration_count >= 1);
        assert!(!r.recovered, "text-scan lowerer has no recovery concept");
    }

    #[test]
    fn parse_file_empty_source_is_zero_not_a_crash() {
        let r = run_parse_file(&ParseFileRequest {
            source: String::new(),
        });
        assert_eq!(r.declaration_count, 0);
    }

    #[test]
    fn get_symbol_finds_a_declared_name() {
        let r = run_get_symbol(&GetSymbolRequest {
            source: PKG.to_string(),
            symbol: "P".to_string(),
        });
        let hit = r.found.expect("package P is declared");
        assert_eq!(hit.name, "P");
        assert!(!hit.kind.is_empty());
    }

    #[test]
    fn get_symbol_absent_is_found_none_not_error() {
        let r = run_get_symbol(&GetSymbolRequest {
            source: PKG.to_string(),
            symbol: "NO_SUCH_THING".to_string(),
        });
        assert!(r.found.is_none(), "absent symbol => found:None (R13)");
    }

    #[test]
    fn compile_check_clean_source_has_no_errors() {
        let r = run_compile_check(&CompileCheckRequest {
            source: "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n".to_string(),
        });
        assert_eq!(r.error_count, 0);
        assert!(r.clean);
        // error/warning counts can never exceed the total
        // diagnostics surfaced (nothing is summarised away).
        assert!(r.error_count + r.warning_count <= r.diagnostics.len());
    }

    #[test]
    fn inspect_profile_reports_default() {
        let r = run_inspect_profile();
        assert!(!r.oracle_version.is_empty());
        assert!(!r.feature_policy.is_empty());
    }

    #[test]
    fn responses_round_trip_through_json() {
        let p = run_parse_file(&ParseFileRequest {
            source: PKG.to_string(),
        });
        let j = serde_json::to_string(&p).unwrap();
        let back: ParseFileResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back, p);

        let prof = run_inspect_profile();
        let pj = serde_json::to_string(&prof).unwrap();
        let pb: InspectProfileResponse = serde_json::from_str(&pj).unwrap();
        assert_eq!(pb, prof);
    }

    #[test]
    fn registers_four_foundation_static_tools() {
        let mut reg = ToolRegistry::new();
        register_parse_tools(&mut reg);
        register_parse_tools(&mut reg);
        assert_eq!(reg.len(), 4);
        assert!(
            reg.tools
                .iter()
                .all(|t| t.tier == ToolTier::FoundationStatic)
        );
        let names: Vec<&str> = reg.tools.iter().map(|t| t.name.as_str()).collect();
        for n in [
            "parse_file",
            "get_symbol",
            "compile_check",
            "inspect_profile",
        ] {
            assert!(names.contains(&n), "missing {n}");
        }
    }
}
