# MCP client integration walkthroughs

How to wire the `plsql-mcp` server into the four MCP-capable
agent clients: **Cursor**, **Claude Desktop**, **Devin**, and
**Windsurf**. The server speaks MCP over **stdio** by default and
exposes the static-analysis tool surface (`analyze_project`,
`find_callers`, `find_callees`, `get_dependencies`,
`dynamic_sql_evidence`, `completeness_report`, `doc_lookup`)
alongside the change-impact tools (`what_breaks`, `sarif_scan`,
`release_gate`, and the rest of module `change_tools`). Every
successful response carries `result.meta.trust_block` (static
analysis, no live DB) so the agent always sees the provenance of
an answer.

> **Status.** The stdio serve loop is gated on `PLSQL-MCP-002`;
> until it lands, `plsql-mcp serve` exits with a clear message and
> `plsql-mcp doctor` is the way to confirm the binary, protocol
> version, and registered tool surface. The client configs below
> are the standard MCP stdio shape and are correct as-is — the
> client will connect once the loop is wired. Confirm any time
> with:
>
> ```
> plsql-mcp doctor --robot-json
> ```

## Build / locate the binary

```
cargo build --release -p plsql-mcp
# binary: target/release/plsql-mcp
```

Use the **absolute path** to the binary in every client config
below (agent clients do not inherit your shell `PATH`).

---

## Cursor

Cursor reads MCP servers from `~/.cursor/mcp.json` (global) or
`.cursor/mcp.json` (per-project).

```json
{
  "mcpServers": {
    "plsql-intelligence": {
      "command": "/abs/path/to/target/release/plsql-mcp",
      "args": ["serve"],
      "env": {}
    }
  }
}
```

Reload: Cursor Settings → MCP → refresh. The `plsql-intelligence`
tools appear in the tool picker. First call to `analyze_project`
with `{"project_root": "/abs/path/to/your/plsql/repo"}` returns
the AnalysisRun summary.

---

## Claude Desktop

Edit `claude_desktop_config.json`:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "plsql-intelligence": {
      "command": "/abs/path/to/target/release/plsql-mcp",
      "args": ["serve"]
    }
  }
}
```

Fully quit and reopen Claude Desktop (it only reads the config at
launch). The hammer/tools icon lists the `plsql-mcp` tools. The
`meta.trust_block` on each result tells the model the answer is
static-analysis-only.

---

## Devin

Devin registers MCP servers through its integrations settings
(Settings → MCP servers → Add). Supply a stdio server:

- **Command:** `/abs/path/to/target/release/plsql-mcp`
- **Args:** `serve`
- **Transport:** stdio

Devin discovers the tool surface via `tools/list`; no manual tool
declaration is needed. Point `analyze_project` at the repository
Devin has checked out.

---

## Windsurf

Windsurf (Cascade) reads `~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "plsql-intelligence": {
      "command": "/abs/path/to/target/release/plsql-mcp",
      "args": ["serve"]
    }
  }
}
```

Cascade → MCP → refresh. Tools surface under the
`plsql-intelligence` server.

---

## Verifying any client

Regardless of client, three checks confirm a healthy integration:

1. `tools/list` returns the full tool surface — the seven
   static-analysis tools plus the `change_tools` change-impact
   tools (the client's tool picker shows them).
2. A trivial `ping` (or any successful call) returns
   `result.meta.trust_block.schema_id == "plsql.mcp.trust_block"`
   with `live_database_used: false`.
3. `plsql-mcp doctor --robot-json` from a shell reports the same
   protocol version and `registered_tool_count` the client sees.

The change-impact tools (`what_breaks`, `sarif_scan`,
`release_gate`, …) live in the same `plsql-mcp` binary as the
static-analysis tools. There is no separate binary and no license
to activate — see
[`../commercial/license-activation.md`](../commercial/license-activation.md).
