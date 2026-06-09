//! Dependency-graph foundation tools:
//! `find_callers`, `find_callees`, `get_dependencies`.
//!
//! All three are pure read queries against the `DepGraph` an
//! `AnalysisRun` carries. They delegate to `plsql-depgraph`'s
//! already-tested `query_neighbors` / `query_reverse_neighbors`
//! and only add: a typed MCP error for an unknown target and the
//! `get_dependencies` reshape (a deduped, sorted dependency id
//! list rather than full edge detail).
//!
//! Direction contract:
//! * `find_callers`  — *reverse* neighbours: nodes with an edge
//!   **into** the target (who depends on / calls it).
//! * `find_callees`  — *forward* neighbours with full edge
//!   detail (what the target calls / reads / writes; the edge
//!   `kind` distinguishes call vs data dependency).
//! * `get_dependencies` — *forward*, reduced to the sorted unique
//!   set of target logical ids (the "what does this need" list).

use oraclemcp_error::{ErrorClass, ErrorEnvelope, fuzzy_suggest};
use plsql_depgraph::{DepGraph, NeighborhoodQueryResult, NodeSelector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ToolDescriptor, ToolRegistry, ToolTier};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphQueryRequest {
    /// Logical object id of the target node, e.g.
    /// `billing.claims_pkg.calculate/1`.
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NeighborhoodResponse {
    pub result: NeighborhoodQueryResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependenciesResponse {
    pub target: String,
    /// Sorted, de-duplicated logical ids the target depends on.
    pub dependencies: Vec<String>,
}

#[derive(Debug, Error)]
pub enum GraphToolError {
    /// The target node is not in the analysed graph. Carries the
    /// selector verbatim so the agent can correct the id (R13 —
    /// "absent" is reported, never an empty success).
    #[error("graph query failed: {0}")]
    Query(String),
}

impl GraphToolError {
    /// Render this failure as an actionable [`ErrorEnvelope`] (oracle-da9j.11).
    ///
    /// An unknown graph target is an [`ErrorClass::ObjectNotFound`]: the agent
    /// supplied a logical object id the analysed [`DepGraph`] does not hold. The
    /// envelope carries near-miss `fuzzy_matches` drawn from the graph's own
    /// node ids (the graph holds every valid id, so a wrong-arity or misspelled
    /// target — `pkg.proc/2` for `pkg.proc/1` — surfaces as a one-character
    /// near miss) and a `suggested_tool` of `analyze_project` (the tool that
    /// (re)loads the graph and emits the canonical id set).
    #[must_use]
    pub fn to_envelope(&self, graph: &DepGraph, target: &str) -> ErrorEnvelope {
        let GraphToolError::Query(message) = self;
        let ids: Vec<String> = graph
            .nodes
            .values()
            .map(|n| n.logical_id.as_str().to_owned())
            .collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let matches = fuzzy_suggest(target, &id_refs, 5);
        let mut env = ErrorEnvelope::new(ErrorClass::ObjectNotFound, message.clone())
            .with_suggested_tool("analyze_project");
        if matches.is_empty() {
            env = env.with_next_step(format!(
                "`{target}` is not a node in the analysed graph and no near match exists — \
                 run analyze_project (or plsql_analyze) to (re)load the graph and obtain the \
                 valid logical object ids"
            ));
        } else {
            env = env
                .with_next_step(format!(
                    "`{target}` is not in the analysed graph — did you mean one of these?"
                ))
                .with_fuzzy_matches(matches);
        }
        env
    }
}

fn selector(target: &str) -> NodeSelector {
    NodeSelector::LogicalObjectId(target.to_string())
}

/// `find_callers` — who points **at** `target`.
pub fn run_find_callers(
    graph: &DepGraph,
    req: &GraphQueryRequest,
) -> Result<NeighborhoodResponse, GraphToolError> {
    let result = graph
        .query_reverse_neighbors(&selector(&req.target))
        .map_err(|e| GraphToolError::Query(e.to_string()))?;
    Ok(NeighborhoodResponse { result })
}

/// `find_callees` — what `target` points at (full edge detail).
pub fn run_find_callees(
    graph: &DepGraph,
    req: &GraphQueryRequest,
) -> Result<NeighborhoodResponse, GraphToolError> {
    let result = graph
        .query_neighbors(&selector(&req.target))
        .map_err(|e| GraphToolError::Query(e.to_string()))?;
    Ok(NeighborhoodResponse { result })
}

/// `get_dependencies` — the sorted unique set of logical ids the
/// target depends on (forward edges, reshaped to a plain list).
pub fn run_get_dependencies(
    graph: &DepGraph,
    req: &GraphQueryRequest,
) -> Result<DependenciesResponse, GraphToolError> {
    let result = graph
        .query_neighbors(&selector(&req.target))
        .map_err(|e| GraphToolError::Query(e.to_string()))?;
    let mut deps: Vec<String> = result
        .edges
        .iter()
        .map(|e| e.to.logical_id.clone())
        .collect();
    deps.sort();
    deps.dedup();
    Ok(DependenciesResponse {
        target: req.target.clone(),
        dependencies: deps,
    })
}

/// Register the three descriptors. Foundation-static tier.
pub fn register_graph_tools(registry: &mut ToolRegistry) {
    for (name, summary) in [
        (
            "find_callers",
            "Reverse dependency-graph neighbours: every node with an edge into the target \
             (callers / dependents). Reads the AnalysisRun's DepGraph; no live DB.",
        ),
        (
            "find_callees",
            "Forward dependency-graph neighbours with full edge detail (kind/confidence): \
             what the target calls, reads, or writes.",
        ),
        (
            "get_dependencies",
            "The sorted, de-duplicated set of logical object ids the target depends on \
             (forward edges reshaped to a flat list). When to use: prefer find_callees when \
             you need per-edge kind/confidence detail — this returns only the flat id set. \
             Run analyze_project (or plsql_analyze) first to load the graph and obtain valid \
             target ids.",
        ),
    ] {
        // All three graph tools take a single `target` logical object id in the
        // arity-form the engine assigns (oracle-da9j.1). Run plsql_analyze /
        // analyze_project first to obtain a valid id.
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["target"],
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Logical object id of the target node in arity form, \
                                    e.g. `billing.claims_pkg.calculate/1`. Obtain valid ids \
                                    from plsql_analyze / analyze_project output.",
                },
            },
        });
        registry.register(
            ToolDescriptor::new(name, ToolTier::FoundationStatic, summary)
                .with_input_schema(schema),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{
        Confidence, ConfidenceLevel, FileId, ObjectName, Position, Span, SymbolInterner,
    };
    use plsql_depgraph::{
        Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind, ObjectRevisionId,
        Provenance, QualifiedName, ResolutionStrategy,
    };

    fn span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 9, 8),
        )
    }

    /// Tiny graph: `pkg.proc` --calls--> `pkg.helper`,
    /// `pkg.proc` --reads--> `app.t`.
    fn fixture() -> DepGraph {
        let mut interner = SymbolInterner::new();
        let s = |i: &mut SymbolInterner, n: &str| i.intern(n).unwrap();
        let mut g = DepGraph::new();
        let mk = |g: &mut DepGraph, id: u64, lid: &str, sym, kind| {
            g.insert_node(Node::new(
                NodeId::new(id),
                LogicalObjectId::new(lid),
                ObjectRevisionId::new("sha256:x"),
                QualifiedName::new(None, ObjectName::from(sym)),
                kind,
            ));
        };
        let a = s(&mut interner, "PROC");
        let b = s(&mut interner, "HELPER");
        let c = s(&mut interner, "T");
        mk(
            &mut g,
            1,
            "pkg.proc/1",
            a,
            NodeIdentityKind::PackageProcedure,
        );
        mk(
            &mut g,
            2,
            "pkg.helper/1",
            b,
            NodeIdentityKind::PackageProcedure,
        );
        mk(&mut g, 3, "app.t", c, NodeIdentityKind::Table);
        let prov = || Provenance::new(FileId::new(1), span(), ResolutionStrategy::CatalogLookup);
        g.insert_edge(
            Edge::new(
                EdgeId::new(1),
                NodeId::new(1),
                NodeId::new(2),
                EdgeKind::Calls,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            prov(),
            None,
        );
        g.insert_edge(
            Edge::new(
                EdgeId::new(2),
                NodeId::new(1),
                NodeId::new(3),
                EdgeKind::Reads,
                Confidence::new(ConfidenceLevel::High, None),
            ),
            prov(),
            None,
        );
        g
    }

    fn req(t: &str) -> GraphQueryRequest {
        GraphQueryRequest {
            target: t.to_string(),
        }
    }

    #[test]
    fn callees_returns_forward_edges() {
        let g = fixture();
        let r = run_find_callees(&g, &req("pkg.proc/1")).unwrap();
        assert_eq!(r.result.edges.len(), 2);
    }

    #[test]
    fn callers_returns_reverse_edges() {
        let g = fixture();
        let r = run_find_callers(&g, &req("pkg.helper/1")).unwrap();
        assert_eq!(r.result.edges.len(), 1);
        assert_eq!(r.result.edges[0].from.logical_id, "pkg.proc/1");
    }

    #[test]
    fn dependencies_are_sorted_unique_ids() {
        let g = fixture();
        let r = run_get_dependencies(&g, &req("pkg.proc/1")).unwrap();
        assert_eq!(r.dependencies, vec!["app.t", "pkg.helper/1"]);
    }

    #[test]
    fn unknown_target_is_typed_error_not_empty_success() {
        let g = fixture();
        let e = run_find_callers(&g, &req("does.not.exist")).unwrap_err();
        let GraphToolError::Query(msg) = e;
        assert!(msg.contains("does.not.exist"), "selector echoed: {msg}");
    }

    // ── oracle-da9j.11: unknown graph target -> ObjectNotFound w/ fuzzy ──

    #[test]
    fn wrong_arity_target_yields_fuzzy_near_miss() {
        // A wrong-arity target (`pkg.proc/2` for the graph's `pkg.proc/1`) is a
        // one-character near miss: the envelope must classify as ObjectNotFound,
        // suggest analyze_project, and surface the real id as a fuzzy candidate.
        let g = fixture();
        let target = "pkg.proc/2";
        let e = run_find_callees(&g, &req(target)).unwrap_err();
        let env = e.to_envelope(&g, target);
        assert_eq!(env.error_class, oraclemcp_error::ErrorClass::ObjectNotFound);
        assert_eq!(env.suggested_tool.as_deref(), Some("analyze_project"));
        assert!(
            env.fuzzy_matches.contains(&"pkg.proc/1".to_owned()),
            "expected pkg.proc/1 near-miss, got {:?}",
            env.fuzzy_matches
        );
    }

    #[test]
    fn far_target_has_no_fuzzy_match_but_still_classifies() {
        // A target with no near miss still classifies as ObjectNotFound and
        // points the agent at analyze_project to (re)load the id set.
        let g = fixture();
        let target = "totally.unrelated.zzzzzzzz";
        let e = run_find_callers(&g, &req(target)).unwrap_err();
        let env = e.to_envelope(&g, target);
        assert_eq!(env.error_class, oraclemcp_error::ErrorClass::ObjectNotFound);
        assert!(env.fuzzy_matches.is_empty(), "no near miss expected");
        assert!(
            env.next_steps.iter().any(|s| s.contains("analyze_project")),
            "next_step must steer to analyze_project: {:?}",
            env.next_steps
        );
    }

    #[test]
    fn leaf_node_has_no_callees() {
        let g = fixture();
        let r = run_find_callees(&g, &req("app.t")).unwrap();
        assert!(r.result.edges.is_empty());
        let d = run_get_dependencies(&g, &req("app.t")).unwrap();
        assert!(d.dependencies.is_empty());
    }

    #[test]
    fn responses_round_trip_through_json() {
        let g = fixture();
        let n = run_find_callees(&g, &req("pkg.proc/1")).unwrap();
        let j = serde_json::to_string(&n).unwrap();
        let back: NeighborhoodResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back, n);
        let d = run_get_dependencies(&g, &req("pkg.proc/1")).unwrap();
        let dj = serde_json::to_string(&d).unwrap();
        let db: DependenciesResponse = serde_json::from_str(&dj).unwrap();
        assert_eq!(db, d);
    }

    #[test]
    fn registers_three_foundation_static_tools() {
        let mut reg = ToolRegistry::new();
        register_graph_tools(&mut reg);
        register_graph_tools(&mut reg);
        assert_eq!(reg.len(), 3);
        assert!(
            reg.tools
                .iter()
                .all(|t| t.tier == ToolTier::FoundationStatic)
        );
        let names: Vec<&str> = reg.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"find_callers"));
        assert!(names.contains(&"find_callees"));
        assert!(names.contains(&"get_dependencies"));
    }
}
