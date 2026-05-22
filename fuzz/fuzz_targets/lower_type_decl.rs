#![no_main]
//! Coverage-guided fuzz target for the type-declaration pre-scanner.
//!
//! Boundary: `plsql_parser_antlr::lower::lower_type_decl` parses a bare
//! type-declaration fragment (e.g. `TYPE foo IS TABLE OF bar`) into an
//! `AstTypeDecl` node. It receives untrusted text from the source-split
//! pipeline and must never panic.
//!
//! Oracle: the scanner is *tolerant* by contract — any input, however
//! adversarial, must produce an `AstTypeDecl` (possibly an `Unknown`
//! variant) rather than a panic or abort. A panic is the bug.
//!
//! `let _ =` is never used to swallow a panic here — libfuzzer treats a
//! panic as the crash. The size guard keeps OOM kills from masking real
//! bugs.

use libfuzzer_sys::fuzz_target;

use plsql_core::FileId;
use plsql_parser_antlr::lower::lower_type_decl;

/// 256 KiB — far larger than any real PL/SQL type declaration, small
/// enough that a pathological input can't OOM-kill the run and hide the
/// actual bug.
const MAX_LEN: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }
    // The function's contract is over `&str`; non-UTF-8 cannot reach it
    // through any production path, so reject it rather than lossily
    // mangling (keeps the corpus realistic + the harness deterministic).
    let Ok(decl) = std::str::from_utf8(data) else {
        return;
    };

    let file = FileId::new(0);

    // Oracle: tolerant type-decl scanner never panics. The result is
    // intentionally used to prevent the compiler from eliding the call.
    let ast_type = lower_type_decl(decl, file, 0);
    let _ = format!("{ast_type:?}").len(); // force evaluation; not swallowing a panic
});
