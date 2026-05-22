# Session pickup — where to resume

> Most-recent snapshot: end of the **2026-05-15** swarm session
> (FuchsiaRobin + GentleTrout / oracle__cc_1, coordinated via Agent
> Mail). Older snapshot from 2026-05-13 below for history.

## 2026-05-15 snapshot

| Metric | Value |
|--------|-------|
| Open beads | ~216 (started session at 246) |
| Closed this session | ~30 (15+ by FuchsiaRobin, ~15 by oracle__cc_1) |
| Commits landed | 38 between `52b4f59` and `HEAD` |
| Workspace status | `cargo test -p <crate>` green across lineage / depgraph / parser-antlr / plan-lint; clippy `--no-deps -- -D warnings` green |
| Disk | `target/`=2.3G, `/tmp/cargo-target`=89G (after a mid-session 28G incremental wipe; user-approved), 35G free on tmpfs |
| Remote | Still no remote configured |

### Highlights this session

- **Layer 0** — `tools/plan-lint/` shipped (`PLSQL-PLAN-001`),
  wired into CI (`PLAN-002`), heading/anchor drift filed as
  `PLAN-003` follow-up. `ci.yml` and `release.yml` are now both
  wired (`WS-015`, `WS-016`).
- **Layer 1.5** — catalog gained live-extraction loaders for views /
  mviews / sequences / type-attrs / grants (`CAT-004`), object
  status + ALL_DEPENDENCIES (`CAT-014`), DBMS_METADATA.GET_DDL
  (`CAT-015`), PL/Scope availability + capability negotiation
  (`CAT-010/016/017`), `ALL_IDENTIFIERS` (`CAT-011`).
- **Layer 2** — `DepGraph::cross_check_with_catalog` (`DEP-014`).
- **Layer 4** — lineage gained `callers`, `column_readers/writers`
  (`LIN-004`), `unsafe_paths` (`LIN-005`), `impact_to_graphml`
  (`LIN-010`), `impact_to_html` with embedded SVG (`LIN-008`),
  `classify_rename` with explicit/persistent-id/git-rename hints
  (`LIN-015`).
- **Parser** — pre-parser now lowers ALTER/DROP/GRANT/REVOKE/COMMENT
  (`PARSE-008`); `UnsupportedDialectFeature` diagnostics shipped
  (`DIALECT-002/003`).
- **MCP** — `plsql-mcp` foundation crate (`MCP-001`), Instant
  Client + doctor (`MCP-LIVE-001`), connection-mgmt tools
  (`MCP-LIVE-002`), audit baseline (`MCP-LIVE-003`), `enable_writes`
  token flow (`MCP-LIVE-008`).
- **Docs** — per-component design docs for every crate
  (`DOC-INDEX-002`); CHANGELOG.md bootstrapped.
- **Hardening** — feature-dev:code-reviewer audit produced four
  findings; one was a false positive (closed as `LIN-018` with a
  regression test), three are now fixed (`LIN-019`, `WS-019`,
  `PLAN-004`).

### Agent Mail coordination

- Identities: `FuchsiaRobin` (oracle:1:3 / cc_2) +
  `GentleTrout` (NTM coordinator, relays to cc_1).
- Thread: `prompt-md-swarm-coordination`.
- File-reservation pattern worked well; one race documented
  in `CHANGELOG.md` under Notes (commit 97c3651 attribution drift).
- Build coordination: per-crate `cargo check -p X` keeps the
  workspace cache warm without thrashing both agents.

### What's next (live triage)

Run `bv --robot-triage 2>&1 | head -25` for the current priorities.
At snapshot, top candidates were:

- `oracle-bfr` (`PLAN-003`) — normalise plan.md heading numbers +
  ToC anchors so plan-lint CI can flip to `continue-on-error: false`
- `oracle-843o` (`LIN-007`) — `what-breaks --change <file>` parser
- `oracle-4k4o` (`CICD-002`) — `predict <changeset>` consuming the
  new ChangeSet types
- `oracle-mi9` (`PARSE-005`) — statement-body lowering
- `oracle-leq6` is closed; remaining doc beads ladder into per-domain
  guides

---

## 2026-05-13 snapshot (historical)

| Metric | Value |
|--------|-------|
| Open beads | ~245 (started session at 285) |
| Beads closed this session | ~42 (~14% of total) |
| Commits landed | 22+ across parser/catalog/IR/symbols/lineage/doc/bindgen/privileges/corpus/decisions |
| Workspace status | `cargo check` clean, all per-crate `cargo test` pass |
| Disk | `target/` 2.3G, 3.2T free |
| Remote | No remote configured on `master` (local-only history) |

## What's done

Foundation + Layer 1 (parser):

- `plsql-parser` crate with `ParseBackend` trait + conformance suite, `Spanned` trait, `Visitor`/`Walker`, `SourceMap`, AST/CST/TokenTape types
- `plsql-parser-antlr` backend crate, vendored BSD-3 grammars-v4 PL/SQL `.g4` files, `build.rs` invoking antlr-rust codegen (gated by `antlr-codegen` feature), `lower.rs` for top-level declarations, error recovery at statement boundaries, round-trip token-tape proptest
- Parser-002 (`build.rs`) lands; spike notes captured in `docs/decisions/D1-parser-backend-spike.md`

Layer 1.5 (catalog / project):

- `plsql-catalog` synthetic test catalog builder (CAT-006), `DBMS_METADATA`-file ingestion path (CAT-005), catalog-crate docs at `docs/components/catalog.md` (CAT-009)

Layer 2 (semantics):

- `plsql-ir` crate with `SemanticModel` + `Declaration` enum/variants (IR-001, IR-002)
- `plsql-symbols` with `DeclTable` scaffold (SYM-001)
- `plsql-privileges` (PRIV-001)

Layer 3+ (product surfaces):

- `plsql-doc`: `DocSet` / `ObjectDoc` / `DocComment` types (DOC-001), doc-comment lexer (DOC-002), tag parser for `@description`/`@param`/etc. (DOC-003)
- `plsql-bindgen`: `BindingPlan` IR (BG-000), sync-first `OracleExecutor` trait + opt-in async wrapper (BG-001)
- `plsql-lineage`: `LineageQuery` / `LineageResult` / `Confidence` (LIN-001), `SemanticChangeSet` (LIN-000), `impact()` traversal with confidence aggregation (LIN-002), `dependencies()` reverse traversal (LIN-003), Git-diff / dir-diff / catalog-snapshot-diff classifier (LIN-007A), `OrphanCandidate` + `OrphanConfidenceTier` types in `plsql-output` (LIN-018), `--robot-json` envelope wrappers (LIN-009), graph-completeness `doctor()` report (LIN-011), customer-facing `explain` for edges/nodes/paths (LIN-014), `recompile_order` topological sort (LIN-006)
- `plsql-depgraph`: `explain` subcommand (DEP-015)

Workspace + corpus + docs:

- Corpus L1 synthetic PL/SQL seed (LAB-001) at `corpus/synthetic/l1/`
- `corpus-license-check/` CI gate (WS-014)
- `docs/architecture.md` (DOC-INDEX-001) — 313-line top-level reference
- `docs/components/parser.md` (PARSE-017) — AST schema reference
- `docs/components/catalog.md` (CAT-009)
- `docs/decisions/README.md` (DECISION-LOG-001) — index + protocol

## What's next

Run `bv --robot-triage 2>&1 | head -25` for the live priority view. Highest-leverage open clusters at this snapshot:

1. Parser core — finish `PARSE-008` (DDL lower.rs for CREATE/ALTER/DROP/GRANT), then `PARSE-005`/`PARSE-006`/`PARSE-007` (statements/expressions/types lowering). A previous attempt at PARSE-008 by a stalled hermes agent generated ~614 lines of `lower/mod.rs` that didn't pass tests; the stash for that attempt was dropped during wrap-up, so PARSE-008 starts fresh.
2. Semantics — `SYM-002` (resolution strategies 1–3), `SYM-009` (overload resolution), `SYM-010` (catalog facts feeding symbols)
3. Dependency graph — `DEP-003` (Reads/Writes edge extraction), `DEP-014` (catalog cross-check report)
4. Lineage — `LIN-021` (HTML/Markdown/JSON report with Trust Block), `LIN-023` (doctor: orphan freshness)
5. SAST — `SAST-019` / `SAST-020` (PERF rules with tests)
6. Bindings — `BG-004` (function/procedure wrapper emission)

## Operational notes for the next orchestrator

- The 4-minute tending cron + hourly cleanup cron were torn down at session end. Re-establish if running unattended.
- Build serialization: workspace-wide cargo commands should be wrapped in `flock -w 300 /tmp/oracle-cargo.lock` to avoid contention when multiple agents build the same crates concurrently. Per-crate `cargo check -p <crate>` does not need the flock.
- This session ran with one CC orchestrator (me) and rotated through 2 hermes (mimo), 1 opencode/gpt-5.2-codex, 1 opencode/zai-glm-5.1, and finally a CC implementer. CC was by far the most productive; hermes/mimo hit shared HTTP-429 quota lockouts and opencode CLIs froze in "Build mode" without actually executing tool calls. Recommended baseline: 1 CC orchestrator + 1 CC implementer, scale up only after verifying provider headroom.
- `.claude/` and `.ntm/` agent state directories are now in `.gitignore`.
- Beads: closed-via-orchestrator commits during this session include DOC-001, BG-000, BG-001, LIN-001, DECISION-LOG-001, DOC-INDEX-001 (finalizer commit on hermes_2's draft), and the duplicate-close of oracle-11x (folded into oracle-716r).

## Open questions / known issues

- `docs/decisions/D1-backend-tournament-result.md` is still a placeholder — `PLSQL-PARSE-000C` tournament work hasn't been scheduled.
- No git remote configured. If/when the founder wants to publish, `git remote add origin <url>` + `git push -u origin master`.
- `/tmp/cargo-target` grew to ~17 GiB during the session (CARGO_TARGET_DIR override that some agents used). Safe to delete between sessions if disk pressure shows up; not monitored by the existing hourly cleanup cron.
