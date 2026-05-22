# D2: Final parser-backend decision (evidence-based, supersedes D1)

> **Decision: GO `antlr4rust`** (in-process, `plsql-parser-antlr`). Confirms
> D1's direction, now backed by spike evidence that its blocker is a
> **bounded** fix, not a fundamental wall. `java-antlr` remains a documented
> fallback only.
>
> **Date:** 2026-05-18 · **Supersedes:** the "measured without a built jar"
> caveat in `D1-backend-tournament-result.md` (which is hereby annotated
> "superseded by D2").

## Why D1 needed re-deciding

D1 chose `antlr4rust` but its `--features antlr-codegen` build **did not
compile** (14 errors) and the engine never actually called any
`ParseBackend` (`plsql-engine/src/lib.rs:643` calls the shallow
text-scanner `lower_source`). So D1's "✅ builds in-tree" was true only for
the grammar-files-only mode. Both backend paths were re-spiked with real
evidence (`docs/decisions/_spike/`).

## Evidence

| | `antlr4rust` (chosen) | `java-antlr` (fallback) |
|---|---|---|
| Spike | `_spike/antlr4rust-errors.md` (commit 5aca071) | `_spike/java-antlr-evidence.md` (commit ef5b89f) |
| Blocker verdict | **BOUNDED**: 14(+~5) errors = 4 mechanical `build.rs` post-process patches; no antlr-rust-beta size limitation found | **VIABLE-WITH-DEGRADATION**: parses the private estate well but needs jar build + JVM + unbuilt PARSE-000D wire decode |
| Private estate parse profile | same grammar → same profile (~99% TRG, ~84% PKG, ~100% DDL, APEX degrades) | ~99% TRG / 84% PKG / 100% DDL / 0% APEX |
| Runtime deps | none (pure Rust, in-process) | JVM + packaged jar + subprocess |
| Remaining work | 4 build.rs patches + `ParseBackend` impl + engine wiring | jar build + reproducible packaging + full wire-protocol decode + JVM dep |

## Rationale (honest, evidence-weighted)

1. The only reason to prefer `java-antlr` was "handles the 10K-line grammar
   reliably." Spike 0.1 proves `antlr4rust` **also** handles it once 4
   mechanical post-process rules are added, neutralizing that advantage.
2. The real-Oracle grammar gaps (`$IF` placement, reserved-id-as-identifier,
   SQL*Plus/`QUIT`, APEX export format) come from the shared `.g4` and so
   affect **both** backends identically. They are handled by a shared
   SQL*Plus preprocessor + targeted grammar patches + honest
   degrade-with-diagnostic for the irreducible remainder (APEX f*.sql ≈
   0.09 % of the estate). The §0 correctness criterion is *truthful*, not
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
`java-antlr` (PARSE-000B/C/D) stays NO-GO, documented, untouched.
