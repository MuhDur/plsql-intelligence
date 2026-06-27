# `plsql-mcp` live-DB integration on Windows

Sister of [`linux.md`](linux.md). Windows-specific notes only.

## 1. Binary setup

`plsql-mcp` uses the pure-Rust thin live-DB stack on Windows. No Oracle
Instant Client directory, `OCI.dll`, SDK zip, or `PATH` loader entry is
required for the normal live-DB path.

Build from source with the pinned nightly:

```powershell
cargo build -p plsql-mcp --release
plsql-mcp.exe doctor
```

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

- "ORA-12154 TNS:could not resolve the connect identifier":
  `TNS_ADMIN` is not set, or the alias is not in the `tnsnames.ora` in
  that directory.
- Prefer an Easy Connect string (`//host:port/service`) while validating a
  new setup; add wallet/TNS aliases once the server starts cleanly.
