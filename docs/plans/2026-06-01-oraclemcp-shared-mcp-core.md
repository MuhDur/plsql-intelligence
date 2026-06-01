# oraclemcp — Production Plan

> **Status:** Planning — v2 (build/packaging strategy revised 2026-06-01; see §0 and §17). Self-contained: a fresh agent can implement from this document without external clarification.
> **Decision date:** 2026-05-31 (architecture); 2026-06-01 (build/packaging — improve in place now, extract later).
> **One-line:** An open-source, production-grade **Oracle Database MCP server in Rust** that any AI coding agent can connect to — safe by default, fast, observable, with **human step-up confirmation** gating destructive work (not device 2FA — §7.2) and a deep PL/SQL-intelligence toolset.

---

## How to read this document

This plan is ordered so that **the load-bearing decisions and the hard problems come first**, because they constrain everything else:

1. **§1 Vision & non-goals** — what we are and are not building.
2. **§2 Foundational decisions** — the six choices that must be made before any feature code (language, Oracle mode, license, safety posture, spec baseline, transports). Each is justified.
3. **§3 The language decision** — the adversarial duel result and why Rust won, with residual risks.
4. **§4 Architecture** — the workspace, the request lifecycle, the async/sync boundary.
5. **§5 The hard problems** — the thirteen correctness/safety problems that sink naïve DB-MCP servers, each with a concrete resolution. **This is the most important section.** Treat each resolution as an acceptance gate.
6. **§6–§11** — safety/security, auth & step-up confirmation, tool surface & ergonomics, feature set, observability/hardening, extensibility.
7. **§12 Testing strategy** — the gate that lets us call it "safe for production."
8. **§13 Build/packaging/licensing**, **§14 Module layout**, **§15 Phased roadmap (dependency-aware)**, **§16 Risk register**, **§17 Open questions for the maintainer**, **§18 Appendices** (pinned crate versions, Oracle dictionary reference, sources).

Every architectural choice carries a *why*. When implementing, if a local decision is ambiguous, the invariants in §5 win.

---

## §0 Build & Sharing Strategy — improve in place now, extract later (READ FIRST)

> **REVISED 2026-06-01.** This section supersedes the original "seed a new `oraclemcp` repo and depend back from day one" strategy. The decision below was reached after weighing reversibility, CI cost, and the safety-critical path with the maintainer. The three open questions this used to defer (shared-MCP architecture, commercial tier, naming) are now **resolved** — see §17.

**This is not a clean-room build, and it is not a repo split — yet.** A working MCP server already exists *in this repository* as the **`plsql-mcp`** crate, on top of a real PL/SQL-analysis engine (ANTLR parser → IR → dep-graph → lineage → SAST). `plsql-mcp` already implements ~70% of this plan's *safety spine* — independently converging on the same primitives: per-connection `permanently_read_only`, a single-use 60-second `enable_writes` token, a preview→approve flow with a connection-bound token, session-tagging audit (markers + an optional Oracle audit-table insert — **not** a durable out-of-band sink yet; §5.13), and the **same `oracle` 0.6.x thick/ODPI-C driver** this plan chose. What it lacks is exactly this plan's *upgrades*: it hand-rolls JSON-RPC (`mcp_protocol.rs` + `tcp.rs`, ~1060 LOC, no rmcp, spec 2025-06-18), is fully synchronous (stdio + plain TCP, no TLS/OAuth/auth), guards the query path with a **string predicate** (`is_read_only_sql`, `query.rs:367`) rather than a fail-closed, engine-aware classifier, and has no pooling/leases/HTTP/cloud.

**Strategy (decided): improve in place now; extract, publish, and depend back LATER — only when we are confident the MCP is great.** Two phases, with the split deliberately deferred behind a *quality trigger*, not a calendar date and not a phase boundary.

- **Phase A — improve in place (now).** All MCP work happens inside *this* `plsql-intelligence` workspace. **No new repository. No crates.io publish.** The MCP stays `crates/plsql-mcp`; the engine stays the `plsql-*` crates. We do the upgrades here — classifier, rmcp/async, ergonomics, and the rest of this plan — against the existing live-XE tests, in one workspace, one CI, one issue tracker. The only structural discipline we add now is a clean, one-way boundary (see *Hard rules*) so the eventual extraction is mechanical rather than a refactor.

- **Phase B — extract + publish + depend back (later, trigger-gated; the §15 roadmap calls this exact step **Phase E**).** When — and only when — we judge the MCP genuinely great (the §12 quality bar is met and we are happy with the tool surface and protocol), we lift the MCP and its generic, non-PL/SQL infrastructure out into a separate **`oraclemcp`** repository, publish the `oraclemcp` crate(s) to crates.io, and flip `plsql-intelligence` to consume the published crate instead of the in-workspace one. **The trigger is a quality judgment, not "end of Phase 1."**

- **Why this order (the reasoning that decided it).** The split-and-publish-back is a **one-way door**: crates.io names and semver promises are hard to walk back, and a prematurely-published core forces a publish → bump → coordinate cycle on *every* API change precisely while the API churns most (pre-1.0). Doing the same extraction *inside one workspace first* is a **two-way door** — Rust enforces "the core never depends on the engine" identically whether the crates live in one workspace or two repos (it is just a `Cargo.toml` that omits the engine), so we get 100% of the modularity with none of the coordination tax: **one** live-Oracle CI matrix instead of two, atomic cross-boundary refactors in a single PR, and — critically — the **safety-critical classifier swap lands in place immediately** instead of being stranded across a seed-and-flip seam while the old fail-open predicate keeps guarding writes. We pay the split cost once, at the end, when it buys something real (distribution + a clean public artifact), not up front when it only buys overhead.

- **Naming — the platform keeps its name; `oraclemcp` is the surface.** This repository/project remains **`plsql-intelligence`**: it is a PL/SQL *intelligence platform* (engine + analysis + CI/CD, of which the MCP is one surface), and naming the whole thing after one of its doors would mislabel it and box it in. **`oraclemcp` is the name of the MCP-server binary/crate** — the future published artifact, the `cargo install oraclemcp` command, and the string the MCP registries list. That surface owns the high-traffic "oracle mcp" search term while the platform keeps an honest, extensible name. **Market as MCP, build as platform.** (See §13 for how visibility actually accrues — to the runnable product and its registry listings, not to internal library crates.)

- **License — uniformly `Apache-2.0 OR MIT`, no moat.** The whole project is and stays permissively dual-licensed. There is **no commercial / source-available tier** (the former `plsql-mcp-pro` FSL idea is dropped). The eventual Phase-B split is therefore motivated only by modularity, distribution, and visibility — **never by a license seam.** (Resolved; §17.)

- **One full product binary (no "lite" variant).** The shipped **`oraclemcp` binary always includes the full engine** (parser → IR → dep-graph → lineage → SAST) — the engine is the differentiator and is **never** an optional build-out. The **only** feature toggle is **`live-db`** (the Oracle driver + Instant Client dependency): with it off you get a pure offline static-analysis server with zero native deps. Everything the engine offers is reachable through MCP tools. The `oraclemcp-*` core crates and the `plsql-*` engine crates (incl. `plsql-parser-antlr`) are *also* reusable libraries for embedders. There is **no** core-only "product" binary — an Oracle MCP stripped of its intelligence is a product nobody wants. (Full model in §13.1; client reach in §8.5.)
- **Hard rules (the discipline that keeps Phase B cheap):**
  1. **Dependency is one-way, enforced now.** The MCP/core depends on the engine; the engine never depends on the MCP. Engine intelligence reaches the MCP by the engine-side code implementing the MCP's `Tool`/registry contract — not by the MCP reaching into engine internals. A CI check (the MCP crate's `Cargo.toml` + a dependency lint) keeps this true.
  2. **Factor the generic core behind clean module/crate boundaries inside `plsql-mcp` now.** The non-PL/SQL pieces — protocol, the fail-closed guard/classifier, DB connection/pool/lease, audit, config — are the future `oraclemcp-*` crates. Keep them PL/SQL-engine-free and separable so Phase B is a `git filter-repo` + `cargo publish`, not a rewrite.
  3. **Critical-path order:** replace the string predicate with the fail-closed, **engine-aware** classifier FIRST (it guards real writes today — a fail-open footgun; §5.3), *then* the sync→async/rmcp migration (engine stays sync behind `spawn_blocking`), *then* ergonomics/tool-surface. All in place.
  4. **Don't publish until it's great.** Internal crates stay `publish = false`; the flip to published deps happens once, at the Phase-B trigger, after the §12 quality bar is met.

The rest of this document specifies the *target* design of the MCP and its core. Read every "new crate `oraclemcp-*`" reference as **"a cleanly-bounded module/crate inside `plsql-mcp` now, extractable to the `oraclemcp` repo at Phase B."** §14/§15 reflect this improve-in-place-then-extract strategy.

---

## §1 Vision, Goals, and Non-Goals

### 1.1 What oraclemcp is

A long-lived **MCP server** that exposes an Oracle database to AI coding agents through a small, well-designed tool surface. It is **harness-agnostic**: it speaks the Model Context Protocol over stdio and Streamable HTTP, so Claude Code, Codex, Gemini CLI, Hermes, Cursor, VS Code Agent Mode, and anything else that speaks MCP can use it without bespoke integration.

It is built for people who work in **large, real Oracle/PL-SQL codebases** — many packages, deep dependency graphs, and multiple environments (dev / test / prod / read-replica). The differentiating value is **PL/SQL intelligence**: not just "run a query," but "show me the call graph of this package," "what breaks if I change this view," "extract the canonical DDL," "explain why this query is slow," "search the source," — answered safely, on a connection that an agent can hold for a whole session.

### 1.2 Goals (the requirements, restated as acceptance criteria)

| Goal | Concretely means |
|------|------------------|
| **Full feature set** | Connection/session management, guarded query+DML+PL/SQL execution, schema introspection, PL/SQL static intelligence (call graphs, dependencies, DDL, compile errors), execution-plan analysis, source search. |
| **Open source** | Public repo, permissive license (§2.3), `SECURITY.md` + threat model, SBOM, reproducible builds, contributor docs. |
| **Stable** | Runs for weeks without degradation; memory-safe; graceful under listener drops, RAC failover, credential rotation. |
| **Extensible** | Internal trait-based tool registry; user-defined "virtual tools" via TOML config (**Phase 1**, §8.6 — SQL or full PL/SQL, classified fail-closed at load); optional sandboxed plugin boundary later (§11). |
| **Easy for any agent** | ≤12 **built-in** tools (operator virtual tools are additive — §8.6), flat input schemas, dual text+structured output, actionable errors with fix suggestions, a zero-arg `oracle_capabilities` entry point, trivial connection bootstrap via named profiles. |
| **Fast** | Pooled connections, statement cache, cursor pagination, `readOnlyHint` to skip client confirmations, sub-second introspection on cached schema. |
| **Safe for production** | **Read-only by default** with runtime-selectable operating levels (one user, all levels — §6.6), fail-closed classifier, real impact preview (execute-in-savepoint-rollback), durable audit, admission control, no silent data corruption (the type/NLS contract). |
| **Step-up confirmation** | Every level escalation requires a **human confirmation** — DCG-style prompt / selector options via MCP elicitation (in-band, no separate device). Device-based out-of-band 2FA is out of scope. OAuth 2.1 on HTTP + the full range of Oracle DB auth methods. |
| **Both transports + cloud (v1)** | stdio **and** Streamable HTTP(S) from day one (§2.6); connects to local/on-prem Oracle **and OCI / Oracle Cloud Autonomous DB** via wallet/mTLS + IAM token (§2.2, §9.1). |
| **Harness-agnostic & agent-friendly** | Works with any MCP client (Claude Code, Codex, Gemini, Cursor, …); safe-by-default exploration, escalation errors that tell the agent exactly how to proceed. |

### 1.3 Non-goals (v1.0)

- **Not** a general SQL IDE, a migration framework, or an ORM.
- **Not** a thin/pure-protocol Oracle driver — none exists in Rust/C, and writing one is out of scope (see §2.2; a long-term R&D note in §17).
- **Not** a multi-database tool — Oracle only. (Generic-DB ambitions dilute the PL/SQL-intelligence differentiator.)
- **Not** an AWR/ASH performance suite as a *headline* feature — those need a separately licensed Diagnostics Pack and DBA privilege; they are opportunistic, license-gated extras (§5.11, §9).
- **Not** a sandbox. The classifier reduces risk; it does **not** make destructive SQL impossible. The DB-level privilege model is the real boundary (§5.5).

---

## §2 Foundational Decisions (decide before any feature code)

The completeness critic's strongest point: several "P0 features" are not implementable until a handful of foundational choices are locked, because they cascade. These six are cheap to decide now and expensive to reverse.

### 2.1 Language: **Rust** ✅

Decided by an adversarial duel (§3). Summary: Rust is the only candidate with a first-party, spec-tracking MCP SDK; it gives memory safety for a long-lived server on untrusted input; it ships a single binary; it matches the maintainer's toolchain. Verdict was overdetermined — even the C and C++ advocates ranked Rust first.

### 2.2 Oracle connectivity mode: **Thick (ODPI-C + Oracle Instant Client)** ✅

This is the decision the critic flagged as "cascading through everything," so we make it explicitly and own its consequences.

- **There is no production-grade thin (pure-protocol) Oracle driver in Rust or C.** Thin mode exists in `python-oracledb`/`node-oracledb` only because those teams reimplemented the Oracle Net wire protocol in Python/JS; it is not extractable, and reimplementing it in Rust is a multi-year effort, explicitly out of scope.
- Therefore we use **thick mode**: Rust `oracle` crate → ODPI-C → `libclntsh` (Oracle Instant Client), `dlopen`-loaded at runtime.
- **Consequence — not hermetic:** the binary requires Oracle Instant Client (Basic or Basic Light) present at runtime. This is a *hard Oracle constraint, not a Rust one* — it is true for every thick driver in every language.
- **Consequence — glibc, not musl:** ODPI-C uses `dlopen`, which is incompatible with fully-static musl linking. Target `x86_64-unknown-linux-gnu`. Ship a Docker image with an Instant Client layer (≈100 MB with IC Basic Light, not the 15 MB a pure musl binary would be). *(Claims in some sources about "thin ODPI-C mode" or `ODPIC_BUILD_STATIC` statically linking the client are incorrect; do not design around them.)*
- **Consequence — this is actually an advantage for the auth + cloud requirements:** Kerberos, RADIUS/native-MFA, wallet/SEPS, proxy authentication, **and OCI / Oracle Cloud (Autonomous Database)** connectivity **all require thick mode**. Thin mode could satisfy neither the enterprise auth goal nor cloud connectivity.
- **OCI / Oracle Cloud is a first-class target:** thick-mode ODPI-C/Instant Client natively connects to OCI-hosted Oracle and **Autonomous Database** via the downloaded cloud wallet (mTLS, `cwallet.sso` + `TNS_ADMIN`) and/or **OCI IAM database tokens**. A connection profile just points at the wallet/alias or supplies an IAM token; no special code path. (See §7.3, §9.1.)
- **Minimum supported Oracle = 19c** ✅ (maintainer decision). Target Oracle DB **19c and up** (through 23ai/Autonomous); use an Instant Client ≥ 19c. Features needing newer servers (23ai JSON type, native MFA, some DRCP enhancements) are capability-gated (§5.11), never assumed. No 12.2/11g support.

### 2.3 Open-source license: **Apache-2.0 OR MIT** (dual) ✅

Decided to **match the rest of the `plsql-intelligence` workspace exactly** (which is `Apache-2.0 OR MIT`), because the MCP/core and the engine share code (§0) and must be license-compatible — and post-Phase-E the extracted `oraclemcp` crates and `plsql-intelligence` must stay compatible too. The dual is also the Rust ecosystem default, keeps Apache-2.0's patent grant available, is compatible with ODPI-C (UPL-1.0/Apache-2.0), and is compatible with dicklesworthstone's tools (DCG is MIT). Dual `Apache-2.0 OR MIT` it is.

### 2.4 Default safety posture: **read-only by default, but runtime-selectable operating levels (one user, all levels)** ✅

The server **starts read-only**, but a single connection/user can operate at **any level** — read-only, read-write, DDL, admin — selected and escalated *at runtime*, with every escalation gated and audited. This is the "one user, all levels" requirement, modeled as ordered **operating levels** (`READ_ONLY` → `READ_WRITE` → `DDL` → `ADMIN`); see the full model in **§6.6**.

Two deployment modes compose (effective capability = **DB-privilege ceiling ∩ session operating level**):
- **Hard-ceiling mode (strongest):** a least-privilege Oracle user makes higher levels impossible at the engine regardless of server state — use for shared/untrusted/prod-read connections.
- **All-levels mode (the requested flexibility):** one privileged user that *can* do everything; the server's operating-level + **step-up confirmation gate** (a DCG-style prompt / selector — §7.2) is the runtime boundary that keeps it read-only by default and requires explicit, audited, human-confirmed escalation to write/DDL/admin.

Fail-closed throughout: any statement the classifier cannot prove read-only is treated as mutating. **Honesty caveat (carried everywhere):** in all-levels mode the *server* is the boundary, which is weaker than a least-privilege DB user — `SET TRANSACTION READ ONLY` + the fail-closed classifier + the human step-up confirmation + durable audit are the defense-in-depth. We never describe the server as a "sandbox."

### 2.5 MCP spec baseline: **2025-11-25**, architected for **2026-07-28 statelessness** ✅

Implement against `2025-11-25` (Tasks, URL-mode elicitation, structured output, OAuth resource-server model). Do **not** depend on session-sticky routing or on `Roots`/`Sampling`/`Logging` primitives (deprecated with a 12-month window in the 2026 RC). Make the capability object serializable as a standalone document so the move to per-request `_meta` is cheap.

### 2.6 Transports: **stdio AND Streamable HTTP(S) from day one (both in v1)** ✅

Both transports ship in v1 (maintainer requirement). **stdio** is the lowest-attack-surface, works-with-every-harness local path (auth = OS process boundary + init token). **Streamable HTTP(S)** (axum-based, via `rmcp`) is the remote/multi-agent/containerized path and brings its full security stack with it from the start: TLS, OAuth 2.1 resource-server validation, mTLS, origin checks, and the step-up confirmation gate (§7). Because HTTP is day-one, its hardening is **not** deferred — we pin `rmcp`, own/wrap the auth edge ourselves rather than trusting the transport with security decisions, track `rmcp`'s HTTP/SSE advisories, and use the poll/Task pattern for the step-up gate so we never depend on a long-held request (§7.2). **No legacy HTTP+SSE** (deprecated; hard-removal mid-2026).

---

## §3 The Language Decision, in Depth

A "dueling-idea-wizards" workflow ran three advocates (C, C++, Rust), an adversarial cross-scoring round (each advocate critically scores the rivals), and two neutral judges. Scores (0–1000):

| Source | C | C++ | Rust |
|--------|---|-----|------|
| Self-score (each advocate on its own language) | 285 | 520 | 820 |
| C advocate scoring rivals | — | 430 | **760** |
| C++ advocate scoring rivals | 260 | — | **790** |
| Rust advocate scoring rivals | 210 | 390 | — |
| Neutral Judge #1 | 250 | 415 | **805** |
| Neutral Judge #2 | 250 | 415 | **815** |

**The decision is overdetermined** — both rival advocates ranked Rust first against their own pick. Three convergent, load-bearing facts:

1. **MCP SDK reality.** Rust has `rmcp` (official, `modelcontextprotocol/rust-sdk`, v1.7.x, Tokio-based, both stdio and Streamable HTTP transports, `#[tool]` macros, `schemars` schema generation, OAuth utilities). The best C++ options (`mcp-cpp` 0.1-beta, `fastmcpp`, `cpp-mcp`) are prototype-grade and/or spec-frozen; C has no SDK at all. Hand-rolling JSON-RPC 2.0 + transports + every primitive is **4–8 engineer-weeks (C++) / 12–20 weeks (C)** of protocol plumbing that must be re-diffed against every spec revision forever.
2. **Memory safety.** A server that agents leave running for days, handling effectively-untrusted SQL strings across concurrent sessions, is exactly the workload where C/C++ use-after-free and data races cause slow degradation and crashes. Rust eliminates that class at compile time; the only unsafe surface is the bounded ODPI-C FFI.
3. **Distribution + maintainer fit.** Single `cargo`-built binary, `cargo-dist` release pipelines, `cargo audit`/`cargo deny` supply-chain gates — matching the maintainer's existing static-binary CLI workflow.

### 3.1 Residual risks of the Rust choice, and mitigations

| Risk | Mitigation |
|------|-----------|
| `oracle` crate is **synchronous**; blocking the Tokio executor would stall all sessions. | All DB I/O goes through `tokio::task::spawn_blocking` with an explicit, observable boundary; never hold an `oracle::Connection` across `.await` (compiler-enforced via ownership). Pool with `r2d2-oracle`. Cap concurrent blocking work with a `Semaphore` so the 512-thread blocking pool can't be exhausted (§5.6). |
| `oracle` 0.6.x tracks **ODPI-C 5.4.x**, behind ODPI-C 6.0.0 (2026-05-04). Some advanced features (pool session callbacks, `dpiConn_setAction`, AQ, TAC callbacks) aren't exposed. | Use the `oracle` crate for the 95% path; keep an `odpic-sys` escape hatch crate for advanced ODPI-C surface. Apply login scripts via the pool's connection-customizer hook (run `ALTER SESSION` on checkout) rather than relying on `plsqlFixupCallback`. |
| `rmcp` HTTP/SSE transport has had active bugfixes (DNS-rebinding, SSE-before-initialize, 401 behind load balancers). | **stdio-first *internally*** (1a stdio core → 1b HTTP on the same spine, §15) — but HTTP is **day-one/v1** (maintainer requirement), so its hardening is **front-loaded, not deferred**: pin `rmcp` in `Cargo.lock`, vendor if needed, own/wrap the auth edge, poll/Task for the gate, track the advisories, contribute fixes upstream (§2.6, R12). |
| Contributor onboarding (borrow checker, async/sync boundary) is harder for Oracle/PL-SQL specialists. | A `CONTRIBUTING.md` that explicitly teaches the `spawn_blocking` Oracle pattern; `thiserror` for legible errors; heavy doc-comments; a thin-bin-crate / fat-lib-crate workspace split (for compile speed) + `sccache` + `cargo-nextest` to keep the feedback loop fast. |

---

## §4 High-Level Architecture

### 4.1 The stack

```
┌──────────────────────────────────────────────────────────────────┐
│  AI agent (Claude Code / Codex / Gemini / Hermes / Cursor / ...)   │
└───────────────▲───────────────────────────────────────────────────┘
                │ MCP (JSON-RPC 2.0)
        ┌───────┴────────┐                ┌──────────────────────────┐
        │  stdio (v1)    │   or           │  Streamable HTTP(S) (v1)  │
        │  init-token    │                │  TLS + OAuth2 RS + mTLS   │
        └───────┬────────┘                └────────────┬─────────────┘
                └──────────────┬───────────────────────┘
                               ▼
                    ┌─────────────────────┐    rmcp ServerHandler
                    │  MCP layer (rmcp)    │    #[tool] registry, capabilities,
                    │  tools/resources/    │    progress, cancellation, tasks
                    │  prompts/tasks       │
                    └──────────┬──────────┘
                               ▼  tower middleware chain (async, pure-CPU)
   ┌───────────────────────────────────────────────────────────────────┐
   │  Admission/rate-limit → AuthZ/scope → DCG classifier → danger level  │
   │  → Step-up gate (required vs current) → Audit (durable, pre-execute) │
   └──────────────────────────────┬────────────────────────────────────┘
                                   ▼  spawn_blocking boundary  (the ONLY place DB I/O happens)
                    ┌─────────────────────────────────┐
                    │  Session/Lease manager          │  pins one physical session per
                    │  r2d2-oracle pool               │  stateful unit of work (§5.1)
                    │  login-script executor          │
                    │  type-mapping + NLS serializer  │  (§5.2)
                    └──────────────┬──────────────────┘
                                   ▼
                    oracle crate → ODPI-C 5.4.x → libclntsh (Instant Client) → Oracle DB
```

### 4.2 Request lifecycle (a single `tools/call`)

1. **Transport** decodes the JSON-RPC request (`rmcp`).
2. **Admission control** checks global + per-agent concurrency; returns a structured `BUSY, retry-after` if over budget (§5.6).
3. **AuthZ** validates the bearer token / stdio init-token and the required scope for this tool.
4. **DCG classifier** parses & classifies the SQL/PL-SQL, fail-closed (§5.3) → **danger level + required operating level**. On block, return `isError:true` with a safe alternative. *(Classification must precede the gate: you need the required level before you can compare it to the session's current level — §6.6.)*
5. **Step-up gate**: if the required level (from step 4) exceeds the session's current level and no valid approval is present, return `CHALLENGE_REQUIRED` and request human confirmation (selector via elicitation; §7.2).
6. **Audit** writes a durable pre-execution record (§6.4).
7. **spawn_blocking**: acquire a leased session (§5.1), run the login script if new, execute under the appropriate transaction mode, fetch rows through the type/NLS serializer (§5.2) with row/byte caps and cursor pagination.
8. **Response**: dual `content` (human/LLM text) + `structuredContent` (machine JSON) per `outputSchema`; audit the outcome.

### 4.3 Concurrency model

- Tokio multi-thread runtime; each MCP session is an async task.
- **One invariant above all:** an `oracle::Connection` is never held across an `.await`. It enters and leaves a `spawn_blocking` closure by ownership.
- CPU-bound PL/SQL analysis (graph building, classification) runs in `spawn_blocking` or `rayon`, never on the async executor.
- Backpressure via a global `Semaphore` sized to the pool, plus per-agent limits (§5.6).
- **Multi-agent topology (N clients at once — e.g. Codex + two Claude Codes + Hermes).** Over **stdio**, each client *spawns its own server process*, so that example is **four independent processes** — each with its own pool, in-process memory, leases, and audit; they cannot interfere (cost: N× connections + Instant-Client memory). Over **HTTP**, many agents share **one** server process, and isolation is then enforced *in-process*: a per-agent **session-lease** pins each stateful unit of work to one physical Oracle session (§5.1 — no cross-agent transaction/savepoint bleed), **per-agent admission caps** stop one agent starving the pool / `ORA-12519` (§5.6), and every record is tagged with per-agent identity (§6.4). Either way agents are isolated — stdio by OS process, HTTP by lease + admission + identity.

---

## §5 The Hard Problems and Their Resolutions

These are the problems that separate a demo from a production tool. **Each resolution is an acceptance gate**: code review and the test suite must enforce it. Most were surfaced by an adversarial completeness critic of the feature set.

### 5.1 Session-state coherence — the **session-lease primitive** (the #1 blocker)

**Problem.** A connection pool hands out a *different physical Oracle session per checkout*. But transactions, savepoints, `DBMS_OUTPUT`, temporary tables, package global state, and login-script-installed session settings are **all session-scoped**. If `begin_transaction()` runs on call 1 and `commit()` on call 2, they may land on different sessions — silent corruption that appears only under concurrency and is nearly undebuggable in the field.

**Resolution — first-class session leases.**

- A **lease** = one physical Oracle session pinned to one agent for a unit of work. Tools:
  - `oracle_session.acquire_lease(profile, ttl_seconds) -> lease_id`
  - `oracle_session.release_lease(lease_id)`
  - every stateful tool (transaction control, DBMS_OUTPUT capture, temp-table workflows, savepoint preview) takes an optional `lease_id`; **if omitted, the operation runs in autocommit-off-single-statement mode and any attempt to open a transaction/savepoint without a lease is a structured error**, never a silent best-effort.
- Leases have a **TTL on a monotonic clock**; on expiry the manager **rolls back the open transaction, clears session state, and returns the session to the pool**, emitting an audit event. Lease renewal at 75% TTL.
- Stateless reads (the common case) **do not need a lease** — they check out, run, check in.
- On `acquire_lease`, run the profile's login script (`ALTER SESSION ...`) and stamp `DBMS_APPLICATION_INFO` module/action with the agent identity.

This is **Phase 0** — nothing involving transactions, savepoints, `DBMS_OUTPUT`, temp tables, or DRCP `purity=SELF` is correct without it.

### 5.2 Value fidelity — the **type-mapping & NLS contract**

**Problem.** Silent wrong numbers are the worst possible outcome for a data tool, and they happen by default: Oracle `NUMBER(38)` overflows IEEE-754 `f64`; dates/decimals render differently under different `NLS_LANG`; many real-schema types (INTERVAL, TIMESTAMP WITH LOCAL TIME ZONE, RAW, ROWID, BINARY_DOUBLE/FLOAT, NCLOB/NVARCHAR2, object types/VARRAY/nested tables, REF CURSOR as OUT bind, ANYDATA, XMLTYPE, 23ai JSON, SDO_GEOMETRY, BFILE) have no obvious JSON mapping.

**Resolution — a documented, deterministic contract (Phase 0).**

- **NUMBER and any numeric with > 15 significant digits → serialize as a JSON string** by default (`"1234567890123456789"`), with an opt-in `numbers_as_float` flag for callers who accept precision loss. This default is non-negotiable.
- **Canonical output serialization, decoupled from query-semantics NLS:** dates/timestamps → **ISO-8601**; decimals → **period decimal**; text → **UTF-8**. The session NLS used to *interpret* the query is configurable per profile; the *output* is always canonical, so identical queries return identical values regardless of host `NLS_LANG`/CI locale.
- A published **type table**: every Oracle type → its JSON representation, or `{"unsupported": "<type>", "value": null, "warning": "..."}` for types we explicitly don't serialize yet. No silent best-effort.
- LOBs (CLOB/BLOB/NCLOB) are **streamed/paginated**, never inlined whole; BLOB defaults to base64 with a size cap and a "fetch by range" tool.

### 5.3 The **fail-closed statement classifier** (DCG core)

**Problem.** The entire safety story rests on classifying SQL/PL-SQL. `sqlparser-rs` (`sqlparser` crate, OracleDialect) parses SQL-92+ but **does not parse PL/SQL** (anonymous/named blocks, package bodies). A classifier that *fails open* on something it can't parse is worse than none — it creates false confidence, and one false negative on a `readonly=true` server means an agent deletes production data.

**Resolution — a staged, fail-closed classifier (two syntactic stages + an engine-aware purity consult), with an adversarial corpus.**

- **Stage A (fast pre-filter):** config allow-list (SHA-256 of normalized text) → config block-list (regex) → PL/SQL block detector (input starting with `DECLARE`/`BEGIN`/`/`, or containing `EXECUTE IMMEDIATE`/`DBMS_SQL`/`UTL_FILE`/`UTL_HTTP`/`DBMS_SCHEDULER`).
- **Stage B (AST):** for pure SQL, parse with `sqlparser` OracleDialect → map the `Statement` variant to a `DangerLevel`. For DELETE/UPDATE, inspect the `selection` field — **`None` (no WHERE) escalates to Destructive**.
- **`DangerLevel`:** `Safe` (SELECT with no unproven function call, WITH…SELECT, DBMS_OUTPUT.PUT_LINE) · `Guarded` (INSERT, UPDATE/DELETE with WHERE, MERGE, CTAS, **`SELECT … FOR UPDATE`, LOCK TABLE, standalone COMMIT/ROLLBACK/SAVEPOINT, CALL, non-allowlisted ALTER SESSION, EXPLAIN PLAN** — it writes `PLAN_TABLE`, see below) · `Destructive` (DROP, TRUNCATE, DELETE/UPDATE without WHERE, MERGE with a DELETE clause, GRANT/REVOKE, **SET ROLE**, ALTER USER/SYSTEM, CREATE OR REPLACE on existing) · `Forbidden` (dynamic SQL via string concat, `UTL_FILE` write, outbound network, unconditional DDL inside PL/SQL).
  - **`EXPLAIN PLAN` is NOT `Safe`** (resolves the contradiction with §5.4/§5.8: it writes `PLAN_TABLE`). It is `Guarded`, blocked on `read_only_standby`, and exposed only via the dedicated `oracle_explain_plan` tool — never as a `Safe` passthrough in `oracle_query`.
- **Fail-closed law:** *any* statement that does not parse, **or any PL/SQL block at all**, is classified at minimum `Guarded` and, if it cannot be proven side-effect-free, `Destructive`/`Forbidden`. PL/SQL blocks get the regex side-effect scanner; unknown = unsafe.
- **Multi-statement splitting** uses a **lexer-based** depth state machine, not a character scan: it tokenizes first so `;`, `BEGIN`, `END`, `CASE` inside string literals (`'…'`, `q'[…]'`, `N'…'`) and quoted identifiers (`"…"`) are never counted (the current `is_read_only_sql`/`has_trailing_non_empty_statement` are literal-blind — a crafted `q'{ … END; … }'` can desync the `BEGIN`/`END` counter and hide a boundary, a fail-open direction). A `;` at depth 0 is a boundary; if the lexer cannot reach a balanced state (depth never returns to 0, or goes negative), the **entire batch is `Forbidden`** (fail-closed on desync), never best-effort split. If *any* sub-statement is `Forbidden`, the whole batch is rejected.
- **Engine-aware side-effect resolution (this project's edge — but it must *prove purity*, never *assume* it).** A pure-syntax classifier (`sqlparser` AST + regex) has a blind spot live in today's code: `is_read_only_sql` (`query.rs:367`) accepts `SELECT billing.purge_old_rows() FROM dual` as read-only, yet `purge_old_rows` may do DML — including via `EXECUTE IMMEDIATE` / `PRAGMA AUTONOMOUS_TRANSACTION`. This project's PL/SQL engine (IR + call/dep-graph) can see that — **but only if the consult is framed as proof-of-purity, not detection-of-writes.** The engine's own model defaults completeness signals (`dynamic_sql_sites`, `opaque_dynamic_sql_sites`, `unresolved_references`) to `plsql_core::Measured::Unmeasured` and represents `EXECUTE IMMEDIATE` as an `OpaqueDynamic` dep-graph edge, so a naïve *"no `Writes` edge reachable → Safe"* query is **fail-open** under exactly the `EXECUTE IMMEDIATE 'DELETE …'` case this feature is sold to catch. The classifier therefore consumes a **three-valued `Purity` verdict** and may clear a statement to `Safe` **only** on an explicit `ProvenReadOnly`:
  - `ProvenReadOnly` — body fully loaded and parsed clean; this routine and every transitively-reachable routine have all completeness signals `Measured(0)`; **and** no `Writes`/DDL/`OpaqueDynamic`/`DbLink`/`TriggersOn` edge is reachable. *Only this verdict permits `Safe`.*
  - `ProvenSideEffecting` — a reachable write/DDL/autonomous-transaction edge → escalate to ≥ `Guarded`.
  - `Unknown` (**the default**) — body not loaded, parse-recovered, **any** `Measured::Unmeasured` field, **any** reachable `OpaqueDynamic`/dynamic-SQL edge, or a recursion/cycle/traversal-budget hit → **treated as `ProvenSideEffecting` (fail-closed).**
  - **Law:** the consult may only *raise* danger or *clear to `Safe` via an explicit `ProvenReadOnly`*. Absence of a write edge is `Unknown`, never `Safe`; `Measured::Unmeasured` is dispositive of `Unknown`. A `SELECT` that calls any user-defined function is `Guarded` unless that function is `ProvenReadOnly`.
  - **Trigger / VPD walk (the statement is not the only actor).** Resolve the statement's base objects — and for views their underlying tables and projected function columns — then walk `TriggersOn` and known policy/VPD (`DBMS_RLS`) function attachments: a `SELECT` or DML can fire a side-effecting trigger or row-level-security function the statement text never names. Any that is not `ProvenReadOnly` forces `Unknown` → ≥ `Guarded`, *including for `SELECT`*. (This is why §6.3's `SET TRANSACTION READ ONLY` is **not** a sufficient backstop: an `AUTONOMOUS_TRANSACTION` trigger commits independently and raises no `ORA-01456`.)
  - **Boundary + placement (keeps §0 hard rule 1 intact).** The classifier lives in the engine-free guard core and depends only on a narrow `SideEffectOracle` port trait whose **default impl returns `Unknown` (fail-closed)** — so the guard ships fully functional with no engine dependency, and the safety-critical swap (P1-1) is **not** blocked on wiring engine reachability. The engine binds the real implementation (over `DepGraph` / `plsql-lineage::column_writers`) from the *consumer* side, exactly like every other engine `Tool` impl. The consult runs on a `spawn_blocking`/`rayon` worker, never the async executor (§4.3).
  - This makes the classifier strictly safer than any off-the-shelf Oracle-MCP server and is the real differentiator — but it is *additive enrichment on a whole fail-closed syntactic core*, landed after the core swap, never a relaxation of it.
- The classifier ships only with a **differential adversarial corpus** (comment-hidden DML, CTE-wrapped DML, MERGE, FORALL, AUTONOMOUS_TRANSACTION, side-effecting function calls inside SELECT, hint-changed semantics, multi-statement payloads) and fuzz tests (§12). **It is never described as a sandbox.**

### 5.4 Impact preview — **execute-in-savepoint-then-rollback**, not EXPLAIN-PLAN cardinality

**Problem.** A dry-run that trusts optimizer cardinality is dangerously misleading: on stale stats, `EXPLAIN PLAN` reports "estimated_rows: 12" for a DELETE that removes 4 million rows. The agent shows that to a human, gets approval, and causes exactly the catastrophe the gate was meant to prevent. Worse, `EXPLAIN PLAN` itself **writes to `PLAN_TABLE`**, so it fails on a read-only physical standby and needs a private PLAN_TABLE privilege.

**Resolution — real preview as the impact gate (Phase 2).**

- **Preview is the `oracle_query` preview mode, not a separate tool** (keeps the surface ≤12): on a DML/DDL statement with a `lease_id`, inside an autonomous savepoint on the leased session, **actually execute** the statement, capture `SQL%ROWCOUNT` and a sample of affected rows, then **`ROLLBACK TO SAVEPOINT`** unconditionally — returning the impact plus a single-use token that `oracle_query_execute` consumes. This yields *ground truth* blast radius, not a guess.
- `EXPLAIN PLAN` / `DBMS_XPLAN` remain available as a **plan-analysis** tool (`oracle_explain_plan`) for performance work — but they are explicitly **not** the safety impact gate, and `oracle_explain_plan` is disabled on profiles flagged `read_only_standby` (§5.8) or downgraded to `DISPLAY_CURSOR` against existing cursors.
- For pure reads, an optional `SELECT COUNT(*) WHERE <predicate>` gives exact pre-counts cheaply.

### 5.5 The **allow-once token is friction, not a security control**

**Problem.** A DCG-style allow-once token that the *agent* echoes back is self-authorized. The agent is the untrusted party (LLM-generated code); a compromised or merely overeager agent issues its own token. Marketing this alongside real 2FA as if it were a safety boundary is dangerous.

**Resolution — honest enforcement layers, in order of strength.** (These back the operating-level model in §6.6.)

1. **DB-level privilege ceiling (the real, hard boundary).** A statement can never exceed the connecting user's actual Oracle grants. In **hard-ceiling mode** the user is least-privileged (`CREATE SESSION` + `SELECT`-only/object grants), so write/DDL/admin are impossible at the engine no matter what any layer above decides. In **all-levels mode** the user is privileged and this ceiling is high — so the boundary moves *up* to the server layer (weaker; documented).
2. **Operating level + `SET TRANSACTION READ ONLY` (server boundary).** The session's current level (default `READ_ONLY`) is enforced by the fail-closed classifier, and `SET TRANSACTION READ ONLY` is issued whenever the level is `READ_ONLY` so even a misclassification of *direct* DML raises `ORA-01456` at the engine. (It does **not** stop `AUTONOMOUS_TRANSACTION` side-effects fired by triggers/VPD functions — those commit independently; the §5.3 trigger/VPD walk is the defense there, and on a `protected` profile layer 1's least-privilege user is the real boundary.)
3. **Step-up confirmation gate (the real boundary for *elevation*).** Crossing from a lower to a higher operating level (read→write→DDL→admin) requires a **human confirmation** — a DCG-style prompt / selector via MCP elicitation (§7.2). No escalation happens without it. (Device-based out-of-band 2FA is out of scope.)

The **allow-once token** sits *below* these — it is UX friction + an audit artifact (the agent took a deliberate second step), explicitly documented as **NOT a security boundary** (the agent self-issues it). Docs and tool descriptions state this plainly so operators never mistake convenience for control.

### 5.6 **Admission control & backpressure**

**Problem.** A fixed pool + N agents × M concurrent calls = pool starvation and `ORA-12519`/`ORA-00018` against a shared production DB — the most likely way the server itself causes an incident.

**Resolution (Phase 2).** Global concurrency cap = pool `max_size`, enforced by a `tokio::sync::Semaphore`; **per-agent** caps on top; a bounded **fair queue**; when over budget, return a structured `{ "error": "BUSY", "retry_after_ms": N }` *before* touching the pool. HTTP path additionally uses `tower-governor` (GCRA) keyed by agent identity. Never let the blocking-thread pool (default 512) be the limiter — the semaphore is.

### 5.7 **Cancellation & crash-safety correctness**

**Problem.** Agents abort and retry constantly. Without a clean cancel-and-cleanup path you leak cursors, sessions, and row locks, and you risk **double-executing DML on retry**. On SIGTERM/SIGKILL, in-flight transactions and held leases can strand locks.

**Resolution.**
- **MCP cancel** (`notifications/cancelled` / `tasks/cancel`) → `conn.break_execution()` (OCI break) → **rollback any open transaction on the leased session** → close cursors → return a deterministic `{ "can_retry": bool }`. **DML is never auto-retried** (only transient connection errors are — §10).
- **Graceful shutdown:** SIGTERM sets a `CancellationToken`, fails `/readyz`, stops accepting new work, rolls back in-flight transactions, revokes leases and Vault leases, drains the pool with a deadline, flushes the audit + OTel exporters, then exits.
- **Crash:** `panic = "abort"`, panic hook logs through `tracing` first; systemd `Restart=on-failure`; on restart, any session that held locks is already gone (Oracle rolls back killed sessions), but the audit log records the gap.

### 5.8 **Read-replica / standby awareness**

**Problem.** Read-replica environments typically mean Active Data Guard physical standbys, which are *physically* read-only and reject even `EXPLAIN PLAN` (it writes to `PLAN_TABLE`). A classifier that assumes "EXPLAIN PLAN is safe/read-only" is wrong there.

**Resolution.** A profile flag `read_only_standby: bool` (auto-detected at connect via `SELECT database_role, open_mode FROM v$database`). On a standby: disable all write paths and `EXPLAIN PLAN`-into-PLAN_TABLE; route plan analysis to `DBMS_XPLAN.DISPLAY_CURSOR`; expose the standby status in `oracle_capabilities`.

### 5.9 **Config: one validated schema, defined precedence, atomic reload**

**Problem.** Multiple overlapping config surfaces (profiles, virtual tools, transport.auth, rbac, secrets, stepup) with ad-hoc `SIGHUP` reload and no defined precedence is a top operational/security footgun.

**Resolution.** A single `figment`-layered config with **strict precedence: built-in defaults < `config.toml` < env (`ORACLEMCP_*`) < CLI flags**, deserialized into one validated, versioned struct (reject unknown keys; validate at startup, fail fast). Hot-reload via `notify` is allowed only for **non-credential, non-security** sections, applied atomically through `Arc<ArcSwap<Config>>`; credential/auth/RBAC changes **require restart**. The config struct carries a `schema_version` for upgrade migrations.

### 5.10 **Clock & monotonic-time discipline**

**Problem.** TOTP (±30s), challenge/lease/cursor TTLs, audit timestamps, and credential-lease renewal all depend on accurate, monotonic time; wall-clock jumps silently break or bypass security features.

**Resolution.** All TTLs use `tokio::time::Instant` (monotonic), never wall clock. Audit timestamps are RFC-3339 from the system clock **plus** a monotonic sequence number for ordering — the **sequence number, not the wall timestamp, is the authoritative order key** for the hash chain, so a clock jump cannot reorder or collide entries. The server documents that it **requires NTP**, logs detected clock skew between MCP host and Oracle (`SELECT systimestamp`), and refuses to validate TOTP if skew exceeds a threshold.

> **Migration task (the current code violates this invariant).** `safety.rs` (`EnableWritesToken::is_expired`) and `preview.rs` both expire tokens on **wall-clock `SystemTime` seconds** — a backward NTP/VM correction makes `now.saturating_sub(issued_at)` clamp to 0, so an expired write token reads as fresh and the write window silently extends. This is **not** a mechanical swap: these tokens are `Serialize`/`Deserialize` and `Instant` is not serializable. Convert expiry to a monotonic deadline anchored at issue time; **reject (fail-closed) any deserialized token whose monotonic anchor is from a prior process generation.** Tracked under P1-10.

### 5.11 **Privilege graceful-degradation matrix**

**Problem.** Many flagship features need privileges (`SELECT ANY DICTIONARY`, `DBA_*`, PL/Scope settings, `DBMS_FGA`) or a licensed Diagnostics Pack that real least-privilege service accounts won't have. Without a plan, these silently return empty or error.

**Resolution.** A single source-of-truth table mapping **every tool → required Oracle privileges/license → documented degraded behavior** (e.g., fall back from `DBA_*` to `ALL_*` to `USER_*`; disable AWR tools when `control_management_pack_access = NONE` and offer Statspack; return a clear "insufficient privilege: needs X" structured error, never an empty success). `oracle_capabilities` reports which tiers are actually available on the connected account. At startup, probe `V$OPTION`/`V$PARAMETER`/dictionary access and cache the privilege profile.

### 5.12 **Plugin safety — no in-process native plugins**

**Problem.** A `.so` loaded into the server process has full process access; it bypasses DCG/RBAC/audit/Zeroize regardless of any "sandboxed handle" API, and a buggy one crashes the long-lived server.

**Resolution.** Extensibility ships in this order (§11): (1) internal trait-based tool registry (compile-time); (2) **config-driven "virtual tools"** — TOML-defined named SQL/PL-SQL with typed params, which run through the *same* DCG/audit pipeline (**Phase 1; §8.6**); (3) only later, an **out-of-process / WASM** plugin boundary with a capability-scoped API. **Never** in-process `dlopen` of third-party native plugins.

### 5.13 **Compliance honesty — durable audit**

**Problem.** Calling the audit log "immutable/tamper-evident for SOX/PCI" while also accepting "at-most-once log loss on crash" is a contradiction that creates legal exposure for adopters. A hash chain proves *ordering*, not *durability*.

**Resolution.** The "production" security profile uses **fsync-before-execute** for the durable audit sink (a statement is logged and flushed *before* it runs), plus an optional hash chain for tamper-evidence, with **Oracle Unified Auditing** as the authoritative system of record (a per-MCP-user policy → `UNIFIED_AUDIT_TRAIL`). Compliance language is only used where this guarantee holds; non-production profiles may use a buffered sink and say so. Audit entries store SQL **SHA-256 + truncated preview**, never bind values or secrets.

> **Status: NOT YET IMPLEMENTED — this is the single biggest gap between plan and code.** Today's `audit.rs` provides only session tagging (`DBMS_APPLICATION_INFO`), a SQL comment marker, and an *optional* INSERT into a customer Oracle table. There is **no out-of-band durable sink, no fsync, no hash chain, no pre-execution write ordering.** Two design corrections this exposes: **(1) the durable sink MUST be out-of-band** (a local append-only file or SQLite), **never the Oracle session that runs the audited statement** — an INSERT on that connection shares the statement's transaction, so any `ROLLBACK` (the §5.4 savepoint preview, the §5.7 cancel-rollback, or any error) erases the audit row, violating "logged before it runs." **(2) fsync-before-execute is mandatory only for `Guarded`/`Destructive`/escalation calls;** pure `READ_ONLY` reads may use a batched/group-commit fsync buffer (documented as such) so the read hot path does not serialize on `fsync` (1–10 ms each). Add a §12 test: `kill -9` between audit-fsync and execute; assert the log contains the statement and the DB does not (at-least-once log, at-most-once execute).

---

## §6 Safety & Security Architecture

### 6.1 The DCG pipeline (server-side, invisible to the agent)

Adapted from DCG's 7-step model (config-allow → config-block → heredoc/PL-SQL scan → quick-reject → parse+classify → normalize+scope-check → emit decision). Implemented as a `tower`-style chain of `Box<dyn StatementEvaluator>` so steps compose and are independently testable. Output: `GuardDecision { action: Allow|RequireToken|RequireStepUp|Block, danger_level, objects_affected, safe_alternative, audit_record }`.

### 6.2 Per-schema scoping & allow/deny lists

TOML policy per schema: `default_mode = read_only|guarded|permissive`, `allow_dml`, `deny_ddl`, `deny_patterns`, `deny_all`. Schema extracted from the parsed `ObjectName` or `SYS_CONTEXT('USERENV','CURRENT_SCHEMA')`. `SYSTEM`/`SYS`/`SYSAUX` are deny-all by default and cannot be unlocked by an allow-once token.

### 6.3 Read-only enforcement — three complementary layers

(A) DB-level privilege ceiling (strongest); (B) `SET TRANSACTION READ ONLY` whenever the session level is `READ_ONLY`; (C) guard-layer pre-execution classification + audit. Use all three; A is the hard boundary, but in all-levels mode (§6.6) the connecting user is privileged so B+C become the operative boundary (§5.5). **Caveat on layer B (do not over-claim it):** `SET TRANSACTION READ ONLY` blocks *direct* DML in the calling transaction but **does not** stop `PRAGMA AUTONOMOUS_TRANSACTION` side-effects fired by triggers or VPD/policy functions — those commit independently and raise no `ORA-01456`. The classifier's trigger/VPD walk (§5.3) is the only defense against those, and on a `protected` profile the least-privilege DB user (layer A) is the real boundary. Recommended minimal grants for a hard-ceiling read-only profile: `CREATE SESSION`, `SELECT` on the needed `DBA_*`/`ALL_*` views (or `SELECT ANY DICTIONARY` if acceptable), object `SELECT` grants, **never** `DBA`/`RESOURCE`/`DROP ANY`.

### 6.4 Audit

`tracing` JSON to stderr for operational logs (never bind values); a separate **durable** audit sink (file or SQLite) with fsync-before-execute in the production profile (§5.13); optional Oracle Unified Auditing policy as system of record. Every tool call records: timestamp+seq, agent/session identity, tool, SQL SHA-256 + preview, danger level, decision, step-up token id (hashed), rows affected, outcome.

### 6.5 Secrets

`keyring` (OS keychain) for dev/single-node; `vaultrs` (Vault/OpenBao KV v2 via AppRole) for production, with dynamic Oracle credentials + `SIGHUP` re-fetch where available. **Default-deny plaintext passwords under the production profile** (hard startup error). End-to-end `zeroize` discipline: no `String` clones of secrets through FFI, log formatters, or error messages. Documented fail-closed fallback ordering (never silently degrade to env vars in production). **Login / house-convention scripts (recommended design).** These are **operator-supplied configuration, never shipped in the repo** — the repo ships only a commented *example* profile template. Three layers, composing in order:
1. **Profile baseline (operator):** a profile points at a `login_script = "<path>.sql"` (a file of `ALTER SESSION …`, `SET ROLE …`, optimizer/NLS/time-zone settings) **or** inline `login_statements = ["ALTER SESSION SET …", …]`. The server runs it once when a session/lease is acquired (and re-applies on pool re-create). Lives in the user's config dir (e.g. `~/.config/oraclemcp/`), version-controlled by the operator if they wish — outside this repo.
2. **Agent runtime append (easy):** the agent can add/adjust session settings on its leased session via `oracle_session(set_session, ["ALTER SESSION SET CURRENT_SCHEMA=…"])` — no need to touch config. Because `ALTER SESSION` is session-scoped and non-data-mutating, it is permitted at `READ_ONLY` **through a strict allowlist** (NLS, `CURRENT_SCHEMA`, optimizer params, time zone, `NLS_DATE_FORMAT`, etc.); anything outside the allowlist (e.g. statements that change security/audit context) is rejected.
3. **Safety:** every login statement is whitelist-validated; production (`protected`) profiles may additionally require the script to be **HMAC-signed** and verified at load (tamper-evidence); all applied session settings are audited and reported in `oracle_capabilities` / `oracle://session/{lease}`.

So: the *house convention* is the operator's profile `login_script`; the agent layers ad-hoc tweaks on top via `oracle_session` — both pass the same allowlist.

### 6.6 Operating levels — "one user, all levels" (the runtime privilege model)

The requirement: a **single connection/user** must be able to do **read-only as well as read-write — all levels** — not a fixed posture baked into the DB grants. Design:

**Ordered levels** (each a strict superset of the one below):

| Level | Permits | Default escalation gate (per profile) |
|-------|---------|----------------------------------------|
| `READ_ONLY` *(default)* | SELECT (no unproven function call — §5.3), introspection, plan analysis via `DBMS_XPLAN.DISPLAY_CURSOR`, safe sampling | — (always allowed) |
| `READ_WRITE` | INSERT/UPDATE/DELETE/MERGE, transaction control (begin/commit/rollback/savepoint), `DBMS_OUTPUT` | **interactive confirmation** — DCG-style approve/deny prompt or selector options (§7.2) |
| `DDL` | CREATE/ALTER/DROP/TRUNCATE, CREATE OR REPLACE, recompile | **interactive confirmation** (selector) |
| `ADMIN` | GRANT/REVOKE, ALTER USER/SYSTEM, cross-schema, DCL | **interactive confirmation** (often policy-disabled) |

The **gate is an in-band human confirmation** (DCG-style prompt / selector options via the harness + MCP elicitation — §7.2), not a separate device. (Device-based out-of-band 2FA is **out of scope**.) Headless CI uses a pre-issued scoped token.

**How it works:**
- Each session/lease carries a **`current_level`** (starts `READ_ONLY`) and a **`max_level` ceiling that is a property of the connection profile — i.e. per target database.** This is the primary control: you set `max_level = READ_ONLY` on a **production** profile and **no agent, token, confirmation, 2FA, scope, or config-reload can ever escalate past it** — the ceiling is immutable for the life of the process and escalation requests above it are hard-rejected, not merely gated. Over HTTP the OAuth scope can only *lower* the effective ceiling further, never raise it. (See the per-connection ceiling note below.)
- The classifier (§5.3) maps **every statement → its required level**. If `required > current_level`, the call is blocked with an actionable error telling the agent exactly how to escalate ("needs `READ_WRITE`; call `oracle_session(escalate, target=READ_WRITE)` — the operator will be asked to confirm").
- **Escalation** is explicit (`oracle_session` escalate) and gated by the table above. It can be granted **per-statement** (a step-up token bound to that exact SQL digest, single-use) **or** as a **time-boxed session elevation window** (e.g. 15 min at `READ_WRITE`) so a scripted multi-statement transaction isn't re-prompted each call. The window has a monotonic TTL and **auto-drops back to `READ_ONLY`** on expiry.
- **Effective capability = DB-privilege ceiling ∩ `current_level`.** You can never exceed your Oracle grants; within them you operate at the gated, audited level. Per-schema policy (§6.2) can cap further (e.g. `SYSTEM` deny-all regardless of level).
- `oracle_capabilities` always reports `{current_level, max_level, gates, elevation_expires_at}` so the agent knows where it stands; every escalation grant/denial/expiry and every required-vs-current decision is audited (§6.4).

This gives the agent a **frictionless read-only default** (with `readOnlyHint:true` so harnesses auto-approve), and a **clear, gated, audited path to write/DDL/admin on the same connection** — satisfying "one user, all levels" without ever silently exceeding the posture.

**Per-connection ceiling — the production guarantee (non-negotiable).** "One user, all levels" applies *within a profile's ceiling*, and that ceiling is **per target database, not global**. A profile may be marked **`protected` (production)**, which makes three guarantees:
1. `max_level` is pinned (default `READ_ONLY`) and **cannot be raised at runtime by anything** — escalation above it returns a hard error, never a confirmation prompt; the value is not hot-reloadable.
2. **Defense in depth is required, not optional:** a `protected` profile must connect with a **least-privilege (read-only) Oracle user**. At startup the server probes the account, but the probe is a **best-effort warning, not a proof** — Oracle write capability can arrive via role grants (incl. non-default roles a `SET ROLE` enables mid-session), `PUBLIC` grants, `ANY` system privileges, proxy/`CONNECT THROUGH`, or `EXECUTE` on a `DEFINER`-rights package that itself writes; enumerating "can this user mutate anything" across all of those is undecidable in practice, and grants can drift after startup (TOCTOU). So the **enforced** boundary on `protected` is: (i) `max_level=READ_ONLY` + the fail-closed classifier (no statement above `READ_ONLY` is ever dispatched), (ii) `SET TRANSACTION READ ONLY`, and (iii) **`SET ROLE` and non-allowlisted `ALTER SESSION` are disabled** so a session cannot enable a write-bearing role post-connect. The least-privilege user is the operator's responsibility, which the probe *recommends*; we do **not** claim the server can detect every write-capable account.
3. Standby/replica auto-detection (§5.8) independently forces `READ_ONLY` regardless of profile.

So a production database is read-only **by the profile (server refuses to escalate) AND by the DB grants (engine refuses) AND by physical role if it is a standby** — three independent locks. There is no single point whose failure grants write access to a `protected` target. Different targets (dev vs prod) are simply different profiles with different ceilings; the same human/agent gets read-write on dev and only-ever-read-only on prod.

---

## §7 Authentication & Step-Up Confirmation Architecture

Three layers. Layer 1 = transport auth; **Layer 2 = the step-up *confirmation* gate for privileged operations** (in-band human confirmation, DCG-style / selector — *not* a separate device); Layer 3 = Oracle DB auth. (Device-based out-of-band 2FA is **out of scope** — §7.2.)

### 7.1 Layer 1 — transport authentication

- **stdio:** trust boundary is the OS process (the agent spawned it). Harden with an HMAC-validated init token from `$ORACLEMCP_STDIO_TOKEN`, checked on the first `initialize` before any other request; refuse to start without it unless `--allow-no-auth` is explicit.
- **Streamable HTTP(S) (v1, day-one):** OAuth 2.1 **resource server only** (validate, never issue) via `tower-oauth2-resource-server` + `jsonwebtoken`; RFC 9728 protected-resource metadata at `/.well-known/oauth-protected-resource`; PKCE S256 for interactive flows; RFC 8707 resource indicators + RFC 9207 `iss` validation; TLS/HTTPS-only, reject non-loopback `http://`; **mTLS** (`rustls`) as defense-in-depth. Progressive scopes `oracle:read` → `oracle:execute` → `oracle:admin` map to the operating-level ceiling (§6.6) and are challenged via `WWW-Authenticate`.

### 7.1.1 Two different hops — MCP transport vs Oracle Net (EZConnect)

A common confusion: "why not use Oracle Net / EZConnect as the transport?" Because there are **two independent connections** at different layers, and EZConnect belongs to the *second* one — which we already use.

```
   AI agent  ──[ HOP 1: MCP transport ]──>  oraclemcp server  ──[ HOP 2: Oracle Net ]──>  Oracle DB
              JSON-RPC over stdio | HTTP(S)                     EZConnect / EZConnect+ / TNS + wallet
```

- **Hop 1 (agent ⇄ server) = the MCP transport.** This *must* be MCP (JSON-RPC 2.0 over **stdio** or **Streamable HTTP**), because that is the only protocol AI agents/harnesses speak to discover and call tools. Oracle Net is a *database wire protocol* — an agent cannot "speak Oracle Net" to get MCP tools, and Oracle Net carries no notion of tools/resources/prompts/auth-scopes. So EZConnect is **not applicable** as the agent transport.
- **Hop 2 (server ⇄ Oracle) = Oracle Net, and yes, this is exactly EZConnect.** The profile's `connect_string` is an Oracle Net descriptor — **EZConnect** (`host:port/service`), **EZConnect-Plus** (`tcps://host:port/service?wallet_location=...&...` for TLS/cloud), or a **`tnsnames.ora` alias** — handled transparently by ODPI-C / Instant Client (thick mode). Wallets/mTLS/Kerberos/IAM ride on this hop. So Oracle Net + EZConnect *is already the chosen DB-side connection method*; it just isn't (and can't be) the agent-facing transport.

### 7.2 Layer 2 — the step-up **confirmation** gate (default: DCG-style prompt / selector)

The agent is the untrusted party, so escalating a session to a higher operating level (§6.6) or running a guarded statement requires a **human-in-the-loop confirmation**. The **default mechanism is an in-band confirmation — no separate device** — delivered two complementary ways:

1. **Harness confirmation (DCG-style).** Write/escalate tools carry `destructiveHint:true`, so a harness like Claude Code already prompts the operator "allow this tool call?" — the familiar DCG checkpoint. We rely on this where present.
2. **Server-driven selector via MCP elicitation (harness-agnostic, the real default gate).** When the server needs approval it returns an **elicitation request with explicit selector options**, which any MCP client renders as a choice. This works regardless of whether the harness honors annotations, and it lets the operator pick an outcome, not just yes/no. Example — escalation request to `READ_WRITE` to run a shown `UPDATE`:

   ```
   oraclemcp: Agent requests READ_WRITE to run:
     UPDATE orders SET status='X' WHERE id=42      (preview: ~1 row affected)
   Choose:  [Approve once]  [Approve READ_WRITE for 15 min]  [Preview only]  [Deny]
   ```

   The server maps the chosen option to a single-use approval bound to `(tool + canonicalized-SQL-digest + session)` or to a time-boxed elevation window; the choice and outcome are audited (§6.4).

**The confirmation IS the gate.** It is honest about what it is: human-in-the-loop approval in the current session (like DCG), **not** a cryptographic second factor. It is the v1 requirement.

**Out of scope (not under discussion):** a true out-of-band phone/passkey second factor (TOTP/push/WebAuthn). We are *not* building device-based 2FA — the in-band confirmation/selector above is the gate.

**Non-interactive / CI mode:** when no human is present, operators pre-issue a scoped, revocable, time-boxed token (`oraclemcp token issue --scope oracle:execute --ttl 1h`) — a real credential to protect, not a self-issued convenience token.

**Implementation note:** use the **poll/Task pattern** (return `CHALLENGE_REQUIRED`, agent polls `tasks/get`) rather than holding an HTTP request open for the confirmation — robust across SSE keepalives/proxies/load balancers.

### 7.3 Layer 3 — Oracle database authentication (thick mode enables all of these)

Password (via keyring/Vault, never env in prod) · **Wallet/SEPS** auto-login (`/@alias`, `orapki secretstore`, `TNS_ADMIN`) — the preferred production mode · **Kerberos** (`SQLNET.KERBEROS5_DELEGATION_MODE=CONSTRAINED`, keytab at startup) · **RADIUS/native MFA** (Oracle 19c Jul-2025 DBRU / 23ai) — applied to *interactive DBA users, not pool service accounts* (enforce MFA at the MCP layer for pooled connections instead) · **OCI IAM token** for Autonomous DB · **Proxy auth** (`CONNECT THROUGH`) for per-agent identity in Unified Auditing without per-agent passwords.

---

## §8 Tool Surface & Agent Ergonomics

Grounded in 2025–26 research (97% of real MCP tools have a description defect; >15 undiscriminated tools degrade non-agentic models ~85%; flat schemas give +47% calling accuracy). Therefore: **≤12 *core built-in* tools, namespaced `oracle_`, flat inputs, dual output, actionable errors.** The ≤12 budget bounds the surface *we* ship; **operator-added virtual tools (§8.6) are additive by the operator's choice**, and the `oracle_run_named` meta-dispatch exists precisely so a large custom catalog need not enlarge the top-level surface.

### 8.1 The tools (v1 target)

| Tool | One-line contract | Annotations |
|------|-------------------|-------------|
| `oracle_connect` | Open/attach a pooled connection by profile (incl. OCI/cloud wallet or IAM token), run login script, set `max_level`, return session state + capabilities. | `readOnly:false, idempotent:true` |
| `oracle_capabilities` | Zero-arg entry point: tools, current/max operating level (§6.6) + escalation gates, connection/standby/cloud status, feature tiers, privilege profile, version. | `readOnly:true, idempotent:true` |
| `oracle_query` | Execute SQL/PL-SQL; reads run; DML/DDL default to **preview** (classify + ground-truth impact + token). Cursor-paginated. Optional `lease_id`. | `readOnly:false, destructive:true, openWorld:false` |
| `oracle_query_execute` | Consume a single-use preview/step-up token and run the pre-classified statement. | `readOnly:false, destructive:true` |
| `oracle_schema_inspect` | Tables/views/packages/types with `depth=summary\|full`; columns, constraints, indexes, synonyms. Paginated. **`depth=full` also captures/refreshes the offline catalog snapshot (§9.3) the engine tools consume — so `oracle_connect → schema_inspect(full) → dependency_graph` needs no out-of-band setup.** | `readOnly:true, idempotent:true` |
| `oracle_plsql_analyze` | Static analysis of a PL/SQL object: signatures (`ALL_ARGUMENTS`), calls/refs (PL/Scope), lint hints, complexity. | `readOnly:true, idempotent:true` |
| `oracle_dependency_graph` | Transitive deps (up/down) for an object as adjacency list + DOT/JSON, depth-limited, cycle-safe. | `readOnly:true, idempotent:true` |
| `oracle_get_ddl` | `DBMS_METADATA.GET_DDL` (+ dependent DDL), storage/tablespace stripped for diff-friendliness. | `readOnly:true, idempotent:true` |
| `oracle_explain_plan` | Summarized plan (cost hotspots, E-rows vs A-rows, index hints) as structured JSON. Standby-aware. | `readOnly:true` (writes PLAN_TABLE — see §5.8) |
| `oracle_search` | Full-text search across object names + `ALL_SOURCE` + comments; fuzzy. | `readOnly:true, idempotent:true` |
| `oracle_session` | Manage leases (acquire/release/renew), **escalate/de-escalate operating level** (§6.6, confirmation-gated via selector), get/set ALTER SESSION, capture DBMS_OUTPUT, transaction control (begin/commit/rollback/savepoint — lease-bound). | `readOnly:false` |
| `oracle_compare_schemas` *(P3)* | Diff two schemas/DBs/source → safe ALTER+recompile migration sequence. | `readOnly:true, idempotent:true` |

### 8.2 Output & errors

- **Dual output:** `content` (concise text for LLM reasoning) + `structuredContent` (JSON validated against `outputSchema`, generated from Rust types via `schemars`). Results carry `truncated`, `row_count`, `next_cursor`, `columns`.
- **Token budget:** default `max_rows=200`, `max_result_bytes=10MB`, hard cap sized against Claude Code's ~25k-token tool-response limit; the *same* streaming + cursor substrate is shared by query *and* heavy introspection tools (the critic's gap — introspection gets the same caps as query).
- **Actionable errors** (`isError:true`, not JSON-RPC error codes): on `ORA-00942` return `{error_class:"OBJECT_NOT_FOUND", suggested_tool:"oracle_schema_inspect", fuzzy_matches:[...]}` using a Levenshtein/trie over cached schema. Errors say what to do next.

### 8.3 Resources & Prompts (the critic's "tools-only" gap)

- **Resources** with a coherent scheme: `oracle://schema/{owner}` (object listing), `oracle://object/{owner}/{type}/{name}` (DDL/source), `oracle://session/{lease_id}` (live session state), `oracle://capabilities`, `oracle://tools` (the virtual-tool catalog — §8.6; in v1 the catalog is also reported by `oracle_capabilities`, since Resources land in P2). Cursor pagination; `resources/list_changed` + `resources/updated` notifications where feasible (e.g. DDL change via `DBMS_CHANGE_NOTIFICATION`).
- **Prompts** (parameterized recipes that make the server discoverable/harness-agnostic): `investigate_slow_query`, `safe_column_rename`, `explain_this_package`, `find_callers_of`, `generate_migration`. These ship "expert playbooks" any harness can list.

### 8.4 Connection bootstrap (make it trivial)

`~/.config/oraclemcp/profiles.toml` (or `.oraclemcp.toml` in-project): named profiles with `connect_string` (an **Oracle Net EZConnect / EZConnect-Plus / TNS-alias** string — see §7.1.1), `username`, `credential_ref`, `pool` settings, `login_script` path, **`max_level`** (per-target ceiling, default `READ_ONLY`), **`protected = true`** for production (pins the ceiling + requires a least-privilege user — §6.6), `read_only_standby`, and `base` for inheritance (shallow-merge, cycle-detected). `oracle_connect(profile:"prod_ro")` — the agent never handles raw credentials or learns Oracle connection syntax. `list_profiles()` returns non-secret metadata (incl. each profile's `max_level`) for self-discovery; secret refs are never materialized into returned metadata.

### 8.5 Client compatibility — harness-agnostic by construction

Standard MCP (JSON-RPC 2.0) over **stdio** (universal — the agent spawns the binary) and **Streamable HTTP(S)** (remote/shared). Any MCP-compliant client connects and calls **all** tools — no per-agent integration, no bespoke SDK.

| Client | Transport | All tools | Step-up confirmation UX |
|--------|-----------|-----------|--------------------------|
| **Claude Code** | stdio + HTTP | ✅ | native MCP **elicitation** selector (best) — `claude mcp add oraclemcp -- oraclemcp serve` |
| **Codex** | stdio | ✅ | tool-approval prompt / pre-issued CI token — `[mcp_servers]` in `~/.codex/config.toml` |
| **Hermes** | stdio + HTTP | ✅ | elicitation where supported, else token |
| **Gemini CLI / Cursor / VS Code Agent** | stdio (+ HTTP) | ✅ | the client's own "allow this tool?" prompt |

- **Reads are frictionless everywhere** (`readOnlyHint` → harness auto-approves). The bulk of the value (explore, analyze, dep-graph, change-impact) needs nothing special from the client.
- **Writes / escalation** are always reachable; only the *prompt polish* varies — clients with MCP **elicitation** get the in-band selector (§7.2); others fall back to their own tool-approval prompt or a pre-issued token (CI). **Capability is never gated on client features**, only confirmation UX.
- **Resources & Prompts** (§8.3) are a bonus where the client supports them; tools never depend on them.
- **Server-host requirement (invisible to the agent):** live-DB mode needs Oracle Instant Client on the machine running the server — via the Docker image or a native install + `oraclemcp doctor` (§13). **Offline mode (`live-db` off) needs nothing** and runs anywhere.

### 8.6 Custom / virtual tools — operator extensions without a fork (P1)

Companies need to expose their **own, proprietary operations** as MCP tools **without forking the repo or upstreaming a PR** (their logic is company-specific). The mechanism is **config-driven "virtual tools"** (the `dbhub` pattern), and it ships in **v1 (P1-13)** — it is an *adoption driver*, not hardening, and it reuses the Phase-1 spine (classifier, bind-first execution, audit, registry) rather than adding a subsystem.

**Where they live.** Operator-supplied, **never in this repo** (like login scripts, §6.5): a `~/.config/oraclemcp/tools.d/*.toml` directory, version-controlled privately by the company. Loaded at startup; each entry registers into the same `ToolRegistry`, so **every MCP client (Claude Code, Codex, Hermes, …) discovers them in `tools/list` automatically** (§8.5).

**A definition is a named, typed tool whose body is SQL *or* real PL/SQL — not just one call:**

```toml
[[tool]]
name        = "customer_360"
description = "Full customer profile: orders, invoices, open tickets."
# Form A — inline SQL, a multi-statement batch, OR a full PL/SQL block (DECLARE/BEGIN…END;, loops, cursors):
sql  = "SELECT … FROM … WHERE customer_id = :customer_id"
# Form B (preferred for rich logic) — wrap an EXISTING package the company already ships:
# call = "billing_api.get_customer_360(:customer_id)"
params = [ { name = "customer_id", type = "number", required = true } ]
output = "rows"          # rows | scalar | ref_cursor | json
```

- **Form A — inline SQL / PL/SQL.** A single statement, a multi-statement batch, or a full anonymous block. **Real code, not just one call.** Inline PL/SQL blocks hit the fail-closed floor (any block is **≥ `Guarded`** — §5.3) and write-bearing bodies need the matching level.
- **Form B — existing-package wrapper (preferred for anything rich).** Because we ship the engine, it analyses the *package body's* call/dep-graph offline and can **prove it read-only** (→ the tool can be `Safe`/auto-approved) or precisely flag that it writes. A deliberate incentive: push complex logic into **analysable DB packages**, not opaque config blobs.
- **Form C — out-of-process / WASM** for genuinely custom non-SQL logic: deferred (§11, Phase 3). (A)+(B) cover Oracle operations.

**Safety — a custom tool grants ZERO new privilege (acceptance gates):**
- **Operator-supplied only, never agent-defined at runtime** — an agent inventing arbitrary-SQL tools is just `oracle_query` with more risk; *definitions* come from operator config.
- **Classified fail-closed at LOAD** (§5.3): danger level / required operating level is **derived from what the body does**, not the author's claim (the author can only make it stricter). A `Forbidden` body — or any `Forbidden` sub-statement in a batch — **refuses to load**.
- **Bind-variable-only params** — agent argument values are bound, never interpolated → no injection through a tool's parameters.
- **Respects `max_level` / per-schema policy / `protected`** — a read-only profile's custom tools are read-only. If a tool's required level exceeds the profile's `max_level` (e.g. a write block on a `protected` read-only target), it **refuses to load** (fail-fast at startup, not at call time).
- **HMAC-signed on `protected` profiles** (like login scripts) so a tampered `tools.toml` is rejected at load.
- **Audited identically to built-ins** (tool name + resolved SQL digest; §6.4). Custom tools share the classifier-trust risk (R1) — classify-at-load + the fail-closed law are the mitigation.

**Agent ergonomics (the ≤12-tools tension).** Small catalog → register as **first-class MCP tools** (proper `inputSchema`, annotations, dual output — best UX). Large catalog → a single **`oracle_run_named(name, params)`** meta-dispatch keeps the top-level surface tiny, with the full catalog discoverable via `oracle_capabilities` / an `oracle://tools` resource. Operators choose per profile.

---

## §9 Feature Set (tiered, by domain)

Consolidated from five domain ideators + the completeness critic. Priorities: **P0** = correctness substrate (must precede everything) · **P1** = MVP an Oracle user would adopt · **P2** = the gate to "safe for production" · **P3** = deferred, build only after the core is proven.

### 9.1 Connection & Session Management
- **P0** Named connection profiles with inheritance (incl. `default_level`/`max_level`, both defaulting to `READ_ONLY`); credential-backend abstraction with secrets isolation; **session-lease primitive** (§5.1); login-script executor (ALTER SESSION on lease acquire); **operating-level core** (§6.6: current/max level, classifier→level mapping).
- **P1** `r2d2-oracle` pool with health-check-on-recycle, ping, auto-reconnect; standby auto-detection (§5.8); `list_profiles`; **OCI / Oracle Cloud (Autonomous DB) connectivity** — cloud wallet (mTLS, `cwallet.sso` + `TNS_ADMIN`) and OCI IAM database-token paths.
- **P2** DRCP support (`SERVER=POOLED`, `connectionClass=ORACLEMCP`); pool tuning surface; SIGHUP non-credential reload.
- **P3** Non-homogeneous/proxy session pools; multi-tenant proxy-auth identity.

### 9.2 Query & PL/SQL Execution + Safety
- **P0** Bind-variable-first execution (no string interpolation of agent values); type/NLS serializer (§5.2); structured error envelope; autocommit-off.
- **P1** `oracle_query` read path with cursor pagination + row/byte caps; fail-closed classifier (§5.3); read-only enforcement layers (§6.3); durable audit (§5.13); honest allow-once token (§5.5).
- **P2** Execute-in-savepoint **preview** (§5.4); transaction/savepoint tools (lease-bound); DBMS_OUTPUT capture; admission control (§5.6); cancellation correctness (§5.7).
- **P3** Multi-statement batch orchestration; long-op Tasks with progress; result export.

### 9.3 PL/SQL Intelligence & Schema Introspection (the differentiator)

**Reuse `plsql-intelligence`'s engine — do not reimplement (§0).** The deep tools (`oracle_plsql_analyze`, `oracle_dependency_graph`, change-impact, recompile order, lint) call `plsql_engine::analyze_project` → an `AnalysisRun` (parse results + semantic model + dep-graph + completeness report), then `plsql_lineage` (impact/recompile/classify), `plsql_sast` (static analysis → SARIF), `plsql_cicd` (predict impact), wrapped via `plsql_output`. This is a *real offline parser/IR/dep-graph* (catalog-snapshot, no live DB needed), far beyond dictionary queries. **The engine is always compiled into the shipped `oraclemcp` binary — it is the differentiator, never optional (§13.1).** The only build toggle is the **`live-db`** Cargo feature, which adds/removes the *Oracle driver* (and the Instant Client dependency) — **not** the engine: with `live-db` off you get a pure offline static-analysis server (parser + IR + dep-graph over source/snapshot) with zero native deps. Import `plsql-engine` (never ANTLR types directly). The live-dictionary tools below complement the offline engine.

**Snapshot capture — the missing link (close the loop from live connection to offline engine).** The engine consumes a `CatalogSnapshot` (objects, columns, dependencies, source, arguments — `plsql_catalog::CatalogSnapshot`), and today the MCP tools *consume* one (`compare_oracle_deps`) but **no tool *produces* one from a live connection** — a gap that would force out-of-band setup before the dependency-graph differentiator works. Resolution: snapshot capture is **a mode of `oracle_schema_inspect depth=full`** (and optionally eager at `oracle_connect`): it reads the dictionary (`ALL_OBJECTS`/`ALL_SOURCE`/`ALL_DEPENDENCIES`/`ALL_TAB_COLUMNS`/`ALL_ARGUMENTS`, schema-filtered), serializes a `CatalogSnapshot`, and **caches it per profile with a `captured_at` stamp and a staleness check** (re-capture on TTL or detected DDL drift; `oracle_capabilities` reports snapshot freshness). The engine's live loader (`CatalogLoadRequest { schema_filters }`) already exists in `plsql-catalog` — this exposes it through the tool surface so every engine tool reads the cached snapshot and stays offline/repeatable. No new tool (keeps the surface ≤12).

- **P1 (Tier 1, zero extra license):** `oracle_schema_inspect`, `oracle_get_ddl` (`DBMS_METADATA`), compile-error retrieval (`ALL_ERRORS` + `PLW-` warnings), dependency traversal (`DBA_DEPENDENCIES` hierarchical, cycle-safe; cross-checkable against the engine's offline graph via `compare_oracle_deps`), `oracle_explain_plan` (structured JSON), `oracle_search`, safe data sampling.
- **P2 (Tier 2, opt-in, PL/Scope recompile):** call graphs & symbol cross-reference (`ALL_IDENTIFIERS`), SQL statement map (`ALL_STATEMENTS`), lint (unused vars, dead code, `EXECUTE IMMEDIATE` audit), with a `recompile_with_plscope` helper.
- **P3 (Tier 3, license-gated):** AWR/ASH top-SQL & wait analysis **only** when `control_management_pack_access != NONE`; Statspack fallback otherwise (§5.11); `oracle_compare_schemas`/`generate_migration`.

### 9.4 Auth, Step-up Confirmation & Security
- **P1 (both transports + confirmation gate are day-one — maintainer requirement)** stdio init-token; **Streamable HTTP(S): TLS + OAuth 2.1 RS + mTLS** (§7.1); **step-up confirmation gate** (§7.2: DCG-style prompt / selector via MCP elicitation) driving operating-level escalation (§6.6), with single-use SQL-digest-bound approvals + pre-issued CI tokens; **OCI IAM token + cloud wallet auth** (§7.3); operating-level enforcement; least-privilege user model; per-schema allow/deny policy; secrets via keyring; SECURITY.md + threat model.
- **P2** Vault secrets backend; RBAC per tool (scope→`max_level` mapping); Oracle Unified Auditing policy; session-elevation windows + replay-hardening.
- **P3** Kerberos / RADIUS / proxy auth adapters; Vault dynamic credentials + zero-downtime rotation. *(Device-based out-of-band 2FA is out of scope — §7.2.)*

### 9.5 Ergonomics, Observability & Extensibility
- **P1** `oracle_capabilities`; dual output + actionable errors; `tracing` JSON logs; health endpoint; **custom / virtual tools (operator config — §8.6).**
- **P2** OTel metrics/traces; Prometheus via OTel collector; Resources + Prompts; graceful shutdown.
- **P3** WASM/subprocess plugin boundary; resource subscriptions; W3C trace-context propagation.

---

## §10 Observability & Production Hardening

- **Logging:** `tracing` + `tracing-subscriber` (JSON, env-filter); a span per request carrying `request_id`, `tool_name`, `db_user`; never log bind values.
- **Metrics/traces:** `opentelemetry` + `opentelemetry-otlp` + `tracing-opentelemetry` → OTel Collector (avoid the beta pull-based Prometheus exporter). Instruments: `db.query.duration_ms`, `db.pool.active_connections`, `db.pool.wait_ms`, `mcp.requests.total{tool,status}`, `db.errors.total{ora_code}`.
- **Health:** separate port `:9090` with `/healthz` (liveness) and `/readyz` (pings a pool conn; fails immediately on shutdown).
- **Pool:** `r2d2-oracle`; `max_size = min(cpu*2+1, 20)`, `min_idle=2`, acquire timeout 5s, `statement_cache_size≥50`, ping on recycle.
- **Timeouts & cancel:** `conn.set_call_timeout(30s)` per round-trip; race query future against `tokio::time::timeout`; on timeout `conn.break_execution()` then discard the connection.
- **Resource limits:** `max_rows` 10k hard cap, `max_result_bytes` 10MB, `max_execution_time` 30s — enforced in the handler via a `LimitedStream` over the row iterator.
- **Rate limiting:** `Semaphore` (stdio) / `tower-governor` GCRA (HTTP), per-agent.
- **Resilience:** `failsafe-rs` circuit breaker (open after 5 consecutive ORA- errors); `backon` retries **only** transient codes (ORA-03113/03114/12170/12541), never ORA-00942/01403; **never** retry DML.
- **Crash safety:** `panic=abort`, panic hook → `tracing::error`, no `unwrap()` on DB ops, `anyhow`/`thiserror`, PID file, systemd restart, flush exporters on shutdown.

---

## §11 Extensibility

Three layers, in safety order (see §5.12):
1. **Internal `ToolRegistry` trait** — compile-time tool registration; the DCG pipeline is `Vec<Box<dyn StatementEvaluator>>`; new tools implement a `Tool` trait. (P1)
2. **Config-driven virtual tools** — `tools.d/*.toml` defines named SQL/PL-SQL (inline statement, multi-statement batch, or full PL/SQL block) or existing-package wrappers, with typed params and annotations; they run through the *same* fail-closed classifier + audit + limits as built-ins (the `dbhub` pattern), classified-at-load. No code, no recompile, no PR. **(P1 — full design in §8.6.)**
3. **Out-of-process / WASM plugins** — a capability-scoped, sandboxed boundary for third-party logic, communicating over IPC/WASM with no direct process access. (P3, optional) **Never** in-process `.so`.

---

## §12 Testing & Quality Strategy (the "safe for production" gate)

A production-safety claim **cannot rest on unit tests**. The non-negotiable suite:

- **Real-Oracle integration matrix** (no mocks for DB behavior): Oracle **XE**, **19c**, and **23ai** (features span DRCP, JSON type, Kerberos modes, native MFA). Run in CI via containers (gvenzl/oracle-xe, gvenzl/oracle-free) and a tagged "enterprise" job for 19c/23ai.
- **Classifier differential fuzz corpus** — an adversarial corpus (comment-hidden DML, CTE-wrapped DML, MERGE, FORALL, AUTONOMOUS_TRANSACTION, dynamic SQL, **a side-effecting function call inside a SELECT, a side-effecting trigger/VPD policy fired by a read, `q'[…]'`/literal `;` desync, EXPLAIN PLAN**, multi-statement, semantic-changing hints) asserting **fail-closed**: every unparseable input, every PL/SQL block, and **every statement the engine cannot prove `ProvenReadOnly`** is classified ≥ Guarded; a literal/quote desync makes the whole batch `Forbidden`. `cargo-fuzz` target on the classifier.
- **Chaos tests** — listener drop, pool exhaustion, RAC/standby failover, credential rotation mid-flight, lease-TTL expiry with an open transaction (assert rollback), cancel mid-DML (assert no double-execute).
- **Type-fidelity golden tests** — every type in the §5.2 table round-trips correctly; NUMBER(38) is a string; dates are ISO-8601 regardless of host NLS.
- **Step-up/token tests** — approvals single-use, replay-rejected, binding-checked, monotonic-TTL, hashed-at-rest, never-in-audit-clear.
- **Privilege-degradation tests** — run the suite under a least-privilege account and assert clear "needs privilege X" errors, never empty successes.
- **Unit + property tests** (classifier, type mapping, config precedence), **golden tests** (`insta`) for tool JSON output, `cargo nextest`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo audit`, `cargo deny`.

A safety feature may not back a production claim until it has an adversarial corpus **and** a real-Oracle integration test. Structured JSON-line test logging throughout, per the maintainer's preference for verifiable evidence.

---

## §13 Build, Packaging, Distribution, Licensing

### 13.1 Packaging model — one product binary, three ways to consume

**There is exactly one product binary, `oraclemcp`, and it always includes the full engine** (parser → IR → dep-graph → lineage → SAST). The engine is the differentiator and is **never** an optional build-out; the *only* feature toggle is **`live-db`** (the Oracle driver + Instant Client dependency). Everything the engine offers is reachable through MCP tools (§8.1). Three modes off the one codebase:

| Persona | Gets | How |
|---------|------|-----|
| **Full Oracle MCP — all capabilities** | live DB **+** complete engine (analyze, dep-graph, change-impact, DDL, …) | `oraclemcp` — **default build** (`live-db` on) |
| **Offline / no live DB** | full engine intelligence, **no** Oracle driver, **no** Instant Client, tiny binary | `oraclemcp --no-default-features` (offline; analyses source + catalog snapshot) |
| **Just the parser / engine in your own code** | the engine as Rust **libraries** | `cargo add plsql-parser-antlr` (or `plsql-engine`, `plsql-depgraph`, …) |

There is **no** core-only "product" binary — an Oracle MCP stripped of its intelligence is a product nobody wants. The published `oraclemcp-*` crates are *libraries* (the reusable safety/protocol/DB spine); the product binary is the engine-side build that depends on them (§14). This matches the code today: `plsql-mcp`'s `default = ["live-db"]`, and with it off the binary still exposes the static-analysis tools.

- **Targets:** `x86_64-unknown-linux-gnu` (primary; glibc required for ODPI-C dlopen), macOS, Windows. **Not musl-static** (§2.2). *(Offline builds — `live-db` off — have no ODPI-C dep and can target musl.)*
- **Runtime dependency:** Oracle Instant Client (Basic/Basic Light). Document `LD_LIBRARY_PATH`/`TNS_ADMIN`.
- **`oraclemcp doctor` — a first-class diagnostic mode** (the `brew`/`flutter`/`cargo doctor` pattern; the code's `doctor.rs` already does the Instant Client posture report — this extends it). It checks, with actionable fixes and a non-zero exit on failure: **(1)** Instant Client loadable + resolved version; **(2)** `TNS_ADMIN` / wallet / `tnsnames` resolution; **(3)** connectivity + auth to each configured profile; **(4)** DB role / standby + `open_mode` (§5.8); **(5)** NLS / charset sanity + host↔DB clock skew (§5.10); **(6)** the privilege / capability-tier probe (§5.11) and the `protected`-profile write-privilege warning (§6.6); **(7)** catalog-snapshot freshness (§9.3); **(8)** a classifier self-test against the bundled adversarial corpus (§12); **(9)** virtual-tool config load + classification (§8.6). It runs without a live DB for the offline checks, so it is the supported onboarding/triage step for both Docker and native installs.
- **Docker:** distroless/Chainguard base **+ an Instant Client layer** (≈100 MB). Honest about size; not a 15 MB image.
- **Releases:** `cargo-dist` for cross-builds + installer scripts; `cargo auditable build` for embedded SBOM; Syft → CycloneDX/SPDX; `cosign`-signed artifacts + checksums; GitHub Actions pipeline (fall back to `dsr`/`rch` if Actions is throttled).
- **Visibility accrues to the product, not to libraries.** Discovery for an MCP server comes from the *runnable thing* and its listings — a differentiator-led README + asciinema demo, GitHub topics (`mcp`, `model-context-protocol`, `oracle`, `plsql`), and entries in the MCP registries (the official servers list, mcp.so, Smithery, Glama, PulseMCP). Internal library crates on crates.io drive ~none of it; the `oraclemcp` **binary** owns the "oracle mcp" search term. So distribution effort goes to the binary + registry presence, and the internal `oraclemcp-*` libs stay `publish = false` until the Phase-E flip (§0, §15).
- **Docker is the PRIMARY install artifact, not a secondary one** — because thick mode (§2.2) means `cargo install oraclemcp` *fails* without Instant Client present, a terrible first-run experience and exactly the friction that kills Rust-tool adoption vs. zero-native-dep npx servers. The Docker image is the only channel where the ~100 MB Instant Client layer is invisible to the user. So the README quick-start is **Docker-first** (a copy-paste `mcp.json` / `claude_desktop_config.json` snippet wired to one `docker run`), with `cargo install` as the advanced/native path and `oraclemcp doctor` (already in `doctor.rs`) marketed as the first-class native-install triage step. This turns the plan's biggest adoption liability into a managed onboarding flow.
- **Lead the demo with the *demonstrable* half of the wedge.** The asciinema/README hero is **PL/SQL intelligence** (`oracle_dependency_graph` / change-impact / `oracle_get_ddl` on a real package — the five-second "wow" no "run SQL" competitor can match), with **fail-closed safety as the kicker**, landed via one adversarial example ("every other Oracle MCP would run this `SELECT` against your prod DB; ours sees the function does DML and stops") — which is *also* a fixture in the §12 corpus, so the marketing artifact and the test are the same thing.
- **License:** `Apache-2.0 OR MIT` (dual, §2.3 — *not* Apache-only); `SECURITY.md`, `CONTRIBUTING.md`, `THREATMODEL.md`, dependency provenance for the Instant Client bundling.

---

## §14 Crate / Module Layout (Cargo workspace)

These are the **core crates** (§0). **Now (Phase A): they live as cleanly-bounded modules/crates inside the existing `plsql-intelligence` workspace** — the diagram below is the *Phase-B extraction target*, the shape they take once lifted into the `oraclemcp` repo and published. A thin-bin-crate / fat-lib-crate split keeps incremental builds fast and enables embedding. Crates are **refactored in place** from today's `plsql-mcp`/`plsql-catalog` (not copied into a sibling repo) and upgraded (§0); the engine dependency stays strictly one-way so the extraction is mechanical.

```
oraclemcp/                      # Phase-B extraction TARGET (published to crates.io). TODAY these are
                                #   cleanly-bounded modules/crates INSIDE plsql-intelligence — not a separate repo yet.
├── Cargo.toml                  # workspace
├── crates/
│   ├── oraclemcp/              # (optional) minimal CORE-ONLY example bin — NOT the product. The shipped
│   │                           #   oraclemcp PRODUCT binary is the engine-side build (see plsql-intelligence below).
│   │                           #   The oraclemcp-* crates below are published as LIBRARIES (the reusable core).
│   ├── oraclemcp-core/          # rmcp ServerHandler + ToolRegistry/Tool trait, capabilities, resources, prompts
│   │                           #   (replaces plsql-mcp's hand-rolled mcp_protocol.rs/tcp.rs)
│   ├── oraclemcp-db/       # OracleConnection trait + RustOracleConnection (lifted from plsql-catalog)
│   │                           #   + r2d2 pool + session-lease + type/NLS serializer; odpic-sys escape hatch; live-db feature
│   ├── oraclemcp-guard/        # fail-closed classifier + SideEffectOracle port (default Unknown=fail-closed;
│   │                           #   engine binds impl from the consumer side), operating levels, policy,
│   │                           #   step-up/approval TOKEN + level state (policy/token only — NOT delivery)
│   ├── oraclemcp-audit/        # LEAF crate: out-of-band durable sink (fsync file/SQLite) + hash chain +
│   │                           #   Unified-Auditing adapter; depended on by core/db/guard/auth (avoids fan-in)
│   ├── oraclemcp-auth/         # transport auth, OAuth2 RS, mTLS, step-up confirmation DELIVERY (elicitation/
│   │                           #   selector + poll/Task), secrets backends. Depends on -guard ONE-WAY (auth→guard):
│   │                           #   guard owns the token/level; auth mints into guard's type — never the reverse (no cycle)
│   ├── oraclemcp-telemetry/    # tracing/OTel/metrics/health
│   └── oraclemcp-config/       # figment config, profiles (max_level/protected, OCI/IAM), validation, reload
├── tests/ fuzz/ corpus/        # real-Oracle integration, chaos, golden; cargo-fuzz + adversarial classifier corpus
└── plan.md, AGENTS.md, CLAUDE.md, README.md, SECURITY.md, CONTRIBUTING.md

plsql-intelligence/             # TODAY: the one workspace holding BOTH the core modules above and the
                                #   engine below. At Phase B it becomes the consumer repo that DEPENDS ON
                                #   the published oraclemcp-* crates (path dep → versioned dep).
├── crates/plsql-mcp (bin)      # THE shipped `oraclemcp` PRODUCT binary = core + FULL engine (always on).
│                               #   `live-db` feature toggles the Oracle driver only (off = offline, zero native
│                               #   deps). [[bin]] name = "oraclemcp"; `cargo install oraclemcp` installs this.
└── crates/plsql-engine, -lineage, -sast, -depgraph, -parser-antlr, …  # register Tool impls into the shared ToolRegistry;
                                #   also usable as standalone libraries (e.g. `cargo add plsql-parser-antlr`)
```

Note: there is **no `oraclemcp-intel` crate** — PL/SQL intelligence lives in `plsql-intelligence`'s engine crates and registers tools into `oraclemcp-core`'s registry from that side (one-way dependency, §0). And there is **one product binary, not two**: the published `oraclemcp-*` crates are **libraries** (the reusable core); the **`oraclemcp` product binary is the engine-side build** (core + full engine), so it always carries the differentiator. The core-only example bin exists only to exercise the libraries, and is not a shipped product (§13.1).

---

## §15 Phased Roadmap (dependency-aware — converts directly to beads)

> **Guiding rule: depth-first, not breadth-first.** A narrow, demonstrably-correct, fail-closed read-only core a DBA trusts beats a wide surface of 70%-done features. Both transports, the step-up confirmation gate, and OCI/cloud connectivity are **v1 (maintainer requirements)** — so Phase 1 is larger than a typical MVP, but it is still sequenced depth-first internally (prove the stdio read-only core, *then* layer the secure HTTP + confirmation path on the same spine). IDs are stable handles for the future beads graph; "→" = blocks.

**Per §0 (revised), this is improve-in-place, then extract — not a clean build and not a day-one repo split.** Phase 0 factors the generic MCP core into clean, engine-free module/crate boundaries *inside* `plsql-mcp` (no new repo). The two highest-priority upgrades are the **engine-aware fail-closed classifier (replaces the `is_read_only_sql` string predicate that guards writes today)** and the **sync→async/rmcp migration**. The immediate in-place focus (2026-06-01 direction) starts with the core trio — **classifier → rmcp/async → ergonomics/tool-surface**; the larger **HTTP/OAuth/mTLS + OCI/cloud surface stays in v1 (day-one — reaffirmed 2026-06-01, §17)**, sequenced on the same rmcp spine afterward (1a secure stdio core → 1b HTTP on the same spine). Only the **extract-and-publish + flip** step is deferred — to the trigger-gated **Phase E**, not the end of Phase 1.

### Phase 0 — Correctness substrate (must precede all feature work)
- **P0-0** Boundary step (in place, no new repo) — **a real but bounded refactor, not a `Cargo.toml` edit** (it relocates `OracleConnection`/`RustOracleConnection` out of `plsql-catalog` and carves the ~5 engine-coupled tool handlers — `change_tools.rs`, `parse_tools.rs`, `foundation_tools.rs`, `graph_tools.rs`, `analyze_project.rs` — away from the engine-free spine; its completeness is what makes *Phase E* mechanical, so it carries test risk and must be budgeted as engineering work). Factor `plsql-mcp`'s generic, non-PL/SQL pieces (`safety.rs`, `preview.rs`, `audit.rs`, `dispatch.rs`, `connections.rs`, the protocol) and the lifted DB types into cleanly-bounded modules/crates *inside the existing workspace* that import **no** `plsql-*` engine crate. Add a CI dependency-lint asserting the one-way boundary. These become the future `oraclemcp-*` crates at Phase E. (§0 hard rules 1–2) → P0-*
- **P0-1** Workspace scaffold, CI (clippy/fmt/nextest/audit/deny), error envelope (`thiserror`). → everything
- **P0-2** Config + profiles (`figment`, precedence, validation, versioned schema; `default_level`/`max_level`; OCI wallet/IAM profile fields) [§5.9]. → P1-*
- **P0-3** Oracle connectivity in `oraclemcp-db`: lift `OracleConnection`/`RustOracleConnection` from `plsql-catalog`, add `r2d2-oracle` pool + `spawn_blocking` boundary; **incl. OCI/cloud wallet + IAM-token connect**; `doctor` (keep `plsql-mcp`'s Instant Client posture report). [§4.3] → P0-4,P0-5
- **P0-4** **Session-lease primitive** with TTL + forced rollback/return [§5.1]. → all stateful tools
- **P0-5** **Type-mapping + NLS canonical serializer** with golden tests [§5.2]. → all data tools
- **P0-6** Adopt `rmcp` in `oraclemcp-core` (replacing `plsql-mcp`'s hand-rolled `mcp_protocol.rs`/`tcp.rs`); stdio transport + init-token auth [§2.6, §7.1]; `oracle_capabilities`. Begins the sync→async migration (engine stays sync behind `spawn_blocking`); lockstep dispatch test is the safety net.
- **P0-7** **Operating-level core** (current/max level, classifier→level mapping, level-gated dispatch) [§6.6]. → P1-2,P1-10

### Phase 1 — v1 (SAFE-by-default core + BOTH transports + confirmation gate + cloud)
*Internal order: 1a (read-only core over stdio) → 1b (secure HTTP + confirmation gate on the same spine).*
- **P1-1** Fail-closed, **engine-aware** classifier + adversarial corpus + fuzz [§5.3] — **replaces `plsql-mcp`'s `is_read_only_sql` string predicate (`query.rs:367`), which today passes `SELECT side_effecting_fn() FROM dual` as read-only; do this FIRST (safety-critical).** Adds the engine call-graph consult for side-effect reachability on top of the syntactic fail-closed core. (P0-1,P0-7) → P1-2
- **P1-2** `oracle_query` read path: bind-first, cursor pagination, row/byte caps. (P0-3,P0-5,P1-1)
- **P1-3** Read-only enforcement layers + least-privilege docs [§6.3]. (P0-3)
- **P1-4** Durable audit (fsync-before-execute) [§5.13]. (P0-1) → P1-2
- **P1-5** Tier-1 intelligence: `schema_inspect` (**incl. live catalog-snapshot capture + per-profile cache at `depth=full` — §9.3**), `get_ddl`, dependency graph, compile errors, `search`, `explain_plan` (standby-aware), sampling [§9.3]. (P0-5,P1-7)
- **P1-6** `oracle_connect`/`list_profiles` + login scripts + honest allow-once token [§5.5,§8.4]. (P0-2,P0-4)
- **P1-7** Standby auto-detection [§5.8]. (P0-3)
- **P1-8** `tracing` JSON logs + health endpoint [§10]. (P0-1)
- **P1-9** **Streamable HTTP(S): TLS + OAuth 2.1 RS + mTLS** [§7.1]. (P0-6) — day-one transport
- **P1-10** **Step-up confirmation gate (DCG-style prompt / selector via MCP elicitation) + operating-level escalation** (poll/Task; per-statement approval + elevation window) [§7.2,§6.6]. (P0-7,P1-6) — day-one *(device-based out-of-band 2FA is out of scope)*
- **P1-11** **OCI / Oracle Cloud (Autonomous DB) connectivity** hardening: cloud wallet + IAM token, ADB connect strings [§7.3,§9.1]. (P0-3)
- **P1-13** **Custom / virtual tools** (operator config; §8.6): config-driven named SQL/PL-SQL (inline statement, batch, or full PL/SQL block) **or** existing-package wrappers; **classified fail-closed at load** (level derived from behavior), bind-only params, respects `max_level`/policy/`protected`, HMAC-signed on `protected`, first-class-tools or `oracle_run_named` meta-dispatch. (P1-1,P1-2,P1-4) — adoption-critical, ships in v1.
- *(The former P1-12 "extract & publish + flip" is **removed from Phase 1** and deferred to the trigger-gated **Phase E** below — §0. Phase 1 ends with a great MCP shipping in-place from the `plsql-intelligence` workspace, not with a repo split.)*

### Phase 2 — Production hardening (the gate to fully "safe for production")
- **P2-1** Admission control / backpressure [§5.6]. (P0-3)
- **P2-2** Cancellation + graceful shutdown + crash rollback [§5.7]. (P0-4)
- **P2-3** Execute-in-savepoint **preview** + transaction/savepoint/DBMS_OUTPUT tools [§5.4]. (P0-4,P1-1)
- **P2-4** RBAC per tool (scope→`max_level`) + session-elevation windows + replay-hardening [§6.6,§7.2]. (P1-10)
- **P2-5** Vault secrets backend [§6.5]. (P1-6)
- **P2-6** OTel metrics/traces [§10]. (P1-8)
- **P2-7** Tier-2 PL/Scope intelligence + `recompile_with_plscope` [§9.3]. (P1-5)
- *(P2-8 "config-driven virtual tools" **promoted to P1-13** — §8.6 — it's an adoption driver, not hardening.)*
- **P2-9** Privilege-degradation matrix + capability reporting [§5.11]. (P1-5)
- **P2-10** Oracle Unified Auditing policy as system-of-record [§6.4]. (P1-4)

### Phase 3 — Deferred (only after the core is proven against real Oracle)
- **P3-1** Auth adapters: Kerberos / RADIUS / proxy [§7.3]. *(OCI IAM is P1-11, not deferred. Device-based out-of-band 2FA is out of scope — §7.2.)*
- **P3-2** Vault dynamic credentials + zero-downtime rotation.
- **P3-3** Tier-3 AWR/ASH (license-gated) + Statspack fallback [§9.3].
- **P3-4** `oracle_compare_schemas` / migration generation [§9.3].
- **P3-5** WASM/subprocess plugin boundary [§11].
- **P3-6** DRCP non-homogeneous/proxy pools; resource subscriptions; W3C trace-context; WebAuthn admin UI.

### Phase E — Extraction, publish & depend-back (= §0's "Phase B"; trigger-gated — NOT before the §12 quality bar)
*Deferred by design (§0) — but on **objective gates, not a vibe**, so it cannot slip forever (R14). Extraction becomes **due** (a release blocker, not merely "allowed") once ALL hold; review the checklist at every minor release:*
1. *§12 suite green for two consecutive releases: real-Oracle matrix (XE + 19c + 23ai) + classifier adversarial corpus + fuzz, **zero fail-open findings.***
2. *The `oraclemcp-*` candidate modules have compiled with **zero `plsql-*` engine imports** (CI dependency-lint, P0-0) for ≥30 days — the boundary has been load-bearing, not just declared.*
3. *No breaking change to the `Tool`/registry trait or the ≤12-tool public surface in the last 30 days (the API-churn proxy for "stable enough to semver" — this is the genuine pre-1.0-publish-churn protection).*
4. *A dated owner + target version are recorded; once gates 1–3 are green, shipping Phase E within one minor-release cycle is a tracked release blocker. Until then everything above ships in-place from the `plsql-intelligence` workspace; Phase A's boundary discipline (P0-0) is what makes the extraction itself mechanical.*
- **E-1** Verify the boundary holds: the generic core (`oraclemcp-*` candidate modules) imports no `plsql-*` engine crate; CI dependency-lint green. (§0 hard rules 1–2)
- **E-2** `git filter-repo` the generic core + the `oraclemcp` binary into a new **`oraclemcp`** repository, carrying history; the engine + PL/SQL-intelligence tools stay in `plsql-intelligence`.
- **E-3** Publish the `oraclemcp` crate(s) to crates.io (`publish = true`); `cargo-dist` installers + a Docker image with the Instant Client layer so `cargo install oraclemcp` / one-line install works. (§13)
- **E-4** Flip `plsql-intelligence` from in-workspace path deps to the published versioned `oraclemcp` deps; confirm its lockstep + live-XE tests pass unchanged on the published core. (§0 hard rule 4)
- **E-5** List `oraclemcp` in the MCP registries (official servers list, mcp.so, Smithery, Glama, PulseMCP); ship the differentiator-led README + demo. (§13)

---

## §16 Risk Register

| # | Risk | Severity | Mitigation |
|---|------|----------|-----------|
| R1 | **Classifier false sense of security** (false negatives on obfuscated/PL-SQL DML) | Critical | Fail-closed law + adversarial corpus + fuzz; DB-level least privilege as real boundary; never claim "sandbox" (§5.3,§5.5). |
| R2 | **Pool + session-state incoherence** | Critical | Session-lease primitive; stateful ops require a lease or error (§5.1). |
| R3 | **Allow-once token mistaken for a control** | High | Documented as friction-only; real boundaries are the DB privilege ceiling + the human step-up confirmation (§5.5). |
| R4 | **EXPLAIN-PLAN cardinality as ground truth** | High | Execute-in-savepoint preview is the impact gate; EXPLAIN PLAN is for perf only (§5.4). |
| R5 | **Scope/time-to-v1 explosion** (large day-one v1: both transports + OAuth + mTLS + cloud + confirmation gate) | High | Depth-first phasing *within* v1 (1a stdio core → 1b HTTP on the same spine); narrow fail-closed core first (§15); extraction itself deferred to Phase E. |
| R6 | **Thick-mode/Instant-Client friction** | Medium | Decided & documented up front; `doctor` command; Docker IC layer (§2.2). |
| R7 | **Plugin FFI = arbitrary code in trusted process** | High | No in-process `.so`; virtual tools then WASM/subprocess (§5.12). |
| R8 | **Observability features need DBA priv / licensed packs** | Medium | License/priv detection; Statspack fallback; capability reporting (§5.11). |
| R9 | **Compliance over-claim** | High | fsync-before-execute durable audit or drop the SOX/PCI framing (§5.13). |
| R10 | **Human-in-the-loop confirmation over long-held HTTP request** | Medium | Poll/Task pattern (return `CHALLENGE_REQUIRED`, agent polls) instead of holding a request open; document proxy/LB caveats (§7.2). |
| R11 | **Silent wrong numbers (NUMBER→f64)** | Critical | NUMBER→string default; type table; golden tests (§5.2). |
| R12 | **`rmcp` HTTP transport immaturity** (HTTP is day-one, so this is now front-loaded) | Medium-High | Pin `rmcp` + track advisories; **own/wrap the auth edge** rather than trusting the transport with security; poll/Task pattern for the step-up gate (no long-held request); prove stdio core first then layer HTTP on the same spine (§2.6,§3.1). |
| R13 | **All-levels mode: server is the boundary, not the DB** | High | Honest framing everywhere (§2.4,§5.5); fail-closed classifier + `SET TRANSACTION READ ONLY` + human step-up confirmation for every escalation + durable audit; offer hard-ceiling (least-privilege) mode for shared/untrusted use; `max_level` defaults to READ_ONLY (writes are opt-in per profile). |
| R14 | **Phase E never fires (perpetual in-place; the public `oraclemcp` artifact never ships)** — a subjective "when it's great" trigger + the day-to-day convenience of one workspace means inertia compounds against ever paying the split cost, defeating the §13 distribution goal | Medium-High | Replace the subjective trigger with the objective gate checklist in §15 Phase E; once gates are green, extraction is a *release blocker*, not optional; record a dated owner + target version. |
| R15 | **Engine-aware classifier consulted naïvely is fail-OPEN** — the engine defaults completeness to `Measured::Unmeasured` and models `EXECUTE IMMEDIATE` as an `OpaqueDynamic` edge, so "no write edge → Safe" passes the exact dynamic-DML attack it is sold to catch | Critical | §5.3 inverts the predicate: clear to `Safe` only on an explicit `ProvenReadOnly`; `Unmeasured`/`OpaqueDynamic`/unloaded/cycle → `Unknown` → side-effecting; trigger/VPD walk; `SideEffectOracle` port defaults to `Unknown`. |

---

## §17 Open Questions for the Maintainer

These are the few decisions where your direction could change the plan. Defaults are chosen; flag any you want changed.

**Resolved by the maintainer (locked in):**
- Both **stdio + Streamable HTTP(S)** day-one/v1; **OCI / Oracle Cloud (Autonomous DB)** required (P1-11).
- **One user, all levels** via runtime operating levels (§6.6) — starts `READ_ONLY`; `max_level` is a **per-target-DB** ceiling; production profiles are hard-capped + require a read-only DB user (three locks, §6.6).
- **Escalation gate = in-band human confirmation** (DCG-style prompt / selector via MCP elicitation). **Device-based out-of-band 2FA is OUT OF SCOPE.**
- **License = `Apache-2.0 OR MIT`** (uniform across the `plsql-intelligence` workspace; §2.3).
- **Names** `oraclemcp` / `oracle_*` confirmed fine.
- **Minimum Oracle = 19c** (§2.2); CI matrix 19c + 23ai (+ Autonomous DB).
- **Login/house-convention scripts are operator-supplied, never in the repo** — see §6.5/§8.4: per-profile `login_script` file or inline `login_statements`, plus runtime append via `oracle_session` (ALTER SESSION whitelist). The repo ships only an *example* profile template.
- **Build/packaging strategy (revised 2026-06-01):** **improve the MCP in place now** in the `plsql-intelligence` workspace; **extract to a separate `oraclemcp` repo + publish to crates.io + depend back only later**, gated on a quality judgment (Phase E), not a date. No day-one repo split, no premature publish. (§0)
- **Naming:** the **platform/repo stays `plsql-intelligence`**; **`oraclemcp` is the name of the MCP-server binary/crate** — the search-term-owning surface and future published artifact. Market as MCP, build as platform. (§0, §13)
- **Commercial model:** **none — fully permissive (`Apache-2.0 OR MIT`) throughout.** No FSL/source-available tier; the `plsql-mcp-pro` idea is dropped. The Phase-B split is for modularity/visibility, not licensing. (§0, §2.3)
- **v1 transport/cloud scope (reaffirmed 2026-06-01):** HTTP/OAuth/mTLS **and** OCI/Autonomous-DB stay **day-one in v1** (P1-9, P1-11) — *not* deferred to Phase E, despite the review's recommendation to defer. Internal sequencing is 1a (secure stdio core) → 1b (HTTP on the same rmcp spine); both land in v1. Accepted trade-off: a larger v1 surface and v1's security claim tracking `rmcp`'s HTTP advisories (R12). (§2.6, §15)

Remaining genuinely-open:
1. **Thin-mode R&D:** a pure-Rust Oracle Net driver would remove the Instant Client dependency but is a multi-quarter effort. Park as a long-term research track, or out of scope entirely?
2. **Beads:** still "no beads yet." When ready, §15 converts 1:1 into an epic-per-phase beads graph with the listed dependencies.

---

## §18 Appendices

### 18.1 Pinned dependency baseline (verify latest at implementation time)

| Concern | Crate / tool | Version (as of 2026-05) |
|---------|--------------|--------------------------|
| MCP SDK | `rmcp` (`modelcontextprotocol/rust-sdk`) | 1.7.x |
| Oracle driver | `oracle` (kubo/rust-oracle, ODPI-C 5.4.x) | 0.6.x |
| Oracle raw FFI escape hatch | `odpic-sys` | latest |
| Pool | `r2d2-oracle` (+ `bb8-oracle` alt) | latest |
| SQL parser | `sqlparser` (OracleDialect) | 0.62.x |
| Async runtime | `tokio` (multi-thread) | 1.x |
| HTTP | `axum` + `tower` + `tower-governor` | 0.8.x / latest |
| TLS | `rustls` + `tokio-rustls` | latest |
| OAuth2 RS | `tower-oauth2-resource-server` + `jsonwebtoken` | latest / 9.x |
| Secrets | `keyring` / `vaultrs` / `zeroize` | latest |
| Config | `figment` + `notify` + `arc-swap` | latest |
| Logging/metrics | `tracing` + `tracing-subscriber` + `opentelemetry`(+otlp) + `tracing-opentelemetry` | latest |
| Resilience | `failsafe-rs` + `backon` + `governor` | latest |
| Errors | `thiserror` + `anyhow` | latest |
| Concurrency | `dashmap` (token store) | 6.x |
| Test/build | `cargo-nextest`, `insta`, `cargo-fuzz`, `cargo-dist`, `cargo-auditable`, `cargo-deny` | latest |
| Oracle client (runtime) | Oracle Instant Client Basic/Basic Light | 19c–23ai |

### 18.2 Key Oracle dictionary views & packages used

`ALL_/DBA_/USER_OBJECTS`, `…_SOURCE`, `…_PROCEDURES`, `…_ARGUMENTS`, `…_DEPENDENCIES`, `…_CONSTRAINTS`/`…_CONS_COLUMNS`, `…_INDEXES`/`…_IND_COLUMNS`, `…_SYNONYMS`, `…_ERRORS`, `…_TRIGGERS`, `…_TAB_COLUMNS`, `…_IDENTIFIERS`/`…_STATEMENTS` (PL/Scope), `DBMS_METADATA.GET_DDL`/`GET_DEPENDENT_DDL`, `EXPLAIN PLAN`+`DBMS_XPLAN.DISPLAY[_CURSOR]`, `V$DATABASE`/`V$OPTION`/`V$PARAMETER`/`V$LICENSE` (capability/standby/license probes), `DBMS_APPLICATION_INFO` (session tagging), `SET TRANSACTION READ ONLY`, Unified Auditing (`CREATE AUDIT POLICY`/`UNIFIED_AUDIT_TRAIL`), Statspack (free AWR alternative).

### 18.3 Primary sources (from the research phase)

MCP spec (`2025-11-25`, `2026-07-28-rc`), `modelcontextprotocol/rust-sdk` (rmcp), MCP Authorization (OAuth 2.1 / RFC 9728 / 8707 / 9207), Tool Annotations blog; ODPI-C 6.0.0 docs (`oracle/odpi`), `kubo/rust-oracle`, `r2d2-oracle`/`bb8-oracle`; Oracle SQLcl 25.2 MCP, `oracle/mcp`, `crystaldba/postgres-mcp`, `bytebase/dbhub`, `danielmeppiel/oracle-mcp-server`, `tannerpace/mcp-oracle-database`; `sqlparser` OracleDialect; Oracle Unified Auditing best-practice guide; Oracle MFA (Jul-2025 DBRU/23ai), Kerberos/RADIUS/IAM docs; `totp-rs`/`webauthn-rs`/`tower-oauth2-resource-server`/`vaultrs`/`keyring`; `tracing`/OpenTelemetry-Rust/`deadpool`/`governor`/`failsafe-rs`/`backon`/`figment`/`cargo-auditable`/`cargo-dist`; Anthropic "Writing Tools for Agents", Microsoft "Tool-Space Interference", arxiv "MCP Tool Descriptions Are Smelly"; DCG (Destructive Command Guard) philosophy. Full URLs were captured during research and can be re-expanded on request.

---

*End of plan. Begin implementation at Phase 0 (§15). The invariants in §5 are acceptance gates, not suggestions.*
