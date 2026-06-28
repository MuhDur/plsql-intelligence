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
use std::{sync::Arc, time::Duration};

use asupersync::Cx;
use asupersync::cx::SubsetOf;
use oraclemcp_audit::{AuditDecision, AuditOutcome, AuditRecord};
use oraclemcp_core::{
    CapabilitiesReport, DispatchContext, DispatchFuture, FeatureTiers, ReadPathCaps, RequestBudget,
    ToolDispatch, ToolRegistry, narrow_to_read_path,
};
use oraclemcp_error::{ErrorClass, ErrorEnvelope, enrich_oracle_error};
use oraclemcp_guard::{OperatingLevel, SessionLevelState};
use plsql_catalog::{CatalogError, OracleBind};

use crate::{
    AnalyzeProjectRequest, CompileCheckRequest, CompletenessReportRequest, DocLookupRequest,
    DynamicSqlEvidenceRequest, GetSymbolRequest, ParseFileRequest, PlsqlAnalyzeRequest, QueryError,
    run_analyze_project, run_compile_check, run_completeness_report, run_doc_lookup,
    run_dynamic_sql_evidence, run_get_symbol, run_inspect_profile, run_parse_file,
    run_plsql_analyze,
};
use crate::{AuditClient, AuditPlan, GuardedAuditDraft, SafetyProfileError};
use crate::{ConnectionProfile, LiveDbRuntime, LiveRuntimeError, OraclemcpCatalogConnection};

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
///
/// ```compile_fail
/// use asupersync::Cx;
/// use plsql_mcp::{ReadPathCaps, requires_privileged_effect};
///
/// fn read_handler(cx: &Cx<ReadPathCaps>) {
///     requires_privileged_effect(cx);
/// }
/// ```
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
        "list_objects",
        "describe_table",
        "describe_view",
        "describe_trigger",
        "describe_index",
        "get_object_source",
        "get_errors",
        "get_clob",
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
pub(crate) fn capabilities_report_for_registry(registry: &ToolRegistry) -> Value {
    let live_db = cfg!(feature = "live-db");
    let mut report = CapabilitiesReport::new(
        env!("CARGO_PKG_VERSION"),
        registry.tools.clone(),
        OperatingLevel::Ddl,
        FeatureTiers {
            live_db,
            engine: true,
            http_transport: false,
        },
    );
    report.server_name = "plsql-mcp".to_owned();
    report.protocol_version = crate::mcp_protocol::PROTOCOL_VERSION.to_owned();

    let mut value = serde_json::to_value(report).unwrap_or_else(|_| {
        serde_json::json!({
            "server_name": "plsql-mcp",
            "server_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": crate::mcp_protocol::PROTOCOL_VERSION,
        })
    });
    if let Value::Object(obj) = &mut value {
        obj.insert(
            "tool_count".to_owned(),
            serde_json::json!(registry.tools.len()),
        );
        obj.insert(
            "runtime".to_owned(),
            serde_json::json!({
                "live_db_active": live_db,
                "note": "Static-analysis tools (parse_file, analyze_project, plsql_analyze, the graph \
                         tools, ...) run with no database. Live-DB tools (query, connect, deploy_ddl, ...) \
                         require the `live-db` build feature AND an active connection; without it they \
                         return a runtime-state-required result naming the recovery tool."
            }),
        );
        obj.insert(
            "resources".to_owned(),
            serde_json::json!(["oracle://capabilities", "oracle://tools"]),
        );
        obj.insert(
            "next_actions".to_owned(),
            serde_json::json!([
                "Call resources/list to discover the oracle:// resources.",
                "Call resources/read with oracle://capabilities for this report as a resource.",
                "Call tools/list to read each tool's argument inputSchema and readOnlyHint/destructiveHint.",
                "Static analysis needs no connection: start with analyze_project, then graph tools.",
                "For any live-DB tool, call connect first."
            ]),
        );
    }
    value
}

fn capabilities_report() -> Value {
    capabilities_report_for_registry(&crate::default_tool_registry())
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

/// Runtime-aware dispatch used by the real MCP server.
///
/// The pure [`dispatch_tool`] entry point remains available for offline tests
/// and `oraclemcp-core::ToolDispatch`, but the shipping server enters here so
/// live-DB arms see the same request [`Cx`] and [`LiveDbRuntime`] that the
/// protocol layer owns.
#[must_use]
pub fn dispatch_tool_with_runtime<'a>(
    cx: &'a Cx,
    context: PlsqlDispatchContext<'a>,
    live_runtime: &'a mut LiveDbRuntime,
    name: &'a str,
    arguments: Value,
) -> DispatchFuture<'a> {
    Box::pin(async move {
        checkpoint(cx, name).map_err(|err| *err)?;
        enforce_oauth_scope_ceiling(context, live_runtime, name).map_err(|err| *err)?;
        match name {
            "connect" => run_connect(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "list_connections" => run_list_connections(cx, live_runtime).map_err(|err| *err),
            "current_database" => run_current_database(cx, context, live_runtime)
                .await
                .map_err(|err| *err),
            "current_safety_profile" => {
                run_current_safety_profile(cx, live_runtime).map_err(|err| *err)
            }
            "set_safety_profile" => {
                run_set_safety_profile(cx, live_runtime, &arguments).map_err(|err| *err)
            }
            "enable_writes" => run_enable_writes(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "disable_writes" => run_disable_writes(cx, live_runtime).map_err(|err| *err),
            "query" => run_query_live(
                cx,
                context,
                context.request_budget(),
                live_runtime,
                &arguments,
            )
            .await
            .map_err(|err| *err),
            "list_objects" => run_list_objects_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "describe_table" => run_describe_table_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "describe_view" => run_describe_view_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "describe_trigger" => run_describe_trigger_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "describe_index" => run_describe_index_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "get_object_source" => run_get_object_source_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "get_errors" => run_get_errors_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "get_clob" => run_get_clob_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "patch_package" => run_patch_package_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "patch_view" => run_patch_view_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "create_or_replace" => run_create_or_replace_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "execute_approved" => run_execute_approved_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            "deploy_ddl" => run_deploy_ddl_live(cx, live_runtime, &arguments)
                .await
                .map_err(|err| *err),
            _ => match dispatch_tool_outcome(name, &arguments) {
                Ok(DispatchOutcome::Ran(value)) => Ok(value),
                Ok(DispatchOutcome::RuntimeStateRequired(kind)) => {
                    Err(runtime_state_envelope(kind, name))
                }
                Err(err) => Err(err.into_envelope()),
            },
        }
    })
}

fn enforce_oauth_scope_ceiling(
    context: PlsqlDispatchContext<'_>,
    live_runtime: &LiveDbRuntime,
    tool: &str,
) -> Result<(), Box<ErrorEnvelope>> {
    let Some(required) = required_operating_level(tool) else {
        return Ok(());
    };
    let Some(grant) = context.core().scope_grant() else {
        return Ok(());
    };

    let max_level = live_runtime
        .active_session()
        .map(|session| safety_profile_ceiling(session.safety().profile))
        .unwrap_or(OperatingLevel::Admin);
    let mut scoped = SessionLevelState::new(max_level, false);
    let scopes = grant.0.iter().map(String::as_str).collect::<Vec<_>>();
    oraclemcp_auth::apply_oauth_scopes(&mut scoped, &scopes);
    let ceiling = scoped.effective_ceiling();
    if required <= ceiling {
        return Ok(());
    }

    Err(Box::new(
        ErrorEnvelope::new(
            ErrorClass::OperatingLevelTooLow,
            format!(
                "tool `{tool}` requires {} but the authenticated request's OAuth scope ceiling is {}",
                required.as_str(),
                ceiling.as_str()
            ),
        )
        .with_next_step(
            "Retry with an OAuth token carrying a sufficient oracle:* scope, or use a read-only tool.",
        ),
    ))
}

pub(crate) fn required_operating_level(tool: &str) -> Option<OperatingLevel> {
    match tool {
        "enable_writes" => Some(OperatingLevel::ReadWrite),
        "set_safety_profile" | "patch_package" | "patch_view" | "create_or_replace"
        | "execute_approved" | "deploy_ddl" => Some(OperatingLevel::Ddl),
        _ => None,
    }
}

pub(crate) fn safety_profile_ceiling(profile: crate::SafetyProfile) -> OperatingLevel {
    match profile {
        crate::SafetyProfile::StaticOnly | crate::SafetyProfile::InspectOnly => {
            OperatingLevel::ReadOnly
        }
        crate::SafetyProfile::DdlGuarded | crate::SafetyProfile::SessionWriteEnabled => {
            OperatingLevel::Ddl
        }
    }
}

pub(crate) fn active_safety_profile(live_runtime: &LiveDbRuntime) -> crate::SafetyProfile {
    live_runtime
        .active_session()
        .map(|session| session.safety().profile)
        .unwrap_or_else(|_| live_runtime.default_safety_profile())
}

pub(crate) fn effective_oauth_scope_ceiling(
    context: DispatchContext<'_>,
    live_runtime: &LiveDbRuntime,
) -> Option<OperatingLevel> {
    let grant = context.scope_grant()?;
    let max_level = live_runtime
        .active_session()
        .map(|session| safety_profile_ceiling(session.safety().profile))
        .unwrap_or(OperatingLevel::Admin);
    let mut scoped = SessionLevelState::new(max_level, false);
    let scopes = grant.0.iter().map(String::as_str).collect::<Vec<_>>();
    oraclemcp_auth::apply_oauth_scopes(&mut scoped, &scopes);
    Some(scoped.effective_ceiling())
}

async fn run_connect(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: ConnectArgs =
        parse_args("connect", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let name = args.name.trim();
    if name.is_empty() {
        return Err(Box::new(invalid_arguments_envelope(
            "connect",
            "`name` must not be empty",
        )));
    }

    if args.connect_string.is_none() {
        let lease = live_runtime.activate(name).map_err(|err| {
            if matches!(err, LiveRuntimeError::UnknownConnection { .. }) {
                Box::new(invalid_arguments_envelope(
                    "connect",
                    "no existing live session has that name, and no `connect_string` was supplied",
                ))
            } else {
                Box::new(live_runtime_error_envelope(err, "connect"))
            }
        })?;
        return Ok(serde_json::json!({
            "connected": true,
            "reused_existing_session": true,
            "active": name,
            "lease": lease,
            "connected_count": live_runtime.len(),
        }));
    }

    let connect_string = args
        .connect_string
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            Box::new(invalid_arguments_envelope(
                "connect",
                "`connect_string` must not be empty when supplied",
            ))
        })?;

    let profile = ConnectionProfile {
        name: String::from(name),
        description: args.description.clone(),
        connect_string: connect_string.clone(),
        username: args.username.clone(),
        permanently_read_only: args.permanently_read_only,
        dbtools_alias: None,
    };
    let options = oraclemcp_db::OracleConnectOptions {
        connect_string,
        username: args.username,
        password: args.password,
        external_auth: args.external_auth,
        ..oraclemcp_db::OracleConnectOptions::default()
    };

    checkpoint(cx, "connect")?;
    let connection = oraclemcp_db::RustOracleConnection::connect(cx, options)
        .await
        .map_err(|err| Box::new(db_connection_error_envelope(err, "connect")))?;
    checkpoint(cx, "connect")?;

    let lease = live_runtime
        .insert_and_activate(profile, Box::new(connection))
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "connect")))?;

    Ok(serde_json::json!({
        "connected": true,
        "reused_existing_session": false,
        "active": name,
        "lease": lease,
        "connected_count": live_runtime.len(),
    }))
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
        "list_objects" => {
            let _args: ListObjectsArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "describe_table" | "describe_trigger" | "describe_index" => {
            let _args: OwnerNameArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "describe_view" => {
            let _args: DescribeViewArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "get_object_source" => {
            let _args: GetObjectSourceArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "get_errors" => {
            let _args: GetErrorsArgs = parse_args(name, arguments)?;
            Ok(DispatchOutcome::RuntimeStateRequired(
                RuntimeKind::LiveConnection,
            ))
        }
        "get_clob" => {
            let _args: GetClobArgs = parse_args(name, arguments)?;
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

fn run_list_connections(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
) -> Result<Value, Box<ErrorEnvelope>> {
    let mut connected = Vec::with_capacity(live_runtime.len());
    for name in live_runtime.connected_names() {
        checkpoint(cx, "list_connections")?;
        connected.push(serde_json::json!({
            "name": name,
            "is_active": live_runtime.active_name() == Some(name),
        }));
    }
    Ok(serde_json::json!({
        "connected_count": live_runtime.len(),
        "active": live_runtime.active_name(),
        "connections": connected,
    }))
}

async fn run_current_database(
    cx: &Cx,
    context: PlsqlDispatchContext<'_>,
    live_runtime: &LiveDbRuntime,
) -> Result<Value, Box<ErrorEnvelope>> {
    checkpoint(cx, "current_database")?;
    let lease = live_runtime.active_lease().cloned();
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "current_database")))?;
    let profile = session.profile();
    let safety = session.safety();
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    let read_cx = context.narrow_to_read_path(cx);
    // oraclemcp-db 0.4.0 still accepts `&Cx`, so keep passing the upstream
    // trait shape while narrowing any ambient `Cx::current()` lookups during
    // the catalog read.
    let _read_path_guard = read_cx.set_current_restricted();
    let catalog = adapter.describe(cx).await.map_err(|err| {
        Box::new(ErrorEnvelope::new(
            ErrorClass::ConnectionFailed,
            err.to_string(),
        ))
    })?;
    checkpoint(cx, "current_database")?;

    Ok(serde_json::json!({
        "active": {
            "name": profile.name.clone(),
            "description": profile.description.clone(),
            "connect_string": profile.connect_string.clone(),
            "username": profile.username.clone(),
            "permanently_read_only": profile.permanently_read_only,
            "backend": format!("{:?}", session.backend()),
            "safety_profile": safety.profile.as_str(),
            "session_writes_enabled": safety.session_writes_enabled,
            "active_enable_writes_token": safety.active_token.is_some(),
            "lease": lease,
            "catalog": {
                "current_schema": catalog.current_schema,
                "server_version": catalog.server_version,
                "server_type": catalog.server_type,
            },
        },
        "connected_count": live_runtime.len(),
    }))
}

fn run_current_safety_profile(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
) -> Result<Value, Box<ErrorEnvelope>> {
    checkpoint(cx, "current_safety_profile")?;
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "current_safety_profile")))?;
    Ok(safety_profile_json(session))
}

fn run_set_safety_profile(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    checkpoint(cx, "set_safety_profile")?;
    let args: SetSafetyProfileArgs =
        parse_args("set_safety_profile", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    live_runtime
        .set_active_safety_profile(args.profile)
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "set_safety_profile")))?;
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "set_safety_profile")))?;
    Ok(serde_json::json!({
        "updated": true,
        "safety": safety_profile_json(session),
    }))
}

async fn run_enable_writes(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    checkpoint(cx, "enable_writes")?;
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "enable_writes")))?;
    let args: EnableWritesArgs =
        parse_args("enable_writes", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let connection = session.profile().name.clone();
    let operation_summary = session
        .safety()
        .active_token
        .as_ref()
        .map(|token| token.operation_summary.clone())
        .unwrap_or_else(|| String::from("enable_writes"));
    let audit_record = append_guarded_audit(
        live_runtime,
        "enable_writes",
        &format!("enable_writes {connection}: {operation_summary}"),
        "ESCALATION",
    )?;
    let now = unix_now_seconds();
    live_runtime
        .active_session_mut()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "enable_writes")))?
        .enable_writes(&args.token, now)
        .map_err(|err| Box::new(safety_error_envelope(err, "enable_writes")))?;
    checkpoint(cx, "enable_writes")?;
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "enable_writes")))?;
    Ok(serde_json::json!({
        "enabled": true,
        "safety": safety_profile_json(session),
        "audit_record": audit_record,
    }))
}

fn run_disable_writes(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
) -> Result<Value, Box<ErrorEnvelope>> {
    checkpoint(cx, "disable_writes")?;
    let changed = match live_runtime
        .active_session_mut()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "disable_writes")))?
        .disable_writes()
    {
        Ok(()) => true,
        Err(SafetyProfileError::AlreadyReadOnly) => false,
        Err(err) => return Err(Box::new(safety_error_envelope(err, "disable_writes"))),
    };
    let session = live_runtime
        .active_session()
        .map_err(|err| Box::new(live_runtime_error_envelope(err, "disable_writes")))?;
    Ok(serde_json::json!({
        "disabled": true,
        "changed": changed,
        "safety": safety_profile_json(session),
    }))
}

async fn run_patch_package_live(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let req: crate::patch::PatchPackageRequest =
        parse_args("patch_package", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let token = mint_preview_token()?;
    let response =
        crate::patch::run_patch_package(live_runtime.preview_registry_mut(), req, move || token)
            .map_err(|err| {
                Box::new(invalid_arguments_envelope(
                    "patch_package",
                    &err.to_string(),
                ))
            })?;
    match response {
        crate::patch::PatchPackageResponse::DryRun {
            token,
            connection,
            ddl_bytes,
            ddl_sha256,
        } => Ok(serde_json::json!({
            "kind": "dry_run",
            "token": token,
            "connection": connection,
            "ddl_bytes": ddl_bytes,
            "ddl_sha256": ddl_sha256,
            "impact_summary": crate::impact::guarded_write_impact("patch_package", &connection, &ddl_bytes),
        })),
        crate::patch::PatchPackageResponse::Apply {
            connection,
            ddl_bytes,
            ddl_sha256,
        } => {
            let impact_summary =
                crate::impact::guarded_write_impact("patch_package", &connection, &ddl_bytes);
            let executed =
                execute_guarded_sql(cx, live_runtime, &connection, "patch_package", &ddl_bytes)
                    .await?;
            live_runtime.preview_registry_mut().consume(&connection);
            Ok(serde_json::json!({
                "kind": "apply",
                "connection": connection,
                "ddl_bytes": ddl_bytes,
                "ddl_sha256": ddl_sha256,
                "impact_summary": impact_summary,
                "rows_affected": executed.rows_affected,
                "audit_record": executed.audit_record,
            }))
        }
    }
}

async fn run_patch_view_live(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let req: crate::patch::PatchViewRequest =
        parse_args("patch_view", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let token = mint_preview_token()?;
    let response =
        crate::patch::run_patch_view(live_runtime.preview_registry_mut(), req, move || token)
            .map_err(|err| Box::new(invalid_arguments_envelope("patch_view", &err.to_string())))?;
    match response {
        crate::patch::PatchViewResponse::DryRun {
            token,
            connection,
            ddl_bytes,
            ddl_sha256,
        } => Ok(serde_json::json!({
            "kind": "dry_run",
            "token": token,
            "connection": connection,
            "ddl_bytes": ddl_bytes,
            "ddl_sha256": ddl_sha256,
            "impact_summary": crate::impact::guarded_write_impact("patch_view", &connection, &ddl_bytes),
        })),
        crate::patch::PatchViewResponse::Apply {
            connection,
            ddl_bytes,
            ddl_sha256,
        } => {
            let impact_summary =
                crate::impact::guarded_write_impact("patch_view", &connection, &ddl_bytes);
            let executed =
                execute_guarded_sql(cx, live_runtime, &connection, "patch_view", &ddl_bytes)
                    .await?;
            live_runtime.preview_registry_mut().consume(&connection);
            Ok(serde_json::json!({
                "kind": "apply",
                "connection": connection,
                "ddl_bytes": ddl_bytes,
                "ddl_sha256": ddl_sha256,
                "impact_summary": impact_summary,
                "rows_affected": executed.rows_affected,
                "audit_record": executed.audit_record,
            }))
        }
    }
}

async fn run_create_or_replace_live(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let req: crate::create_or_replace::CreateOrReplaceRequest =
        parse_args("create_or_replace", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let token = mint_preview_token()?;
    let response = crate::create_or_replace::run_create_or_replace(
        live_runtime.preview_registry_mut(),
        req,
        move || token,
    )
    .map_err(|err| {
        Box::new(invalid_arguments_envelope(
            "create_or_replace",
            &err.to_string(),
        ))
    })?;
    match response {
        crate::create_or_replace::CreateOrReplaceResponse::DryRun {
            token,
            connection,
            object_kind,
            ddl_bytes,
            ddl_sha256,
        } => Ok(serde_json::json!({
            "kind": "dry_run",
            "token": token,
            "connection": connection,
            "object_kind": object_kind,
            "ddl_bytes": ddl_bytes,
            "ddl_sha256": ddl_sha256,
            "impact_summary": crate::impact::guarded_write_impact("create_or_replace", &connection, &ddl_bytes),
        })),
        crate::create_or_replace::CreateOrReplaceResponse::Apply {
            connection,
            object_kind,
            ddl_bytes,
            ddl_sha256,
        } => {
            let impact_summary =
                crate::impact::guarded_write_impact("create_or_replace", &connection, &ddl_bytes);
            let executed = execute_guarded_sql(
                cx,
                live_runtime,
                &connection,
                "create_or_replace",
                &ddl_bytes,
            )
            .await?;
            live_runtime.preview_registry_mut().consume(&connection);
            Ok(serde_json::json!({
                "kind": "apply",
                "connection": connection,
                "object_kind": object_kind,
                "ddl_bytes": ddl_bytes,
                "ddl_sha256": ddl_sha256,
                "impact_summary": impact_summary,
                "rows_affected": executed.rows_affected,
                "audit_record": executed.audit_record,
            }))
        }
    }
}

async fn run_execute_approved_live(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let req: crate::execute_approved::ExecuteApprovedRequest =
        parse_args("execute_approved", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let plan =
        crate::execute_approved::run_execute_approved(live_runtime.preview_registry_mut(), req)
            .map_err(|err| {
                Box::new(invalid_arguments_envelope(
                    "execute_approved",
                    &err.to_string(),
                ))
            })?;
    let impact_summary =
        crate::impact::guarded_write_impact("execute_approved", &plan.connection, &plan.ddl_bytes);
    let executed = execute_guarded_sql(
        cx,
        live_runtime,
        &plan.connection,
        "execute_approved",
        &plan.ddl_bytes,
    )
    .await?;
    crate::execute_approved::consume_approved(live_runtime.preview_registry_mut(), &plan);
    Ok(serde_json::json!({
        "kind": "executed",
        "plan": plan,
        "impact_summary": impact_summary,
        "rows_affected": executed.rows_affected,
        "audit_record": executed.audit_record,
    }))
}

async fn run_deploy_ddl_live(
    cx: &Cx,
    live_runtime: &mut LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: DeployDdlArgs =
        parse_args("deploy_ddl", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    if args.job_name.trim().is_empty() {
        return Err(Box::new(invalid_arguments_envelope(
            "deploy_ddl",
            "`job_name` must not be empty",
        )));
    }
    if args.ddl_bytes.trim().is_empty() {
        return Err(Box::new(invalid_arguments_envelope(
            "deploy_ddl",
            "`ddl_bytes` must not be empty",
        )));
    }
    let connection = live_runtime
        .active_name()
        .ok_or_else(|| {
            Box::new(live_runtime_error_envelope(
                LiveRuntimeError::NoActiveConnection,
                "deploy_ddl",
            ))
        })?
        .to_string();
    let plan = crate::execute_approved::build_deploy_plan(&args.job_name, &args.ddl_bytes);
    let impact_summary =
        crate::impact::guarded_write_impact("deploy_ddl", &connection, &args.ddl_bytes);
    let executed = execute_guarded_sql(
        cx,
        live_runtime,
        &connection,
        "deploy_ddl",
        &plan.submit_block,
    )
    .await?;
    Ok(serde_json::json!({
        "kind": "submitted",
        "connection": connection,
        "plan": plan,
        "impact_summary": impact_summary,
        "rows_affected": executed.rows_affected,
        "audit_record": executed.audit_record,
    }))
}

fn live_read_session<'a>(
    live_runtime: &'a LiveDbRuntime,
    tool: &str,
    connection: Option<&str>,
) -> Result<&'a crate::LiveDbSession, Box<ErrorEnvelope>> {
    match connection.map(str::trim) {
        Some("") => Err(Box::new(invalid_arguments_envelope(
            tool,
            "`connection`, when supplied, must not be empty",
        ))),
        Some(connection) => live_runtime
            .session(connection)
            .map_err(|err| Box::new(live_runtime_error_envelope(err, tool))),
        None => live_runtime
            .active_session()
            .map_err(|err| Box::new(live_runtime_error_envelope(err, tool))),
    }
}

fn serialize_live_response<T: serde::Serialize>(tool: &str, response: T) -> Value {
    serde_json::to_value(response).unwrap_or_else(|err| {
        serde_json::json!({
            "serialization_error": format!("tool `{tool}` response could not be serialized: {err}")
        })
    })
}

async fn run_list_objects_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: ListObjectsArgs =
        parse_args("list_objects", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "list_objects", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "list_objects")?;
    let response = crate::list_objects::run_list_objects(cx, &adapter, &args.into_request())
        .await
        .map_err(|err| Box::new(list_objects_error_envelope(err, "list_objects")))?;
    checkpoint(cx, "list_objects")?;
    Ok(serialize_live_response("list_objects", response))
}

async fn run_describe_table_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: OwnerNameArgs =
        parse_args("describe_table", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "describe_table", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "describe_table")?;
    let response = crate::describe::run_describe_table(cx, &adapter, &args.owner, &args.name)
        .await
        .map_err(|err| Box::new(err.to_envelope(&[])))?;
    checkpoint(cx, "describe_table")?;
    Ok(serialize_live_response("describe_table", response))
}

async fn run_describe_view_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: DescribeViewArgs =
        parse_args("describe_view", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "describe_view", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "describe_view")?;
    let response = crate::describe::run_describe_view(
        cx,
        &adapter,
        &args.owner,
        &args.name,
        args.text_preview_chars,
    )
    .await
    .map_err(|err| Box::new(err.to_envelope(&[])))?;
    checkpoint(cx, "describe_view")?;
    Ok(serialize_live_response("describe_view", response))
}

async fn run_describe_trigger_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: OwnerNameArgs =
        parse_args("describe_trigger", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "describe_trigger", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "describe_trigger")?;
    let response = crate::describe::run_describe_trigger(cx, &adapter, &args.owner, &args.name)
        .await
        .map_err(|err| Box::new(err.to_envelope(&[])))?;
    checkpoint(cx, "describe_trigger")?;
    Ok(serialize_live_response("describe_trigger", response))
}

async fn run_describe_index_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: OwnerNameArgs =
        parse_args("describe_index", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "describe_index", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "describe_index")?;
    let response = crate::describe::run_describe_index(cx, &adapter, &args.owner, &args.name)
        .await
        .map_err(|err| Box::new(err.to_envelope(&[])))?;
    checkpoint(cx, "describe_index")?;
    Ok(serialize_live_response("describe_index", response))
}

async fn run_get_object_source_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: GetObjectSourceArgs =
        parse_args("get_object_source", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(
        live_runtime,
        "get_object_source",
        args.connection.as_deref(),
    )?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    checkpoint(cx, "get_object_source")?;
    let response = crate::source::run_get_object_source(
        cx,
        &adapter,
        &args.owner,
        &args.object_name,
        &args.object_type,
    )
    .await
    .map_err(|err| Box::new(source_tool_error_envelope(err, "get_object_source")))?;
    checkpoint(cx, "get_object_source")?;
    Ok(serialize_live_response("get_object_source", response))
}

async fn run_get_errors_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: GetErrorsArgs =
        parse_args("get_errors", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    let session = live_read_session(live_runtime, "get_errors", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    let owner = args.owner.unwrap_or_default();
    checkpoint(cx, "get_errors")?;
    let response = crate::source::run_get_errors(cx, &adapter, &owner, &args.object_name)
        .await
        .map_err(|err| Box::new(source_tool_error_envelope(err, "get_errors")))?;
    checkpoint(cx, "get_errors")?;
    Ok(serialize_live_response("get_errors", response))
}

async fn run_get_clob_live(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: GetClobArgs =
        parse_args("get_clob", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    crate::query::ensure_read_only_query(&args.sql)
        .map_err(|err| Box::new(query_error_envelope(&err)))?;
    let session = live_read_session(live_runtime, "get_clob", args.connection.as_deref())?;
    let adapter =
        OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(session.connection()));
    let recorder = Arc::new(crate::query::StatementObjectRecorder::default());
    crate::query::ensure_read_only_query_with_oracle(&args.sql, recorder.clone())
        .map_err(|err| Box::new(query_error_envelope(&err)))?;
    let side_effect_oracle = adapter
        .side_effect_oracle(cx, &recorder.base_objects())
        .await
        .map_err(|err| Box::new(catalog_error_envelope(err, "get_clob")))?;
    crate::query::ensure_read_only_query_with_oracle(&args.sql, Arc::new(side_effect_oracle))
        .map_err(|err| Box::new(query_error_envelope(&err)))?;
    let params = args.params.as_deref().unwrap_or(&[]);
    checkpoint(cx, "get_clob")?;
    let response = crate::source::run_get_clob(cx, &adapter, &args.sql, params, args.max_chars)
        .await
        .map_err(|err| Box::new(source_tool_error_envelope(err, "get_clob")))?;
    checkpoint(cx, "get_clob")?;
    Ok(serialize_live_response("get_clob", response))
}

async fn run_query_live(
    cx: &Cx,
    context: PlsqlDispatchContext<'_>,
    request_budget: RequestBudget,
    live_runtime: &LiveDbRuntime,
    arguments: &Value,
) -> Result<Value, Box<ErrorEnvelope>> {
    let args: QueryArgs =
        parse_args("query", arguments).map_err(|err| Box::new(err.into_envelope()))?;
    crate::query::ensure_read_only_query(&args.sql)
        .map_err(|err| Box::new(query_error_envelope(&err)))?;

    let session = match args.connection.as_deref().map(str::trim) {
        Some("") => {
            return Err(Box::new(invalid_arguments_envelope(
                "query",
                "`connection`, when supplied, must not be empty",
            )));
        }
        Some(connection) => live_runtime
            .session(connection)
            .map_err(|err| Box::new(live_runtime_error_envelope(err, "query")))?,
        None => live_runtime
            .active_session()
            .map_err(|err| Box::new(live_runtime_error_envelope(err, "query")))?,
    };

    request_budget
        .enforce(cx)
        .map_err(|err| Box::new(db_error_envelope(err, "query")))?;

    let connection = session.connection();
    let restore = install_request_call_timeout(cx, request_budget, connection)?;
    let adapter = OraclemcpCatalogConnection::new(BorrowedOracleConnection::new(connection));
    let read_cx = context.narrow_to_read_path(cx);
    let rows_result = async {
        // oraclemcp-db 0.4.0 still accepts `&Cx`, so keep passing the upstream
        // trait shape while narrowing any ambient `Cx::current()` lookups during
        // the live read.
        let _read_path_guard = read_cx.set_current_restricted();
        checkpoint(cx, "query")?;
        let recorder = Arc::new(crate::query::StatementObjectRecorder::default());
        crate::query::ensure_read_only_query_with_oracle(&args.sql, recorder.clone())
            .map_err(|err| Box::new(query_error_envelope(&err)))?;
        let side_effect_oracle = adapter
            .side_effect_oracle(cx, &recorder.base_objects())
            .await
            .map_err(|err| Box::new(query_error_envelope(&QueryError::Backend(err))))?;
        crate::query::ensure_read_only_query_with_oracle(&args.sql, Arc::new(side_effect_oracle))
            .map_err(|err| Box::new(query_error_envelope(&err)))?;
        checkpoint(cx, "query")?;
        let rows = match adapter.query_rows(cx, &args.sql, &[]).await {
            Ok(rows) => rows,
            Err(err) => {
                if let Err(budget_err) = request_budget.enforce(cx) {
                    return Err(Box::new(db_error_envelope(budget_err, "query")));
                }
                return Err(Box::new(query_error_envelope(&QueryError::Backend(err))));
            }
        };
        request_budget
            .enforce(cx)
            .map_err(|err| Box::new(db_error_envelope(err, "query")))?;
        checkpoint(cx, "query")?;
        Ok(rows)
    }
    .await;
    let restore_result = restore_request_call_timeout(connection, restore);
    let rows = match (rows_result, restore_result) {
        (Ok(rows), Ok(())) => rows,
        (Err(err), Ok(())) => return Err(err),
        (Ok(_), Err(err)) => return Err(err),
        (Err(err), Err(_)) => return Err(err),
    };

    Ok(serde_json::to_value(crate::query::query_response_from_rows(
        rows,
        args.lob_truncation_chars,
    ))
    .unwrap_or(Value::Object(Default::default())))
}

struct GuardedExecutionResult {
    rows_affected: u64,
    audit_record: AuditRecord,
}

async fn execute_guarded_sql(
    cx: &Cx,
    live_runtime: &LiveDbRuntime,
    connection: &str,
    tool_name: &str,
    sql: &str,
) -> Result<GuardedExecutionResult, Box<ErrorEnvelope>> {
    let session = live_runtime
        .session(connection)
        .map_err(|err| Box::new(live_runtime_error_envelope(err, tool_name)))?;
    if !session.safety().writes_allowed() {
        return Err(Box::new(
            ErrorEnvelope::new(
                ErrorClass::OperatingLevelTooLow,
                format!(
                    "tool `{tool_name}` refused: connection `{connection}` is not write-enabled"
                ),
            )
            .with_suggested_tool("enable_writes")
            .with_next_step(
                "Call enable_writes with a fresh operator confirmation token before executing DDL.",
            ),
        ));
    }

    let audit_record = append_guarded_audit(live_runtime, tool_name, sql, "DDL")?;
    checkpoint(cx, tool_name)?;

    let audit_plan = AuditPlan::for_tool(default_audit_client(), tool_name);
    let connection_handle = session.connection();
    run_audit_session_markers(cx, connection_handle, &audit_plan, tool_name).await?;
    let annotated = audit_plan.annotate(sql);
    let rows_affected = connection_handle
        .execute(cx, &annotated, &[])
        .await
        .map_err(|err| Box::new(db_error_envelope(err, tool_name)))?;
    connection_handle
        .commit(cx)
        .await
        .map_err(|err| Box::new(db_error_envelope(err, tool_name)))?;
    checkpoint(cx, tool_name)?;
    Ok(GuardedExecutionResult {
        rows_affected,
        audit_record,
    })
}

async fn run_audit_session_markers(
    cx: &Cx,
    connection: &dyn oraclemcp_db::OracleConnection,
    audit_plan: &AuditPlan,
    tool_name: &str,
) -> Result<(), Box<ErrorEnvelope>> {
    let (module_sql, module_params) = audit_plan.set_module_sql();
    let module_binds = module_params
        .into_iter()
        .map(oraclemcp_db::OracleBind::String)
        .collect::<Vec<_>>();
    connection
        .execute(cx, module_sql, &module_binds)
        .await
        .map_err(|err| Box::new(db_error_envelope(err, tool_name)))?;

    let (action_sql, action_params) = audit_plan.set_action_sql();
    let action_binds = action_params
        .into_iter()
        .map(oraclemcp_db::OracleBind::String)
        .collect::<Vec<_>>();
    connection
        .execute(cx, action_sql, &action_binds)
        .await
        .map_err(|err| Box::new(db_error_envelope(err, tool_name)))?;
    Ok(())
}

fn append_guarded_audit(
    live_runtime: &LiveDbRuntime,
    tool_name: &str,
    sql: &str,
    danger_level: &str,
) -> Result<AuditRecord, Box<ErrorEnvelope>> {
    let Some(audit) = live_runtime.guarded_audit() else {
        return Err(Box::new(
            ErrorEnvelope::new(
                ErrorClass::OperatingLevelTooLow,
                format!(
                    "tool `{tool_name}` refused: guarded-write audit is not configured"
                ),
            )
            .with_next_step(format!(
                "Set `{}` and `{}` before starting plsql-mcp serve; guarded writes fail closed without a signed audit sink.",
                crate::GUARDED_AUDIT_FILE_ENV,
                crate::GUARDED_AUDIT_KEY_ENV
            )),
        ));
    };
    let client = default_audit_client();
    audit
        .append(GuardedAuditDraft {
            client: &client,
            tool_name,
            sql,
            danger_level,
            decision: AuditDecision::Allowed,
            outcome: AuditOutcome::Pending,
            rows_affected: None,
        })
        .map_err(|err| {
            Box::new(
                ErrorEnvelope::new(
                    ErrorClass::Internal,
                    format!("guarded audit append failed before `{tool_name}` executed: {err}"),
                )
                .with_next_step(
                    "Inspect the audit sink path/key configuration and rerun audit verification before retrying the write.",
                ),
            )
        })
}

fn safety_profile_json(session: &crate::LiveDbSession) -> Value {
    let safety = session.safety();
    serde_json::json!({
        "connection": session.profile().name.clone(),
        "profile": safety.profile.as_str(),
        "session_writes_enabled": safety.session_writes_enabled,
        "permanently_read_only": safety.permanently_read_only,
        "active_enable_writes_token": safety.active_token.as_ref().map(|token| {
            serde_json::json!({
                "operation_summary": token.operation_summary.clone(),
                "issued_at": token.issued_at,
                "ttl_seconds": token.ttl_seconds,
                "expired": token.is_expired(),
            })
        }),
    })
}

fn safety_error_envelope(err: SafetyProfileError, tool: &str) -> ErrorEnvelope {
    let class = match &err {
        SafetyProfileError::PermanentlyReadOnly { .. }
        | SafetyProfileError::EnableWritesTokenMissing { .. }
        | SafetyProfileError::EnableWritesTokenMismatch => ErrorClass::OperatingLevelTooLow,
        SafetyProfileError::Unknown { .. } | SafetyProfileError::AlreadyReadOnly => {
            ErrorClass::InvalidArguments
        }
    };
    ErrorEnvelope::new(class, err.to_string()).with_next_step(format!(
        "Inspect current_safety_profile, then retry `{tool}` with the required safety state."
    ))
}

fn mint_preview_token() -> Result<String, Box<ErrorEnvelope>> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|err| {
        Box::new(
            ErrorEnvelope::new(
                ErrorClass::Internal,
                format!("failed to mint approval token: {err}"),
            )
            .with_next_step("Retry the dry-run after the OS random source is available."),
        )
    })?;
    Ok(hex_bytes(&bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn default_audit_client() -> AuditClient {
    AuditClient::new("mcp-client", "unknown-model", "local-session")
}

fn unix_now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[derive(Clone, Copy, Debug)]
struct CallTimeoutRestore {
    previous: Option<Duration>,
}

fn install_request_call_timeout(
    cx: &Cx,
    request_budget: RequestBudget,
    connection: &dyn oraclemcp_db::OracleConnection,
) -> Result<Option<CallTimeoutRestore>, Box<ErrorEnvelope>> {
    let Some(remaining) = request_budget_call_timeout(cx, request_budget) else {
        return Ok(None);
    };
    let previous = connection
        .call_timeout()
        .map_err(|err| Box::new(db_error_envelope(err, "query")))?;
    let effective = previous.map_or(remaining, |existing| existing.min(remaining));
    connection
        .set_call_timeout(Some(effective))
        .map_err(|err| Box::new(db_error_envelope(err, "query")))?;
    Ok(Some(CallTimeoutRestore { previous }))
}

fn restore_request_call_timeout(
    connection: &dyn oraclemcp_db::OracleConnection,
    restore: Option<CallTimeoutRestore>,
) -> Result<(), Box<ErrorEnvelope>> {
    let Some(restore) = restore else {
        return Ok(());
    };
    connection
        .set_call_timeout(restore.previous)
        .map_err(|err| Box::new(db_error_envelope(err, "query")))
}

fn request_budget_call_timeout(cx: &Cx, request_budget: RequestBudget) -> Option<Duration> {
    let deadline = request_budget.budget().deadline?;
    let remaining = Duration::from_nanos(deadline.duration_since(cx.now()));
    // Oracle call timeouts are millisecond-granularity downstream; preserve
    // sub-millisecond budgets as a cancellable 1ms backend timeout and let
    // RequestBudget checkpoints enforce the exact budget boundary.
    Some(remaining.max(Duration::from_millis(1)))
}

struct BorrowedOracleConnection<'a> {
    inner: &'a dyn oraclemcp_db::OracleConnection,
}

impl<'a> BorrowedOracleConnection<'a> {
    fn new(inner: &'a dyn oraclemcp_db::OracleConnection) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait(?Send)]
impl oraclemcp_db::OracleConnection for BorrowedOracleConnection<'_> {
    fn backend(&self) -> oraclemcp_db::OracleBackend {
        self.inner.backend()
    }

    async fn ping(&self, cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
        self.inner.ping(cx).await
    }

    async fn describe(
        &self,
        cx: &Cx,
    ) -> Result<oraclemcp_db::OracleConnectionInfo, oraclemcp_db::DbError> {
        self.inner.describe(cx).await
    }

    async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        binds: &[oraclemcp_db::OracleBind],
    ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
        self.inner.query_rows(cx, sql, binds).await
    }

    async fn query_rows_with_serialize_options(
        &self,
        cx: &Cx,
        sql: &str,
        binds: &[oraclemcp_db::OracleBind],
        serialize_opts: &oraclemcp_db::SerializeOptions,
    ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
        self.inner
            .query_rows_with_serialize_options(cx, sql, binds, serialize_opts)
            .await
    }

    async fn execute(
        &self,
        cx: &Cx,
        sql: &str,
        binds: &[oraclemcp_db::OracleBind],
    ) -> Result<u64, oraclemcp_db::DbError> {
        self.inner.execute(cx, sql, binds).await
    }

    async fn commit(&self, cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
        self.inner.commit(cx).await
    }

    async fn rollback(&self, cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
        self.inner.rollback(cx).await
    }

    fn call_timeout(&self) -> Result<Option<Duration>, oraclemcp_db::DbError> {
        self.inner.call_timeout()
    }

    fn set_call_timeout(&self, timeout: Option<Duration>) -> Result<(), oraclemcp_db::DbError> {
        self.inner.set_call_timeout(timeout)
    }
}

fn checkpoint(cx: &Cx, tool: &str) -> Result<(), Box<ErrorEnvelope>> {
    cx.checkpoint().map_err(|err| {
        Box::new(ErrorEnvelope::new(
            ErrorClass::Timeout,
            format!("tool `{tool}` was cancelled at an Asupersync checkpoint: {err}"),
        ))
    })
}

fn live_runtime_error_envelope(err: LiveRuntimeError, tool: &str) -> ErrorEnvelope {
    match err {
        LiveRuntimeError::NoActiveConnection | LiveRuntimeError::UnknownConnection { .. } => {
            let kind = if matches!(
                tool,
                "query"
                    | "list_objects"
                    | "describe_table"
                    | "describe_view"
                    | "describe_trigger"
                    | "describe_index"
                    | "get_object_source"
                    | "get_errors"
                    | "get_clob"
            ) {
                RuntimeKind::LiveConnection
            } else {
                RuntimeKind::SessionState
            };
            runtime_state_envelope(kind, tool)
        }
        LiveRuntimeError::StaleLease { .. } => {
            ErrorEnvelope::new(ErrorClass::LeaseRequired, format!("{err}"))
                .with_next_step("Call `current_database` to refresh session state, then retry.")
        }
        other => ErrorEnvelope::new(ErrorClass::Internal, other.to_string()),
    }
}

fn invalid_arguments_envelope(tool: &str, detail: &str) -> ErrorEnvelope {
    ErrorEnvelope::new(
        ErrorClass::InvalidArguments,
        format!("invalid arguments for tool `{tool}`: {detail}"),
    )
    .with_next_step(format!(
        "Inspect `{tool}`'s inputSchema in tools/list and supply the required fields."
    ))
}

fn db_connection_error_envelope(err: oraclemcp_db::DbError, tool: &str) -> ErrorEnvelope {
    ErrorEnvelope::new(
        ErrorClass::ConnectionFailed,
        format!("{tool} could not open an Oracle session: {err}"),
    )
    .with_suggested_tool("connect")
    .with_next_step(
        "Verify the connect_string, username/password or external-auth settings, then retry connect.",
    )
}

fn db_error_envelope(err: oraclemcp_db::DbError, tool: &str) -> ErrorEnvelope {
    let envelope = err.into_envelope();
    if matches!(envelope.error_class, ErrorClass::Timeout) {
        return envelope.with_next_step(format!(
            "The `{tool}` call exhausted its request budget or was cancelled; retry with a fresh \
             request budget or narrow the query."
        ));
    }
    envelope
}

fn query_error_envelope(err: &QueryError) -> ErrorEnvelope {
    err.to_envelope(None, &[])
}

fn list_objects_error_envelope(
    err: crate::list_objects::ListObjectsError,
    tool: &str,
) -> ErrorEnvelope {
    match err {
        crate::list_objects::ListObjectsError::InvalidCursor { .. } => {
            invalid_arguments_envelope(tool, &err.to_string())
        }
        crate::list_objects::ListObjectsError::Backend(err) => catalog_error_envelope(err, tool),
    }
}

fn source_tool_error_envelope(err: crate::source::SourceToolError, tool: &str) -> ErrorEnvelope {
    match err {
        crate::source::SourceToolError::Backend(err) => catalog_error_envelope(err, tool),
    }
}

fn catalog_error_envelope(err: CatalogError, tool: &str) -> ErrorEnvelope {
    let message = err.to_string();
    match err {
        CatalogError::OracleBackendError { message, .. } => enrich_oracle_error(&message, None, &[])
            .with_next_step(format!(
                "The `{tool}` dictionary read failed in Oracle; verify the object name, schema, and current user's dictionary privileges."
            )),
        _ => ErrorEnvelope::new(
            ErrorClass::Internal,
            format!("tool `{tool}` failed while reading Oracle dictionary metadata: {message}"),
        )
        .with_next_step("Retry after narrowing the request or reconnecting to the target database."),
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
    sql: String,
    #[serde(default)]
    connection: Option<String>,
    #[serde(default)]
    lob_truncation_chars: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ListObjectsArgs {
    #[serde(default)]
    connection: Option<String>,
    #[serde(default)]
    object_type: Option<String>,
    #[serde(default)]
    name_pattern: Option<String>,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    page_size: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
}

impl ListObjectsArgs {
    fn into_request(self) -> crate::ListObjectsRequest {
        crate::ListObjectsRequest {
            object_type: self.object_type,
            name_pattern: self.name_pattern,
            schema: self.schema,
            page_size: self.page_size,
            cursor: self.cursor,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerNameArgs {
    #[serde(default)]
    connection: Option<String>,
    owner: String,
    name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeViewArgs {
    #[serde(default)]
    connection: Option<String>,
    owner: String,
    name: String,
    #[serde(default)]
    text_preview_chars: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GetObjectSourceArgs {
    #[serde(default)]
    connection: Option<String>,
    owner: String,
    object_name: String,
    object_type: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GetErrorsArgs {
    #[serde(default)]
    connection: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    object_name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GetClobArgs {
    sql: String,
    #[serde(default)]
    connection: Option<String>,
    #[serde(default)]
    params: Option<Vec<OracleBind>>,
    #[serde(default)]
    max_chars: Option<usize>,
}

/// Argument shape for `set_safety_profile`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SetSafetyProfileArgs {
    profile: crate::SafetyProfile,
}

/// Argument shape for `enable_writes`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EnableWritesArgs {
    token: String,
}

/// Argument shape for `connect`.
///
/// D.3 keeps this intentionally explicit: a name-only call re-activates an
/// existing in-process session, while opening a new session requires the
/// connect material in this request. Profile-file secret resolution lands in a
/// later loader bead; this path must not invent an implicit credential source.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectArgs {
    name: String,
    #[serde(default)]
    connect_string: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permanently_read_only: bool,
    #[serde(default)]
    external_auth: bool,
}

/// Argument shape for `deploy_ddl` — validates the two fields the
/// `build_deploy_plan` surface needs before gating on the runtime.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeployDdlArgs {
    job_name: String,
    ddl_bytes: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::Budget;
    use asupersync::lab::{DporExplorer, ExplorerConfig, LabRuntime};
    use asupersync::types::CancelKind;
    use serde_json::json;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

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

    fn dispatch_with_runtime_for_test(
        name: &str,
        args: Value,
        live_runtime: &mut LiveDbRuntime,
    ) -> Result<Value, Box<ErrorEnvelope>> {
        dispatch_with_runtime_and_budget_for_test(name, args, live_runtime, |cx| {
            RequestBudget::from_budget(cx.budget())
        })
    }

    fn dispatch_with_runtime_and_budget_for_test(
        name: &str,
        args: Value,
        live_runtime: &mut LiveDbRuntime,
        request_budget: impl FnOnce(&Cx) -> RequestBudget,
    ) -> Result<Value, Box<ErrorEnvelope>> {
        dispatch_with_runtime_context_and_budget_for_test(
            name,
            args,
            live_runtime,
            DispatchContext::default(),
            request_budget,
        )
    }

    fn dispatch_with_runtime_context_and_budget_for_test<'a>(
        name: &str,
        args: Value,
        live_runtime: &mut LiveDbRuntime,
        core_context: DispatchContext<'a>,
        request_budget: impl FnOnce(&Cx) -> RequestBudget,
    ) -> Result<Value, Box<ErrorEnvelope>> {
        let reactor = asupersync::runtime::reactor::create_reactor().unwrap();
        let runtime = asupersync::runtime::RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .unwrap();
        runtime.block_on(async {
            let cx = Cx::current().expect("block_on installs a request Cx");
            let context = PlsqlDispatchContext::new(core_context, request_budget(&cx));
            dispatch_tool_with_runtime(&cx, context, live_runtime, name, args)
                .await
                .map_err(Box::new)
        })
    }

    fn dispatch_with_runtime_on_cx_for_test(
        cx: &Cx,
        name: &str,
        args: Value,
        live_runtime: &mut LiveDbRuntime,
    ) -> Result<Value, Box<ErrorEnvelope>> {
        let reactor = asupersync::runtime::reactor::create_reactor().unwrap();
        let runtime = asupersync::runtime::RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .unwrap();
        runtime.block_on(async {
            let context = PlsqlDispatchContext::from_cx(cx, DispatchContext::default());
            dispatch_tool_with_runtime(cx, context, live_runtime, name, args)
                .await
                .map_err(Box::new)
        })
    }

    fn live_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile {
            name: String::from(name),
            description: Some(String::from("test profile")),
            connect_string: String::from("//localhost/FREEPDB1"),
            username: Some(String::from("system")),
            permanently_read_only: false,
            dbtools_alias: None,
        }
    }

    fn active_recording_runtime() -> (LiveDbRuntime, RecordingOracleConnection) {
        let mut live_runtime = LiveDbRuntime::new();
        let connection =
            RecordingOracleConnection::with_initial_timeout(Some(Duration::from_secs(30)));
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("stub session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");
        (live_runtime, observed)
    }

    fn assert_live_session_reusable(
        live_runtime: &mut LiveDbRuntime,
        observed: &RecordingOracleConnection,
    ) {
        assert_eq!(live_runtime.len(), 1, "live session count leaked");
        assert_eq!(live_runtime.active_name(), Some("dev"));
        live_runtime
            .active_session()
            .expect("ready-or-dead invariant keeps session ready");
        assert_eq!(
            observed.current_timeout(),
            Some(Duration::from_secs(30)),
            "request-scoped timeout leaked into the live session"
        );

        let result = dispatch_with_runtime_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            live_runtime,
        )
        .expect("ready session remains reusable after cancellation/timeout/drop");
        assert_eq!(result["rows"][0]["cells"][0]["value"], "1");
        assert_eq!(live_runtime.len(), 1, "recovery query leaked a session");
        assert_eq!(live_runtime.active_name(), Some("dev"));
        assert_eq!(
            observed.current_timeout(),
            Some(Duration::from_secs(30)),
            "recovery query must preserve the session timeout"
        );
    }

    #[derive(Clone, Copy, Debug)]
    enum LiveDispatchDrainScenario {
        CancelBeforeDispatch,
        TimeoutBeforeQuery,
        DropUnpolledFuture,
    }

    impl LiveDispatchDrainScenario {
        const ALL: [Self; 3] = [
            Self::CancelBeforeDispatch,
            Self::TimeoutBeforeQuery,
            Self::DropUnpolledFuture,
        ];

        fn exercise(self) {
            let (mut live_runtime, observed) = active_recording_runtime();
            match self {
                Self::CancelBeforeDispatch => {
                    let cx = Cx::for_testing();
                    cx.cancel_with(CancelKind::User, Some("deterministic live-dispatch cancel"));
                    let err = dispatch_with_runtime_on_cx_for_test(
                        &cx,
                        "query",
                        json!({"sql": "SELECT 1 AS val FROM dual"}),
                        &mut live_runtime,
                    )
                    .unwrap_err();

                    assert_eq!(err.error_class, ErrorClass::Timeout);
                    assert!(
                        observed.observed_queries().is_empty(),
                        "cancelled request must not reach Oracle"
                    );
                    assert!(
                        observed.observed_timeout_sets().is_empty(),
                        "cancelled request must not mutate session call_timeout"
                    );
                }
                Self::TimeoutBeforeQuery => {
                    let err = dispatch_with_runtime_and_budget_for_test(
                        "query",
                        json!({"sql": "SELECT 1 AS val FROM dual"}),
                        &mut live_runtime,
                        |_| RequestBudget::from_budget(Budget::ZERO),
                    )
                    .unwrap_err();

                    assert_eq!(err.error_class, ErrorClass::Timeout);
                    assert!(
                        observed.observed_queries().is_empty(),
                        "exhausted budget must fail before touching Oracle"
                    );
                    assert!(
                        observed.observed_timeout_sets().is_empty(),
                        "exhausted budget must not mutate session call_timeout"
                    );
                }
                Self::DropUnpolledFuture => {
                    let cx = Cx::for_testing();
                    let context = PlsqlDispatchContext::new(
                        DispatchContext::default(),
                        RequestBudget::from_budget(cx.budget()),
                    );
                    let future = dispatch_tool_with_runtime(
                        &cx,
                        context,
                        &mut live_runtime,
                        "query",
                        json!({"sql": "SELECT 1 AS val FROM dual"}),
                    );
                    drop(future);

                    assert!(
                        observed.observed_queries().is_empty(),
                        "dropping an unpolled dispatch future must not reach Oracle"
                    );
                    assert!(
                        observed.observed_timeout_sets().is_empty(),
                        "dropping an unpolled dispatch future must not mutate call_timeout"
                    );
                }
            }

            assert_live_session_reusable(&mut live_runtime, &observed);
        }
    }

    fn run_live_dispatch_lab_probe(runtime: &mut LabRuntime) {
        let root = runtime.state.create_root_region(Budget::INFINITE);
        const CHECKPOINTS: [(&str, &str); 3] = [
            (
                "live-dispatch-drain-start-cancel",
                "live-dispatch-drain-finish-cancel",
            ),
            (
                "live-dispatch-drain-start-timeout",
                "live-dispatch-drain-finish-timeout",
            ),
            (
                "live-dispatch-drain-start-drop",
                "live-dispatch-drain-finish-drop",
            ),
        ];
        for (start, finish) in CHECKPOINTS {
            let (task, _handle) = runtime
                .state
                .create_task(root, Budget::INFINITE, async move {
                    let cx = Cx::current().expect("LabRuntime installs a task Cx");
                    cx.checkpoint_with(start)
                        .expect("lab probe starts uncancelled");
                    cx.checkpoint_with(finish)
                        .expect("lab probe drains uncancelled");
                })
                .expect("lab task creates");
            runtime.scheduler.lock().schedule(task, 0);
        }
        runtime.run_until_quiescent();
        assert!(runtime.is_quiescent(), "lab runtime must drain all tasks");
    }

    #[derive(Debug, Clone)]
    struct RecordingOracleConnection {
        queries: Arc<Mutex<Vec<String>>>,
        executes: Arc<Mutex<Vec<String>>>,
        metadata_queries: Arc<Mutex<Vec<String>>>,
        query_timeouts: Arc<Mutex<Vec<Option<Duration>>>>,
        ambient_remote_seen: Arc<Mutex<Vec<Option<bool>>>>,
        current_timeout: Arc<Mutex<Option<Duration>>>,
        timeout_sets: Arc<Mutex<Vec<Option<Duration>>>>,
        side_effecting_objects: Arc<Mutex<HashSet<(String, String)>>>,
        query_delay: Option<Duration>,
        query_error: Option<oraclemcp_db::DbError>,
    }

    impl RecordingOracleConnection {
        fn new() -> Self {
            Self::with_initial_timeout(None)
        }

        fn with_initial_timeout(timeout: Option<Duration>) -> Self {
            Self::with_initial_timeout_and_query_delay(timeout, None)
        }

        fn with_initial_timeout_and_query_delay(
            timeout: Option<Duration>,
            query_delay: Option<Duration>,
        ) -> Self {
            Self::with_initial_timeout_delay_and_error(timeout, query_delay, None)
        }

        fn with_initial_timeout_delay_and_error(
            timeout: Option<Duration>,
            query_delay: Option<Duration>,
            query_error: Option<oraclemcp_db::DbError>,
        ) -> Self {
            Self {
                queries: Arc::new(Mutex::new(Vec::new())),
                executes: Arc::new(Mutex::new(Vec::new())),
                metadata_queries: Arc::new(Mutex::new(Vec::new())),
                query_timeouts: Arc::new(Mutex::new(Vec::new())),
                ambient_remote_seen: Arc::new(Mutex::new(Vec::new())),
                current_timeout: Arc::new(Mutex::new(timeout)),
                timeout_sets: Arc::new(Mutex::new(Vec::new())),
                side_effecting_objects: Arc::new(Mutex::new(HashSet::new())),
                query_delay,
                query_error,
            }
        }

        fn with_side_effecting_object(owner: &str, name: &str) -> Self {
            let connection = Self::new();
            connection
                .side_effecting_objects
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert((owner.to_ascii_uppercase(), name.to_ascii_uppercase()));
            connection
        }

        fn observed_queries(&self) -> Vec<String> {
            self.queries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn observed_executes(&self) -> Vec<String> {
            self.executes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn observed_metadata_queries(&self) -> Vec<String> {
            self.metadata_queries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn observed_query_timeouts(&self) -> Vec<Option<Duration>> {
            self.query_timeouts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn observed_ambient_remote_seen(&self) -> Vec<Option<bool>> {
            self.ambient_remote_seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn observed_timeout_sets(&self) -> Vec<Option<Duration>> {
            self.timeout_sets
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn current_timeout(&self) -> Option<Duration> {
            *self
                .current_timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }
    }

    #[async_trait::async_trait(?Send)]
    impl oraclemcp_db::OracleConnection for RecordingOracleConnection {
        fn backend(&self) -> oraclemcp_db::OracleBackend {
            oraclemcp_db::OracleBackend::RustOracle
        }

        async fn ping(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn describe(
            &self,
            _cx: &Cx,
        ) -> Result<oraclemcp_db::OracleConnectionInfo, oraclemcp_db::DbError> {
            self.ambient_remote_seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(Cx::current().map(|cx| cx.has_remote()));
            Ok(oraclemcp_db::OracleConnectionInfo {
                backend: Some(oraclemcp_db::OracleBackend::RustOracle),
                current_schema: Some(String::from("SYSTEM")),
                server_version: Some(String::from("23ai")),
                ..oraclemcp_db::OracleConnectionInfo::default()
            })
        }

        async fn query_rows(
            &self,
            _cx: &Cx,
            sql: &str,
            binds: &[oraclemcp_db::OracleBind],
        ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
            let timeout = self.current_timeout();
            self.query_timeouts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(timeout);
            self.ambient_remote_seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(Cx::current().map(|cx| cx.has_remote()));
            if sql.contains("from all_policies") {
                self.metadata_queries
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(String::from(sql));
                let (owner, name) = match binds {
                    [
                        oraclemcp_db::OracleBind::String(owner),
                        oraclemcp_db::OracleBind::String(name),
                    ] => (owner.to_ascii_uppercase(), name.to_ascii_uppercase()),
                    _ => {
                        return Err(oraclemcp_db::DbError::Query(String::from(
                            "side-effect query expected owner/name string binds",
                        )));
                    }
                };
                let side_effecting = self
                    .side_effecting_objects
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .contains(&(owner, name));
                return Ok(vec![oraclemcp_db::OracleRow {
                    columns: vec![(
                        String::from("SIDE_EFFECTING"),
                        oraclemcp_db::OracleCell::new(
                            "NUMBER",
                            Some(if side_effecting { "1" } else { "0" }.to_string()),
                        ),
                    )],
                }]);
            }
            if let Some(delay) = self.query_delay {
                std::thread::sleep(delay);
            }
            self.queries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(String::from(sql));
            if let Some(err) = self.query_error.clone() {
                return Err(err);
            }
            Ok(vec![oraclemcp_db::OracleRow {
                columns: vec![(
                    String::from("VAL"),
                    oraclemcp_db::OracleCell::new("NUMBER", Some(String::from("1"))),
                )],
            }])
        }

        async fn execute(
            &self,
            _cx: &Cx,
            sql: &str,
            _binds: &[oraclemcp_db::OracleBind],
        ) -> Result<u64, oraclemcp_db::DbError> {
            self.executes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(String::from(sql));
            Ok(1)
        }

        async fn commit(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn rollback(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        fn call_timeout(&self) -> Result<Option<Duration>, oraclemcp_db::DbError> {
            Ok(self.current_timeout())
        }

        fn set_call_timeout(&self, timeout: Option<Duration>) -> Result<(), oraclemcp_db::DbError> {
            *self
                .current_timeout
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = timeout;
            self.timeout_sets
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(timeout);
            Ok(())
        }
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

            crate::requires_privileged_effect(&cx);
        });
    }

    #[test]
    fn live_catalog_reads_install_read_path_ambient_caps() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::new();
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("test session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let full_cx = Cx::for_testing_with_remote(asupersync::RemoteCap::new());
        assert!(
            full_cx.has_remote(),
            "test setup must start from a privileged caller context"
        );

        let current = dispatch_with_runtime_on_cx_for_test(
            &full_cx,
            "current_database",
            json!({}),
            &mut live_runtime,
        )
        .expect("current_database runs through live runtime");
        assert_eq!(current["active"]["catalog"]["current_schema"], "SYSTEM");

        let query = dispatch_with_runtime_on_cx_for_test(
            &full_cx,
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
        )
        .expect("query runs through live runtime");

        assert_eq!(query["rows"][0]["cells"][0]["value"], "1");
        assert_eq!(
            observed.observed_ambient_remote_seen(),
            vec![Some(false), Some(false), Some(false), Some(false)],
            "catalog read loaders must hide remote capability from ambient Cx lookups"
        );
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
    fn connect_without_existing_session_is_invalid_arguments_not_runtime_state_required() {
        let mut live_runtime = LiveDbRuntime::new();
        let err =
            dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
                .unwrap_err();
        assert_eq!(err.error_class, ErrorClass::InvalidArguments);
        assert!(
            err.message.contains("connect_string"),
            "connect must explain the missing connect material: {err:?}"
        );
    }

    #[test]
    fn query_after_connect_uses_active_upstream_session() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::new();
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("test session inserts");

        let connected =
            dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
                .expect("connect activates existing live session");
        assert_eq!(connected["connected"], true);
        assert_eq!(connected["active"], "dev");

        let result = dispatch_with_runtime_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
        )
        .expect("query runs through live runtime");
        assert_eq!(result["columns"][0]["name"], "VAL");
        assert_eq!(result["rows"][0]["cells"][0]["value"], "1");
        assert_eq!(
            observed.observed_queries(),
            vec!["SELECT 1 AS val FROM dual"]
        );
        assert_eq!(observed.observed_metadata_queries().len(), 1);
    }

    #[test]
    fn guarded_create_or_replace_apply_audits_before_execute() {
        use oraclemcp_audit::{MemoryAuditSink, SigningKey, VerifyOutcome, verify_records};

        struct SharedSink(Arc<MemoryAuditSink>);
        impl oraclemcp_audit::AuditSink for SharedSink {
            fn append(
                &self,
                record: &oraclemcp_audit::AuditRecord,
            ) -> Result<(), oraclemcp_audit::AuditError> {
                self.0.append(record)
            }

            fn flush(&self) -> Result<(), oraclemcp_audit::AuditError> {
                self.0.flush()
            }
        }

        let sink = Arc::new(MemoryAuditSink::new());
        let mut live_runtime =
            LiveDbRuntime::with_guarded_audit(crate::GuardedAudit::from_sink_for_test(
                Box::new(SharedSink(Arc::clone(&sink))),
                "k-dispatch",
                b"dispatch-audit-key".to_vec(),
            ));
        let connection = RecordingOracleConnection::new();
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("test session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");
        dispatch_with_runtime_for_test(
            "set_safety_profile",
            json!({"profile": "ddl_guarded"}),
            &mut live_runtime,
        )
        .expect("ddl_guarded profile enables previews");

        let ddl = "CREATE OR REPLACE VIEW BILLING.V_AUDIT AS SELECT 1 AS id FROM dual";
        let dry_run = dispatch_with_runtime_for_test(
            "create_or_replace",
            json!({
                "connection": "dev",
                "operation_summary": "replace BILLING.V_AUDIT",
                "ddl_bytes": ddl,
                "mode": {"mode": "dry_run"}
            }),
            &mut live_runtime,
        )
        .expect("dry-run mints approval token");
        assert_eq!(
            dry_run["impact_summary"]["schema_id"],
            "plsql.mcp.guarded_write_impact"
        );
        assert_eq!(
            dry_run["impact_summary"]["target"]["name"], "V_AUDIT",
            "dry-run must show the target before the approval token is spent"
        );
        assert_eq!(
            dry_run["impact_summary"]["change_impact"]["payload"]["summary"]["invalidation_count"],
            1
        );
        let approval_token = dry_run["token"].as_str().expect("approval token");

        let write_token = live_runtime
            .active_session_mut()
            .expect("active session")
            .mint_enable_writes_token("replace BILLING.V_AUDIT", "write-ok")
            .expect("mint write token");
        dispatch_with_runtime_for_test(
            "enable_writes",
            json!({"token": write_token.token}),
            &mut live_runtime,
        )
        .expect("enable_writes is audited and succeeds");

        let applied = dispatch_with_runtime_for_test(
            "create_or_replace",
            json!({
                "connection": "dev",
                "operation_summary": "replace BILLING.V_AUDIT",
                "ddl_bytes": ddl,
                "mode": {"mode": "apply", "token": approval_token}
            }),
            &mut live_runtime,
        )
        .expect("apply executes through guarded audit");

        assert_eq!(applied["kind"], "apply");
        assert_eq!(applied["audit_record"]["tool"], "create_or_replace");
        assert_eq!(
            applied["impact_summary"]["target"]["object_type"], "VIEW",
            "apply response carries the same typed blast-radius payload"
        );
        assert_eq!(sink.flush_count(), 2, "enable + DDL both fsync");
        assert_eq!(
            verify_records(
                &sink.records(),
                &[SigningKey::new(
                    "k-dispatch",
                    b"dispatch-audit-key".to_vec()
                )],
            ),
            VerifyOutcome::Ok { records: 2 }
        );

        let executes = observed.observed_executes();
        assert!(
            executes
                .iter()
                .any(|sql| sql.contains("dbms_application_info.set_module")),
            "write path should set module/action markers: {executes:?}"
        );
        let ddl_exec = executes
            .iter()
            .find(|sql| sql.contains("CREATE OR REPLACE VIEW BILLING.V_AUDIT"))
            .expect("DDL executed");
        assert!(
            ddl_exec.contains("/* plsql-mcp create_or_replace local-session unknown-model */"),
            "DDL must carry the SQL audit marker: {ddl_exec}"
        );
    }

    #[test]
    fn guarded_write_without_audit_fails_before_execute() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::new();
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("test session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");
        {
            let safety = live_runtime
                .active_session_mut()
                .expect("active session")
                .safety_mut();
            safety.profile = crate::SafetyProfile::SessionWriteEnabled;
            safety.session_writes_enabled = true;
        }
        live_runtime
            .preview_registry_mut()
            .preview_sql(
                "dev",
                "replace view",
                "CREATE OR REPLACE VIEW V_NO_AUDIT AS SELECT 1 FROM dual",
                "tok-no-audit",
            )
            .expect("seed preview");

        let err = dispatch_with_runtime_for_test(
            "create_or_replace",
            json!({
                "connection": "dev",
                "operation_summary": "replace view",
                "ddl_bytes": "CREATE OR REPLACE VIEW V_NO_AUDIT AS SELECT 1 FROM dual",
                "mode": {"mode": "apply", "token": "tok-no-audit"}
            }),
            &mut live_runtime,
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::OperatingLevelTooLow);
        assert!(
            err.message
                .contains("guarded-write audit is not configured"),
            "missing audit must be explicit: {err:?}"
        );
        assert!(
            observed.observed_executes().is_empty(),
            "no Oracle execute before durable audit is configured"
        );
    }

    #[test]
    fn query_side_effect_oracle_blocks_before_user_sql() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection =
            RecordingOracleConnection::with_side_effecting_object("SYSTEM", "AUDIT_LOG");
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("test session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let err = dispatch_with_runtime_for_test(
            "query",
            json!({"sql": "SELECT id FROM audit_log"}),
            &mut live_runtime,
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::ForbiddenStatement);
        assert_eq!(
            observed.observed_queries(),
            Vec::<String>::new(),
            "side-effecting metadata must block before the user query executes"
        );
        assert_eq!(observed.observed_metadata_queries().len(), 1);
    }

    #[test]
    fn oauth_read_scope_blocks_write_capable_tool_before_runtime_state() {
        let mut live_runtime = LiveDbRuntime::new();
        let grant = oraclemcp_core::ScopeGrant(vec![String::from("oracle:read")]);
        let err = dispatch_with_runtime_context_and_budget_for_test(
            "deploy_ddl",
            json!({"job_name": "DEPLOY_APP", "ddl_bytes": "CREATE OR REPLACE VIEW v AS SELECT 1 FROM dual"}),
            &mut live_runtime,
            DispatchContext::with_scope_grant(&grant),
            |cx| RequestBudget::from_budget(cx.budget()),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::OperatingLevelTooLow);
        assert!(
            err.message.contains("OAuth scope ceiling is READ_ONLY"),
            "scope refusal must name the effective ceiling: {err:?}"
        );
    }

    #[test]
    fn oauth_ddl_scope_preserves_existing_runtime_state_gate() {
        let mut live_runtime = LiveDbRuntime::new();
        let grant = oraclemcp_core::ScopeGrant(vec![String::from("oracle:ddl")]);
        let err = dispatch_with_runtime_context_and_budget_for_test(
            "deploy_ddl",
            json!({"job_name": "DEPLOY_APP", "ddl_bytes": "CREATE OR REPLACE VIEW v AS SELECT 1 FROM dual"}),
            &mut live_runtime,
            DispatchContext::with_scope_grant(&grant),
            |cx| RequestBudget::from_budget(cx.budget()),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::RuntimeStateRequired);
        assert_eq!(err.suggested_tool.as_deref(), Some("connect"));
    }

    #[test]
    fn oauth_execute_scope_allows_write_step_but_not_ddl_tool() {
        let mut live_runtime = LiveDbRuntime::new();
        let grant = oraclemcp_core::ScopeGrant(vec![String::from("oracle:execute")]);
        let enable_err = dispatch_with_runtime_context_and_budget_for_test(
            "enable_writes",
            json!({}),
            &mut live_runtime,
            DispatchContext::with_scope_grant(&grant),
            |cx| RequestBudget::from_budget(cx.budget()),
        )
        .unwrap_err();
        assert_eq!(enable_err.error_class, ErrorClass::RuntimeStateRequired);

        let ddl_err = dispatch_with_runtime_context_and_budget_for_test(
            "deploy_ddl",
            json!({"job_name": "DEPLOY_APP", "ddl_bytes": "CREATE OR REPLACE VIEW v AS SELECT 1 FROM dual"}),
            &mut live_runtime,
            DispatchContext::with_scope_grant(&grant),
            |cx| RequestBudget::from_budget(cx.budget()),
        )
        .unwrap_err();
        assert_eq!(ddl_err.error_class, ErrorClass::OperatingLevelTooLow);
        assert!(
            ddl_err
                .message
                .contains("OAuth scope ceiling is READ_WRITE"),
            "execute scope must not authorize DDL: {ddl_err:?}"
        );
    }

    #[test]
    fn oauth_scope_cannot_raise_active_safety_profile_ceiling() {
        let mut live_runtime = LiveDbRuntime::new();
        live_runtime
            .insert_connected(
                live_profile("dev"),
                Box::new(RecordingOracleConnection::new()),
            )
            .expect("test session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");
        assert_eq!(
            live_runtime
                .active_session()
                .expect("active session")
                .safety()
                .profile,
            crate::SafetyProfile::InspectOnly
        );

        let grant = oraclemcp_core::ScopeGrant(vec![String::from("oracle:admin")]);
        let err = dispatch_with_runtime_context_and_budget_for_test(
            "deploy_ddl",
            json!({"job_name": "DEPLOY_APP", "ddl_bytes": "CREATE OR REPLACE VIEW v AS SELECT 1 FROM dual"}),
            &mut live_runtime,
            DispatchContext::with_scope_grant(&grant),
            |cx| RequestBudget::from_budget(cx.budget()),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::OperatingLevelTooLow);
        assert!(
            err.message.contains("OAuth scope ceiling is READ_ONLY"),
            "scope must not raise the active inspect-only safety profile: {err:?}"
        );
    }

    #[test]
    fn live_query_applies_request_budget_timeout_and_restores_session_timeout() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection =
            RecordingOracleConnection::with_initial_timeout(Some(Duration::from_secs(30)));
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("stub session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let result = dispatch_with_runtime_and_budget_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
            |cx| RequestBudget::from_call_timeout(cx.now(), Some(Duration::from_millis(250))),
        )
        .expect("query runs with budget timeout");

        assert_eq!(result["rows"][0]["cells"][0]["value"], "1");
        assert_eq!(observed.current_timeout(), Some(Duration::from_secs(30)));
        let sets = observed.observed_timeout_sets();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets.get(1), Some(&Some(Duration::from_secs(30))));
        let applied = sets
            .first()
            .copied()
            .flatten()
            .expect("budget installs a finite call timeout");
        assert!(applied > Duration::ZERO);
        assert!(applied <= Duration::from_millis(250));

        let query_timeouts = observed.observed_query_timeouts();
        assert_eq!(query_timeouts.len(), 2);
        assert_eq!(query_timeouts[0], Some(applied));
        assert_eq!(query_timeouts[1], Some(applied));
    }

    #[test]
    fn live_query_that_outlives_request_budget_returns_timeout_and_restores_timeout() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::with_initial_timeout_and_query_delay(
            Some(Duration::from_secs(30)),
            Some(Duration::from_millis(75)),
        );
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("stub session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let err = dispatch_with_runtime_and_budget_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
            |cx| RequestBudget::from_call_timeout(cx.now(), Some(Duration::from_millis(25))),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::Timeout);
        assert_eq!(
            observed.observed_queries(),
            vec!["SELECT 1 AS val FROM dual"]
        );
        assert_eq!(observed.current_timeout(), Some(Duration::from_secs(30)));
        let sets = observed.observed_timeout_sets();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets.get(1), Some(&Some(Duration::from_secs(30))));
        let applied = sets
            .first()
            .copied()
            .flatten()
            .expect("budget installs a finite call timeout");
        assert!(applied > Duration::ZERO);
        assert!(applied <= Duration::from_millis(25));
    }

    #[test]
    fn live_query_backend_error_after_budget_expiry_returns_timeout() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::with_initial_timeout_delay_and_error(
            Some(Duration::from_secs(30)),
            Some(Duration::from_millis(75)),
            Some(oraclemcp_db::DbError::Query(String::from(
                "ORA-01013: user requested cancel of current operation",
            ))),
        );
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("stub session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let err = dispatch_with_runtime_and_budget_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
            |cx| RequestBudget::from_call_timeout(cx.now(), Some(Duration::from_millis(25))),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::Timeout);
        assert_eq!(
            observed.observed_queries(),
            vec!["SELECT 1 AS val FROM dual"]
        );
        assert_eq!(observed.current_timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn live_query_exhausted_request_budget_fails_before_query() {
        let mut live_runtime = LiveDbRuntime::new();
        let connection = RecordingOracleConnection::new();
        let observed = connection.clone();
        live_runtime
            .insert_connected(live_profile("dev"), Box::new(connection))
            .expect("stub session inserts");
        dispatch_with_runtime_for_test("connect", json!({"name": "dev"}), &mut live_runtime)
            .expect("connect activates existing live session");

        let err = dispatch_with_runtime_and_budget_for_test(
            "query",
            json!({"sql": "SELECT 1 AS val FROM dual"}),
            &mut live_runtime,
            |_| RequestBudget::from_budget(asupersync::Budget::ZERO),
        )
        .unwrap_err();

        assert_eq!(err.error_class, ErrorClass::Timeout);
        assert!(
            observed.observed_queries().is_empty(),
            "exhausted budget must fail before touching Oracle"
        );
        assert!(
            observed.observed_timeout_sets().is_empty(),
            "exhausted budget must not mutate session call timeout"
        );
    }

    #[test]
    fn dpor_live_dispatch_cancel_timeout_drop_leave_session_reusable() {
        let mut explorer = DporExplorer::new(
            ExplorerConfig::new(0x5eed_1506, 8)
                .worker_count(2)
                .max_steps(10_000),
        );
        let report = explorer.explore(|runtime| {
            run_live_dispatch_lab_probe(runtime);
            for scenario in LiveDispatchDrainScenario::ALL {
                scenario.exercise();
            }
        });

        assert!(
            !report.has_violations(),
            "LabRuntime found invariant violations for seeds {:?}: {report:?}",
            report.violation_seeds()
        );
        assert!(report.total_runs > 0, "DPOR explorer must execute a run");
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
