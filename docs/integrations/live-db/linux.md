# `plsql-mcp` live-DB integration on Linux

This walkthrough sets up the live-DB feature of `plsql-mcp` on a typical
Linux developer machine (Ubuntu / Fedora / Debian / Arch). It covers the
Oracle Instant Client install, wallet-based connection setup, the
`permanently_read_only` hard guard, and editor / agent integration snippets.

## 1. Oracle Instant Client install

`plsql-mcp` depends on the `rust-oracle` driver, which in turn calls into
the libclntsh.so shipped by Oracle Instant Client. Oracle does not allow
us to bundle Instant Client, so it has to be installed on the host first.

1. Pick a 23ai client matching the database's major release if the choice
   is yours; the 21c / 23ai clients work fine against 19c databases too.
2. Download from Oracle's Instant Client page (Basic + SDK packages):
   <https://www.oracle.com/database/technologies/instant-client/linux-x86-64-downloads.html>.
3. Unzip into a stable directory:

   ```sh
   sudo mkdir -p /opt/oracle
   sudo unzip instantclient-basic-linux.x64-23.x.0.0.0dbru.zip -d /opt/oracle/
   sudo unzip instantclient-sdk-linux.x64-23.x.0.0.0dbru.zip -d /opt/oracle/
   ```

4. Export the directory so the dynamic loader finds it. Pick one:

   - **Per-shell (recommended for development):**
     ```sh
     export LD_LIBRARY_PATH="/opt/oracle/instantclient_23_8:$LD_LIBRARY_PATH"
     ```
     Persist in `~/.bashrc` / `~/.zshrc`.

   - **System-wide:** `echo /opt/oracle/instantclient_23_8 | sudo tee /etc/ld.so.conf.d/oracle.conf && sudo ldconfig`.

5. Verify with `plsql-mcp doctor`:

   ```text
   $ plsql-mcp doctor
   plsql-mcp 0.1.0 (live-db: true, transport: stdio, safety: InspectOnly)
   ...
   [OK] MCP_DOCTOR_OK — plsql-mcp doctor: no blockers detected.
   ```

   Doctor reports the detected Instant Client path + version hint. If you
   see `MCP_INSTANT_CLIENT_NOT_DETECTED`, recheck step 4.

## 2. Wallet setup

For Autonomous Database (and any other Oracle service that ships a wallet
zip), prefer wallet-based auth. No password lives on the developer's
filesystem outside the wallet.

1. Place the wallet directory (the unzipped one with `tnsnames.ora`,
   `cwallet.sso`, `sqlnet.ora`) somewhere stable, e.g. `~/oracle/wallets/prod`.
2. Set:

   ```sh
   export TNS_ADMIN="$HOME/oracle/wallets/prod"
   ```

3. `tnsnames.ora` will already contain the connect aliases (`billing_high`,
   `billing_low`, ...). Reference those by name in `connections.toml`.

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
