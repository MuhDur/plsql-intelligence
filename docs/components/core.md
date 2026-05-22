# plsql-core

Shared primitive types for every other crate in the workspace. Layer 0.

## Purpose

`plsql-core` owns the cross-cutting types that downstream crates depend on
without ever calling each other directly. By centralising them here we keep
the dependency graph shallow (Layer 1+ all hit Layer 0 first, nothing else)
and the public API auditable from a single crate.

See plan.md §6.2 for the Layer 0 architecture rationale.

## Surface

| Type | Purpose |
|------|---------|
| `FileId` | Stable opaque identifier for a source file across runs |
| `Span` / `Position` | Byte + line/column source ranges |
| `Severity` | `Error`, `Warn`, `Note`, `Info` |
| `Diagnostic` | Code + severity + message + spans + `UnknownReason` tag |
| `UnknownReason` | Discriminated reason every blind spot carries (R13) |
| `Confidence` | `High`, `Medium`, `Low`, `Opaque` — graded certainty |
| `Evidence` | Provenance an edge or fact can attach |
| `SymbolInterner` | Bidirectional `SymbolId ↔ String` table |
| `ObjectName` / `MemberName` / `SymbolId` | Typed-ID newtypes |
| `OracleVersion` / `OracleFeature` | Capability matrix used by the parser dialect filter |

## Invariants

- **R13 — no silent uncertainty.** Every analysis pathway that cannot prove
  a fact emits an `UnknownReason` instead of pretending it knows. The
  reason kind is enumerated, never a free-form string.
- **R20 — backend-isolated types.** No type in `plsql-core` leaks ANTLR or
  any other parse-backend's identity. Downstream code stays portable across
  parser implementations.
- **No I/O.** Every type in `plsql-core` is plain data; loaders and writers
  live in `plsql-store`, `plsql-output`, etc.

## Pointers

- Source: `crates/plsql-core/src/`
- Plan: `plan.md` §6.2 (Layer 0 components), §4 (R-rules), §22 (verification
  standards)
- Consumers: every other workspace crate
