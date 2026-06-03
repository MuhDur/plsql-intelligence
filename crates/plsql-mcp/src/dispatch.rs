//! Tool dispatch for `tools/call`.
//!
//! [`mcp_protocol::handle_tools_call`](crate::mcp_protocol) used to
//! return one static "registered but execution gated" placeholder
//! for *every* tool — none of the ~30 `run_*` implementations were
//! reachable over the JSON-RPC wire. This module is the missing
//! bridge: for each registered tool name it deserializes the JSON
//! `arguments` into that tool's Request type, calls the real
//! implementation, and serializes the Response back.
//!
//! ## Single source of truth
//!
//! [`dispatch_table`] enumerates every tool that has a dispatch
//! arm. It is kept in lockstep with
//! [`default_tool_registry`](crate::default_tool_registry): the
//! test `dispatch_table_matches_default_registry` fails the build
//! if a tool is registered but undispatched (or vice versa). A
//! tool advertised over `tools/list` that the dispatcher does not
//! know is a wire gap, and the test makes that impossible to ship.
//!
//! ## Two honest outcomes
//!
//! A dispatch arm produces one of:
//!
//! * [`DispatchOutcome::Ran`] — a self-contained static-analysis
//!   tool (the request fully describes the work) executed end to
//!   end; the payload is the real, structured Response.
//! * [`DispatchOutcome::RuntimeStateRequired`] — the tool is wired
//!   (the arm exists and the arguments were validated), but the
//!   call needs ambient runtime state the pure protocol layer does
//!   not hold: a live Oracle connection, a loaded dependency
//!   graph, or a session-scoped preview registry. This is an
//!   *honest* result, not a stub and not a fake success: it names
//!   exactly what is missing so the agent can correct course.
//! * an [`DispatchError`] — the tool name is unknown, or the
//!   `arguments` object did not deserialize into the tool's
//!   Request type. Both map to a JSON-RPC error, never a panic.
//!
//! Live-DB tools deliberately do **not** silently succeed: a
//! `query` with no connection returns `RuntimeStateRequired`, so a
//! client can never mistake "no database wired in this process"
//! for "the query ran and found nothing".

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    AnalyzeProjectRequest, CompileCheckRequest, CompletenessReportRequest, DocLookupRequest,
    DynamicSqlEvidenceRequest, GetSymbolRequest, ParseFileRequest, PlsqlAnalyzeRequest,
    run_analyze_project, run_compile_check, run_completeness_report, run_doc_lookup,
    run_dynamic_sql_evidence, run_get_symbol, run_inspect_profile, run_parse_file,
    run_plsql_analyze,
};

/// Why a dispatched tool could not run to completion in the pure
/// protocol layer. Distinct from [`DispatchError`]: the tool *is*
/// wired and its arguments validated — it just needs runtime state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeKind {
    /// Needs an active Oracle connection / live-DB session.
    LiveConnection,
    /// Needs a dependency graph from a prior project analysis.
    DependencyGraph,
    /// Needs a session-scoped preview/approval registry.
    PreviewSession,
    /// Needs mutable per-session safety/connection state.
    SessionState,
}

impl RuntimeKind {
    /// Honest, agent-facing explanation of the missing state.
    #[must_use]
    pub fn message(self, tool: &str) -> String {
        match self {
            Self::LiveConnection => format!(
                "tool `{tool}` is wired but needs an active Oracle connection; the foundation \
                 MCP server has no live-db runtime in this process. Build with the `live-db` \
                 feature and run inside a connected session to execute it."
            ),
            Self::DependencyGraph => format!(
                "tool `{tool}` is wired but needs a dependency graph; run `analyze_project` \
                 first to load one. The pure protocol layer holds no analysis state between \
                 calls."
            ),
            Self::PreviewSession => format!(
                "tool `{tool}` is wired but needs a session-scoped preview/approval registry; \
                 it executes inside the live-db runtime, which is not active in this process."
            ),
            Self::SessionState => format!(
                "tool `{tool}` is wired but needs mutable per-session connection/safety state; \
                 it executes inside the live-db runtime, which is not active in this process."
            ),
        }
    }
}

/// The result of dispatching one `tools/call`.
#[derive(Clone, Debug)]
pub enum DispatchOutcome {
    /// A self-contained tool ran; carries its structured Response.
    Ran(Value),
    /// The tool is wired but the call needs runtime state absent
    /// from the pure protocol layer.
    RuntimeStateRequired(RuntimeKind),
}

/// A dispatch failure that maps to a JSON-RPC error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchError {
    /// No dispatch arm for this tool name → `-32601`.
    UnknownTool(String),
    /// `arguments` did not deserialize into the Request type →
    /// `-32602`. Carries the serde message verbatim.
    InvalidArguments { tool: String, detail: String },
}

/// Every tool name with a dispatch arm. The single source of truth
/// the lockstep test checks against [`default_tool_registry`].
///
/// [`default_tool_registry`]: crate::default_tool_registry
#[must_use]
pub fn dispatch_table() -> &'static [&'static str] {
    &[
        // ── zero-arg discovery (call first) ──
        "oracle_capabilities",
        // ── self-contained static-analysis tools ──
        "parse_file",
        "get_symbol",
        "compile_check",
        "inspect_profile",
        "analyze_project",
        "plsql_analyze",
        "dynamic_sql_evidence",
        "completeness_report",
        "doc_lookup",
        // ── graph tools — need a loaded DepGraph ──
        "find_callers",
        "find_callees",
        "get_dependencies",
        // ── change-analysis tools — need graphs / reports ──
        "what_breaks",
        "recompile_plan",
        "classify_change",
        "compare_oracle_deps",
        "release_gate",
        "sarif_scan",
        "orphan_candidates",
        "explain_lifecycle",
        // ── connection / safety tools — need session state ──
        "list_connections",
        "connect",
        "disconnect",
        "current_database",
        "switch_database",
        "current_safety_profile",
        "set_safety_profile",
        "enable_writes",
        "disable_writes",
        // ── live-DB tools — need an Oracle connection ──
        "query",
        "patch_package",
        "patch_view",
        "create_or_replace",
        "execute_approved",
        "deploy_ddl",
    ]
}

/// The zero-arg `oracle_capabilities` discovery report (oracle-da9j.3): a
/// session-orientation document an agent calls FIRST. It reports the build
/// feature flags, the surface size, and static-vs-live guidance, and points at
/// `tools/list` for each tool's argument schema + read-only/destructive
/// annotations (so this stays a lean orientation doc, not a duplicate of the
/// per-tool detail). Honest about the runtime: the pure protocol layer holds no
/// live connection between calls; live-DB tools require the `live-db` build
/// feature + an active `connect`.
fn capabilities_report() -> Value {
    let live_db = cfg!(feature = "live-db");
    serde_json::json!({
        "server": "plsql-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": crate::mcp_protocol::PROTOCOL_VERSION,
        "tool_count": dispatch_table().len(),
        "features": { "live_db": live_db },
        "runtime": {
            "live_db_active": live_db,
            "note": "Static-analysis tools (parse_file, analyze_project, plsql_analyze, the graph \
                     tools, …) run with no database. Live-DB tools (query, connect, deploy_ddl, …) \
                     require the `live-db` build feature AND an active connection; without it they \
                     return a runtime-state-required result naming the recovery tool."
        },
        "next_actions": [
            "Call tools/list to read each tool's argument inputSchema and readOnlyHint/destructiveHint.",
            "Static analysis needs no connection — start with analyze_project, then the graph tools (find_callers / find_callees / get_dependencies).",
            "For any live-DB tool, call `connect` first."
        ]
    })
}

/// Deserialize the `arguments` object into a tool Request type,
/// turning a serde failure into a typed [`DispatchError`].
fn parse_args<T: DeserializeOwned>(tool: &str, arguments: &Value) -> Result<T, DispatchError> {
    serde_json::from_value(arguments.clone()).map_err(|e| DispatchError::InvalidArguments {
        tool: tool.to_string(),
        detail: e.to_string(),
    })
}

/// Dispatch one `tools/call` by tool name.
///
/// `arguments` is the raw `params.arguments` object (defaulting to
/// `{}` when the caller omitted it). Self-contained tools run and
/// return [`DispatchOutcome::Ran`]; tools needing ambient runtime
/// state validate their arguments (where a Request type exists)
/// and then return [`DispatchOutcome::RuntimeStateRequired`].
///
/// # Errors
///
/// [`DispatchError::UnknownTool`] when `name` has no arm, and
/// [`DispatchError::InvalidArguments`] when `arguments` does not
/// deserialize into the tool's Request type.
pub fn dispatch_tool(name: &str, arguments: &Value) -> Result<DispatchOutcome, DispatchError> {
    match name {
        // ── zero-arg discovery: a session-orientation report ──────
        "oracle_capabilities" => Ok(DispatchOutcome::Ran(capabilities_report())),
        // ── self-contained static tools: run end to end ──────────
        "parse_file" => {
            let req: ParseFileRequest = parse_args(name, arguments)?;
            Ok(ran(&run_parse_file(&req)))
        }
        "get_symbol" => {
            let req: GetSymbolRequest = parse_args(name, arguments)?;
            Ok(ran(&run_get_symbol(&req)))
        }
        "compile_check" => {
            let req: CompileCheckRequest = parse_args(name, arguments)?;
            Ok(ran(&run_compile_check(&req)))
        }
        "inspect_profile" => {
            // No request fields; ignore arguments entirely.
            Ok(ran(&run_inspect_profile()))
        }
        "analyze_project" => {
            let req: AnalyzeProjectRequest = parse_args(name, arguments)?;
            match run_analyze_project(req) {
                Ok(resp) => Ok(ran(&resp)),
                Err(e) => Err(DispatchError::InvalidArguments {
                    tool: name.to_string(),
                    detail: e.to_string(),
                }),
            }
        }
        "plsql_analyze" => {
            let req: PlsqlAnalyzeRequest = parse_args(name, arguments)?;
            match run_plsql_analyze(req) {
                Ok(resp) => Ok(ran(&resp)),
                Err(e) => Err(DispatchError::InvalidArguments {
                    tool: name.to_string(),
                    detail: e.to_string(),
                }),
            }
        }
        "dynamic_sql_evidence" => {
            let req: DynamicSqlEvidenceRequest = parse_args(name, arguments)?;
            Ok(ran(&run_dynamic_sql_evidence(&req)))
        }
        "completeness_report" => {
            let req: CompletenessReportRequest = parse_args(name, arguments)?;
            match run_completeness_report(&req) {
                Ok(resp) => Ok(ran(&resp)),
                Err(e) => Err(DispatchError::InvalidArguments {
                    tool: name.to_string(),
                    detail: e.to_string(),
                }),
            }
        }
        "doc_lookup" => {
            let req: DocLookupRequest = parse_args(name, arguments)?;
            Ok(ran(&run_doc_lookup(&req)))
        }

        // ── graph tools: validate the selector, then gate ────────
        // `GraphQueryRequest` is the validatable shape; the call
        // itself needs a loaded `DepGraph`.
        "find_callers" | "find_callees" | "get_dependencies" | "explain_lifecycle" => {
            let _req: crate::graph_tools::GraphQueryRequest = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::DependencyGraph,
            ))
        }
        "what_breaks" | "recompile_plan" | "classify_change" | "compare_oracle_deps"
        | "release_gate" | "sarif_scan" | "orphan_candidates" => {
            // These take graphs / reports / catalog snapshots that
            // are analysis state, not part of a JSON request.
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::DependencyGraph,
            ))
        }

        // ── connection / safety tools: need session state ────────
        "list_connections" | "connect" | "disconnect" | "current_database" | "switch_database"
        | "current_safety_profile" | "set_safety_profile" | "enable_writes" | "disable_writes" => {
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::SessionState,
            ))
        }

        // ── live-DB tools: arguments validated, then gated ───────
        "query" => {
            let _args: QueryArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "patch_package" => {
            let _req: crate::patch::PatchPackageRequest = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::PreviewSession,
            ))
        }
        "patch_view" => {
            let _req: crate::patch::PatchViewRequest = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::PreviewSession,
            ))
        }
        "create_or_replace" => {
            let _req: crate::create_or_replace::CreateOrReplaceRequest =
                parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::PreviewSession,
            ))
        }
        "execute_approved" => {
            let _req: crate::execute_approved::ExecuteApprovedRequest =
                parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::PreviewSession,
            ))
        }
        "deploy_ddl" => {
            let _args: DeployDdlArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }

        other => Err(DispatchError::UnknownTool(other.to_string())),
    }
}

/// Serialize a tool Response into a [`DispatchOutcome::Ran`]. The
/// Response types are all `Serialize`, so this never fails in
/// practice; a serialization failure is surfaced as an empty
/// object rather than a panic (the protocol layer keeps the wire
/// alive).
fn ran<T: serde::Serialize>(response: &T) -> DispatchOutcome {
    DispatchOutcome::Ran(serde_json::to_value(response).unwrap_or(Value::Object(Default::default())))
}

/// Argument shape for the `query` tool — mirrors the `run_query`
/// call surface so a malformed `arguments` object is rejected with
/// `-32602` before the (gated) execution path is reached. `sql` is
/// required; the rest are optional.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    #[allow(dead_code)]
    sql: String,
    #[serde(default)]
    #[allow(dead_code)]
    connection: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    lob_truncation_chars: Option<usize>,
}

/// Argument shape for `deploy_ddl` — validates the two fields the
/// `build_deploy_plan` surface needs before gating on the runtime.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeployDdlArgs {
    #[allow(dead_code)]
    job_name: String,
    #[allow(dead_code)]
    ddl_bytes: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dispatch_table_matches_default_registry() {
        // oracle-l65d: the dispatch table and the registry the
        // server actually advertises must be the same set — no
        // registered-but-undispatched tool, no phantom dispatch arm.
        let registry = crate::default_tool_registry();
        let mut registered: Vec<&str> =
            registry.tools.iter().map(|t| t.name.as_str()).collect();
        registered.sort_unstable();
        let mut dispatched: Vec<&str> = dispatch_table().to_vec();
        dispatched.sort_unstable();
        assert_eq!(
            registered, dispatched,
            "registry and dispatch table drifted out of lockstep"
        );
    }

    #[test]
    fn every_dispatch_table_entry_actually_dispatches() {
        // Every name in the table must resolve to an arm (never
        // `UnknownTool`) when handed an empty arguments object —
        // either it runs, gates, or rejects the empty args as
        // invalid, but it is never "unknown".
        for name in dispatch_table() {
            let outcome = dispatch_tool(name, &json!({}));
            if let Err(DispatchError::UnknownTool(t)) = &outcome {
                panic!("table entry `{t}` has no dispatch arm");
            }
        }
    }

    #[test]
    fn parse_file_runs_and_returns_real_response() {
        let out = dispatch_tool(
            "parse_file",
            &json!({"source": "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n"}),
        )
        .unwrap();
        let DispatchOutcome::Ran(v) = out else {
            panic!("parse_file is self-contained, must run");
        };
        assert!(v["declaration_count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn get_symbol_absent_is_a_real_found_none() {
        let out = dispatch_tool(
            "get_symbol",
            &json!({
                "source": "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n",
                "symbol": "NOPE"
            }),
        )
        .unwrap();
        let DispatchOutcome::Ran(v) = out else {
            panic!("get_symbol runs");
        };
        assert!(v["found"].is_null(), "absent symbol => found:null");
    }

    #[test]
    fn inspect_profile_ignores_arguments() {
        // No request fields — even junk arguments are accepted.
        let out = dispatch_tool("inspect_profile", &json!({"junk": true})).unwrap();
        assert!(matches!(out, DispatchOutcome::Ran(_)));
    }

    #[test]
    fn unknown_tool_is_a_typed_error() {
        let err = dispatch_tool("no_such_tool", &json!({})).unwrap_err();
        assert!(matches!(err, DispatchError::UnknownTool(_)));
    }

    #[test]
    fn malformed_arguments_are_invalid_arguments() {
        // `parse_file` needs a string `source`; a number fails.
        let err = dispatch_tool("parse_file", &json!({"source": 42})).unwrap_err();
        assert!(matches!(err, DispatchError::InvalidArguments { .. }));
    }

    #[test]
    fn query_without_connection_gates_honestly() {
        let out = dispatch_tool("query", &json!({"sql": "SELECT 1 FROM dual"})).unwrap();
        assert!(matches!(
            out,
            DispatchOutcome::RuntimeStateRequired(RuntimeKind::LiveConnection)
        ));
    }

    #[test]
    fn query_with_bad_sql_type_fails_before_gating() {
        // Argument validation runs before the runtime gate.
        let err = dispatch_tool("query", &json!({"sql": 7})).unwrap_err();
        assert!(matches!(err, DispatchError::InvalidArguments { .. }));
    }

    #[test]
    fn graph_tool_validates_selector_then_gates() {
        // A well-formed GraphQueryRequest gates on the missing graph.
        let out = dispatch_tool("find_callers", &json!({"target": "pkg.proc/1"})).unwrap();
        assert!(matches!(
            out,
            DispatchOutcome::RuntimeStateRequired(RuntimeKind::DependencyGraph)
        ));
        // A malformed selector is rejected before the gate.
        let err = dispatch_tool("find_callers", &json!({"target": 99})).unwrap_err();
        assert!(matches!(err, DispatchError::InvalidArguments { .. }));
    }

    #[test]
    fn patch_package_validates_request_then_gates() {
        let out = dispatch_tool(
            "patch_package",
            &json!({
                "connection": "c",
                "schema": "HR",
                "package": "PKG",
                "part": "spec",
                "source": "PACKAGE PKG AS END;",
                "mode": {"mode": "dry_run"}
            }),
        )
        .unwrap();
        assert!(matches!(
            out,
            DispatchOutcome::RuntimeStateRequired(RuntimeKind::PreviewSession)
        ));
    }

    #[test]
    fn runtime_kind_messages_name_the_missing_state() {
        assert!(
            RuntimeKind::LiveConnection
                .message("query")
                .contains("connection")
        );
        assert!(
            RuntimeKind::DependencyGraph
                .message("find_callers")
                .contains("graph")
        );
    }
}
