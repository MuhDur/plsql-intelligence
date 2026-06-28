# D1: Parser Backend Tournament — Result

> ⚠️ **SUPERSEDED by [`D2-backend-final.md`](D2-backend-final.md) (2026-05-18).**
> D1's "builds in-tree" for `antlr4rust` held only in grammar-files-only
> mode; `--features antlr-codegen` did not compile and the engine never
> called any `ParseBackend`. D2 re-decides on fresh spike evidence
> (`_spike/`): still GO `antlr4rust`, now with proof the blocker is a
> bounded 4-patch fix. Read D2 for the operative decision.

> **Status:** DECIDED — 2026-05-16 (PLSQL-PARSE-000C)
> **Decision:** **GO `antlr4rust`** as the production parser backend.
> **`java-antlr`:** RETIRED — it is no longer an active fallback
> candidate after the 2026-06-28 workspace removal.
> **Depends on:** PLSQL-PARSE-000 (trait + conformance suite),
> PLSQL-PARSE-000A (antlr4rust backend), PLSQL-PARSE-000B (Java
> subprocess backend), PLSQL-PARSE-000D (neutral wire protocol).
> **Supersedes the OPEN decision in:** `D1-parser-backend-spike.md`.

## 1. Contenders

| Backend | Crate | State in this repo |
|---|---|---|
| `antlr4rust` | `plsql-parser-antlr` | In-process; the working default. Drives lowering for the whole pipeline. |
| `java-antlr` | n/a (retired) | Retired on 2026-06-28 after the backend tournament loser was removed from the workspace. Historical notes below describe the removed fallback candidate. |

The current workspace contains only the `antlr4rust` implementation. The
historical Java worker spike used the same `ParseBackend` boundary, but it
is no longer shipped or selectable.

## 2. Go/No-Go matrix

Honest reporting (R13): dimensions are marked **measured** (from
committed, re-runnable test evidence), or **deferred** (cannot be
measured here without a built Java jar — *not* fabricated). The `java-antlr`
column is historical pre-retirement evidence, not an active release path.

| Dimension | `antlr4rust` | `java-antlr` |
|---|---|---|
| **Builds in-tree** | ✅ measured — workspace builds; `cargo test -p plsql-parser-antlr` green | ✅ measured — crate builds; 14 tests green. Worker **jar build deferred** (own toolchain bead). |
| **Panic-rate on adversarial input** | ✅ measured — `recover.rs`, `snapshot_constructs.rs`, corpus harness exercise NUL/huge/unterminated with zero panics | ✅ measured — `adversarial_inputs_never_panic` (NUL, 100k, unterminated) → typed degradation, zero panics |
| **Span stability / lossless tape** | ✅ measured — snapshot + recovery tests assert stable byte spans | ⏸ deferred — token-tape decode is PARSE-000D's contract; needs a real worker to exercise |
| **Perf (parse throughput)** | ⏸ deferred — no statistically-valid benchmark bead run yet (corpus-bench exists) | ⏸ deferred — requires the jar; subprocess RTT will be measured in the benchmarking bead |
| **Memory** | ⏸ deferred — `plsql-engine doctor --memory` measures *artifact* size (PERF-002); process RSS profiling is a separate bead | ⏸ deferred — requires the jar |
| **Portability** | ✅ measured — pure Rust, no external runtime | ⚠ measured — needs a JVM (`java` present here: OpenJDK 17) **and** a built grammar jar (absent) |
| **API isolation (R20)** | ✅ measured — backend-internal ANTLR types never escape the trait | ✅ measured — `r20_isolation_no_java_or_antlr_identifier_in_serialized_shape` asserts the wire shape leaks no Java/ANTLR identifier |
| **Graceful degradation** | n/a (in-process) | ✅ measured — every failure mode → one typed `PARSE-JAVA-UNAVAILABLE` diagnostic, never a fabricated AST |

## 3. Decision

**`antlr4rust` is GO** as the production backend: it builds with
no external runtime, is the only contender with a working
end-to-end parse path in-tree, has zero measured panic-rate on
adversarial input, and keeps backend internals off the public
API. Every downstream crate already consumes it.

**`java-antlr` is retired.** Its former Rust integration and neutral wire
contract were useful spike evidence, but the crate is removed from the
workspace and the backend is not selectable. A future revival would require
a new decision and new beads; it is **not** production-eligible until all of
the following historical flip criteria are re-established:

- [ ] A Java ANTLR PL/SQL grammar worker jar is built and
      committed/distributable (Apache-2.0-compatible grammar).
- [ ] The worker speaks PARSE-000D's wire protocol and the Rust
      side reconstructs a CST from the token tape (no AST shape
      over the wire).
- [ ] It passes the *same* PARSE-000 conformance fixture set as
      `antlr4rust`, byte-for-byte on spans.
- [ ] A statistically-valid benchmark (≥30 samples, paired)
      shows parse throughput + subprocess RTT within an
      acceptable budget vs `antlr4rust`.
- [ ] Memory (peak RSS) profiled and within budget.
- [ ] Zero panic-rate on the adversarial corpus (already true
      for the degradation path; must remain true for the live
      path).

After the 2026-06-28 retirement, the system ships single-backend
(`antlr4rust`). Reintroducing a Java fallback would be a new
tournament/revival decision, not a hidden configuration switch.

## 4. Why this is safe to decide now

The `ParseBackend` trait keeps backend internals isolated: if a future
backend is approved, consumers should not need API churn. Deciding GO
`antlr4rust` now unblocks the release line (PLSQL-RELEASE-001) without
carrying the retired Java worker as hidden product surface.
