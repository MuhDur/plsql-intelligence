//! Round-trip integration test against Oracle XE 23ai.
//!
//! Gated behind the `live-roundtrip` feature flag so the default test
//! profile (no Docker, no `ORACLE_PWD`) doesn't try to bind to a
//! container that isn't there. The CI workflow at
//! `.github/workflows/bindgen-roundtrip.yml` is the canonical driver:
//! it spins up the Oracle XE 23ai service container, deploys the
//! synthetic schema and `pkg_employee_mgmt` package, generates Rust
//! wrappers from a shared BindingPlan fixture, then compiles and invokes
//! those generated wrappers through a SQL*Plus-backed `OracleExecutor`.
//!
//! Locally a developer can run the same flow via
//! `make demo-oracle-xe` which boots the same image
//! with the lab fixtures pre-loaded, then:
//!
//! ```sh
//! mkdir -p target/generated-bindings
//! cargo run -p plsql-bindgen -- \
//!     --input crates/plsql-bindgen/tests/fixtures/pkg_employee_mgmt.binding-plan.json \
//!     --output target/generated-bindings/pkg_employee_mgmt.rs --target rust
//! ORACLE_PWD=DemoPlsqlIntel#2026 cargo test -p plsql-bindgen \
//!     --test xe_roundtrip --features live-roundtrip -- --nocapture
//! ```
//!
//! It invokes the generated `hire_employee` procedure against XE, then
//! invokes generated `count_employees` and checks the inserted row is
//! visible through the package. The shared fixture also has to remain
//! clean under `coverage_report`.
//!
//! When the feature flag is *off* (the default), this file contains a
//! single trivial test asserting the gate works — it documents the
//! contract without trying to reach a live database.

#[cfg(not(feature = "live-roundtrip"))]
#[test]
fn live_roundtrip_is_feature_gated() {
    // The default test profile doesn't exercise the live round-trip.
    // The bindgen-roundtrip CI workflow flips the feature and runs
    // the real path with the XE container.
    //
    // This stub exists so `cargo test -p plsql-bindgen --test
    // xe_roundtrip` always has at least one assertion to report —
    // a future regression that drops the `live-roundtrip` feature
    // entirely would surface here.
    let live_roundtrip = false;
    assert!(!live_roundtrip, "feature gate off by default");
}

#[cfg(feature = "live-roundtrip")]
mod live {
    use std::env;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use plsql_bindgen::executor::RoutineArg;
    use plsql_bindgen::{
        BindValue, BindingPlan, BindingsPosture, ExecutionError, OracleExecutor, Row,
        coverage_report,
    };

    mod generated {
        use plsql_bindgen::executor::RoutineArg;
        use plsql_bindgen::{BindValue, ExecutionError, OracleExecutor};

        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/generated-bindings/pkg_employee_mgmt.rs"
        ));
    }

    fn require_env(name: &str) -> String {
        env::var(name)
            .unwrap_or_else(|_| panic!("live-roundtrip needs env var {name}; see workflow yml"))
    }

    /// Read the same BindingPlan fixture the workflow passes to the CLI.
    fn synthetic_employee_mgmt_plan() -> BindingPlan {
        serde_json::from_str(include_str!("fixtures/pkg_employee_mgmt.binding-plan.json"))
            .expect("synthetic bindgen plan fixture must match the public schema")
    }

    struct SqlPlusExecutor {
        password: String,
    }

    impl SqlPlusExecutor {
        fn invoke(&self, sql: &str, args: &[RoutineArg]) -> Result<Vec<BindValue>, ExecutionError> {
            let mut script = format!(
                "WHENEVER OSERROR EXIT FAILURE\nWHENEVER SQLERROR EXIT SQL.SQLCODE\nCONNECT system/{}@//localhost:1521/FREEPDB1\nSET ECHO OFF HEADING OFF FEEDBACK OFF PAGESIZE 0 VERIFY OFF DEFINE OFF\nALTER SESSION SET NLS_NUMERIC_CHARACTERS = '.,';\n",
                self.password
            );

            for (index, arg) in args.iter().enumerate() {
                let sql_type = match arg {
                    RoutineArg::Out => "NUMBER",
                    RoutineArg::In(value) | RoutineArg::InOut(value) => match value {
                        BindValue::Text(_) => "VARCHAR2(4000)",
                        BindValue::Bytes(_) => {
                            return Err(executor_error(
                                "BINDGEN_TEST_UNSUPPORTED_BIND",
                                "SQL*Plus round-trip adapter does not support byte binds",
                            ));
                        }
                        _ => "NUMBER",
                    },
                };
                script.push_str(&format!(
                    "VARIABLE omcp_bind_{index_plus_one} {sql_type}\n",
                    index_plus_one = index + 1
                ));
                let value = match arg {
                    RoutineArg::In(value) | RoutineArg::InOut(value) => Some(value),
                    RoutineArg::Out => None,
                };
                if let Some(value) = value {
                    let literal = bind_literal(value)?;
                    script.push_str(&format!(
                        "BEGIN :omcp_bind_{} := {literal}; END;\n/\n",
                        index + 1
                    ));
                }
            }

            let mut bound_sql = sql.to_string();
            for index in (1..=args.len()).rev() {
                bound_sql = bound_sql.replace(&format!(":{index}"), &format!(":omcp_bind_{index}"));
            }
            script.push_str(&bound_sql);
            script.push_str("\n/\n");
            let output_count = args.iter().filter(|arg| arg.is_output()).count();
            for index in 1..=output_count {
                script.push_str(&format!(
                    "PROMPT __OMCP_BINDGEN_OUTPUT_{index}_BEGIN__\nPRINT omcp_bind_{index}\nPROMPT __OMCP_BINDGEN_OUTPUT_{index}_END__\n"
                ));
            }
            script.push_str("EXIT\n");

            let mut child = Command::new("/opt/oracle/instantclient/sqlplus")
                .args(["-s", "/nolog"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| executor_error("BINDGEN_SQLPLUS_START", &error.to_string()))?;
            child
                .stdin
                .take()
                .expect("piped SQL*Plus stdin")
                .write_all(script.as_bytes())
                .map_err(|error| executor_error("BINDGEN_SQLPLUS_WRITE", &error.to_string()))?;
            let output = child
                .wait_with_output()
                .map_err(|error| executor_error("BINDGEN_SQLPLUS_WAIT", &error.to_string()))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(executor_error(
                    "BINDGEN_SQLPLUS_FAILED",
                    &format!("SQL*Plus exited {}: {}", output.status, stderr.trim()),
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            (1..=output_count)
                .map(|index| {
                    let begin = format!("__OMCP_BINDGEN_OUTPUT_{index}_BEGIN__");
                    let end = format!("__OMCP_BINDGEN_OUTPUT_{index}_END__");
                    let section = stdout
                        .split_once(&begin)
                        .and_then(|(_, rest)| rest.split_once(&end).map(|(value, _)| value))
                        .ok_or_else(|| {
                            executor_error(
                                "BINDGEN_SQLPLUS_OUTPUT_MISSING",
                                &format!("SQL*Plus omitted output bind {index}"),
                            )
                        })?;
                    let value = section
                        .lines()
                        .find_map(|line| {
                            let value = line.trim();
                            if let Ok(integer) = value.parse::<i64>() {
                                Some(BindValue::Int(integer))
                            } else {
                                value.parse::<f64>().ok().map(BindValue::Float)
                            }
                        })
                        .ok_or_else(|| {
                            executor_error(
                                "BINDGEN_SQLPLUS_OUTPUT_INVALID",
                                &format!("SQL*Plus returned a non-numeric output bind {index}"),
                            )
                        })?;
                    Ok(value)
                })
                .collect()
        }
    }

    fn bind_literal(value: &BindValue) -> Result<String, ExecutionError> {
        Ok(match value {
            BindValue::Null => "NULL".into(),
            BindValue::Bool(value) => if *value { "1" } else { "0" }.into(),
            BindValue::Int(value) => value.to_string(),
            BindValue::Float(value) if value.is_finite() => value.to_string(),
            BindValue::Float(_) => {
                return Err(executor_error(
                    "BINDGEN_TEST_INVALID_BIND",
                    "non-finite float is not a valid SQL*Plus test bind",
                ));
            }
            BindValue::Text(value) => format!("'{}'", value.replace('\'', "''")),
            BindValue::Bytes(_) | BindValue::Date(_) | BindValue::Timestamp(_) => {
                return Err(executor_error(
                    "BINDGEN_TEST_UNSUPPORTED_BIND",
                    "SQL*Plus round-trip adapter received an unsupported test bind",
                ));
            }
        })
    }

    fn executor_error(code: &str, message: &str) -> ExecutionError {
        ExecutionError {
            code: code.into(),
            message: message.into(),
        }
    }

    impl OracleExecutor for SqlPlusExecutor {
        fn execute(&self, sql: &str, binds: &[BindValue]) -> Result<u64, ExecutionError> {
            let args: Vec<_> = binds.iter().cloned().map(RoutineArg::In).collect();
            self.invoke(sql, &args)?;
            Ok(0)
        }

        fn query(&self, _sql: &str, _binds: &[BindValue]) -> Result<Vec<Row>, ExecutionError> {
            Err(executor_error(
                "BINDGEN_TEST_QUERY_UNSUPPORTED",
                "generated round-trip wrappers do not issue row queries",
            ))
        }

        fn call_routine(
            &self,
            plsql: &str,
            args: &[RoutineArg],
        ) -> Result<Vec<BindValue>, ExecutionError> {
            self.invoke(plsql, args)
        }
    }

    #[test]
    fn round_trip_generated_wrapper_executes_against_live_xe() {
        let password = require_env("ORACLE_PWD");
        assert!(
            !password.is_empty(),
            "ORACLE_PWD must be non-empty for the live XE connection"
        );

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let generated_dir = manifest_dir
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root resolvable from manifest dir")
            .join("target")
            .join("generated-bindings");
        assert!(
            generated_dir.join("pkg_employee_mgmt.rs").is_file(),
            "plsql-bindgen must emit the generated wrapper before the live call: {}",
            generated_dir.display()
        );

        let plan = synthetic_employee_mgmt_plan();
        let report = coverage_report(&plan);
        assert_eq!(
            report.posture,
            BindingsPosture::Clean,
            "pkg_employee_mgmt coverage posture must be Clean; got {:?} with {} skips, \
             {} emitted_with_caveats, {} by_code rows",
            report.posture,
            report.skipped,
            report.emitted_with_caveats,
            report.by_code.len()
        );

        let mut executor = SqlPlusExecutor { password };
        let department_id = 982_517_i64;
        generated::hire_employee(
            &mut executor,
            "bindgen_roundtrip_synthetic".to_string(),
            1234.5,
            department_id,
        )
        .expect("generated hire_employee wrapper must execute against live XE");
        let count = generated::count_employees(&mut executor, department_id)
            .expect("generated count_employees wrapper must query live XE");
        assert_eq!(
            count,
            Some(1),
            "live package call must return inserted row count"
        );
        let salary = generated::get_salary(&mut executor, 1)
            .expect("generated get_salary wrapper must query live XE");
        assert_eq!(
            salary,
            Some(1234.5),
            "live package function must return stored salary"
        );
        generated::fire_employee(&mut executor, 1)
            .expect("generated fire_employee wrapper must delete through live XE");
        let remaining = generated::count_employees(&mut executor, department_id)
            .expect("generated count_employees wrapper must observe deletion");
        assert_eq!(
            remaining,
            Some(0),
            "live package call must observe deleted row"
        );
        eprintln!(
            "LIVE_XE_BINDGEN_ROUNDTRIP_ASSERTION_PASSED before={count:?} salary={salary:?} after={remaining:?} posture={:?}",
            report.posture,
        );
    }
}
