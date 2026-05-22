//! Foundation tools (PLSQL-MCP-006): `dynamic_sql_evidence`,
//! `completeness_report`, `doc_lookup`.
//!
//! All three are pure, read-only, no-live-DB foundation-static
//! tools layered directly on already-tested Layer-2 surfaces:
//!
//! * `dynamic_sql_evidence` → `plsql_symbols::recognise_dynamic_sql`
//!   over a single call-text site.
//! * `completeness_report` → run the engine over a project root
//!   and return the populated `CompletenessReport` (the same
//!   honest, R13-partial block the `analyze_project` summary and
//!   `plsql-engine doctor` expose).
//! * `doc_lookup` → `plsql_doc::extract_doc_comments` over a
//!   source unit, filtered by a query substring.

use plsql_core::CompletenessReport;
use plsql_doc::{DocComment, extract_doc_comments};
use plsql_engine::{AnalysisRequest, analyze_project};
use plsql_symbols::{DynamicSqlEvidence, recognise_dynamic_sql};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ToolDescriptor, ToolRegistry, ToolTier};

// --- dynamic_sql_evidence ---------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DynamicSqlEvidenceRequest {
    /// The dynamic-SQL call text (e.g. an `EXECUTE IMMEDIATE …`
    /// statement) to analyse.
    pub call_text: String,
    /// Logical site id for the report (`file:line` or unit id).
    pub site: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicSqlEvidenceResponse {
    /// `None` ⇒ the text is not a recognised dynamic-SQL sink
    /// (not an error — a definite "no evidence here", R13).
    pub evidence: Option<DynamicSqlEvidence>,
}

#[must_use]
pub fn run_dynamic_sql_evidence(req: &DynamicSqlEvidenceRequest) -> DynamicSqlEvidenceResponse {
    DynamicSqlEvidenceResponse {
        evidence: recognise_dynamic_sql(&req.call_text, &req.site),
    }
}

// --- completeness_report ----------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompletenessReportRequest {
    pub project_root: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletenessReportResponse {
    pub project_root: String,
    pub completeness: CompletenessReport,
}

#[derive(Debug, Error)]
pub enum FoundationToolError {
    #[error("engine analysis failed: {0}")]
    Engine(String),
}

pub fn run_completeness_report(
    req: &CompletenessReportRequest,
) -> Result<CompletenessReportResponse, FoundationToolError> {
    let run = analyze_project(AnalysisRequest {
        project_root: std::path::PathBuf::from(&req.project_root),
        ..AnalysisRequest::default()
    })
    .map_err(|e| FoundationToolError::Engine(format!("{e}")))?;
    Ok(CompletenessReportResponse {
        project_root: req.project_root.clone(),
        completeness: run.completeness,
    })
}

// --- doc_lookup -------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocLookupRequest {
    /// PL/SQL source to extract doc comments from.
    pub source: String,
    /// Case-insensitive substring to match against a comment's
    /// tag or body. Empty ⇒ return every doc comment.
    #[serde(default)]
    pub query: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocLookupResponse {
    pub matches: Vec<DocComment>,
}

#[must_use]
pub fn run_doc_lookup(req: &DocLookupRequest) -> DocLookupResponse {
    let q = req.query.to_ascii_lowercase();
    let matches = extract_doc_comments(&req.source)
        .into_iter()
        .filter(|c| {
            q.is_empty()
                || c.text.to_ascii_lowercase().contains(&q)
                || c.tag
                    .as_deref()
                    .is_some_and(|t| t.to_ascii_lowercase().contains(&q))
        })
        .collect();
    DocLookupResponse { matches }
}

/// Register the three descriptors. Foundation-static tier.
pub fn register_foundation_tools(registry: &mut ToolRegistry) {
    for (name, summary) in [
        (
            "dynamic_sql_evidence",
            "Recognise a dynamic-SQL call site (EXECUTE IMMEDIATE / DBMS_SQL) and return its \
             fragment shape, bind usage, DBMS_ASSERT sanitisers, and candidate objects. \
             Not-recognised ⇒ evidence:null (a definite no, not an error).",
        ),
        (
            "completeness_report",
            "Run the canonical pipeline over a project root and return the CompletenessReport \
             block (files parsed/recovered, object totals, catalog/PL-Scope availability).",
        ),
        (
            "doc_lookup",
            "Extract Javadoc-style / legacy doc comments from a PL/SQL source unit, optionally \
             filtered by a case-insensitive query against tag or body.",
        ),
    ] {
        registry.register(ToolDescriptor {
            name: String::from(name),
            tier: ToolTier::FoundationStatic,
            summary: String::from(summary),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_sql_evidence_recognises_execute_immediate() {
        let resp = run_dynamic_sql_evidence(&DynamicSqlEvidenceRequest {
            call_text: "EXECUTE IMMEDIATE 'SELECT * FROM ' || p_tab USING x".to_string(),
            site: "hr.proc:12".to_string(),
        });
        let ev = resp.evidence.expect("EXECUTE IMMEDIATE is dynamic SQL");
        assert_eq!(ev.site, "hr.proc:12");
        assert!(ev.uses_binds, "USING clause -> bound");
        assert!(!ev.fragments.is_empty());
    }

    #[test]
    fn dynamic_sql_evidence_none_for_plain_sql_is_not_an_error() {
        let resp = run_dynamic_sql_evidence(&DynamicSqlEvidenceRequest {
            call_text: "v_total := a + b".to_string(),
            site: "s".to_string(),
        });
        assert!(resp.evidence.is_none(), "not a dynamic-SQL sink => None");
    }

    #[test]
    fn completeness_report_runs_pipeline() {
        let dir = std::env::temp_dir().join(format!(
            "plsql-mcp006-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.sql"),
            "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n",
        )
        .unwrap();
        let resp = run_completeness_report(&CompletenessReportRequest {
            project_root: dir.display().to_string(),
        })
        .expect("pipeline ok");
        assert_eq!(resp.completeness.files_total, 1);
        assert!(!resp.completeness.catalog_available);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn completeness_report_empty_root_is_clean_zero() {
        let resp = run_completeness_report(&CompletenessReportRequest {
            project_root: String::new(),
        })
        .unwrap();
        assert_eq!(resp.completeness.files_total, 0);
    }

    #[test]
    fn doc_lookup_extracts_and_filters() {
        let src = "/**\n * @param p_id the employee id\n * Fetches one row.\n */\n\
                   PROCEDURE get_emp(p_id NUMBER);\n";
        let all = run_doc_lookup(&DocLookupRequest {
            source: src.to_string(),
            query: String::new(),
        });
        assert!(!all.matches.is_empty(), "extracts the block comment");

        let hit = run_doc_lookup(&DocLookupRequest {
            source: src.to_string(),
            query: "employee".to_string(),
        });
        assert_eq!(hit.matches.len(), 1);

        let miss = run_doc_lookup(&DocLookupRequest {
            source: src.to_string(),
            query: "zzzznotpresent".to_string(),
        });
        assert!(miss.matches.is_empty());
    }

    #[test]
    fn responses_round_trip_through_json() {
        let d = run_dynamic_sql_evidence(&DynamicSqlEvidenceRequest {
            call_text: "EXECUTE IMMEDIATE 'x'".to_string(),
            site: "s".to_string(),
        });
        let j = serde_json::to_string(&d).unwrap();
        let back: DynamicSqlEvidenceResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn registers_three_foundation_static_tools() {
        let mut reg = ToolRegistry::new();
        register_foundation_tools(&mut reg);
        register_foundation_tools(&mut reg);
        assert_eq!(reg.len(), 3);
        assert!(
            reg.tools
                .iter()
                .all(|t| t.tier == ToolTier::FoundationStatic)
        );
        let names: Vec<&str> = reg.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"dynamic_sql_evidence"));
        assert!(names.contains(&"completeness_report"));
        assert!(names.contains(&"doc_lookup"));
    }
}
