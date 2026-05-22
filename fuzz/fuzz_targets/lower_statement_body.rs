#![no_main]
//! Coverage-guided fuzz target for the statement-body pre-scanner.
//!
//! Boundary: `plsql_parser_antlr::lower::lower_statement_body` is the
//! entry point used by the IR pipeline to re-scan a bare statement block
//! (`BEGIN … END`) after a first-pass source split. It receives untrusted
//! text directly and must never panic on any input.
//!
//! Oracle: the pre-parser is *tolerant* by contract — any input, however
//! adversarial, must produce a result (possibly an `Unknown` statement
//! list) rather than a panic or abort. Returning `Err` / an empty Vec is
//! fine; a panic is the bug.
//!
//! `let _ =` is never used to swallow a panic here — libfuzzer treats a
//! panic as the crash. The size guard keeps OOM kills from masking real
//! bugs.

use libfuzzer_sys::fuzz_target;

use plsql_core::FileId;
use plsql_parser_antlr::lower::lower_statement_body;

/// 256 KiB — far larger than any real PL/SQL unit, small enough that a
/// pathological input can't OOM-kill the run and hide the actual bug.
const MAX_LEN: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }
    // The function's contract is over `&str`; non-UTF-8 cannot reach it
    // through any production path, so reject it rather than lossily
    // mangling (keeps the corpus realistic + the harness deterministic).
    let Ok(body) = std::str::from_utf8(data) else {
        return;
    };

    let file = FileId::new(0);

    // Oracle: tolerant pre-parser never panics. The result (any Vec<AstStatement>)
    // is intentionally used to prevent the compiler from eliding the call.
    let stmts = lower_statement_body(body, file, 0);
    let _ = stmts.len(); // force evaluation; not swallowing a panic
});
