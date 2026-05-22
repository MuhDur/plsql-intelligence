# plsql-parser-antlr

ANTLR4-based PL/SQL parser backend. Layer 1.

## Purpose

Implements `plsql_parser::ParseBackend` using the vendored PL/SQL grammar
from `antlr/grammars-v4`. R20 mandates that no downstream crate sees
ANTLR types directly — this crate is the boundary that lets us swap to a
different backend without touching consumer code.

## Status

- **Grammar files** vendored from `antlr/grammars-v4` under Apache-2.0.
  See `LICENSE-GRAMMARS.md`.
- **Codegen** is feature-gated (`--features antlr-codegen`): when enabled,
  `build.rs` runs the antlr4rust Java tool to emit `OUT_DIR/plsql{lexer,
  parser,parserlistener}.rs`, which are then included as private modules.
- **Text-scanning pre-parser** at `src/lower/mod.rs` recognises top-level
  `CREATE [OR REPLACE]` declarations and emits `AstDecl` nodes today. It
  will be superseded by ANTLR parse-tree lowering once the generated code
  compiles cleanly.
- **Error recovery** at `;` and `/` boundaries lives in `src/recover.rs`
  (`PLSQL-PARSE-009`).

## Surface

| Module | Purpose |
|--------|---------|
| `lower` | Pre-parser that produces `plsql_parser::Ast` from source text |
| `recover` | Statement-boundary recovery + `Diagnostic` emission |

## Pointers

- Source: `crates/plsql-parser-antlr/src/`
- Plan: `plan.md` §7 (Layer 1), §4 (R20 — parser backend isolation), §17.5
- Sibling: `crates/plsql-parser/` (backend-independent surface types)
