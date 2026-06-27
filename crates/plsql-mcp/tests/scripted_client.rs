//! Scripted MCP client integration test.
//!
//! Drives `plsql-mcp` exactly as a real MCP client would — one
//! newline-delimited JSON-RPC request at a time through
//! [`handle_request_line`] — over the full registered foundation
//! tool surface, and golden-asserts the structural invariants of
//! every response.
//!
//! "Golden snapshot" here follows the workspace idiom:
//! deterministic structural assertions on the canonical JSON rather
//! than an `insta` dependency. Every
//! invariant that must never silently regress is pinned:
//! JSON-RPC framing, id echo, the MCP-007 trust block on every
//! success, and the exact registered foundation tool set.

use plsql_mcp::{
    PlsqlMcpServer, ToolRegistry, default_tool_registry, register_analyze_project_tool,
    register_foundation_tools, register_graph_tools,
};

/// The foundation tool surface a scripted client should see.
const EXPECTED_TOOLS: &[&str] = &[
    "analyze_project",
    "find_callers",
    "find_callees",
    "get_dependencies",
    "dynamic_sql_evidence",
    "completeness_report",
    "doc_lookup",
];

fn foundation_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    register_analyze_project_tool(&mut r);
    register_graph_tools(&mut r);
    register_foundation_tools(&mut r);
    r
}

fn server(registry: ToolRegistry) -> PlsqlMcpServer {
    PlsqlMcpServer::new(registry).expect("server runtime builds")
}

fn call(server: &mut PlsqlMcpServer, line: &str) -> serde_json::Value {
    let resp = server
        .handle_request_line(line)
        .expect("server produced a response");
    serde_json::to_value(&resp).expect("response serializes")
}

#[test]
fn scripted_session_golden_invariants() {
    let mut server = server(foundation_registry());

    // --- frame 1: initialize ---
    let init = call(
        &mut server,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["id"], 1, "id is echoed");
    assert!(init["error"].is_null(), "initialize must not error");
    assert!(init["result"]["serverInfo"]["name"].is_string());
    // MCP-007: trust block on every success.
    assert_eq!(
        init["result"]["meta"]["trust_block"]["schema_id"],
        "plsql.mcp.trust_block"
    );

    // --- frame 2: tools/list — exact foundation surface ---
    let list = call(
        &mut server,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    );
    assert_eq!(list["id"], 2);
    let tools = list["result"]["tools"]
        .as_array()
        .expect("tools array present");
    let mut names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    let mut expected: Vec<String> = EXPECTED_TOOLS.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(names, expected, "registered foundation surface is pinned");
    assert_eq!(
        list["result"]["meta"]["trust_block"]["live_database_used"],
        false
    );

    // --- frame 3: ping — empty payload + trust block ---
    let ping = call(
        &mut server,
        r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}"#,
    );
    assert_eq!(ping["id"], 3);
    assert!(ping["result"]["meta"]["trust_block"].is_object());

    // --- frame 4: unknown method — typed JSON-RPC error ---
    let bad = call(
        &mut server,
        r#"{"jsonrpc":"2.0","id":4,"method":"no/such","params":{}}"#,
    );
    assert_eq!(bad["id"], 4);
    assert_eq!(bad["error"]["code"], -32601, "method not found");
    assert!(
        bad["result"].is_null(),
        "an error response carries no result (and so no trust block)"
    );

    // --- frame 5: malformed line — parse error, never a panic ---
    let parse_err = call(&mut server, "{not valid json");
    assert_eq!(parse_err["error"]["code"], -32700);
}

#[test]
fn responses_are_deterministic_across_runs() {
    let mut server = server(foundation_registry());
    let line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list","params":{}}"#;
    let a = call(&mut server, line);
    let b = call(&mut server, line);
    assert_eq!(a, b, "identical request -> byte-identical response");
}

#[test]
fn tools_call_parse_file_executes_a_real_tool_over_the_wire() {
    // oracle-l65d: a `tools/call` for a self-contained static tool
    // must reach the real `run_parse_file` and return a structured
    // parse result over the wire — not a placeholder. This is the
    // headline "fully wired" assertion, exercised through the exact
    // `handle_request_line` path a real MCP client uses.
    let mut server = server(default_tool_registry());
    let resp = call(
        &mut server,
        concat!(
            r#"{"jsonrpc":"2.0","id":50,"method":"tools/call","params":"#,
            r#"{"name":"parse_file","arguments":"#,
            r#"{"source":"CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n"}}}"#,
        ),
    );
    assert_eq!(resp["id"], 50);
    assert!(resp["error"].is_null(), "parse_file must not error");
    let result = &resp["result"];
    assert_eq!(result["isError"], false);
    // The real ParseFileResponse rode back in structuredContent.
    assert!(
        result["structuredContent"]["declaration_count"]
            .as_u64()
            .unwrap()
            >= 1,
        "real parser counted declarations: {result}"
    );
    // MCP-007 trust block still attached to the wired result.
    assert_eq!(
        result["meta"]["trust_block"]["schema_id"],
        "plsql.mcp.trust_block"
    );
}

#[test]
fn tools_call_live_db_tool_returns_an_honest_gated_result_over_the_wire() {
    // oracle-l65d: a live-DB tool (`query`) is wired — it dispatches
    // — but with no connection it returns a typed, honest error
    // result over the wire. Never a panic, never a fake success.
    let mut server = server(default_tool_registry());
    let resp = call(
        &mut server,
        concat!(
            r#"{"jsonrpc":"2.0","id":51,"method":"tools/call","params":"#,
            r#"{"name":"query","arguments":{"sql":"SELECT 1 FROM dual"}}}"#,
        ),
    );
    assert_eq!(resp["id"], 51);
    assert!(resp["error"].is_null(), "transport call itself succeeds");
    assert_eq!(
        resp["result"]["isError"], true,
        "no connection => honest error result"
    );
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        text.contains("connection") || text.contains("live-db"),
        "gated message names the missing runtime: {text}"
    );
}

#[test]
fn tools_call_bad_arguments_returns_invalid_params_over_the_wire() {
    // oracle-l65d: un-deserializable arguments are a proper -32602
    // JSON-RPC error, never a panic.
    let mut server = server(default_tool_registry());
    let resp = call(
        &mut server,
        concat!(
            r#"{"jsonrpc":"2.0","id":52,"method":"tools/call","params":"#,
            r#"{"name":"parse_file","arguments":{"source":99}}}"#,
        ),
    );
    assert_eq!(resp["id"], 52);
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn every_registered_tool_is_discoverable_and_described() {
    let mut server = server(foundation_registry());
    let list = call(
        &mut server,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
    );
    for t in list["result"]["tools"].as_array().unwrap() {
        assert!(t["name"].as_str().is_some_and(|n| !n.is_empty()));
        // Each tool advertises a non-empty human description so an
        // agent can choose without trial calls.
        let desc = t
            .get("description")
            .and_then(|d| d.as_str())
            .or_else(|| t.get("summary").and_then(|d| d.as_str()))
            .unwrap_or("");
        assert!(!desc.is_empty(), "tool {} has no description", t["name"]);
    }
}
