# USR Loop: Uncertainty-Sourced Repair (Self-Healing Coverage Flywheel)

> **Ref:** `PLSQL-USR-001` · **Status:** SPEC (complete; build-ready) · **Date:** 2026-05-19
> **For agentic workers:** implement via superpowers:subagent-driven-development. Every claim is gate-verified by re-run (compliance Axiom 16), never self-report. The bar is §3's conformance gate; it does not move.

**Goal (one sentence):** Turn the engine's honest-uncertainty exhaust into a self-healing pipeline that, every time the tool is run on a real Oracle estate, produces *proven, privacy-clean, behavior-preserving* parser/lowering patches, so coverage compounds with use instead of with engineering headcount.

**Why this is the single radically-innovative + accretive thing (each claim is a hard requirement, not marketing):**

- **Radical:** the data that competitors discard as failure (parse errors, `UnknownReason`) becomes the engine's training/repair signal. The tool is the *only* one that records uncertainty as a typed, provenanced, minimizable, **offline** artifact (R13). That design, made for honesty, is the precondition no competitor has.
- **Accretive:** every estate the tool touches permanently raises coverage for **all** future runs, across all customers, forever. Accuracy is a function of adoption, not staffing. This is a compounding moat.
- **Working & useful:** it directly closes the tool's only honest weakness (the ~1% real-Oracle/APEX dialect tail; "proven on synthetic, asymptotic on real"). The asymptote closes itself.
- **Non-negotiable framing:** it must *never* trade honesty or correctness for coverage. Suppressing a diagnostic to "fix" a gap is the exact `oracle-bh4p` dishonesty and is auto-rejected by the gate (§3.G7). Coverage gains are only real if matched by measured semantic-extraction gains.

---

## 1. The Seven Invariants (violating any one makes the feature net-negative; they are the spec's spine)

1. **I-PRIVACY (AGENTS.md C1/C2/C5/C6, absolute).** No customer or private estate byte ever leaves the estate. Every artifact the loop persists (fixtures, gap records, candidate diffs, ledger) is a *re-synthesized, structurally-equivalent* minimal reproduction, never copied source. Enforced by `plsql_support::redaction_delta` + `scrub_literals::strict()` and *proven* per artifact (§3.G7); a single failed privacy proof aborts the whole run and quarantines nothing to disk.
2. **I-NO-REGRESSION.** A candidate patch lands only if it is provably behavior-preserving on the entire existing corpus (lossless round-trip + conformance + golden isomorphism + §0 monotonic non-regression). Diagnosis is automatic; *landing requires the proof gate to pass*: "propose, prove, then land," never auto-merge unproven.
3. **I-NO-GAMING.** A coverage gain is valid only if accompanied by a commensurate, measured rise in extracted semantics (dep-graph edges / facts / `extracted_semantics_ratio`) for the targeted gap signature. A patch that reduces diagnostics without raising extraction (i.e. suppression) is auto-rejected. Honest-uncertainty posture (`oracle-bh4p` machinery) must be preserved or improved, never weakened.
4. **I-DETERMINISM.** Same estate + same engine commit → byte-identical gap records, fixtures, signatures, and candidate set. No wall-clock, no RNG, no map-iteration order in any persisted artifact. Re-runnable and diffable.
5. **I-PROVENANCE.** Every gap record, fixture, candidate, gate verdict, and landed patch is content-addressed and traces 3 hops: estate-run → diagnostic(code+rule+span) → minimized fixture → candidate diff → gate result. The ledger is append-only and auditable.
6. **I-ISOLATION (R20).** Patches may only touch the `.g4` grammar, `plsql-parser-antlr` codegen/`tree_lower`/`lower`, or the typed-degradation classifier. They may never make a downstream crate depend on ANTLR-generated types, and never alter public `Ast`/`ParseBackend` contracts in a non-additive way.
7. **I-MONOTONIC-VALUE.** A tracked accretion metric (§4) is monotonic non-decreasing across releases. A release that lowers it fails CI (tripwire). This is what makes "accretive" a verified property, not a hope.

---

## 2. Architecture & data flow (every component maps to a real crate; nothing new is invented that an existing module already does)

```
 real estate (private estate / customer; READ-IN-PLACE, never copied)
        │  plsql-engine analyze  (already emits typed diagnostics + UnknownReason + provenance)
        ▼
 [A] GAP CAPTURE  ── filter AnalysisRun.diagnostics for repairable classes:
        PARSE-ANTLR4RUST-001 (grammar gap), IR_UNCLASSIFIED_DECL (lowering gap),
        IR_DDL_NOT_LOWERED (dispatch gap), UnknownReason::* (semantic gap)
        → emits GapRecord (schema §2.1): provenance only, NO source bytes
        ▼
 [B] MINIMIZE + PRIVACY-PROVE  (plsql_support::{plan_minimize, shrink_lines,
        scrub_literals::strict, record_redaction_delta + verify})
        → MinFixture: smallest input that still triggers the same (code,rule,signature),
          with every literal/identifier re-synthesized; redaction-delta PROVES 0 leak
        ▼
 [C] CLUSTER/DEDUP  signature = (diag_code, antlr_rule_path, token-shape hash).
        N estate occurrences → 1 GapCluster (keeps ≤K representative MinFixtures)
        ▼
 [D] PATCH PROPOSER  (LLM-assisted; output is a CANDIDATE DIFF, never a merge)
        chooses exactly one repair class per cluster:
          (g) grammar `.g4` delta   (l) tree_lower/lower dispatch extension
          (d) typed-degradation: convert an Unknown into a typed-known UnknownReason
              (honest "we recognise this and choose not to deep-parse it, with the reason")
        ▼
 [E] CONFORMANCE GATE  (§3, the heart; 9 ordered all-must-pass stages)
        ▼ pass                                   ▼ fail
 [F] LAND + LEDGER                        [F'] QUARANTINE-AS-OPEN-BEAD
   apply diff on current branch,            file a provenanced bead with the
   add MinFixture to the corpus +           MinFixture + the failing stage;
   a pinned regression test, append          NEVER weaken the gate to pass.
   to the append-only Ledger, re-measure
        ▼
 [G] ACCRETION TRIPWIRE (§4): monotonic metric updated; CI fails if it dropped
```

Orchestrator: a new tool `tools/usr-loop/` (binary `usr-loop`, R10/R11: ships `--robot-json` + `doctor`). Library logic: a new crate `plsql-accretion` (Layer 5; depends only on plsql-core/-support/-parser/-parser-antlr/-engine; **never** the reverse, which preserves layering). Reuses, does not reimplement: `minimize_repro`, `shrink`, `scrub_literals`, `redaction_delta`, `UnknownReason`, `Diagnostic`, `RobotJsonEnvelope`/`SchemaDescriptor`, `conformance.rs`, the `fuzz/` targets, `scripts/estate_correctness.sh`.

### 2.1 GapRecord schema (versioned robot-JSON envelope, `plsql.usr.gap_record` v1)

Required fields, all derived, none containing source:
`signature` (content hash of code+rule+token-shape), `diag_code`, `antlr_rule_path` (the rule the parser was in), `unknown_reason` (the typed `UnknownReason` variant or null), `span_shape` (token-kind sequence, never text), `estate_run_id` (content hash of the AnalysisRun, not the estate), `occurrence_count`, `first_seen_commit`, `min_fixture_id` (content hash of the synthetic fixture), `repair_class` (`g`|`l`|`d`|`unrepairable`), `privacy_proof_id` (the redaction-delta manifest hash). Determinism: serialization is sorted-key; the same estate+commit reproduces every byte.

### 2.2 MinFixture (the privacy-critical artifact)

A `.sql` snippet, ≤ a hard cap (default 4 KB), that (a) triggers the byte-identical `(diag_code, antlr_rule_path, signature)`, (b) has every identifier/literal replaced by a deterministic synthetic token via `scrub_literals::strict()` + structural re-synthesis, (c) carries a `RedactionDeltaManifest` proving, by reconstruction diff, that **zero** original characters survive beyond grammar keywords/punctuation. Minimization uses `plan_minimize` + `shrink_lines` with a `ReproOracle` whose predicate is "same signature still fires." A MinFixture that cannot be proven privacy-clean is **discarded, not stored** (I-PRIVACY beats coverage).

---

## 3. THE CONFORMANCE GATE: `scripts/usr_gate.sh <candidate-diff>` (committed; CI-wired; the bar)

Nine stages, **strictly ordered, every one must PASS**, fail-closed (any non-pass → REJECT, candidate becomes a bead, gate is never weakened to admit it). Each stage prints `GATE Gn: PASS|FAIL <evidence>`. Exit 0 only if all nine PASS.

| # | Stage | Exact check | PASS criterion | REJECT (auto) |
|---|-------|-------------|----------------|---------------|
| **G1** | Builds | `rustup run nightly cargo build -p plsql-parser-antlr --features antlr-codegen` + `cargo build --workspace` | both exit 0 | any compile error |
| **G2** | Lossless round-trip | the `antlr4rust_backend` round-trip suite over the **full** `corpus/` + every prior MinFixture | `reconstruct(tape)==input` byte-for-byte, 100% | one mismatch |
| **G3** | Backend conformance | `crates/plsql-parser/tests/conformance.rs` | identical-behavior contract holds for all fixtures | any divergence |
| **G4** | Golden isomorphism | every committed golden artifact re-rendered | byte-identical, OR a golden delta that is *explicitly listed, reviewed, and semantically justified in the candidate* | any silent/unjustified golden churn |
| **G5** | Never-panic + fuzz | the 6 `fuzz/` targets, short campaign + the full regression corpus incl. new MinFixture | 0 crashes, 0 panics | any crash |
| **G6** | §0 monotonic non-regression | `scripts/estate_correctness.sh` (and any registered estate proof) | RESULT: PASS **and** dep_graph edges ≥ baseline **and** facts ≥ baseline **and** `extracted_semantics_ratio` ≥ baseline | any metric below baseline, or harness FAIL |
| **G7** | Anti-gaming + honesty | diff vs the targeted gap: the cluster's diagnostics drop **iff** extracted semantics rose by ≥ the count of resolved occurrences; completeness `posture` not weakened (no Clean-where-uncertain); for repair-class `d`, the Unknown is replaced by a *typed* `UnknownReason` (still surfaced), never silenced | diagnostics fell with no commensurate extraction rise; OR posture weakened; OR a gap was suppressed not resolved |
| **G8** | Privacy | `verify_redaction_delta` over the candidate + every MinFixture it adds; grep candidate+fixtures for any estate-derived identifier set | 0 surviving original bytes; manifest verifies | any leak signal → REJECT **and** the run aborts (I-PRIVACY) |
| **G9** | Test pins behavior | the candidate's added regression test, under `cargo mutants` scoped to the patched fns | the new test FAILS if the patch is reverted (mutation-killed) | test passes on reverted code (vacuous test) |

**Gate properties (themselves spec, themselves tested by `tests/gate_selftest.rs`):**
- **Fail-closed & immutable:** the gate script is content-pinned (`sha256` in `plsql-accretion`'s manifest); a run whose gate sha ≠ pinned sha aborts. Changing the gate requires a human-reviewed commit + a deliberate sha bump (mirrors compliance `☖ STAKE-RUBRIC`).
- **No partial credit:** 8/9 is REJECT. There is no "mostly passes."
- **Determinism:** two runs of the gate on the same candidate + same commit produce identical verdicts (asserted).
- **Adversarial self-test:** `gate_selftest.rs` feeds the gate three known-bad candidates (a suppression-only patch that must die at G7, a privacy-leaking fixture that must die at G8 + abort, a coverage-up-but-round-trip-breaking patch that must die at G2) and asserts each is rejected at the named stage. If the gate ever fails to reject any of these, the feature is broken by definition.

---

## 4. Accretion metric & monotonic tripwire (makes "accretive" a *verified* property)

Tracked in an append-only `accretion_ledger.jsonl` (its own content-addressed history, gitignored from churn but committed at release tags):

- **`coverage_index`** = `extracted_semantics_ratio` over a *frozen public benchmark set* (`corpus/`-derived, never private estate code) **+** `distinct_resolved_gap_signatures` (count of signature classes the loop has permanently closed).
- **Tripwire:** `scripts/accretion_tripwire.sh` asserts `coverage_index(HEAD) ≥ coverage_index(last_release_tag)`. Wired into CI as a required check. A release that lowers it fails (I-MONOTONIC-VALUE). Recoveries (a closed signature regressing) are themselves filed as gaps and fed back into the loop.
- **Public dashboard line:** the README/CHANGELOG carry `coverage_index` over time; the compounding is visible and auditable, never asserted by vibes.

---

## 5. Definition of "100% implemented": a single re-runnable acceptance proof

`scripts/usr_acceptance.sh` (committed; the DoD; mirrors the `estate_correctness.sh` discipline). It is the **bootstrapping end-to-end proof**: it does not assert the loop "looks built," it makes the loop close a *real, currently-open private estate gap* and proves every invariant held. Steps, all must PASS:

1. Pick a real open gap class from a live private estate run, one of the **54 `PARSE-ANTLR4RUST-001`** or **992 `IR_DDL_NOT_LOWERED`** classes measured 2026-05-18 (the harness reports them; the script selects the highest-occurrence one deterministically).
2. Run [A]→[E] end to end. Assert: a GapRecord was produced with full provenance; a MinFixture was produced **and privacy-proven** (G8 logic standalone); a candidate diff was proposed in a valid repair class.
3. Run the full §3 gate on that candidate. Assert exit 0 (all 9 PASS), OR, if the proposer's first candidate fails the gate, assert it was quarantined as a provenanced bead with the failing stage named, then iterate (the loop is *allowed* to need >1 candidate; it is *not* allowed to land an unproven one).
4. Assert the targeted gap's signature count **strictly decreased** on a fresh private estate run, `extracted_semantics_ratio` **strictly increased**, posture honesty preserved, and `scripts/estate_correctness.sh` still RESULT: PASS (§0 never regresses).
5. Assert `accretion_tripwire.sh` shows `coverage_index` strictly up.
6. Assert the Ledger appended exactly one landed entry, fully content-addressed, and the added regression test is mutation-killed (G9 standalone).
7. Assert `gate_selftest.rs` (the adversarial trio) is green; the gate provably still rejects suppression / privacy-leak / round-trip-break.
8. Run twice; assert byte-identical artifacts (I-DETERMINISM).

`usr_acceptance.sh` exit 0 == the feature is 100% implemented, working, useful (it closed a real gap), accretive (metric rose), safe (gate held), private (G8 held), and honest (G7 held). Anything less is not done. No milestones; the script is the contract.

---

## 6. File / crate structure (concrete, real paths, R20-safe)

| Path | Responsibility |
|------|----------------|
| `crates/plsql-accretion/` (new, Layer 5) | loop library: GapRecord, MinFixture builder (wraps support infra), clusterer, proposer interface, gate runner, ledger. No reverse deps. |
| `crates/plsql-accretion/src/gap.rs` | GapRecord schema + capture from `AnalysisRun` diagnostics |
| `crates/plsql-accretion/src/fixture.rs` | MinFixture: `plan_minimize`+`shrink`+`scrub_literals::strict`+`record/verify_redaction_delta` |
| `crates/plsql-accretion/src/cluster.rs` | signature + dedup |
| `crates/plsql-accretion/src/proposer.rs` | candidate-diff interface (LLM-backed impl behind a trait so it's testable with a deterministic stub) |
| `crates/plsql-accretion/src/gate.rs` | typed runner that shells the 9 stages, parses verdicts, enforces fail-closed + sha-pin |
| `crates/plsql-accretion/src/ledger.rs` | append-only content-addressed ledger + accretion index |
| `tools/usr-loop/src/main.rs` | binary `usr-loop`; subcommands `scan`/`propose`/`gate`/`land`/`doctor`; `--robot-json` global |
| `scripts/usr_gate.sh` | §3 conformance gate (sha-pinned) |
| `scripts/usr_acceptance.sh` | §5 DoD proof |
| `scripts/accretion_tripwire.sh` | §4 monotonic CI check |
| `crates/plsql-accretion/tests/gate_selftest.rs` | adversarial gate trio (§3) |
| `.github/workflows/usr.yml` | CI: gate-selftest + tripwire on every PR; full acceptance nightly |
| `docs/decisions/D3-usr-repair-class-policy.md` | when each repair class (g/l/d) is permitted; "d is last resort, must stay honest" |

---

## 7. Failure modes & rollback (each has a defined, tested response)

- **Proposer produces no valid candidate** → cluster filed as `unrepairable`-for-now bead with MinFixture; loop continues; honest, not a failure.
- **Candidate fails gate** → quarantined bead naming the failing stage; **never** retried by weakening the gate.
- **Privacy proof fails (G8)** → entire run aborts, in-memory artifacts dropped, alert; nothing persisted (I-PRIVACY is fail-safe, not fail-open).
- **Landed patch later regresses a closed signature** → the regression *is* a new gap; tripwire catches the index drop and CI fails until re-resolved; `git revert` of the offending landed patch is the immediate rollback (the Ledger maps signature→commit for one-command revert).
- **Gate sha mismatch** → run aborts (immutability guard).
- **Gate self-test (adversarial trio) ever green-passes a bad candidate** → CI red, feature disabled until fixed; this is the canary that the safety rail itself is intact.

---

## 8. Test matrix (what to run; layered; the gate is the apex)

- **Unit** (per `plsql-accretion` module): GapRecord determinism; signature stability; clusterer dedup; MinFixture predicate ("same signature still fires"); ledger append/content-address.
- **Property** (`proptest`): for any input that triggers a diagnostic, the MinFixture triggers the *same* `(code,rule,signature)` and is ≤ cap.
- **Privacy** (must, AGENTS.md): metamorphic. For any synthetic-but-realistic input with planted secret-shaped literals, the MinFixture's `verify_redaction_delta` shows 0 surviving planted bytes; fuzz the scrubber.
- **Metamorphic** (testing-metamorphic): a landed grammar/lowering patch's newly-extracted semantics are invariant under comment/whitespace insertion and identifier rename.
- **Mutation** (`cargo mutants`): every landed regression test must be mutation-killed (this is G9, also run in CI).
- **Conformance/golden/fuzz/§0**: unchanged existing suites, re-run inside the gate (G2–G6).
- **Adversarial gate self-test** (`gate_selftest.rs`): the suppression / privacy-leak / round-trip-break trio, the single most important test; if it ever fails the whole feature is unsafe by definition.
- **End-to-end acceptance** (`usr_acceptance.sh`): the §5 bootstrapping proof on a real private estate gap, run nightly + at release.

---

## 9. Non-goals & honest limits (stated, not hidden; R13 applies to the feature itself)

- Not an auto-merge bot: it proposes proven candidates; landing on `main` follows the repo's normal review for the diff (the *proof* is automatic and complete; the *decision* is gated by that proof, optionally + human review per `D3`).
- Will not resolve genuinely-ambiguous Oracle dialect forms by guessing; those become honest typed `UnknownReason` (repair-class `d`): coverage of *honesty*, never of *fabrication*.
- The accretion compounds on *signature classes*, not raw counts; a long tail of singleton estate quirks may stay open and that is reported, not masked.
- It cannot exceed the grammar's theoretical reach without grammar patches; class-`g` patches are real grammar work, gated like everything else: slower, but sound.

---

## 10. Build order (each phase ships working, testable software; no phase is a milestone-stub)

1. **P1 GapRecord + capture** (`gap.rs`, schema, `usr-loop scan`). Exit: deterministic GapRecords from a live private estate run; unit+property green.
2. **P2 MinFixture + privacy proof** (`fixture.rs`). Exit: privacy metamorphic + property suites green; a MinFixture provably triggers the same signature and leaks 0 bytes.
3. **P3 Cluster + ledger.** Exit: dedup correct; ledger append/content-address tested.
4. **P4 The gate** (`gate.rs`, `usr_gate.sh`, `gate_selftest.rs`). Exit: the adversarial trio is rejected at the exact named stages; gate is fail-closed + sha-pinned. **This phase is the safety rail; nothing after it lands without it.**
5. **P5 Proposer** (`proposer.rs`, trait + LLM impl + deterministic stub). Exit: produces valid candidate diffs for the top private estate gap classes; stub-driven gate runs deterministically.
6. **P6 Land + tripwire + acceptance.** Exit: `usr_acceptance.sh` exit 0 on a real private estate gap; `accretion_tripwire.sh` wired in CI; CHANGELOG/README carry the `coverage_index`.

Done = `usr_acceptance.sh` exits 0, `gate_selftest.rs` green, tripwire green, workspace `clippy -D warnings`/`cargo test` green, `estate_correctness.sh` still PASS, working tree clean. Verified by independent re-run, not self-report.
