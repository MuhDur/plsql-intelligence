//! Compile-time smoke test: verify that `antlr-codegen` generated code is
//! reachable and compiles cleanly.
//!
//! This test is only compiled when the `antlr-codegen` feature is active.
//! Its purpose is to serve as a CI sentinel: if the generated code stops
//! compiling (due to a grammar change, antlr-rust API change, or a regression
//! in build.rs post-processing), this test will fail to compile.
//!
//! The generated modules are intentionally private (`mod generated { ... }`)
//! per the crate's architecture rule R20 (no ANTLR types escape this crate).
//! We therefore cannot name those types here.  The compile-time check is
//! implicit: if `plsql-parser-antlr` builds with `--features antlr-codegen`,
//! the generated code compiled.  This test file ensures the feature-gated
//! build is exercised in the test suite (not just in `cargo build`) so CI
//! catches regressions under `cargo test --features antlr-codegen`.

#[cfg(feature = "antlr-codegen")]
#[test]
fn antlr_codegen_crate_builds() {
    // The three generated files are included in the `generated` module of
    // `plsql_parser_antlr`.  If this test binary compiled, all three files
    // (plsqllexer.rs, plsqlparser.rs, plsqlparserlistener.rs) compiled too.
    //
    // No runtime assertion is needed: successful compilation IS the test.
    // The assertion below is trivially true and exists only to give the test
    // a visible runtime outcome in `cargo test` output.
    // Successful compilation of this binary is the assertion.
    // No runtime check is needed.
}
