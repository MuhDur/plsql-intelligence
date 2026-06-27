#![forbid(unsafe_code)]

//! Model Context Protocol server for the PL/SQL Intelligence engine.
//!
//! `plsql-mcp` is a single-binary MCP server that speaks JSON-RPC 2.0
//! over stdio (default) or TCP (`serve --listen <host:port>`) and exposes
//! the PL/SQL Intelligence engine as a structured tool surface an AI
//! agent can call. The canonical surface — built by
//! [`default_tool_registry`] — is fully populated: foundation
//! static-analysis tools (parsing, project analysis, dependency graph
//! queries, change analysis, SARIF rendering, doc lookup) plus, when the
//! `live-db` Cargo feature is enabled, the read-only-by-default live
//! Oracle tool surface (connection / safety management, schema
//! describe, query, audit-emitting DDL with previewed approval tokens).
//!
//! ## Module layout
//!
//! - `config` — runtime configuration: transport, safety profile,
//!   connection profile, audit posture.
//! - `safety` — read-only-by-default guard, named safety profiles
//!   (`static_only`, `inspect_only`, `ddl_guarded`,
//!   `session_write_enabled`), and the `permanently_read_only` hard
//!   guard.
//! - `tools` — typed [`ToolRegistry`] / [`ToolDescriptor`]; the
//!   canonical registry lives in [`default_tool_registry`].
//! - `dispatch` — `tools/call` dispatcher with a tri-state outcome
//!   (`Ran` / `RuntimeStateRequired` / `DispatchError`) so the protocol
//!   layer never silently no-ops an unknown tool.
//! - `mcp_protocol` — JSON-RPC 2.0 request / response handling and the
//!   MCP `initialize` / `tools/list` / `tools/call` surface.
//! - `tcp` — TCP accept loop and the shared `process_stream` pump the
//!   stdio path reuses; loopback-only by default, wider binds require
//!   `--allow-public-bind`.
//! - `doctor` — diagnostic report (transport, live-DB backend posture,
//!   connection write-posture, profile sanity) consumed by both the
//!   `doctor` subcommand and the `--robot-triage` mega-object.
//! - `connections` — named connection profiles loaded from
//!   `~/.plsql-mcp/connections.toml`, with structural
//!   [`DbToolsAlias::probe`] for optional `~/.dbtools` mirroring.
//! - `live_runtime` — stateful connected-session runtime: opened
//!   `oraclemcp-db` connections, active-session leases, safety state,
//!   and preview approvals.
//!
//! ## License
//!
//! Apache-2.0 OR MIT.

pub mod analyze_project;
pub mod audit;
pub mod change_tools;
pub mod compile;
pub mod config;
pub mod connections;
pub mod create_or_replace;
pub mod cross_schema;
pub mod describe;
pub mod dispatch;
pub mod doctor;
pub mod execute_approved;
pub mod foundation_tools;
pub mod graph_tools;
pub mod list_objects;
pub mod live_runtime;
pub mod mcp_protocol;
pub mod oraclemcp_catalog;
pub mod parse_tools;
pub mod patch;
pub mod plsql_analyze;
pub mod preview;
pub mod query;
pub mod safety;
pub mod source;
pub mod tcp;
pub mod tools;
pub mod trust;

pub use oraclemcp_catalog::OraclemcpCatalogConnection;
pub use oraclemcp_core::{
    PrivilegedEffect, ReadPathCaps, RequestBudget, narrow_to_read_path, requires_privileged_effect,
};

pub use analyze_project::{
    AnalyzeProjectError, AnalyzeProjectRequest, AnalyzeProjectResponse,
    register_analyze_project_tool, run_analyze_project,
};
pub use change_tools::{
    ChangeToolError, register_change_tools, run_classify_change, run_compare_oracle_deps,
    run_explain_lifecycle, run_orphan_candidates, run_recompile_plan, run_release_gate,
    run_sarif_scan, run_what_breaks,
};
pub use create_or_replace::{
    CreateOrReplaceError, CreateOrReplaceMode, CreateOrReplaceRequest, CreateOrReplaceResponse,
    classify_kind, run_create_or_replace,
};
pub use cross_schema::{
    CrossSchemaConfirmation, CrossSchemaDecision, CrossSchemaError,
    require_cross_schema_confirmation,
};
pub use execute_approved::{
    ApprovedExecutionPlan, DeployDdlPlan, ExecuteApprovedError, ExecuteApprovedRequest,
    build_deploy_plan, consume_approved, run_execute_approved,
};
pub use foundation_tools::{
    CompletenessReportRequest, CompletenessReportResponse, DocLookupRequest, DocLookupResponse,
    DynamicSqlEvidenceRequest, DynamicSqlEvidenceResponse, FoundationToolError,
    register_foundation_tools, run_completeness_report, run_doc_lookup, run_dynamic_sql_evidence,
};
pub use graph_tools::{
    DependenciesResponse, GraphQueryRequest, GraphToolError, NeighborhoodResponse,
    register_graph_tools, run_find_callees, run_find_callers, run_get_dependencies,
};
pub use parse_tools::{
    CompileCheckRequest, CompileCheckResponse, GetSymbolRequest, GetSymbolResponse,
    InspectProfileResponse, ParseFileRequest, ParseFileResponse, register_parse_tools,
    run_compile_check, run_get_symbol, run_inspect_profile, run_parse_file,
};
pub use plsql_analyze::{
    CallRef, ComplexityInfo, LintFinding, PlsqlAnalyzeError, PlsqlAnalyzeRequest,
    PlsqlAnalyzeResponse, RoutineInfo, register_plsql_analyze_tool, run_plsql_analyze,
};
pub use trust::{TrustBlock, attach_trust_block, trust_block_value};

/// Register the `execute_approved` + `deploy_ddl` tool descriptors.
pub fn register_execute_approved_tools(registry: &mut ToolRegistry) {
    registry.register(
        ToolDescriptor::new(
            "execute_approved",
            ToolTier::FoundationLiveDb,
            "Run a previously-previewed DDL statement under its approval token. Verifies the supplied bytes against the previewed payload byte-for-byte and runs the cross-schema typed-name guard before returning the execution plan. Prerequisites: a prior dry_run (create_or_replace / patch_package / patch_view) minted the 60s approval token, and the session is write-enabled (connect → enable_writes); call within 60s of the dry_run or re-preview.",
        )
        .destructive(),
    );
    registry.register(
        ToolDescriptor::new(
            "deploy_ddl",
            ToolTier::FoundationLiveDb,
            "Lock-free DDL deployment via a one-shot DBMS_SCHEDULER PLSQL_BLOCK job. Returns the submit block + the USER_SCHEDULER_JOB_RUN_DETAILS poll query.",
        )
        .destructive(),
    );
}
pub use patch::{
    PackagePart, PatchMode, PatchPackageError, PatchPackageRequest, PatchPackageResponse,
    PatchViewError, PatchViewRequest, PatchViewResponse, run_patch_package, run_patch_view,
    synthesise_ddl, synthesise_view_ddl,
};

/// Register the `patch_view` tool descriptor.
pub fn register_patch_view_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolDescriptor::new(
            "patch_view",
            ToolTier::FoundationLiveDb,
            "Targeted view replacement. `dry_run` synthesises CREATE OR REPLACE VIEW <schema>.<name> AS … and mints a 60s approval token; `apply` verifies the supplied query byte-for-byte against the previewed payload before returning the executable DDL.",
        )
        .destructive(),
    );
}
pub use mcp_protocol::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION, PlsqlMcpServer,
    ServerInitError, handle_request, handle_request_line,
};
pub use preview::{PreviewError, PreviewRegistry, PreviewedDdl};

/// Register the `create_or_replace` tool descriptor.
pub fn register_create_or_replace_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolDescriptor::new(
            "create_or_replace",
            ToolTier::FoundationLiveDb,
            "Full-DDL deployment under per-operation approval. Accepts CREATE OR REPLACE … for PACKAGE [BODY] / PROCEDURE / FUNCTION / TRIGGER / VIEW / TYPE [BODY] / SYNONYM / LIBRARY. Guarded-write workflow: connect → enable_writes (consumes the single-use operator token) → this tool with mode=dry_run (mints a 60s approval token) → mode=apply (verifies the supplied bytes against the previewed payload byte-for-byte under that token before returning the executable DDL). The approval token expires 60s after dry_run.",
        )
        .destructive(),
    );
}

/// Register the `patch_package` tool descriptor.
pub fn register_patch_package_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolDescriptor::new(
            "patch_package",
            ToolTier::FoundationLiveDb,
            "Targeted REPLACE-based package edit. `dry_run` synthesises CREATE OR REPLACE PACKAGE [BODY] DDL and mints a 60s approval token via the preview registry; `apply` verifies the supplied source byte-for-byte against the previewed payload before returning the executable DDL.",
        )
        .destructive(),
    );
}

pub use describe::{
    DescribeColumn, DescribeConstraint, DescribeError, DescribeIndex, DescribeIndexResponse,
    DescribeTableResponse, DescribeTriggerResponse, DescribeViewResponse, run_describe_index,
    run_describe_table, run_describe_trigger, run_describe_view,
};

pub use dispatch::{DispatchError, DispatchOutcome, RuntimeKind, dispatch_table, dispatch_tool};

pub use compile::{
    CompileToolError, CompileWithWarningsResponse, WarningCategory, categorize_error,
    run_compile_with_warnings,
};

pub use source::{
    GetClobResponse, GetErrorsResponse, GetObjectSourceResponse, ObjectError, SourceToolError,
    run_get_clob, run_get_errors, run_get_object_source,
};

pub use list_objects::{
    DEFAULT_PAGE_SIZE, ListObjectsEntry, ListObjectsError, ListObjectsRequest, ListObjectsResponse,
    MAX_PAGE_SIZE, run_list_objects,
};

pub use live_runtime::{
    BoxedOracleConnection, LiveDbRuntime, LiveDbSession, LiveRuntimeError, LiveSessionLease,
};

pub use query::{
    QueryCell, QueryColumnMeta, QueryError, QueryResponse, QueryRow, UNTRUSTED_DATA_NOTICE,
    run_query, sanitize,
};

/// Register the read-only `query` tool descriptor.
pub fn register_query_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolDescriptor::new(
            "query",
            ToolTier::FoundationLiveDb,
            "Run a SELECT / WITH against the active Oracle connection and return structured rows. Result cells are untrusted data: markup-shaped sequences are structurally neutralized (casing/spacing/unicode-robust) and the response carries an explicit data-envelope notice; LOB cells truncate to a per-call limit.",
        )
        .with_input_schema(serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sql"],
            "properties": {
                "sql": {"type": "string", "description": "A read-only SELECT / WITH statement. Writes/DDL are rejected by the SQL guard."},
                "connection": {"type": ["string", "null"], "description": "Optional named connection profile; defaults to the active connection."},
                "lob_truncation_chars": {"type": ["integer", "null"], "minimum": 0, "description": "Per-cell LOB truncation limit for this call."},
            },
        })),
    );
}

pub use audit::{APPLICATION_MODULE, AuditClient, AuditPlan, AuditSink};

pub use connections::{
    ConnectionError, ConnectionListEntry, ConnectionProfile, ConnectionRegistry, DbToolsAlias,
};
pub use doctor::{DoctorReport, doctor_report};
pub use safety::{
    ENABLE_WRITES_TOKEN_TTL_SECONDS, EnableWritesToken, SafetyProfile, SafetyProfileError,
    SessionSafetyState,
};
// Re-exported because it is the type of the public `EnableWritesToken::deadline`
// field; without this re-export external callers (and integration tests) cannot
// name the field's type to construct or match an `EnableWritesToken`.
pub use oraclemcp_guard::MonotonicDeadline;
pub use tools::{ToolDescriptor, ToolRegistry, ToolTier};

/// Build the canonical tool registry the `serve` command exposes.
///
/// `ToolRegistry::new()` is intentionally *empty* — it is the bare
/// container the per-module `register_*` helpers populate. Until this
/// function existed every caller wired tools ad-hoc (the doctor, the
/// scripted-client test, the protocol unit tests), which meant
/// `plsql-mcp serve` would have advertised **zero** tools. This is the
/// single source of truth for the surface a live MCP client sees: every
/// static-analysis tool (parsing, project analysis, graph queries,
/// change analysis) plus the full live-DB descriptor set (connection,
/// safety, inspection, and guarded-write tools). `ToolRegistry::register`
/// deduplicates by name, so the order here is irrelevant and re-calling is
/// idempotent.
#[must_use]
pub fn default_tool_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    // Zero-arg discovery — the session-orientation entry point an agent calls
    // FIRST to learn feature flags + static-vs-live guidance (oracle-da9j.3).
    r.register(
        ToolDescriptor::new(
            "oracle_capabilities",
            ToolTier::FoundationStatic,
            "Zero-arg session-orientation report: build feature flags (live-db on/off), the \
             tool-surface size, static-vs-live guidance, and next_actions. Call this (and \
             tools/list) FIRST to plan a session.",
        )
        .with_input_schema(serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        })),
    );
    // Static-analysis tools (no project, no DB) — always safe to call.
    register_parse_tools(&mut r);
    register_analyze_project_tool(&mut r);
    register_plsql_analyze_tool(&mut r);
    register_graph_tools(&mut r);
    register_foundation_tools(&mut r);
    register_change_tools(&mut r);
    // Foundation live-DB descriptors — discoverable so an agent can plan a
    // session; their execution stays gated by the safety profile.
    register_connection_tools(&mut r);
    register_safety_tools(&mut r);
    register_query_tool(&mut r);
    register_patch_package_tool(&mut r);
    register_patch_view_tool(&mut r);
    register_create_or_replace_tool(&mut r);
    register_execute_approved_tools(&mut r);
    r
}

/// Register the four safety-state tool descriptors into the given
/// registry. Tools are `FoundationLiveDb` tier and gate every write
/// the live-DB surface emits.
pub fn register_safety_tools(registry: &mut ToolRegistry) {
    let descriptors = [
        (
            "current_safety_profile",
            "Return the active named safety profile (static_only / inspect_only / ddl_guarded / session_write_enabled), permanently_read_only flag, and any active enable_writes token TTL.",
        ),
        (
            "set_safety_profile",
            "Switch to a named safety profile. Refused when the active connection is permanently_read_only and the target would allow writes.",
        ),
        (
            "enable_writes",
            "Consume a single-use operator confirmation token to flip the session into session_write_enabled. Token TTL: 60s.",
        ),
        (
            "disable_writes",
            "Drop write privilege and revert to inspect_only. Idempotent for read-only sessions.",
        ),
    ];
    for (name, summary) in descriptors {
        let descriptor = ToolDescriptor::new(name, ToolTier::FoundationLiveDb, summary);
        let descriptor = match name {
            "connect" => descriptor.with_input_schema(serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string", "description": "Stable in-process connection name to activate."},
                        "connect_string": {"type": ["string", "null"], "description": "Oracle Net connect identifier. Required when opening a new session; omitted when re-activating an existing live session."},
                        "username": {"type": ["string", "null"], "description": "Oracle username, or null for wallet/external authentication."},
                        "password": {"type": ["string", "null"], "description": "Oracle password for this request. Never returned in responses."},
                        "description": {"type": ["string", "null"], "description": "Optional operator-facing profile description."},
                        "permanently_read_only": {"type": "boolean", "default": false, "description": "When true, this session refuses enable_writes for its lifetime."},
                        "external_auth": {"type": "boolean", "default": false, "description": "Use external/wallet authentication instead of password auth."}
                    }
                })),
            _ => descriptor,
        };
        registry.register(descriptor);
    }
}

/// Register the five connection-management tool descriptors into the
/// given tool registry. Idempotent — the underlying [`ToolRegistry`]
/// deduplicates by name.
pub fn register_connection_tools(registry: &mut ToolRegistry) {
    let descriptors = [
        (
            "list_connections",
            "List named Oracle connection profiles available to the agent.",
        ),
        (
            "connect",
            "Activate a named connection profile (mirrors plsql-catalog's OracleConnection).",
        ),
        ("disconnect", "Clear the active connection profile."),
        (
            "current_database",
            "Report the active connection profile, safety profile, and audit posture.",
        ),
        (
            "switch_database",
            "Switch the active connection profile in a single round trip.",
        ),
    ];
    for (name, summary) in descriptors {
        registry.register(ToolDescriptor::new(
            name,
            ToolTier::FoundationLiveDb,
            summary,
        ));
    }
}
