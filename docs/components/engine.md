# plsql-engine

Canonical analysis orchestration. Layer 2.5.

## Purpose

`plsql-engine` is the single front door for every product surface in the
workspace. Consumers hand it an `AnalysisRequest`; it parses, lowers,
resolves symbols, applies catalog metadata, runs SQL semantic / value
flow / fact extraction, builds the depgraph, and returns an
`AnalysisRun` artifact that every downstream CLI (SAST, docs, lineage,
CI/CD, MCP) consumes verbatim.

## Surface

| Type | Purpose |
|------|---------|
| `AnalysisRequest` | What to analyse: file list, catalog snapshot, profile |
| `AnalysisRun` | The full output: IR + symbols + privileges + facts + depgraph + completeness report |
| `AnalysisArtifactManifest` | Index over an `AnalysisRun`'s parts so callers can stream what they need |
| `CompletenessReport` | Trust-block data: which inputs we had, which we didn't, derived UnknownReasons |
| `AnalysisProfile` | Per-run config (Oracle version target, ccflags, feature policy, redaction policy, sampling) |

## Pipeline (plan §10A)

```
project → parse → catalog → IR → symbols → privileges
                                          → sqlsem → flow → facts → depgraph
```

Each stage emits a typed slice into the `AnalysisRun`. Stages can be
skipped or re-run from cache; `plsql-store` is the cache backend.

## Invariants

- **Idempotent on cache hits.** Re-running the same request must return
  byte-identical output (modulo timing).
- **Public release-ready by default.** Every output goes through the
  redaction policy before leaving the engine, so no private literals
  escape unredacted in support bundles.
- **Pipeline failures are typed, not panics.** A missing catalog returns
  an `UnknownReason::CatalogMissing` blind spot; the run continues.

## Pointers

- Source: `crates/plsql-engine/src/`
- Plan: `plan.md` §10A (Analysis Orchestration), §22 (verification)
- Downstream: every Layer 3+ product surface
