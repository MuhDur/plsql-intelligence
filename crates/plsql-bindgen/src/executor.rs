//! `OracleExecutor` trait — the sync-first execution model the bindings
//! generator targets.
//!
//! Concrete implementations are out of scope of this crate; the trait lives
//! here so generated wrappers can be parameterized over any driver that
//! satisfies the contract. The optional async wrapper has explicit blocking-
//! pool semantics: there is no fake async over a blocking driver.

use std::fmt;

/// A bind value the executor accepts. Kept open-shaped so wrappers can
/// translate Rust types into driver-native bindings without leaking
/// driver internals into the public trait.
#[derive(Debug, Clone, PartialEq)]
pub enum BindValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    /// `Date(YYYY-MM-DD)` wire string; the driver adapter parses to native.
    Date(String),
    /// `Timestamp(RFC-3339)` wire string.
    Timestamp(String),
}

/// One row in a result set. Stays as a heterogeneous bag of `BindValue`s so
/// the generator can map columns positionally without bringing in a row
/// trait per package.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub values: Vec<BindValue>,
}

/// Error returned by the executor. The bindings generator does not depend on
/// a specific driver error type; the wrapper crate adapts.
#[derive(Debug)]
pub struct ExecutionError {
    /// Stable code; `BINDING_EXECUTE_FAILED`, `BINDING_BIND_FAILED`, etc.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ExecutionError {}

/// Direction of a positional bind in a [`OracleExecutor::call_routine`]
/// invocation. `In` carries the caller's value; `Out` is a slot the
/// routine fills; `InOut` carries a value *and* expects it updated.
/// The driver returns the post-call value of every `Out`/`InOut` slot.
#[derive(Debug, Clone, PartialEq)]
pub enum RoutineArg {
    In(BindValue),
    Out,
    InOut(BindValue),
}

impl RoutineArg {
    /// `true` if the routine produces a value for this slot (the
    /// driver must return it).
    #[must_use]
    pub fn is_output(&self) -> bool {
        matches!(self, RoutineArg::Out | RoutineArg::InOut(_))
    }
}

/// Sync-first executor contract. The bindings generator targets this trait.
///
/// Implementations MUST be safe to call from a blocking context. The async
/// wrapper (`AsyncOracleExecutor`) is opt-in and is required to dispatch to
/// a blocking pool — never fake async over a blocking driver. This is
/// non-negotiable per the bindings-generator design (R7 / plan.md §13).
pub trait OracleExecutor {
    /// Execute a SQL or PL/SQL statement with positional binds. Returns
    /// the number of affected rows for DML or `0` for PL/SQL anonymous
    /// blocks.
    fn execute(&self, sql: &str, binds: &[BindValue]) -> Result<u64, ExecutionError>;

    /// Execute a query and materialize all rows. Streaming variants are a
    /// concrete implementation detail; the contract surface is row-set.
    fn query(&self, sql: &str, binds: &[BindValue]) -> Result<Vec<Row>, ExecutionError>;

    /// Invoke a PL/SQL routine through an anonymous block.
    ///
    /// `plsql` is the anonymous block the bindings generator composed
    /// (e.g. `BEGIN :1 := hr.pkg.f(:2); END;`); `args` is the
    /// positional bind list, one entry per `:n` in declaration order.
    /// The return value is the post-call value of every output slot
    /// (`Out`/`InOut`), in the order those slots appear in `args` —
    /// which the generator aligns with the wrapper's tuple return.
    ///
    /// A default implementation is provided for IN-only routines via
    /// [`OracleExecutor::execute`]; drivers that support OUT binds
    /// override this to also marshal output slots back.
    fn call_routine(
        &self,
        plsql: &str,
        args: &[RoutineArg],
    ) -> Result<Vec<BindValue>, ExecutionError> {
        if args.iter().any(RoutineArg::is_output) {
            return Err(ExecutionError {
                code: "BINDING_OUT_UNSUPPORTED".into(),
                message:
                    "this OracleExecutor does not implement call_routine for OUT/INOUT/return \
                          binds; override call_routine in the driver adapter"
                        .into(),
            });
        }
        let binds: Vec<BindValue> = args
            .iter()
            .map(|a| match a {
                RoutineArg::In(v) => v.clone(),
                RoutineArg::Out | RoutineArg::InOut(_) => unreachable!("guarded above"),
            })
            .collect();
        self.execute(plsql, &binds)?;
        Ok(Vec::new())
    }
}

/// Optional async wrapper for the executor. Implementations MUST dispatch
/// to a blocking thread pool (e.g. `tokio::task::spawn_blocking`) and
/// MUST document that semantics on the impl.
///
/// This trait exists so wrapper generators can opt into async surfaces
/// without the foundation trait pretending to be async.
#[cfg(feature = "async")]
pub trait AsyncOracleExecutor {
    fn execute(
        &self,
        sql: &str,
        binds: &[BindValue],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, ExecutionError>> + Send + '_>>;

    fn query(
        &self,
        sql: &str,
        binds: &[BindValue],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Row>, ExecutionError>> + Send + '_>,
    >;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubExecutor;
    impl OracleExecutor for StubExecutor {
        fn execute(&self, _sql: &str, _binds: &[BindValue]) -> Result<u64, ExecutionError> {
            Ok(1)
        }
        fn query(&self, _sql: &str, _binds: &[BindValue]) -> Result<Vec<Row>, ExecutionError> {
            Ok(vec![Row {
                values: vec![BindValue::Int(42)],
            }])
        }
        /// Echoes each output slot as `Int(7)` so round-trip tests can
        /// assert the wrapper marshals outputs back in slot order.
        fn call_routine(
            &self,
            _plsql: &str,
            args: &[RoutineArg],
        ) -> Result<Vec<BindValue>, ExecutionError> {
            Ok(args
                .iter()
                .filter(|a| a.is_output())
                .map(|_| BindValue::Int(7))
                .collect())
        }
    }

    /// IN-only call falls through the default `call_routine` (no
    /// override needed): no output slots, returns an empty vec.
    struct InOnlyExecutor {
        executed: std::cell::Cell<bool>,
    }
    impl OracleExecutor for InOnlyExecutor {
        fn execute(&self, sql: &str, binds: &[BindValue]) -> Result<u64, ExecutionError> {
            assert!(sql.starts_with("BEGIN"));
            assert_eq!(binds, &[BindValue::Int(5)]);
            self.executed.set(true);
            Ok(0)
        }
        fn query(&self, _sql: &str, _binds: &[BindValue]) -> Result<Vec<Row>, ExecutionError> {
            Ok(vec![])
        }
    }

    #[test]
    fn default_call_routine_handles_in_only_via_execute() {
        let exec = InOnlyExecutor {
            executed: std::cell::Cell::new(false),
        };
        let out = exec
            .call_routine("BEGIN hr.p(:1); END;", &[RoutineArg::In(BindValue::Int(5))])
            .unwrap();
        assert!(out.is_empty(), "IN-only call yields no output slots");
        assert!(exec.executed.get(), "default impl must delegate to execute");
    }

    #[test]
    fn default_call_routine_refuses_out_binds() {
        let exec = InOnlyExecutor {
            executed: std::cell::Cell::new(false),
        };
        let err = exec
            .call_routine("BEGIN hr.p(:1); END;", &[RoutineArg::Out])
            .unwrap_err();
        assert_eq!(err.code, "BINDING_OUT_UNSUPPORTED");
        assert!(
            !exec.executed.get(),
            "must not execute when it cannot honor OUT"
        );
    }

    #[test]
    fn overriding_call_routine_returns_output_slots_in_order() {
        let exec = StubExecutor;
        let out = exec
            .call_routine(
                "BEGIN :1 := hr.f(:2, :3); END;",
                &[
                    RoutineArg::Out,                      // function return
                    RoutineArg::In(BindValue::Int(1)),    // p_in
                    RoutineArg::InOut(BindValue::Int(2)), // p_inout
                ],
            )
            .unwrap();
        // Two output slots (return + inout), echoed in slot order.
        assert_eq!(out, vec![BindValue::Int(7), BindValue::Int(7)]);
    }

    #[test]
    fn stub_executor_roundtrip() {
        let exec = StubExecutor;
        assert_eq!(exec.execute("BEGIN NULL; END;", &[]).unwrap(), 1);
        let rows = exec.query("SELECT 1 FROM dual", &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values, vec![BindValue::Int(42)]);
    }

    #[test]
    fn execution_error_display() {
        let e = ExecutionError {
            code: "BINDING_EXECUTE_FAILED".into(),
            message: "ORA-00942".into(),
        };
        assert_eq!(format!("{e}"), "BINDING_EXECUTE_FAILED: ORA-00942");
    }
}
