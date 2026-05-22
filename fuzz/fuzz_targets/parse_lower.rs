#![no_main]
//! Coverage-guided fuzz target for the real text-scanning pre-parser +
//! IR lowering pipeline (`PLSQL-PARSE-015` follow-through).
//!
//! Boundary (Hard Rule #2 — narrowest input boundary *in active use*):
//! `plsql_parser_antlr::lower::lower_source` is the entry point the whole
//! IR / SAST / lineage stack consumes. We chain it into
//! `plsql_ir::lower_top_level` so the fuzzer drives real pipeline depth,
//! not just the first scan.
//!
//! Oracle: the pre-parser is *tolerant* by contract — it must never
//! panic on any input, however adversarial, and lowering its output must
//! likewise never panic. It must also be deterministic: the same source
//! lowered twice yields the byte-identical debug encoding (catches
//! HashMap-iteration / pointer-address nondeterminism that would make
//! every downstream golden flaky).
//!
//! `let _ =` is never used to swallow a panic here — libfuzzer treats a
//! panic as the crash. The size guard keeps OOM kills from masking real
//! bugs.

use libfuzzer_sys::fuzz_target;

use plsql_core::{FileId, SymbolInterner};
use plsql_ir::lower_top_level;
use plsql_parser_antlr::lower::lower_source;

/// 256 KiB — far larger than any real PL/SQL unit, small enough that a
/// pathological input can't OOM-kill the run and hide the actual bug.
const MAX_LEN: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }
    // The pre-parser's contract is over `&str`; non-UTF-8 cannot reach it
    // through any production path, so reject it rather than lossily
    // mangling (keeps the corpus realistic + the harness deterministic).
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    let file = FileId::new(0);

    // Oracle 1: tolerant pre-parser never panics.
    let ast = lower_source(src, file);

    // Oracle 1 (depth): lowering the produced AST to IR never panics.
    let mut interner = SymbolInterner::new();
    let lowered = lower_top_level(&ast, &mut interner);

    // Oracle 2: determinism. Same source → identical lowered IR. A
    // mismatch means nondeterministic iteration leaked into the model and
    // every downstream snapshot/golden is silently flaky.
    let ast2 = lower_source(src, file);
    let mut interner2 = SymbolInterner::new();
    let lowered2 = lower_top_level(&ast2, &mut interner2);
    assert_eq!(
        format!("{lowered:?}"),
        format!("{lowered2:?}"),
        "lower_source+lower_top_level is non-deterministic for this input"
    );
});
