# MCP Client Integration

MCP client setup is now owned by `oraclemcp`.

`plsql-intelligence` is the offline PL/SQL analysis engine. It does not
ship a server binary, stdio loop, connection profile loader, or live
database tool runtime. Agent clients should connect to `oraclemcp`, which
can embed this engine for parser, dependency, lineage, SAST, change-impact,
documentation, and binding tools.

Typical client configuration shape:

```json
{
  "mcpServers": {
    "oracle": {
      "command": "oraclemcp",
      "args": ["serve"]
    }
  }
}
```

Keep PL/SQL source analysis inputs local to the agent host. The engine can
run without a database when the MCP layer provides source files and, when
available, a `CatalogSnapshot` JSON file.

For this repository, verify the reusable engine instead:

```sh
cargo test --workspace --all-targets
scripts/offline_boundary_lint.sh
scripts/offline_honesty_grep.sh
```
