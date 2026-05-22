# plsql-mcp — MCP server directory listing

Submission-ready artifacts for listing `plsql-mcp` in the
official MCP servers index and the community `awesome-mcp` lists.

> **Scope note (R13 honesty).** This file *prepares* the
> submission; it does not perform it. Submitting to
> `modelcontextprotocol/servers` and `awesome-mcp-servers` means
> opening a pull request against those external repositories —
> an outbound, human-reviewed action that is intentionally **not**
> automated here. A maintainer copies the entries below into the
> respective PRs. Treat "submitted" as a manual operational step
> tracked outside the bead graph.

## Server manifest entry

For `modelcontextprotocol/servers` (README "Community Servers" /
`servers.json`-style entry):

```json
{
  "name": "plsql-intelligence",
  "description": "Static analysis for Oracle PL/SQL: dependency graph, lineage, dynamic-SQL/taint evidence, SAST findings, change-impact tools, and completeness reporting. Apache-2.0 OR MIT; no live database required.",
  "vendor": "plsql-intelligence",
  "sourceUrl": "https://example.invalid/plsql-intelligence",
  "license": "Apache-2.0 OR MIT",
  "runtime": "native binary (Rust)",
  "transport": ["stdio"],
  "command": "plsql-mcp",
  "args": ["serve"]
}
```

(Replace `sourceUrl` with the public repository URL at submission
time — left as a non-resolving placeholder here per the no-guessed-URLs
rule.)

## awesome-mcp-servers entry

Markdown list line (under the "Databases" or "Developer Tools"
section):

```markdown
- [plsql-intelligence](https://example.invalid/plsql-intelligence) - Static analysis for Oracle PL/SQL: dependency/lineage graph, dynamic-SQL & taint evidence, SAST findings, completeness reporting. Stdio MCP, no live DB. (Rust, Apache-2.0/MIT)
```

## One-line description (≤ 160 chars, for indexes that cap it)

```
Static analysis for Oracle PL/SQL — dependency/lineage graph, dynamic-SQL & taint evidence, SAST findings, completeness reporting. Stdio MCP, no live DB.
```

## Tool surface advertised

`plsql-mcp` ships a single open-source tool surface. Static-analysis
tools a directory entry should mention: `analyze_project`,
`find_callers`, `find_callees`, `get_dependencies`,
`dynamic_sql_evidence`, `completeness_report`, `doc_lookup`. The
change-impact tools (module `change_tools`) add `what_breaks`,
`classify_change`, `compare_oracle_deps`, `sarif_scan`,
`orphan_candidates`, `explain_lifecycle`, `release_gate`,
`recompile_plan`. Every tool is available to everyone — there is no
license gate.

## Submission checklist (for the maintainer doing the PRs)

- [ ] Public source URL is live and replaces every `example.invalid`.
- [ ] README has an install + MCP-client config section (see
      [`mcp-clients.md`](mcp-clients.md)).
- [ ] `plsql-mcp doctor --robot-json` output pasted into the PR as
      proof the server starts and lists its tools.
- [ ] LICENSE files present (Apache-2.0 OR MIT for `plsql-mcp`).
- [ ] Entry added to `modelcontextprotocol/servers` via PR.
- [ ] Entry added to `awesome-mcp-servers` via PR.
- [ ] Both PR links recorded in the project tracker (operational,
      outside the bead graph).
