# D2: Final parser-backend decision (evidence-based, supersedes D1)

> **Decision: GO `antlr4rust`** (in-process, `plsql-parser-antlr`). Confirms
> D1's direction, now backed by spike evidence that its blocker is a
> **bounded** fix, not a fundamental wall. The former `java-antlr` spike was
> retired from the active workspace on 2026-06-28.
>
> **Date:** 2026-05-18 · **Supersedes:** the "measured without a built jar"
> caveat in `D1-backend-tournament-result.md` (which is hereby annotated
> "superseded by D2").

## Why D1 needed re-deciding

D1 chose `antlr4rust` but its `--features antlr-codegen` build **did not
compile** (14 errors) and the engine never actually called any
`ParseBackend` (`plsql-engine/src/lib.rs:643` calls the shallow
text-scanner `lower_source`). So D1's "✅ builds in-tree" was true only for
the grammar-files-only mode. D2 re-checked the `antlr4rust` blocker against
real spike evidence and retired the Java worker line instead of carrying it
as a hidden fallback.

## Evidence

| Dimension | `antlr4rust` (chosen) |
|---|---|
| Spike | `_spike/antlr4rust-errors.md` (commit 5aca071) |
| Blocker verdict | **BOUNDED**: 14(+~5) errors = 4 mechanical `build.rs` post-process patches; no antlr-rust-beta size limitation found |
| Runtime deps | none (pure Rust, in-process) |
| Remaining work at decision time | 4 build.rs patches + `ParseBackend` impl + engine wiring |

## Rationale (honest, evidence-weighted)

1. Spike 0.1 proved `antlr4rust` handles the 10K-line grammar once 4
   mechanical post-process rules are added; the blocker is bounded rather
   than architectural.
2. The real-Oracle grammar gaps (`$IF` placement, reserved-id-as-identifier,
   SQL*Plus/`QUIT`, APEX export format) come from the shared `.g4` and so
   must be handled in the shared grammar/preprocessor path, not by retaining
   a second backend. They are handled by a shared SQL*Plus preprocessor +
   targeted grammar patches + honest degrade-with-diagnostic for the
   irreducible remainder. The §0 correctness criterion is *truthful*, not
   *100 %-parse*, so this is acceptable and correct.
3. `antlr4rust` is strictly lower total complexity and risk: no JVM, no jar
   distribution, no subprocess, no wire protocol, in-process speed. Minimum
   work to a real backend.

## Consequence

Phase 1 proceeds as **1A (antlr4rust)**: apply the 4 `build.rs` patches →
green `--features antlr-codegen` build → `Antlr4RustBackend: ParseBackend`
(lossless tape) → wire `plsql-engine` off `lower_source` onto
`parse_with_backend`. The shared SQL*Plus-preprocessor + grammar-patch
workstream lifts the parse rate for the §0 / Phase-4 private estate proof.
`java-antlr` (PARSE-000B/C/D) stays NO-GO and has been removed from the
active workspace; reviving it requires a new decision record and bead set.
