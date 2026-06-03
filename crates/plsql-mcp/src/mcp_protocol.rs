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

use crate::tools::{ToolDescriptor, ToolRegistry};

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
        }
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
    serde_json::json!({
        "name": t.name,
        "description": t.summary,
        "inputSchema": input_schema,
        "annotations": {
            "readOnlyHint": !t.destructive,
            "destructiveHint": t.destructive,
        },
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
    // share `registry` as the single source of truth.
    if !registry.tools.iter().any(|t| t.name == name) {
        return JsonRpcResponse::err(id, -32601, format!("tool not found: {name}"));
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
        Ok(DispatchOutcome::Ran(structured)) => JsonRpcResponse::ok(
            id,
            tool_result(&structured_text(name, &structured), false, Some(structured)),
        ),
        Ok(DispatchOutcome::RuntimeStateRequired(kind)) => {
            // Wired, arguments validated — but the runtime state is
            // absent. Honest error *result* (transport-level call
            // succeeded; the tool reports it cannot run here).
            JsonRpcResponse::ok(id, tool_result(&kind.message(name), true, None))
        }
        Err(DispatchError::UnknownTool(tool)) => {
            // Registry/dispatch drift — should be impossible (the
            // lockstep test guards it), but never panic.
            JsonRpcResponse::err(id, -32601, format!("tool not found: {tool}"))
        }
        Err(DispatchError::InvalidArguments { tool, detail }) => JsonRpcResponse::err(
            id,
            -32602,
            format!("invalid arguments for tool `{tool}`: {detail}"),
        ),
    }
}

/// Build an MCP `tools/call` result object: a human-readable
/// `content` text block, the `isError` flag, and (for tools that
/// ran) the machine-readable `structuredContent` payload.
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
