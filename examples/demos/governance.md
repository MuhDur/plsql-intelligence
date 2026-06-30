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
- `ResponseSanitized` — an upstream agent-facing host reports that a
  response was scrubbed before returning it to an agent

Compliance posture: every `UnknownReason` is a logged, citable gap.

## Step 3 — Trust Block

The CLI/reporting surfaces carry the same trust ingredients:
`analysis_profile`, `completeness_report`, and the diagnostic count by
`UnknownReason`. An external MCP host such as `oraclemcp` should forward
those structured fields rather than inventing a second confidence model.

State: engine and CI/CD doctor surfaces expose typed uncertainty and
remediation hints; MCP presentation belongs in `oraclemcp`.

## Step 4 — External live audit ownership

Every live-DB tool call is now owned outside this repository. The live
host should record:
- Operation name (e.g. `compile_with_warnings`).
- Connection DSN (redacted to `host:port/service` form).
- Caller program (`V$SESSION.MODULE` marker).
- Timestamp + outcome.

This repo contributes the offline impact facts and typed uncertainty
records; `oraclemcp` owns durable live audit sinks and write guards.

## Step 5 — Agent-facing response sanitization

`UnknownReason::ResponseSanitized` remains in the shared output model so
an external host can state when it scrubbed tool-call or chat-template
markers before returning data to an agent. The actual live-result
scrubbing implementation belongs with that host.

## Notes

- The compliance bar is **R13: typed uncertainty, never silent**.
  Drop into `cargo test --workspace -- --nocapture` and search for
  `UnknownReason` in the output to walk every blind spot the engine
  recorded.
