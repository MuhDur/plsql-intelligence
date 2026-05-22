#![no_main]
//! Coverage-guided fuzz target for the IR-layer statement-body lowerer.
//!
//! Boundary: `plsql_ir::stmt::lower_statement_body` is a *different*
//! function from the antlr-layer one — it lives in the `plsql-ir` crate
//! and produces fully-resolved IR `Statement` nodes (not raw AST). It
//! is the entry point used by the SAST / lineage pipeline when it needs
//! to re-lower a statement block, and it receives untrusted source text
//! directly.
//!
//! Oracle: the IR lowerer is *tolerant* by contract — any `&str` input,
//! however adversarial, must produce a `Vec<Statement>` (possibly all
//! `Statement::Unknown`) rather than a panic or abort. A panic is the bug.
//!
//! `let _ =` is never used to swallow a panic here — libfuzzer treats a
//! panic as the crash. The size guard keeps OOM kills from masking real
//! bugs.

use libfuzzer_sys::fuzz_target;

use plsql_ir::stmt::lower_statement_body;

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
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    // Oracle: tolerant IR lowerer never panics. The result is
    // intentionally used to prevent the compiler from eliding the call.
    let stmts = lower_statement_body(source);
    let _ = stmts.len(); // force evaluation; not swallowing a panic
});
