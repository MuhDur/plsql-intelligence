//! `analyze_project` foundation tool.
//!
//! Loads the engine, runs the canonical analysis pipeline
//! ([`plsql_engine::analyze_project`]) over a project root, and
//! returns a compact `AnalysisRun` summary. This is the entry point
//! every agent uses before asking for lineage / SAST / lifecycle —
//! one pipeline pass, summarised.
//!
//! Mirrors the per-tool module convention (`describe`, `query`,
//! …): a serde request/response pair plus a pure `run_*`
//! function and a descriptor registrar. The summary reuses the
//! engine's own [`EngineDoctorReport`] so the MCP surface and the
//! `plsql-engine doctor` CLI report identical, R13-honest numbers
//! (unwired stages stay zero with the catalog/PL-Scope
//! availability flags making the boundary explicit).

use std::path::PathBuf;

use plsql_engine::{
    AnalysisRequest, EngineDoctorReport, analyze_project as engine_analyze, engine_doctor_report,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ToolDescriptor, ToolRegistry, ToolTier};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeProjectRequest {
    /// Filesystem path to the project root to analyze.
    pub project_root: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeProjectResponse {
    pub project_root: String,
    /// Files the project walker discovered under the root.
    pub file_count: usize,
    /// Canonical AnalysisRun summary (same shape the
    /// `plsql-engine doctor` CLI emits).
    pub summary: EngineDoctorReport,
}

#[derive(Debug, Error)]
pub enum AnalyzeProjectError {
    /// The engine pipeline failed (bad root, unreadable source).
    /// The engine's typed message is preserved verbatim so the
    /// agent sees exactly which path/stage blocked the run (R13).
    #[error("engine analysis failed: {0}")]
    Engine(String),
}

/// Run the pipeline and return the summary.
pub fn run_analyze_project(
    req: AnalyzeProjectRequest,
) -> Result<AnalyzeProjectResponse, AnalyzeProjectError> {
    let request = AnalysisRequest {
        project_root: PathBuf::from(&req.project_root),
        ..AnalysisRequest::default()
    };
    let run = engine_analyze(request).map_err(|e| AnalyzeProjectError::Engine(format!("{e}")))?;
    Ok(AnalyzeProjectResponse {
        project_root: req.project_root,
        file_count: run.project.file_count,
        summary: engine_doctor_report(&run),
    })
}

/// Register the `analyze_project` descriptor. Foundation-static
/// tier — available regardless of safety profile / license, no
/// live DB.
pub fn register_analyze_project_tool(registry: &mut ToolRegistry) {
    registry.register(ToolDescriptor {
        name: String::from("analyze_project"),
        tier: ToolTier::FoundationStatic,
        summary: String::from(
            "Load the engine and run the canonical analysis pipeline over a project root; \
             returns an AnalysisRun summary (object/declaration counts, parsed-vs-recovered, \
             catalog/PL-Scope availability, diagnostic count, schema id/version).",
        ),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_a_clean_zero_run() {
        let resp = run_analyze_project(AnalyzeProjectRequest {
            project_root: String::new(),
        })
        .expect("empty root is a valid no-op run");
        assert_eq!(resp.file_count, 0);
        assert_eq!(resp.summary.objects_total, 0);
        assert!(!resp.summary.catalog_available);
        // The summary is an EngineDoctorReport — it carries the
        // doctor schema, distinct from the run-artifact schema.
        assert_eq!(resp.summary.schema_id, "plsql.engine.doctor");
    }

    #[test]
    fn analyzes_a_real_project_tree() {
        let dir = std::env::temp_dir().join(format!(
            "plsql-mcp003-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("p.sql"),
            "CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n",
        )
        .unwrap();

        let resp = run_analyze_project(AnalyzeProjectRequest {
            project_root: dir.display().to_string(),
        })
        .expect("pipeline ok");
        assert_eq!(resp.file_count, 1);
        assert!(resp.summary.objects_total >= 1);
        assert_eq!(resp.summary.objects_total, resp.summary.declaration_count);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nonexistent_root_is_a_clean_zero_run_not_a_crash() {
        // The engine treats a missing root as an empty,
        // reproducible no-op run (no manifest, no files) rather
        // than an error — the MCP tool surfaces that faithfully
        // instead of inventing a failure.
        let resp = run_analyze_project(AnalyzeProjectRequest {
            project_root: "/no/such/plsql/project/xyzzy".to_string(),
        })
        .expect("missing root is a clean zero run");
        assert_eq!(resp.file_count, 0);
        assert_eq!(resp.summary.objects_total, 0);
        assert!(!resp.summary.catalog_available);
    }

    #[test]
    fn registers_a_foundation_static_descriptor() {
        let mut reg = ToolRegistry::new();
        register_analyze_project_tool(&mut reg);
        register_analyze_project_tool(&mut reg); // idempotent
        assert_eq!(reg.len(), 1);
        let t = &reg.tools[0];
        assert_eq!(t.name, "analyze_project");
        assert_eq!(t.tier, ToolTier::FoundationStatic);
    }

    #[test]
    fn response_round_trips_through_json() {
        let resp = run_analyze_project(AnalyzeProjectRequest {
            project_root: String::new(),
        })
        .unwrap();
        let json = serde_json::to_string(&resp).unwrap();
        let back: AnalyzeProjectResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }
}
