//! End-to-end dep-graph test over a synthetic 30-package corpus.
//!
//! Builds a deterministic [`DepGraph`] mimicking a 30-package schema
//! and asserts two contracts an operator depends on:
//!
//! 1. **Expected edges** — the reads / writes / call-chain edges the
//!    builder wired are observable via `query_neighbors`, with the
//!    right [`EdgeKind`].
//! 2. **Cycle detection** — a deliberately injected 3-package call
//!    cycle (`PKG27 → PKG28 → PKG29 → PKG27`) is reported by
//!    `detect_cycles`, while the acyclic `PKG00…PKG24` call chain is
//!    NOT mis-reported as a cycle.
//!
//! Shape — 40 nodes:
//!
//! - 5 source tables `T00…T04` (pure sources: only `table → pkg`
//!   [`Reads`] edges, never written — so they cannot sit in a
//!   cycle).
//! - 5 sink tables `S00…S04` (pure sinks: only `pkg → sink`
//!   [`Writes`] edges, no outgoing edges — also cycle-free).
//! - 30 packages `PKG00…PKG29`.
//!   - Each package reads 1–2 source tables (`table → pkg`,
//!     [`Reads`]).
//!   - Even-indexed packages write one sink table (`pkg → sink`,
//!     [`Writes`]).
//!   - `PKG00…PKG24` form an acyclic call chain
//!     (`PKG(i-1) → PKG(i)`, [`Calls`]).
//!   - `PKG27 → PKG28 → PKG29 → PKG27` is the single closed call
//!     cycle — keeping tables acyclic means `detect_cycles` returns
//!     exactly this one.
//!
//! [`Reads`]: EdgeKind::Reads
//! [`Writes`]: EdgeKind::Writes
//! [`Calls`]: EdgeKind::Calls

use std::collections::BTreeMap;

use plsql_core::{
    Confidence, ConfidenceLevel, FileId, ObjectName, Position, SchemaName, Span, SymbolInterner,
};
use plsql_depgraph::{
    DepGraph, Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind,
    NodeSelector, ObjectRevisionId, Provenance, QualifiedName, ResolutionStrategy,
};

const SCHEMA: &str = "WAREHOUSE";
const TABLE_COUNT: usize = 5;
const PACKAGE_COUNT: usize = 30;

struct Fixture {
    graph: DepGraph,
    nodes_by_name: BTreeMap<String, NodeId>,
}

fn build_fixture() -> Fixture {
    let mut interner = SymbolInterner::new();
    let schema_name = SchemaName::from(
        interner
            .intern_schema_name(SCHEMA)
            .expect("schema interns")
            .symbol(),
    );
    let mut graph = DepGraph::new();
    let mut nodes_by_name: BTreeMap<String, NodeId> = BTreeMap::new();
    let mut next_node_id: u64 = 1;
    let mut next_edge_id: u64 = 1;

    let mut mk_node = |graph: &mut DepGraph,
                       nodes: &mut BTreeMap<String, NodeId>,
                       interner: &mut SymbolInterner,
                       name: &str| {
        let node_id = NodeId::new(next_node_id);
        next_node_id += 1;
        let object_name = ObjectName::from(interner.intern(name).expect("object name interns"));
        let qname = QualifiedName::new(Some(schema_name), object_name);
        graph.insert_node(Node::new(
            node_id,
            LogicalObjectId::new(format!("{SCHEMA}.{name}")),
            ObjectRevisionId::new("rev1"),
            qname,
            NodeIdentityKind::PackageBody,
        ));
        nodes.insert(name.to_string(), node_id);
        node_id
    };

    let mut mk_edge = |graph: &mut DepGraph, from: NodeId, to: NodeId, kind: EdgeKind| {
        let edge = Edge::new(
            EdgeId::new(next_edge_id),
            from,
            to,
            kind,
            Confidence::new(ConfidenceLevel::High, None),
        );
        next_edge_id += 1;
        let span = Span::new(
            FileId::new(0),
            Position::new(1, 1, 0),
            Position::new(1, 1, 1),
        );
        let provenance = Provenance::new(FileId::new(0), span, ResolutionStrategy::CatalogLookup);
        graph.insert_edge(edge, provenance, None);
    };

    let tables: Vec<NodeId> = (0..TABLE_COUNT)
        .map(|i| {
            mk_node(
                &mut graph,
                &mut nodes_by_name,
                &mut interner,
                &format!("T{i:02}"),
            )
        })
        .collect();
    let sinks: Vec<NodeId> = (0..TABLE_COUNT)
        .map(|i| {
            mk_node(
                &mut graph,
                &mut nodes_by_name,
                &mut interner,
                &format!("S{i:02}"),
            )
        })
        .collect();

    let mut packages: Vec<NodeId> = Vec::with_capacity(PACKAGE_COUNT);
    for i in 0..PACKAGE_COUNT {
        let pkg = mk_node(
            &mut graph,
            &mut nodes_by_name,
            &mut interner,
            &format!("PKG{i:02}"),
        );
        packages.push(pkg);

        // Reads: 1-2 tables, table → pkg.
        let reads = (i % 2) + 1;
        for r in 0..reads {
            mk_edge(
                &mut graph,
                tables[(i + r) % TABLE_COUNT],
                pkg,
                EdgeKind::Reads,
            );
        }
        // Even packages write one *sink* table, pkg → sink. Sinks
        // have no outgoing edges, so this never closes a loop.
        if i % 2 == 0 {
            mk_edge(&mut graph, pkg, sinks[i % TABLE_COUNT], EdgeKind::Writes);
        }
        // Acyclic call chain PKG00..PKG24.
        if (1..25).contains(&i) {
            mk_edge(&mut graph, packages[i - 1], pkg, EdgeKind::Calls);
        }
    }

    // Deliberate 3-package call cycle: 27 → 28 → 29 → 27.
    mk_edge(&mut graph, packages[27], packages[28], EdgeKind::Calls);
    mk_edge(&mut graph, packages[28], packages[29], EdgeKind::Calls);
    mk_edge(&mut graph, packages[29], packages[27], EdgeKind::Calls);

    Fixture {
        graph,
        nodes_by_name,
    }
}

fn id(fx: &Fixture, name: &str) -> NodeId {
    *fx.nodes_by_name
        .get(name)
        .unwrap_or_else(|| panic!("missing node `{name}`"))
}

fn outgoing(fx: &Fixture, name: &str) -> Vec<(String, EdgeKind)> {
    let sel = NodeSelector::NodeId(id(fx, name));
    fx.graph
        .query_neighbors(&sel)
        .expect("node resolves")
        .edges
        .into_iter()
        .map(|e| {
            let to =
                e.to.logical_id
                    .strip_prefix(&format!("{SCHEMA}."))
                    .unwrap_or(&e.to.logical_id)
                    .to_string();
            (to, e.kind)
        })
        .collect()
}

#[test]
fn fixture_has_expected_node_and_edge_counts() {
    let fx = build_fixture();
    assert_eq!(fx.graph.node_count(), TABLE_COUNT * 2 + PACKAGE_COUNT);
    // 30 pkgs * (1-2 reads) + 15 even writes + 24 chain calls + 3
    // cycle calls — exact count is deterministic; assert it's
    // non-trivial and stable across runs.
    let n = fx.graph.edge_count();
    assert!(n >= PACKAGE_COUNT, "expected many edges, got {n}");
    let again = build_fixture().graph.edge_count();
    assert_eq!(n, again, "edge count must be deterministic");
}

#[test]
fn call_chain_edges_are_observable_with_calls_kind() {
    let fx = build_fixture();
    // PKG00 → PKG01 is the first chain link.
    let edges = outgoing(&fx, "PKG00");
    assert!(
        edges
            .iter()
            .any(|(to, k)| to == "PKG01" && *k == EdgeKind::Calls),
        "PKG00 must Call PKG01; got {edges:?}"
    );
}

#[test]
fn read_and_write_edges_have_correct_kinds() {
    let fx = build_fixture();
    // T00 → PKG00 is a Reads edge (table feeds package).
    let from_t00 = outgoing(&fx, "T00");
    assert!(
        from_t00
            .iter()
            .any(|(to, k)| to == "PKG00" && *k == EdgeKind::Reads),
        "T00 must be Read by PKG00; got {from_t00:?}"
    );
    // PKG00 is even → writes sink S00 (pkg → sink).
    let from_pkg00 = outgoing(&fx, "PKG00");
    assert!(
        from_pkg00
            .iter()
            .any(|(to, k)| to == "S00" && *k == EdgeKind::Writes),
        "PKG00 (even) must Write sink S00; got {from_pkg00:?}"
    );
}

#[test]
fn injected_cycle_is_detected() {
    let fx = build_fixture();
    let result = fx.graph.detect_cycles().expect("cycle detection runs");
    assert_eq!(
        result.cycles.len(),
        1,
        "exactly the injected 3-package cycle; got {} cycles",
        result.cycles.len()
    );
    let members: std::collections::BTreeSet<String> = result.cycles[0]
        .nodes
        .iter()
        .filter_map(|n| {
            n.logical_id
                .strip_prefix(&format!("{SCHEMA}."))
                .map(str::to_string)
        })
        .collect();
    for pkg in ["PKG27", "PKG28", "PKG29"] {
        assert!(
            members.contains(pkg),
            "cycle must include {pkg}; got {members:?}"
        );
    }
}

#[test]
fn acyclic_call_chain_is_not_reported_as_cycle() {
    let fx = build_fixture();
    let result = fx.graph.detect_cycles().expect("cycle detection runs");
    // The only cycle is the injected one; the PKG00..PKG24 chain
    // must not appear. Verify no cycle contains PKG10 (mid-chain).
    for cycle in &result.cycles {
        let has_chain_node = cycle.nodes.iter().any(|n| {
            n.logical_id
                .strip_prefix(&format!("{SCHEMA}."))
                .map(|s| s == "PKG10")
                .unwrap_or(false)
        });
        assert!(
            !has_chain_node,
            "acyclic chain node PKG10 must not be in any cycle"
        );
    }
}
