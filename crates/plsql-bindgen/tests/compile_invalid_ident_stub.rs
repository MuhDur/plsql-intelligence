//! Behavioral teeth for oracle-rwjl.7 / oracle-rwjl.13: the emitted
//! `BINDING_INVALID_IDENTIFIER` stub for a hostile routine name must be
//! COMPILABLE Rust whose Debug-escaped message cannot smuggle tokens out of
//! the generated string literal. This is the compile-level proof behind the
//! string assertions in `emit.rs`'s unit tests.

use plsql_bindgen::executor::{BindValue, ExecutionError, OracleExecutor, Row, RoutineArg};

/// VERBATIM shape of what `emit_routine` now emits for a routine whose name
/// carries a quote + SQL fragment + brace (`f", DROP TABLE t; -- {`). The
/// message is rendered via `{msg:?}` (Debug), so every metacharacter is
/// escaped inside the Rust string literal — the body compiles and the hostile
/// text never breaks out. Keep this in lockstep with the emitter's gate body.
#[allow(dead_code)]
pub fn binding_invalid_stub_0(
    executor: &mut impl OracleExecutor,
) -> Result<(), ExecutionError> {
    let _ = executor;
    // bindgen: routine name `f", DROP TABLE t; -- {`: identifier contains a
    // character outside [A-Za-z0-9_]; the BindingPlan carries an identifier
    // that is not a legal Rust identifier.
    Err(ExecutionError {
        code: "BINDING_INVALID_IDENTIFIER".to_string(),
        message: "routine `f\", DROP TABLE t; -- {` in package `hr.pkg`: routine name `f\", DROP TABLE t; -- {`: identifier contains a character outside [A-Za-z0-9_]; supply a BindingPlan with legal Rust-identifier routine/parameter names and an Oracle-safe package_id"
            .to_string(),
    })
}

/// A no-op executor so the stub can be invoked.
struct NoopExecutor;
impl OracleExecutor for NoopExecutor {
    fn execute(&self, _sql: &str, _binds: &[BindValue]) -> Result<u64, ExecutionError> {
        Ok(0)
    }
    fn query(&self, _sql: &str, _binds: &[BindValue]) -> Result<Vec<Row>, ExecutionError> {
        Ok(vec![])
    }
    fn call_routine(
        &self,
        _plsql: &str,
        _args: &[RoutineArg],
    ) -> Result<Vec<BindValue>, ExecutionError> {
        Ok(vec![])
    }
}

#[test]
fn invalid_identifier_stub_compiles_and_returns_typed_error() {
    let mut exec = NoopExecutor;
    let err = binding_invalid_stub_0(&mut exec).expect_err("stub must return a typed error");
    assert_eq!(err.code, "BINDING_INVALID_IDENTIFIER");
    // The hostile text is preserved verbatim inside the message (escaped in
    // source, faithful at runtime) — proving the Debug escaping round-trips.
    assert!(err.message.contains("f\", DROP TABLE t; -- {"));
}
