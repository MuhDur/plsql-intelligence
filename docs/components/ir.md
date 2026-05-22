# plsql-ir

Typed semantic intermediate representation. Layer 2.

## Purpose

`plsql-ir` is the first semantic layer above the parser's lossless CST.
Where the CST captures every trivia byte for source-fidelity round-tripping,
the IR collapses that into a typed `SemanticModel` that downstream layers
(`plsql-symbols`, `plsql-privileges`, `plsql-sqlsem`, `plsql-flow`,
`plsql-facts`, `plsql-depgraph`) consume. Lowering happens in this crate.

## Surface

| Type | Purpose |
|------|---------|
| `SemanticModel` | Per-file semantic root produced by lowering |
| `Declaration` | Enum over package / procedure / function / type / trigger etc. |
| `Statement` | Body-level statement IR (assign, call, control-flow, EXECUTE IMMEDIATE) |
| `Expression` | Typed expression IR |
| `LoweringDiagnostic` | Anything we couldn't lower lands here, never panics |

## Lowering

- Driven from `plsql_parser::Ast` — no ANTLR types reach the IR (R20).
- DDL statements (CREATE / ALTER / DROP / GRANT) are lowered for dependency
  analysis even though the lowered shape is intentionally simpler than the
  full SQL grammar.
- Bodies of routines (assignments, control flow, EXECUTE IMMEDIATE) gain
  typed shapes; unrecognised fragments become `Statement::Unknown` with an
  `UnknownReason` carrying the source span (R13).

## Pointers

- Source: `crates/plsql-ir/src/`
- Plan: `plan.md` §9 (Layer 2 Semantic IR), §4 (R-rules)
- Downstream: `plsql-symbols`, `plsql-privileges`, `plsql-sqlsem`,
  `plsql-flow`, `plsql-facts`, `plsql-depgraph`
