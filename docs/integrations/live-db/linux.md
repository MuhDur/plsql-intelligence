# `plsql-mcp` live-DB integration on Linux

This walkthrough sets up the live-DB feature of `plsql-mcp` on a typical
Linux developer machine (Ubuntu / Fedora / Debian / Arch). It covers the
thin-driver connection setup, optional wallet/TNS aliases, the
`permanently_read_only` hard guard, and editor / agent integration snippets.

## 1. Binary setup

`plsql-mcp` live DB access uses the pure-Rust thin stack shared with
`oraclemcp` (`oraclemcp-db` -> `oracledb`). No Oracle Instant Client,
`libclntsh.so`, `LD_LIBRARY_PATH`, or OCI SDK install is required for the
normal live-DB path.

Build from source with the pinned nightly:

```sh
cargo build -p plsql-mcp --release
```

Then verify the surface:

```sh
plsql-mcp doctor
plsql-mcp --robot-json capabilities
```

## 2. Wallet / TNS setup

For Autonomous Database (and any other Oracle service that ships a wallet
zip), prefer wallet-based auth. No password lives on the developer's
filesystem outside the wallet.

1. Place the wallet directory (the unzipped one with `tnsnames.ora`,
   `cwallet.sso`, `sqlnet.ora`) somewhere stable, e.g. `~/oracle/wallets/prod`.
2. Set:

   ```sh
   export TNS_ADMIN="$HOME/oracle/wallets/prod"
   ```

3. `tnsnames.ora` will already contain the connect aliases
   (`billing_high`, `billing_low`, ...). Reference those by name in
   `connections.toml` when your driver build supports TNS aliases; use
   Easy Connect strings (`//host:port/service`) otherwise.

## 3. `~/.plsql-mcp/connections.toml`

Create the file (`mkdir -p ~/.plsql-mcp && $EDITOR ~/.plsql-mcp/connections.toml`):

```toml
[[connection]]
name = "billing-dev"
description = "Developer billing schema"
connect_string = "//localhost/XEPDB1"
username = "billing_app"

[[connection]]
name = "billing-prod-ro"
description = "Production billing — read-only audit account"
connect_string = "billing_low"  # TNS alias from the wallet
username = "billing_audit"
permanently_read_only = true
```

`permanently_read_only = true` is the hard guard — `plsql-mcp` refuses
`enable_writes` on that connection regardless of any session safety profile.
`plsql-mcp doctor` warns when any production-looking DSN (e.g. matches
`prod`) lacks this flag.

## 4. Agent / editor config

### Claude Code (Linux)

`~/.config/claude-code/mcp-servers.json`:

```json
{
  "mcpServers": {
    "plsql": {
      "command": "plsql-mcp",
      "args": ["serve"]
    }
  }
}
```

### Cursor (Linux)

`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "plsql": {
      "command": "plsql-mcp",
      "args": ["serve"]
    }
  }
}
```

### Codex CLI

```sh
codex mcp add plsql -- plsql-mcp serve
```

## 5. Smoke test

With the server registered and a `billing-dev` connection in
`connections.toml`:

1. Restart the editor / agent CLI so it picks up the new MCP server.
2. Ask the agent: *"List the first 10 objects in BILLING."* The agent
   should invoke `list_connections` → `connect billing-dev` →
   `list_objects(schema=BILLING, page_size=10)`.
3. The response is structured rows (owner / name / type / status /
   last_ddl_time). No free text.

If the agent stalls on `tools/list`, `plsql-mcp doctor` (and the doctor's
`registered_tool_count`) is the first place to look.
