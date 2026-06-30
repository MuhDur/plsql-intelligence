# Parser Component — Architecture Reference

> **Crate:** `plsql-parser` (Layer 1) + `plsql-parser-antlr` (Layer 1, backend)
> **Status:** Active implementation — foundational types complete, ANTLR backend in progress
> **Source:** `crates/plsql-parser/src/`, `crates/plsql-parser-antlr/src/`

This document describes the public API surface of the PL/SQL parser frontend:
the `ParseBackend` trait, the typed AST, the lossless CST/token tape, the
`Spanned` invariant, the visitor pattern, and error recovery semantics.

---

## Table of Contents

1. [Design overview](#1-design-overview)
2. [ParseBackend trait](#2-parsebackend-trait)
3. [Token tape and trivia](#3-token-tape-and-trivia)
4. [Concrete syntax tree (CST)](#4-concrete-syntax-tree-cst)
5. [Typed AST](#5-typed-ast)
6. [Spanned trait](#6-spanned-trait)
7. [SourceMap](#7-sourcemap)
8. [Error recovery](#8-error-recovery)
9. [Visitor pattern](#9-visitor-pattern)
10. [Lowering from parse tree to AST](#10-lowering-from-parse-tree-to-ast)
11. [Module layout](#11-module-layout)
12. [Quality gates](#12-quality-gates)

---

## 1. Design overview

The parser frontend is split into two crates:

| Crate | Layer | Purpose |
|-------|-------|---------|
| `plsql-parser` | 1 | Public types: `ParseBackend` trait, AST, CST, TokenTape, Visitor |
| `plsql-parser-antlr` | 1 | ANTLR backend implementation, grammar files, lowering, recovery |

**Backend isolation (R2 / R20):** No ANTLR-generated types or grammar rule
names escape `plsql-parser-antlr`. The public parser surface is our lossless
CST/token tape plus typed AST. Downstream crates depend only on
`plsql-parser`.

**Lossless contract:** The token tape is the source of truth for
round-tripping. Every token and trivia element carries a byte-offset span.
The AST is a *semantic* projection — it is NOT required to preserve
whitespace, comments, or exact delimiter positions.

```
reconstruct(token_tape(input)) == input   // byte-for-byte
```

This contract is enforced by proptest in `tests/round_trip.rs`.

---

## 2. ParseBackend trait

Defined in `plsql-parser/src/lib.rs`. Every parser backend implements this
trait. Backend-internal types (ANTLR parse trees, grammar rule names) are
strictly private to the implementing crate.

```rust
pub trait ParseBackend: Send + Sync {
    /// Human-readable backend name (e.g. "antlr4rust").
    fn name(&self) -> &'static str;

    /// Parse source text and return a BackendParseResult.
    fn parse(&self, input: &str, file_id: FileId, opts: &ParseOptions) -> BackendParseResult;
}
```

**Contract:**
- MUST NOT panic on any input (adversarial or otherwise).
- MUST populate `cst.token_tape` such that `reconstruct(tape) == input`.
- MUST emit at least one diagnostic per syntax error.
- MUST set `recovered = true` if error recovery was used.

### ParseOptions

```rust
pub struct ParseOptions {
    pub oracle_version: OracleTargetVersion,  // default: Oracle19c
    pub recovery: RecoveryMode,               // default: RecoverAtStatementBoundary
}
```

`OracleTargetVersion` covers Oracle 11g through 23ai. `RecoveryMode`
controls error-recovery aggressiveness:
- `FailFast` — stop at first error
- `RecoverAtStatementBoundary` — skip to next `;` or `/` (default)
- `AggressiveRecovery` — recover at any plausible boundary (fuzzing)

### BackendParseResult / ParseResult

`BackendParseResult` is the backend's raw output. The public `parse_with_backend`
function wraps it into `ParseResult` which adds the originating `FileId`:

```rust
pub struct ParseResult {
    pub file_id: FileId,
    pub cst: ConcreteSyntaxTree,
    pub ast: Ast,
    pub diagnostics: Vec<Diagnostic>,
    pub metrics: ParseMetrics,
    pub recovered: bool,
}
```

`ParseMetrics` tracks `total_tokens`, `trivia_count`, `diagnostic_count`,
`recovery_count`, and `source_bytes`.

### Conformance test suite

Every backend must pass `plsql_parser::tests::conformance::run_conformance`.
The suite verifies: no panics on empty/whitespace/simple input, metrics
correctness, and token tape non-empty for non-empty input.

---

## 3. Token tape and trivia

Defined in `plsql-parser/src/tokens.rs`.

### Token

```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,  // raw source text, verbatim
}
```

### TokenKind

Coarse discriminator — backends map their internal token vocabulary into
these kinds. The mapping is backend-private (R20).

**Literals:** `StringLiteral`, `NumericLiteral`, `QuotedIdentifier`
**Keywords:** `Keyword`, `BuiltIn`
**Identifiers:** `Identifier`
**Punctuation:** `Semicolon`, `Slash`, `Dot`, `Comma`, `LParen`, `RParen`,
`Assign` (`:=`), `Arrow` (`=>`), `Concat` (`||`), `Operator`
**Special:** `IncludeDirective`, `StatementTerminator`, `Unknown`

### Trivia

Trivia is whitespace, comments, and other non-token source text that must
be preserved for lossless round-tripping:

```rust
pub enum Trivia {
    Whitespace(String),
    LineComment(String),
    BlockComment(String),
}
```

### TriviaTable

Maps each token index to the trivia that **precedes** it. Index 0 holds
leading trivia; index `tokens.len()` holds trailing trivia.

```rust
pub struct TriviaTable {
    pub leading: Vec<Vec<Trivia>>,
}
```

### TokenTape

Ordered sequence of tokens. Combined with `TriviaTable`, supports
byte-for-byte source reconstruction:

```rust
pub struct TokenTape {
    pub tokens: Vec<Token>,
}

impl TokenTape {
    pub fn reconstruct(&self, trivia: &TriviaTable) -> String;
}
```

**Lossless property:** For any file that lexes successfully,
`tape.reconstruct(&trivia) == original_source` byte-for-byte. This is
verified by 8 proptest cases in `tests/round_trip.rs`.

---

## 4. Concrete syntax tree (CST)

Defined in `plsql-parser/src/ast.rs`.

```rust
pub struct ConcreteSyntaxTree {
    pub root: CstNodeId,
    pub token_tape: TokenTape,
    pub trivia: TriviaTable,
    pub source_map: SourceMap,
}
```

The CST is the **lossless** representation. Every delimiter, keyword, and
trivia is represented with byte-offset spans. Round-tripping goes through
the CST / token tape, not through the AST.

`CstNodeId` is an opaque `u32` identifier — backend-local, NOT stable
across parse invocations or backends.

```rust
pub struct CstNodeId(pub u32);
```

---

## 5. Typed AST

The AST is the **semantic** (lossy) projection. Whitespace, comments, and
exact delimiter positions are NOT preserved.

### SourceFile

Root of the typed AST. Holds the file span and top-level declarations:

```rust
pub struct SourceFile {
    pub span: Span,
    pub declarations: Vec<AstDecl>,
}
```

### AstDecl

Top-level PL/SQL declaration. Covers the full set the parser must recognize
(plan §7.2). The `Unknown` variant satisfies R13 — no uncertainty is
silently dropped.

```rust
pub enum AstDecl {
    PackageSpec { name: String, span: Span },
    PackageBody { name: String, span: Span },
    Procedure  { name: String, span: Span },
    Function   { name: String, span: Span },
    Trigger    { name: String, span: Span },
    View       { name: String, span: Span },
    TypeSpec   { name: String, span: Span },
    TypeBody   { name: String, span: Span },
    Ddl        { kind: String, span: Span },
    Unknown    { span: Span },
}
```

**Every variant carries a `span` field** — enforced by the `Spanned` trait
(§6). New AST node types added by downstream beads must implement `Spanned`.

### Ast

Top-level wrapper:

```rust
pub struct Ast {
    pub root: SourceFile,
    pub source_map: SourceMap,
}
```

---

## 6. Spanned trait

Every AST node must implement this trait. It returns the node's source
`Span` — the byte-offset range in the original source file.

```rust
pub trait Spanned {
    fn span(&self) -> Span;
}
```

**Contract:**
- `span()` MUST return the tightest bounding span covering all tokens
  belonging to this node.
- For nodes spanning multiple non-contiguous ranges, `span()` returns the
  *primary* span. Related spans are carried via `SpanLabel` in `Evidence`
  or `Diagnostic`.

**Implementations:**
- `SourceFile` — returns the file-level span
- `AstDecl` — all 10 variants return their `span` field

The trait is object-safe (`&dyn Spanned` works).

---

## 7. SourceMap

Maps `CstNodeId`s to their source `Span`s. Side-table rather than
embedding spans in every CST node — compact arena, O(log n) lookups.

```rust
pub struct SourceMap {
    inner: BTreeMap<u32, Span>,
}

impl SourceMap {
    pub fn new() -> Self;
    pub fn insert(&mut self, node: CstNodeId, span: Span);
    pub fn get(&self, node: CstNodeId) -> Option<&Span>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

---

## 8. Error recovery

Defined in `plsql-parser-antlr/src/recover.rs`.

### Recovery semantics (plan §7.3)

PL/SQL in real-world codebases contains syntax errors from copy-paste
accidents, vendor extensions, and embedded scripts. The parser must:

1. Recover at statement boundaries (`;` and `/` delimiters)
2. Continue past a malformed block to parse the next block
3. Surface a `Diagnostic` per error with source span
4. Never panic on adversarial input

### recover_to_statement_boundary

```rust
pub fn recover_to_statement_boundary(
    bytes: &[u8],
    start: usize,
    file_id: FileId,
) -> RecoveryResult;

pub struct RecoveryResult {
    pub recovered_at: usize,
    pub diagnostic: Option<Diagnostic>,
}
```

**Behavior:**
- Skips forward from `start` to the next `;` or `/` on its own line
- Tracks `BEGIN`/`END` depth — semicolons inside nested blocks are skipped
- Skips single-line comments (`--`), block comments (`/* */`), and string
  literals (including escaped quotes `''`)
- Emits `PARSE-RECOVERY-001` diagnostic with the error span
- Returns position after the boundary character

**Integration with lowerer:** When the lower module encounters text it
cannot classify, it calls `recover_to_statement_boundary` to skip to the
next statement and emit a diagnostic, then continues parsing.

---

## 9. Visitor pattern

Defined in `plsql-parser/src/visit.rs`.

### Visitor trait

```rust
pub trait Visitor: Sized {
    fn visit_source_file(&mut self, source_file: &SourceFile);
    fn visit_decl(&mut self, decl: &AstDecl);
    fn visit_package_spec(&mut self, name: &str, span: &Span);
    fn visit_package_body(&mut self, name: &str, span: &Span);
    fn visit_procedure(&mut self, name: &str, span: &Span);
    fn visit_function(&mut self, name: &str, span: &Span);
    fn visit_trigger(&mut self, name: &str, span: &Span);
    fn visit_view(&mut self, name: &str, span: &Span);
    fn visit_type_spec(&mut self, name: &str, span: &Span);
    fn visit_type_body(&mut self, name: &str, span: &Span);
    fn visit_ddl(&mut self, kind: &str, span: &Span);
    fn visit_unknown(&mut self, span: &Span);
}
```

Every method has a default implementation that recurses into children via
the corresponding `walk_*` function. Override only the methods you care
about.

### Walk module

`visit::walk` provides default traversal:

```rust
pub fn walk_source_file<V: Visitor>(visitor: &mut V, source_file: &SourceFile);
pub fn walk_decl<V: Visitor>(visitor: &mut V, decl: &AstDecl);
```

### Entry point

```rust
pub fn visit_source_file<V: Visitor>(visitor: &mut V, source_file: &SourceFile);
```

As the AST grows (PARSE-004 through PARSE-011), new `visit_*` / `walk_*`
pairs will be added.

---

## 10. Lowering from parse tree to AST

Defined in `plsql-parser-antlr/src/lower/mod.rs`.

### Entry point

```rust
pub fn lower_source(source: &str, file_id: FileId) -> Ast;
```

Scans source text for top-level `CREATE [OR REPLACE]` declarations and
produces one `AstDecl` per declaration found.

### Per-kind lowering

| Kind | Pattern | AstDecl variant |
|------|---------|-----------------|
| Package spec | `CREATE [OR REPLACE] PACKAGE <name>` | `PackageSpec` |
| Package body | `CREATE [OR REPLACE] PACKAGE BODY <name>` | `PackageBody` |
| Procedure | `CREATE [OR REPLACE] PROCEDURE <name>` | `Procedure` |
| Function | `CREATE [OR REPLACE] FUNCTION <name>` | `Function` |
| Trigger | `CREATE [OR REPLACE] TRIGGER <name>` | `Trigger` |
| View | `CREATE [OR REPLACE] VIEW <name>` | `View` |
| Type spec | `CREATE [OR REPLACE] TYPE <name>` | `TypeSpec` |
| Type body | `CREATE [OR REPLACE] TYPE BODY <name>` | `TypeBody` |
| Other DDL | `CREATE <kind> ...` | `Ddl { kind }` |

### Name extraction

Handles simple identifiers and quoted identifiers (`"My_Name"`).

### Statement boundary detection

Tracks `BEGIN`/`END` depth for proper statement boundary detection. PL/SQL
statements end at `;` (most statements) or `/` on its own line (SQL*Plus
terminator, e.g. after type bodies).

### R13 compliance

Text that cannot be classified as a known declaration kind is lowered to
`AstDecl::Ddl` with the DDL kind string — never silently dropped.

---

## 11. Module layout

```
plsql-parser/
├── src/
│   ├── lib.rs          # ParseBackend trait, ParseOptions, ParseResult
│   ├── ast.rs          # Ast, SourceFile, AstDecl, SourceMap, Spanned, CstNodeId
│   ├── tokens.rs       # Token, TokenKind, TokenTape, Trivia, TriviaTable
│   └── visit.rs        # Visitor trait, walk module
├── tests/
│   ├── conformance.rs  # Backend conformance test suite + StubBackend
│   └── round_trip.rs   # proptest: reconstruct(token_tape(s)) == s
└── Cargo.toml

plsql-parser-antlr/
├── grammars/
│   ├── PlSqlLexer.g4   # 2,618 lines — vendored from antlr/grammars-v4
│   └── PlSqlParser.g4  # 10,011 lines — vendored from antlr/grammars-v4
├── tools/
│   └── antlr4-4.13.3-SNAPSHOT-complete.jar # ANTLR4 Rust codegen tool
├── src/
│   ├── lib.rs          # Generated code modules (behind antlr-codegen feature)
│   ├── lower/mod.rs    # Source text → Ast lowering
│   └── recover.rs      # Error recovery at statement boundaries
├── build.rs            # ANTLR codegen invocation (gated behind feature)
└── Cargo.toml
```

---

## 12. Quality gates

| Gate | Threshold | Status |
|------|-----------|--------|
| Round-trip proptest | `reconstruct(token_tape(s)) == s` for all generated inputs | ✅ 8 cases |
| Backend conformance | All backends pass `run_conformance` | ✅ StubBackend passes |
| Clippy | `-D warnings` | ✅ Clean |
| Test count | 37 tests in plsql-parser, 33 in plsql-parser-antlr | ✅ |
| Spanned invariant | Every `AstDecl` variant carries `span` | ✅ Verified by test |
| R13 compliance | No uncertainty silently dropped | ✅ `Unknown` variant exists |

---

## References

- **Plan:** `plan.md` §7 (Layer 1 — Parser Core)
- **R-rules:** R2 (backend-independent API), R12 (provenance), R13
  (no silent uncertainty), R20 (parser backend isolation)
- **ANTLR grammar source:** <https://github.com/antlr/grammars-v4/tree/master/sql/plsql>
- **Grammar license:** Apache-2.0 (per-file headers)
- **antlr4rust runtime:** <https://github.com/antlr4rust/antlr4>
- **Decision artifact:** `docs/decisions/D1-parser-backend-spike.md`
