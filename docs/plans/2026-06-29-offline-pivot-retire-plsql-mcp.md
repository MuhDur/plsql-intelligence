# plsql-intelligence — Offline Pivot & `plsql-mcp` Retirement Plan

**Status:** DRAFT **v5** — THREE independent fresh-eyes review rounds applied (`planning-workflow`).
v4 fixed the v3 DAG cycle, version-bump breakage, `live`-feature naming, and added every missing artifact
task; round 3 confirmed all 21 prior items fixed + DAG acyclic and caught one blocking contradiction
(Phase-1P "non-blocking" vs the ship edges), now resolved in v5 by splitting P1P.1 (non-blocking) from
P1P.2 (gates the ship for the no-Java guarantee). Plus: §3 crate count (20, incl. cicd), the
`oracle-ublu`-is-closed correction (P0.3), and B2/B4 polish. **READY FOR BEAD CONVERSION.**

**Owner:** founder · **Date:** 2026-06-29 (post-v0.6.0) · **Supersedes:** the live+offline *superset*
topology shipped in v0.5.0/v0.6.0. **Baseline:** committed `main` (the `spike/offline-brain-stable` branch
holds a partial, uncommitted P1.1 as reference only; real work starts on a fresh branch off `main`).

---

## 1. Directive & context

**Founder decision (2026-06-29):** go **fully offline**, **no double tracking**.

- `plsql-intelligence` becomes a **pure offline PL/SQL intelligence engine** — **library crates** + **CLIs**
  (`plsql`, `plsql-depgraph`, `usr-loop`) — on **stable Rust**, `#![forbid(unsafe_code)]`, read-only, no DB
  socket, no telemetry.
- **`plsql-mcp` stops being an MCP server and is removed from this repo.** The MCP surface moves to
  **`oraclemcp`** (separate repo), which optionally consumes our engine **as library crates behind a
  feature gate** (D12). Dependency arrow flips to the correct direction: *oraclemcp (hands) → engine
  (brain)*, optional.
- The published **`plsql-mcp` crate is deprecated** on crates.io and its registry/image retired — **only
  after** oraclemcp ships the replacement (no user-facing capability gap; the old `plsql-mcp` 0.6.0 stays
  installable until then because we never hard-yank it).

**Why** (moat + audience): air-gapped/regulated Oracle estates need a **credential-free, read-only,
no-network** artifact that passes a security review, and `cargo install` must need **neither nightly nor
Java**. The superset blocked all three. The live combination re-forms in oraclemcp, with the engine as a
clean stable dependency.

---

## 2. Grounded technical facts (verified 2026-06-29 against committed `main`; parser-runtime status updated 2026-06-30)

1. **The repo's own code uses ZERO nightly language features** (`grep -r '#![feature('` across
   `crates/*/src` + `tools/*/src` = none).
2. **`asupersync` is the ONLY nightly dep** (fails on stable: `#![feature(try_trait_v2)]`). Full external
   dep set across engine crates: `antlr4rust, async-trait, chrono, clap, fs2, miette, oraclemcp-db,
   rusqlite, serde, serde_json, sha2, thiserror, toml, tracing` — only `asupersync` (+ `oraclemcp-db`,
   which pulls it) is nightly. The real ANTLR parser compiles on **stable 1.96** (`cargo +stable check -p
   plsql-parser-antlr --features antlr-codegen` green; `plsql-engine` enables `antlr-codegen`
   unconditionally; `ParserBackendChoice` defaults to `Antlr4Rust`).
3. **The ANTLR Rust runtime is on the maintained line:** `oracle-lldm` moved the parser backend to
   `antlr4rust 0.5.2` and the paired ANTLR 4.13.2 Rust tool on 2026-06-30.
4. **Build-time Java 11+ for ANTLR codegen is regeneration-only:** `build.rs` runs the vendored jar
   (`tools/antlr4-4.13.3-SNAPSHOT-complete.jar`, 2.1 MB) only when `PLSQL_ANTLR_REGEN=1` is set.
   **No Java is required for normal builds or runtime.** The committed generated code (D13/Q-JAVA=A,
   P1P.2) remains the normal build path; CI drift-check regenerates to a temp directory and diffs.
5. **The live path is already feature-aware (corrects the v3 "invent a `live` feature" mistake).**
   `plsql-catalog` HAS features `oraclemcp-db = ["dep:oraclemcp-db"]` and `live-xe = ["oraclemcp-db"]`
   (`Cargo.toml:11-13`); `oraclemcp-db` is `optional = true`. The concrete `OraclemcpDbConnection` adapter
   is already `#[cfg(feature = "oraclemcp-db")]`. **But on `main`, `asupersync` is UNCONDITIONAL**
   (`Cargo.toml:17`), and the `OracleConnection` trait + the ~26 inline live loaders (`load_catalog_*`,
   `load_snapshot_from_connection`, `negotiate_capabilities`, `fetch/populate_dbms_metadata_ddl`;
   `lib.rs` ~954–3160) reference `Cx`/`OracleConnection` **ungated** — that's the ~52 stable errors. The
   work (P1.1) is to finish gating those under the **existing `oraclemcp-db` feature** + make `asupersync`
   optional pulled by it. **Do NOT invent a new feature.**
6. `plsql-cicd` HAS `live-xe = ["plsql-catalog/live-xe"]` (`Cargo.toml:15`) and **unconditional**
   `asupersync` (`Cargo.toml:19`); `inspector.rs:14` + `verify.rs:65` do module-level `use asupersync::Cx;`
   **outside any cfg**. The pure `is_read_only_sql`/`preview_sql` are offline.
7. **The offline ingestion is stable + interleaved with the live loaders:** `apply_*_row(snapshot, row)`
   (private), `hash_text`, `normalize_dbms_metadata_ddl`, `object_type_to_dbms_metadata_value`,
   `schema_filter_params`, `oracle_bind_placeholders`, and the row types (`OracleRow` `lib.rs:858` pub,
   `OracleBind`, `OracleConnectionInfo`) carry no `Cx`.
8. **The 10 downstream offline crates have 0 live-symbol refs** (`ir, symbols, privileges, depgraph,
   engine, lineage, sast, doc, bindgen, accretion`).
9. **Orphan analysis:** removing `plsql-mcp` orphans exactly **`plsql-doc`** + **`plsql-sast`** (consumed
   only by `plsql-mcp`, no own bin). `plsql-lineage` survives via cicd; `plsql-support` via accretion;
   `plsql-bindgen`/`plsql-depgraph`/`plsql-engine`/`plsql-store`/`plsql-accretion` via their own bins.
   crates.io retirement scope = `plsql-mcp` alone.
10. **The `plsql` CLI is in `plsql-cicd`** (`bin/plsql.rs`: `Predict`/`Doctor`/`RobotDocs`); cicd's normal
    deps are `plsql-catalog` + `plsql-lineage` (`plsql-depgraph` is a **dev-dependency**), not
    `plsql-doc`/`plsql-sast` yet.
11. **Workspace = 21 crates + tools.** Published library set (§3) must include **`plsql-support`** (else
    `plsql-accretion`'s publish fails). Affected artifacts a fresh agent will touch: `Dockerfile`,
    `server.json`, `.github/workflows/{ci,release,usr,docker,publish-mcp,plsql-gate,
    plsql-change-impact-selftest,bindgen-roundtrip}.yml`, `.github/actions/plsql-change-impact/`,
    `.github/assets/{hero.svg,og.svg,og.png}`, **18** `docs/**` files mentioning `plsql-mcp`, and
    `scripts/{oraclemcp_pin_guard,plsql_mcp_boundary_lint,plsql_mcp_honesty_grep}.sh`.

---

## 3. End-state architecture (target)

```
plsql-intelligence (THIS repo) — STABLE Rust, offline, read-only, no MCP, no nightly, no build-time Java
  ├── 20 published crates (the product — 18 libs + cicd[lib+`plsql` CLI] + support):
  │     core, output, render, store, parser, parser-antlr, project, catalog, ir, symbols, privileges,
  │     depgraph, engine, lineage, doc, bindgen, sast, CICD, accretion, SUPPORT
  ├── CLIs / release binaries: plsql (predict|doc|sast|doctor), plsql-depgraph, usr-loop
  ├── dev/internal bins: plsql-engine, plsql-bindgen, plsqld (store), usr-gate-rs (accretion), plan-lint, corpus-*
  ├── parser: ANTLR grammar via antlr4rust 0.5.x; generated code COMMITTED (no build-time Java)
  └── ZERO nightly/live code (asupersync/oraclemcp-db gone).

oraclemcp (SEPARATE repo) — NIGHTLY, the only MCP server
  └── optional `plsql-intelligence` feature → depends on the engine LIBRARY crates (D12) → exposes
        offline intelligence tools AND couples them with live access (live catalog extraction → engine →
        blast-radius/lineage/SAST on the CURRENT schema; blast-radius-on-write).

Seam = the serialized CatalogSnapshot: THIS repo exposes a stable public `CatalogSnapshotBuilder`
  (OracleRow → snapshot); oraclemcp owns the live querying (the SQL + oraclemcp-db connection), feeds
  rows into the builder, hands the snapshot to the engine.

RETIRED (after oraclemcp ships the replacement): the plsql-mcp crate + Dockerfile + server.json, its GHCR
  image, the io.github.MuhDur/plsql-mcp registry entry, and docker.yml / publish-mcp.yml.
```

Offline-MCP use = an intelligence-enabled `oraclemcp` build run without a connection (oraclemcp
deliverable, §9); CLIs cover non-MCP offline use.

---

## 4. Committed decisions

- **D1 — Topology:** offline engine library + CLIs here; MCP only via oraclemcp's optional feature;
  `plsql-mcp` retired. *Why:* decouples the offline moat from the nightly live stack; correct dep arrow.
- **D2 — Live extraction relocates to oraclemcp; this repo ends with ZERO live/nightly code.** Seam = the
  serialized `CatalogSnapshot` (stable `CatalogSnapshotBuilder` here; live querying in oraclemcp). The
  catalog live loaders + the `oraclemcp-db`/`live-xe` features are *gated* in P1.1 (transition), then
  *removed* in P2.3 once oraclemcp owns extraction.
- **D3 — Toolchain → stable + MSRV** (empirical, ≥ what antlr4rust/edition-2024 need; 1.96 known-good).
  During the transition, a nightly CI job (invoked via explicit `cargo +nightly-…`/`RUSTUP_TOOLCHAIN`,
  because the stable `rust-toolchain.toml` otherwise wins) builds the gated `oraclemcp-db` feature +
  `antlr-codegen`. `plsql-mcp` is **excluded from `default-members`** so stable `cargo build` skips it
  until P2.1 deletes it.
- **D4 — `plsql-mcp` removed from the workspace early (P2.1); on crates.io NOT hard-yanked** (0.6.0 stays
  installable as the user's MCP path until oraclemcp ships); optional tombstone version at P3.2.
- **D5 — Distribution retirement (Phase 3) gated on oraclemcp shipping the replacement (P5.5)** so there
  is never a user-facing gap — this is the correct home for the "no gap" guarantee (not P2.1).
- **D6 — Versioning: ONE clean ship.** Publish the offline engine + CLIs as **0.7.0 after** `plsql-mcp` is
  removed and the live code relocated (clean, zero version-skew). An **optional early `0.7.0-alpha`** (PS.0,
  after P2.1) gives immediate crates.io install for those who want it. **1.0** once the API settles.
- **D7 — USR loop / accretion / coverage_index preserved**; tripwire + gate sha-pin + `usr_acceptance.sh`
  stay green across the pivot.
- **D8 — AGENTS.md async rule → sync-first/stable default**; the asupersync/`&Cx` exception lives only in
  oraclemcp now.
- **D9 — `oracle-tdgx` / oraclemcp#4 (TNS hang) ownership → oraclemcp.**
- **D10 — No compat shims; `plsql-mcp` removed cleanly.**
- **D11 — `plsql-doc` + `plsql-sast` KEPT, get `plsql doc`/`plsql sast` CLI subcommands** (P2.2;
  `plsql-cicd` gains them as normal deps); they also remain library crates oraclemcp consumes. Only
  `plsql-mcp` is retired.
- **D12 — Integration = library import** (oraclemcp adds the engine crates as optional Cargo deps behind a
  `plsql-intelligence` feature; in-process, no IPC). Unlocks live-grounded intelligence +
  blast-radius-on-write. Subprocess-the-CLI is a documented fallback.
- **D13 / Q-JAVA = A: commit the generated parser code** (P1P.2). Build uses committed Rust; Java only on
  grammar change; CI drift-check asserts `regenerate == committed`. *Why:* frictionless install (no nightly
  AND no Java). **P1P.2 GATES the 0.7.0 ship** (the no-Java guarantee is a committed promise) and is
  independent of P1P.1 — it commits whatever the current runtime generates. (crates.io 10 MiB limit OK:
  gzipped tarball; generated Rust compresses heavily.)
- **D14 — Migrate `antlr-rust 0.3.0-beta` → `antlr4rust 0.5.x`** (P1P.1), **completed 2026-06-30**:
  `oracle-lldm` moved the committed parser output and backend runtime to `antlr4rust 0.5.2` on stable.

---

## 5. Phase + task breakdown (the bead source)

Notation **[needs: …]** = blockers; **[xrepo]** = lives in the oraclemcp repo; `[SIGN-OFF]` = irreversible.
Tasks are phrased by symbol/file (lines drift) with verifiable acceptance criteria.

### Phase 0 — Ratify & scaffold (no code)
- **P0.1 Ratify** D1–D14 + §6 gates. **Accept:** founder approves in-session; a fresh branch off `main` is
  created. [needs: —] [blocks: all]
- **P0.2 Create the bead epic + children** from §5/§7/§12; `cargo run -p plan-lint -- --doctor` clean
  before `br create`. [needs: P0.1]
- **P0.3 Stand up release tracking under the new epic.** Do NOT reactivate `oracle-ublu` — it is CLOSED
  and scoped to the v0.5.0 *superset* release (being retired). The Ship tasks (PS.*) live as children of
  the new `oracle-offline-pivot` epic and are the 0.7.0→1.0 release tracking. [needs: P0.1]
- **P0.4 Re-home `oracle-tdgx`** to oraclemcp (D9); keep open until P5.4 confirms, then close "moved." [needs: P0.1]

### Phase 1 — Stabilize the engine on stable (NON-breaking, reversible)
- **P1.1 Finish gating the catalog live path under the EXISTING `oraclemcp-db` feature.** Make `asupersync`
  `optional = true` and pulled by `oraclemcp-db` (`oraclemcp-db = ["dep:oraclemcp-db", "dep:asupersync"]`);
  gate the `use asupersync::Cx`, the `OracleConnection` trait, and the ~26 ungated inline loaders under
  `#[cfg(feature = "oraclemcp-db")]` — move them into `crates/plsql-catalog/src/live.rs`
  (`#[cfg(feature="oraclemcp-db")] mod live;`), leaving the offline ingestion (fact 7) in `lib.rs`. Reuse
  `live-xe` as-is (it already implies `oraclemcp-db`). **Do not add a `live` feature.** **Accept:**
  `cargo +stable check -p plsql-catalog` (default) green; `cargo +nightly-2026-05-11 check -p plsql-catalog
  --features oraclemcp-db` green; `grep -n 'asupersync\|: &Cx' crates/plsql-catalog/src/lib.rs` returns
  nothing (all live refs now in `live.rs`). [needs: —] [blocks: P1.3, P1.4]
- **P1.2 Gate the cicd live path.** Make `asupersync` optional, pulled by `live-xe` (which already implies
  `plsql-catalog/live-xe`); gate the module-level `use asupersync::Cx;` in `inspector.rs` + `verify.rs`,
  the async `CicdOracleInspector`, and the whole `verify` module under `#[cfg(feature = "live-xe")]`; keep
  `is_read_only_sql`/`preview_sql` offline. **Accept:** `cargo +stable check -p plsql-cicd` green;
  `--features live-xe` green on nightly. [needs: —] [blocks: P1.4]
- **P1.3 Expose a public stable `CatalogSnapshotBuilder`** (or pub `apply_*_row`) taking `OracleRow`s →
  `CatalogSnapshot`; ensure `OracleRow`/`OracleBind` are publicly constructible. **Accept:** a doc-test
  builds a snapshot from synthetic rows on stable; documented. [needs: P1.1] [blocks: P5.2, PX.5]
- **P1.4 Flip toolchain to stable + MSRV + exclude `plsql-mcp` from `default-members`.** `rust-toolchain.toml`
  → `stable`; root `Cargo.toml` `rust-version` = empirical floor (bisect installed stables; 1.96 known-good);
  add `default-members` excluding `plsql-mcp` (so stable `cargo build`/`test` skip it). **Accept:** `cargo
  +stable check --workspace` green (plsql-mcp excluded); `cargo +stable test --workspace` green. [needs:
  P1.1, P1.2] [blocks: P1.5, P1.6, P4.2]
- **P1.5 CI toolchain: stable default + an explicit nightly transition job.** Convert the default jobs in
  **all five nightly-pinned workflows** (`ci.yml`, `release.yml`, `usr.yml`,
  `plsql-change-impact-selftest.yml`, `bindgen-roundtrip.yml`) to stable. ADD one nightly job (invoked via
  explicit `cargo +nightly-2026-05-11`/`RUSTUP_TOOLCHAIN=nightly-2026-05-11`, since the stable
  `rust-toolchain.toml` otherwise overrides `dtolnay/rust-toolchain`) that builds `-p plsql-catalog
  --features oraclemcp-db`, `-p plsql-cicd --features live-xe`, the `antlr-codegen` path, and (until P2.1)
  `-p plsql-mcp`. Keep Java installed in codegen/live jobs. Decide `bindgen-roundtrip.yml`'s fate per P5.x
  (relocate to oraclemcp) — until then keep it nightly + label it transitional. **Accept:** branch CI
  green. [needs: P1.4]
- **P1.6 Verify gates survive on stable.** `usr_acceptance.sh`, gate sha-pin, `accretion_tripwire.sh`,
  `plsql_mcp_boundary_lint.sh`, `plsql_mcp_honesty_grep.sh`, `oraclemcp_pin_guard.sh` all green (minimum
  edits now; full rewrite/rename in P4.4). **Accept:** each script exits 0 on stable. [needs: P1.4]
  [blocks: PS.0, PS.1]

> **Phase-1 milestone (mergeable, reversible):** offline engine + `plsql`/`plsql-depgraph`/`usr-loop`
> build & test on **stable**; the gated live feature + `plsql-mcp` build on the nightly job.

### Phase 1P — Parser de-risk (P1P.1 NON-blocking; P1P.2 GATES the ship)
- **P1P.1 Migrate `antlr-rust 0.3.0-beta` → `antlr4rust 0.5.x` (D14) — completed 2026-06-30.** Swapped
  the dep + jar, regenerated committed output, adapted the `build.rs` post-process patches + the
  `Antlr4RustBackend: ParseBackend` impl. **Accept:** `--features antlr-codegen` green on stable;
  `plsql-parser/tests/conformance.rs` + never-panic/fuzz suites pass unchanged. [needs: P1.4]
- **P1P.2 Commit the generated parser code (Q-JAVA=A, D13) — GATES the ship.** Independent of P1P.1: run
  codegen once with whichever runtime is current, move output to
  `crates/plsql-parser-antlr/src/generated/`; switch `lib.rs` `include!` from `OUT_DIR` →
  `concat!(env!("CARGO_MANIFEST_DIR"), "/src/generated/…")`; make `build.rs` codegen a **dev/CI-only** step
  gated on an env var (e.g. `PLSQL_ANTLR_REGEN=1`); add a CI job (with Java) that regenerates into a temp
  dir and `diff`s against the committed files (drift guard); mark the dir `linguist-generated` and exclude
  from fmt/clippy. **Accept:** `cargo +stable build -p plsql-engine` green **with no Java on PATH**; the
  drift-check job (with Java) green. [needs: P1.4] [blocks: PX.6]

### Phase 2 — Retire `plsql-mcp` + remove live code (BREAKING)
- **P2.2 Add `plsql doc` + `plsql sast` subcommands (D11).** Add `plsql-doc`+`plsql-sast` normal deps to
  `plsql-cicd`; wire `Doc`/`Sast` `Command` variants in `bin/plsql.rs` with `--robot-json`. Land **before**
  P2.1 so doc/sast never have a zero-consumer window. **Accept:** `plsql doc`/`plsql sast` run on stable;
  no orphan crates. [needs: P1.4] [blocks: P2.1]
- **P2.1 [SIGN-OFF] Remove the `plsql-mcp` crate.** Delete `crates/plsql-mcp/`; remove from `members` +
  `default-members` + `release.yml` `RELEASE_BINS`. Its tool logic + live-xe/hero tests remain in git
  history (reference for P5.3/P5.6). The published 0.6.0 stays installable (D4). **Accept:** `cargo +stable
  build --workspace` green; `grep -rn plsql-mcp Cargo.toml` empty; no dangling refs. [needs: P1.6, P2.2,
  §6 sign-off] [blocks: P2.3, P3.1, P4.1, P4.3, P4.5, PS.0]
- **P2.3 Remove the catalog/cicd live code (relocated to oraclemcp).** Delete `live.rs` + the
  `oraclemcp-db`/`live-xe` features + the `asupersync`/`oraclemcp-db`/(now-unused) `async-trait` deps from
  catalog + cicd; delete the `*_live_xe.rs` tests here; drop the transition nightly job from P1.5.
  **Accept:** `cargo tree -e normal -i asupersync` errors (no such dep in the graph); no `oraclemcp-db` in
  any `[dependencies]`; `cargo +stable build --workspace` green; **zero nightly code** (build-time Java is
  P1P.2's concern, separate). [needs: P2.1, P5.2 (extraction live in oraclemcp), P5.6 (tests migrated) — RISK-1]
  [blocks: P3.1, P4.4, PX.1, PS.1]
- **P2.4 Remove vestigial Java-backend naming.** Drop `ParserBackendChoice::JavaAntlrWorker`
  (`plsql-engine/src/lib.rs:13`) + the `--parser-backend antlr-java` diagnostic. **Accept:** no
  Java-backend refs; engine tests green. [needs: —]

### Phase 3 — Distribution retirement (IRREVERSIBLE; after the cut AND oraclemcp ships)
- **P3.1 [SIGN-OFF] Retire MCP-server build + CI.** Delete `Dockerfile`, `server.json`, `docker.yml`,
  `publish-mcp.yml`; set `release.yml` `RELEASE_BINS="plsql plsql-depgraph"` (consider `usr-loop`); remove
  the `oraclemcp-pin-guard` + `live-wire-xe` jobs from `ci.yml`; delete `scripts/oraclemcp_pin_guard.sh`.
  **Accept:** CI green; `grep -rn plsql-mcp .github Dockerfile server.json` empty. [needs: P2.1, P2.3]
- **P3.2 [SIGN-OFF] crates.io: deprecate `plsql-mcp`.** Do NOT hard-yank 0.6.0. Publish a final tombstone
  `0.6.1` whose `description`/README says "deprecated; offline engine = the `plsql-*` library crates;
  live+intelligence MCP = oraclemcp's `plsql-intelligence` feature". **Accept:** `cargo info plsql-mcp`
  (or the crates.io page) shows the tombstone notice; 0.6.0 still installable. [needs: P2.1, P5.5, sign-off]
- **P3.3 [SIGN-OFF] Retire the registry entry + GHCR image.** Mark `io.github.MuhDur/plsql-mcp` deprecated
  via the MCP-registry API (`mcp-publisher` deprecate, or the registry's documented deprecation endpoint —
  confirm the exact command at execution, RISK-4); stop publishing the GHCR image (existing tags may remain
  pullable; deletion optional via `gh api`). **Accept:** the registry no longer lists the server as active;
  no new images on tag. [needs: P2.1, P5.5, sign-off]

### Phase 4 — Docs, positioning, toolchain truth (non-breaking)
- **P4.1 README rewrite.** Offline library + CLIs; "MCP via oraclemcp"; drop the two-MCP-servers table;
  install: no nightly, no Java (P1P.2 gates the ship; source build needs Java only to *regenerate* the
  parser); fix badges (stable). [needs: P2.1]
- **P4.2 AGENTS.md (D8).** Sync-first/stable default; toolchain → stable + MSRV + codegen-Java note; remove
  two-servers + oraclemcp-pin notes. [needs: P1.4]
- **P4.3 plan.md.** §5 architecture: Layer 5 no longer contains `plsql-mcp`; MCP surface external;
  reconcile layer tables; drop JavaAntlrWorker. `plan-lint` clean. [needs: P2.1]
- **P4.4 Lint/honesty scripts.** Rewrite/rename `plsql_mcp_boundary_lint.sh` → boundary lint that bans any
  `oraclemcp-*`/`oracle`/`oracledb` dep anywhere; rewrite `plsql_mcp_honesty_grep.sh` phrase sets
  (nightly/asupersync/Instant-Client/two-servers flip). **Accept:** lints green via their `--self-test`
  and catch real drift; no script references removed files. [needs: P2.3]
- **P4.5 Sweep `docs/**` (18 files) + `.github/assets`.** Update every `docs/**` file mentioning
  `plsql-mcp` (integrations/live-db/*, mcp-clients, mcp-server-listing, architecture(.md/ARCHITECTURE.md),
  components/*, session-pickup, oraclemcp/SECURITY) to the offline-library framing; regenerate
  `hero.svg`/`og.svg`/`og.png` without MCP-server branding. **Accept:** `grep -rl plsql-mcp docs .github/assets`
  returns only intentional historical references; honesty-grep (P4.4) green over `docs/`. [needs: P2.1]
- **P4.6 CHANGELOG 0.7.0 entry** + preserve the `coverage_index` continuity table; regenerate `Cargo.lock`
  and confirm `plsql-change-impact-selftest.yml` (triggers on `Cargo.lock`) green. [needs: P1.6]

### Phase 5 — Cross-repo: oraclemcp consumes the engine [xrepo]
- **P5.1 Add optional `plsql-intelligence` feature to oraclemcp** → depend on the engine library crates
  (git/path dep during dev; switch to crates.io 0.7.0 at P5.7). [needs: P1.3]
- **P5.2 Port live catalog extraction into oraclemcp** using this repo's public `CatalogSnapshotBuilder`
  (P1.3) over `oraclemcp-db`; lift the SQL from the **current tree** (`plsql-catalog/src/live.rs`, present
  until P2.3). [needs: P1.3] [blocks: P2.3]
- **P5.3 Expose intelligence MCP tools in oraclemcp** (parse/analyze, what_breaks, lineage, sast, doc),
  reusing the `plsql-mcp` tool logic from the **current tree / git history**; couple with live (live
  snapshot → engine; blast-radius-on-write). [needs: P5.1, P5.2]
- **P5.4 Take ownership of `oracle-tdgx` / oraclemcp#4.** [needs: P0.4]
- **P5.5 Offer an intelligence-enabled oraclemcp build/image** for the offline-MCP use case. [needs: P5.3]
  [blocks: P3.2, P3.3]
- **P5.6 Migrate the `live-xe`/hero-demo tests** into oraclemcp (RISK-1). [needs: P5.3] [blocks: P2.3]
- **P5.7 Switch oraclemcp to the crates.io 0.7.0 engine dep** for its release. [needs: PS.1]

### Phase X — Verification (cross-cutting)
- **PX.1** Full clean-workspace `cargo +stable test` green (post-removal). [needs: P2.3]
- **PX.2** `coverage_index` monotone tripwire green across the pivot tag. [needs: P1.6]
- **PX.3** `usr_acceptance.sh` green. [needs: P1.6]
- **PX.4** `plsql predict|doc|sast` golden + `--robot-json` schema-stability. [needs: P2.2]
- **PX.5** Public `CatalogSnapshotBuilder` doc-test/golden (the oraclemcp seam). [needs: P1.3]
- **PX.6** Parser conformance + never-panic/fuzz unchanged after P1P. [needs: P1P.2]

### Ship
- **PS.0 (OPTIONAL) early `0.7.0-alpha` publish** of the engine + CLIs (after `plsql-mcp` removed; catalog
  may still carry the gated `oraclemcp-db` feature). Gives immediate stable crates.io install; signals API
  may move. [needs: P1.6, P2.1] (non-blocking)
- **PS.1 Publish the clean 0.7.0** engine + CLIs (align all engine crates incl. `plsql-support` to 0.7.0;
  publish to crates.io; release binaries `plsql`,`plsql-depgraph` + `SHA256SUMS`). [needs: P2.3, P4.*, PX.*]
- **PS.2 Tag `v0.7.0`** + GitHub release. [needs: PS.1]
- **PS.3 1.0** once the offline library API settles (exit condition: one minor cycle with no breaking API
  change + the §11 checks green). [needs: PS.2 + one release cycle]

---

## 6. Irreversible sign-off gates (explicit founder approval at execution)
1. **P2.1** — delete the `plsql-mcp` crate (AGENTS.md RULE 1).
2. **P3.1** — delete `Dockerfile`/`server.json`/`docker.yml`/`publish-mcp.yml`.
3. **P3.2** — crates.io tombstone (confirm deprecate-not-yank).
4. **P3.3** — registry-entry + GHCR-image retirement.
Each restated verbatim with blast radius before running.

---

## 7. Dependency DAG (acyclic — verified)

```
P0.1 → P0.2/3/4 → (all)
P1.1 ─┐                         P1.1 → P1.3 → {P5.2, PX.5}
P1.2 ─┴→ P1.4 → P1.5
                → P1.6 → {PS.0, PS.1, PX.2, PX.3, P4.6}
P1.4 → {P1P.1 (non-blocking leaf),  P1P.2 → PX.6,  P4.2,  P2.2}
P2.2 → P2.1[SO] → {P3.1, P4.1, P4.3, P4.5, PS.0}
P5.1/P5.2(→needs P1.3) → P5.3 → {P5.5, P5.6};  P0.4 → P5.4
P2.1 + P5.2 + P5.6 → P2.3 → {P3.1, P4.4, PX.1, PS.1}
P2.1 + P5.5 → {P3.2[SO], P3.3[SO]}
P2.2 → PX.4
{P2.3, P4.*, PX.*} → PS.1 → {PS.2, P5.7};  PS.2 → PS.3
P2.4 (independent)
```
**No cycles.** PS.1 depends on P2.3 (clean) and PX.* but **not** on P5.1 (oraclemcp dev uses a git dep;
the crates.io switch is P5.7, *after* PS.1). The only cross-repo edges INTO this repo's critical path are
**P5.2 → P2.3** and **P5.6 → P2.3** (extraction + tests live in oraclemcp before we delete them here) and
**P5.5 → P3.2/P3.3** (replacement shipped before distribution retirement). **Critical path:** P0.1 → P1.1
→ P1.4 → {P1.6, P2.2} → P2.1 → (oraclemcp P5.2/P5.6) → P2.3 → PX.1 → PS.1 → PS.2 (parser stream
P1P.2 → PX.6 → PS.1 runs in parallel and also gates PS.1).

---

## 8. Risks & mitigations
- **RISK-1 — live coverage lost at P2.3.** *Mitigation:* DAG-enforced — P2.3 needs P5.2 (extraction works
  in oraclemcp) AND P5.6 (tests migrated). No silent drop.
- **RISK-2 — stale `antlr-rust 0.3.0-beta`.** *Mitigation:* closed by D14/P1P.1 on 2026-06-30; the
  parser backend now uses `antlr4rust 0.5.2`.
- **RISK-3 — catalog untangle (P1.1).** *Mitigation:* hand-move to `live.rs` (no codemod), compile-iterate
  stable+nightly; offline ingestion stays in `lib.rs`.
- **RISK-4 — registry/GHCR retirement mechanics uncertain.** *Mitigation:* P3.3 confirms the exact
  `mcp-publisher`/registry command at execution; default to deprecate not delete; `log` reversibility.
- **RISK-5 — committed 15.4 MB generated code bloats the repo.** *Mitigation:* `linguist-generated`,
  fmt/clippy-excluded, drift-checked; payoff = no nightly + no Java. Revisit if clone size complaints.
- **RISK-6 — coverage_index/gate continuity.** *Mitigation:* PX.2/PX.3 required Phase-1 acceptance.
- **RISK-7 — cross-repo coupling (PS.1 waits for oraclemcp P5.2/P5.6).** *Mitigation:* the engine is usable
  on stable from the Phase-1 milestone (git/source install) and via the optional PS.0 alpha; the clean
  crates.io 0.7.0 follows the relocation. The repo is shippable throughout.
- **RISK-8 — `bindgen-roundtrip.yml` is a live-DB CI test in an offline repo.** *Mitigation:* P1.5 keeps it
  nightly + transitional; P5.6-class work relocates the live round-trip into oraclemcp (track as an
  oraclemcp bead); `plsql-bindgen`'s codegen stays offline here, only the `live-roundtrip` *test* moves.

---

## 9. Cross-repo (oraclemcp) summary
oraclemcp opts in via a `plsql-intelligence` feature (D12), pulls the 0.7.0 engine crates, ports live
extraction over `oraclemcp-db` feeding our public builder (P5.2), exposes intelligence tools coupled with
live access (P5.3), takes the TNS-hang bug (P5.4), migrates the live tests (P5.6), offers an
intelligence-enabled build (P5.5), and switches to the crates.io dep at release (P5.7). The
`P5.2/P5.6 → P2.3` and `P5.5 → P3.2/P3.3` edges are the cross-repo safety gates.

---

## 10. Open questions
**None.** All design decisions settled (D1–D14). Execution-time verifications only: empirical MSRV (P1.4),
antlr4rust migration boundedness (P1P.1 spike + fallback), registry/GHCR retirement command (P3.3).

---

## 11. Validation loop (planning-workflow self-check, v4)
- **Self-containment:** every §5 task names files/symbols + verifiable acceptance; §2 gives grounded facts. ✅
- **Dependency-graph:** §7 re-derived acyclic; the v3 PS.1→cut cycle is removed (single clean ship, P5.7
  switch after PS.1); orphans (doc/sast) handled before removal (P2.2→P2.1); RISK-1 DAG-enforced
  (P5.2/P5.6→P2.3). ✅
- **Justification:** D1–D14 each carry a *why*. ✅
- **Steady-state:** REACHED. Round 3 verified all 21 prior items fixed + DAG acyclic; the one blocking
  contradiction (B1: Phase-1P "non-blocking" vs ship edges) is resolved in v5 by splitting P1P.1
  (non-blocking) from P1P.2 (gates ship). Remaining changes are wording-level. **Ready for bead conversion.**

---

## 12. Bead-conversion guide
Epic **`oracle-offline-pivot`** (parent), children = §5 tasks (P0.1…PS.3), edges from §7. Label
**P2.1, P3.1, P3.2, P3.3** `needs-signoff`. Label **P5.\*** `xrepo` (oraclemcp). Re-home `oracle-tdgx`
(P0.4/P5.4). `oracle-ublu` is CLOSED (superset v0.5.0) — do NOT reactivate; release tracking is the PS.*
children of this epic (P0.3). Cross-repo edges to record explicitly:
`P5.2→P2.3`, `P5.6→P2.3`, `P5.5→P3.2`, `P5.5→P3.3`, `PS.1→P5.7`. Run `plan-lint --doctor` before
`br create`. Suggested creation order (respects deps):

```
P0.* → P1.1,P1.2 → P1.3,P1.4 → P1.5,P1.6,P1P.1,P2.2,P2.4,P4.2 → P1P.2 →
P5.1,P5.2,P5.3,P5.4,P5.5,P5.6 (xrepo) → P2.1 → P2.3 → P3.*,P4.1,P4.3,P4.4,P4.5,P4.6 →
PX.* → PS.0,PS.1 → PS.2 → P5.7 → PS.3
```
