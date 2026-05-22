# `plsql-mcp` live-DB integration

Per-platform walkthroughs for the `live-db` feature of `plsql-mcp`. Pick
the platform you're developing on:

- [Linux](linux.md)
- [macOS](macos.md)
- [Windows](windows.md)

All three cover the same five steps:

1. Install Oracle Instant Client.
2. Set up an Oracle wallet for credential-free auth.
3. Create `~/.plsql-mcp/connections.toml` with `permanently_read_only`
   for any production-looking connection.
4. Wire `plsql-mcp serve` into Claude Code / Cursor / Codex CLI.
5. Smoke-test through the agent.

`plsql-mcp doctor` is the source of truth for build-status, Instant
Client detection, audit posture, and per-connection write-posture. Run
it after every install step; it returns a structured JSON report under
`--robot-json` so an agent can diagnose itself.
