# `plsql-mcp` live-DB integration on macOS

Sister of [`linux.md`](linux.md). macOS-specific notes only — refer to the
Linux walkthrough for the shared conceptual setup (connections.toml,
agent config, smoke test).

## 1. Oracle Instant Client install (macOS)

1. Apple Silicon (M-series) and Intel are both supported — pick the matching
   ZIP from
   <https://www.oracle.com/database/technologies/instant-client/macos-arm64-downloads.html>
   (or `macos-intel64-downloads.html`).
2. Quarantine: macOS Gatekeeper will refuse to load the `.dylib`s on first
   use. After unzipping, run:

   ```sh
   sudo xattr -dr com.apple.quarantine /opt/oracle/instantclient_23_8
   ```

3. Set the runtime loader path:

   ```sh
   export DYLD_LIBRARY_PATH="/opt/oracle/instantclient_23_8:$DYLD_LIBRARY_PATH"
   ```

   System Integrity Protection scrubs `DYLD_LIBRARY_PATH` from subprocesses
   under certain shells; if `plsql-mcp doctor` can't find the install, try
   exporting both `DYLD_LIBRARY_PATH` and `DYLD_FALLBACK_LIBRARY_PATH`.

4. Verify with `plsql-mcp doctor` — the `instant_client.probable_path`
   field should be populated.

## 2. Wallet setup

Same as Linux: drop the wallet directory somewhere stable and point
`TNS_ADMIN` at it. macOS-specific: tag with `xattr -dr` if the wallet zip
came from a download you opened in Safari, otherwise Gatekeeper may
refuse to read it during connection.

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

- `dlopen` errors typically mean DYLD path is scrubbed (SIP) or quarantine
  was not cleared. `xattr` + `DYLD_FALLBACK_LIBRARY_PATH` resolves both.
- For Apple Silicon, ensure the Instant Client zip you grabbed is the
  `aarch64` build — Rosetta-translated x86_64 libs are unstable inside
  long-running MCP servers.
