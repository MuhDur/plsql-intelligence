# `plsql-mcp` live-DB integration

Per-platform walkthroughs for the `live-db` feature of `plsql-mcp`. The
normal live-DB path uses the pure-Rust thin stack shared with `oraclemcp`
(`oraclemcp-db` -> `oracledb`) and does not require Oracle Instant
Client. Pick the platform you're developing on:

- [Linux](linux.md)
- [macOS](macos.md)
- [Windows](windows.md)

All three cover the same setup shape:

1. Build or install `plsql-mcp`.
2. Configure Oracle connect strings / wallets where your estate uses them.
3. Create `~/.plsql-mcp/connections.toml` with `permanently_read_only`
   for any production-looking connection.
4. Wire `plsql-mcp serve` into Claude Code / Cursor / Codex CLI.
5. Smoke-test through the agent.

`plsql-mcp doctor` is the source of truth for build status, live-DB
feature posture, audit posture, and per-connection write posture. Run it
after setup changes; it returns a structured JSON report under
`--robot-json` so an agent can diagnose itself.

Guarded writes require a local signed audit sink. Set
`PLSQL_MCP_AUDIT_FILE` to the append-only JSONL path and
`PLSQL_MCP_AUDIT_KEY` to the HMAC key before `plsql-mcp serve`; the
server refuses guarded writes when only one is configured or when neither
is present.
