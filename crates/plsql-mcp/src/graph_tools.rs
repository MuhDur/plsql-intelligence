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
