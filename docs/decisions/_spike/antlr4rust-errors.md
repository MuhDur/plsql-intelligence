# Spike: antlr4rust 0.3.0-beta Compile Error Taxonomy

> **Status:** COMPLETE — 2026-05-18
> **Verdict:** BOUNDED — 4 classes of mechanical post-processing patches; no fundamental wall.
> **Toolchain:** `rustc 1.97.0-nightly (64a965e90 2026-05-11)`, `antlr-rust = "0.3.0-beta"`,
>   ANTLR 4.8 jar (`tools/antlr4-rust.jar`)
> **Workspace edition:** 2024

---

## 1. Build reproduction

```
rustup run nightly cargo build -p plsql-parser-antlr --features antlr-codegen 2>&1 \
  | tee /tmp/antlr4rust_full.txt
```

Result: codegen succeeds ("ANTLR codegen complete"), **14 compile errors** reported.
Generated files are written to `$OUT_DIR` (`/tmp/cargo-target/debug/build/plsql-parser-antlr-*/out/`):

| File | Lines | Size |
|------|-------|------|
| `plsqllexer.rs` | 24,292 | 1.74 MB |
| `plsqlparser.rs` | 284,280 | 15.4 MB |
| `plsqlparserlistener.rs` | 12,088 | 473 KB |

The files are included via `include!()` macros inside `mod generated { ... }` blocks in
`crates/plsql-parser-antlr/src/lib.rs`, which already applies `#![allow(warnings)]` and
`#![allow(clippy::all)]` at the module level.

---

## 2. Error taxonomy — the 14 visible errors

### Class A: Inner attribute `#![...]` not permitted in `include!()` context (12 errors)

**Count:** 4 in `plsqllexer.rs`, 7 in `plsqlparser.rs`, 1 in `plsqlparserlistener.rs` = **12 total**

**Sample:**
```
error: an inner attribute is not permitted in this context
 --> plsqllexer.rs:2:1
  |
2 | #![allow(dead_code)]
  | ^^^^^^^^^^^^^^^^^^^^
...
6 | use antlr_rust::atn::ATN;
  | ------------------------- the inner attribute doesn't annotate this `use` import
```

**Root cause:** The ANTLR 4.8 Rust codegen emits `#![allow(...)]` at the top of each generated
file (appropriate for standalone module roots). These files are included via `include!()` inside
a `mod` block in `lib.rs`. When `#![...]` appears inside an `include!()` that is itself inside
a `mod {}` block, Rust treats it as an inner attribute of the next item (the first `use`
statement), which is invalid. This is NOT an edition-2024 regression — the same error appears
in edition 2021. Confirmed with a minimal repro: `rustc --edition 2021` produces the identical
error.

**Affected locations:**
- `plsqllexer.rs` lines 2–5: `#![allow(dead_code)]`, `#![allow(nonstandard_style)]`,
  `#![allow(unused_imports)]`, `#![allow(unused_variables)]`
- `plsqlparser.rs` lines 2–8: `#![allow(dead_code)]`, `#![allow(non_snake_case)]`,
  `#![allow(non_upper_case_globals)]`, `#![allow(nonstandard_style)]`,
  `#![allow(unused_imports)]`, `#![allow(unused_mut)]`, `#![allow(unused_braces)]`
- `plsqlparserlistener.rs` line 1: `#![allow(nonstandard_style)]`

**Classification:** (d) post-processing gap in `build.rs`.

**Fix shape:** One-line change to `post_process()` in `build.rs`:
```rust
let content = content.replace("#![allow(", "#[allow(");
```
This is safe: no legitimate use of crate-level `#![...]` exists in the included files because the
module-level suppression is already applied by `lib.rs`'s `mod generated { #![allow(warnings)] }`.

---

### Class B: Broken `r#fn` field access — missing dot (1 reported error, 2 affected sites)

**Count:** 1 compiler error at line 81131 (a second occurrence at 81163 is unreachable because
the parser stops at the first syntax error in the function body).

**Sample:**
```
error: expected one of `.`, `;`, `?`, `}`, or an operator, found `r#fn`
     --> plsqlparser.rs:81131:68
      |
81131 |  cast_mut::<_,Disassociate_statisticsContext >(&mut _localctx)r#fn = Some(tmp.clone());
      |                                                                ^^^^ expected one of...
```

**Root cause:** The grammar has `fn = id_expression` labels (the grammar-v4 rule
`disassociate_statistics` at lines 2438–2439). ANTLR 4.8 generates field-access expressions
like `cast_mut::<...>(&mut ctx).fn = Some(...)`. The `build.rs` post-processor applies:
```rust
content.replace(".fn =", "r#fn =")
```
This strips the dot, producing `cast_mut(...)r#fn = Some(...)` — syntactically invalid (missing
the member-access operator).

**The struct field definition** (`pub r#fn:`) and struct initializer (`r#fn: None`) are fixed
correctly. Only the field-assignment sites are broken.

**Classification:** (d) post-processing gap — a bug in the existing `fix_fn_keyword_collisions`
function.

**Fix shape:**
```rust
// In fix_fn_keyword_collisions():
// WRONG:  content.replace(".fn =", "r#fn =")
// RIGHT:  content.replace(".fn =", ".r#fn =")
```
Two call sites will be fixed (81131 and 81163). The struct field definition (`pub r#fn:`) and
initializer (`r#fn: None`) replacements are already correct and unaffected.

---

### Class C: Doubled `Parser` in generated trait name (1 error)

**Count:** 1 error, 1 occurrence in the entire generated file.

**Sample:**
```
error[E0405]: cannot find trait `PlSqlParserParserContext` in this scope
      --> plsqlparser.rs:201620:14
       |
  5447 | pub trait PlSqlParserContext<'input>: ...
       | ----- similarly named trait `PlSqlParserContext` defined here
...
201620 |  impl<'input> PlSqlParserParserContext<'input> for Table_ref_aux_internalContextAll<'input>{}
```

**Root cause:** `table_ref_aux_internal` is the only rule in the grammar that uses labeled
alternatives (generating a `*ContextAll` enum). The ANTLR 4.8 Rust codegen has an isolated
template bug that emits `PlSqlParser` + `ParserContext` = `PlSqlParserParserContext` for the
`ContextAll` enum's trait impl, while all 14,013 other trait impl sites in the file correctly
emit `PlSqlParserContext`. This is a one-off codegen glitch, not a systemic issue.

**Classification:** (a) antlr-rust 0.3.0-beta API / codegen template glitch.

**Fix shape:**
```rust
// In post_process():
let content = content.replace("PlSqlParserParserContext", "PlSqlParserContext");
```

---

## 3. Hidden errors — the ~5 semantic errors behind the syntactic ones

These errors do not appear in the current build output because the Class A/B syntactic errors
prevent the compiler from reaching the affected code. They **will appear** after Classes A–C are
fixed.

### Class D: User-defined semantic predicates called on `BaseParser`/`BaseLexer` (hidden)

**Count:** 41 call sites across 5 distinct methods, producing approximately 5 unique E0599
errors (rustc emits one "no method named `X` for type `Y`" per unique `(method, receiver type)`
pair, not per call site).

**Affected methods and call counts:**

| Method | File | Calls |
|--------|------|-------|
| `recog.isVersion12()` | `plsqlparser.rs` | 27 |
| `recog.IsNotNumericFunction()` | `plsqlparser.rs` | 6 |
| `recog.isVersion11()` | `plsqlparser.rs` | 3 |
| `recog.isVersion10()` | `plsqlparser.rs` | 3 |
| `recog.IsNewlineAtPos(-4)` | `plsqllexer.rs` | 2 |

**Root cause:** The grammars-v4 PL/SQL grammar embeds semantic predicates that call user-defined
methods on the parser/lexer (Java convention: `{this.isVersion12()}?`). The `build.rs` correctly
rewrites `this.` to `recog.`. However, the `recog` parameter in the generated `*_sempred`
helper functions has type `&mut <Self as Deref>::Target`:

- For the parser: `&mut BaseParser<'input, PlSqlParserExt, I, PlSqlParserContextType, ...>`
- For the lexer: `&mut BaseLexer<'input, PlSqlLexerActions, Input, LocalTokenFactory>`

Neither `BaseParser` nor `BaseLexer` (external crate types) defines `isVersion12()`,
`isVersion11()`, `isVersion10()`, `IsNotNumericFunction()`, or `IsNewlineAtPos()`. These
methods are not in `antlr_rust::parser::BaseParser`'s inherent impl. There are no `@members`
blocks in the grammar that would generate stub implementations.

The grammars-v4 grammar author expected these methods to be provided by a Java subclass or an
ANTLR @members block. Neither was ported when the grammar was vendored.

**Classification:** (b) grammar embedded-action Rust syntax gap (user-defined predicates with
no stub implementation).

**Fix shape (bounded):** Add an extension trait in the post-processor that provides default stubs
for all 5 methods, implemented for both `BaseParserType` and the lexer's base type. The trait is
defined and implemented in the generated file itself — no orphan rule violation:

```rust
// Injected by build.rs post_process() at end of plsqlparser.rs:
trait PlSqlPredicates {
    fn isVersion12(&mut self) -> bool { true }
    fn isVersion11(&mut self) -> bool { true }
    fn isVersion10(&mut self) -> bool { true }
    fn IsNotNumericFunction(&mut self) -> bool { false }
}
impl<'input, I> PlSqlPredicates for BaseParserType<'input, I>
where
    I: antlr_rust::token_stream::TokenStream<'input, TF = LocalTokenFactory<'input>>
     + antlr_rust::TidAble<'input>
{}
```

```rust
// Injected by build.rs post_process() at end of plsqllexer.rs:
trait PlSqlLexerPredicates {
    fn IsNewlineAtPos(&mut self, _pos: isize) -> bool { false }
}
impl<'input, Input> PlSqlLexerPredicates for BaseLexer<'input, PlSqlLexerActions, Input, LocalTokenFactory<'input>>
where
    Input: antlr_rust::char_stream::CharStream<
        antlr_rust::input_stream::From<'input>
    >
{}
```

The default values (version predicates return `true`, `IsNotNumericFunction` returns `false`,
`IsNewlineAtPos` returns `false`) make the parser accept the maximum set of syntax. Strict
version-gating can be wired later by extending the extension trait or replacing with a runtime
flag.

---

## 4. Ecosystem assessment

### antlr-rust 0.3.0-beta (current)

- **Crates.io:** `antlr-rust = "0.3.0-beta"`, checksum `cfc6ab5...`
- **Last updated:** 2025-10-25 (per crates.io; epoch mismatch suggests metadata vs source)
- **Paired ANTLR jar:** 4.8 (vendored at `tools/antlr4-rust.jar`, confirmed via `java -jar` output: "ANTLR Parser Generator Version 4.8")
- **Status:** This is the crate pinned in `Cargo.lock`. It compiled successfully as a
  dependency (`libantlr_rust-*.rlib` present in `$CARGO_TARGET_DIR/debug/deps/`). No runtime
  API mismatch was detected — all types imported by the generated code (`ParserRecog`,
  `ParserNodeType`, `Listenable`, `VocabularyImpl`, etc.) exist in this version.

### antlr4rust 0.5.2 (fork, different crate name)

- **Crates.io:** `antlr4rust = "0.5.2"` (distinct crate from `antlr-rust`)
- **Homepage:** `https://github.com/antlr4rust/antlr4` — described as picking up where the
  original author left off
- **Paired ANTLR jar:** 4.13.2 (seen in generated file headers: `// Generated from CSV.g4 by ANTLR 4.13.2`)
- **Import style:** `use antlr4rust::...` (crate name contains digit, no underscore); **incompatible**
  with the generated code which uses `use antlr_rust::...`
- **Same inner-attr issue:** The `tests/gen/csvlexer.rs` and `csvparser.rs` in antlr4rust 0.5.2
  also emit `#![allow(...)]` at file top, so the same Class A problem would occur if integrated
  the same way (via `include!()`). The antlr4rust 0.5.2 test suite avoids this by compiling
  generated files as separate crates (not via `include!()` in a mod block).
- **Verdict:** Switching to antlr4rust 0.5.2 would require (a) a new ANTLR 4.13.2 Rust jar,
  (b) full regeneration of all 320K lines, (c) updating all imports in the generated code and
  in `lib.rs`, (d) re-solving the same Class D predicate problem. No benefit over fixing the
  current setup.

### git log evidence

```
11ba2c2  docs: D1 parser backend spike — antlr4rust codegen results (PLSQL-PARSE-000A)
53e5ad4  docs(decisions): D1 parser-backend tournament result — GO antlr4rust (PLSQL-PARSE-000C)
```

The original spike (PARSE-000A) identified 3 blockers (caseInsensitive, fn keyword, this.→recog.)
and declared them fixable. The tournament result (PARSE-000C) declared antlr4rust the GO backend.
The build.rs was authored to apply the `fn`/`this.` fixes. The inner-attribute and doubled-Parser
issues were not caught in the spike because the spike measured against a different include strategy
or compiler version. The predicate method gap was mentioned in the spike (Blocker 3, 14 embedded
actions) but the fix was not completed.

---

## 5. Verdict

**BOUNDED: ~4 concrete build.rs patches, here is the exact shape.**

All 14 current compile errors and the ~5 hidden E0599 errors are **mechanical post-processing
gaps** in `build.rs`. No fundamental antlr-rust-0.3.0-beta limitation was found:

1. The generated code structure (traits, lifetimes, generics, ATN deserialization) is correct.
2. The antlr-rust runtime compiled successfully as a dependency.
3. Only 1 type of semantic error exists beyond the syntactic ones: 5 user-defined predicate
   methods missing from `BaseParser`/`BaseLexer`.
4. All errors are fixable by extending `post_process()` in `build.rs` — no grammar surgery,
   no jar replacement, no antlr-rust version upgrade required.

### Concrete patches needed (all in `build.rs::post_process`)

| # | Error class | Count | Fix |
|---|-------------|-------|-----|
| 1 | Inner `#![...]` in `include!()` | 12 visible | `content.replace("#![allow(", "#[allow(")` |
| 2 | `r#fn` missing dot in field assignment | 1 visible (+1 hidden) | Change `".fn ="` replacement target from `"r#fn ="` to `".r#fn ="` |
| 3 | `PlSqlParserParserContext` doubled name | 1 visible | `content.replace("PlSqlParserParserContext", "PlSqlParserContext")` |
| 4 | 5 user-defined predicate methods absent | ~5 hidden E0599 | Append extension trait impls to `plsqlparser.rs` and `plsqllexer.rs` |

Estimated build time for the fix: less than 1 hour of coding + the ~3–10 minute compile time
for the 15.4 MB generated parser (memory is not a constraint: 221 GB available).

**This does NOT decide whether to sink days into antlr4rust** — that question is upstream in the
tournament result (D1-backend-tournament-result.md, decision: GO antlr4rust). This spike only
answers: "are the current compile errors a wall or a patch list?" They are a patch list.
