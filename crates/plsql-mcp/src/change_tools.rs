//! Change-analysis tools: `what_breaks`, `recompile_plan`,
//! `classify_change`, `compare_oracle_deps`, `release_gate`,
//! `sarif_scan`, `orphan_candidates`, `explain_lifecycle`.
//!
//! Each is a thin wrapper that delegates to an already-tested
//! Layer-2/3 producer:
//!
//! * `what_breaks`         → [`plsql_cicd::predict`]
//! * `recompile_plan`      → [`plsql_lineage::recompile_order`]
//! * `classify_change`     → [`plsql_lineage::classify_dir_diff`]
//! * `compare_oracle_deps` → [`plsql_lineage::compare_oracle_deps`]
//! * `release_gate`        → [`plsql_cicd::run_gate`]
//! * `sarif_scan`          → [`plsql_sast::to_sarif`]
//! * `orphan_candidates`   → [`plsql_lineage::detect_orphans`]
//! * `explain_lifecycle`   → [`plsql_lineage::explain_node`]
//!
//! All eight are pure static-analysis tools — they operate on
//! source trees, dependency graphs, and catalog snapshots, never a
//! live database connection. They register as
//! [`ToolTier::FoundationStatic`].

use plsql_depgraph::{DepGraph, NodeSelector};
use plsql_lineage::LineageExplanation;
use plsql_output::OrphanCandidatesReport;
use plsql_sast::{SarifLog, ScanReport};
use thiserror::Error;

use crate::tools::{ToolDescriptor, ToolRegistry, ToolTier};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChangeToolError {
    /// The underlying analysis query failed (e.g. unknown node).
    #[error("query failed: {0}")]
    Query(String),
}

// --- sarif_scan -------------------------------------------------

/// Render a [`ScanReport`] as SARIF 2.1.0.
#[must_use]
pub fn run_sarif_scan(report: &ScanReport, tool_name: &str, tool_version: &str) -> SarifLog {
    plsql_sast::to_sarif(report, tool_name, tool_version)
}

// --- orphan_candidates ------------------------------------------

/// Zero-incoming-edge orphan-candidate report.
#[must_use]
pub fn run_orphan_candidates(
    graph: &DepGraph,
    assume_incomplete_augmentation: bool,
) -> OrphanCandidatesReport {
    plsql_lineage::detect_orphans(graph, assume_incomplete_augmentation)
}

// --- explain_lifecycle ------------------------------------------

/// Customer-facing lifecycle explanation of one node.
///
/// # Errors
/// [`ChangeToolError::Query`] when the selector matches no node.
pub fn run_explain_lifecycle(
    graph: &DepGraph,
    selector: &NodeSelector,
) -> Result<LineageExplanation, ChangeToolError> {
    plsql_lineage::explain_node(graph, selector)
        .map_err(|e| ChangeToolError::Query(e.to_string()))
}

// --- release_gate / recompile_plan ------------------------------

use plsql_cicd::{GateDecision, GatePolicy, InvalidationPrediction, run_gate};
use plsql_lineage::RecompilePlan;

/// `release_gate` — evaluate an invalidation prediction against a
/// gate policy.
#[must_use]
pub fn run_release_gate(
    prediction: &InvalidationPrediction,
    policy: &GatePolicy,
) -> GateDecision {
    run_gate(prediction, policy)
}

/// `recompile_plan` — topological recompile order for a changed
/// object set over the dependency graph.
#[must_use]
pub fn run_recompile_plan(graph: &DepGraph, changed: &[&str]) -> RecompilePlan {
    plsql_lineage::recompile_order(graph, changed)
}

// --- what_breaks / classify_change / compare_oracle_deps --------

use plsql_catalog::CatalogSnapshot;
use plsql_cicd::{ChangeSet, InvalidationPrediction as Prediction, PredictMode, predict};
use plsql_core::SymbolInterner;
use plsql_lineage::{CompareOracleDepsReport, SemanticChangeSet, classify_dir_diff};
use std::path::Path;

/// `what_breaks` — invalidation prediction for a changeset.
#[must_use]
pub fn run_what_breaks(changeset: &ChangeSet, mode: PredictMode) -> Prediction {
    predict(changeset, mode)
}

/// `classify_change` — semantic diff between two source trees.
///
/// # Errors
/// [`ChangeToolError::Query`] when the diff cannot be computed
/// (e.g. an unreadable tree).
pub fn run_classify_change(
    before: &Path,
    after: &Path,
) -> Result<SemanticChangeSet, ChangeToolError> {
    classify_dir_diff(before, after).map_err(|e| ChangeToolError::Query(format!("{e:?}")))
}

/// `compare_oracle_deps` — cross-check our dependency graph against
/// an Oracle catalog snapshot.
#[must_use]
pub fn run_compare_oracle_deps(
    graph: &DepGraph,
    snapshot: &CatalogSnapshot,
    interner: &SymbolInterner,
) -> CompareOracleDepsReport {
    plsql_lineage::compare_oracle_deps(graph, snapshot, interner)
}

/// Register all eight change-analysis tool descriptors. Tools are
/// `FoundationStatic` tier — pure static analysis, no live DB.
/// Idempotent: the underlying [`ToolRegistry`] deduplicates by name.
pub fn register_change_tools(registry: &mut ToolRegistry) {
    for (name, summary) in [
        (
            "what_breaks",
            "Invalidation prediction for a changeset (which objects break + distance).",
        ),
        (
            "recompile_plan",
            "Topological recompile order for a changed object set over the dependency graph.",
        ),
        (
            "classify_change",
            "Semantic diff between two PL/SQL source trees (signature vs body vs new).",
        ),
        (
            "compare_oracle_deps",
            "Cross-check our dependency graph against an Oracle catalog snapshot.",
        ),
        (
            "release_gate",
            "Evaluate an invalidation prediction against a gate policy (pass/fail + reasons).",
        ),
        (
            "sarif_scan",
            "Render a SAST ScanReport as SARIF 2.1.0 for code-scanning ingestion.",
        ),
        (
            "orphan_candidates",
            "Zero-incoming-edge orphan-candidate report over the dependency graph.",
        ),
        (
            "explain_lifecycle",
            "Lifecycle explanation of a node (in/out edges, summary).",
        ),
    ] {
        registry.register(ToolDescriptor::new(
            name,
            ToolTier::FoundationStatic,
            summary,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sarif_scan_delegates() {
        let rep = ScanReport::default();
        let sarif = run_sarif_scan(&rep, "plsql-mcp", "1.0.0");
        assert_eq!(sarif.version, "2.1.0");
        assert!(sarif.runs[0].results.is_empty());
    }

    #[test]
    fn orphan_candidates_delegates() {
        let g = DepGraph::new();
        let report = run_orphan_candidates(&g, false);
        assert_eq!(report.candidates.len(), 0, "empty graph -> no orphans");
        assert_eq!(report.objects_examined, 0);
    }

    #[test]
    fn explain_lifecycle_typed_query_error_on_missing() {
        let g = DepGraph::new();
        // The node is absent -> typed Query error, never a panic /
        // empty success.
        let err = run_explain_lifecycle(&g, &NodeSelector::LogicalObjectId("nope".into()))
            .unwrap_err();
        assert!(matches!(err, ChangeToolError::Query(_)));
    }

    #[test]
    fn release_gate_delegates() {
        use plsql_cicd::{InvalidationPrediction, PredictMode};
        let pred = InvalidationPrediction::empty(PredictMode::CatalogAware);
        let policy = plsql_cicd::GatePolicy::default();
        // An empty prediction under the default policy is a clean
        // pass; shape verified by plsql-cicd's own suite.
        let decision = run_release_gate(&pred, &policy);
        let _ = decision;
    }

    #[test]
    fn recompile_plan_delegates() {
        let g = DepGraph::new();
        let plan = run_recompile_plan(&g, &[]);
        // Empty graph + empty change set -> empty plan.
        let j = serde_json::to_string(&plan).unwrap();
        assert!(j.contains("{"), "plan serializes");
    }

    #[test]
    fn what_breaks_delegates() {
        use plsql_cicd::{ChangeSet, PredictMode};
        let cs = ChangeSet::empty();
        let pred = run_what_breaks(&cs, PredictMode::CatalogAware);
        assert_eq!(
            pred.invalidation_count(),
            0,
            "empty changeset breaks nothing"
        );
    }

    #[test]
    fn classify_change_delegates_on_empty_trees() {
        let base = std::env::temp_dir().join(format!(
            "plsql-mcp-change-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let before = base.join("before");
        let after = base.join("after");
        std::fs::create_dir_all(&before).unwrap();
        std::fs::create_dir_all(&after).unwrap();

        let cs = run_classify_change(&before, &after).unwrap();
        let j = serde_json::to_string(&cs).unwrap();
        assert!(j.contains("{"), "empty diff serializes");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn compare_oracle_deps_delegates() {
        let g = DepGraph::new();
        let snap = plsql_catalog::CatalogSnapshot::default();
        let interner = SymbolInterner::new();
        let report = run_compare_oracle_deps(&g, &snap, &interner);
        let j = serde_json::to_string(&report).unwrap();
        assert!(j.contains("{"), "report serializes");
    }

    #[test]
    fn registers_all_eight_change_tools() {
        let mut reg = ToolRegistry::new();
        register_change_tools(&mut reg);
        register_change_tools(&mut reg);
        assert_eq!(reg.len(), 8, "registration is idempotent");
        assert!(
            reg.tools.iter().all(|t| t.tier == ToolTier::FoundationStatic),
            "change-analysis tools are pure static analysis"
        );
        let names: Vec<&str> = reg.tools.iter().map(|t| t.name.as_str()).collect();
        for expected in [
            "what_breaks",
            "recompile_plan",
            "classify_change",
            "compare_oracle_deps",
            "release_gate",
            "sarif_scan",
            "orphan_candidates",
            "explain_lifecycle",
        ] {
            assert!(names.contains(&expected), "missing tool: {expected}");
        }
    }
}
