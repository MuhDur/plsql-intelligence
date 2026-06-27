# `plsql-mcp` vs Oracle SQLcl MCP — compatibility matrix

> **Snapshot date:** 2026-06-27
> **`plsql-mcp` version:** 0.5.0 convergence branch (HEAD)
> **SQLcl MCP version:** Oracle SQLcl 24.4 / 25.x preview (Oracle's general
> availability cadence — verify the exact build at oracle.com/sqlcl before
> quoting public claims).

This matrix exists so README, sales copy, and downstream documentation
stay source-backed. Re-run it whenever either tool ships a release; the
header date and version row are the source of truth, not memory.

## Tool surface overlap

Both servers speak the [Model Context Protocol](https://modelcontextprotocol.io)
over stdio. The `MODULE` marker convention (`DBMS_APPLICATION_INFO.SET_MODULE`)
matches deliberately so DBAs reviewing `V$SESSION` see a consistent vendor
prefix across MCP servers.

| Capability                              | SQLcl MCP                                  | `plsql-mcp`                                         |
|-----------------------------------------|--------------------------------------------|-----------------------------------------------------|
| stdio transport                         | ✅ default                                 | ✅ default (`PLSQL-MCP-001`)                        |
| TCP transport                           | (operator-controlled)                      | reserved — `--listen 127.0.0.1:<port>` (planned)    |
| `tools/list` discovery                  | ✅                                         | ✅ via `ToolRegistry` (`PLSQL-MCP-001`)              |
| `DBMS_APPLICATION_INFO.SET_MODULE`      | `MODULE='SQLcl-MCP'`                       | `MODULE='plsql-mcp'` (`PLSQL-MCP-LIVE-003`)         |
| `V$SESSION.ACTION` set per call         | ✅                                         | ✅ (from agent model identifier)                    |
| Per-statement comment marker            | `/* sqlcl-mcp ... */`                       | `/* plsql-mcp <tool> <session-id> <agent-model> */` (`PLSQL-MCP-LIVE-003`) |
| Optional audit-table append             | ✅                                         | ✅ — `audit_table` per connection (`PLSQL-MCP-LIVE-003`) |
| Read-only-by-default                    | ✅ via SQLcl's `restrict` levels           | ✅ via named SafetyProfile (`PLSQL-MCP-LIVE-008`)    |
| `permanently_read_only` hard guard      | configurable via SQLcl restrict            | ✅ per-connection in `connections.toml` (`PLSQL-MCP-LIVE-009`) |
| Production-DSN doctor warning           | ❌ (no equivalent surface today)            | ✅ `MCP_PROD_DSN_WITHOUT_PERMANENTLY_READ_ONLY` (`PLSQL-MCP-LIVE-009`) |
| Single-use, time-limited enable_writes  | ❌                                          | ✅ 60s token + connection + operation-summary binding (`PLSQL-MCP-LIVE-008`) |
| K18 prompt-injection sanitization        | ❌                                          | ✅ `query` / `get_object_source` / `get_clob` (`PLSQL-MCP-LIVE-004/007`) |
| `query` with structured rows + types     | ✅                                          | ✅ (`PLSQL-MCP-LIVE-004`)                            |
| `list_objects` with cursor paging        | partial                                     | ✅ `OWNERNAME` opaque cursor (`PLSQL-MCP-LIVE-005`) |
| Structured `describe_*` tools            | partial (text-heavy)                        | ✅ table / view / trigger / index (`PLSQL-MCP-LIVE-006`) |
| `get_object_source` / `get_clob` / `get_errors` | partial                            | ✅ — structured errors + K18 sanitization (`PLSQL-MCP-LIVE-007`) |
| `compile_with_warnings` + categorization | text-only                                   | ✅ — severe / performance / informational / other (`PLSQL-MCP-LIVE-010`) |
| Connection management (`list_connections` / `connect` / `disconnect` / `current_database` / `switch_database`) | ✅ | ✅ (`PLSQL-MCP-LIVE-002`) |
| `~/.dbtools` interop                    | ✅                                          | ✅ via `DbToolsAlias::probe` (`PLSQL-MCP-LIVE-002`) |
| Oracle Wallet support                   | ✅                                          | connection-string / wallet path support through the thin `oracledb` stack |
| Instant Client requirement              | SQLcl ships its own runtime                 | ❌ no Instant Client in the normal `plsql-mcp` live-DB path |
| `OracleConnection` backend choice        | SQLcl-internal                              | trait-isolated (`oraclemcp-db` -> `oracledb`; legacy thick path pending removal) |
| Static-analysis tool surface             | ❌ (SQLcl is connectivity-first)            | ✅ — parser / catalog / depgraph / completeness + lineage / SAST / CICD / docs / bindings |
| `what-breaks` / change classification    | ❌                                          | ✅ (`change_tools`)                                  |
| `release_gate` / `recompile_plan`        | ❌                                          | ✅ (`change_tools`)                                  |
| `sarif_scan`                             | ❌                                          | ✅ (`change_tools`)                                  |
| `orphan_candidates`                      | ❌                                          | ✅ (`change_tools`)                                  |

## Capability gaps — `plsql-mcp` does NOT cover today

These are deliberate carve-outs or open work; do not claim parity until
the matching bead lands.

| Gap                                                  | Tracking bead       | Mitigation today                                                |
|------------------------------------------------------|---------------------|-----------------------------------------------------------------|
| `verify <changeset>` against an XE container         | `PLSQL-CICD-005`    | Use `cargo run -p plsql-catalog --example generate_lab_snapshots` for fixture-based dry runs. |
| `predict <changeset>` Oracle invalidation rules      | `PLSQL-CICD-002`    | `plsql-cicd` ships `ChangeSet` / `InvalidationPrediction` types today; the rule engine is the next bead. |
| Lineage `compare-oracle-deps` customer report        | `PLSQL-LIN-016`     | Implemented in commit `7c66451`; see lineage REPORT.md.         |
| `patch_package` (targeted REPLACE-based edit)        | `PLSQL-MCP-LIVE-012`| Use `compile_with_warnings` after manual `CREATE OR REPLACE`.   |
| TCP transport for remote sessions                     | `PLSQL-MCP-002`     | stdio works in every MCP client today; remote sessions deferred. |
| Per-platform thin-driver setup walkthroughs           | `oracle-plsql-converge-0lnu.10.5` | Updated — `docs/integrations/live-db/{linux,macos,windows}.md`. |

## Areas where `plsql-mcp` differs *intentionally*

1. **K18 sanitization.** SQLcl MCP does not scrub prompt-injection markers
   in row values. `plsql-mcp` does — the
   `crates/plsql-mcp/src/query.rs::sanitize` function reads `injection_markers()`
   built at runtime so the source file does not itself carry the literal
   tool-call shapes. Scrubbed cells get a `sanitized` flag plus a
   response-level `UnknownReason::ResponseSanitized` so downstream tooling
   can render a "this was rewritten" badge.
2. **`permanently_read_only`.** `plsql-mcp` exposes it as a per-connection
   TOML field with a hard guard — `enable_writes` refuses regardless of
   safety profile + active token + operator confirmation. SQLcl restrict
   levels approximate the effect but have no immutable connection-level
   marker.
3. **Static-analysis surface.** `plsql-mcp` ships the parser / catalog /
   depgraph / completeness tools alongside live-DB connectivity. SQLcl
   MCP is connectivity-first. The change-impact tools (lineage / SAST /
   CICD / orphan-candidates) ship in the same `plsql-mcp` binary, in
   module `change_tools`.
4. **Audit-trail shape.** Both tools embed a per-statement comment marker;
   `plsql-mcp`'s is `/* plsql-mcp <tool> <session-id> <agent-model> */`
   (operator can grep agent-model directly).

## Refresh procedure

When SQLcl ships a new release:

1. Read the SQLcl MCP release notes / changelog on
   <https://www.oracle.com/database/sqldeveloper/technologies/sqlcl/>
   (router pointer per the `/oracle` skill `CLIENT-TOOLS-REFERENCE.md`).
2. Walk each row in this matrix and update the SQLcl column.
3. If a new capability lands on SQLcl that `plsql-mcp` does not cover,
   file a bead under the relevant bead family (`PLSQL-MCP-LIVE-*`) and
   add a row to the Capability gaps section.
4. Bump the snapshot date + SQLcl version in the header.
5. Run `tools/corpus-license-check` to confirm no inadvertent corpus
   churn (this file lives under `docs/`, which the tool does not enforce,
   but the routine is worth running anyway).

## Cross-references

- [`plan.md`](../../../plan.md) §13A — MCP Adapter Surface.
- [`plan.md`](../../../plan.md) §2.2 — Boundary with the wider Track B Live-DB Oracle MCP.
- [`README.md`](README.md) — Per-platform install pointer.
- [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) — Live-DB feature posture across the workspace.
