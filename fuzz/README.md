# plsql-fuzz — coverage-guided fuzzing

Detached cargo-fuzz crate (its own `[workspace]`, so the nightly +
sanitizer build never perturbs the main workspace lockfile or CI gate).

## Target: `parse_lower`

Drives the **real text-scanning pre-parser** that the whole IR / SAST /
lineage stack consumes — `plsql_parser_antlr::lower::lower_source` — and
chains it into `plsql_ir::lower_top_level` for pipeline-depth coverage.
This is the narrowest untrusted-input boundary *in active use*. The deeper
ANTLR `ParseBackend` has its own crate-local regression coverage; this
detached fuzz target stays focused on the backend-free surface exposed to
the rest of the pipeline.

**Oracle.** The pre-parser is *tolerant* by contract, so the harness
asserts two things, never `let _ =`-swallowing a panic:

1. **never panics** on any input (`lower_source` then `lower_top_level`);
2. **deterministic** — the same source lowered twice yields a
   byte-identical debug encoding (catches HashMap-iteration /
   pointer-address nondeterminism that would make downstream goldens
   flaky).

Input is UTF-8-gated (the pre-parser's contract is over `&str`) and
size-bounded to 256 KiB so a pathological input can't OOM-mask a real
bug.

## Run it

```sh
# Build (nightly + ASan + UBSan, libFuzzer; rustup default here is
# stable, so go through the nightly toolchain explicitly):
rustup run nightly cargo fuzz build parse_lower

# Seed the corpus from the repo's own fixtures (idempotent):
mkdir -p corpus/parse_lower
find ../corpus/public ../corpus/synthetic ../corpus/lab \
  -type f \( -name '*.sql' -o -name '*.pks' -o -name '*.pkb' \) \
  -exec sh -c 'cp "$1" "corpus/parse_lower/seed_$(echo "$1" | md5sum | cut -c1-12).$(basename "$1")"' _ {} \;

# Campaign (LD_LIBRARY_PATH not needed — pure-Rust text scanner):
rustup run nightly cargo fuzz run parse_lower -- -max_total_time=2100
```

`corpus/`, `artifacts/`, `target/`, `coverage/` are git-ignored
(regenerable; Hard Rule #8 — a bloated committed corpus is slower, not
better). Seeds derive from the in-repo `corpus/` fixtures, so nothing is
lost by not committing them.

## Last campaign (2026-05-17)

~1.3 K exec/s (above the 1000 parser floor), coverage grew 1168→1300
edges / 4434→5437 features, corpus grew 96→986, **0 crashes and 0
determinism violations over 600 K+ executions**. For a parser whose
contract *is* "never panic, deterministic", a clean coverage-guided
campaign at this depth is the result you want — it is evidence of
robustness, not an empty run.

Every crash artifact, if one ever appears, MUST become a regression test
(Hard Rule #10): minimize with `cargo fuzz tmin`, then pin the minimized
input in a `#[test]` that parses it and asserts no panic.

---

## Target: `lower_statement_body`

Drives `plsql_parser_antlr::lower::lower_statement_body(body, file, offset)` —
the antlr-layer pre-parser for statement blocks (the text between `BEGIN`
and `END;`). Oracle: never panics on any UTF-8 input.

**Finding (fixed 2026-05-17):** Byte-index slicing in `keyword_at` panicked on
multi-byte UTF-8 chars (e.g. `ΤΤ';`). Fix: replace `s[pos..pos+kw.len()]`
with `s.as_bytes()[pos..pos+kw.len()].eq_ignore_ascii_case(kw.as_bytes())`.
Smoke (post-fix): ~35 K exec/s, 0 crashes.

## Target: `lower_expression`

Drives `plsql_parser_antlr::lower::lower_expression_text(expr, file, offset)` —
the expression text pre-scanner (RHS, conditions, RETURN values).
Oracle: never panics on any UTF-8 input.

**Finding (fixed 2026-05-17):** Byte-index slicing in `split_top_level_bin`
panicked on multi-byte UTF-8 chars. Fix: use `b[i..i+ob.len()].eq_ignore_ascii_case(ob)`
and guard split points with `is_char_boundary`. Smoke (post-fix): ~72 K exec/s, 0 crashes.

## Target: `lower_type_decl`

Drives `plsql_parser_antlr::lower::lower_type_decl(decl, file, offset)` —
the type-declaration pre-scanner (`TYPE … IS RECORD`, `TABLE OF`, `VARRAY`).
Oracle: never panics on any UTF-8 input. Smoke: ~111 K exec/s, 0 crashes.

## Target: `ir_lower_statement_body`

Drives `plsql_ir::stmt::lower_statement_body(source)` — the IR-layer statement
body lowerer in `plsql-ir` (separate crate from the antlr one; produces
fully-resolved `Statement` IR nodes). Oracle: never panics on any UTF-8 input.

**Finding (fixed 2026-05-17):** Two panics in `find_keyword` /
`find_any_keyword`: (1) `search_from = abs + 1` advanced a byte-index
by 1 regardless of char width; (2) cursor values like `then_token + 4`
could exceed the string length, causing an out-of-bounds slice. Fix:
clamp `search_from` to the next char boundary via `char_indices` and
advance by `char::len_utf8`. Smoke (post-fix): ~18 K exec/s, 0 crashes.

## Target: `catalog_snapshot_json`

Drives `serde_json::from_str::<plsql_catalog::CatalogSnapshotDocument>` —
the offline JSON catalog snapshot parser. Any attacker-controlled snapshot
`.json` file flows through this path before any other validation.
Oracle: never panics (serde returning `Err` for invalid JSON is correct;
only a panic/abort is the bug). Smoke: ~145 K exec/s, 0 crashes.

## Target: `mcp_async_dispatch`

Drives `plsql_mcp::dispatch_tool` inside a current-thread Asupersync runtime.
The fuzzer chooses a real `dispatch_table()` tool name (or an unknown tool)
and arbitrary JSON arguments. Oracle: invalid arguments, runtime-state-required
tools, and unknown tools may return error envelopes, but the async dispatcher
must never panic.

Last local soak (2026-06-28): `cargo +nightly-2026-05-11 fuzz run
mcp_async_dispatch -- -max_total_time=30 -timeout=10 -verbosity=0
-print_final_stats=1` executed 17,999 units with 0 crashes.

## Target: `catalog_async_loader`

Drives `plsql_mcp::OraclemcpCatalogConnection<FuzzDbConnection>` into
`plsql_catalog::load_snapshot_from_connection`. This covers the MCP-side
`oraclemcp-db` row/bind adapter plus the async dictionary-loader sequence
without opening a real Oracle connection. Oracle: malformed metadata rows,
permission-like query failures, and invalid schema filters may return `Err`;
panic is the bug.

Last local soak (2026-06-28): `cargo +nightly-2026-05-11 fuzz run
catalog_async_loader -- -max_total_time=30 -timeout=10 -verbosity=0
-print_final_stats=1` executed 21,214 units with 0 crashes.

## Target: `live_runtime_ops`

Drives `plsql_mcp::LiveDbRuntime` session operations with a boxed fake
`oraclemcp-db::OracleConnection`: insert, activate, remove, lease validation,
safety-profile transitions, DDL previews, enable/disable write tokens, and
connection call-timeout access. Oracle: refused transitions, stale leases, and
missing active sessions may return typed errors, but runtime state management
must never panic.

Last local soak (2026-06-28): `cargo +nightly-2026-05-11 fuzz run
live_runtime_ops -- -max_total_time=30 -timeout=10 -verbosity=0
-print_final_stats=1` executed 45,778 units with 0 crashes.
