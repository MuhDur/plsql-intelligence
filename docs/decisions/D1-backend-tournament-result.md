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
> PLSQL-PARSE-000A (antlr4rust backend), and the now-retired
> PLSQL-PARSE-000B/000D Java spike line.
> **Supersedes the OPEN decision in:** `D1-parser-backend-spike.md`.

## 1. Contenders

| Backend | Crate | State in this repo |
|---|---|---|
| `antlr4rust` | `plsql-parser-antlr` | In-process; the working default. Drives lowering for the whole pipeline. |
| `java-antlr` | n/a (retired) | Removed from the workspace on 2026-06-28. It is not a fallback, selectable backend, or shipped artifact. |

The current workspace contains only the `antlr4rust` implementation. The
historical Java worker spike used the same `ParseBackend` boundary, but it
is no longer shipped or selectable.

## 2. Go/No-Go matrix

Honest reporting (R13): dimensions are marked **measured** (from
committed, re-runnable test evidence), or **deferred** when the project has
not run the evidence gate yet. The retired `java-antlr` path is deliberately
absent from the matrix because it is no longer an active release path.

| Dimension | `antlr4rust` |
|---|---|
| **Builds in-tree** | ✅ measured — workspace builds; `cargo test -p plsql-parser-antlr` green |
| **Panic-rate on adversarial input** | ✅ measured — `recover.rs`, `snapshot_constructs.rs`, corpus harness exercise NUL/huge/unterminated with zero panics |
| **Span stability / lossless tape** | ✅ measured — snapshot + recovery tests assert stable byte spans |
| **Perf (parse throughput)** | ⏸ deferred — no statistically-valid benchmark bead run yet (corpus-bench exists) |
| **Memory** | ⏸ deferred — `plsql-engine doctor --memory` measures *artifact* size (PERF-002); process RSS profiling is a separate bead |
| **Portability** | ✅ measured — pure Rust, no external runtime |
| **API isolation (R20)** | ✅ measured — backend-internal ANTLR types never escape the trait |

## 3. Decision

**`antlr4rust` is GO** as the production backend: it builds with
no external runtime, is the only contender with a working
end-to-end parse path in-tree, has zero measured panic-rate on
adversarial input, and keeps backend internals off the public
API. Every downstream crate already consumes it.

**`java-antlr` is retired.** Its former spike evidence is no longer part of
the source tree, the crate is removed from the workspace, and the backend is
not selectable. A future revival would require a new decision and new beads;
there are no dormant flip criteria or hidden worker-jar tasks in this release
line.

After the 2026-06-28 retirement, the system ships single-backend
(`antlr4rust`). Reintroducing a Java fallback would be a new
tournament/revival decision, not a hidden configuration switch.

## 4. Why this is safe to decide now

The `ParseBackend` trait keeps backend internals isolated: if a future
backend is approved, consumers should not need API churn. Deciding GO
`antlr4rust` now unblocks the release line (PLSQL-RELEASE-001) without
carrying the retired Java worker as hidden product surface.
