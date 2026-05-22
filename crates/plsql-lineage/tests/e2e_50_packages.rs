//! End-to-end test (`PLSQL-LIN-012`).
//!
//! Builds a synthetic DepGraph that mimics a 50-object schema and
//! asserts `impact(table) ⊇ expected_set` for a handful of seed
//! tables. The graph shape is intentionally well-known so a
//! regression in `impact()` shows up as a missing expected dependent.
//!
//! Shape — 50 nodes, ~75 edges:
//!
//! - 10 base tables `T00…T09`.
//! - 20 packages `PKG00…PKG19`, each reading 1-3 tables and writing
//!   to 0-1 tables. `PKG00`..`PKG09` form a call-chain.
//! - 10 views `V00…V09`, each projecting from one table + optionally
//!   calling a package function.
//! - 5 triggers `TRG00…TRG04`, each fires on one table.
//! - 5 sequences `SEQ00…SEQ04`, each owned by one schema.
//!
//! For each seed (table T00..T09), the bead's contract is that
//! `impact(table)` is a *superset* of the hand-rolled expected set
//! (every direct + transitive dependent the engine should reach).

use plsql_core::{
    Confidence, ConfidenceLevel, FileId, ObjectName, Position, SchemaName, Span, SymbolInterner,
};
use plsql_depgraph::{
    DepGraph, Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind,
    ObjectRevisionId, Provenance, QualifiedName,
};
use plsql_lineage::impact;

const SCHEMA: &str = "BILLING";

struct TestFixture {
    graph: DepGraph,
    nodes_by_name: std::collections::BTreeMap<String, NodeId>,
}

fn build_fixture() -> TestFixture {
    let mut interner = SymbolInterner::new();
    let schema_name = SchemaName::from(
        interner
            .intern_schema_name(SCHEMA)
            .expect("schema interns")
            .symbol(),
    );
    let mut graph = DepGraph::new();
    let mut nodes_by_name = std::collections::BTreeMap::new();
    let mut next_node_id: u64 = 1;
    let mut next_edge_id: u64 = 1;

    let mut mk_node = |graph: &mut DepGraph,
                       nodes: &mut std::collections::BTreeMap<String, NodeId>,
                       interner: &mut SymbolInterner,
                       name: &str,
                       kind: NodeIdentityKind| {
        let node_id = NodeId::new(next_node_id);
        next_node_id += 1;
        let object_sym = interner.intern(name).expect("object name interns");
        let object_name = ObjectName::from(object_sym);
        let qname = QualifiedName::new(Some(schema_name), object_name);
        let node = Node::new(
            node_id,
            LogicalObjectId::new(format!("{SCHEMA}.{name}")),
            ObjectRevisionId::new("rev1"),
            qname,
            kind,
        );
        graph.insert_node(node);
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
        let provenance = Provenance::new(
            FileId::new(0),
            span,
            plsql_depgraph::ResolutionStrategy::CatalogLookup,
        );
        graph.insert_edge(edge, provenance, None);
    };

    // 10 tables.
    let tables: Vec<NodeId> = (0..10)
        .map(|i| {
            mk_node(
                &mut graph,
                &mut nodes_by_name,
                &mut interner,
                &format!("T{i:02}"),
                NodeIdentityKind::PackageBody,
            )
        })
        .collect();

    // 20 packages, each reads 1-3 tables. PKG00..PKG09 form a chain
    // where PKG(N+1).calls(PKG(N)).
    let mut packages: Vec<NodeId> = Vec::new();
    for i in 0..20 {
        let pkg = mk_node(
            &mut graph,
            &mut nodes_by_name,
            &mut interner,
            &format!("PKG{i:02}"),
            NodeIdentityKind::PackageBody,
        );
        packages.push(pkg);
        // Read 1-3 tables; the table index is (i + offset) mod 10.
        // Edge direction is `tableX → pkg` ("change in tableX impacts pkg").
        let reads_count = (i % 3) + 1;
        for r in 0..reads_count {
            let table_idx = (i + r) % 10;
            mk_edge(&mut graph, tables[table_idx], pkg, EdgeKind::Reads);
        }
        // PKG_even writes to table i%10 → pkg → table.
        if i % 2 == 0 {
            mk_edge(&mut graph, pkg, tables[i % 10], EdgeKind::Writes);
        }
        // Call chain: PKG(i-1) → PKG(i) ("change in i-1 impacts i").
        if (1..10).contains(&i) {
            mk_edge(&mut graph, packages[i - 1], pkg, EdgeKind::Calls);
        }
    }

    // 10 views, each projects from one table.
    // Edge direction is `table → view` ("change in table impacts view").
    for (i, table) in tables.iter().enumerate().take(10) {
        let view = mk_node(
            &mut graph,
            &mut nodes_by_name,
            &mut interner,
            &format!("V{i:02}"),
            NodeIdentityKind::PackageBody,
        );
        mk_edge(&mut graph, *table, view, EdgeKind::Reads);
    }

    // 5 triggers, each fires on one table.
    // Edge direction: `table → trigger` ("change in table impacts trigger").
    for (i, table) in tables.iter().enumerate().take(5) {
        let trg = mk_node(
            &mut graph,
            &mut nodes_by_name,
            &mut interner,
            &format!("TRG{i:02}"),
            NodeIdentityKind::PackageBody,
        );
        mk_edge(&mut graph, *table, trg, EdgeKind::TriggersOn);
    }

    // 5 sequences (standalone nodes — no incoming edges).
    for i in 0..5 {
        mk_node(
            &mut graph,
            &mut nodes_by_name,
            &mut interner,
            &format!("SEQ{i:02}"),
            NodeIdentityKind::PackageBody,
        );
    }

    TestFixture {
        graph,
        nodes_by_name,
    }
}

fn impacts_of(fixture: &TestFixture, seed: &str) -> std::collections::BTreeSet<String> {
    let seed_id = *fixture
        .nodes_by_name
        .get(seed)
        .unwrap_or_else(|| panic!("missing seed node `{seed}` in fixture"));
    let result = impact(&fixture.graph, &seed_id, None);
    result
        .affected_nodes
        .iter()
        .map(|n| n.logical_id.clone())
        .filter_map(|id| id.strip_prefix(&format!("{SCHEMA}.")).map(str::to_string))
        .collect()
}

#[test]
fn fixture_has_50_nodes() {
    let fixture = build_fixture();
    assert_eq!(fixture.graph.node_count(), 50);
}

#[test]
fn impact_of_t00_is_superset_of_direct_dependents() {
    let fixture = build_fixture();
    let reached = impacts_of(&fixture, "T00");
    // V00 reads T00 directly.
    assert!(
        reached.contains("V00"),
        "impact must reach V00; got {reached:?}"
    );
    // TRG00 fires on T00 directly.
    assert!(
        reached.contains("TRG00"),
        "impact must reach TRG00; got {reached:?}"
    );
}

#[test]
fn impact_of_t05_reaches_call_chain_transitively() {
    let fixture = build_fixture();
    let reached = impacts_of(&fixture, "T05");
    // PKG05 reads T05; PKG06 calls PKG05; PKG07 calls PKG06; etc.
    // Every PKG{05..09} should be reached.
    for i in 5..10 {
        let pkg = format!("PKG{i:02}");
        assert!(
            reached.contains(&pkg),
            "impact(T05) must reach {pkg} via call-chain; got {reached:?}"
        );
    }
}

#[test]
fn impact_emits_no_unknown_edges_on_well_formed_synthetic_graph() {
    let fixture = build_fixture();
    let seed_id = *fixture.nodes_by_name.get("T00").unwrap();
    let result = impact(&fixture.graph, &seed_id, None);
    assert!(
        result.unknown_edges.is_empty(),
        "synthetic graph has no opaque edges; got {:?}",
        result.unknown_edges
    );
}

#[test]
fn impact_orphan_sequences_yield_empty_reach() {
    let fixture = build_fixture();
    // Sequences have no incoming edges in the fixture.
    let reached = impacts_of(&fixture, "SEQ00");
    assert!(
        reached.is_empty(),
        "SEQ00 has no dependents; impact must be empty; got {reached:?}"
    );
}

#[test]
fn impact_max_depth_one_excludes_transitive() {
    let fixture = build_fixture();
    let seed_id = *fixture.nodes_by_name.get("T05").unwrap();
    let result = impact(&fixture.graph, &seed_id, Some(1));
    let reached: std::collections::BTreeSet<String> = result
        .affected_nodes
        .iter()
        .map(|n| n.logical_id.clone())
        .filter_map(|id| id.strip_prefix(&format!("{SCHEMA}.")).map(str::to_string))
        .collect();
    // Direct dependents (PKG05, V05, TRG05) should appear; transitive
    // ones (PKG06..09 via call chain) should NOT.
    assert!(reached.contains("V05"));
    for i in 6..10 {
        let pkg = format!("PKG{i:02}");
        assert!(
            !reached.contains(&pkg),
            "max_depth=1 must NOT reach transitive {pkg}; got {reached:?}"
        );
    }
}
