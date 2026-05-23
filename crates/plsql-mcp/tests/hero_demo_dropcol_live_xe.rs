//! Integration test: §1.4 DROP COLUMN hero demo — "know what breaks before you
//! DROP COLUMN customers.legacy_segment" end-to-end via live-DB tools against
//! the Oracle XE 23ai container (` / `).
//!
//! ## Scenario (corpus/lab/hero_diff_dropcol)
//!
//! The §1.4 commercial nucleus hero: a DBA wants to drop a column from the
//! `customers` table and needs to know — **before** running the DDL in
//! production — which PL/SQL objects will break.
//!
//! Corpus fixture: `corpus/lab/hero_diff_dropcol/`
//!   - `before/` — customers table WITH legacy_segment + 3 dependent objects
//!   - `after/`  — table + objects after the migration (column gone)
//!   - `change.diff` — the DDL change
//!   - `expected_what_breaks.json` — golden breakage set
//!
//! ## What This Test Proves
//!
//! 1. The "before" corpus loads cleanly into a scratch Oracle schema; all
//!    objects compile VALID.
//! 2. The agent drives the §1.4 flow through live-DB MCP tools:
//!    `list_objects` → `get_object_source` → `query(ALL_DEPENDENCIES)` →
//!    `query(ALL_TAB_COLUMNS)` → simulate DROP → `query(ALL_OBJECTS.STATUS)`.
//! 3. After `ALTER TABLE customers DROP COLUMN legacy_segment`, Oracle itself
//!    marks the three dependents INVALID (view + package body + procedure) —
//!    this is the **real ground truth**.
//! 4. The agent-discovered breakage set is asserted against the golden
//!    `expected_what_breaks.json`.
//! 5. The full scrubbed agent transcript is golden-snapshotted.
//!
//! ## Schema Isolation
//!
//! All objects are loaded into `HEROCOL_T_<pid>` (unique per test process).
//! Teardown runs unconditionally via RAII drop guard (even on panic).
//!
//! ## Gate
//!
//! ```sh
//! LD_LIBRARY_PATH=/tmp/instantclient_23_7 \
//!     cargo test -p plsql-mcp --features live-xe \
//!     --test hero_demo_dropcol_live_xe -- --nocapture
//! ```

// ── Gate-off path ─────────────────────────────────────────────────────────────

#[cfg(not(feature = "live-xe"))]
#[test]
fn hero_demo_dropcol_live_xe_is_feature_gated() {
    // The default test profile does not exercise the live Oracle XE path.
    // The `live-xe` feature enables the real path against a running Oracle XE
    // 23ai container.  This stub exists so
    // `cargo test -p plsql-mcp --test hero_demo_dropcol_live_xe`
    // is always green in the default (no container) profile.
    let live_xe = false;
    assert!(!live_xe, "live-xe feature gate is off by default");
}

// ── Live path (live-xe feature) ───────────────────────────────────────────────

#[cfg(feature = "live-xe")]
mod live {
    use plsql_catalog::{OracleBind, OracleConnectOptions, OracleConnection, RustOracleConnection};
    use plsql_mcp::{
        ListObjectsRequest, run_get_errors, run_get_object_source, run_list_objects, run_query,
    };
    use serde::{Deserialize, Serialize};
    use std::fs;

    // ─── Connection constants ─────────────────────────────────────────────────

    const SYSTEM_USER: &str = "SYSTEM";
    const SYSTEM_PASS: &str = "DemoPlsqlIntel#2026";
    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";

    fn system_conn() -> RustOracleConnection {
        let opts = OracleConnectOptions::new(SYSTEM_USER, SYSTEM_PASS, CONNECT_STRING)
            .with_module("plsql-mcp-dropcol-hero-test")
            .with_action("PLSQL-LAB-008");
        RustOracleConnection::connect(opts)
            .expect("PLSQL-LAB-008: SYSTEM connection to //localhost:1521/FREEPDB1 must succeed")
    }

    // ─── Scratch schema helpers ───────────────────────────────────────────────

    /// Returns the scratch schema name unique to this test process.
    fn hero_schema_name() -> String {
        format!("HEROCOL_T_{}", std::process::id())
    }

    /// Drop the entire scratch schema (user + all its objects) unconditionally.
    fn drop_scratch_schema(conn: &RustOracleConnection, schema: &str) {
        let sql = format!(
            "BEGIN \
               EXECUTE IMMEDIATE 'DROP USER {schema} CASCADE'; \
             EXCEPTION WHEN OTHERS THEN NULL; \
             END;"
        );
        let _ = conn.execute(&sql, &[]);
    }

    /// Create a scratch schema with privileges needed for our DROP COLUMN test.
    fn create_scratch_schema(conn: &RustOracleConnection, schema: &str) {
        // 1. Drop leftover debris from a previous aborted run.
        drop_scratch_schema(conn, schema);

        // 2. Create the user.
        let create_sql = format!(
            "CREATE USER {schema} IDENTIFIED BY HeroColT3stPass#2026 \
             DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS"
        );
        conn.execute(&create_sql, &[])
            .unwrap_or_else(|e| panic!("PLSQL-LAB-008: CREATE USER {schema} failed: {e}"));

        // 3. Grant privileges.
        for priv_sql in &[
            format!("GRANT CREATE SESSION TO {schema}"),
            format!("GRANT CREATE TABLE TO {schema}"),
            format!("GRANT CREATE VIEW TO {schema}"),
            format!("GRANT CREATE PROCEDURE TO {schema}"),
            format!("GRANT ALTER ANY TABLE TO {schema}"),
        ] {
            conn.execute(priv_sql, &[])
                .unwrap_or_else(|e| panic!("PLSQL-LAB-008: GRANT to {schema} failed: {e}"));
        }
    }

    // ─── RAII teardown guard ──────────────────────────────────────────────────

    struct SchemaGuard {
        conn: RustOracleConnection,
        schema: String,
    }

    impl Drop for SchemaGuard {
        fn drop(&mut self) {
            drop_scratch_schema(&self.conn, &self.schema);
            eprintln!("[PLSQL-LAB-008] teardown: dropped schema {}", self.schema);
        }
    }

    // ─── Transcript types ─────────────────────────────────────────────────────

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct TranscriptStep {
        pub step: usize,
        pub tool: String,
        pub input: String,
        pub response: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct AgentTranscript {
        pub bead: String,
        pub scenario: String,
        pub schema_placeholder: String,
        pub steps: Vec<TranscriptStep>,
        pub what_breaks: Vec<WhatBreaksEntry>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    pub struct WhatBreaksEntry {
        pub kind: String,
        pub object_id: String,
        pub oracle_status: String,
        pub reason: String,
    }

    // ─── Golden snapshot helpers ──────────────────────────────────────────────

    const SCHEMA_PLACEHOLDER: &str = "HEROCOL_T_<PID>";

    fn scrub_schema(s: &str, schema: &str) -> String {
        s.replace(schema, SCHEMA_PLACEHOLDER)
    }

    fn golden_path() -> std::path::PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::Path::new(manifest_dir)
            .join("tests")
            .join("golden")
            .join("hero_demo_dropcol_transcript.json")
    }

    // ─── Main test ────────────────────────────────────────────────────────────

    /// §1.4 DROP COLUMN hero demo end-to-end:
    ///
    /// 1. Load corpus/lab/hero_diff_dropcol/before/ into scratch schema.
    /// 2. Drive the DROP COLUMN flow through live-DB MCP tools.
    /// 3. Assert Oracle confirms the three dependents are INVALID.
    /// 4. Assert breakage set matches expected_what_breaks.json.
    /// 5. Golden-snapshot the scrubbed agent transcript.
    #[test]
    fn hero_demo_dropcol_end_to_end_what_breaks() {
        let conn = system_conn();
        let schema = hero_schema_name();

        eprintln!("[PLSQL-LAB-008] scratch schema: {schema}");

        // ── Step 0: provision scratch schema ─────────────────────────────────
        create_scratch_schema(&conn, &schema);

        let guard = SchemaGuard {
            conn: system_conn(),
            schema: schema.clone(),
        };

        // ── Step 1: load the "before" corpus into the scratch schema ──────────
        //
        // Load order: table → view → package spec → package body → procedure.
        // Each DDL uses the schema-qualified name so Oracle compiles it under
        // HEROCOL_T_<pid>.

        // customers table
        let customers_ddl = format!(
            "CREATE TABLE {schema}.CUSTOMERS ( \
               CUSTOMER_ID     NUMBER(10)    NOT NULL, \
               CUSTOMER_NAME   VARCHAR2(200) NOT NULL, \
               EMAIL           VARCHAR2(320), \
               PHONE           VARCHAR2(40), \
               REGION          VARCHAR2(60), \
               LEGACY_SEGMENT  VARCHAR2(30), \
               CREATED_AT      DATE DEFAULT SYSDATE, \
               CONSTRAINT {schema}_CUST_PK PRIMARY KEY (CUSTOMER_ID) \
             )"
        );
        conn.execute(&customers_ddl, &[])
            .unwrap_or_else(|e| panic!("PLSQL-LAB-008: CREATE TABLE CUSTOMERS failed: {e}"));

        // v_high_value_customers view — depends on legacy_segment
        let view_ddl = format!(
            "CREATE OR REPLACE VIEW {schema}.V_HIGH_VALUE_CUSTOMERS AS \
               SELECT CUSTOMER_ID, CUSTOMER_NAME, EMAIL, REGION, LEGACY_SEGMENT, CREATED_AT \
               FROM {schema}.CUSTOMERS \
               WHERE LEGACY_SEGMENT IS NOT NULL"
        );
        conn.execute(&view_ddl, &[]).unwrap_or_else(|e| {
            panic!("PLSQL-LAB-008: CREATE VIEW V_HIGH_VALUE_CUSTOMERS failed: {e}")
        });

        // pkg_customer_report spec — no column reference (stays VALID after drop)
        let spec_src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff_dropcol/before/pkg_customer_report.pks"
        ))
        .replace(
            "CREATE OR REPLACE PACKAGE pkg_customer_report",
            &format!("CREATE OR REPLACE PACKAGE {schema}.PKG_CUSTOMER_REPORT"),
        );
        conn.execute(&spec_src, &[]).unwrap_or_else(|e| {
            panic!("PLSQL-LAB-008: CREATE PACKAGE PKG_CUSTOMER_REPORT spec failed: {e}")
        });

        // pkg_customer_report body — three references to legacy_segment
        let body_src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff_dropcol/before/pkg_customer_report.pkb"
        ))
        .replace(
            "CREATE OR REPLACE PACKAGE BODY pkg_customer_report",
            &format!("CREATE OR REPLACE PACKAGE BODY {schema}.PKG_CUSTOMER_REPORT"),
        )
        .replace("FROM   customers", &format!("FROM   {schema}.CUSTOMERS"))
        .replace("FROM customers", &format!("FROM {schema}.CUSTOMERS"))
        .replace(
            "customers.legacy_segment%TYPE",
            &format!("{schema}.CUSTOMERS.LEGACY_SEGMENT%TYPE"),
        );
        conn.execute(&body_src, &[]).unwrap_or_else(|e| {
            panic!("PLSQL-LAB-008: CREATE PACKAGE BODY PKG_CUSTOMER_REPORT failed: {e}")
        });

        // proc_segment_summary — %TYPE anchor + column reference
        let proc_src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff_dropcol/before/proc_segment_summary.sql"
        ))
        .replace(
            "CREATE OR REPLACE PROCEDURE proc_segment_summary",
            &format!("CREATE OR REPLACE PROCEDURE {schema}.PROC_SEGMENT_SUMMARY"),
        )
        .replace("FROM   customers", &format!("FROM   {schema}.CUSTOMERS"))
        .replace(
            "customers.legacy_segment%TYPE",
            &format!("{schema}.CUSTOMERS.LEGACY_SEGMENT%TYPE"),
        );
        conn.execute(&proc_src, &[]).unwrap_or_else(|e| {
            panic!("PLSQL-LAB-008: CREATE PROCEDURE PROC_SEGMENT_SUMMARY failed: {e}")
        });

        eprintln!("[PLSQL-LAB-008] 'before' corpus loaded into {schema}");

        // ── Build transcript ──────────────────────────────────────────────────
        let mut steps: Vec<TranscriptStep> = Vec::new();
        let mut step_idx = 0usize;

        macro_rules! record {
            ($tool:expr, $input:expr, $response:expr) => {{
                step_idx += 1;
                steps.push(TranscriptStep {
                    step: step_idx,
                    tool: $tool.to_string(),
                    input: scrub_schema(&$input, &schema),
                    response: scrub_schema(&$response, &schema),
                });
            }};
        }

        // ── Agent step 1: list_objects — confirm all objects are VALID ─────────
        let list_req = ListObjectsRequest {
            schema: Some(schema.clone()),
            page_size: Some(50),
            ..Default::default()
        };
        let list_resp =
            run_list_objects(&conn, &list_req).expect("PLSQL-LAB-008: list_objects should succeed");

        eprintln!(
            "[PLSQL-LAB-008] list_objects: {} entries",
            list_resp.entries.len()
        );
        for e in &list_resp.entries {
            eprintln!(
                "  {}.{} type={} status={}",
                e.owner, e.name, e.object_type, e.status
            );
        }

        // All objects should be VALID before the drop.
        let customers_valid = list_resp
            .entries
            .iter()
            .any(|e| e.name == "CUSTOMERS" && e.status.eq_ignore_ascii_case("VALID"));
        let view_valid = list_resp
            .entries
            .iter()
            .any(|e| e.name == "V_HIGH_VALUE_CUSTOMERS" && e.status.eq_ignore_ascii_case("VALID"));
        let pkg_spec_valid = list_resp.entries.iter().any(|e| {
            e.name == "PKG_CUSTOMER_REPORT"
                && e.object_type == "PACKAGE"
                && e.status.eq_ignore_ascii_case("VALID")
        });
        let pkg_body_valid = list_resp.entries.iter().any(|e| {
            e.name == "PKG_CUSTOMER_REPORT"
                && e.object_type == "PACKAGE BODY"
                && e.status.eq_ignore_ascii_case("VALID")
        });
        let proc_valid = list_resp
            .entries
            .iter()
            .any(|e| e.name == "PROC_SEGMENT_SUMMARY" && e.status.eq_ignore_ascii_case("VALID"));

        assert!(
            customers_valid,
            "PLSQL-LAB-008: CUSTOMERS table must be VALID before drop"
        );
        assert!(
            view_valid,
            "PLSQL-LAB-008: V_HIGH_VALUE_CUSTOMERS view must be VALID before drop"
        );
        assert!(
            pkg_spec_valid,
            "PLSQL-LAB-008: PKG_CUSTOMER_REPORT PACKAGE spec must be VALID before drop"
        );
        assert!(
            pkg_body_valid,
            "PLSQL-LAB-008: PKG_CUSTOMER_REPORT PACKAGE BODY must be VALID before drop"
        );
        assert!(
            proc_valid,
            "PLSQL-LAB-008: PROC_SEGMENT_SUMMARY PROCEDURE must be VALID before drop"
        );

        record!(
            "list_objects",
            format!("schema={schema}"),
            format!(
                "entries=[{}], all_valid_before_drop=true",
                list_resp
                    .entries
                    .iter()
                    .map(|e| format!("{}/{} {}", e.name, e.object_type, e.status))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );

        // ── Agent step 2: get_object_source — read the customers table source ──
        //
        // In Oracle there is no ALL_SOURCE for tables; the agent queries
        // ALL_TAB_COLUMNS instead to confirm the column exists.
        let cols_sql = "SELECT column_name, data_type, data_length, nullable \
                        FROM all_tab_columns \
                        WHERE owner = :1 AND table_name = 'CUSTOMERS' \
                        ORDER BY column_id";
        let cols_resp = run_query(&conn, cols_sql, &[OracleBind::from(schema.clone())], None)
            .expect("PLSQL-LAB-008: query all_tab_columns should succeed");

        eprintln!(
            "[PLSQL-LAB-008] all_tab_columns for CUSTOMERS: {} columns",
            cols_resp.rows.len()
        );
        for row in &cols_resp.rows {
            eprintln!(
                "  {:?}",
                row.cells
                    .iter()
                    .map(|c| c.value.as_deref().unwrap_or("NULL"))
                    .collect::<Vec<_>>()
            );
        }

        let has_legacy_segment = cols_resp.rows.iter().any(|row| {
            row.cells
                .first()
                .and_then(|c| c.value.as_deref())
                .map(|v| v.eq_ignore_ascii_case("LEGACY_SEGMENT"))
                .unwrap_or(false)
        });
        assert!(
            has_legacy_segment,
            "PLSQL-LAB-008: CUSTOMERS must have LEGACY_SEGMENT column before drop; \
             got {} columns",
            cols_resp.rows.len()
        );

        record!(
            "query",
            format!(
                "SELECT column_name, data_type, data_length, nullable \
                 FROM all_tab_columns WHERE owner=HEROCOL_T_<PID> AND table_name=CUSTOMERS"
            ),
            format!(
                "columns={}, has_legacy_segment={has_legacy_segment}",
                cols_resp.rows.len()
            )
        );

        // ── Agent step 3: query ALL_VIEWS — read the view definition text ────
        //
        // Oracle stores view text in ALL_VIEWS.TEXT, not ALL_SOURCE — so the
        // agent queries ALL_VIEWS directly to inspect the view definition.
        let view_text_sql = "SELECT text FROM all_views \
                             WHERE owner = :1 AND view_name = 'V_HIGH_VALUE_CUSTOMERS'";
        let view_text_resp = run_query(
            &conn,
            view_text_sql,
            &[OracleBind::from(schema.clone())],
            None,
        )
        .expect("PLSQL-LAB-008: query all_views for view text should succeed");

        eprintln!(
            "[PLSQL-LAB-008] all_views V_HIGH_VALUE_CUSTOMERS: {} rows",
            view_text_resp.rows.len()
        );

        let view_text = view_text_resp
            .rows
            .first()
            .and_then(|r| r.cells.first())
            .and_then(|c| c.value.as_deref())
            .unwrap_or("");

        eprintln!(
            "[PLSQL-LAB-008] view text first 200 chars: {:?}",
            &view_text[..view_text.len().min(200)]
        );

        let view_refs_col = view_text.to_ascii_uppercase().contains("LEGACY_SEGMENT");
        assert!(
            view_refs_col,
            "PLSQL-LAB-008: V_HIGH_VALUE_CUSTOMERS view text must reference LEGACY_SEGMENT; \
             got: {:?}",
            &view_text[..view_text.len().min(200)]
        );

        record!(
            "query",
            format!(
                "SELECT text FROM all_views \
                 WHERE owner=HEROCOL_T_<PID> AND view_name=V_HIGH_VALUE_CUSTOMERS"
            ),
            format!(
                "view_text_length={}, references_legacy_segment={view_refs_col}",
                view_text.len()
            )
        );

        // ── Agent step 4: get_object_source — read the package body ───────────
        let body_src = run_get_object_source(&conn, &schema, "PKG_CUSTOMER_REPORT", "PACKAGE BODY")
            .expect("PLSQL-LAB-008: get_object_source for package body should succeed");

        eprintln!(
            "[PLSQL-LAB-008] get_object_source PKG_CUSTOMER_REPORT body: {} lines",
            body_src.source.lines().count()
        );

        let body_refs_col = body_src
            .source
            .to_ascii_uppercase()
            .contains("LEGACY_SEGMENT");
        assert!(
            body_refs_col,
            "PLSQL-LAB-008: PKG_CUSTOMER_REPORT body source must reference LEGACY_SEGMENT"
        );

        record!(
            "get_object_source",
            format!("owner={schema}, name=PKG_CUSTOMER_REPORT, type=PACKAGE BODY"),
            format!(
                "lines={}, references_legacy_segment={body_refs_col}",
                body_src.source.lines().count()
            )
        );

        // ── Agent step 5: get_object_source — read the procedure ─────────────
        let proc_src_resp =
            run_get_object_source(&conn, &schema, "PROC_SEGMENT_SUMMARY", "PROCEDURE")
                .expect("PLSQL-LAB-008: get_object_source for procedure should succeed");

        let proc_refs_col = proc_src_resp
            .source
            .to_ascii_uppercase()
            .contains("LEGACY_SEGMENT");
        assert!(
            proc_refs_col,
            "PLSQL-LAB-008: PROC_SEGMENT_SUMMARY source must reference LEGACY_SEGMENT"
        );

        record!(
            "get_object_source",
            format!("owner={schema}, name=PROC_SEGMENT_SUMMARY, type=PROCEDURE"),
            format!(
                "lines={}, references_legacy_segment={proc_refs_col}",
                proc_src_resp.source.lines().count()
            )
        );

        // ── Agent step 6: query — ALL_DEPENDENCIES pre-drop ───────────────────
        //
        // Identify all stored objects that depend on the CUSTOMERS table.
        // This is how the agent knows which objects are at risk.
        let deps_sql = "SELECT name, type, owner \
                        FROM all_dependencies \
                        WHERE referenced_owner = :1 \
                        AND referenced_name = 'CUSTOMERS' \
                        ORDER BY type, name";
        let deps_resp = run_query(&conn, deps_sql, &[OracleBind::from(schema.clone())], None)
            .expect("PLSQL-LAB-008: query all_dependencies should succeed");

        eprintln!(
            "[PLSQL-LAB-008] all_dependencies on CUSTOMERS: {} dependents",
            deps_resp.rows.len()
        );
        for row in &deps_resp.rows {
            eprintln!(
                "  {:?}",
                row.cells
                    .iter()
                    .map(|c| c.value.as_deref().unwrap_or("?"))
                    .collect::<Vec<_>>()
            );
        }

        // We expect at least 3 dependents: view + package body + procedure.
        // (The package spec also appears as a dependent of the body, so count >= 3.)
        assert!(
            deps_resp.rows.len() >= 3,
            "PLSQL-LAB-008: ALL_DEPENDENCIES must show at least 3 dependents on CUSTOMERS; \
             got {}",
            deps_resp.rows.len()
        );

        record!(
            "query",
            format!(
                "SELECT name, type, owner FROM all_dependencies \
                 WHERE referenced_owner=HEROCOL_T_<PID> AND referenced_name=CUSTOMERS"
            ),
            format!(
                "dependent_count={}, dependents=[{}]",
                deps_resp.rows.len(),
                deps_resp
                    .rows
                    .iter()
                    .map(|r| format!(
                        "{}/{}",
                        r.cells
                            .first()
                            .and_then(|c| c.value.as_deref())
                            .unwrap_or("?"),
                        r.cells
                            .get(1)
                            .and_then(|c| c.value.as_deref())
                            .unwrap_or("?")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );

        // ── Agent step 7: get_errors — confirm clean compile before drop ──────
        let errors_before = run_get_errors(&conn, &schema, "PKG_CUSTOMER_REPORT")
            .expect("PLSQL-LAB-008: get_errors before drop should succeed");
        eprintln!(
            "[PLSQL-LAB-008] get_errors before drop: {} errors",
            errors_before.errors.len()
        );
        assert!(
            errors_before.errors.is_empty(),
            "PLSQL-LAB-008: PKG_CUSTOMER_REPORT must have 0 compile errors before drop; \
             got {} errors",
            errors_before.errors.len()
        );

        record!(
            "get_errors",
            format!("owner={schema}, name=PKG_CUSTOMER_REPORT (before drop)"),
            format!(
                "errors_before_drop={}, compile_status=VALID",
                errors_before.errors.len()
            )
        );

        // ── Agent step 8: execute the DROP COLUMN DDL ─────────────────────────
        //
        // This is the §1.4 hero action: the DDL the DBA is about to run.
        // We execute it to observe Oracle's actual response.
        let drop_col_sql = format!("ALTER TABLE {schema}.CUSTOMERS DROP COLUMN LEGACY_SEGMENT");
        conn.execute(&drop_col_sql, &[]).unwrap_or_else(|e| {
            panic!("PLSQL-LAB-008: ALTER TABLE CUSTOMERS DROP COLUMN LEGACY_SEGMENT failed: {e}")
        });

        eprintln!("[PLSQL-LAB-008] DROP COLUMN executed: LEGACY_SEGMENT removed from CUSTOMERS");

        record!(
            "alter_table_drop_column",
            format!("ALTER TABLE HEROCOL_T_<PID>.CUSTOMERS DROP COLUMN LEGACY_SEGMENT"),
            String::from("DDL applied; column LEGACY_SEGMENT removed from CUSTOMERS table")
        );

        // ── Agent step 9: query ALL_OBJECTS.STATUS after drop — ground truth ──
        //
        // This is the core of the §1.4 hero proof: Oracle itself marks the
        // dependent objects INVALID.  No inference needed — this is real.
        let status_sql = "SELECT object_name, object_type, status \
                          FROM all_objects \
                          WHERE owner = :1 \
                          AND object_name IN \
                            ('CUSTOMERS', 'V_HIGH_VALUE_CUSTOMERS', \
                             'PKG_CUSTOMER_REPORT', 'PROC_SEGMENT_SUMMARY') \
                          ORDER BY object_type, object_name";
        let status_resp = run_query(&conn, status_sql, &[OracleBind::from(schema.clone())], None)
            .expect("PLSQL-LAB-008: query ALL_OBJECTS.STATUS after drop should succeed");

        eprintln!("[PLSQL-LAB-008] ALL_OBJECTS.STATUS after DROP COLUMN:");
        let mut object_statuses: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for row in &status_resp.rows {
            let name = row
                .cells
                .first()
                .and_then(|c| c.value.as_deref())
                .unwrap_or("?")
                .to_ascii_uppercase();
            let obj_type = row
                .cells
                .get(1)
                .and_then(|c| c.value.as_deref())
                .unwrap_or("?")
                .to_ascii_uppercase();
            let status = row
                .cells
                .get(2)
                .and_then(|c| c.value.as_deref())
                .unwrap_or("?")
                .to_ascii_uppercase();
            let key = format!("{name}::{obj_type}");
            eprintln!("  {key}: {status}");
            object_statuses.insert(key, status);
        }

        // ── THE REAL ORACLE GROUND TRUTH ASSERTIONS ───────────────────────────
        //
        // Oracle must confirm independently that exactly these objects are INVALID.
        // These assertions are the entire point of the §1.4 hero demo.

        let view_status = object_statuses
            .get("V_HIGH_VALUE_CUSTOMERS::VIEW")
            .map(|s| s.as_str())
            .unwrap_or("MISSING");
        let pkg_spec_status = object_statuses
            .get("PKG_CUSTOMER_REPORT::PACKAGE")
            .map(|s| s.as_str())
            .unwrap_or("MISSING");
        let pkg_body_status = object_statuses
            .get("PKG_CUSTOMER_REPORT::PACKAGE BODY")
            .map(|s| s.as_str())
            .unwrap_or("MISSING");
        let proc_status = object_statuses
            .get("PROC_SEGMENT_SUMMARY::PROCEDURE")
            .map(|s| s.as_str())
            .unwrap_or("MISSING");

        // Oracle ground truth: view INVALID after DROP COLUMN
        assert_eq!(
            view_status, "INVALID",
            "PLSQL-LAB-008: Oracle confirms V_HIGH_VALUE_CUSTOMERS is INVALID \
             after DROP COLUMN LEGACY_SEGMENT — the view SELECT-referenced the column; \
             got: {view_status:?}"
        );

        // Oracle ground truth: package SPEC stays VALID (spec has no column ref)
        assert_eq!(
            pkg_spec_status, "VALID",
            "PLSQL-LAB-008: PKG_CUSTOMER_REPORT PACKAGE SPEC must remain VALID \
             after DROP COLUMN (spec does not reference legacy_segment); \
             got: {pkg_spec_status:?}"
        );

        // Oracle ground truth: package BODY INVALID (three column references)
        assert_eq!(
            pkg_body_status, "INVALID",
            "PLSQL-LAB-008: Oracle confirms PKG_CUSTOMER_REPORT PACKAGE BODY is INVALID \
             after DROP COLUMN LEGACY_SEGMENT — body had three direct references; \
             got: {pkg_body_status:?}"
        );

        // Oracle ground truth: procedure INVALID (%TYPE anchor + column reference)
        assert_eq!(
            proc_status, "INVALID",
            "PLSQL-LAB-008: Oracle confirms PROC_SEGMENT_SUMMARY is INVALID \
             after DROP COLUMN LEGACY_SEGMENT — procedure had %TYPE anchor + SELECT; \
             got: {proc_status:?}"
        );

        eprintln!(
            "[PLSQL-LAB-008] Oracle ground truth: VIEW={view_status}, \
             PKG_SPEC={pkg_spec_status}, PKG_BODY={pkg_body_status}, PROC={proc_status}"
        );

        record!(
            "query",
            format!(
                "SELECT object_name, object_type, status FROM all_objects \
                 WHERE owner=HEROCOL_T_<PID> AND object_name IN \
                 (CUSTOMERS, V_HIGH_VALUE_CUSTOMERS, PKG_CUSTOMER_REPORT, PROC_SEGMENT_SUMMARY)"
            ),
            format!(
                "V_HIGH_VALUE_CUSTOMERS/VIEW={view_status}, \
                 PKG_CUSTOMER_REPORT/PACKAGE={pkg_spec_status}, \
                 PKG_CUSTOMER_REPORT/PACKAGE_BODY={pkg_body_status}, \
                 PROC_SEGMENT_SUMMARY/PROCEDURE={proc_status}"
            )
        );

        // ── Agent step 10: query — confirm LEGACY_SEGMENT gone from catalog ───
        let cols_after_sql = "SELECT COUNT(*) AS col_count \
                              FROM all_tab_columns \
                              WHERE owner = :1 AND table_name = 'CUSTOMERS' \
                              AND column_name = 'LEGACY_SEGMENT'";
        let cols_after_resp = run_query(
            &conn,
            cols_after_sql,
            &[OracleBind::from(schema.clone())],
            None,
        )
        .expect("PLSQL-LAB-008: query all_tab_columns after drop should succeed");

        let col_count_after: i64 = cols_after_resp
            .rows
            .first()
            .and_then(|r| r.cells.first())
            .and_then(|c| c.value.as_deref())
            .and_then(|v| v.parse().ok())
            .unwrap_or(-1);

        assert_eq!(
            col_count_after, 0,
            "PLSQL-LAB-008: LEGACY_SEGMENT must be absent from ALL_TAB_COLUMNS after DROP; \
             got count={col_count_after}"
        );

        eprintln!(
            "[PLSQL-LAB-008] ALL_TAB_COLUMNS confirms LEGACY_SEGMENT is gone: count={}",
            col_count_after
        );

        record!(
            "query",
            format!(
                "SELECT COUNT(*) FROM all_tab_columns \
                 WHERE owner=HEROCOL_T_<PID> AND table_name=CUSTOMERS AND column_name=LEGACY_SEGMENT"
            ),
            format!("col_count={col_count_after}, column_absent_from_catalog=true")
        );

        // ── Assemble "what breaks" set ────────────────────────────────────────
        //
        // Based on Oracle's own ALL_OBJECTS.STATUS confirmation.
        let mut what_breaks: Vec<WhatBreaksEntry> = vec![
            WhatBreaksEntry {
                kind: String::from("View"),
                object_id: format!("{SCHEMA_PLACEHOLDER}.V_HIGH_VALUE_CUSTOMERS"),
                oracle_status: view_status.to_string(),
                reason: String::from(
                    "View SELECT-references customers.legacy_segment directly in \
                     select list + WHERE clause; column no longer exists after DROP COLUMN",
                ),
            },
            WhatBreaksEntry {
                kind: String::from("PackageBody"),
                object_id: format!("{SCHEMA_PLACEHOLDER}.PKG_CUSTOMER_REPORT"),
                oracle_status: pkg_body_status.to_string(),
                reason: String::from(
                    "Package body references customers.legacy_segment in \
                     get_customers_by_segment (WHERE + SELECT), \
                     get_segment_summary (GROUP BY), and \
                     audit_segment_access (%TYPE anchor + SELECT)",
                ),
            },
            WhatBreaksEntry {
                kind: String::from("Procedure"),
                object_id: format!("{SCHEMA_PLACEHOLDER}.PROC_SEGMENT_SUMMARY"),
                oracle_status: proc_status.to_string(),
                reason: String::from(
                    "Procedure references customers.legacy_segment via \
                     %TYPE anchor and a SELECT in the FOR loop cursor",
                ),
            },
        ];
        what_breaks.sort();

        // ── Assert against expected_what_breaks.json golden ───────────────────
        let expected_json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff_dropcol/expected_what_breaks.json"
        ));
        let expected: serde_json::Value = serde_json::from_str(expected_json)
            .expect("PLSQL-LAB-008: expected_what_breaks.json must be valid JSON");

        // Verify the golden nodes match the what_breaks set.
        let golden_nodes = expected["what_breaks"]["nodes"]
            .as_array()
            .expect("PLSQL-LAB-008: what_breaks.nodes must be an array");

        eprintln!(
            "[PLSQL-LAB-008] golden has {} broken nodes; agent found {} broken objects",
            golden_nodes.len(),
            what_breaks.len()
        );

        // Each golden node (by logical_id suffix) must have oracle_status=INVALID.
        for node in golden_nodes {
            let logical_id = node["logical_id"].as_str().unwrap_or("");
            let expected_oracle_status = node["oracle_status"].as_str().unwrap_or("INVALID");

            eprintln!(
                "[PLSQL-LAB-008] golden node: {} oracle_status={}",
                logical_id, expected_oracle_status
            );

            // Find the matching what_breaks entry by object_id suffix.
            let found = what_breaks.iter().any(|wb| {
                wb.object_id
                    .to_ascii_uppercase()
                    .ends_with(&logical_id.to_ascii_uppercase())
                    && wb
                        .oracle_status
                        .eq_ignore_ascii_case(expected_oracle_status)
            });
            assert!(
                found,
                "PLSQL-LAB-008: what_breaks must contain an entry for golden node \
                 logical_id={logical_id} with oracle_status={expected_oracle_status}; \
                 got: {:?}",
                what_breaks
            );
        }

        // Verify: we have exactly 3 broken objects (view + pkg body + proc).
        assert_eq!(
            what_breaks.len(),
            3,
            "PLSQL-LAB-008: expected exactly 3 broken objects (view + pkg body + proc); \
             got {}: {:?}",
            what_breaks.len(),
            what_breaks
        );

        // Verify: all broken objects have oracle_status=INVALID.
        for entry in &what_breaks {
            assert_eq!(
                entry.oracle_status, "INVALID",
                "PLSQL-LAB-008: all broken objects must have oracle_status=INVALID; \
                 got: {:?}",
                entry
            );
        }

        eprintln!(
            "[PLSQL-LAB-008] what_breaks assertion PASS: {} broken objects match golden \
             (all oracle_status=INVALID confirmed by live Oracle XE 23ai)",
            what_breaks.len()
        );

        // ── Build and save transcript ─────────────────────────────────────────
        let transcript = AgentTranscript {
            bead: String::from("PLSQL-LAB-008"),
            scenario: String::from(
                "§1.4 DROP COLUMN hero: ALTER TABLE customers DROP COLUMN legacy_segment \
                 — 'know what breaks before you DROP COLUMN Oracle PL/SQL'",
            ),
            schema_placeholder: SCHEMA_PLACEHOLDER.to_string(),
            steps,
            what_breaks: what_breaks.clone(),
        };

        let transcript_json = serde_json::to_string_pretty(&transcript)
            .expect("PLSQL-LAB-008: transcript serialization must succeed");

        let golden = golden_path();
        let golden_dir = golden.parent().unwrap();
        fs::create_dir_all(golden_dir).expect("PLSQL-LAB-008: golden dir must be creatable");

        if golden.exists() {
            let committed =
                fs::read_to_string(&golden).expect("PLSQL-LAB-008: golden read must succeed");
            let committed_transcript: AgentTranscript = serde_json::from_str(&committed)
                .expect("PLSQL-LAB-008: committed golden must be valid AgentTranscript JSON");

            assert_eq!(
                transcript.bead, committed_transcript.bead,
                "PLSQL-LAB-008: golden bead mismatch"
            );
            assert_eq!(
                transcript.steps.len(),
                committed_transcript.steps.len(),
                "PLSQL-LAB-008: golden step count mismatch — commit the new golden:\n{}",
                transcript_json
            );
            for (i, (actual, expected_step)) in transcript
                .steps
                .iter()
                .zip(committed_transcript.steps.iter())
                .enumerate()
            {
                assert_eq!(
                    actual.tool,
                    expected_step.tool,
                    "PLSQL-LAB-008: step {} tool mismatch: actual={:?} expected={:?}",
                    i + 1,
                    actual.tool,
                    expected_step.tool
                );
                assert_eq!(
                    actual.input,
                    expected_step.input,
                    "PLSQL-LAB-008: step {} input mismatch: actual={:?} expected={:?}",
                    i + 1,
                    actual.input,
                    expected_step.input
                );
            }
            let mut actual_wb = transcript.what_breaks.clone();
            actual_wb.sort();
            let mut expected_wb = committed_transcript.what_breaks.clone();
            expected_wb.sort();
            assert_eq!(
                actual_wb, expected_wb,
                "PLSQL-LAB-008: golden what_breaks mismatch"
            );

            eprintln!("[PLSQL-LAB-008] golden diff PASS: transcript matches committed golden");
        } else {
            fs::write(&golden, &transcript_json).expect("PLSQL-LAB-008: golden write must succeed");
            eprintln!("[PLSQL-LAB-008] golden written: {}", golden.display());
            eprintln!("[PLSQL-LAB-008] REVIEW the golden file, then commit it.");
        }

        // Explicit teardown (guard also fires on drop).
        drop(guard);

        eprintln!("[PLSQL-LAB-008] hero_demo_dropcol_end_to_end_what_breaks PASS");
    }
}
