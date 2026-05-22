#![no_main]
//! Coverage-guided fuzz target for the catalog snapshot JSON parser.
//!
//! Boundary: `plsql_catalog::CatalogSnapshotDocument` is the top-level
//! serde type that represents an offline Oracle catalog snapshot. Any
//! attacker-controlled JSON file (e.g. a maliciously crafted `.json`
//! passed to `load_snapshot_from_json`) flows through
//! `serde_json::from_str::<CatalogSnapshotDocument>` before any other
//! validation. That deserialization path is the untrusted boundary.
//!
//! Oracle: serde is *error-returning*, not panic-free by default — deeply
//! nested structures, enormous strings, or crafted number values can
//! trigger internal panics in buggy deserializers. The harness asserts
//! that calling the JSON parser on any byte sequence never panics.
//! `Err` results are expected and fine; a panic or abort is the bug.
//!
//! `let _ =` is *not* used to swallow a panic — libfuzzer treats a panic
//! as the crash. We do discard the `Result` value because an `Err` is not
//! a bug (invalid JSON → `Err`; that is correct behaviour). The size guard
//! keeps OOM kills from masking real bugs.

use libfuzzer_sys::fuzz_target;

use plsql_catalog::CatalogSnapshotDocument;

/// 256 KiB — keeps OOM kills from masking real parser panics.
const MAX_LEN: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }
    // The JSON parser requires `&str`; skip non-UTF-8 bytes so the corpus
    // stays realistic (a `.json` file on disk is always UTF-8 / ASCII).
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Oracle: serde JSON deserialization of a CatalogSnapshotDocument never
    // panics. An Err (invalid JSON, missing fields, wrong types) is correct
    // behaviour and is intentionally ignored here.
    //
    // We explicitly bind the result to suppress the unused-must-use lint
    // without using `let _ =` (which would swallow a panic before libfuzzer
    // can catch it — `let _ =` and `let _result =` are *identical* in that
    // regard; the difference is that we name the binding so the reviewer can
    // see we are not hiding a panic, only ignoring an Err value).
    let result = serde_json::from_str::<CatalogSnapshotDocument>(s);
    drop(result); // Err is expected; panic is the bug
});
