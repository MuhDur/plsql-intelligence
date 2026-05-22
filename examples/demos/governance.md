# Governance / compliance demo — doctor reports, Trust Block, audit log

You're answering: "how confident can I be that this PL/SQL analysis
is complete?" and "what's the audit trail?". Six doctor surfaces +
the Trust Block + MCP audit log are the answer.

## Setup

```sh
git clone <repo> && cd oracle
cargo build --workspace
```

## Step 1 — six doctor surfaces, one runtime

Every analysis layer ships a `*DoctorReport` with `posture`, counts,
and `remediation_hints`. Run them all:

```sh
cargo test --workspace --lib doctor -- --nocapture
```

| Layer | Report | Surfaces |
|-------|--------|----------|
| Catalog | `CatalogDoctorReport` | object counts, capability warnings, missing-permission suggestions, PL/Scope availability |
| Depgraph | `DepGraphDoctorReport` | node/edge counts, invariants, cycle detection summary |
| Lineage | `LineageDoctorReport` | orphan reports, observation-window expiry, audit-enablement recommendations |
| Privileges | `PrivilegeDoctorReport` | privilege posture, cross-schema-write surface, public-synonym hotspots |
| CICD | `ChangesetDoctorReport` | overall risk (Destructive/Caution/Unknown/Safe), per-`UnknownReason` counts, remediation hints |
| MCP | `DoctorReport` | tool registry, Instant Client posture, write-safety token state |

Each is stable JSON (serde) so you can diff successive runs.

## Step 2 — R13 typed-uncertainty discipline

The `UnknownReason` enum (`crates/plsql-core/src/lib.rs`) has 12
variants — every blind spot in the analysis pipeline is one of them:

- `DynamicSqlOpaque` — `EXECUTE IMMEDIATE` with non-literal text
- `DbLinkRemoteObject` — `@db_link` reference, opaque external
- `WrappedSource` — Oracle `WRAP`ped package body
- `MissingCatalogObject` — referenced object not in the snapshot
- `MissingPackageBody` — spec present, body missing
- `ConditionalCompilationBranch` — `$IF` branch not selected
- `EditionedObject` — child-edition variant not enumerated
- `InvokerRightsRuntimeResolution` — `AUTHID CURRENT_USER` runtime gap
- `RuntimeGrantOrRole` — role-state-dependent authorization
- `UnsupportedDialectFeature` — Oracle feature outside the parser's
  current support window (with per-feature remediation)
- `ParserRecoveryRegion` — region recovered by error-tolerant parse
- `ResponseSanitized` — K18 sanitizer rewrote a tool-call marker

Compliance posture: every `UnknownReason` is a logged, citable gap.

## Step 3 — Trust Block (planned)

`plsql-mcp` will wire a `meta.trust_block` field into every MCP
response (`PLSQL-MCP-007`). The block carries `analysis_profile`,
`completeness_report`, and the diagnostic count by `UnknownReason`
so an agent consuming `plsql-mcp` output can decide whether to act
on the data or escalate.

State: gated on PLSQL-ENG-005 (engine doctor). Track via
`oracle-ic04`.

## Step 4 — MCP audit log

Every live-DB tool call routes through `plsql-mcp::audit::AuditPlan`
which records:
- Operation name (e.g. `compile_with_warnings`).
- Connection DSN (redacted to `host:port/service` form).
- Caller program (`V$SESSION.MODULE` marker).
- Timestamp + outcome.

The audit log writer is currently in-memory; a rotating disk-backed
sink is a documented follow-up (filed during /mcp-server-design
audit, oracle-3tff). For 1.0 the audit-plan-only surface is the
compliance evidence; the rotating sink lands before GA.

## Step 5 — K18 prompt-injection sanitization

`plsql-mcp::query::sanitize` scrubs MCP / tool-call / chat-template
markers from query results before they're returned to the agent.
Coverage (33 marker families): `tool_call`, `tool_use`, antml:*
family, OpenAI tokenizer-control tokens, Llama `<<SYS>>` / `[INST]`,
chat-role prefixes. Audit-flagged: PLSQL-MCP-SEC-1.

## Notes

- The compliance bar is **R13: typed uncertainty, never silent**.
  Drop into `cargo test --workspace -- --nocapture` and search for
  `UnknownReason` in the output to walk every blind spot the engine
  recorded.
