# D1: Parser Backend Selection — antlr4rust Codegen Spike

> **Status:** SUPERSEDED — 2026-05-18 by [`D2-backend-final.md`](D2-backend-final.md)
> and by the 2026-06-28 Java-worker retirement.
> **Decision:** Historical spike only. The operative backend is
> `plsql-parser-antlr` (`antlr4rust`), with any future replacement requiring
> a fresh decision and bead set.
> **Spike author:** hermes_1 (mimo)
> **Depends on:** PLSQL-PARSE-000 (ParseBackend trait)

## 1. What was tested

Rust code generation from the grammars-v4 PL/SQL grammar using the
`antlr4rust` project (rrevenantt/antlr4rust) — the only actively maintained
ANTLR4 Rust target.

- **Grammar source:** `antlr/grammars-v4` master branch, `sql/plsql/`
  - `PlSqlLexer.g4` — 2,618 lines
  - `PlSqlParser.g4` — 10,011 lines
  - License: Apache-2.0 (compatible with our Apache-2.0 OR MIT)
- **Codegen tool:** `antlr4-4.8-2-SNAPSHOT-complete.jar` from
  `rrevenantt/antlr4rust/releases/tag/antlr4-4.8-2-Rust0.3.0-beta`
- **Runtime crate:** `antlr-rust = "0.3.0-beta"` (305K downloads, last
  updated 2025-10-25)

## 2. Codegen results

| Metric | Value |
|--------|-------|
| Lexer generated code | 24,292 lines (`plsqllexer.rs`) |
| Parser generated code | 284,280 lines (`plsqlparser.rs`) |
| Listener generated code | 472,925 lines (`plsqlparserlistener.rs`) |
| Total generated Rust | ~779K lines |
| Codegen warnings | 1 |
| Codegen errors | 2 (non-fatal — `fn` keyword collision) |
| Compile errors (first attempt) | 32 |

## 3. Blockers found

### Blocker 1: `caseInsensitive` lexer option unsupported

```
warning(83): PlSqlLexer.g4:29:4: unsupported option caseInsensitive
```

The grammars-v4 PL/SQL lexer uses `options { caseInsensitive = true; }`
which antlr4rust 4.8-2 does not support. The ANTLR4 Java target added this
in 4.10. antlr4rust is based on ANTLR 4.8-2.

**Impact:** Keywords and identifiers won't be case-insensitive in the
generated lexer. PL/SQL is case-insensitive by default. This would require
either:
- (a) Upgrading antlr4rust to support 4.10+ features (uncertain timeline)
- (b) Manually expanding case variants in the grammar (verbose but doable)
- (c) Handling case-insensitivity in a post-lexer normalization pass

### Blocker 2: `fn` keyword collision in generated code

```
error(134): PlSqlParser.g4:2438:39: symbol fn conflicts with generated
code in target language or runtime
```

The grammar uses `fn` as a label name (`fn = id_expression` in the
`DISASSOCIATE_STATISTICS` rule). `fn` is a reserved keyword in Rust. The
codegen emits bare `fn` instead of `r#fn` (Rust's raw identifier syntax).

**Impact:** 4 compile errors. Fixable by:
- (a) Patching the grammar to rename the label (e.g., `func = id_expression`)
- (b) Post-processing generated code with `sed` to add `r#` escaping
- (c) Upstream fix in antlr4rust codegen

**Workaround applied:** `sed` replacement works for this specific case.

### Blocker 3: Java-specific embedded actions use `this` instead of `recog`

```
error[E0425]: cannot find value `this` in this scope
--> src/plsqlparser.rs:4385
    this.IsNewlineAtPos(-4)
```

The grammar contains 14 embedded actions (semantic predicates) that use
`this.MethodName()` — the Java ANTLR convention. The antlr4rust README
explicitly states that embedded actions must use `recog` instead of `self`
or `this`. The generated code faithfully preserves the Java syntax, which
is invalid Rust.

**Affected predicates in grammar:**
- `this.IsNewlineAtPos(-4)` (2 occurrences in lexer)
- `this.isVersion12()`, `this.isVersion11()`, `this.isVersion10()` (version checks)
- `this.IsNotNumericFunction()` (function name context sensitivity)

**Impact:** 30 compile errors. Fixable by:
- (a) Patching the grammar to replace `this.` with `recog.` in all
  embedded actions (mechanical find-replace on the .g4 files)
- (b) Post-processing generated code with `sed` (fragile)

### Blocker 4 (minor): Missing `PlSqlParserParserContext` trait

```
error[E0405]: cannot find trait `PlSqlParserParserContext` in this scope
```

One trait import is missing in the generated code. Likely a codegen ordering
issue. May resolve once the other blockers are fixed.

## 4. Runtime observations

- Codegen time: ~3 seconds for both lexer and parser (acceptable)
- Generated code size: ~779K lines is large but within Rust compiler
  capabilities (the Rust compiler handles multi-million-line crates)
- The antlr4rust runtime crate (`antlr-rust 0.3.0-beta`) has 305K
  downloads and was last updated 2025-10-25 — not abandoned but not
  actively maintained either
- The codegen tool is based on ANTLR 4.8-2 (released ~2020); current
  ANTLR is 4.13+. The Rust target has not been merged into mainline ANTLR.

## 5. Risk assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| antlr4rust abandoned/unmaintained | Medium | High | Backend abstraction (R20) isolates this risk. A replacement backend requires a fresh decision and bead set. |
| Grammar patches needed for Rust | High | Medium | All 3 blockers have known workarounds. Grammar forking is acceptable. |
| Performance below expectations | Low | Medium | Benchmark under the current parser/perf beads; do not retain a hidden Java fallback. |
| caseInsensitive never supported | Medium | Low | Post-lexer normalization pass is straightforward for PL/SQL. |

## 6. Recommendation

**antlr4rust is viable but requires grammar patching.** All three blockers
have known workarounds. The codegen tool generates valid Rust code for the
vast majority of the grammar — only 14 embedded actions and 2 label names
cause issues.

**Next steps:**
1. PLSQL-PARSE-001: Vendor patched .g4 files into the repo (rename `fn`
   label, replace `this.` with `recog.` in embedded actions)
2. PLSQL-PARSE-002: Author `build.rs` that runs codegen at build time
3. PLSQL-PARSE-000C / D2: Backend decision with explicit go/no-go criteria

The backend tournament (PARSE-000C) and D2 made the final go/no-go decision.
This spike remains only as the historical record for why `antlr4rust` was
worth evaluating.

## 7. Files produced

- `/tmp/antlr-spike/PlSqlLexer.g4` — downloaded grammar (2,618 lines)
- `/tmp/antlr-spike/PlSqlParser.g4` — downloaded grammar (10,011 lines)
- `/tmp/antlr-spike/plsqllexer.rs` — generated lexer (24,292 lines)
- `/tmp/antlr-spike/plsqlparser.rs` — generated parser (284,280 lines)
- `/tmp/antlr-spike/plsqlparserlistener.rs` — generated listener (472,925 lines)
- `/tmp/antlr4-rust.jar` — ANTLR4 Rust codegen tool
