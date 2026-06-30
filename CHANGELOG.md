# Changelog

All notable changes to plsql-intelligence are recorded here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 the API can move at any time; the changelog still captures every
substantive change so contributors and downstream tooling can navigate the
work.

## [Unreleased]

- No unreleased changes.

## [0.7.0] - 2026-06-30

- **Offline-pivot release.** The `plsql-mcp` workspace crate, live Oracle
  catalog/CICD code, MCP server build workflows, Dockerfile, and server
  manifest were removed from this repository. MCP serving now belongs in
  the separate `oraclemcp` repository; this repo ships only the offline
  PL/SQL engine and CLI crates.
- **Version alignment.** The product crates and the `usr-loop` CLI are now
  aligned on `0.7.0`, with internal path dependency constraints updated
  for crates.io publication and `Cargo.lock` regenerated.
- **Stable offline CLI expansion.** The `plsql` CLI now exposes offline
  `doc` and `sast` subcommands backed by `plsql-doc` and `plsql-sast`,
  so documentation and static-analysis outputs are available without the
  MCP/live-DB stack.
- **Parser backend cleanup.** The retired Java-worker backend choice was
  removed from the engine surface, while the ANTLR runtime upgrade was
  re-probed and deliberately deferred: the current generated parser stays
  on `antlr-rust 0.3.0-beta`, with the `antlr4rust 0.5.x` migration filed
  as future parser-codegen work instead of blocking the 0.7.0 ship.
- **oraclemcp handoff.** `oraclemcp` now has a `plsql-intelligence`
  feature, catalog rowset extraction for the PL/SQL engine seam,
  feature-gated intelligence tools, and migrated live-XE hero coverage
  for the `DROP COLUMN customers.legacy_segment` scenario.

## [0.6.3] - 2026-06-29

- **Generated parser sources.** ANTLR-generated Rust parser output is now
  committed under the parser crate, so normal parser builds no longer
  require Java. Regeneration is an explicit drift-check path.
- **Stable pivot gates.** The stable workspace checks were verified with
  `plsql-mcp` outside the default member set, preserving the nightly-only
  MCP/live transition while keeping the offline engine buildable on
  the stable toolchain.
- **Catalog snapshot seam.** `plsql-catalog` exposes the stable
  `CatalogSnapshotBuilder`/rowset API used by the downstream oraclemcp
  integration path.

## [0.6.2] - 2026-06-29

- **Trio stack doctor provenance.** `plsql-mcp doctor` now reports the
  exact `plsql-mcp -> oraclemcp-* -> oracledb` live stack, including
  `oraclemcp 0.4.0`, `oracledb 0.5.0`, and the upstream gap issues filed
  from `plsql-mcp`.
- **Agent doctor parity.** `plsql-mcp` now accepts the `--robot` alias,
  `doctor --robot-triage`, parity flags such as `--skip`, `--since`,
  `--severity`, `--budget`, `--quiet`, `--no-color`, and `--no-progress`,
  latest-run `doctor diff`, date-gated `doctor gc`, and guarded
  `--force --yes` semantics.
- **Forensic run contract fix.** Diagnose-only doctor runs now write an
  empty `actions.jsonl`, which is valid empty JSONL instead of a comment
  line masquerading as a log record.

## [0.6.1] - 2026-06-29

- **Doctor contract hardening.** `plsql-mcp doctor` now exposes
  `doctor fix` as an explicit forensic/no-op fixer contract, adds
  `doctor explain <finding-id>`, publishes detector metadata and manual
  remediations in `doctor capabilities`, and writes a complete local run
  bundle: `report.json`, `actions.jsonl`, `scorecard.json`, `stdout.json`,
  `report.md`, and `backups/`.
- **Published build fix.** The `plsql-mcp` crate now enables the
  `plsql-catalog/oraclemcp-db` adapter feature it compiles against, so the
  default published MCP build can use the catalog `OracleConnection` seam.
- **Stable default workspace hardening.** The default Cargo workspace and
  USR gate now exclude the nightly-only `plsql-mcp` live stack; stable gates
  cover the offline engine while the MCP/live stack remains verified on the
  pinned nightly.
- **Upstream timeout issue clarified.** `oraclemcp#4` now carries concrete
  timeout/cancellation acceptance criteria for the `plsql-mcp ->
  oraclemcp-db -> oracledb` path, including the rule for splitting a
  driver-only reproduction to `rust-oracledb` if investigation proves the
  hang lives below the adapter.

## [0.6.0] - 2026-06-28

- **Trio stack-doctor parity.** `plsql-mcp doctor` now follows the
  same agent-facing shape as the lower stack rungs: stable JSON, ordered
  checks, summary counts, explicit exit codes, local run artifacts,
  `doctor health`, `doctor capabilities`, `doctor robot-docs`, `doctor ls`,
  `doctor diff`, and `doctor undo`. `--json` is a visible alias for
  `--robot-json`.
- **Publication hardening.** Release-facing crates are bumped to the
  `0.6.0` line where appropriate, internal `plsql-*` path dependencies now
  carry crates.io version requirements, and the MCP/GHCR workflow defaults
  point at `0.6.0`.
- **Upstream driver gaps filed from `plsql-mcp`.** The remaining driver
  work is tracked upstream instead of being reimplemented here:
  `rust-oracledb#13` for OUT / IN OUT bind ergonomics,
  `oraclemcp#2` for routine execution through `oraclemcp-db`, and
  `oraclemcp#3` for typed or explicitly non-lossy catalog/value
  serialization.
- **Known live-XE timeout gap.** The CI live-wire Oracle 23ai test passed,
  but the broader local live-XE suite exposed an adapter timeout gap now
  tracked as `oraclemcp#4` and local bead `oracle-tdgx`: a thin connection
  can hang instead of returning a typed TNS timeout after Oracle reports
  `TNS-12535`.

### Hardening, MCP agent-UX, and reality-check (2026-06)

A multi-pass remediation sweep across the whole workspace. The suite stayed
green throughout (2603 tests, zero failures; `clippy -D warnings` clean;
`cargo-deny` clean; the `oraclemcp-*` one-way boundary lint clean).

- **Correctness and security hunt (converged).** Thirteen rounds of
  adversarial multi-agent bug-hunting with per-finding ground-truth
  calibration fixed 122 distinct correctness/security defects plus one
  production performance bug, then converged (the last rounds found no new
  reachable high-severity defect). Whole defect classes were closed: SQL-guard
  fail-closed invariants (buried `;`, trailing SQL after `END`, buried verbs,
  whitespace/comment-insensitive `EXECUTE IMMEDIATE`), code-generation
  injection in `plsql-bindgen`, unbounded recursion in expression/statement
  lowering (typed depth caps), UTF-8 char-boundary slicing, taint
  fail-closed-on-unanalyzable, dependency-extraction completeness (legacy comma
  joins, string-literal masking), and an inverted edge-direction in
  `plsql-lineage::dependencies`/`impact`.
- **MCP server agent-ergonomics.** The shipping `plsql-mcp` wire now advertises
  a real argument JSON-Schema and read-only / destructive annotations per tool;
  returns structured error envelopes with a machine class and a fuzzy
  "did you mean" suggestion on an unknown tool; ships a zero-argument
  `oracle_capabilities` discovery tool plus `initialize` orientation
  instructions; gates the advertised surface by build feature and safety
  profile so a static-only build does not present unrunnable live tools as
  callable; normalizes Oracle identifier case on `describe_*` / `list_objects`
  so natural lowercase input resolves; and attaches `next_actions` workflow
  hints to successful results.
- **Honest-uncertainty fidelity.** Parser-recovery diagnostics are now tagged
  with `UnknownReason::ParserRecoveryRegion`, so the accretion gap classifier
  sees them as typed degradation rather than a grammar/lowering repair
  candidate. The CI-cascade `predict` path finalizes its completeness posture
  instead of serializing a default-pessimistic `Degraded`. Engine-built
  dependency edges now carry the real resolution strategy and a populated
  evidence payload rather than a hardcoded constant and `None`.
- **Performance.** `DepGraph::query_path` builds a from-adjacency and edge-id
  index once instead of re-sorting every edge at each BFS node
  (`O(V·E·log E)` → `O(E·log E)`), with byte-identical visitation order
  preserved.

### Open-source readiness (2026-05-22)

- **MCP consolidation.** The MCP server is now a single `plsql-mcp`
  crate. The former source-available `plsql-mcp-pro` tier (FSL-1.1) is
  merged in, its commercial license gating removed, and its eight
  change-impact tools moved to `plsql-mcp::change_tools`. The whole
  workspace is uniformly dual-licensed `Apache-2.0 OR MIT`.
- **No panicking placeholder.** `plsql_parser::parse_file` is now a
  real generic convenience over `ParseBackend` with default
  `ParseOptions` (was `unimplemented!()`); covered by unit tests and a
  doctest. Dead `TransportError` scaffolding in `plsql-mcp` removed.
- **README** rewritten with an SVG hero banner, a how-it-compares
  table, a `License` section, and the consolidated-layer architecture.
- **Genericized private-estate handling.** The correctness harness is
  renamed `scripts/estate_correctness.sh`; the private estate is
  addressed only through the `PLSQL_PRIVATE_ESTATE` environment
  variable. `crates/plsql-accretion/gate.sha256` re-pinned accordingly.
- **Engine correctness.** `split_statements` and the ANTLR lowering
  path now depth-track `IF` / `LOOP` / `CASE` blocks, not just
  `BEGIN`/`END`; statements inside `IF`/`LOOP` bodies are no longer
  torn apart, so call and table Read/Write dependency edges from those
  bodies are extracted correctly. Also fixed: `classify_if` phantom
  arms on multi-`ELSIF` chains, UTF-8 corruption of non-ASCII string
  literals, and a dropped cursor-`FOR`-loop range sub-`SELECT` table
  read.
- **Security.** `build_deploy_plan` no longer wraps approved DDL in
  Oracle `q'[...]'` alternative quoting (DDL containing `]'` could
  close the literal early); it now uses collision-free standard
  `''`-escaping at both nesting levels.
- **`plsql-mcp serve --allow-public-bind`** is now a real flag; the
  public-bind refusal message it references is actionable.
- **Licensing reconciled.** `plan.md` (R16, §21, D8, D19) records the
  fully-open-source resolution: every crate is `Apache-2.0 OR MIT`,
  with no source-available or commercially-restricted tier.
- **Audit pass.** A `codebase-audit`, `security-audit`, and
  multi-pass bug hunt ran over the workspace; high-severity and
  honesty findings were fixed, remaining hardening items are tracked.

### Added

- **Layer 0 / foundations**
  - CI workflow `.github/workflows/ci.yml` with rustfmt + clippy
    `-D warnings` + workspace tests (incl. doctest) + `bench --no-run` +
    corpus-license + parse-success-surrogate jobs (`PLSQL-WS-015`).
  - Release workflow `.github/workflows/release.yml`: cross-platform
    binary matrix for linux x86_64 gnu+musl, linux aarch64 (cross), macOS
    x86_64 + aarch64, and windows x86_64-msvc; SHA256 manifest published
    via `softprops/action-gh-release@v2` (`PLSQL-WS-016`).
  - `tools/plan-lint/`: structural integrity checker for `plan.md` with
    seven rules: heading-monotonicity, ToC anchor validity, duplicate
    bead IDs, missing bead deps, stale §-refs, component coverage matrix
    (multi-segment family-prefix matching), banned release-wedge
    language scanner with double-quote + Status/Version-log whitelist.
    `--robot-json` + `--doctor` surfaces (`PLSQL-PLAN-001`).
  - `plan-lint` wired into ci.yml with the manual pre-bead-conversion
    gate documented in `AGENTS.md` (`PLSQL-PLAN-002`).
  - Per-component design docs under `docs/components/` for every
    workspace crate (`PLSQL-DOC-INDEX-002`).

- **Layer 1 / parser**
  - Lower module recognises ALTER / DROP / GRANT / REVOKE / COMMENT in
    addition to CREATE, emitting `AstDecl::Ddl` with a `verb target`
    kind label (`PLSQL-PARSE-008`).
  - Version-aware `UnsupportedDialectFeature` diagnostics with per-feature
    remediation hints + workaround copy (`PLSQL-DIALECT-003`); 7-test
    integration suite covers BOOLEAN / VECTOR / SPARSE VECTOR / vector
    arithmetic / RESETTABLE (`PLSQL-DIALECT-002`).
  - `BindingDiagnostic` codes enumerated for every unsupported construct
    in `plsql-bindgen` (`PLSQL-BG-011`).

- **Layer 1.5 / catalog**
  - Live-extraction loaders for `ALL_VIEWS`, `ALL_MVIEWS`, `ALL_SEQUENCES`,
    `ALL_TYPE_ATTRS`, and `ALL_TAB_PRIVS` (`PLSQL-CAT-004`).
  - Object status, edition, editionable flag, last DDL time, and
    `ALL_DEPENDENCIES` rows extracted into `CatalogSnapshot`
    (`PLSQL-CAT-014`).
  - `DBMS_METADATA.GET_DDL` + GET_XML extraction with normalization
    (`PLSQL-CAT-015`).
  - PL/Scope availability detection per schema + doctor reporting
    (`PLSQL-CAT-010` / `PLSQL-CAT-016`).
  - Runtime capability negotiation with grant-suggestion diagnostics
    (`PLSQL-CAT-017`).
  - `ALL_IDENTIFIERS` extraction into `PlScopeSnapshot`
    (`PLSQL-CAT-011`).

- **Layer 2 / depgraph**
  - `cross_check_with_catalog(snapshot, interner)` cross-checks depgraph
    edges against `ALL_DEPENDENCIES`, classifying mismatches as match /
    `OurExtra` / `OracleOnly` / `KindMismatch` / `ExpectedGap`
    (`PLSQL-DEP-014`).

- **Layer 5 / USR loop: Uncertainty-Sourced Repair (`PLSQL-USR-001`)**
  - The self-healing coverage flywheel: the engine's honest-uncertainty
    exhaust (parse errors, typed `UnknownReason`, un-lowered DDL) becomes
    a proven, privacy-clean, behaviour-preserving parser/lowering repair
    pipeline. `crates/plsql-accretion/` (Layer 5, no reverse deps) +
    `tools/usr-loop/` (`scan`/`cluster`/`propose`/`gate`/`land`/`doctor`,
    `--robot-json`). Stages [A]–[G]: GapRecord capture → privacy-proven
    MinFixture → cluster/dedup → candidate proposer → the 9-stage §3
    conformance gate (sha-pinned, fail-closed) → land + append-only
    content-addressed Ledger (`signature → commit` rollback anchor) /
    `[F']` provenanced quarantine on REJECT (gate never weakened).
  - **§4 accretion monotonic tripwire** (`scripts/accretion_tripwire.sh`,
    `usr-loop ledger tripwire`): makes "accretive" a *verified* property
    (I-MONOTONIC-VALUE). The dashboard quantity is

    ```
    coverage_index = extracted_semantics_ratio   (frozen public
                                                   corpus benchmark,
                                                   never private estate
                                                   code)
                   + distinct_resolved_gap_signatures
                                                  (signature classes the
                                                   loop has permanently
                                                   closed, from the
                                                   append-only Ledger)
    ```

    appended to a hash-chained `accretion_ledger.jsonl`; CI asserts
    `coverage_index(HEAD) ≥ coverage_index(last release tag)` and the
    tracked deterministic floor in
    `crates/plsql-accretion/accretion_floor.json`; a release that lowers
    `coverage_index` or `extracted_semantics_ratio` fails.
    **coverage_index over time** (the public, auditable compounding line):

    | git ref | coverage_index | extracted_semantics_ratio | distinct_resolved |
    |---------|----------------|---------------------------|-------------------|
    | `0.5.0-migration-floor` | `1.0` | `1.0` | `0` |

  - **§5 acceptance proof** (`scripts/usr_acceptance.sh`): the single
    re-runnable DoD; drives the loop to close a *real* private estate gap
    end-to-end and asserts every invariant (privacy/no-regression/
    no-gaming/determinism/provenance/isolation/monotonic-value). Honest
    SKIP (exit 0 + loud banner) when the private estate is absent.
  - CI `.github/workflows/usr.yml`: gate-selftest + accretion-tripwire
    required on every PR; full acceptance proof nightly.

- **Layer 4 / lineage**
  - `callers(target)` returns first-hop reverse Call-edges
    (`PLSQL-LIN-004`).
  - `column_readers(column)` + `column_writers(column)` route
    `ReadsColumn` / `WritesColumn` / `DerivesColumn` /
    `(Reads|Writes)UnknownColumnOfTable` edges, with
    `is_unknown_column_of_table` surface for parent-table fallbacks
    (`PLSQL-LIN-004`).
  - `unsafe_paths(from, to, max_depth, max_paths)` returns paths
    containing at least one `OpaqueDynamic` or `Unknown`-confidence
    edge, with per-path overall-confidence and truncation flag
    (`PLSQL-LIN-005`).
  - `impact_to_graphml(&LineageResult)` subgraph GraphML export with
    `anchor` / `affected` / `unknown-reason` node roles
    (`PLSQL-LIN-010`).
  - `impact_to_html(&LineageResult)` self-contained HTML report with
    embedded SVG impact subgraph + Markdown summary table
    (`PLSQL-LIN-008`).
  - `classify_rename(changes, hints)` pairs `Created`/`Dropped`
    records into rename candidates from explicit / persistent-id /
    Git-rename hints; never silently merges (`PLSQL-LIN-015`).

- **MCP surface**
  - `plsql-mcp` foundation crate skeleton (`PLSQL-MCP-001`).
  - Instant Client + `OracleConnection` backend doctor reporting
    (`PLSQL-MCP-LIVE-001`).
  - Connection-management tool surface (`PLSQL-MCP-LIVE-002`).
  - Audit baseline: module/action/comment marker + audit table
    (`PLSQL-MCP-LIVE-003`).
  - `query` tool with structured row output + K18 prompt-injection
    sanitization (scrubs MCP / tool-call / antml: / chat-role markers
    via runtime-built marker list; emits
    `UnknownReason::ResponseSanitized`) + LOB truncation
    (`PLSQL-MCP-LIVE-004`).
  - `list_objects` with type / name-pattern / schema filters + cursor
    paging via `OWNERNAME` tuple; `MAX_PAGE_SIZE = 500`,
    `DEFAULT_PAGE_SIZE = 100` (`PLSQL-MCP-LIVE-005`).
  - Four `describe_*` tools (`describe_table` / `describe_view` /
    `describe_trigger` / `describe_index`) with structured responses
    including columns, constraints, indexes, comments, and partition
    info (`PLSQL-MCP-LIVE-006`).
  - `get_object_source` / `get_clob` / `get_errors` tools with K18
    sanitization + structured `USER_ERRORS` / `ALL_ERRORS` rows
    (`PLSQL-MCP-LIVE-007`).
  - Session-level safety state with `enable_writes` token flow
    (`PLSQL-MCP-LIVE-008`).
  - TOML-backed connection profile loader + production-DSN doctor
    warning when `permanently_read_only` is missing
    (`PLSQL-MCP-LIVE-009`).
  - `compile_with_warnings` (`ALTER ... COMPILE` with
    `PLSQL_WARNINGS = ENABLE:ALL`) + categorized warnings (severe /
    performance / informational / other) (`PLSQL-MCP-LIVE-010`).
  - Per-connection write-posture rows in `plsql-mcp doctor`:
    `writes_enabled` / `active_read_only` / `permanently_read_only` /
    `inactive` (`PLSQL-MCP-LIVE-017`).
  - Per-platform live-DB integration walkthroughs at
    `docs/integrations/live-db/{linux,macos,windows}.md`
    (`PLSQL-MCP-LIVE-020`).

- **CI/CD surface**
  - `plsql-cicd` crate skeleton with `ChangeSet`,
    `InvalidationPrediction`, and `DeploymentPlan` foundational types
    (`PLSQL-CICD-001`).
  - `CicdOracleInspector`: read-only Oracle wrapper around the
    `plsql-catalog` `OracleConnection`; refuses DDL/DML/PLSQL with
    `is_read_only_sql` predicate (`PLSQL-CICD-004`).

- **Bindings generator**
  - `BindingDiagnosticCode` enum with 14 stable `BG_UNSUPPORTED_*`
    codes covering REF CURSOR, pipelined, BOOLEAN, associative arrays,
    records, nested-tables-in-parameters, VARRAYs, non-literal
    defaults, LONG, autonomous_transaction, invoker_rights without
    hint, opaque types, overload ambiguity, wrapped package bodies
    (`PLSQL-BG-011`).
  - Oracle → Rust type mapping per plan §12.3 verbatim (`PLSQL-BG-002`).
  - Oracle date/time wrapper types (`OracleDateTime`,
    `OracleTimestamp`, `OracleTimestampTz`, `OracleTimestampLtz`,
    `IntervalYM`) with `DateTimeBackend` enum
    (`Chrono` / `Time` / `Strings`) (`PLSQL-BG-016`).

- **Corpus**
  - Oracle HR/OE/SH sample-schema DDL subset ingested into
    `corpus/public/` with manifest entries (`PLSQL-WS-012`).
  - antlr/grammars-v4 PL/SQL example subset ingested into
    `corpus/public/` (`PLSQL-WS-013`).

### Added (continued)

- **Plan-doc normalization**: `plan.md` H3 numbers across §11/§12/§15/§16 normalised; H2 monotonicity drift §17-§26 renumbered to §19-§28 to match ToC anchors; §4 + §28 heading slugs aligned with ToC. `plan-lint --doctor` 13 errors → 0; the `ci.yml` plan-lint job is now a real blocking gate (`PLSQL-PLAN-003`).
- **CLI agent ergonomics**: `plsql-depgraph` returns distinct exit codes (`1` query-failed / `2` invocation-error); `corpus-license-check` shipped `--robot-json` + `--doctor`; `plan-lint --help` self-documents flags, schema id/version, examples, and exit codes (`PLSQL-CLI-ERG-001/002/003`).
- **Lineage API polish**: `ColumnAccessResult::resolution_error: Option<String>` distinguishes "column not found" from "column found, no accessors" (`PLSQL-LIN-024`); rich envelope-wrapper docstrings cite each schema descriptor inline (`PLSQL-LIN-022`); `unsafe_paths` backtrack pop extracted into a single `pop_path_step` helper so the two backtrack sites can't drift (`PLSQL-LIN-020`).

### Post-compaction (GrayDesert resume, 2026-05-15)

After context compaction the session resumed under a fresh `GrayDesert`
identity. Five closures + seven new beads filed in this segment:

- **`PLSQL-CICD-009`** (`oracle-tjp1`, commit `e8a13fc`): added
  `crates/plsql-cicd/src/doctor.rs` with `doctor_report` aggregating
  `ChangeSet` health into `ChangesetDoctorReport` (overall risk +
  remediation hints + per-`UnknownReason` counts).
- **`PLSQL-PARSE-012`** (`oracle-wua`, commit `e2aeb00`): parse-corpus
  test harness in `crates/plsql-parser-antlr/tests/corpus_harness.rs`
  with `CorpusReport` aggregation across `corpus/public/`. Soft floors:
  `>=18 fixtures`, `>=60% success rate`, `>=1 of each {ddl,view,trigger}`.
- **`PLSQL-CAT-NEW-1`** (`oracle-rr4y`, commit `aaa4649`):
  `ALL_DB_LINKS` loader with `DatabaseLink` struct and
  `SchemaCatalog::db_links` (behind `#[serde(default)]`). Surfaced by
  /oracle skill audit, immediately implemented.
- **`PLSQL-DEPS-003`** (`oracle-ngf4`, commit `3576240`): moved `sha2`
  from three per-crate pins (catalog/store/mcp) into
  `[workspace.dependencies]` so future bumps are one-line.
- **`PLSQL-DOC-README-1`** (`oracle-6vn7`, commit `8087509`): README.md
  pass: Architecture Sketch table with Layer 0..5 crate map, Getting
  Started block with cargo commands + live-db note, Layout section
  drops local agent dirs.

Findings filed but not closed (queued for future passes):

- **/oracle catalog audit**: `oracle-c0gg` (`ALL_POLICIES` / VPD),
  `oracle-fmro` (`ALL_EDITIONS` + `ALL_EDITIONING_VIEWS` / EBR),
  `oracle-jylb` (`ALL_CONS_COLUMNS` / FK column lineage),
  `oracle-grs0` (`ALL_TAB_COMMENTS` + `ALL_COL_COMMENTS`).
- **/library-updater**: `oracle-m0q3` (sha2 0.10 → 0.11 audit),
  `oracle-dd84` (toml 0.8 → 1.1 audit).
- **/mock-code-finder**: `oracle-goz8` records the `plsql-engine`
  skeleton anchor; finds **no silent stubs** elsewhere: every
  placeholder (parse_file `unimplemented!`, mcp skeleton modules,
  catalog DDL-parsing skeleton) is typed + documented + tested.

### /oracle skill: applied across seven surfaces this session

Substantive /oracle skill applications (router at `~/.claude/skills/oracle/`, cited evidence per audit):

1. **Catalog extraction** (`crates/plsql-catalog/src/lib.rs` ↔ `LOW-LEVEL-CATALOGS.md`): 5 findings → 5 beads filed (3 closed as scope-bounded, 2 closed-as-investigated).
2. **L2 synthetic corpus** (`corpus/synthetic/l2/*.{pks,pkb,sql}` ↔ `DATABASE-REFERENCE.md` + plsql-security docs): all 5 files confirmed CANONICAL.
3. **Depgraph EdgeKind taxonomy** (`crates/plsql-depgraph/src/lib.rs` ↔ ALL_DEPENDENCIES semantics): 3 findings → 3 beads filed (all closed as out-of-current-scope).
4. **Parser version coverage** (`crates/plsql-parser/src/{lib.rs,dialect.rs}` ↔ `SUPPORT-RELEASE-MATRIX.md`): Oracle26ai missing from `OracleTargetVersion`; inline-fixed (commit `af88a26`).
5. **MCP surface** (`crates/plsql-mcp/src/doctor.rs` doctor fields ↔ Trust Block conventions): verified `oracle-bt42 MCP-010` doctor already shipped (still gated by `oracle-ic04 MCP-007`).
6. **Bindings type map** (`crates/plsql-bindgen/src/type_mapping.rs` ↔ `OBJECT-TYPES-REFERENCE.md`): NUMBER → i64/Decimal CANONICAL, INTERVAL precision MINOR DEVIATION (chrono::Duration ms only), VECTOR/SPARSE_VECTOR gap noted.
7. **SAST rule pack** (`plan.md` §12 ↔ `SECURITY-OPTIONS-REFERENCE.md` + `sql-injection-avoidance.md`): DBMS_ASSERT framing refinement, TDE/Wallet credential gap, AUTHID CURRENT_USER not routed in security reference, Label Security/VPD violation rule missing; all queued for when the SAST rule pack is implemented.

`AGENTS.md` now locks `/oracle` as a **recurring** workflow gate: every future catalog / parser-dialect / depgraph-edge / SAST / bindings closure must cite both source line and reference section. This makes `/oracle` exhaustively applied by construction going forward, not just historically.

### Fixed

- **Honest `CompletenessReport` / Trust Block: no false-clean on
  low-extraction runs** (`oracle-bh4p`, D2 Phase 2, plan §1.5/§22).
  On real code (a private Oracle PL/SQL estate) the report claimed
  `files_parsed_cleanly: 4224`, all gap counts `0` (a pristine
  picture) while the engine emitted **6,784 diagnostics** of which
  **6,609 were "AST classifier returned Unknown"** (objects never
  lowered). The report now carries honest signals: `posture`
  (`Clean`/`Partial`/`LowConfidence`/`Degraded`, derived, never
  `Clean` when extraction is low), `objects_unrecognized`,
  `diagnostics_total`, `objects_with_extracted_semantics`,
  `extracted_semantics_ratio`. Structurally not-yet-wired gap
  metrics (`dynamic_sql_sites`, `unresolved_references`,
  `db_link_edges`, `opaque_dynamic_sql_sites`, `wrapped_units`,
  `missing_package_bodies`) now serialise as
  `{ "unmeasured": true }` (`Measured::Unmeasured`) instead of a
  misleading `0`. The private estate now reports `posture: LowConfidence`,
  `objects_unrecognized: 6609`, `diagnostics_total: 6784`,
  `extracted_semantics_ratio: 0.384`. Additive schema bump
  `plsql.engine.analysis_run` 1.0.0 → 1.1.0.
- `plsql-privileges/src/resolve.rs`: dropped 4 unused imports
  (`AccessibleByTarget`, `Grant`, `RoutineSignature`, `SynonymTarget`)
  + 2 unused test helpers (`make_role_name`, `make_user_name`);
  `cargo clippy --workspace --all-targets -- -D warnings` was failing
  on these prior to the cleanup (`PLSQL-LINT-002`, GentleTrout audit).
- `plsql-catalog` `upsert_packaged_routine` rewrote from `contains_key`
  + `insert` to `entry().or_insert_with(...)` for clippy::map_entry
  compliance (`PLSQL-LINT-001`, folded into `PLSQL-CAT-004`).
- `classify_git_diff` rename detection matched bare `R` / `C` strings
  but Git emits them with similarity scores (`R100`, `C100`), so renames
  silently fell through to a `Body` change record. Matches now key on
  the leading byte (`b'R' | b'C'`) (`PLSQL-LIN-019`, surfaced by the
  feature-dev:code-reviewer audit).
- `release.yml` was building every workspace binary via `--bins`, which
  pulled internal dev tools into the cross-compile path and could block
  a release on an unrelated platform failure. Now loops explicitly over
  `RELEASE_BINS` so only public binaries participate (`PLSQL-WS-019`).
- `plan-lint::collect_toc_entries` carried a dead-code branch and could
  in principle re-enter the ToC section if a later H2 contained the same
  words. Rewrote with a `past_toc` latch (`PLSQL-PLAN-004`).
- Pre-existing fmt drift in `plsql-bindgen::executor.rs` +
  `plsql-depgraph::main.rs` + `plsql-parser-antlr::recover.rs` cleaned
  up to keep the new CI gate green.

### Live-DB integration + false-open closeout (2026-05-17)

- **Autonomous Oracle env**: `examples/oracle-xe/docker-compose.gvenzl.yml`
  + `make demo-oracle-xe-ci`: boots `gvenzl/oracle-free:23-slim` (no
  Oracle SSO / FUTC wall) so CI and agents run the live-DB suites
  unattended. Fixed a latent portability defect: the base XE targets
  hard-coded the `docker compose` v2 plugin; new `DOCKER_COMPOSE`
  autodetect falls back to standalone `docker-compose`.
- **SAST false-open closeout**: `oracle-n528` (FactKind/FactPayload
  ExceptionHandler/CursorForLoop/MissingInstrumentation schema) and
  `oracle-3qjm` (`scan_exception_handlers` + `emit_exception_handler_facts`
  producer) were implemented but left `IN_PROGRESS`; verified via
  re-run (239 plsql-ir + 94 plsql-sast tests, QUAL001/QUAL004/PERF001/
  PERF002/STYLE001 fire on positive fixtures and skip on negative) and
  closed with evidence.
- **Live-DB integration beads (vs real Oracle Free 23ai)**:
  `PLSQL-CAT-008` live catalog-snapshot golden (`oracle-mi0`);
  `PLSQL-CICD-005` `verify <changeset>` against a `VERIFY_T_*` scratch
  schema with RAII teardown and an explicit no-rollback contract
  (`oracle-m941`); `PLSQL-CICD-005A` interactive in-place safety gate
  requiring verbatim schema-name retype (`oracle-q2o8`); `PLSQL-CICD-010`
  predict→plan→verify cycle (`oracle-fnsh`); `PLSQL-MCP-LIVE-018` every
  live-DB MCP tool E2E + chained preview→execute_approved + 7-case
  refusal matrix (`oracle-7nmg`); `PLSQL-MCP-LIVE-019` hero-demo golden
  transcript (`oracle-6hlb`). 25 feature-gated `live-xe` integration
  tests, all green against the container.
- **`PLSQL-LAB-008`** (`oracle-yd96`): authored the real §1.4
  `DROP COLUMN customers.legacy_segment` hero corpus (`corpus/lab/l1/`
  + `corpus/lab/hero_diff_dropcol/`, closing the missing-L1-corpus gap)
  with a live test proving Oracle marks the view + package body +
  procedure `INVALID`; the prior param-rename `hero_diff/` fixture is
  preserved unchanged. `plan.md` §1.4 reconciled (DROP COLUMN thesis
  kept). Discovered-from `oracle-6hlb`.

### Fixed (2026-05-17)

- `plsql-mcp/src/describe.rs`: `load_partition_info` queried a
  non-existent `ALL_PART_TABLES.PARTITIONED` column (`ORA-00904`);
  stub-based tests had masked it. Now reads `PARTITIONED` from
  `ALL_TABLES`, then `ALL_PART_TABLES.PARTITION_COUNT` only when
  partitioned (surfaced by the `oracle-7nmg` real-DB E2E).
- Workspace `cargo fmt --all` normalization (101 files, behavior-
  preserving: full `--all-targets` build + test suite green); resolved
  abandoned uncommitted swarm WIP.

### Notes

- 2026-05-15: commit `97c3651` ("feat(lineage): add callers + column_readers/writers")
  swept up oracle__cc_1's WIP for `oracle-764` (PLSQL-CAT-007 doctor)
  due to a stage/commit race in the shared working tree. Code +
  bead-state are both correct in `master`; only the commit-message
  attribution drifted.

[Unreleased]: https://github.com/USER/oracle/compare/52b4f59...HEAD
