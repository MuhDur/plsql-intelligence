use std::io::Write;
use std::process::Command;

use plsql_core::{Confidence, ConfidenceLevel, Evidence, FileId, Position, Span};
use plsql_depgraph::{
    DepGraph, Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind,
    ObjectRevisionId, Provenance, QualifiedName, ResolutionStrategy,
};
use serde_json::Value;
use tempfile::NamedTempFile;

fn sample_graph() -> DepGraph {
    let mut graph = DepGraph::new();

    graph.insert_node(Node::new(
        NodeId::new(1),
        LogicalObjectId::new("billing.claims_pkg.calculate/1"),
        ObjectRevisionId::new("sha256:pkg"),
        QualifiedName::new(
            None,
            plsql_core::ObjectName::from(plsql_core::SymbolId::new(10)),
        ),
        NodeIdentityKind::PackageProcedure,
    ));
    graph.insert_node(Node::new(
        NodeId::new(2),
        LogicalObjectId::new("billing.claims"),
        ObjectRevisionId::new("sha256:claims"),
        QualifiedName::new(
            None,
            plsql_core::ObjectName::from(plsql_core::SymbolId::new(11)),
        ),
        NodeIdentityKind::Table,
    ));
    graph.insert_node(Node::new(
        NodeId::new(3),
        LogicalObjectId::new("billing.claim_audit"),
        ObjectRevisionId::new("sha256:audit"),
        QualifiedName::new(
            None,
            plsql_core::ObjectName::from(plsql_core::SymbolId::new(12)),
        ),
        NodeIdentityKind::Table,
    ));

    graph.insert_edge(
        Edge::new(
            EdgeId::new(1),
            NodeId::new(1),
            NodeId::new(2),
            EdgeKind::Reads,
            Confidence::new(ConfidenceLevel::High, None),
        ),
        Provenance::new(
            FileId::new(1),
            Span::new(
                FileId::new(1),
                Position::new(1, 1, 0),
                Position::new(1, 10, 9),
            ),
            ResolutionStrategy::CatalogLookup,
        ),
        None,
    );
    graph.insert_edge(
        Edge::new(
            EdgeId::new(2),
            NodeId::new(2),
            NodeId::new(3),
            EdgeKind::Writes,
            Confidence::new(
                ConfidenceLevel::Medium,
                Some(String::from("refresh target inferred from metadata")),
            ),
        ),
        Provenance::new(
            FileId::new(1),
            Span::new(
                FileId::new(1),
                Position::new(2, 1, 10),
                Position::new(2, 10, 19),
            ),
            ResolutionStrategy::CatalogLookup,
        ),
        Some(Evidence::new(
            "DEP003",
            "refresh target confirmed from catalog",
        )),
    );
    graph.insert_edge(
        Edge::new(
            EdgeId::new(3),
            NodeId::new(3),
            NodeId::new(2),
            EdgeKind::Reads,
            Confidence::new(ConfidenceLevel::High, None),
        ),
        Provenance::new(
            FileId::new(1),
            Span::new(
                FileId::new(1),
                Position::new(3, 1, 20),
                Position::new(3, 10, 29),
            ),
            ResolutionStrategy::CatalogLookup,
        ),
        None,
    );

    graph
}

fn write_graph() -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp graph file should be created");
    let graph = sample_graph();
    let encoded = serde_json::to_string_pretty(&graph).expect("graph should serialize");
    file.write_all(encoded.as_bytes())
        .expect("graph should be written");
    file
}

#[test]
fn neighbors_query_robot_json_is_versioned_and_deterministic() {
    let graph_file = write_graph();
    let output = Command::new(env!("CARGO_BIN_EXE_plsql-depgraph"))
        .args([
            "--graph",
            graph_file.path().to_str().expect("utf-8 path"),
            "--robot-json",
            "query",
            "neighbors",
            "--logical-id",
            "billing.claims_pkg.calculate/1",
        ])
        .output()
        .expect("cli should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(
        parsed["schema_id"],
        Value::String(String::from("plsql.depgraph.query"))
    );
    assert_eq!(
        parsed["payload"]["operation"],
        Value::String(String::from("neighbors"))
    );
    assert_eq!(parsed["payload"]["edges"][0]["id"], Value::from(1));
}

#[test]
fn path_query_human_output_reports_directed_chain() {
    let graph_file = write_graph();
    let output = Command::new(env!("CARGO_BIN_EXE_plsql-depgraph"))
        .args([
            "--graph",
            graph_file.path().to_str().expect("utf-8 path"),
            "query",
            "path",
            "--from-logical-id",
            "billing.claims_pkg.calculate/1",
            "--to-logical-id",
            "billing.claim_audit",
        ])
        .output()
        .expect("cli should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");

    assert!(
        stdout.contains("Directed path from billing.claims_pkg.calculate/1 to billing.claim_audit")
    );
    assert!(stdout.contains("[1] Reads billing.claims_pkg.calculate/1 -> billing.claims"));
    assert!(stdout.contains("[2] Writes billing.claims -> billing.claim_audit"));
}

#[test]
fn doctor_robot_json_reports_low_confidence_edge_inventory() {
    let graph_file = write_graph();
    let output = Command::new(env!("CARGO_BIN_EXE_plsql-depgraph"))
        .args([
            "--graph",
            graph_file.path().to_str().expect("utf-8 path"),
            "--robot-json",
            "doctor",
        ])
        .output()
        .expect("cli should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(
        parsed["schema_id"],
        Value::String(String::from("plsql.depgraph.doctor"))
    );
    assert_eq!(parsed["payload"]["node_count"], Value::from(3));
    assert_eq!(parsed["payload"]["cycle_count"], Value::from(1));
    assert_eq!(
        parsed["payload"]["low_confidence_edges"][0]["id"],
        Value::from(2)
    );
}
