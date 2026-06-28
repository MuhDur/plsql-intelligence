#![no_main]
//! Coverage-guided fuzz target for the async MCP dispatcher.
//!
//! Boundary: `plsql_mcp::dispatch_tool` is the pure async `tools/call`
//! dispatcher used by the `oraclemcp-core::ToolDispatch` integration. It
//! receives untrusted JSON arguments and a caller-controlled tool name before
//! any tool-specific request type can validate the payload.
//!
//! Oracle: the dispatcher may return an error envelope for invalid arguments,
//! unknown tools, or runtime-state requirements, but it must never panic on
//! any bounded JSON payload.

use asupersync::{runtime::RuntimeBuilder, Cx};
use libfuzzer_sys::fuzz_target;
use oraclemcp_core::DispatchContext;
use plsql_mcp::{dispatch::PlsqlDispatchContext, dispatch_table, dispatch_tool};
use serde_json::Value;

const MAX_LEN: usize = 64 * 1024;
const UNKNOWN_TOOL: &str = "fuzz_unknown_tool";

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }

    let selector = data.first().copied().unwrap_or_default();
    let payload = data.get(1..).unwrap_or_default();
    let table = dispatch_table();
    let index = usize::from(selector) % (table.len() + 1);
    let tool_name = if index == table.len() {
        UNKNOWN_TOOL
    } else {
        table[index]
    };

    let arguments = match serde_json::from_slice::<Value>(payload) {
        Ok(value) => value,
        Err(_) => Value::Object(Default::default()),
    };

    let Ok(runtime) = RuntimeBuilder::current_thread().build() else {
        return;
    };
    runtime.block_on(async move {
        let Some(cx) = Cx::current() else {
            return;
        };
        let context = PlsqlDispatchContext::from_cx(&cx, DispatchContext::default());
        let result = dispatch_tool(&cx, context, tool_name, arguments).await;
        drop(result);
    });
});
