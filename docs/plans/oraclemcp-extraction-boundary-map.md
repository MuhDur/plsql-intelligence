All findings are verified against the actual code. I have confirmed:
- Only `analyze_project.rs` (line 19) and `foundation_tools.rs` (line 18) import `plsql_engine` — these are the sole engine-lifecycle couplings.
- `OracleConnection` trait at `plsql-catalog/src/lib.rs:925`, `RustOracleConnection` at 972, all gated by `oracle-driver` feature.
- `is_read_only_sql` at `query.rs:367` with its fail-open vectors (literal-blind multi-statement, ASCII-only `FOR UPDATE`, function-call blindness).
- `Measured<T>` at `plsql-core/src/lib.rs:626`, `column_writers` at `plsql-lineage/src/lib.rs:1608`, `edge_kind_is_expected_gap` at `plsql-depgraph/src/lib.rs:1482`.

Now I'll produce the final document.

# oraclemcp — EXTRACTION / BOUNDARY MAP (P0-0)

Synthesis of the reconnaissance pass over `crates/plsql-mcp`, `crates/plsql-catalog`, and the layer-1/2 analysis crates, mapped against plan `docs/plans/2026-06-01-oraclemcp-shared-mcp-core.md` §0, §14, §15. All file paths are absolute under `/home/durakovic/projects/plsql-intelligence`. Per §0, every "new crate `oraclemcp-*`" is, in Phase A, a cleanly-bounded **module/crate inside the workspace** that imports no `plsql_*` engine crate; Phase E lifts it mechanically.

---

## 1. Engine-free core inventory

Target crates per plan §14: `oraclemcp-core` (rmcp ServerHandler + ToolRegistry/Tool, protocol, trust, resources), `oraclemcp-db` (OracleConnection + pool + serializer), `oraclemcp-guard` (fail-closed classifier + SideEffectOracle port + operating levels + approval token/level state), `oraclemcp-audit` (out-of-band durable sink), `oraclemcp-auth` (transport auth + step-up *delivery*), `oraclemcp-telemetry` (tracing/OTel/health), `oraclemcp-config` (figment config/profiles).

| Existing file | Target crate | generic/coupled | What must change |
|---|---|---|---|
| `crates/plsql-mcp/src/mcp_protocol.rs` | oraclemcp-core | generic | None structurally for the in-place module. `handle_request`/`handle_request_line` accept abstract `ToolRegistry`; `dispatch::dispatch_tool` becomes an injected fn (see §2). Trust injection at `JsonRpcResponse::ok` (lines ~84, ~567) calls `crate::trust::attach_trust_block` — move trust with it. Phase E: replace hand-rolled JSON-RPC with `rmcp` (P0-6) — this whole file is the rmcp swap target. |
| `crates/plsql-mcp/src/tcp.rs` | oraclemcp-core | generic | None. `process_stream<R,W>` calls `crate::mcp_protocol::handle_request_line` — becomes an external import. Loopback-only safety gate (`parse_listen_target`, `ListenTarget`) is domain-agnostic; keep. |
| `crates/plsql-mcp/src/tools.rs` | oraclemcp-core | generic | None. `ToolDescriptor{name,tier,summary}`, `ToolRegistry` (dedup-by-name). `ToolTier::{FoundationStatic,FoundationLiveDb}` names are plsql-flavored; rename to generic tiers at Phase E (or keep — they are just strings). |
| `crates/plsql-mcp/src/trust.rs` | oraclemcp-core | generic | Schema id `plsql.mcp.trust_block` (TRUST_BLOCK_SCHEMA_ID) and version `1.0.0` are hardcoded; rename id/bump version at extraction. `attach_trust_block` is idempotent JSON merge — keep. |
| `crates/plsql-mcp/src/dispatch.rs` | **stays engine-side** (architecture reusable) | plsql-coupled | The tri-state `DispatchOutcome{Ran,RuntimeStateRequired(RuntimeKind),Error}` + `parse_args::<T>` + lockstep test (`dispatch_table_matches_default_registry`) is the reusable *pattern*; the *table* hard-codes `crate::run_*` engine impls. Keep table engine-side; oraclemcp-core gets only the `RuntimeKind`/`DispatchOutcome`/`DispatchError` types + the dispatch-fn injection point. `RuntimeKind::message` strings ("foundation MCP server") need editing. |
| `crates/plsql-mcp/src/config.rs` | oraclemcp-config | generic | Trivial (14 LOC, data shape only). `McpConfig{transport,safety,connections_path}`, `TransportConfig::{Stdio,Tcp}`. `safety: SafetyProfile` field imports from oraclemcp-guard. No loader yet — P0-2 adds figment + precedence + versioned schema + `default_level`/`max_level`/OCI fields. |
| `crates/plsql-mcp/src/connections.rs` | oraclemcp-db (or oraclemcp-config) | generic | None for the in-place module. `ConnectionProfile`, `ConnectionRegistry`, `DbToolsAlias::probe`. **Couples one-way to `SafetyProfile`** in `set_safety` (checks `permanently_read_only` ∧ `next.allows_direct_writes()`) — must import guard, never the reverse. Hardcoded path `~/.plsql-mcp/connections.toml` → `~/.config/oraclemcp/profiles.toml` at extraction; add `max_level`/`protected`/`read_only_standby`/OCI fields (§8.4). |
| `crates/plsql-mcp/src/safety.rs` | oraclemcp-guard | generic | **P0-CLK fix is load-bearing (§5.10, P1-10):** `EnableWritesToken::is_expired`/`mint_token` capture `issued_at` as `SystemTime::now() → UNIX_EPOCH` u64 (`safety.rs:~128`, `~175`); a backward clock makes `saturating_sub` clamp to 0 → expired token reads fresh. Convert TTL checks to a monotonic `Instant` deadline anchored at issue; tokens are `Serialize`/`Deserialize` so **reject (fail-closed) any deserialized token whose monotonic anchor is from a prior process generation.** `SafetyProfile` enum is the operating-level seed (§6.6); `SessionSafetyState` + `EnableWritesToken` are the enforcement machinery. `ENABLE_WRITES_TOKEN_TTL_SECONDS=60` is shared with preview.rs — hoist to a guard consts module. |
| `crates/plsql-mcp/src/preview.rs` | oraclemcp-guard (registry to oraclemcp-core per recon, but guard is the coherent home for the approval state machine) | generic | Same P0-CLK fix as safety.rs (`PreviewedDdl::is_expired_at`, `~98`). Imports `safety::ENABLE_WRITES_TOKEN_TTL_SECONDS` (line ~35) — resolve the shared constant. `PreviewRegistry` (BTreeMap connection→PreviewedDdl, one-preview-per-connection, re-issue invalidates), `verify_byte_for_byte` (string equality on `ddl_bytes`, SHA-256 for operator visibility only), `purge_expired`. SHA-256 via a thin hasher wrapper to avoid hard-tying `sha2`. **Byte-stability invariant** must be preserved exactly (see §6). |
| `crates/plsql-mcp/src/cross_schema.rs` | oraclemcp-guard | generic | `require_cross_schema_confirmation`, `CrossSchemaConfirmation{SameSchema,CrossSchemaConfirmed}`. Generic principal-vs-target comparison after case-normalization. Must consume the schema parsed from the **byte-verified** DDL header, never the caller field (oracle-jy0w). |
| `crates/plsql-mcp/src/audit.rs` | oraclemcp-audit | generic | **Single biggest plan↔code gap (§5.13, P1-4).** Today = in-session markers only: `DBMS_APPLICATION_INFO` SET_MODULE/SET_ACTION, `/* plsql-mcp … */` comment marker, optional INSERT into a customer Oracle table, `is_valid_audit_table_name`/`is_simple_sql_name` validators. The optional INSERT **rides the audited statement's own transaction** → any ROLLBACK (savepoint preview §5.4, cancel §5.7, error) erases the row, violating "logged before it runs." Must add an **out-of-band** `AuditSink` trait (append-only file + SQLite/WAL), **fsync-before-execute** for Guarded/Destructive/escalation, batched fsync for pure reads, a **monotonic seq** as the hash-chain order key (not wall clock), and a §12 `kill -9`-between-fsync-and-execute test. Existing markers + validators stay (synergistic, in-band). |
| `crates/plsql-mcp/src/doctor.rs` | oraclemcp-core (+ oraclemcp-config split optional) | generic | One soft coupling: `plsql_core::AnalysisProfile::default()` at `doctor.rs:311` — used only as a bounds-check (compatibility ≤ oracle_version), zero engine intelligence. Replace with a generic `OracleVersion{major,minor}` param or inject via config pre-extraction. `DoctorReport`, `McpHealth`, `DoctorFinding`, `InstantClientPosture`, `derive_write_posture` are zero-engine. P1-DOC extends 4→9 checks; current synchronous emission is fine (async deferred). |
| (new) | oraclemcp-auth | new | No current file. Transport auth (stdio init-token HMAC, OAuth 2.1 RS, mTLS), step-up confirmation **delivery** (elicitation/selector + poll/Task), secrets backends. Depends one-way on guard (auth mints into guard's token/level type; guard never imports auth). P1-9/P1-10. |
| (new) | oraclemcp-telemetry | new | No current file. tracing/OTel/metrics/health. P1-8. Today only `tracing` instrumentation is sprinkled in plsql-catalog (`query_optional_row`/`query_one_row` with `skip(self,sql,params)` — preserve). |
| `crates/plsql-catalog/src/lib.rs` (driver+conn layer: `OracleConnection` trait l.925, `RustOracleConnection` l.972, `OracleConnectOptions`, `OracleBind` l.762, `OracleRow` l.829, `OracleBackend` l.677, `CatalogError` l.~635) | oraclemcp-db | generic | **Cleanest extraction; P0-3.** Lift the trait + impl + value types + error enum out of plsql-catalog. All `RustOracleConnection` code is `#[cfg(feature="oracle-driver")]` — preserve the feature gate (oraclemcp-db gets `live-db`/`oracle-driver`). Driver is synchronous, not cloneable, no pool — P0-3 adds `r2d2-oracle` + `spawn_blocking` boundary at the MCP layer. Recon suggests splitting `CatalogError` into `DbError` (Io/Json/RowCount/Null/InvalidValue) + leaving snapshot-versioning variants (UnsupportedSchemaVersion, UnexpectedSchemaId) behind. |

**Not moving (engine-side, in plsql-catalog/plsql-engine):** the entire `CatalogSnapshot` metadata model + 40+ metadata types, `load_snapshot_from_connection` and its 15+ private `load_catalog_*` loaders, `negotiate_capabilities`, `populate_dbms_metadata_ddl`, `load_snapshot_from_json`/`export_snapshot_to_json`, `SyntheticCatalogBuilder`, and all of `plsql-engine/src/lib.rs` (`analyze_project`, `AnalysisRun`, `EngineDoctorReport`). These encode Oracle dictionary semantics and `SymbolInterner`-based interning — extracting them would mean extracting the engine.

---

## 2. Engine-side handlers (stay in plsql-mcp; register Tool impls into the core registry)

These import layer-1/2/3 analysis crates and/or the `OracleConnection` trait. Per §0 hard rule 1, engine intelligence reaches the core by **implementing the core's Tool/registry contract from the engine side** — the core never reaches in. Each `register_*` helper populates the shared `ToolRegistry`; each `run_*` is the impl.

| File | Stays because | Engine couplings (verified) |
|---|---|---|
| `analyze_project.rs` | **The P0-0 cut line** — the only file that *runs* the engine pipeline. | `plsql_engine::{AnalysisRequest, EngineDoctorReport, analyze_project, engine_doctor_report}` (line 19). |
| `foundation_tools.rs` | **Mixed.** `dynamic_sql_evidence` + `doc_lookup` are pure source-text (generic, extractable later); `completeness_report` runs the engine. | `plsql_engine::{AnalysisRequest, analyze_project}` (line 18); `plsql_core::CompletenessReport`, `plsql_doc::extract_doc_comments`, `plsql_symbols::recognise_dynamic_sql`. Mark `// ENGINE-SIDE: completeness_report` / `// GENERIC: dynamic_sql_evidence, doc_lookup` for mechanical Phase E split. |
| `change_tools.rs` | Pure delegation to layer-2/3 analysis on already-built artifacts. No engine *lifecycle*. | `plsql_depgraph`, `plsql_lineage` (classify_dir_diff, recompile_order, explain_node, compare_oracle_deps, detect_orphans), `plsql_cicd` (predict, run_gate, ChangeSet, GatePolicy), `plsql_sast` (to_sarif), `plsql_output::OrphanCandidatesReport`, `plsql_catalog::CatalogSnapshot`, `plsql_core::SymbolInterner` (19 imports — **none is `plsql_engine`**). |
| `parse_tools.rs` | Pure source-text functions; layer-1/2 only. | `plsql_core::{AnalysisProfile,FileId,Severity,SymbolInterner}`, `plsql_ir::lower_top_level`, `plsql_parser_antlr::lower::lower_source`, `plsql_symbols::DeclTable`. No `plsql_engine`. |
| `graph_tools.rs` | Pure query wrappers over a *provided* `&DepGraph`. | `plsql_depgraph::{DepGraph,NeighborhoodQueryResult,NodeSelector}`. No `plsql_engine`. |
| `query.rs` | Live-DB read tool; the read-only **gate** stays here as a trait impl (see §5). Generic transforms (sanitize, truncate, response envelope) extract to core. | `plsql_catalog::{CatalogError,OracleBind,OracleConnection,OracleRow}`, `plsql_core::UnknownReason`. |
| `source.rs`, `compile.rs`, `list_objects.rs`, `describe.rs` | Oracle dictionary handlers (ALL_SOURCE/ALL_ERRORS/ALL_OBJECTS/ALL_TABLES…). Response *types* are generic; runners are Oracle-specific. | `plsql_catalog::{CatalogError,OracleBind,OracleConnection,OracleRow}`; source.rs+compile.rs also `plsql_core::UnknownReason`; source.rs imports `crate::query::sanitize`. |
| `create_or_replace.rs`, `patch.rs`, `execute_approved.rs` | **Safety spine tool entry points.** Generic safety machinery (PreviewRegistry, cross-schema, `run_execute_approved`) moves to guard/core; these stay as the engine-side handlers + DDL synthesizers. | Only `crate::preview`, `crate::cross_schema`, `crate::create_or_replace::parse_target_schema` — **no `plsql_*` engine imports.** `build_deploy_plan` (DBMS_SCHEDULER, `''`-doubling quote escaping resisting `q'[…]'` breakout — oracle-tx8d) stays engine-side. |
| `dispatch.rs` | Hard-codes the `crate::run_*` table (lockstep with `default_tool_registry`). | Re-exports of plsql-mcp's own Request types + run_* (see §1 row). |

`lib.rs` `default_tool_registry()` currently registers generic-tier tools (connection/safety) *and* coupled tools in one function. **Phase A change:** split into a core `base_tool_registry()` (protocol/connection/safety descriptors) in oraclemcp-core + an engine-side extension that adds the static-analysis + live-DB handler descriptors.

---

## 3. The one-way boundary — what the CI dependency-lint must forbid, and current violations

**Rule (§0 hard rule 1; §15 P0-0, E-1):** the generic core (the future `oraclemcp-*` modules/crates) must **never** import any `plsql_*` *engine* crate. The hard forbidden import is **`plsql_engine`** (`AnalysisRequest`, `EngineDoctorReport`, `analyze_project`, `engine_doctor_report`). The core *may* import layer-1/2 analysis crates (`plsql_core`, `plsql_parser_antlr`, `plsql_symbols`, `plsql_depgraph`, `plsql_lineage`, `plsql_cicd`, `plsql_sast`, `plsql_output`, `plsql_catalog`, `plsql_doc`, `plsql_ir`, `plsql_store`) — these are precursor artifacts, not engine lifecycle.

**CI lint must assert:** the oraclemcp-core/db/guard/audit/auth/telemetry/config crates have **zero dependency on `plsql_engine`** in `Cargo.toml`, and a source-grep gate forbids `use plsql_engine` / `plsql_engine::` in any file slated for those crates. Implement via `cargo deny` (`[bans] deny = [{name="plsql-engine"}]` scoped to the core crates) plus a grep guard in CI.

**Current violations in the generic set — what P0-0 must fix:**

1. **`crates/plsql-mcp/src/foundation_tools.rs:18`** — `use plsql_engine::{AnalysisRequest, analyze_project};`. This file is "mixed": `dynamic_sql_evidence` and `doc_lookup` are generic but `completeness_report` runs the engine. **Fix:** keep the file engine-side for Phase A with explicit `// ENGINE-SIDE` / `// GENERIC` markers; the generic two extract at Phase E. It must **not** be placed in an oraclemcp-* module while it imports `plsql_engine`.
2. **`crates/plsql-mcp/src/analyze_project.rs:19`** — `use plsql_engine::{…}`. This is the **intended** engine-side file (the boundary cut line); it is a violation only if mis-filed into the core. **Fix:** keep engine-side; it is the reference for "build operations stay engine-side."

**Soft coupling (not a hard violation, but flagged):** `crates/plsql-mcp/src/doctor.rs:311` imports `plsql_core::AnalysisProfile::default()`. `plsql_core` is a layer-1 crate (allowed by the rule), but `AnalysisProfile` is a config/data type bundled with engine primitives. To keep oraclemcp-core's dep surface clean, replace with a generic `OracleVersion` param before extraction.

**Everything else in the generic set is already clean** (verified by grep): `safety.rs`, `preview.rs`, `audit.rs`, `connections.rs`, `config.rs`, `trust.rs`, `tools.rs`, `mcp_protocol.rs`, `tcp.rs`, `dispatch.rs` import **no `plsql_*` crate at all**. The boundary is mostly already true in practice — P0-0 makes it *structural* and *enforced*.

Phase E gate (§15) requires the lint to have been **green for ≥30 days** before extraction fires.

---

## 4. P0-0 concrete step list (ordered, low-risk, in place, preserving tests)

Per §15 P0-0, this is "a real but bounded refactor, not a `Cargo.toml` edit." Sequence chosen so each step compiles + passes tests before the next, and the safety-critical P1-1 classifier swap lands on a stable boundary.

1. **Add the CI dependency-lint first (declares the target).** Add a `cargo deny` config + a CI grep gate that forbids `plsql_engine` in the to-be-core files. It will currently flag `foundation_tools.rs` and `analyze_project.rs` — that is the expected starting state and documents the boundary work. Add `CONTRIBUTING.md` rule: "new MCP tool handlers must not import `plsql_engine`; engine results arrive as `AnalysisRun`/`DepGraph`/`CatalogSnapshot` parameters."

2. **Create `oraclemcp-db` crate; lift the driver layer out of plsql-catalog (P0-3 prep).** Move `OracleConnection` trait (`plsql-catalog/src/lib.rs:925`), `RustOracleConnection` (972), `OracleConnectOptions`, `OracleBind` (762), `OracleCell`/`OracleRow` (829), `OracleConnectionInfo`, `OracleBackend` (677), `rust_oracle_driver_compiled`, and the DB-generic subset of `CatalogError` (635) into `oraclemcp-db`. Preserve `#[cfg(feature="oracle-driver")]` and the `tracing` instrumentation. `plsql-catalog` re-exports them (or depends on oraclemcp-db) so `query.rs`/`source.rs`/`compile.rs`/`describe.rs`/`list_objects.rs` and the 15+ loaders keep compiling unchanged. **Highest test risk** — run the live-XE catalog tests here.

3. **Create `oraclemcp-core` crate; move protocol + transport + registry + trust.** Move `mcp_protocol.rs`, `tcp.rs`, `tools.rs`, `trust.rs` and the `RuntimeKind`/`DispatchOutcome`/`DispatchError` types from `dispatch.rs`. Inject the dispatch fn into `handle_tools_call` (it currently calls `dispatch::dispatch_tool` directly at `mcp_protocol.rs:~210`) so the table stays engine-side. Keep `plsql-mcp` re-exporting these so `main.rs` and tests are unchanged. The lockstep test `dispatch_table_matches_default_registry` stays engine-side.

4. **Create `oraclemcp-guard` crate; move safety + preview + cross-schema + the generic safety spine.** Move `safety.rs`, `preview.rs`, `cross_schema.rs`, plus `execute_approved::{run_execute_approved, consume_approved, ApprovedExecutionPlan, ExecuteApprovedError}` and the DDL header parsers `create_or_replace::{classify_kind, parse_target_schema}`. Resolve the shared `ENABLE_WRITES_TOKEN_TTL_SECONDS`. Leave the tool entry points (`run_create_or_replace`, `run_patch_*`, `build_deploy_plan`, DDL synthesizers) engine-side, consuming guard via re-export. **Add the `SideEffectOracle` port trait here with a default impl returning `Unknown` (fail-closed)** — so the guard ships functional with no engine dep (§5.3).

5. **Create `oraclemcp-config` + move `config.rs`; create `oraclemcp-audit` + move `audit.rs`.** Config is trivial (data shape). Audit moves as-is; the durable out-of-band sink (P1-4) is added *after* extraction, in oraclemcp-audit. `connections.rs` moves to oraclemcp-db (or oraclemcp-config), importing `SafetyProfile` from guard one-way.

6. **Move `doctor.rs` to oraclemcp-core** after replacing `plsql_core::AnalysisProfile::default()` (line 311) with a generic `OracleVersion` param.

7. **Mark `foundation_tools.rs`** with `// ENGINE-SIDE: completeness_report` / `// GENERIC: dynamic_sql_evidence, doc_lookup`; leave it engine-side for Phase A. **Keep `analyze_project.rs` engine-side** (the cut line).

8. **Split `default_tool_registry()`** into `base_tool_registry()` (core) + engine-side extension.

9. **Verify:** `cargo nextest run` across the workspace green; CI dependency-lint green for the core crates (now only `analyze_project.rs` + `foundation_tools.rs::completeness_report` legitimately touch `plsql_engine`, both engine-side). Start the 30-day clock for the Phase E gate.

Each step preserves the existing test suites because `plsql-mcp` keeps re-exporting moved symbols at their old paths until Phase E.

---

## 5. Key reusable APIs

### oraclemcp-db — the OracleConnection / RustOracleConnection surface
`crates/plsql-catalog/src/lib.rs`:
```rust
pub trait OracleConnection: Send + Sync {           // l.925
    fn backend(&self) -> OracleBackend;
    fn ping(&self) -> Result<(), CatalogError>;
    fn describe(&self) -> Result<OracleConnectionInfo, CatalogError>;
    fn query_rows(&self, sql: &str, params: &[OracleBind]) -> Result<Vec<OracleRow>, CatalogError>;
    fn execute(&self, sql: &str, params: &[OracleBind]) -> Result<u64, CatalogError>;
    // default helpers:
    fn query_optional_row(...) -> Result<Option<OracleRow>, CatalogError>;
    fn query_one_row(...) -> Result<OracleRow, CatalogError>;
}
pub struct RustOracleConnection { /* l.972, #[cfg(feature="oracle-driver")] inner: DriverConnection */ }
impl RustOracleConnection { pub fn connect(options: OracleConnectOptions) -> Result<Self, CatalogError>; }  // l.980
pub enum OracleBind { String(String), I64(i64), U64(u64), Bool(bool) }   // l.762
pub struct OracleRow { /* text(name), require_text, parse_i64/u64/bool */ } // l.829
pub enum OracleBackend { RustOracle, OracleRs }      // l.677 (OracleRs = placeholder)
pub fn rust_oracle_driver_compiled() -> bool;        // l.967
```
Synchronous, single-connection, not cloneable, no pooling. P0-3 wraps with `r2d2-oracle` + `spawn_blocking` at the MCP layer (never hold an `oracle::Connection` across `.await`). `Send + Sync` bound on the trait is the seam the async pool needs.

### oraclemcp-guard — the read-only predicate to replace (P1-1) and its fail-open paths
`crates/plsql-mcp/src/query.rs:367` `fn is_read_only_sql(sql: &str) -> bool`. **Verified fail-open / fragile vectors that the engine-aware fail-closed classifier must close (§5.3):**
- **Function-call blindness (the headline fail-open, §5.3):** `SELECT billing.purge_old_rows() FROM dual` passes as read-only — a user function may DML via `EXECUTE IMMEDIATE`/`AUTONOMOUS_TRANSACTION`. The classifier must require a `ProvenReadOnly` purity verdict for any user-defined function; absence of a write edge is `Unknown`, never `Safe`.
- **Literal-blind multi-statement / `FOR UPDATE`:** `has_trailing_non_empty_statement` (l.428) scans for `;` with comment-skipping but **does not parse string/quoted literals** — `SELECT 'a;b' FROM t` misclassifies as multi-statement, and a crafted `q'{ … END; … }'` can hide a boundary the other direction. `has_for_update_lock` (l.406) tokenizes on `!is_ascii_alphanumeric` — **ASCII-only**, so non-ASCII/quoted identifiers can evade. Plan mandates a **lexer-based** depth state machine that fails-closed (`Forbidden`) on desync.
- **Fail-closed paths that are correct (preserve the posture):** unterminated `/*` comment → `false` (l.373); non-`SELECT`/`WITH` leading token → `false` (l.381).

Generic data-plane to extract to oraclemcp-core from `query.rs`: `sanitize` (3-layer: zero-width/control strip → fullwidth `<…>` neutralization → blocklist redaction), LOB char truncation, `QueryResponse`/`QueryCell`/`QueryColumnMeta`, `UNTRUSTED_DATA_NOTICE`. Replace `plsql_core::UnknownReason` coupling with a generic `ExecutionNote`/`SanitizationMarker`.

### SideEffectOracle-relevant engine APIs (the purity consult; engine-side, bound to the guard port)
- `crates/plsql-core/src/lib.rs:626` `pub enum Measured<T> { Measured(T), Unmeasured }`. **Serialization invariant is non-negotiable:** `Unmeasured` emits `{ "unmeasured": true }`, not `0`. Any `Measured::Unmeasured` field is dispositive of `Unknown` → fail-closed.
- `crates/plsql-depgraph/src/lib.rs:~540` `pub enum EdgeKind` — the four pillars of the Purity verdict: `Writes`, `WritesColumn` (550), `DerivesColumn` (551), `OpaqueDynamic` (557); plus `DbLink`, `TriggersOn`, `Constrains`, `WritesUnknownColumnOfTable`. `edge_kind_is_expected_gap` (1482) hard-codes `OpaqueDynamic|DbLink|Constrains|TriggersOn` — **parameterize** when its `cross_check_with_catalog` consumer moves to oraclemcp-audit.
- `crates/plsql-lineage/src/lib.rs:1608` `pub fn column_writers(graph: &DepGraph, column: &NodeSelector) -> ColumnAccessResult` — filters incoming `WritesColumn|WritesUnknownColumnOfTable|DerivesColumn` edges via `query_reverse_neighbors`; `is_unknown_column_of_table` distinguishes exact from table-level-unknown. `impact` (1365) and `unsafe_paths` (1771) aggregate confidence as **MIN along path**; an edge is unsafe iff `OpaqueDynamic` or `Confidence::Unknown` (1876). These are the reachability primitives the engine-side `SideEffectOracle` impl is built over.
- The guard's `SideEffectOracle` port **default impl returns `Unknown` (fail-closed)**; the engine binds the real impl from the consumer side (§5.3, keeps §0 hard rule 1 intact). The consult runs on `spawn_blocking`/`rayon`, never the async executor.

### CatalogSnapshot / analyze_project entry points (stay engine-side)
- `crates/plsql-engine/src/lib.rs` `pub fn analyze_project(req: AnalysisRequest) -> Result<AnalysisRun, EngineError>` — the only engine-pipeline invocation; reached via `analyze_project.rs:19`/`foundation_tools.rs:18`. `AnalysisRun` is a serializable snapshot (not live state), safe to pass to query handlers.
- `crates/plsql-catalog/src/lib.rs` `pub fn load_snapshot_from_connection<C: OracleConnection>(conn, request) -> Result<CatalogSnapshot, CatalogError>` (l.~1777) — orchestrates 15+ private `load_catalog_*` loaders; `negotiate_capabilities<C>` for feature probes; `load_snapshot_from_json`/`export_snapshot_to_json` (l.~1227) for persistence. The `<C: OracleConnection>` generic is the seam: MCP live-DB tools call these with their pooled connection impl; the snapshot model itself never moves. Plan §9.3: `oracle_schema_inspect(depth=full)` captures/refreshes the per-profile snapshot that engine tools consume.

---

## 6. Risks & unknowns for the build

1. **P0-CLK wall-clock token expiry (§5.10, P1-10) is a live fail-open today.** `safety.rs` + `preview.rs` expire on `SystemTime` u64 seconds with `saturating_sub`; a backward NTP/VM clock jump makes an expired write token read as fresh. The fix is **not** a mechanical `Instant` swap because tokens are `Serialize`/`Deserialize` and `Instant` is not serializable — needs a monotonic deadline anchored at issue **plus fail-closed rejection of tokens whose monotonic anchor is from a prior process generation.** No current mechanism detects skew (silent).

2. **Durable audit is the single biggest plan↔code gap (§5.13, P1-4).** Today's `audit.rs` has no out-of-band sink, no fsync, no hash chain, no pre-execution ordering, and the optional Oracle INSERT rides the audited transaction (ROLLBACK erases it). This is net-new code in oraclemcp-audit, and the `kill -9`-between-fsync-and-execute chaos test (§12) gates the compliance claims. Until built, no SOX/PCI language is legally supportable.

3. **`foundation_tools.rs` is the one mixed-coupling file.** Splitting `completeness_report` (engine) from `dynamic_sql_evidence`/`doc_lookup` (generic) across one `register_foundation_tools` function is the only non-mechanical Phase E cut. Mitigation: comment markers in Phase A.

4. **`OracleConnection` lift (P0-3) carries the highest test risk** in P0-0 — it relocates types out of `plsql-catalog` that 15+ loaders + 5 tool handlers depend on. Re-export shims keep tests green; live-XE tests must run after the move.

5. **`CatalogError` conflates DB errors with snapshot-versioning errors** (UnsupportedSchemaVersion, UnexpectedSchemaId). The recon proposes splitting into `DbError` (oraclemcp-db) + `SnapshotError` (engine-side). Unknown: whether downstream `#[from]` conversions make this split clean or churny — needs a spike.

6. **Byte-stability invariant** for token verification: `synthesise_ddl`/`synthesise_view_ddl` (patch.rs) and the preview SHA-256 binding require byte-exact, idempotent DDL generation (literal `\n`, `trim_start_matches('\n')`). Any reformatting during extraction breaks `verify_byte_for_byte`. `verify_byte_for_byte` does **string equality** on `ddl_bytes` (not hash compare) — strict and fragile to whitespace/CRLF/non-ASCII normalization.

7. **`ToolTier` / `RuntimeKind` / trust-block strings are plsql-flavored** ("Foundation", "foundation MCP server", `plsql.mcp.trust_block`). Cosmetic but must be renamed at Phase E; cross-check the drift-guard tests (`CAPABILITIES_CONTRACT_VERSION`, capabilities JSON) that pin these.

8. **`PreviewRegistry` is session-scoped, keyed by connection name** — if a new session reuses a connection name before the old session's registry is purged, a stale preview can be consumed (race). Caller must guarantee unique-per-session connection identity; `purge_expired` is not called automatically.

9. **No pooling/async/lease today** — P0-3/P0-4 add `r2d2-oracle` + `spawn_blocking` + the session-lease primitive (§5.1). The `OracleConnection: Send + Sync` bound supports it, but the lease/transaction-coherence machinery (`DBMS_OUTPUT`, savepoints, package state pinned to one physical session) is entirely net-new and is the #1 production blocker per §5.1.

10. **rmcp swap (P0-6) replaces `mcp_protocol.rs` + `tcp.rs` (~1060 LOC) wholesale.** The lockstep dispatch test is the safety net, but rmcp's `ToolRegistry`/Request serde shapes must stay compatible with the existing dispatch arms; HTTP/SSE rmcp advisories (DNS-rebinding, SSE-before-initialize, 401-behind-LB) must be tracked and the auth edge owned/wrapped (§2.6, R12).
