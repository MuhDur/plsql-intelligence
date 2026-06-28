# Real Parser Backend + Completeness Honesty + Serve Loop + Private Estate Correctness: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development to implement this plan. Steps use checkbox (`- [ ]`) tracking. Orchestrator verifies every subagent claim by re-running (compliance Axiom 16); never trust self-reports.

**Goal:** Make `plsql-intelligence` actually deliver its thesis on *real* Oracle PL/SQL: wire a real parser backend so semantic extraction is non-empty on a real estate (#1), make the completeness/Trust Block tell the truth (#2), make `plsql-mcp serve` reachable (#3), and prove correctness over the local private estate including edge cases (#4), fully, no milestones/deferrals.

**Architecture:** A `ParseBackend` (trait already defined in `plsql-parser`) becomes the engine's real parse path, replacing the shallow text-scanner call at `plsql-engine/src/lib.rs:643`. The backend is chosen by evidence in Phase 0; the Java fallback branch has since been retired, so the active path is the in-process `antlr4rust` backend. The completeness report is recomputed from real extraction signals. The serve loop reuses the existing `mcp_protocol::handle_request_line` + `tcp::serve`. Correctness is proven by a private estate harness asserting the *honest correctness criterion* below.

**Tech Stack:** Rust (workspace, stable + nightly for fuzz), ANTLR4 (`.g4` grammars present), `antlr-rust 0.3.0-beta`, and the existing `plsql-parser` / `plsql-parser-antlr` crates.

---

## 0. Honest correctness criterion (the definition "fully working / proven over the private estate" is measured against)

A *tolerant offline analyzer's* correctness is NOT "perfectly parse every APEX/dialect construct" (an open-ended tail no tool achieves). Per the project's own §1.5 evidence-UX thesis, correct = **truthful**:

1. **Robustness:** zero panics / zero non-zero-exit crashes across **all 4,251 private estate files** (edge cases included). Verified by running `plsql-engine analyze` over the whole estate and by fuzzing every untrusted boundary.
2. **Non-empty semantics:** on the private estate the real backend produces a **non-empty dependency graph and fact store** (today both are 0). Concretely: dep_graph edges > 0, fact_store facts > 0, and a meaningful fraction of the 4,132 objects resolve references. Exact thresholds set in Phase 0 from a measured baseline (evidence, not a guess).
3. **Honest uncertainty (the §1.5 contract):** the `CompletenessReport`/Trust Block MUST reflect reality. It may not report `unresolved_references: 0 / skipped_token_ratio: 0.0 / 0 unknowns` while emitting tens of thousands of diagnostics and an empty graph. Where the backend cannot understand a construct, that is recorded as a typed `UnknownReason`, and the headline aggregates them honestly.
4. **Reachability:** `plsql-mcp serve` runs a real MCP loop (stdio + TCP) that an MCP client can drive end-to-end.
5. **Lossless round-trip preserved:** `reconstruct(token_tape) == input` byte-for-byte for every private estate file the backend accepts (the `ParseBackend` hard contract).

"Proven" = an automated, re-runnable harness (`estate_correctness.sh` + gated tests) asserting 1–5 with captured evidence, plus an independent fresh-eyes re-verification (compliance Axiom 16). Private estate source is **never** copied into the repo (AGENTS.md C5/C6); the harness reads it in place from the directory named by the `PLSQL_PRIVATE_ESTATE` environment variable.

---

## File Structure

| File | Responsibility | Phase |
|------|----------------|-------|
| `docs/decisions/D1-backend-tournament-result.md` (modify) | Re-open & re-decide with Phase-0 evidence | 0 |
| `docs/decisions/D2-backend-final.md` (create) | Record the evidence-based final backend choice | 0 |
| `crates/plsql-parser-antlr/build.rs` + generated glue (modify) | If antlr4rust wins: make codegen compile (the 14 errors) | 1A |
| `crates/plsql-parser-antlr/src/backend.rs` (create) | `impl ParseBackend` over the antlr-rust parser → lossless tape + AST | 1A |
| `crates/plsql-engine/src/lib.rs:~643` (modify) | Replace `lower_source(...)` with `parse_with_backend(chosen_backend, ...)`; map CST/AST into the IR pipeline | 1C |
| `crates/plsql-core/src/lib.rs` (CompletenessReport) (modify) | Recompute from real signals; never false-clean | 2 |
| `crates/plsql-engine/src/lib.rs` (completeness assembly) (modify) | Feed real diagnostic/unknown counts into the report + Trust Block | 2 |
| `crates/plsql-mcp/src/main.rs` + `serve` module (modify/create) | Real stdio + TCP MCP loop via `handle_request_line`/`tcp::serve` | 3 |
| `scripts/estate_correctness.sh` (create) | The re-runnable correctness harness (criterion 1–5) | 4 |
| `crates/*/tests/` + `fuzz/fuzz_targets/` (create/extend) | Regression + fuzz coverage for every fix | 1–4 |
| `CHANGELOG.md`, `docs/ARCHITECTURE.md` (modify) | Record reality after each phase | all |

---

## Phase 0: Evidence-based backend decision (NOT a milestone; the decisive engineering step)

The user mandates the *best* choice on honest evidence. D1 said antlr4rust but its codegen does not compile today (14 errors). Java 17 is present. Decide by measurement, then go all-in on the winner.

### Task 0.1: Characterize the antlr4rust failure

- [ ] Run `rustup run nightly cargo build -p plsql-parser-antlr --features antlr-codegen 2>&1 | tee /tmp/antlr4rust_errs.txt`; classify the 14 errors (generated-code bug class: missing trait impls / lifetime / `antlr-rust` beta API drift / grammar-action Rust syntax). Output: `docs/decisions/_spike/antlr4rust-errors.md` with the error taxonomy + an honest tractability verdict (bounded patch set vs. fundamental beta limitation), citing specific errors.

### Task 0.2: Retired java-antlr path

This branch was retired on 2026-06-28. Do not build, preserve, or revive a
Java worker jar from this plan; `docs/decisions/D2-backend-final.md` is the
operative decision, and any future Java backend would require a fresh decision
record and bead set.

### Task 0.3: Decide & record (D2)

- [ ] Write `docs/decisions/D2-backend-final.md`: the chosen backend, with the evidence from 0.1, the risk assessment, and the explicit re-open of D1. Update `D1-backend-tournament-result.md` with a "superseded by D2" banner. Optionally triangulate the decision via `/multi-model-triangulation` (high-stakes, the skill's intended use).

> Superseded heuristic: the old plan considered a Java ANTLR reference target
> as a possible fallback. D2 and the 2026-06-28 retirement resolved this in
> favor of the in-process `antlr4rust` backend.

---

## Phase 1: Wire a real ParseBackend through the engine

Branch A (1A) **or** Branch B (1B) per the D2 decision; then 1C is common.

### 1A (if antlr4rust): make codegen compile + implement the backend
- [ ] Fix the generated-code compile errors (patch `build.rs` post-processing and/or pin a working `antlr-rust` revision); `cargo build -p plsql-parser-antlr --features antlr-codegen` is green; commit per fixed error class with a regression test that the crate builds with the feature.
- [ ] Implement `crates/plsql-parser-antlr/src/backend.rs`: `struct Antlr4RustBackend; impl ParseBackend`. Drive the generated lexer/parser, build the **lossless `token_tape`** (assert `reconstruct(tape) == input` in a unit test on ≥10 corpus fixtures), produce the `Ast`/CST the IR consumes, emit ≥1 diagnostic per syntax error, set `recovered`. TDD: failing round-trip test → impl → green.

### 1B (retired): Java ANTLR fallback

The Java ANTLR fallback branch is no longer an active workspace path. Do not recreate it for this plan; D2 keeps the in-process `antlr4rust` backend as the operative parser route.

### 1C (common): engine integration
- [ ] Replace `crates/plsql-engine/src/lib.rs:~643` `let ast = plsql_parser_antlr::lower::lower_source(&source, file_id);` with the real `parse_with_backend(&chosen_backend, ...)`; map its CST/AST into the existing IR → symbols → facts → depgraph pipeline so dep_graph/fact_store populate. Keep `lower_source` available as an explicit fallback only when the backend degrades a file (honest, diagnosed).
- [ ] Conformance: `crates/plsql-parser/tests/conformance.rs` passes for the chosen backend across the canonical fixture set (identical behavior contract). Synthetic corpus (`make demo-no-db`) still green; the hero `DROP COLUMN` demo still produces the correct what-breaks set via the real backend.

---

## Phase 2: Completeness / Trust-Block honesty (oracle-bh4p, P1)

- [ ] Failing test: construct an `AnalysisRun` with N>0 diagnostics + empty dep_graph and assert the `CompletenessReport` does NOT report `unresolved_references==0 && skipped_token_ratio==0.0 && files_recovered==0` as "clean"; it must surface aggregate uncertainty (a `confidence`/`unknown` summary derived from real diagnostic + unresolved + degraded-file counts).
- [ ] Implement: recompute `CompletenessReport` (in `plsql-core`) + its assembly in `plsql-engine` from real signals (per-file degraded flag, unresolved-reference count, diagnostic volume, objects-with-extracted-semantics ratio). The Trust Block headline reflects it. Never false-clean.
- [ ] Close `oracle-bh4p` only with re-run evidence (compliance kernel): the private estate run's completeness now honestly reflects its 53K-diagnostic / (post-#1) graph reality.

---

## Phase 3: `plsql-mcp serve` real loop (PLSQL-MCP-002)

- [ ] Replace the `Command::Serve` "not yet implemented" stub: stdio loop = read line-delimited JSON-RPC from stdin → `mcp_protocol::handle_request_line(&line, &registry)` → write response line to stdout (diagnostics to stderr, Axiom 4). `--listen` path = the existing `tcp::serve` (already built/tested, oracle-k8ef).
- [ ] Tests: a stdio integration test (spawn the binary, pipe an `initialize` + `tools/list` + a real tool call, assert responses) and a TCP one (reuse the loopback pattern). `--robot-triage` health must report serve as implemented. Update `transport::is_transport_implemented`/capabilities accordingly.

---

## Phase 4: Prove correctness over the local private estate (incl. edge cases)

- [ ] Create `scripts/estate_correctness.sh`: runs `plsql-engine analyze "$PLSQL_PRIVATE_ESTATE"`, then asserts the §0 criterion via `jq`: exit 0, 0 panics across all 4,251 files, dep_graph edges > threshold, fact_store facts > threshold, completeness honestly non-clean, per-accepted-file round-trip spot-check. Emits a pass/fail report to `/tmp` (never commits private estate content). Re-runnable.
- [ ] Run it. For every panic/edge case found: minimize (re-synthesize a *generic anonymized* fixture, never private estate source, AGENTS.md C5/C6), add a regression test, fix, re-run. Iterate until zero panics and the semantic thresholds hold. This is the hardening loop; repeat until quiet (not a single pass).
- [ ] Extend the `fuzz/` targets' corpus discipline so the parser-backend path (not just the pre-parser) is fuzzed; run a campaign; triage→regress every crash.

---

## Phase 5: Improvement skills (mandatory, per the goal "improved with skills")

After Phases 1–4 are green, apply, with findings filed as beads + fixed (not just reported):
- [ ] `simplify-and-refactor-code-isomorphically` over the new backend + integration code.
- [ ] `multi-pass-bug-hunting` and `ubs` over the changed surface.
- [ ] `testing-conformance-harnesses` (backend conformance) + `testing-golden-artifacts` (private estate aggregate-metric golden, scrubbed) + `testing-metamorphic` (e.g. comment/whitespace-invariance of extracted semantics).
- [ ] `rust-unsafe-code-exorcist` re-check (workspace stays `forbid(unsafe_code)`).
- [ ] `beads-compliance-and-completion-verification` final pass: every bead this plan closes is verified by re-run, not self-report.

---

## Verification protocol (every phase)

Per-crate `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; full workspace `clippy -D warnings` + `cargo test --workspace` green before any phase is called done. Orchestrator independently re-runs every subagent's claimed gate (Axiom 16). One commit per coherent change with the Claude co-author trailer; `.beads/issues.jsonl` flushed+committed per closed bead; no push (no remote, per user). The private estate stays local-only.

## Self-review (spec coverage)

- #1 → Phase 0 + Phase 1 (backend decision + real ParseBackend + engine wiring). ✓
- #2 → Phase 2 (completeness honesty, closes oracle-bh4p). ✓
- #3 → Phase 3 (real serve loop). ✓
- #4 → Phase 4 (private estate correctness harness + hardening loop) + §0 criterion. ✓
- "improved with skills" → Phase 5. ✓
- "no half attempts / milestones" → §0 makes "done" a measurable, re-runnable, evidence-backed criterion, not a milestone; Phase 4 iterates until it holds. ✓
- Honesty (user demands evidence-based choices) → Phase 0 is an explicit evidence spike with a recorded decision; the §0 correctness criterion is itself the anti-spin guard (truthful uncertainty, not false-clean). ✓

**Open risk, stated honestly (not a deferral):** if Phase 0 evidence shows *neither* backend can reach §0.2 (non-empty semantics) on the private estate within the chosen approach, the correct response per the goal is to keep working the winning path (fix more codegen errors / extend the wire decode / grammar patches) until §0 holds, escalating effort, not lowering the bar. The bar (§0) does not move.
