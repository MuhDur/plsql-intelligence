# plsql-intelligence — Technical Architecture Report

> **Date:** 2026-05-15
> **Workspace:** `oracle/` (project key `plsql-intelligence`)
> **Author:** GentleTrout (Claude Code, `claude-opus-4-7`) compiling the
> session arc with FuchsiaRobin (Claude Code) under the `prompt.md` swarm
> coordination protocol.

## Executive summary

`plsql-intelligence` is a Rust Cargo workspace that ships **offline-first
Oracle PL/SQL code intelligence**. The headline product story is the one
sentence on `README.md:5`:

> *"Know what breaks before you change Oracle PL/SQL."*

The repository implements that thesis as a layered toolchain (Layer 0 →
Layer 5 per `plan.md` §5) covering a tolerant PL/SQL parser, an
offline-first Oracle catalog snapshot model, semantic IR, symbol /
privilege / sqlsem / flow / facts / depgraph layers, lineage + SAST + docs
+ bindings + CI-cascade product surfaces, and a single MCP adapter for AI
coding agents (`plsql-mcp`, Apache-2.0 OR MIT).

**Key stats** (2026-05-15):
- 17 workspace crates + 2 tools (`tools/plan-lint`, `tools/corpus-license-check`).
- ~31,800 lines of Rust across `crates/` and `tools/`.
- 416 `#[test]` functions; 4 integration-test files.
- `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check`: all green at `HEAD = e4bc88c`.

## Entry points

| Entry                          | Location                                                     | Purpose                                                                                                 |
|--------------------------------|--------------------------------------------------------------|---------------------------------------------------------------------------------------------------------|
| `plsql-mcp` binary             | `crates/plsql-mcp/src/main.rs:47`                            | MCP adapter. `serve` / `doctor` / `info` subcommands; `--robot-json` global flag.                       |
| `plsql-depgraph` binary        | `crates/plsql-depgraph/src/main.rs:152`                      | Query/explain/graphml CLI over the dependency graph. Distinct exit codes per `PLSQL-CLI-ERG-001`.       |
| `plan-lint` tool               | `tools/plan-lint/src/main.rs`                                | Structural integrity checker for `plan.md` (7 rules; `--robot-json` + `--doctor`).                      |
| `corpus-license-check` tool    | `tools/corpus-license-check/src/main.rs`                     | Verifies every file under `corpus/public/` has a matching `[[file]]` entry in `corpus/manifest.toml`.   |
| Library entry: catalog loader  | `crates/plsql-catalog/src/lib.rs:1502 load_snapshot_from_connection` | Pipes a live `OracleConnection` through 12 dictionary loaders + capability negotiation.                 |
| Library entry: lineage `impact`| `crates/plsql-lineage/src/lib.rs::impact`                    | Downstream impact traversal with confidence aggregation.                                                |

## Key types

| Type                                  | Location                                          | Purpose                                                                                          |
|---------------------------------------|---------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `CatalogSnapshot`                      | `crates/plsql-catalog/src/lib.rs:173`             | Top-level offline-first Oracle dictionary model — schemas, capabilities, source, interner.       |
| `CatalogDoctorReport`                  | `crates/plsql-catalog/src/lib.rs:299`             | Structured doctor surface for snapshots (object counts, capability warnings, grant suggestions). |
| `DepGraph` + `EdgeKind`                | `crates/plsql-depgraph/src/lib.rs`                | Three-layer node identity + evidence-bearing edges + Oracle dictionary cross-check.              |
| `LineageResult` / `AffectedNode`       | `crates/plsql-lineage/src/lib.rs`                 | `impact()` / `dependencies()` traversal result with confidence aggregation.                      |
| `ChangeSet` + `InvalidationPrediction` | `crates/plsql-cicd/src/lib.rs`                    | Foundational CI/CD types (`PLSQL-CICD-001`).                                                     |
| `BindingPlan` + `OracleType`           | `crates/plsql-bindgen/src/{lib.rs,type_mapping.rs}` | Per-package binding IR + §12.3 Oracle → Rust type mapping.                                       |
| `DoctorReport` (MCP)                   | `crates/plsql-mcp/src/doctor.rs:22`               | Live-DB build status, Instant Client posture, audit posture, per-connection write posture.       |
| `SessionSafetyState` + `EnableWritesToken` | `crates/plsql-mcp/src/safety.rs`              | Read-only-by-default session guard + single-use 60s confirmation token (`PLSQL-MCP-LIVE-008`).   |
| `Diagnostic` + `UnknownReason`         | `crates/plsql-core/src/lib.rs:313`                | Layer-0 typed diagnostic surface. R13 invariant: every blind spot is a `UnknownReason` variant.  |

## Data flow

The product surfaces all consume an `AnalysisRun` produced by the engine.
The two main flows are **catalog extraction** (offline-first) and
**downstream impact analysis**.

```
        ┌──────────────────────────┐
        │ source files / DDL / git │
        └────────────┬─────────────┘
                     │
                     ▼
        ┌──────────────────────────┐         ┌──────────────────────────────┐
        │   plsql-parser           │         │   plsql-catalog              │
        │   (ANTLR backend in      │         │   load_snapshot_from_*       │
        │    parser-antlr crate;   │         │   (live OracleConnection or  │
        │    ParseBackend trait    │         │    DBMS_METADATA dir or JSON)│
        │    isolates downstream)  │         │   + negotiate_capabilities() │
        └────────────┬─────────────┘         └────────────┬─────────────────┘
                     │ AST                                │ CatalogSnapshot
                     ▼                                    ▼
        ┌──────────────────────────────────────────────────────────┐
        │            plsql-engine (Layer 2.5)                       │
        │  AnalysisRun = parse + catalog + IR + symbols + flow      │
        │  + facts + depgraph + CompletenessReport (R13)            │
        └────────────┬─────────────────────────────────────────────┘
                     │
        ┌────────────┼──────────────┬─────────────────┬─────────────────┐
        ▼            ▼              ▼                 ▼                 ▼
   plsql-doc    plsql-bindgen   plsql-lineage    plsql-cicd        plsql-mcp
   (Markdown    (per-package    (impact /        (predict /        (MCP server:
    + HTML +    BindingPlan +   dependencies /   plan / gate /     list_objects,
    object     OracleType map / what-breaks /    verify / lifec.   query, source,
    pages)     OracleDateTime  classify-change)  classifier)       compile,
                wrappers)                                          change tools)
```

The R13 invariant ("no uncertainty silently dropped") means every layer
emits a typed `UnknownReason` instead of guessing — `CompletenessReport`
percolates through `AnalysisRun` into every product surface so customer
reports carry an explicit Trust Block (`plan.md` §1.5).

`CompletenessReport` (schema `plsql.engine.analysis_run` ≥ 1.1.0,
oracle-bh4p) carries **honest extraction signals** so a near-pristine
file tally can never masquerade as a clean run: `posture`
(`Clean`/`Partial`/`LowConfidence`/`Degraded`, derived — never `Clean`
on a low-extraction run), `objects_unrecognized` (top-level objects the
classifier could not lower; `IR_UNCLASSIFIED_DECL`), `diagnostics_total`,
and `extracted_semantics_ratio`. Structurally not-yet-wired gap metrics
(`dynamic_sql_sites`, `unresolved_references`, `db_link_edges`,
`opaque_dynamic_sql_sites`, `wrapped_units`, `missing_package_bodies`)
serialise as the honest `Measured::Unmeasured` form
(`{ "unmeasured": true }`) instead of a misleading `0` that a reader
could confuse with "looked, found none".

## External dependencies

| Dependency          | Purpose                                                                                              | Critical?                                              |
|---------------------|------------------------------------------------------------------------------------------------------|--------------------------------------------------------|
| `rust-oracle` 0.6   | Live-DB connection (loaded behind `oracle-driver` Cargo feature; gated on Instant Client at runtime) | Yes (live-DB feature)                                  |
| `antlr-rust` 0.3    | ANTLR4 runtime for the parser backend (codegen behind `antlr-codegen` feature)                       | Yes (parser)                                           |
| `chrono` 0.4        | Temporal type wrappers; `CatalogSnapshot.generated_at`, `OracleDateTime` default backend             | Yes                                                    |
| `serde` / `serde_json` | Every public type is serde-derived; JSON snapshots round-trip via `CatalogSnapshotDocument`       | Yes                                                    |
| `sha2`              | Content-addressed hashes for view query text, mview query text, source hashes                        | Yes                                                    |
| `toml` 0.8 (in mcp) | `~/.plsql-mcp/connections.toml` loader (`PLSQL-MCP-LIVE-009`)                                        | Yes (MCP)                                              |
| `clap` 4            | CLI argument parsing for `plsql-mcp`, `plsql-depgraph`, plan-lint, corpus-license-check              | Yes (CLI surfaces)                                     |
| `miette` 7          | Human-readable diagnostics (downstream of `thiserror`)                                               | Indirect                                               |
| `thiserror` 2       | Library-level error enums (used in every crate)                                                      | Yes                                                    |
| `tracing` 0.1       | Structured logging; spans on every public API call per `AGENTS.md`                                   | Yes                                                    |
| Oracle Instant Client (runtime) | Required by `rust-oracle` when the `live-db` feature is on                              | Yes (live-DB)                                          |

## Configuration

| Source                                   | Priority  | Example / Notes                                                                                                  |
|------------------------------------------|-----------|------------------------------------------------------------------------------------------------------------------|
| `~/.plsql-mcp/connections.toml`           | Per-user  | `[[connection]]` tables w/ `name`, `connect_string`, `username`, `permanently_read_only` (`PLSQL-MCP-LIVE-009`). |
| `~/.dbtools`                              | Per-user  | Mirrored by `DbToolsAlias::probe` so SQLcl / SQL Developer aliases work verbatim (`PLSQL-MCP-LIVE-002`).         |
| `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` / `ORACLE_HOME` | Env       | Doctor's Instant Client detection (`PLSQL-MCP-LIVE-001`).                                            |
| `TNS_ADMIN`                               | Env       | Wallet directory; resolved by `rust-oracle` at connect time.                                                     |
| `--robot-json` global flag                | Per-call  | Every CLI surface honors it (R10/R11). Distinct schemas registered in `LINEAGE_SCHEMAS` + per-tool envelopes.    |
| `OracleTargetVersion` (parser)            | Per-call  | Drives dialect-feature diagnostic emission via `unsupported_dialect_feature_diagnostic` (`PLSQL-DIALECT-003`).   |
| Cargo features `live-db`, `oracle-driver`, `antlr-codegen` | Build-time | Gate Instant Client + ANTLR codegen so static-only builds stay fast.                          |
| `.plsql-bindgen.toml` (planned)           | Per-project | Manual row-shape overrides for REF CURSOR (`PLSQL-BG-008`), datetime backend selection (`PLSQL-BG-016`).        |
| `.plsql-cicd-policy.toml` (planned)       | Per-project | Release-gate thresholds + predict-mode default (`PLSQL-CICD-006`).                                              |

## Test infrastructure

| Type                       | Location                                                         | Count                       |
|----------------------------|------------------------------------------------------------------|-----------------------------|
| Unit tests                 | `#[cfg(test) mod tests]` inside each crate's `lib.rs` / submodules | 416 `#[test]` functions     |
| Integration tests          | `crates/*/tests/*.rs`                                            | 4 files; `plsql-parser/tests/dialect_features.rs` is the model |
| Property tests             | `proptest` is a dev-dep of `plsql-parser`                         | Used opportunistically       |
| Conformance harness        | `crates/plsql-parser/tests/conformance.rs`                       | One per backend (placeholder) |
| Golden artifact tests      | `plsql-render` golden snapshots; `plsql-depgraph` GraphML stability tests | Embedded                  |
| Lint gate                  | `cargo clippy --workspace --all-targets -- -D warnings` in `.github/workflows/ci.yml` | CI-enforced |
| Format gate                | `cargo fmt --all -- --check`                                     | CI-enforced                 |
| Corpus license check       | `tools/corpus-license-check` runs in CI (`PLSQL-WS-015`)         | 47 entries / 1 enforced root |
| Plan structural check      | `tools/plan-lint` runs in CI; 7 rules                            | `PLSQL-PLAN-001/002`        |

The L1 / L2 lab corpora live under `corpus/synthetic/l1` + `corpus/lab/`
(plan §6.2.8.1). Public Oracle sample schemas (HR/OE/SH) and antlr/grammars-v4
PL/SQL examples are vendored under `corpus/public/` with manifest entries
(`PLSQL-WS-012/013`).

## Notes & gotchas

- **Shared working tree race.** Two MCP / Claude Code agents in the same
  repo can stage each other's WIP if either uses `git add -A` / `commit -a`.
  Use `git commit -- <pathspec>` only. The 97c3651 retrospective entry in
  `CHANGELOG.md` documents the one time this happened in this session.
- **`/tmp/cargo-target` is a tmpfs.** 124 GiB total; both agents share it.
  Periodically clear `debug/incremental` (28 GiB at peak) when free space
  drops below ~10 GiB; coordinate via `[urgent]` Agent Mail before any
  destructive cargo clean.
- **R20 — parser backend isolation.** No downstream crate may depend on
  ANTLR-generated types; `plsql-parser-antlr/src/lib.rs` documents this
  invariant. Plug-in replacement backends (Java subprocess, tree-sitter)
  must implement `ParseBackend` only.
- **R17 — no telemetry by default.** Every binary the project ships must
  operate offline. `plsql-mcp` audit posture writes to `stdout` /
  configured per-connection audit tables only; no outbound calls.
- **`permanently_read_only` is the hardest guard.** It overrides safety
  profile, session token, and `--dangerously-verify-in-place`. The doctor
  surfaces `MCP_PROD_DSN_WITHOUT_PERMANENTLY_READ_ONLY` warnings whenever a
  production-looking connect string lacks it.
- **K18 prompt-injection sanitization** is built at runtime via `format!`
  so the source file does not itself carry the literal MCP / tool-call
  shapes downstream parsers might react to (see `query::sanitize`,
  `source::run_get_object_source`).
- **Hash type evolution.** `CatalogSnapshot` stores SHA-256 hashes
  (`sha256:<hex>`) for view + materialized-view bodies and DDL via
  `plsql-catalog::Hash`. Downstream change detection compares stable
  hashes rather than free text.
- **Wrapped material.** Anything from a private Oracle PL/SQL estate
  is local-only per AGENTS.md C5/C6 — never copied into this repo's
  history. Test patterns are re-synthesized from grammar + descriptions.

## Cross-references

- [`plan.md`](../plan.md) §1.4 — Commercial nucleus + product family rationale.
- [`plan.md`](../plan.md) §5 — Layer 0 → Layer 5 architecture.
- [`plan.md`](../plan.md) §12.3 — Oracle → Rust type mapping table
  (implemented at `crates/plsql-bindgen/src/type_mapping.rs`).
- [`plan.md`](../plan.md) §13A — MCP Adapter Surface (`plsql-mcp`).
- [`AGENTS.md`](../AGENTS.md) — Repo operating rules; the destructive-ops
  hard list, R-rule pointers, and Beads + Agent Mail + CASS conventions.
- [`CHANGELOG.md`](../CHANGELOG.md) — Unreleased entries enumerate every
  bead closed under this multi-agent session arc.
- [`docs/integrations/live-db/`](integrations/live-db/) — Per-platform
  Instant Client setup, wallet configuration, and editor / agent config
  snippets (`PLSQL-MCP-LIVE-020`).

## Session delta (2026-05-17)

Corrections and additions since the 2026-05-15 snapshot above.

**Corrected stats.** The workspace is **24 crates** (`ls crates/ | wc -l`),
not 17; 167 `.rs` source files. All 24 are unsafe-free and the workspace
is now **100 % `#![forbid(unsafe_code)]`** (the last two crates,
`plsql-doc` and `plsql-lineage`, got the attribute this session; forbid
verified airtight by clean builds).

**Autonomous live-DB environment.** `examples/oracle-xe/docker-compose.gvenzl.yml`
+ `make demo-oracle-xe-ci` boot `gvenzl/oracle-free:23-slim` (Docker Hub,
no Oracle SSO / FUTC wall) as a drop-in for the licence-walled
`container-registry.oracle.com` image, so CI and agents run the live-DB
suites unattended. A `DOCKER_COMPOSE` autodetect in the Makefile falls
back to standalone `docker-compose` when the v2 plugin is absent.

**Live-DB integration surface (now wired + verified).** A feature-gated
`live-xe = ["oracle-driver"]` test pattern (gate-off trivial stub +
gated real test, mirroring `crates/plsql-bindgen/tests/xe_roundtrip.rs`)
runs across `plsql-catalog`, `plsql-cicd`, `plsql-mcp`. Delivered + live-
verified against the container: catalog snapshot golden (`PLSQL-CAT-008`);
`plsql-cicd::verify` against a throwaway `VERIFY_T_*` scratch schema with
RAII teardown + no-rollback contract (`PLSQL-CICD-005`) and the
interactive in-place safety gate `confirm_in_place_verification`
(`PLSQL-CICD-005A`); predict→plan→verify cycle (`PLSQL-CICD-010`); every
live-DB MCP tool E2E + chained preview→execute_approved + a 7-case
refusal matrix (`PLSQL-MCP-LIVE-018`); and two hero-demo golden
transcripts (`PLSQL-MCP-LIVE-019`).

**§1.4 DROP COLUMN hero is now real.** `corpus/lab/l1/` (a `customers`
table with `legacy_segment` + view/package/procedure dependents) and
`corpus/lab/hero_diff_dropcol/` implement the plan §1.4
`DROP COLUMN customers.legacy_segment` narrative the commercial thesis
promises; `crates/plsql-mcp/tests/hero_demo_dropcol_live_xe.rs` proves
Oracle marks the dependents `INVALID` end-to-end. The older param-rename
`corpus/lab/hero_diff/` (LAB-002 lineage showcase) is deliberately kept
as a separate fixture.

**TCP transport is implemented.** `crates/plsql-mcp/src/tcp.rs::serve`
binds the validated `ListenTarget`, accepts connections, and pumps
line-delimited JSON-RPC through `mcp_protocol::handle_request_line`;
`transport::is_transport_implemented(Tcp)` is now `true`. The
serve-command startup wiring that selects stdio vs TCP remains
`PLSQL-MCP-002` (oracle-vnlk).

**Agent contract surface.** `plsql-mcp capabilities` emits a pinned
machine-readable contract (`contract_version`, transports, command list,
exit-code dictionary, feature flags) honouring `--robot-json`
(single-line), guarded by a drift-test coupled to
`CAPABILITIES_CONTRACT_VERSION`.

**Quality / verification infrastructure.** `fuzz/` is a detached
cargo-fuzz crate; target `parse_lower` drives the real text-scanning
pre-parser (`plsql_parser_antlr::lower::lower_source`) into
`plsql_ir::lower_top_level` with a never-panic + determinism oracle
(>1300 exec/s, coverage-growing, 0 crashes over 400 K+ executions).
`beads_compliance_audit/` (its own git, gitignored from the project)
holds a converged 2-pass `/beads-compliance-and-completion-verification`
run + a real Phase-10 fresh-eyes review: of 383 closed beads exactly one
was genuinely false-closed (`oracle-d812` `--listen`), remediated and the
missing TCP accept loop implemented.
