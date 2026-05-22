//! MCP stdio protocol layer (`PLSQL-MCP-002`).
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
//!   defers to the `ToolRegistry` populated by the foundation
//!   live-DB beads (PLSQL-MCP-LIVE-002 / -004 / -011 / -012 /
//!   -013 / -014 / -015 / -016). This module is the transport
//!   shim above those tools, not an Oracle behaviour change.

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
    serde_json::json!({
        "name": t.name,
        "description": t.summary,
        "inputSchema": {
            "type": "object",
            "additionalProperties": true,
        }
    })
}

fn handle_tools_call(
    id: Value,
    params: Option<&Value>,
    registry: &ToolRegistry,
) -> JsonRpcResponse {
    let Some(params) = params else {
        return JsonRpcResponse::err(id, -32602, "tools/call requires params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return JsonRpcResponse::err(id, -32602, "tools/call params missing `name`");
    };
    let Some(_tool) = registry.tools.iter().find(|t| t.name == name) else {
        return JsonRpcResponse::err(id, -32601, format!("tool not found: {name}"));
    };
    // The per-tool runtime dispatchers land in the individual
    // MCP-LIVE-* beads. Until they wire into this dispatcher we
    // return a structured "not yet executable" payload so a
    // client can verify discovery without panicking.
    JsonRpcResponse::ok(
        id,
        serde_json::json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "tool {name} is registered but execution is gated on the per-tool MCP-LIVE wiring; consult the tool descriptor for the call shape."
                ),
            }],
            "isError": false,
        }),
    )
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
    fn tools_call_for_registered_tool_returns_text_content() {
        let r = registry_with_query();
        let resp = handle_request(
            &req(
                4,
                "tools/call",
                Some(serde_json::json!({
                    "name": "query",
                    "arguments": {"sql": "SELECT 1 FROM dual"}
                })),
            ),
            &r,
        )
        .unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], Value::Bool(false));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("query"));
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
