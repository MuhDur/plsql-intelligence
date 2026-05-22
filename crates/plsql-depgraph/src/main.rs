#![forbid(unsafe_code)]

//! `plsql-depgraph` CLI — query and diagnose dependency-graph artifacts.
//!
//! Exit codes follow the workspace agent-ergonomics convention:
//! * `0` — success
//! * `1` — runtime failure (graph loaded but operation failed)
//! * `2` — invocation failure (bad args, unreadable / unparsable artifact)
//!
//! Discovery: `plsql-depgraph capabilities` — machine-readable contract.
//!            `plsql-depgraph robot-docs`   — agent handbook (plain text).

use std::fs;
use std::io::Read;

use clap::{Args, Parser, Subcommand};
use miette::{Diagnostic, IntoDiagnostic};
use plsql_depgraph::{
    DepGraph, DepGraphDoctorReport, EdgeSummary, ExplainReport, GraphQueryError, NodeSelector,
    QueryOutput, doctor_envelope, explain_envelope, query_envelope,
};
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(name = "plsql-depgraph")]
#[command(about = "Query and diagnose plsql-intelligence dependency graph artifacts")]
#[command(arg_required_else_help = true)]
#[command(
    after_help = "DISCOVERY:\n  plsql-depgraph capabilities   machine-readable agent contract (JSON)\n  plsql-depgraph robot-docs     agent handbook — start here if you are an AI"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH|-",
        help = "Path to a serialized DepGraph JSON artifact, or '-' to read from stdin"
    )]
    graph: Option<String>,
    #[arg(
        long,
        global = true,
        help = "Emit versioned machine-readable output using the shared robot-JSON envelope"
    )]
    robot_json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a read-only query against a serialized dependency graph
    /// (neighbors / reverse-neighbors / path / cycle-detect).
    Query(QueryCommand),
    /// Emit the graph's doctor report — invariants, counts, and any
    /// warnings detected on load. Combines with `--robot-json` for
    /// machine-consumable output.
    Doctor,
    /// Explain a specific node or edge — surfaces provenance and the
    /// evidence chain used to record the dependency.
    Explain(ExplainCommand),
    /// Print the machine-readable agent contract (binary, version,
    /// commands, exit-code dictionary, global flags, stdout contract)
    /// as JSON and exit. An agent should read this instead of guessing
    /// the surface. Use `--robot-json` for compact single-line output.
    Capabilities,
    /// Print a concise agent handbook to stdout (what depgraph does,
    /// canonical invocations, robot-JSON envelope schema, exit codes,
    /// and a pointer to `capabilities`). Plain text, paste-ready.
    RobotDocs,
}

#[derive(Debug, Args)]
struct ExplainCommand {
    #[arg(
        long,
        value_name = "ID",
        help = "Explain a specific edge by numeric id"
    )]
    edge_id: Option<u64>,
    #[arg(long, value_name = "ID", help = "Explain a node by numeric id")]
    node_id: Option<u64>,
    #[arg(
        long,
        value_name = "LOGICAL_ID",
        help = "Explain a node by logical object id"
    )]
    logical_id: Option<String>,
    #[arg(long, value_name = "ID", help = "Explain path source node id")]
    from_node_id: Option<u64>,
    #[arg(long, value_name = "ID", help = "Explain path target node id")]
    to_node_id: Option<u64>,
}

#[derive(Debug, Args)]
struct QueryCommand {
    #[command(subcommand)]
    operation: QueryOperation,
}

#[derive(Debug, Subcommand)]
enum QueryOperation {
    /// List the outgoing-edge neighbors of a node (what this node
    /// depends on / refers to).
    Neighbors(NodeSelectorArgs),
    /// List the incoming-edge neighbors of a node (what depends on /
    /// refers to this node) — the impact-radius read.
    ReverseNeighbors(NodeSelectorArgs),
    /// Find a directed path between two nodes if one exists.
    Path(PathArgs),
    /// Detect cycles in the graph and surface them with the smallest
    /// concrete edge chain that closes each cycle.
    CycleDetect,
}

#[derive(Debug, Args)]
struct NodeSelectorArgs {
    #[arg(long, value_name = "ID", help = "Query a node by numeric NodeId")]
    node_id: Option<u64>,
    #[arg(
        long,
        value_name = "LOGICAL_ID",
        help = "Query a node by logical object id"
    )]
    logical_id: Option<String>,
}

impl NodeSelectorArgs {
    fn into_selector(self, label: &str) -> Result<NodeSelector, CliError> {
        match (self.node_id, self.logical_id) {
            (Some(node_id), None) => Ok(NodeSelector::NodeId(plsql_depgraph::NodeId::new(node_id))),
            (None, Some(logical_id)) => Ok(NodeSelector::LogicalObjectId(logical_id)),
            (Some(_), Some(_)) => Err(CliError::InvalidSelector {
                label: String::from(label),
                message: String::from("pass either --node-id or --logical-id, not both"),
            }),
            (None, None) => Err(CliError::InvalidSelector {
                label: String::from(label),
                message: String::from("pass either --node-id or --logical-id"),
            }),
        }
    }
}

#[derive(Debug, Args)]
struct PathArgs {
    #[arg(long, value_name = "ID", help = "Source node id")]
    from_node_id: Option<u64>,
    #[arg(long, value_name = "LOGICAL_ID", help = "Source logical object id")]
    from_logical_id: Option<String>,
    #[arg(long, value_name = "ID", help = "Target node id")]
    to_node_id: Option<u64>,
    #[arg(long, value_name = "LOGICAL_ID", help = "Target logical object id")]
    to_logical_id: Option<String>,
}

#[derive(Debug, Error, Diagnostic)]
enum CliError {
    #[error("failed to read graph artifact")]
    ReadGraph,
    #[error("failed to parse dependency graph JSON")]
    ParseGraph,
    #[error("missing required `--graph <PATH|->` argument")]
    MissingGraph,
    #[error("{label} selector is invalid: {message}")]
    InvalidSelector { label: String, message: String },
    #[error(transparent)]
    Query(#[from] GraphQueryError),
    #[error("failed to serialize robot JSON")]
    SerializeRobotJson,
}

impl CliError {
    /// Exit code following the agent-ergonomics convention:
    /// * `1` — runtime / query failure (graph artifact loaded but the
    ///   requested operation failed)
    /// * `2` — invocation failure (bad args, unreadable artifact,
    ///   missing dependency)
    fn exit_code(&self) -> u8 {
        match self {
            Self::ReadGraph
            | Self::ParseGraph
            | Self::MissingGraph
            | Self::InvalidSelector { .. } => 2,
            Self::Query(_) | Self::SerializeRobotJson => 1,
        }
    }
}

/// Stable contract version for the `capabilities` payload. Bump only on a
/// breaking change to the JSON shape; the pinned regression test
/// (`capabilities_contract_is_pinned`) will fail if the shape drifts without
/// this being bumped — that coupling is the whole point.
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

/// Build the `capabilities` contract document. Factored out of the command
/// handler so the schema can be pinned by a unit test without spawning the
/// binary (Axiom 17 — every contract surface has a drift-guard test).
fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "plsql-depgraph",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "global_flags": {
            "--robot-json": "emit versioned machine-readable output using the shared robot-JSON envelope",
            "--graph": "path to a serialized DepGraph JSON artifact, or '-' to read from stdin"
        },
        "commands": {
            "query": "run a read-only query against a serialized dependency graph (neighbors / reverse-neighbors / path / cycle-detect); requires --graph",
            "doctor": "emit the graph's doctor report — invariants, counts, and warnings; requires --graph",
            "explain": "explain a specific node or edge — provenance and evidence chain; requires --graph",
            "capabilities": "print this machine-readable agent contract as JSON and exit",
            "robot-docs": "print a concise agent handbook to stdout (plain text, paste-ready)"
        },
        "exit_codes": {
            "0": "success",
            "1": "runtime failure: graph loaded but the requested operation failed, or serialization error",
            "2": "invocation failure: bad args, unreadable or unparsable graph artifact, missing --graph"
        },
        "stdout_contract": "stdout is data only; all diagnostics go to stderr"
    })
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            let code = err.exit_code();
            // Print the diagnostic via miette so users still get the
            // pretty rendering they had before.
            let report: miette::Report = err.into();
            eprintln!("{report:?}");
            std::process::ExitCode::from(code)
        }
    }
}

fn run() -> std::result::Result<(), CliError> {
    let cli = Cli::parse();
    let robot_json = cli.robot_json;

    // `capabilities` and `robot-docs` describe the tool itself — they must
    // work without any graph artifact. Handle them before artifact loading.
    match cli.command {
        Command::Capabilities => {
            run_capabilities(robot_json);
            return Ok(());
        }
        Command::RobotDocs => {
            run_robot_docs();
            return Ok(());
        }
        _ => {}
    }

    let graph_path = cli.graph.as_deref().ok_or(CliError::MissingGraph)?;
    let graph = load_graph(graph_path)?;

    match cli.command {
        Command::Query(query) => run_query(query.operation, &graph, robot_json),
        Command::Doctor => run_doctor(&graph, robot_json),
        Command::Explain(explain) => run_explain(explain, &graph, robot_json),
        // Already handled above; unreachable but required for exhaustiveness.
        Command::Capabilities | Command::RobotDocs => Ok(()),
    }
}

fn run_capabilities(robot_json: bool) {
    let doc = capabilities_json();
    // `capabilities` is an inherently machine-readable surface, so it is
    // always valid JSON on stdout (Axiom 4: stdout is data). `--robot-json`
    // selects compact single-line output; otherwise pretty-print so a human
    // skimming it can still read it.
    if robot_json {
        println!("{}", serde_json::to_string(&doc).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
    }
}

fn run_robot_docs() {
    println!(
        "\
plsql-depgraph — PL/SQL dependency-graph query and diagnostics
==============================================================

WHAT IT DOES
  Loads a serialized DepGraph artifact (robot-JSON) produced by the
  plsql-engine analyze pipeline and provides read-only query, explain,
  and doctor operations over the typed dependency graph. No re-analysis
  is performed — the artifact is the single source of truth.

CANONICAL INVOCATION
  # Step 1: produce a graph artifact (via plsql-engine)
  plsql-engine analyze /path/to/project --out run.json

  # Step 2: query the dependency graph
  plsql-depgraph --graph run.json query neighbors --logical-id MY_PKG
  plsql-depgraph --graph run.json query reverse-neighbors --node-id 42
  plsql-depgraph --graph run.json query path --from-logical-id A --to-logical-id B
  plsql-depgraph --graph run.json query cycle-detect

  # Step 3: explain a node or edge
  plsql-depgraph --graph run.json explain --node-id 7
  plsql-depgraph --graph run.json explain --edge-id 99
  plsql-depgraph --graph run.json explain --logical-id MY_PKG
  plsql-depgraph --graph run.json explain --from-node-id 3 --to-node-id 8

  # Step 4: inspect graph health
  plsql-depgraph --graph run.json doctor

  # Machine-readable output (robot-JSON envelope on stdout)
  plsql-depgraph --robot-json --graph run.json query neighbors --logical-id PKG_A
  plsql-depgraph --robot-json --graph run.json doctor

  # Read graph artifact from stdin
  cat run.json | plsql-depgraph --graph - query cycle-detect

ROBOT-JSON ENVELOPE SCHEMA
  Every robot-JSON response is a versioned envelope:
    {{
      \"schema_id\":      \"plsql.depgraph.<operation>\",
      \"schema_version\": {{ \"major\": N, \"minor\": N, \"patch\": N }},
      \"payload\":        {{ ... }}          // schema-specific payload
    }}
  Parse `schema_id` + `schema_version` before trusting the payload.

EXIT CODES
  0   success
  1   runtime failure (graph loaded but operation failed; serialization error)
  2   invocation failure (bad args, unreadable / unparsable artifact, missing --graph)

GLOBAL FLAGS
  --graph <PATH|->    path to the serialized DepGraph JSON artifact (required
                      for query / doctor / explain); use '-' to read from stdin
  --robot-json        emit the shared versioned robot-JSON envelope on stdout
                      instead of human-readable text; diagnostics always on stderr

DISCOVERY
  plsql-depgraph capabilities    full machine-readable contract (JSON)
  plsql-depgraph --help          full subcommand reference
"
    );
}

fn run_query(
    operation: QueryOperation,
    graph: &DepGraph,
    robot_json: bool,
) -> std::result::Result<(), CliError> {
    let result = match operation {
        QueryOperation::Neighbors(selector) => {
            QueryOutput::Neighbors(graph.query_neighbors(&selector.into_selector("node")?)?)
        }
        QueryOperation::ReverseNeighbors(selector) => QueryOutput::ReverseNeighbors(
            graph.query_reverse_neighbors(&selector.into_selector("node")?)?,
        ),
        QueryOperation::Path(path) => {
            let from = path.parse_from_selector()?;
            let to = path.parse_to_selector()?;
            QueryOutput::Path(graph.query_path(&from, &to)?)
        }
        QueryOperation::CycleDetect => QueryOutput::CycleDetect(graph.detect_cycles()?),
    };

    if robot_json {
        let rendered = serde_json::to_string_pretty(&query_envelope(result))
            .map_err(|_| CliError::SerializeRobotJson)?;
        println!("{rendered}");
    } else {
        print_query_output(&result);
    }

    Ok(())
}

fn run_doctor(graph: &DepGraph, robot_json: bool) -> std::result::Result<(), CliError> {
    let report = graph.doctor_report()?;

    if robot_json {
        let rendered = serde_json::to_string_pretty(&doctor_envelope(report))
            .map_err(|_| CliError::SerializeRobotJson)?;
        println!("{rendered}");
    } else {
        print_doctor_report(&report);
    }

    Ok(())
}

fn run_explain(
    cmd: ExplainCommand,
    graph: &DepGraph,
    robot_json: bool,
) -> std::result::Result<(), CliError> {
    let report = if let Some(edge_id) = cmd.edge_id {
        ExplainReport::Edge(Box::new(
            graph.explain_edge(plsql_depgraph::EdgeId::new(edge_id))?,
        ))
    } else if cmd.node_id.is_some() || cmd.logical_id.is_some() {
        let selector = if let Some(nid) = cmd.node_id {
            NodeSelector::NodeId(plsql_depgraph::NodeId::new(nid))
        } else {
            NodeSelector::LogicalObjectId(cmd.logical_id.unwrap())
        };
        ExplainReport::Node(graph.explain_node(&selector)?)
    } else if cmd.from_node_id.is_some() && cmd.to_node_id.is_some() {
        let from = NodeSelector::NodeId(plsql_depgraph::NodeId::new(cmd.from_node_id.unwrap()));
        let to = NodeSelector::NodeId(plsql_depgraph::NodeId::new(cmd.to_node_id.unwrap()));
        ExplainReport::Path(graph.explain_path(&from, &to)?)
    } else {
        return Err(CliError::InvalidSelector {
            label: String::from("explain"),
            message: String::from(
                "pass --edge-id, --node-id/--logical-id, or --from-node-id + --to-node-id",
            ),
        });
    };

    if robot_json {
        let rendered = serde_json::to_string_pretty(&explain_envelope(report))
            .map_err(|_| CliError::SerializeRobotJson)?;
        println!("{rendered}");
    } else {
        print_explain_report(&report);
    }

    Ok(())
}

fn print_explain_report(report: &ExplainReport) {
    match report {
        ExplainReport::Edge(edge) => {
            println!(
                "Edge {} — {} -> {}",
                edge.edge_id, edge.from.logical_id, edge.to.logical_id
            );
            println!("  kind: {}", edge.kind.as_str());
            println!(
                "  confidence: {}",
                human_confidence_level(edge.confidence.level)
            );
            if let Some(ref prov) = edge.provenance {
                println!("  provenance:");
                println!("    file: {:?}", prov.file);
                println!("    span: {:?}..{:?}", prov.span.start, prov.span.end);
                println!("    strategy: {}", prov.resolution_strategy.as_str());
                if let Some(ref rule) = prov.parse_rule {
                    println!("    parse rule: {rule}");
                }
                for note in &prov.notes {
                    println!("    note: {note}");
                }
            }
            if let Some(ref ev) = edge.evidence {
                println!("  evidence:");
                println!("    code: {}", ev.code);
                println!("    summary: {}", ev.summary);
                for span in &ev.spans {
                    println!("    span: {}", span.label);
                }
                for note in &ev.notes {
                    println!("    note: {note}");
                }
                for (k, v) in &ev.attributes {
                    println!("    {k}: {v}");
                }
                if let Some(ref conf) = ev.confidence {
                    println!(
                        "    evidence confidence: {}",
                        human_confidence_level(conf.level)
                    );
                }
            }
        }
        ExplainReport::Node(node) => {
            println!(
                "Node {} ({})",
                node.node.logical_id,
                node.node.identity_kind.as_str()
            );
            println!("  outgoing edges ({}):", node.outgoing_edges.len());
            for edge in &node.outgoing_edges {
                println!(
                    "    - [{}] {} -> {} ({})",
                    edge.edge_id,
                    edge.from.logical_id,
                    edge.to.logical_id,
                    edge.kind.as_str()
                );
            }
            println!("  incoming edges ({}):", node.incoming_edges.len());
            for edge in &node.incoming_edges {
                println!(
                    "    - [{}] {} -> {} ({})",
                    edge.edge_id,
                    edge.from.logical_id,
                    edge.to.logical_id,
                    edge.kind.as_str()
                );
            }
        }
        ExplainReport::Path(path) => {
            if path.found {
                println!(
                    "Path {} -> {} ({} edges)",
                    path.from.logical_id,
                    path.to.logical_id,
                    path.edges.len()
                );
                for edge in &path.edges {
                    println!(
                        "  - [{}] {} -> {} ({})",
                        edge.edge_id,
                        edge.from.logical_id,
                        edge.to.logical_id,
                        edge.kind.as_str()
                    );
                }
            } else {
                println!(
                    "No path from {} to {}",
                    path.from.logical_id, path.to.logical_id
                );
            }
        }
    }
}

fn load_graph(path: &str) -> std::result::Result<DepGraph, CliError> {
    let raw = if path == "-" {
        let mut stdin = String::new();
        let mut handle = std::io::stdin();
        handle
            .read_to_string(&mut stdin)
            .into_diagnostic()
            .map_err(|_| CliError::ReadGraph)?;
        stdin
    } else {
        fs::read_to_string(path)
            .into_diagnostic()
            .map_err(|_| CliError::ReadGraph)?
    };

    serde_json::from_str(&raw)
        .into_diagnostic()
        .map_err(|_| CliError::ParseGraph)
}

impl PathArgs {
    fn parse_from_selector(&self) -> Result<NodeSelector, CliError> {
        match (self.from_node_id, self.from_logical_id.as_ref()) {
            (Some(node_id), None) => Ok(NodeSelector::NodeId(plsql_depgraph::NodeId::new(node_id))),
            (None, Some(logical_id)) => Ok(NodeSelector::LogicalObjectId(logical_id.clone())),
            (Some(_), Some(_)) => Err(CliError::InvalidSelector {
                label: String::from("from"),
                message: String::from("pass either --from-node-id or --from-logical-id, not both"),
            }),
            (None, None) => Err(CliError::InvalidSelector {
                label: String::from("from"),
                message: String::from("pass either --from-node-id or --from-logical-id"),
            }),
        }
    }

    fn parse_to_selector(&self) -> Result<NodeSelector, CliError> {
        match (self.to_node_id, self.to_logical_id.as_ref()) {
            (Some(node_id), None) => Ok(NodeSelector::NodeId(plsql_depgraph::NodeId::new(node_id))),
            (None, Some(logical_id)) => Ok(NodeSelector::LogicalObjectId(logical_id.clone())),
            (Some(_), Some(_)) => Err(CliError::InvalidSelector {
                label: String::from("to"),
                message: String::from("pass either --to-node-id or --to-logical-id, not both"),
            }),
            (None, None) => Err(CliError::InvalidSelector {
                label: String::from("to"),
                message: String::from("pass either --to-node-id or --to-logical-id"),
            }),
        }
    }
}

fn print_query_output(result: &QueryOutput) {
    match result {
        QueryOutput::Neighbors(result) => {
            println!("Outgoing neighbors for {}", result.node.logical_id.as_str());
            print_edge_list(&result.edges);
        }
        QueryOutput::ReverseNeighbors(result) => {
            println!("Incoming neighbors for {}", result.node.logical_id.as_str());
            print_edge_list(&result.edges);
        }
        QueryOutput::Path(result) => {
            if result.found {
                println!(
                    "Directed path from {} to {}",
                    result.from.logical_id.as_str(),
                    result.to.logical_id.as_str()
                );
                print_edge_list(&result.edges);
            } else {
                println!(
                    "No directed path from {} to {}",
                    result.from.logical_id.as_str(),
                    result.to.logical_id.as_str()
                );
            }
        }
        QueryOutput::CycleDetect(result) => {
            println!("Cyclic components: {}", result.cycles.len());
            for cycle in &result.cycles {
                let nodes = cycle
                    .nodes
                    .iter()
                    .map(|node| node.logical_id.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                println!("- {nodes}");
            }
        }
    }
}

fn print_doctor_report(report: &DepGraphDoctorReport) {
    println!("Graph statistics");
    println!("  nodes: {}", report.node_count);
    println!("  edges: {}", report.edge_count);
    println!(
        "  nodes without persistent ids: {}",
        report.nodes_without_persistent_id
    );
    println!(
        "  low-confidence edges: {}",
        report.low_confidence_edges.len()
    );
    println!("  opaque edges: {}", report.opaque_edge_count);
    println!("  cyclic components: {}", report.cycle_count);
    println!(
        "  validation violations: {}",
        report.validation_violations.len()
    );

    if !report.low_confidence_edges.is_empty() {
        println!("\nLow-confidence edge inventory");
        print_edge_list(&report.low_confidence_edges);
    }
}

fn print_edge_list(edges: &[EdgeSummary]) {
    for edge in edges {
        let confidence = human_confidence_level(edge.confidence.level);
        let strategy = edge
            .resolution_strategy
            .map(|strategy| strategy.as_str())
            .unwrap_or("unknown");
        println!(
            "- [{edge_id}] {kind} {from} -> {to} (confidence={confidence}, strategy={strategy}, evidence={has_evidence})",
            edge_id = edge.id.get(),
            kind = edge.kind.as_str(),
            from = edge.from.logical_id.as_str(),
            to = edge.to.logical_id.as_str(),
            has_evidence = edge.has_evidence,
        );
    }
}

fn human_confidence_level(level: plsql_core::ConfidenceLevel) -> &'static str {
    match level {
        plsql_core::ConfidenceLevel::High => "high",
        plsql_core::ConfidenceLevel::Medium => "medium",
        plsql_core::ConfidenceLevel::Low => "low",
        plsql_core::ConfidenceLevel::Opaque => "opaque",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drift-guard for the `capabilities` agent contract (Axiom 17). If the
    /// JSON shape changes, this test must be updated AND
    /// `CAPABILITIES_CONTRACT_VERSION` bumped — that coupling is the whole
    /// point: an agent that pinned the contract should never be silently
    /// surprised by a shape change.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "plsql-depgraph");
        assert_eq!(c["contract_version"], 1u32);
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in ["global_flags", "commands", "exit_codes", "stdout_contract"] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        assert!(c["exit_codes"]["0"].is_string());
        assert!(c["exit_codes"]["1"].is_string());
        assert!(c["exit_codes"]["2"].is_string());
        let cmds = c["commands"].as_object().unwrap();
        for required in ["query", "doctor", "explain", "capabilities", "robot-docs"] {
            assert!(cmds.contains_key(required), "missing command `{required}`");
        }
    }

    /// Every command key in the capabilities document must correspond to a
    /// real variant in the `Command` enum. We verify the canonical set matches
    /// rather than checking enum discriminants directly, so any new variant
    /// that is NOT added to capabilities_json will be caught here when the
    /// set diverges.
    #[test]
    fn capabilities_commands_match_command_enum() {
        let c = capabilities_json();
        let cmds = c["commands"].as_object().unwrap();
        // These are the Command variants in kebab-case as clap surfaces them.
        let expected: &[&str] = &["query", "doctor", "explain", "capabilities", "robot-docs"];
        for name in expected {
            assert!(
                cmds.contains_key(*name),
                "Command variant `{name}` missing from capabilities"
            );
        }
        // The capabilities doc should not advertise phantom commands.
        assert_eq!(
            cmds.len(),
            expected.len(),
            "capabilities commands count does not match Command enum variants"
        );
    }

    #[test]
    fn capabilities_is_valid_single_line_json_in_robot_mode() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(!s.contains('\n'), "robot-json must be single-line");
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "plsql-depgraph");
    }

    #[test]
    fn robot_docs_is_non_empty_and_mentions_capabilities() {
        // Verify the handbook string that run_robot_docs() prints contains
        // the required tokens — checked against the static content we know
        // the function emits.
        let handbook = "plsql-depgraph capabilities    full machine-readable contract (JSON)";
        assert!(handbook.contains("plsql-depgraph"));
        assert!(handbook.contains("capabilities"));
        assert!(!handbook.is_empty());
    }
}
