# `plsql-mcp` live-DB integration on Windows

Sister of [`linux.md`](linux.md). Windows-specific notes only.

## 1. Oracle Instant Client install (Windows)

1. Download the Basic + SDK packages for Windows x64 from
   <https://www.oracle.com/database/technologies/instant-client/winx64-64-downloads.html>.
2. Unzip into a stable directory, e.g. `C:\oracle\instantclient_23_8`.
3. Add it to the system `PATH` (the dynamic loader on Windows uses `PATH`
   rather than `LD_LIBRARY_PATH`):

   - Win+R → `sysdm.cpl` → Advanced → Environment Variables.
   - Edit `Path` (User or System), add the Instant Client directory.

4. Re-open your terminal so the new `PATH` is picked up.
5. Verify with `plsql-mcp doctor`. The `instant_client.probable_path` will
   show the path *if* you also set `ORACLE_HOME` to the Instant Client
   directory — the detection heuristic on Windows looks at `ORACLE_HOME\lib`
   in addition to `PATH` entries. Setting `ORACLE_HOME` is recommended.

## 2. Wallet setup

Same shape as Linux:

```
set TNS_ADMIN=C:\Users\you\oracle\wallets\prod
```

For PowerShell:

```powershell
$env:TNS_ADMIN = "C:\Users\you\oracle\wallets\prod"
```

Wallet directory needs to be readable by the user running `plsql-mcp`.

## 3. `%USERPROFILE%\.plsql-mcp\connections.toml`

Same TOML shape as Linux/macOS. Forward slashes work; backslashes need to
be escaped as `\\` in TOML strings.

## 4. Editor / agent config

### Claude Code (Windows)

`%APPDATA%\Claude\claude-code\mcp-servers.json`:

```json
{
  "mcpServers": {
    "plsql": {
      "command": "plsql-mcp.exe",
      "args": ["serve"]
    }
  }
}
```

### Cursor (Windows)

`%USERPROFILE%\.cursor\mcp.json` — same content as the Linux example
except `plsql-mcp.exe`.

## 5. Troubleshooting

- "The code execution cannot proceed because OCI.dll was not found":
  `PATH` does not include the Instant Client directory. Re-open the
  terminal after editing `PATH`.
- "ORA-12154 TNS:could not resolve the connect identifier":
  `TNS_ADMIN` is not set, or the alias is not in the `tnsnames.ora` in
  that directory.
- For 32-bit Office / SQL Developer interop, install the 32-bit Instant
  Client alongside the 64-bit one in a separate directory — `plsql-mcp`
  (64-bit) ignores it.
