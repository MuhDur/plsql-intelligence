//! MCP stdio protocol layer.
//!
//! MCP (Model Context Protocol) wraps JSON-RPC 2.0 over stdio. This
//! module is the pure protocol layer: it parses request frames,
//! dispatches the recognised methods (`initialize`, `tools/list`,
//! `tools/call`), and produces response frames. It does NOT own
//! the actual stdin/stdout handles — the binary at
//! `crates/plsql-mcp/src/main.rs` wraps these calls in a read /
//! process / write loop.
//!
//! Keeping the protocol layer pure means every recognised method
//! is unit-testable without an I/O harness, and the runtime loop
//! reduces to "read a line of JSON, hand it to
//! `handle_request_line`, write back the response line".
//!
//! ## Frame shape
//!
//! Per MCP convention every frame is a single-line JSON object:
//!
//! ```json
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
//! ```
//!
//! Notifications (no `id`) are routed to `handle_notification` and
//! produce no response.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL routing — the per-tool dispatch
//!   defers to the `ToolRegistry` populated by the foundation and
//!   live-DB tool registrations. This module is the transport shim
//!   above those tools, not an Oracle behaviour change.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::safety::SafetyProfile;
use crate::tools::{ToolDescriptor, ToolRegistry, ToolTier};

/// MCP protocol version this implementation negotiates. Clients
/// that advertise a higher version receive a `version_mismatch`
/// error response from `handle_initialize`.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC 2.0 request envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response envelope. Always carries `jsonrpc` +
/// `id`; exactly one of `result` / `error` is set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            // Every successful response carries the Trust Block
            // (MCP-007 / §1.5). Centralised here so a new tool
            // can never ship without it.
            result: Some(crate::trust::attach_trust_block(result)),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// A JSON-RPC error whose `data` carries a structured [`ErrorEnvelope`]
    /// (error_class + fuzzy_matches + suggested_tool + next_steps) so an agent
    /// can self-heal in one round instead of parsing a bare string
    /// (oracle-da9j.2). The standard `code`/`message` are preserved.
    fn err_with_data(id: Value, code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: Some(data),
            }),
        }
    }
}

/// Dispatch a single JSON-RPC frame against `registry`. Returns:
/// * `Some(response)` for requests — caller writes one line.
/// * `None` for notifications — caller does nothing.
#[must_use]
pub fn handle_request(req: &JsonRpcRequest, registry: &ToolRegistry) -> Option<JsonRpcResponse> {
    if req.jsonrpc != "2.0" {
        if let Some(id) = req.id.clone() {
            return Some(JsonRpcResponse::err(
                id,
                -32600,
                format!("invalid jsonrpc version: {:?}", req.jsonrpc),
            ));
        }
        return None;
    }
    let Some(id) = req.id.clone() else {
        // Notifications produce no response; we accept them silently.
        handle_notification(&req.method);
        return None;
    };

    match req.method.as_str() {
        "initialize" => Some(handle_initialize(id, req.params.as_ref())),
        "tools/list" => Some(handle_tools_list(id, registry)),
        "tools/call" => Some(handle_tools_call(id, req.params.as_ref(), registry)),
        "ping" => Some(JsonRpcResponse::ok(id, Value::Object(Default::default()))),
        other => Some(JsonRpcResponse::err(
            id,
            -32601,
            format!("method not found: {other}"),
        )),
    }
}

fn handle_notification(_method: &str) {
    // No-op for now. `initialized` (the MCP post-init notification)
    // lands here in a future bead if we need to track session
    // state.
}

/// Parse a single JSON-RPC line and dispatch it. Glue used by the
/// runtime loop in `main.rs`. Errors during parsing produce a
/// response with `id = null`, per JSON-RPC convention.
#[must_use]
pub fn handle_request_line(line: &str, registry: &ToolRegistry) -> Option<JsonRpcResponse> {
    match serde_json::from_str::<JsonRpcRequest>(line) {
        Ok(req) => handle_request(&req, registry),
        Err(err) => Some(JsonRpcResponse::err(
            Value::Null,
            -32700,
            format!("parse error: {err}"),
        )),
    }
}

fn handle_initialize(id: Value, params: Option<&Value>) -> JsonRpcResponse {
    let requested_version = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    if requested_version != PROTOCOL_VERSION {
        return JsonRpcResponse::err(
            id,
            -32602,
            format!(
                "protocol version mismatch: server supports {PROTOCOL_VERSION}, client requested {requested_version}"
            ),
        );
    }
    let result = serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "serverInfo": {
            "name": "plsql-mcp",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "tools": { "listChanged": false }
        },
        // Orient the agent before its first tool call (oracle-da9j.3): the
        // zero-arg discovery tool + tools/list together report the feature
        // flags, the tool surface, and each tool's argument schema.
        "instructions": "Call the zero-arg `oracle_capabilities` tool and `tools/list` FIRST: \
                         they report the build feature flags (live-db on/off), the tool surface, \
                         and each tool's argument inputSchema + readOnlyHint/destructiveHint. \
                         Static-analysis tools run with no database; live-DB tools require an \
                         active `connect`."
    });
    JsonRpcResponse::ok(id, result)
}

fn handle_tools_list(id: Value, registry: &ToolRegistry) -> JsonRpcResponse {
    let tools: Vec<Value> = registry.tools.iter().map(tool_to_mcp_value).collect();
    JsonRpcResponse::ok(
        id,
        serde_json::json!({
            "tools": tools,
        }),
    )
}

/// Whether a tool should be advertised as CALLABLE on the current wire — given
/// the build's `live-db` feature and the active safety profile — plus a human
/// reason when it is not (oracle-da9j.4). Foundation-static tools are always
/// callable; a FoundationLiveDb tool needs the live-db feature AND a profile
/// permitting its operation, so a static-only build / inspect-only session no
/// longer advertises ~37% of the surface as plainly callable when every such
/// call would return RuntimeStateRequired or a profile refusal. The tool stays
/// LISTED (discoverable, so an agent can still plan) but flagged `available:false`.
fn tool_availability(t: &ToolDescriptor, live_db: bool, profile: SafetyProfile) -> (bool, Option<String>) {
    if matches!(t.tier, ToolTier::FoundationStatic) {
        return (true, None);
    }
    if !live_db {
        return (
            false,
            Some("requires the `live-db` build feature (this build is static-only)".to_string()),
        );
    }
    if t.destructive {
        if profile.allows_ddl_preview() {
            (true, None)
        } else {
            (
                false,
                Some(format!(
                    "requires a write-capable safety profile (current: {}); call set_safety_profile / enable_writes first",
                    profile.as_str()
                )),
            )
        }
    } else if profile.allows_read_only_live_tools() {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "requires a live safety profile (current: {})",
                profile.as_str()
            )),
        )
    }
}

fn tool_to_mcp_value(t: &ToolDescriptor) -> Value {
    // Advertise the tool's real argument schema when it has one, so an agent can
    // construct a valid call first-try instead of probing -32602 InvalidArguments
    // (oracle-da9j.1); fall back to the permissive object otherwise. Surface
    // destructive intent via the MCP-standard tool annotations (readOnlyHint /
    // destructiveHint) so an agent can isolate the write cluster from read-only
    // tools (oracle-da9j.9).
    let input_schema = t
        .input_schema
        .clone()
        .unwrap_or_else(|| serde_json::json!({ "type": "object", "additionalProperties": true }));
    // Gate the advertised callability by the build feature + the default
    // (inspect-only) safety posture of the pure protocol wire, so a static-only
    // build does not present the live/write surface as plainly callable
    // (oracle-da9j.4). The tool stays listed; `available:false` + a reason tell
    // the agent why a call would be refused here.
    let (available, reason) =
        tool_availability(t, cfg!(feature = "live-db"), SafetyProfile::default());
    let mut annotations = serde_json::json!({
        "readOnlyHint": !t.destructive,
        "destructiveHint": t.destructive,
        "available": available,
    });
    if let Some(why) = reason {
        annotations["unavailableReason"] = Value::String(why);
    }
    serde_json::json!({
        "name": t.name,
        "description": t.summary,
        "inputSchema": input_schema,
        "annotations": annotations,
    })
}

fn handle_tools_call(
    id: Value,
    params: Option<&Value>,
    registry: &ToolRegistry,
) -> JsonRpcResponse {
    use crate::dispatch::{DispatchError, DispatchOutcome, dispatch_tool};

    let Some(params) = params else {
        return JsonRpcResponse::err(id, -32602, "tools/call requires params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return JsonRpcResponse::err(id, -32602, "tools/call params missing `name`");
    };
    // The tool must be advertised — `tools/list` and `tools/call`
    // share `registry` as the single source of truth. On a miss, carry a
    // structured ErrorEnvelope with fuzzy "did you mean" candidates so a
    // misspelled name self-heals in one round (oracle-da9j.2).
    if !registry.tools.iter().any(|t| t.name == name) {
        let names: Vec<&str> = registry.tools.iter().map(|t| t.name.as_str()).collect();
        let envelope = oraclemcp_error::ErrorEnvelope::new(
            oraclemcp_error::ErrorClass::InvalidArguments,
            format!("tool not found: {name}"),
        )
        .with_fuzzy_matches(oraclemcp_error::fuzzy_suggest(name, &names, 5))
        .with_next_step("Call tools/list to see the exact tool names, then retry with one of them.");
        return JsonRpcResponse::err_with_data(
            id,
            -32601,
            format!("tool not found: {name}"),
            envelope.to_json(),
        );
    }

    // `arguments` is optional per MCP; a missing object means "no
    // arguments", which the per-tool Request types accept or reject
    // on their own terms.
    let empty = Value::Object(Default::default());
    let arguments = params.get("arguments").unwrap_or(&empty);

    // oracle-l65d: dispatch into the real `run_*` implementation.
    // `dispatch_tool` is the single dispatch table; it deserializes
    // the arguments into the tool's Request type and either runs the
    // tool (self-contained static analysis) or returns an honest
    // "runtime state required" outcome for tools that need a live
    // connection / loaded graph / preview session.
    match dispatch_tool(name, arguments) {
        Ok(DispatchOutcome::Ran(structured)) => {
            let mut result = tool_result(&structured_text(name, &structured), false, Some(structured));
            // Workflow-first: attach the natural follow-up tools so an agent can
            // chain a multi-step task without re-planning (oracle-da9j.7).
            let next = next_actions_for(name);
            if !next.is_empty() {
                result["next_actions"] = Value::Array(
                    next.into_iter().map(|s| Value::String(s.to_string())).collect(),
                );
            }
            JsonRpcResponse::ok(id, result)
        }
        Ok(DispatchOutcome::RuntimeStateRequired(kind)) => {
            // Wired, arguments validated — but the runtime state is absent.
            // Honest error *result* (transport-level call succeeded; the tool
            // reports it cannot run here) carrying a structured envelope that
            // names the REAL tool to call next (oracle-da9j.2).
            let msg = kind.message(name);
            let envelope = oraclemcp_error::ErrorEnvelope::new(
                oraclemcp_error::ErrorClass::RuntimeStateRequired,
                msg.clone(),
            )
            .with_suggested_tool(runtime_kind_recovery_tool(kind))
            .with_next_step(format!(
                "Call `{}` to provide the missing runtime state, then retry `{name}`.",
                runtime_kind_recovery_tool(kind)
            ));
            JsonRpcResponse::ok(id, tool_result(&msg, true, Some(envelope.to_json())))
        }
        Err(DispatchError::UnknownTool(tool)) => {
            // Registry/dispatch drift — should be impossible (the lockstep test
            // guards it), but never panic.
            let names: Vec<&str> = registry.tools.iter().map(|t| t.name.as_str()).collect();
            let envelope = oraclemcp_error::ErrorEnvelope::new(
                oraclemcp_error::ErrorClass::InvalidArguments,
                format!("tool not found: {tool}"),
            )
            .with_fuzzy_matches(oraclemcp_error::fuzzy_suggest(&tool, &names, 5));
            JsonRpcResponse::err_with_data(
                id,
                -32601,
                format!("tool not found: {tool}"),
                envelope.to_json(),
            )
        }
        Err(DispatchError::InvalidArguments { tool, detail }) => {
            let envelope = oraclemcp_error::ErrorEnvelope::new(
                oraclemcp_error::ErrorClass::InvalidArguments,
                format!("invalid arguments for tool `{tool}`: {detail}"),
            )
            .with_next_step(format!(
                "Inspect `{tool}`'s inputSchema in tools/list and supply the required fields."
            ));
            JsonRpcResponse::err_with_data(
                id,
                -32602,
                format!("invalid arguments for tool `{tool}`: {detail}"),
                envelope.to_json(),
            )
        }
    }
}

/// The real plsql-mcp tool an agent should call to satisfy a
/// [`RuntimeKind`]'s missing state (oracle-da9j.2 — names a tool that actually
/// exists on the surface, not a placeholder).
fn runtime_kind_recovery_tool(kind: crate::dispatch::RuntimeKind) -> &'static str {
    use crate::dispatch::RuntimeKind;
    match kind {
        // A loaded dependency graph comes from a project analysis.
        RuntimeKind::DependencyGraph => "analyze_project",
        // Live connection / preview / session state all require an active
        // connected live-db session, entered via `connect`.
        RuntimeKind::LiveConnection
        | RuntimeKind::PreviewSession
        | RuntimeKind::SessionState => "connect",
    }
}

/// Build an MCP `tools/call` result object: a human-readable
/// `content` text block, the `isError` flag, and (for tools that
/// ran) the machine-readable `structuredContent` payload.
/// Natural follow-up tools an agent should consider after a tool runs
/// successfully, so a multi-step task chains without re-planning
/// (oracle-da9j.7). Empty for terminal/standalone tools.
fn next_actions_for(name: &str) -> Vec<&'static str> {
    match name {
        "oracle_capabilities" => vec![
            "tools/list — read each tool's argument schema + readOnlyHint/destructiveHint",
            "analyze_project — load a project to enable the graph + analysis tools",
        ],
        "analyze_project" => vec![
            "plsql_analyze — routine/object inventory, lint findings, complexity",
            "find_callers / find_callees / get_dependencies — traverse the dependency graph",
        ],
        "plsql_analyze" => vec![
            "find_callers / get_dependencies — drill into a specific routine's edges",
        ],
        "parse_file" => vec![
            "get_symbol — look up a declaration by name",
            "compile_check — error/warning counts + every diagnostic",
        ],
        "find_callers" | "find_callees" => {
            vec!["get_dependencies — the flat dependency id list for the same target"]
        }
        // A live-DB read naturally precedes a guarded write.
        "describe_table" | "describe_view" => vec![
            "patch_view / create_or_replace (dry_run) — preview a change to this object",
        ],
        _ => vec![],
    }
}

fn tool_result(text: &str, is_error: bool, structured: Option<Value>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "content".into(),
        serde_json::json!([{ "type": "text", "text": text }]),
    );
    obj.insert("isError".into(), Value::Bool(is_error));
    if let Some(s) = structured {
        obj.insert("structuredContent".into(), s);
    }
    Value::Object(obj)
}

/// One-line human summary for a tool that ran. The full result is
/// always in `structuredContent`; this is the `content` text the
/// MCP spec also wants present.
fn structured_text(name: &str, structured: &Value) -> String {
    format!("tool `{name}` executed; structured result: {structured}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register_query_tool;

    fn registry_with_query() -> ToolRegistry {
        let mut r = ToolRegistry::default();
        register_query_tool(&mut r);
        r
    }

    fn req(id: i64, method: &str, params: Option<Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::from(id)),
            method: method.into(),
            params,
        }
    }

    #[test]
    fn initialize_returns_server_info_and_capabilities() {
        let r = registry_with_query();
        let resp = handle_request(
            &req(
                1,
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {}
                })),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "plsql-mcp");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_with_mismatched_version_returns_error() {
        let r = registry_with_query();
        let resp = handle_request(
            &req(
                2,
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": "1999-01-01"
                })),
            ),
            &r,
        )
        .unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("protocol version mismatch"));
    }

    #[test]
    fn tools_list_returns_registered_tools() {
        let r = registry_with_query();
        let resp = handle_request(&req(3, "tools/list", None), &r).unwrap();
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "query"));
    }

    #[test]
    fn tools_list_advertises_real_schemas_and_destructive_annotations() {
        // oracle-da9j.1 + .9: tools/list must advertise each tool's real argument
        // schema (so an agent can construct a valid first call) and surface
        // destructive intent via the MCP-standard annotations.
        let r = crate::default_tool_registry();
        let resp = handle_request(&req(7, "tools/list", None), &r).unwrap();
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        let by = |name: &str| -> Value {
            tools
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("tool {name} advertised"))
                .clone()
        };
        // Real schemas with the right required fields (.1).
        for (name, field) in [
            ("query", "sql"),
            ("parse_file", "source"),
            ("get_symbol", "source"),
            ("find_callers", "target"),
            ("analyze_project", "project_root"),
            ("plsql_analyze", "project_root"),
        ] {
            let t = by(name);
            let req_arr = t["inputSchema"]["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{name} has a required[] (not the permissive blob)"));
            assert!(
                req_arr.iter().any(|v| v == field),
                "{name} inputSchema.required must contain {field}: {t}"
            );
            assert_eq!(
                t["annotations"]["readOnlyHint"],
                Value::Bool(true),
                "{name} is read-only"
            );
        }
        // Destructive write tools carry destructiveHint (.9).
        for name in ["deploy_ddl", "create_or_replace", "execute_approved", "patch_package"] {
            let t = by(name);
            assert_eq!(
                t["annotations"]["destructiveHint"],
                Value::Bool(true),
                "{name} must be flagged destructive"
            );
            assert_eq!(t["annotations"]["readOnlyHint"], Value::Bool(false));
        }
    }

    #[test]
    fn unknown_tool_error_carries_fuzzy_suggestions() {
        // oracle-da9j.2: a misspelled tool name returns a structured ErrorEnvelope
        // in error.data with fuzzy "did you mean" candidates, so an agent
        // self-heals in one round instead of parsing a bare string.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                8,
                "tools/call",
                Some(serde_json::json!({"name": "parse_fil", "arguments": {}})),
            ),
            &r,
        )
        .unwrap();
        let err = resp.error.expect("protocol error");
        assert_eq!(err.code, -32601);
        let data = err.data.expect("structured envelope in error.data");
        let fuzzy = data["fuzzy_matches"].as_array().expect("fuzzy_matches present");
        assert!(
            fuzzy.iter().any(|v| v == "parse_file"),
            "fuzzy_matches should suggest parse_file: {data}"
        );
    }

    #[test]
    fn runtime_state_required_result_names_the_real_recovery_tool() {
        // oracle-da9j.2: a wired tool needing runtime state returns an honest
        // isError result whose structuredContent envelope names a REAL recovery
        // tool (find_callers needs a DepGraph -> analyze_project).
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                9,
                "tools/call",
                Some(serde_json::json!({"name": "find_callers", "arguments": {"target": "a.b/1"}})),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.expect("ok result");
        assert_eq!(result["isError"], Value::Bool(true));
        let env = &result["structuredContent"];
        assert_eq!(env["error_class"], "RUNTIME_STATE_REQUIRED");
        assert_eq!(env["suggested_tool"], "analyze_project");
    }

    #[test]
    fn oracle_capabilities_is_a_zero_arg_discovery_tool() {
        // oracle-da9j.3: the shipping registry advertises a zero-arg discovery
        // tool that runs over the wire and reports feature flags + next_actions.
        let r = crate::default_tool_registry();
        assert!(r.tools.iter().any(|t| t.name == "oracle_capabilities"));
        let resp = handle_request(
            &req(
                11,
                "tools/call",
                Some(serde_json::json!({"name": "oracle_capabilities", "arguments": {}})),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.expect("ok result");
        assert_eq!(result["isError"], Value::Bool(false));
        let doc = &result["structuredContent"];
        assert_eq!(doc["server"], "plsql-mcp");
        assert!(doc["features"]["live_db"].is_boolean());
        assert!(
            doc["next_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap_or("").contains("tools/list")),
            "capabilities should point at tools/list: {doc}"
        );
    }

    #[test]
    fn initialize_emits_orientation_instructions() {
        let resp = handle_request(
            &req(
                12,
                "initialize",
                Some(serde_json::json!({"protocolVersion": PROTOCOL_VERSION})),
            ),
            &crate::default_tool_registry(),
        )
        .unwrap();
        let instr = resp.result.expect("ok")["instructions"]
            .as_str()
            .expect("instructions string present")
            .to_string();
        assert!(
            instr.contains("oracle_capabilities") && instr.contains("tools/list"),
            "initialize must orient the agent: {instr}"
        );
    }

    #[test]
    fn tool_availability_gates_by_feature_and_profile() {
        // oracle-da9j.4: feature + profile projection (tested with live_db=true
        // so the profile dimension is exercised regardless of the build feature).
        let static_tool = ToolDescriptor::new("parse_file", ToolTier::FoundationStatic, "s");
        let read_live = ToolDescriptor::new("query", ToolTier::FoundationLiveDb, "r");
        let write_live =
            ToolDescriptor::new("deploy_ddl", ToolTier::FoundationLiveDb, "w").destructive();
        // Static tools are always available.
        assert!(tool_availability(&static_tool, false, SafetyProfile::StaticOnly).0);
        // No live-db feature → every live tool unavailable (with a reason).
        let (avail, reason) = tool_availability(&read_live, false, SafetyProfile::SessionWriteEnabled);
        assert!(!avail && reason.is_some());
        // live-db on + StaticOnly profile → even read-only live tools off.
        assert!(!tool_availability(&read_live, true, SafetyProfile::StaticOnly).0);
        // live-db on + InspectOnly → read-only live available, writes not.
        assert!(tool_availability(&read_live, true, SafetyProfile::InspectOnly).0);
        assert!(!tool_availability(&write_live, true, SafetyProfile::InspectOnly).0);
        // live-db on + DdlGuarded → write (preview) tools available.
        assert!(tool_availability(&write_live, true, SafetyProfile::DdlGuarded).0);
    }

    #[test]
    fn tools_list_flags_live_tools_unavailable_in_static_build() {
        // oracle-da9j.4: a FoundationLiveDb tool is listed (discoverable) but
        // flagged available:false + a reason in a build without the live-db
        // feature; static tools stay available:true.
        let r = crate::default_tool_registry();
        let resp = handle_request(&req(14, "tools/list", None), &r).unwrap();
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        let find = |n: &str| {
            tools
                .iter()
                .find(|t| t["name"] == n)
                .unwrap_or_else(|| panic!("{n} listed"))
                .clone()
        };
        if !cfg!(feature = "live-db") {
            let q = find("query");
            assert_eq!(q["annotations"]["available"], Value::Bool(false));
            assert!(q["annotations"]["unavailableReason"].is_string());
        }
        assert_eq!(
            find("parse_file")["annotations"]["available"],
            Value::Bool(true)
        );
    }

    #[test]
    fn successful_results_carry_next_actions_workflow_hints() {
        // oracle-da9j.7: a tool that runs attaches its natural follow-ups.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                15,
                "tools/call",
                Some(serde_json::json!({
                    "name": "parse_file",
                    "arguments": {"source": "BEGIN NULL; END;\n/\n"}
                })),
            ),
            &r,
        )
        .unwrap();
        let na = resp.result.expect("ok")["next_actions"]
            .as_array()
            .expect("next_actions present")
            .clone();
        assert!(
            na.iter().any(|s| s.as_str().unwrap_or("").contains("get_symbol")),
            "parse_file should chain to get_symbol/compile_check: {na:?}"
        );
        // The discovery tool chains to tools/list + analyze_project.
        let cap = handle_request(
            &req(
                16,
                "tools/call",
                Some(serde_json::json!({"name": "oracle_capabilities", "arguments": {}})),
            ),
            &r,
        )
        .unwrap();
        assert!(
            cap.result.unwrap()["next_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap_or("").contains("tools/list"))
        );
    }

    #[test]
    fn invalid_arguments_error_carries_a_next_step() {
        // oracle-da9j.2: bad arguments return -32602 with an envelope that points
        // the agent at the tool's inputSchema.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                10,
                "tools/call",
                // get_symbol requires `source` + `symbol`; omit both.
                Some(serde_json::json!({"name": "get_symbol", "arguments": {"wrong": 1}})),
            ),
            &r,
        )
        .unwrap();
        let err = resp.error.expect("protocol error");
        assert_eq!(err.code, -32602);
        let data = err.data.expect("structured envelope");
        assert_eq!(data["error_class"], "INVALID_ARGUMENTS");
        assert!(
            !data["next_steps"].as_array().unwrap().is_empty(),
            "next_steps should guide the agent: {data}"
        );
    }

    #[test]
    fn tools_call_parse_file_runs_real_parser_over_the_wire() {
        // oracle-l65d: a `parse_file` call must reach the real
        // `run_parse_file` implementation and return a structured
        // parse result — not a static "execution gated" placeholder.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                40,
                "tools/call",
                Some(serde_json::json!({
                    "name": "parse_file",
                    "arguments": {
                        "source": "CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n"
                    }
                })),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.expect("ok result");
        assert_eq!(result["isError"], Value::Bool(false));
        // The structured tool output carries the real ParseFileResponse.
        let sc = &result["structuredContent"];
        assert!(
            sc["declaration_count"].as_u64().unwrap() >= 1,
            "real parser counted declarations: {sc:?}"
        );
        assert_eq!(sc["recovered"], Value::Bool(false));
    }

    #[test]
    fn tools_call_compile_check_reports_real_diagnostics() {
        // A clean source must come back clean=true through the wire.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                41,
                "tools/call",
                Some(serde_json::json!({
                    "name": "compile_check",
                    "arguments": {
                        "source": "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n"
                    }
                })),
            ),
            &r,
        )
        .unwrap();
        let sc = resp.result.unwrap()["structuredContent"].clone();
        assert_eq!(sc["clean"], Value::Bool(true));
        assert_eq!(sc["error_count"].as_u64().unwrap(), 0);
    }

    #[test]
    fn tools_call_analyze_project_runs_pipeline_over_the_wire() {
        // analyze_project takes a project_root path in its arguments —
        // a fully self-contained static tool. An empty root is a clean
        // zero run, not an error.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                42,
                "tools/call",
                Some(serde_json::json!({
                    "name": "analyze_project",
                    "arguments": {"project_root": ""}
                })),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.expect("ok result");
        assert_eq!(result["isError"], Value::Bool(false));
        assert_eq!(result["structuredContent"]["file_count"].as_u64().unwrap(), 0);
    }

    #[test]
    fn tools_call_bad_arguments_returns_invalid_params() {
        // oracle-l65d: arguments that do not deserialize into the
        // tool's Request type are a proper -32602, never a panic.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                43,
                "tools/call",
                Some(serde_json::json!({
                    "name": "parse_file",
                    "arguments": {"wrong_field": 123}
                })),
            ),
            &r,
        )
        .unwrap();
        let err = resp.error.expect("invalid arguments => error");
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn tools_call_live_db_tool_degrades_honestly_without_a_connection() {
        // oracle-l65d: a live-DB tool (`query`) IS wired — it has a
        // dispatch arm — but with no active connection it must return
        // a typed, honest result, never a panic and never a fake
        // success. `isError` is true; the message names the missing
        // runtime state.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                44,
                "tools/call",
                Some(serde_json::json!({
                    "name": "query",
                    "arguments": {"sql": "SELECT 1 FROM dual"}
                })),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.expect("a result, not a transport error");
        assert_eq!(
            result["isError"],
            Value::Bool(true),
            "no-connection is an honest error result"
        );
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_lowercase();
        assert!(
            text.contains("connection") || text.contains("live-db") || text.contains("runtime"),
            "message must name the missing runtime state: {text:?}"
        );
    }

    #[test]
    fn tools_call_live_db_arguments_still_validated_before_gating() {
        // Even a gated live-DB tool deserializes its arguments first:
        // malformed arguments are -32602, not a generic gate message.
        let r = crate::default_tool_registry();
        let resp = handle_request(
            &req(
                45,
                "tools/call",
                Some(serde_json::json!({
                    "name": "query",
                    "arguments": {"sql": 12345}
                })),
            ),
            &r,
        )
        .unwrap();
        assert_eq!(resp.error.expect("bad args => error").code, -32602);
    }

    #[test]
    fn every_registered_tool_has_a_dispatch_arm() {
        // oracle-l65d: the dispatch table and default_tool_registry()
        // must stay in lockstep — a tool advertised over tools/list
        // that has no dispatch arm is a wire gap.
        let r = crate::default_tool_registry();
        for tool in &r.tools {
            let resp = handle_request(
                &req(
                    99,
                    "tools/call",
                    Some(serde_json::json!({
                        "name": tool.name,
                        "arguments": {}
                    })),
                ),
                &r,
            )
            .expect("a response");
            // A dispatched tool answers with EITHER a result (ran, or
            // honestly gated, or arg-validation result) OR a -32602
            // invalid-params error for the empty arguments. What it
            // must NEVER do is answer -32601 "tool not found": that
            // would mean the tool is registered but not dispatched.
            if let Some(err) = &resp.error {
                assert_ne!(
                    err.code, -32601,
                    "tool `{}` is registered but has no dispatch arm",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tools_call_for_unknown_tool_returns_method_not_found() {
        let r = registry_with_query();
        let resp = handle_request(
            &req(
                5,
                "tools/call",
                Some(serde_json::json!({
                    "name": "nonexistent"
                })),
            ),
            &r,
        )
        .unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("tool not found"));
    }

    #[test]
    fn tools_call_missing_name_param_returns_invalid_params() {
        let r = registry_with_query();
        let resp = handle_request(&req(6, "tools/call", Some(serde_json::json!({}))), &r).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn ping_result_is_empty_payload_plus_trust_block() {
        // MCP-007: every successful result carries
        // `meta.trust_block`; ping's own payload stays empty.
        let r = registry_with_query();
        let resp = handle_request(&req(7, "ping", None), &r).unwrap();
        let result = resp.result.unwrap();
        let obj = result.as_object().unwrap();
        assert!(obj["meta"]["trust_block"].is_object());
        assert_eq!(obj["meta"]["trust_block"]["tier"], "foundation");
        // The only key besides the injected `meta` is nothing —
        // ping contributes no payload of its own.
        let non_meta: Vec<&String> = obj.keys().filter(|k| k.as_str() != "meta").collect();
        assert!(
            non_meta.is_empty(),
            "ping payload stays empty: {non_meta:?}"
        );
    }

    #[test]
    fn every_ok_response_carries_a_trust_block() {
        let r = registry_with_query();
        for method in ["initialize", "tools/list", "ping"] {
            let resp = handle_request(&req(1, method, None), &r).unwrap();
            let result = resp.result.expect("ok response");
            assert!(
                result["meta"]["trust_block"]["schema_id"] == crate::trust::TRUST_BLOCK_SCHEMA_ID,
                "{method} response missing trust block"
            );
        }
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let r = registry_with_query();
        let resp = handle_request(&req(8, "nope/bogus", None), &r).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn notification_returns_none() {
        let r = registry_with_query();
        let n = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: None,
            method: "initialized".into(),
            params: None,
        };
        assert!(handle_request(&n, &r).is_none());
    }

    #[test]
    fn invalid_jsonrpc_version_returns_invalid_request() {
        let r = registry_with_query();
        let mut bad = req(9, "ping", None);
        bad.jsonrpc = "1.0".into();
        let resp = handle_request(&bad, &r).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
    }

    #[test]
    fn handle_request_line_parses_and_dispatches() {
        let r = registry_with_query();
        let line = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"{PROTOCOL_VERSION}\"}}}}",
        );
        let resp = handle_request_line(&line, &r).unwrap();
        assert!(resp.result.is_some());
    }

    #[test]
    fn handle_request_line_parse_error_returns_minus_32700() {
        let r = registry_with_query();
        let resp = handle_request_line("{not json", &r).unwrap();
        assert_eq!(resp.error.unwrap().code, -32700);
    }
}
