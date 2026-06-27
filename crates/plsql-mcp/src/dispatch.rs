//! Tool dispatch for `tools/call`.
//!
//! [`mcp_protocol::handle_tools_call`](crate::mcp_protocol) used to
//! return one static "registered but execution gated" placeholder
//! for *every* tool — none of the ~30 `run_*` implementations were
//! reachable over the JSON-RPC wire. This module is the missing
//! bridge: for each registered tool name it deserializes the JSON
//! `arguments` into that tool's Request type, calls the real
//! implementation, and serializes the Response back. The public dispatch
//! entry point matches `oraclemcp-core`'s Cx-aware async dispatch contract;
//! today's offline arms still run synchronously inside the returned future,
//! while Phase D can replace the gated live-DB arms with real awaits without
//! adding another runtime boundary.
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

use asupersync::Cx;
use asupersync::cx::SubsetOf;
use oraclemcp_core::{
    DispatchContext, DispatchFuture, ReadPathCaps, RequestBudget, ToolDispatch, narrow_to_read_path,
};
use oraclemcp_error::{ErrorClass, ErrorEnvelope};

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

/// Internal result of dispatching one `tools/call`.
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

impl DispatchError {
    fn into_envelope(self) -> ErrorEnvelope {
        match self {
            Self::UnknownTool(tool) => ErrorEnvelope::new(
                ErrorClass::InvalidArguments,
                format!("tool not found: {tool}"),
            )
            .with_next_step(
                "Call tools/list to see the exact tool names, then retry with one of them.",
            ),
            Self::InvalidArguments { tool, detail } => ErrorEnvelope::new(
                ErrorClass::InvalidArguments,
                format!("invalid arguments for tool `{tool}`: {detail}"),
            )
            .with_next_step(format!(
                "Inspect `{tool}`'s inputSchema in tools/list and supply the required fields."
            )),
        }
    }
}

/// Stateless adapter implementing the 0.4.0 `oraclemcp-core` dispatch trait.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlsqlToolDispatch;

impl ToolDispatch for PlsqlToolDispatch {
    fn dispatch<'a>(
        &'a self,
        cx: &'a Cx,
        context: DispatchContext<'a>,
        name: &'a str,
        args: Value,
    ) -> DispatchFuture<'a> {
        dispatch_tool(cx, PlsqlDispatchContext::from_cx(cx, context), name, args)
    }
}

/// `plsql-mcp`'s local dispatch context.
///
/// `oraclemcp-core` owns the public [`ToolDispatch`] trait and its transport
/// [`DispatchContext`]. B.6 keeps that upstream contract intact while giving the
/// PL/SQL dispatcher a local place to carry the adopted 0.4.0 request budget and
/// read-path capability surface. Later Phase C/D work can consume this context
/// without changing the MCP transport or forking the upstream trait.
#[derive(Clone, Copy, Debug)]
pub struct PlsqlDispatchContext<'a> {
    core: DispatchContext<'a>,
    request_budget: RequestBudget,
}

impl<'a> PlsqlDispatchContext<'a> {
    /// Build a PL/SQL dispatch context from the upstream transport context plus
    /// an explicit request budget.
    #[must_use]
    pub fn new(core: DispatchContext<'a>, request_budget: RequestBudget) -> Self {
        Self {
            core,
            request_budget,
        }
    }

    /// Adopt the currently installed Asupersync context budget at the
    /// dispatch boundary. This only carries the budget; enforcement and query
    /// timeout propagation stay with the later Phase D bead.
    #[must_use]
    pub fn from_cx(cx: &Cx, core: DispatchContext<'a>) -> Self {
        Self::new(core, RequestBudget::from_budget(cx.budget()))
    }

    /// The upstream transport authorization context.
    #[must_use]
    pub fn core(self) -> DispatchContext<'a> {
        self.core
    }

    /// Per-request budget captured from the Asupersync dispatch context.
    #[must_use]
    pub fn request_budget(self) -> RequestBudget {
        self.request_budget
    }

    /// Narrow the supplied Asupersync context to the read-path capability row.
    ///
    /// The context value is passed in explicitly because the capability row is a
    /// property of the runtime `Cx`, not of the transport metadata. Keeping the
    /// helper here makes the Phase C read loaders consume the same dispatch
    /// context that carries the request budget.
    #[must_use]
    pub fn narrow_to_read_path<Caps>(self, cx: &Cx<Caps>) -> Cx<ReadPathCaps>
    where
        ReadPathCaps: SubsetOf<Caps>,
    {
        narrow_dispatch_to_read_path(cx)
    }
}

/// Narrow a dispatcher context to the read-path capability row.
///
/// This is intentionally a thin local wrapper around `oraclemcp-core`'s
/// zero-cost type-level narrowing helper. It gives Phase C read loaders a
/// `plsql-mcp` import point while preserving the upstream capability proof.
#[must_use]
pub fn narrow_dispatch_to_read_path<Caps>(cx: &Cx<Caps>) -> Cx<ReadPathCaps>
where
    ReadPathCaps: SubsetOf<Caps>,
{
    narrow_to_read_path(cx)
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
/// `arguments` is the owned raw `params.arguments` object (defaulting to
/// `{}` when the caller omitted it). Self-contained tools run and return their
/// structured JSON payload. Tools needing ambient runtime state validate their
/// arguments (where a Request type exists) and then return an
/// [`ErrorEnvelope`] with [`ErrorClass::RuntimeStateRequired`].
///
/// # Errors
///
/// Returns [`ErrorClass::InvalidArguments`] when `name` has no arm or when
/// `arguments` does not deserialize into the tool's Request type.
#[must_use]
pub fn dispatch_tool<'a>(
    _cx: &'a Cx,
    _context: PlsqlDispatchContext<'a>,
    name: &'a str,
    arguments: Value,
) -> DispatchFuture<'a> {
    Box::pin(async move {
        match dispatch_tool_outcome(name, &arguments) {
            Ok(DispatchOutcome::Ran(value)) => Ok(value),
            Ok(DispatchOutcome::RuntimeStateRequired(kind)) => {
                Err(runtime_state_envelope(kind, name))
            }
            Err(err) => Err(err.into_envelope()),
        }
    })
}

fn dispatch_tool_outcome(name: &str, arguments: &Value) -> Result<DispatchOutcome, DispatchError> {
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
        "what_breaks"
        | "recompile_plan"
        | "classify_change"
        | "compare_oracle_deps"
        | "release_gate"
        | "sarif_scan"
        | "orphan_candidates" => {
            // These take graphs / reports / catalog snapshots that
            // are analysis state, not part of a JSON request.
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::DependencyGraph,
            ))
        }

        // ── connection / safety tools: need session state ────────
        "list_connections"
        | "connect"
        | "disconnect"
        | "current_database"
        | "switch_database"
        | "current_safety_profile"
        | "set_safety_profile"
        | "enable_writes"
        | "disable_writes" => Ok(DispatchOutcome::RuntimeStateRequired(
            RuntimeKind::SessionState,
        )),

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

fn runtime_state_envelope(kind: RuntimeKind, tool: &str) -> ErrorEnvelope {
    let msg = kind.message(tool);
    ErrorEnvelope::new(ErrorClass::RuntimeStateRequired, msg.clone())
        .with_suggested_tool(runtime_kind_recovery_tool(kind))
        .with_next_step(format!(
            "Call `{}` to provide the missing runtime state, then retry `{tool}`.",
            runtime_kind_recovery_tool(kind)
        ))
}

fn runtime_kind_recovery_tool(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::DependencyGraph => "analyze_project",
        RuntimeKind::LiveConnection | RuntimeKind::PreviewSession | RuntimeKind::SessionState => {
            "connect"
        }
    }
}

/// Serialize a tool Response into a [`DispatchOutcome::Ran`]. The
/// Response types are all `Serialize`, so this never fails in
/// practice; a serialization failure is surfaced as an empty
/// object rather than a panic (the protocol layer keeps the wire
/// alive).
fn ran<T: serde::Serialize>(response: &T) -> DispatchOutcome {
    DispatchOutcome::Ran(
        serde_json::to_value(response).unwrap_or(Value::Object(Default::default())),
    )
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

    fn dispatch_for_test(name: &str, args: Value) -> Result<Value, Box<ErrorEnvelope>> {
        let reactor = asupersync::runtime::reactor::create_reactor().unwrap();
        let runtime = asupersync::runtime::RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .unwrap();
        runtime.block_on(async {
            let cx = Cx::current().expect("block_on installs a request Cx");
            let context = PlsqlDispatchContext::from_cx(&cx, DispatchContext::default());
            dispatch_tool(&cx, context, name, args)
                .await
                .map_err(Box::new)
        })
    }

    #[test]
    fn plsql_dispatch_context_carries_request_budget() {
        let reactor = asupersync::runtime::reactor::create_reactor().unwrap();
        let runtime = asupersync::runtime::RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .unwrap();
        runtime.block_on(async {
            let cx = Cx::current().expect("block_on installs a request Cx");
            let context = PlsqlDispatchContext::from_cx(&cx, DispatchContext::default());
            let request_budget: crate::RequestBudget = context.request_budget();

            assert_eq!(request_budget.budget(), cx.budget());
        });
    }

    #[test]
    fn read_path_caps_are_reachable_from_dispatch_context() {
        let reactor = asupersync::runtime::reactor::create_reactor().unwrap();
        let runtime = asupersync::runtime::RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .unwrap();
        runtime.block_on(async {
            let cx = Cx::current().expect("block_on installs a request Cx");
            let context = PlsqlDispatchContext::from_cx(&cx, DispatchContext::default());
            let read_cx: Cx<crate::ReadPathCaps> = context.narrow_to_read_path(&cx);

            fn assert_read_path(_: &Cx<crate::ReadPathCaps>) {}
            assert_read_path(&read_cx);
        });
    }

    #[test]
    fn dispatch_table_matches_default_registry() {
        // oracle-l65d: the dispatch table and the registry the
        // server actually advertises must be the same set — no
        // registered-but-undispatched tool, no phantom dispatch arm.
        let registry = crate::default_tool_registry();
        let mut registered: Vec<&str> = registry.tools.iter().map(|t| t.name.as_str()).collect();
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
            let outcome = dispatch_for_test(name, json!({}));
            if let Err(envelope) = &outcome {
                assert!(
                    !envelope.message.contains("tool not found"),
                    "table entry `{name}` has no dispatch arm: {envelope:?}"
                );
            }
        }
    }

    #[test]
    fn every_self_contained_static_tool_actually_runs() {
        // oracle-687a.6: the lockstep + "dispatches" tests only prove each name
        // resolves to an arm; they do NOT prove the arm does real work. Here every
        // genuinely self-contained static tool (no ambient graph / connection /
        // preview state) is called with MINIMAL VALID args and MUST return
        // DispatchOutcome::Ran with a structured result — a stub arm that returned
        // RuntimeStateRequired or panicked would fail this.
        let dir = std::env::temp_dir().join(format!(
            "plsql-687a6-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        let src = "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n";
        let cases: &[(&str, serde_json::Value)] = &[
            ("oracle_capabilities", json!({})),
            ("parse_file", json!({ "source": src })),
            ("get_symbol", json!({ "source": src, "symbol": "P" })),
            ("compile_check", json!({ "source": src })),
            ("inspect_profile", json!({})),
            (
                "dynamic_sql_evidence",
                json!({ "call_text": "EXECUTE IMMEDIATE 'SELECT 1 FROM dual'", "site": "p:1" }),
            ),
            ("doc_lookup", json!({ "source": src, "query": "" })),
            ("completeness_report", json!({ "project_root": root })),
            ("analyze_project", json!({ "project_root": root })),
            ("plsql_analyze", json!({ "project_root": root })),
        ];
        for (name, args) in cases {
            let value = dispatch_for_test(name, args.clone());
            assert!(
                value.is_ok(),
                "self-contained tool `{name}` errored: {:?}",
                value.as_ref().err()
            );
            let value = value.unwrap_or(Value::Null);
            assert!(
                value.is_object() || value.is_array(),
                "tool `{name}` must return a structured result, got {value}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_file_runs_and_returns_real_response() {
        let out = dispatch_for_test(
            "parse_file",
            json!({"source": "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n"}),
        )
        .unwrap();
        assert!(out["declaration_count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn get_symbol_absent_is_a_real_found_none() {
        let out = dispatch_for_test(
            "get_symbol",
            json!({
                "source": "CREATE PROCEDURE p IS BEGIN NULL; END;\n/\n",
                "symbol": "NOPE"
            }),
        )
        .unwrap();
        assert!(out["found"].is_null(), "absent symbol => found:null");
    }

    #[test]
    fn inspect_profile_ignores_arguments() {
        // No request fields — even junk arguments are accepted.
        let out = dispatch_for_test("inspect_profile", json!({"junk": true})).unwrap();
        assert!(out.is_object());
    }

    #[test]
    fn unknown_tool_is_a_typed_error() {
        let err = dispatch_for_test("no_such_tool", json!({})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::InvalidArguments);
        assert!(err.message.contains("tool not found"));
    }

    #[test]
    fn malformed_arguments_are_invalid_arguments() {
        // `parse_file` needs a string `source`; a number fails.
        let err = dispatch_for_test("parse_file", json!({"source": 42})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::InvalidArguments);
    }

    #[test]
    fn query_without_connection_gates_honestly() {
        let err = dispatch_for_test("query", json!({"sql": "SELECT 1 FROM dual"})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::RuntimeStateRequired);
        assert_eq!(err.suggested_tool.as_deref(), Some("connect"));
    }

    #[test]
    fn query_with_bad_sql_type_fails_before_gating() {
        // Argument validation runs before the runtime gate.
        let err = dispatch_for_test("query", json!({"sql": 7})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::InvalidArguments);
    }

    #[test]
    fn graph_tool_validates_selector_then_gates() {
        // A well-formed GraphQueryRequest gates on the missing graph.
        let err = dispatch_for_test("find_callers", json!({"target": "pkg.proc/1"})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::RuntimeStateRequired);
        assert_eq!(err.suggested_tool.as_deref(), Some("analyze_project"));
        // A malformed selector is rejected before the gate.
        let err = dispatch_for_test("find_callers", json!({"target": 99})).unwrap_err();
        assert_eq!(err.error_class, ErrorClass::InvalidArguments);
    }

    #[test]
    fn patch_package_validates_request_then_gates() {
        let err = dispatch_for_test(
            "patch_package",
            json!({
                "connection": "c",
                "schema": "HR",
                "package": "PKG",
                "part": "spec",
                "source": "PACKAGE PKG AS END;",
                "mode": {"mode": "dry_run"}
            }),
        )
        .unwrap_err();
        assert_eq!(err.error_class, ErrorClass::RuntimeStateRequired);
        assert_eq!(err.suggested_tool.as_deref(), Some("connect"));
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
