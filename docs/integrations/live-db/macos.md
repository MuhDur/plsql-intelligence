# `plsql-mcp` live-DB integration on macOS

Sister of [`linux.md`](linux.md). macOS-specific notes only — refer to
the Linux walkthrough for the shared conceptual setup
(`connections.toml`, agent config, smoke test).

## 1. Binary setup

`plsql-mcp` uses the same pure-Rust thin live-DB stack on Apple Silicon
and Intel Macs. No Instant Client `.dylib`, `DYLD_LIBRARY_PATH`, or
Gatekeeper quarantine step is required for the normal live-DB path.

Build from source with the pinned nightly:

```sh
cargo build -p plsql-mcp --release
plsql-mcp doctor
```

## 2. Wallet setup

Same as Linux: drop the wallet directory somewhere stable and point
`TNS_ADMIN` at it. macOS-specific: tag with `xattr -dr` if the wallet zip
came from a download you opened in Safari, otherwise Gatekeeper may
refuse to read the wallet files during connection.

## 3. Editor / agent config

### Claude Code (macOS)

`~/Library/Application Support/Claude/claude-code/mcp-servers.json`:

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

### Cursor (macOS)

`~/.cursor/mcp.json` — same content as the Linux example.

## 4. Troubleshooting

- If wallet files came from Safari and connections fail with file-access
  errors, clear quarantine on the wallet directory with `xattr -dr`.
- Prefer Easy Connect strings while validating a new setup; add wallet/TNS
  aliases after `plsql-mcp doctor` is clean.
