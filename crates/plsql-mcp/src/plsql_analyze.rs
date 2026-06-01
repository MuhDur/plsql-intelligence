//! `plsql_analyze` — the `oracle_plsql_analyze` §8.1 capability (bead P1-PA /
//! oracle-qmwz.2.15). Engine-backed **static** analysis of a PL/SQL project:
//! the object/routine inventory (with arity from the logical id), a call/ref
//! summary, lint findings, and **cyclomatic complexity** — returned as one
//! structured JSON document.
//!
//! Backed by [`plsql_engine::analyze_project`] (offline, no live DB): the
//! routine inventory + call/ref edges come from the dependency graph, lint from
//! the analysis diagnostics, and cyclomatic complexity from a McCabe
//! decision-point count over the project's PL/SQL sources. Full per-parameter
//! signatures (`ALL_ARGUMENTS`) are the live-DB enrichment; this offline tool
//! reports the routine identities + arity the engine recovers from source.
//!
//! Mirrors the per-tool module convention (`analyze_project`, `describe`, …): a
//! serde request/response pair, a pure `run_*` function, and a descriptor
//! registrar.

use std::path::{Path, PathBuf};

use plsql_engine::{AnalysisRequest, analyze_project as engine_analyze};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ToolDescriptor, ToolRegistry, ToolTier};

/// `plsql_analyze` request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlsqlAnalyzeRequest {
    /// Filesystem path to the project root to analyze.
    pub project_root: String,
}

/// One analysed routine/object identity (with arity, e.g. `pkg.proc/2`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineInfo {
    /// The logical object id the engine assigned.
    pub logical_id: String,
    /// The node identity kind (e.g. `Routine`, `Table`, `Package`).
    pub kind: String,
}

/// One call/reference edge between two objects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRef {
    /// The source object's logical id.
    pub from: String,
    /// The target object's logical id.
    pub to: String,
    /// The edge kind (`Calls`, `Reads`, `Writes`, `References`, …).
    pub kind: String,
}

/// One lint / diagnostic finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintFinding {
    /// The diagnostic code.
    pub code: String,
    /// Severity (`Info` / `Warn` / `Error` / `Fatal`).
    pub severity: String,
    /// The human-readable message.
    pub message: String,
}

/// Cyclomatic complexity for one source file (McCabe decision-point count).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplexityInfo {
    /// The source file (relative-ish path as discovered).
    pub file: String,
    /// Cyclomatic complexity = 1 + decision points.
    pub cyclomatic: u32,
}

/// `plsql_analyze` response — signatures (routine inventory + arity), a call/ref
/// summary, lint, and complexity, as structured JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlsqlAnalyzeResponse {
    /// The analysed project root.
    pub project_root: String,
    /// The routine/object inventory (logical id + kind).
    pub routines: Vec<RoutineInfo>,
    /// The call/reference summary.
    pub call_refs: Vec<CallRef>,
    /// Lint findings (analysis diagnostics).
    pub lint: Vec<LintFinding>,
    /// Per-file cyclomatic complexity.
    pub complexity: Vec<ComplexityInfo>,
}

/// Why `plsql_analyze` failed.
#[derive(Debug, Error)]
pub enum PlsqlAnalyzeError {
    /// The engine analysis pipeline failed.
    #[error("engine analysis failed: {0}")]
    Engine(String),
}

/// PL/SQL source file extensions the complexity walker considers.
const PLSQL_EXTS: &[&str] = &["sql", "pks", "pkb", "plb", "prc", "fnc", "trg", "tps", "tpb"];

/// Run the engine analysis and assemble the structured analysis document.
pub fn run_plsql_analyze(req: PlsqlAnalyzeRequest) -> Result<PlsqlAnalyzeResponse, PlsqlAnalyzeError> {
    let root = PathBuf::from(&req.project_root);
    let run = engine_analyze(AnalysisRequest {
        project_root: root.clone(),
        ..AnalysisRequest::default()
    })
    .map_err(|e| PlsqlAnalyzeError::Engine(e.to_string()))?;

    // Routine/object inventory from the dependency graph nodes.
    let mut routines: Vec<RoutineInfo> = run
        .dep_graph
        .nodes
        .values()
        .map(|n| RoutineInfo {
            logical_id: n.logical_id.to_string(),
            kind: format!("{:?}", n.identity_kind),
        })
        .collect();
    routines.sort_by(|a, b| a.logical_id.cmp(&b.logical_id));

    // Call/ref summary: resolve each edge's endpoints to their logical ids.
    let mut call_refs: Vec<CallRef> = run
        .dep_graph
        .edges
        .iter()
        .filter_map(|e| {
            let from = run.dep_graph.nodes.get(&e.from)?.logical_id.to_string();
            let to = run.dep_graph.nodes.get(&e.to)?.logical_id.to_string();
            Some(CallRef { from, to, kind: format!("{:?}", e.kind) })
        })
        .collect();
    call_refs.sort_by(|a, b| (&a.from, &a.to, &a.kind).cmp(&(&b.from, &b.to, &b.kind)));

    // Lint = the analysis diagnostics.
    let lint: Vec<LintFinding> = run
        .diagnostics
        .iter()
        .map(|d| LintFinding {
            code: d.code.clone(),
            severity: format!("{:?}", d.severity),
            message: d.message.clone(),
        })
        .collect();

    let complexity = cyclomatic_by_file(&root);

    Ok(PlsqlAnalyzeResponse {
        project_root: req.project_root,
        routines,
        call_refs,
        lint,
        complexity,
    })
}

/// Cyclomatic complexity per PL/SQL source file under `root` (recursive walk).
fn cyclomatic_by_file(root: &Path) -> Vec<ComplexityInfo> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_plsql_source(&path) {
                if let Ok(src) = std::fs::read_to_string(&path) {
                    out.push(ComplexityInfo {
                        file: path.display().to_string(),
                        cyclomatic: cyclomatic(&src),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

fn is_plsql_source(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.to_ascii_lowercase())
        .is_some_and(|x| PLSQL_EXTS.contains(&x.as_str()))
}

/// McCabe cyclomatic complexity = 1 + decision points. Counts the branch-
/// introducing keywords (IF/ELSIF/CASE/WHEN/WHILE/FOR) and short-circuit
/// operators (AND/OR), while NOT counting the `END IF`/`END CASE` closers
/// (tracked via the previous token) or an unconditional bare `LOOP`.
fn cyclomatic(src: &str) -> u32 {
    let mut count: u32 = 1;
    let mut prev = String::new();
    for raw in src.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if raw.is_empty() {
            continue;
        }
        let tok = raw.to_ascii_uppercase();
        let is_decision = match tok.as_str() {
            "ELSIF" | "ELSEIF" | "WHEN" | "WHILE" | "FOR" | "AND" | "OR" => true,
            // `IF`/`CASE` are decisions only when not closing (`END IF`/`END CASE`).
            "IF" | "CASE" => prev != "END",
            _ => false,
        };
        if is_decision {
            count += 1;
        }
        prev = tok;
    }
    count
}

/// Register the `plsql_analyze` descriptor (foundation-static — no live DB).
pub fn register_plsql_analyze_tool(registry: &mut ToolRegistry) {
    registry.register(ToolDescriptor {
        name: String::from("plsql_analyze"),
        tier: ToolTier::FoundationStatic,
        summary: String::from(
            "Static PL/SQL analysis of a project (the oracle_plsql_analyze capability): \
             routine/object inventory, call/reference summary, lint findings, and per-file \
             cyclomatic complexity, as structured JSON. Offline (engine source analysis); \
             full ALL_ARGUMENTS parameter signatures are the live-DB enrichment.",
        ),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclomatic_counts_decision_points_not_closers() {
        // 1 (base) + IF + ELSIF + WHEN + WHILE + FOR + AND + OR = 8.
        // `END IF`/`END CASE`/`END LOOP` and the bare `LOOP`/`CASE`-closer add nothing.
        let src = "
            BEGIN
              IF x AND y THEN NULL;
              ELSIF z OR w THEN NULL;
              END IF;
              CASE a WHEN 1 THEN NULL; END CASE;
              WHILE cond LOOP NULL; END LOOP;
              FOR i IN 1..10 LOOP NULL; END LOOP;
            END;
        ";
        // IF(1) ELSIF(1) WHEN(1) WHILE(1) FOR(1) AND(1) OR(1) + CASE opener(1) = 8 decisions -> 9.
        assert_eq!(cyclomatic(src), 9);
    }

    #[test]
    fn straight_line_code_is_one() {
        assert_eq!(cyclomatic("BEGIN NULL; x := 1; END;"), 1);
        // `ORDER` / `ANDREW` must not match OR / AND (word-boundary tokenization).
        assert_eq!(cyclomatic("SELECT * FROM t ORDER BY andrew_col;"), 1);
    }

    #[test]
    fn end_if_does_not_double_count() {
        // One IF, closed by END IF -> 1 decision -> complexity 2.
        assert_eq!(cyclomatic("IF a THEN b; END IF;"), 2);
    }

    #[test]
    fn is_plsql_source_matches_extensions() {
        assert!(is_plsql_source(Path::new("/p/pkg.pks")));
        assert!(is_plsql_source(Path::new("/p/body.PKB")));
        assert!(!is_plsql_source(Path::new("/p/readme.md")));
    }

    #[test]
    fn request_response_serde_roundtrips() {
        let resp = PlsqlAnalyzeResponse {
            project_root: "/p".to_owned(),
            routines: vec![RoutineInfo { logical_id: "pkg.proc/1".to_owned(), kind: "Routine".to_owned() }],
            call_refs: vec![CallRef { from: "a".to_owned(), to: "b".to_owned(), kind: "Calls".to_owned() }],
            lint: vec![LintFinding { code: "X1".to_owned(), severity: "Warn".to_owned(), message: "m".to_owned() }],
            complexity: vec![ComplexityInfo { file: "/p/a.pkb".to_owned(), cyclomatic: 3 }],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["routines"][0]["logical_id"], serde_json::json!("pkg.proc/1"));
        assert_eq!(json["complexity"][0]["cyclomatic"], serde_json::json!(3));
        let back: PlsqlAnalyzeResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back, resp);
    }
}
