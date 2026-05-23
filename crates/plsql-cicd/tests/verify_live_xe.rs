//! Live Oracle XE 23ai integration test for `verify <changeset>`.
//!
//! Gated behind the `live-xe` feature flag so the default test profile
//! (no Docker, no `LD_LIBRARY_PATH`) doesn't try to reach a container
//! that isn't there. Run the real path with:
//!
//! ```sh
//! LD_LIBRARY_PATH=/tmp/instantclient_23_7 \
//!     cargo test -p plsql-cicd --features live-xe \
//!     --test verify_live_xe -- --nocapture
//! ```
//!
//! The test:
//! 1. Connects as SYSTEM to `//localhost:1521/FREEPDB1`.
//! 2. Calls `verify` with a synthetic three-statement changeset:
//!    a. `CREATE TABLE` — valid DDL, should succeed.
//!    b. A second valid `CREATE TABLE` — should succeed.
//!    c. An intentionally broken statement (bad SQL) — should fail.
//! 3. Asserts the report classifies statements a+b as `Ok` and c as
//!    `Failed`, and that the schema is cleaned up.
//!
//! The scratch schema is `VERIFY_T_<pid>`. It is dropped in teardown
//! even if the test panics — [`ScratchSchemaGuard`] handles this.
//!
//! When the feature flag is *off*, a trivial gate-off test runs so that
//! `cargo test -p plsql-cicd --test verify_live_xe` always has at least
//! one assertion.

// ── Gate-off path (no live-xe feature) ───────────────────────────────────────

#[cfg(not(feature = "live-xe"))]
#[test]
fn live_xe_verify_is_feature_gated() {
    // The default test profile doesn't exercise the live verify path.
    // The `live-xe` feature enables the real test against a running Oracle
    // XE 23ai container. This stub exists so `cargo test -p plsql-cicd
    // --test verify_live_xe` always has at least one assertion to report.
    let live_xe = false;
    assert!(!live_xe, "live-xe feature is off by default");
}

// ── Live path (live-xe feature) ───────────────────────────────────────────────

#[cfg(feature = "live-xe")]
mod live {
    use plsql_catalog::{OracleConnectOptions, RustOracleConnection};
    use plsql_cicd::verify::{
        StatementOutcome, VerifyChangeset, VerifyOptions, is_scratch_schema, verify,
    };

    const SYSTEM_USER: &str = "SYSTEM";
    const SYSTEM_PASS: &str = "DemoPlsqlIntel#2026";
    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";

    /// Connect as SYSTEM (has CREATE USER / DROP USER / GRANT privileges).
    fn system_conn() -> RustOracleConnection {
        let opts = OracleConnectOptions::new(SYSTEM_USER, SYSTEM_PASS, CONNECT_STRING)
            .with_module("plsql-cicd-verify-test")
            .with_action("PLSQL-CICD-005");
        RustOracleConnection::connect(opts)
            .expect("SYSTEM connection to //localhost:1521/FREEPDB1 must succeed")
    }

    /// Build the synthetic three-statement changeset:
    ///
    /// 1. Valid `CREATE TABLE` — should succeed.
    /// 2. Valid `CREATE TABLE` with a NOT NULL column — should succeed.
    /// 3. Intentionally invalid SQL — should fail with an Oracle error.
    fn synthetic_changeset() -> VerifyChangeset {
        VerifyChangeset::with_labels(vec![
            (
                1,
                String::from(
                    "CREATE TABLE ORDERS (ORDER_ID NUMBER PRIMARY KEY, AMOUNT NUMBER NOT NULL)",
                ),
                Some(String::from("CREATE TABLE ORDERS")),
            ),
            (
                2,
                String::from(
                    "CREATE TABLE ORDER_LINES (LINE_ID NUMBER PRIMARY KEY, ORDER_ID NUMBER, ITEM VARCHAR2(200))",
                ),
                Some(String::from("CREATE TABLE ORDER_LINES")),
            ),
            (
                3,
                // Intentionally broken: referencing a non-existent type.
                // This will fail with ORA-00904 or ORA-00902.
                String::from("CREATE TABLE BAD_TABLE (X NONEXISTENT_TYPE_XYZ_INVALID)"),
                Some(String::from(
                    "CREATE TABLE BAD_TABLE (intentionally broken)",
                )),
            ),
        ])
    }

    #[test]
    fn verify_applies_synthetic_changeset_and_reports_per_statement_outcomes() {
        let conn = system_conn();

        let cs = synthetic_changeset();
        let opts = VerifyOptions::default(); // VERIFY_T_<pid>

        eprintln!(
            "[PLSQL-CICD-005] verify scratch schema will be: {}",
            opts.effective_schema()
        );
        assert!(
            is_scratch_schema(&opts.effective_schema()),
            "effective_schema must be a VERIFY_T_* scratch schema"
        );

        let report = verify(&conn, &cs, &opts)
            .expect("verify should return Ok(report) even when some statements fail");

        // Print the report for --nocapture diagnostics.
        eprintln!(
            "[PLSQL-CICD-005] verify report for schema `{}`:",
            report.schema
        );
        for row in &report.rows {
            eprintln!(
                "  ordinal={} label={:?} outcome={}",
                row.ordinal, row.label, row.outcome
            );
        }
        eprintln!(
            "[PLSQL-CICD-005] ok={} failed={} skipped={}",
            report.ok_count(),
            report.failed_count(),
            report.skipped_count()
        );

        // Assertions —————————————————————————————————————————————————————
        assert_eq!(
            report.rows.len(),
            3,
            "report should have exactly 3 rows (one per statement)"
        );

        // Statement 1: CREATE TABLE ORDERS — must succeed.
        assert!(
            matches!(report.rows[0].outcome, StatementOutcome::Ok),
            "statement 1 (CREATE TABLE ORDERS) should be Ok, got: {}",
            report.rows[0].outcome
        );

        // Statement 2: CREATE TABLE ORDER_LINES — must succeed.
        assert!(
            matches!(report.rows[1].outcome, StatementOutcome::Ok),
            "statement 2 (CREATE TABLE ORDER_LINES) should be Ok, got: {}",
            report.rows[1].outcome
        );

        // Statement 3: BAD_TABLE with invalid type — must fail.
        assert!(
            matches!(report.rows[2].outcome, StatementOutcome::Failed { .. }),
            "statement 3 (BAD_TABLE) should be Failed, got: {}",
            report.rows[2].outcome
        );
        if let StatementOutcome::Failed { error } = &report.rows[2].outcome {
            eprintln!("[PLSQL-CICD-005] expected failure message: {error}");
            // Oracle should return an ORA- error code.
            assert!(
                error.contains("ORA-"),
                "failure message should contain ORA- error code, got: `{error}`"
            );
        }

        // Overall: not clean (one failure).
        assert!(
            !report.is_clean(),
            "report should not be clean (one failure expected)"
        );
        assert_eq!(report.ok_count(), 2);
        assert_eq!(report.failed_count(), 1);
        // No skipped: we hit the failure on the last statement.
        assert_eq!(report.skipped_count(), 0);

        // Schema is named correctly.
        assert!(
            is_scratch_schema(&report.schema),
            "report.schema should be a VERIFY_T_* name, got `{}`",
            report.schema
        );

        // Verify the scratch schema has been dropped (teardown).
        // Query ALL_USERS — if the schema persists it would appear here.
        use plsql_catalog::{OracleBind, OracleConnection};
        let rows = conn
            .query_rows(
                "SELECT 1 FROM all_users WHERE username = :1",
                &[OracleBind::String(report.schema.clone())],
            )
            .expect("schema existence check query should succeed");
        assert!(
            rows.is_empty(),
            "scratch schema `{}` should have been dropped in teardown, but still exists",
            report.schema
        );

        eprintln!(
            "[PLSQL-CICD-005] scratch schema `{}` confirmed dropped.",
            report.schema
        );
    }

    #[test]
    fn verify_all_valid_changeset_is_clean_and_schema_dropped() {
        let conn = system_conn();

        let cs = VerifyChangeset::new(vec![
            (1, String::from("CREATE TABLE PING_T (ID NUMBER)")),
            (
                2,
                String::from("CREATE VIEW PING_V AS SELECT ID FROM PING_T"),
            ),
        ]);
        let opts = VerifyOptions {
            schema_override: Some(format!("VERIFY_T_{}_2", std::process::id())),
            ..Default::default()
        };

        eprintln!(
            "[PLSQL-CICD-005] all-valid run using schema: {}",
            opts.effective_schema()
        );

        let report = verify(&conn, &cs, &opts).expect("all-valid verify should succeed");

        eprintln!(
            "[PLSQL-CICD-005] all-valid: ok={} failed={} skipped={}",
            report.ok_count(),
            report.failed_count(),
            report.skipped_count()
        );

        assert!(report.is_clean(), "all-valid changeset should be clean");
        assert_eq!(report.ok_count(), 2);
        assert_eq!(report.failed_count(), 0);
        assert_eq!(report.skipped_count(), 0);

        // Confirm schema dropped.
        use plsql_catalog::{OracleBind, OracleConnection};
        let rows = conn
            .query_rows(
                "SELECT 1 FROM all_users WHERE username = :1",
                &[OracleBind::String(report.schema.clone())],
            )
            .expect("schema existence check");
        assert!(
            rows.is_empty(),
            "schema `{}` should be dropped after clean run",
            report.schema
        );
    }

    #[test]
    fn verify_skips_subsequent_statements_after_first_failure() {
        let conn = system_conn();

        let cs = VerifyChangeset::with_labels(vec![
            (
                1,
                // Fail immediately on the first statement.
                String::from("CREATE TABLE %%INVALID_NAME%% (ID NUMBER)"),
                Some(String::from("invalid table name (expected to fail)")),
            ),
            (
                2,
                String::from("CREATE TABLE SHOULD_BE_SKIPPED (ID NUMBER)"),
                Some(String::from("should be skipped because stmt 1 failed")),
            ),
        ]);
        let opts = VerifyOptions {
            schema_override: Some(format!("VERIFY_T_{}_3", std::process::id())),
            ..Default::default()
        };

        let report = verify(&conn, &cs, &opts).expect("verify returns Ok even on failure");

        eprintln!(
            "[PLSQL-CICD-005] skip-after-fail: ok={} failed={} skipped={}",
            report.ok_count(),
            report.failed_count(),
            report.skipped_count()
        );

        assert_eq!(report.rows.len(), 2);
        assert!(
            matches!(report.rows[0].outcome, StatementOutcome::Failed { .. }),
            "stmt 1 should fail"
        );
        assert!(
            matches!(report.rows[1].outcome, StatementOutcome::Skipped),
            "stmt 2 should be skipped, got: {}",
            report.rows[1].outcome
        );
        assert_eq!(report.skipped_count(), 1);
        assert!(!report.is_clean());

        // Schema still dropped even though execution failed.
        use plsql_catalog::{OracleBind, OracleConnection};
        let rows = conn
            .query_rows(
                "SELECT 1 FROM all_users WHERE username = :1",
                &[OracleBind::String(report.schema.clone())],
            )
            .expect("schema existence check");
        assert!(
            rows.is_empty(),
            "schema `{}` should be dropped even after failure",
            report.schema
        );
    }
}
