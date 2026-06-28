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
  "description": "Superset Oracle MCP server: live DB inspection/guarded writes plus PL/SQL dependency graph, lineage, SAST, change-impact, and completeness reporting. Apache-2.0 OR MIT.",
  "vendor": "plsql-intelligence",
  "sourceUrl": "https://github.com/MuhDur/plsql-intelligence",
  "license": "Apache-2.0 OR MIT",
  "runtime": "native binary or OCI image (Rust)",
  "transport": ["stdio"],
  "command": "plsql-mcp",
  "args": ["serve"]
}
```

Dual-tier positioning: `oraclemcp` is the lean live-Oracle access server;
`plsql-mcp` is the superset that adds offline PL/SQL intelligence and the
guarded change-impact surface.

## awesome-mcp-servers entry

Markdown list line (under the "Databases" or "Developer Tools"
section):

```markdown
- [plsql-intelligence](https://github.com/MuhDur/plsql-intelligence) - Superset Oracle MCP server: live DB tools plus PL/SQL dependency/lineage graph, SAST, change-impact, and completeness reporting. (Rust, Apache-2.0/MIT)
```

## One-line description (≤ 160 chars, for indexes that cap it)

```
Superset Oracle MCP server — live DB tools plus PL/SQL dependency/lineage graph, SAST, change-impact, and completeness reporting.
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

- [ ] Public source URL is live.
- [ ] README has an install + MCP-client config section (see
      [`mcp-clients.md`](mcp-clients.md)).
- [ ] `plsql-mcp doctor --robot-json` output pasted into the PR as
      proof the server starts and lists its tools.
- [ ] LICENSE files present (Apache-2.0 OR MIT for `plsql-mcp`).
- [ ] Entry added to `modelcontextprotocol/servers` via PR.
- [ ] Entry added to `awesome-mcp-servers` via PR.
- [ ] Both PR links recorded in the project tracker (operational,
      outside the bead graph).
