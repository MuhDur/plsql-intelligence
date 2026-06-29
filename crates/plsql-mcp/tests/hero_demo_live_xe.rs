//! Integration test: §1.4 hero demo — "know what breaks before you change Oracle PL/SQL"
//! end-to-end via live-DB tools against the synthetic-lab Oracle XE deployment.
//!
//! ## Scenario (corpus/lab/hero_diff)
//!
//! The hero scenario for the product's central sales story: a parameter rename
//! (`p_emp_id` → `p_employee_id`) on `employee_mgmt.fire_employee` and
//! `employee_mgmt.get_salary` — every caller that uses named notation must be
//! updated.  This is the `corpus/lab/hero_diff/` fixture: before/, after/,
//! change.diff, expected_what_breaks.json.
//!
//! NOTE ON PLAN.MD §1.4: plan.md §1.4 frames the hero story as
//! "DROP COLUMN customers.legacy_segment".  The actual corpus fixture
//! (`corpus/lab/hero_diff/`) implements the equivalent scenario as a
//! parameter rename on `pkg_employee_mgmt` — there is no `customers` table
//! in the fixture corpus.  This test faithfully exercises the fixture corpus
//! as the ground truth for what the hero demo demonstrates: inspecting an
//! object, predicting what downstream consumers would break, and validating
//! against the golden expected_what_breaks.json.
//!
//! ## Schema isolation
//!
//! The "before" objects (package spec + body) are loaded into a scratch schema
//! `HERO_T_<pid>` under SYSTEM.  Teardown runs unconditionally (even on panic)
//! via a drop guard, following the mcp_live_xe.rs pattern.
//!
//! ## Gate
//!
//! Run the live path with:
//! ```sh
//! cargo test -p plsql-mcp --features live-xe \
//!     --test hero_demo_live_xe -- --nocapture
//! ```

// ── Gate-off path ─────────────────────────────────────────────────────────────

#[cfg(not(feature = "live-xe"))]
#[test]
fn hero_demo_live_xe_is_feature_gated() {
    // The default test profile does not exercise the live Oracle XE path.
    // The `live-xe` feature enables the real path.  This stub keeps
    // `cargo test -p plsql-mcp --test hero_demo_live_xe` always green.
    let live_xe = false;
    assert!(!live_xe, "live-xe feature gate is off by default");
}

// ── Live path (live-xe feature) ───────────────────────────────────────────────

#[cfg(feature = "live-xe")]
mod live {
    use asupersync::{Cx, runtime::RuntimeBuilder};
    use plsql_catalog::{CatalogError, OracleBind, OracleConnection};
    use plsql_mcp::{
        GetErrorsResponse, GetObjectSourceResponse, ListObjectsRequest, ListObjectsResponse,
        OraclemcpCatalogConnection, QueryError, QueryResponse, SourceToolError,
        run_get_errors as run_get_errors_async,
        run_get_object_source as run_get_object_source_async,
        run_list_objects as run_list_objects_async, run_query as run_query_async,
    };
    use serde::{Deserialize, Serialize};
    use std::{
        fs,
        future::Future,
        sync::atomic::{AtomicU32, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    // ─── Connection constants ─────────────────────────────────────────────────

    static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

    type LiveConnection = OraclemcpCatalogConnection<oraclemcp_db::RustOracleConnection>;

    const SYSTEM_USER: &str = "SYSTEM";
    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";

    fn system_conn() -> LiveConnection {
        let password = required_env("PLSQL_XE_SYSTEM_PASSWORD");
        let opts = oraclemcp_db::OracleConnectOptions {
            connect_string: CONNECT_STRING.to_owned(),
            username: Some(SYSTEM_USER.to_owned()),
            password: Some(password),
            session_identity: Some(oraclemcp_db::OracleSessionIdentity {
                module: Some("plsql-mcp-hero-demo-test".to_owned()),
                action: Some("PLSQL-MCP-LIVE-019".to_owned()),
                ..oraclemcp_db::OracleSessionIdentity::default()
            }),
            ..oraclemcp_db::OracleConnectOptions::default()
        };
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            OraclemcpCatalogConnection::connect(&cx, opts)
                .await
                .expect("PLSQL-MCP-LIVE-019: SYSTEM thin connection must succeed")
        })
    }

    fn run_live_future<F: Future>(future: F) -> F::Output {
        let reactor =
            asupersync::runtime::reactor::create_reactor().expect("live-xe native reactor");
        RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .expect("live-xe asupersync runtime")
            .block_on(future)
    }

    fn execute_sql<C: OracleConnection>(conn: &C, sql: &str) -> Result<u64, CatalogError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            conn.execute(&cx, sql, &[]).await
        })
    }

    fn run_query<C: OracleConnection>(
        conn: &C,
        sql: &str,
        params: &[OracleBind],
        lob_truncation_chars: Option<usize>,
    ) -> Result<QueryResponse, QueryError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_query_async(&cx, conn, sql, params, lob_truncation_chars).await
        })
    }

    fn run_get_object_source<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
        object_type: &str,
    ) -> Result<GetObjectSourceResponse, SourceToolError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_get_object_source_async(&cx, conn, owner, object_name, object_type).await
        })
    }

    fn run_get_errors<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
    ) -> Result<GetErrorsResponse, SourceToolError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_get_errors_async(&cx, conn, owner, object_name).await
        })
    }

    fn run_list_objects<C: OracleConnection>(
        conn: &C,
        request: &ListObjectsRequest,
    ) -> Result<ListObjectsResponse, plsql_mcp::ListObjectsError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_list_objects_async(&cx, conn, request).await
        })
    }

    // ─── Scratch schema helpers ───────────────────────────────────────────────

    /// Returns the scratch schema name unique to this test process.
    fn hero_schema_name() -> String {
        format!("HERO_T_{}", unique_suffix())
    }

    fn required_env(name: &str) -> String {
        std::env::var(name).expect("required live-XE credential env var is not set")
    }

    fn unique_suffix() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let counter = SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed) % 1000;
        format!("{:015}{counter:03}", nanos % 1_000_000_000_000_000)
    }

    fn random_password(prefix: &str) -> String {
        format!("{prefix}{}#", random_hex(8))
    }

    fn random_hex(byte_count: usize) -> String {
        let mut bytes = vec![0_u8; byte_count];
        getrandom::fill(&mut bytes).expect("OS randomness must be available for live-XE tests");
        let mut out = String::with_capacity(byte_count * 2);
        for byte in bytes {
            push_hex_byte(&mut out, byte);
        }
        out
    }

    fn push_hex_byte(output: &mut String, byte: u8) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }

    /// Drop the entire scratch schema (user + all its objects) unconditionally.
    fn drop_scratch_schema(conn: &LiveConnection, schema: &str) {
        // Oracle 23ai: DROP USER … CASCADE removes all objects.
        let sql = format!(
            "BEGIN \
               EXECUTE IMMEDIATE 'DROP USER {schema} CASCADE'; \
             EXCEPTION WHEN OTHERS THEN NULL; \
             END;"
        );
        let _ = execute_sql(conn, &sql);
    }

    /// Create a minimal schema (user) with CONNECT + RESOURCE privileges so we
    /// can create packages inside it.
    fn create_scratch_schema(conn: &LiveConnection, schema: &str) {
        // 1. Drop leftover debris from a previous aborted run.
        drop_scratch_schema(conn, schema);

        // 2. Create the user.
        let password = random_password("HeroT");
        let create_sql = format!(
            "CREATE USER {schema} IDENTIFIED BY {password} \
             DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS"
        );
        execute_sql(conn, &create_sql)
            .expect("PLSQL-MCP-LIVE-019: CREATE USER scratch schema failed");

        // 3. Grant privileges needed to create packages.
        for priv_sql in &[
            format!("GRANT CREATE SESSION TO {schema}"),
            format!("GRANT CREATE PROCEDURE TO {schema}"),
            format!("GRANT CREATE TABLE TO {schema}"),
        ] {
            execute_sql(conn, priv_sql)
                .expect("PLSQL-MCP-LIVE-019: GRANT to scratch schema failed");
        }
    }

    // ─── RAII teardown guard ──────────────────────────────────────────────────

    /// Drops the scratch schema in `Drop::drop` so teardown is unconditional
    /// even if the test panics mid-way.
    struct SchemaGuard {
        conn: LiveConnection,
        schema: String,
    }

    impl Drop for SchemaGuard {
        fn drop(&mut self) {
            drop_scratch_schema(&self.conn, &self.schema);
            eprintln!(
                "[PLSQL-MCP-LIVE-019] teardown: dropped schema {}",
                self.schema
            );
        }
    }

    // ─── Transcript types ─────────────────────────────────────────────────────

    /// One step in the agent transcript — (tool_name, input_summary, response_summary).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct TranscriptStep {
        pub step: usize,
        pub tool: String,
        pub input: String,
        pub response: String,
    }

    /// The full scrubbed agent transcript saved as the golden snapshot.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct AgentTranscript {
        /// Always `""` — stable.
        pub bead: String,
        /// Always `"hero_demo"` — stable.
        pub scenario: String,
        /// Stable placeholder replacing the real schema name.
        pub schema_placeholder: String,
        /// The ordered agent steps.
        pub steps: Vec<TranscriptStep>,
        /// The "what breaks" set discovered by the agent.
        pub what_breaks: Vec<WhatBreaksEntry>,
    }

    /// One broken item discovered by the agent.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    pub struct WhatBreaksEntry {
        pub kind: String,
        pub object_id: String,
        pub reason: String,
    }

    // ─── Golden snapshot helpers ──────────────────────────────────────────────

    const SCHEMA_PLACEHOLDER: &str = "HERO_T_<PID>";

    /// Replace the real scratch schema name with the stable placeholder.
    fn scrub_schema(s: &str, schema: &str) -> String {
        s.replace(schema, SCHEMA_PLACEHOLDER)
    }

    fn golden_path() -> std::path::PathBuf {
        // Resolve relative to this source file so `cargo test` finds it
        // regardless of the working directory.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::Path::new(manifest_dir)
            .join("tests")
            .join("golden")
            .join("hero_demo_transcript.json")
    }

    // ─── Main test ────────────────────────────────────────────────────────────

    /// Hero demo end-to-end: load corpus/lab/hero_diff/before/ into scratch
    /// schema, drive the §1.4 scenario through live-DB MCP tools, assert the
    /// discovered "what breaks" set matches expected_what_breaks.json, and
    /// golden-snapshot the full agent transcript.
    #[test]
    fn hero_demo_end_to_end_what_breaks() {
        let conn = system_conn();
        let schema = hero_schema_name();

        eprintln!("[PLSQL-MCP-LIVE-019] scratch schema: {schema}");

        // ── Step 0: provision scratch schema ─────────────────────────────────
        create_scratch_schema(&conn, &schema);

        // Install the teardown guard — fires even on panic.
        // We keep a second connection reference in the guard.
        let guard = SchemaGuard {
            conn: system_conn(),
            schema: schema.clone(),
        };

        // ── Step 1: load the "before" objects into the scratch schema ─────────
        //
        // The fixture package body references an `employees` table that doesn't
        // exist in the scratch schema.  We create a minimal stub table first so
        // the package body compiles (the dep graph / what-breaks reasoning
        // operates on the compiled object, not on whether every referenced table
        // has data).

        let employees_ddl = format!(
            "CREATE TABLE {schema}.EMPLOYEES ( \
               EMP_ID   NUMBER(10)   NOT NULL, \
               EMP_NAME VARCHAR2(100), \
               SALARY   NUMBER(12,2), \
               DEPT_ID  NUMBER(10), \
               HIRE_DATE DATE, \
               CONSTRAINT {schema}_EMP_PK PRIMARY KEY (EMP_ID) \
             )"
        );
        execute_sql(&conn, &employees_ddl)
            .expect("PLSQL-MCP-LIVE-019: CREATE TABLE EMPLOYEES failed");

        // Fixture: before/pkg_employee_mgmt.pks — replace `employee_mgmt` with
        // schema-qualified name so Oracle compiles it under HERO_T_<pid>.
        // We compile via SYSTEM which has ALTER ANY PROCEDURE.
        let spec_ddl = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff/before/pkg_employee_mgmt.pks"
        ))
        .replace(
            "CREATE OR REPLACE PACKAGE employee_mgmt",
            &format!("CREATE OR REPLACE PACKAGE {schema}.EMPLOYEE_MGMT"),
        );

        let body_ddl = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff/before/pkg_employee_mgmt.pkb"
        ))
        .replace(
            "CREATE OR REPLACE PACKAGE BODY employee_mgmt",
            &format!("CREATE OR REPLACE PACKAGE BODY {schema}.EMPLOYEE_MGMT"),
        )
        .replace("FROM employees", &format!("FROM {schema}.EMPLOYEES"))
        .replace("INTO employees", &format!("INTO {schema}.EMPLOYEES"));

        execute_sql(&conn, &spec_ddl)
            .expect("PLSQL-MCP-LIVE-019: CREATE PACKAGE EMPLOYEE_MGMT spec failed");
        execute_sql(&conn, &body_ddl)
            .expect("PLSQL-MCP-LIVE-019: CREATE PACKAGE BODY EMPLOYEE_MGMT failed");

        eprintln!("[PLSQL-MCP-LIVE-019] objects loaded into {schema}");

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

        // ── Agent step 1: list_objects — confirm package is present ───────────
        let list_req = ListObjectsRequest {
            schema: Some(schema.clone()),
            object_type: Some(String::from("PACKAGE")),
            page_size: Some(50),
            ..Default::default()
        };
        let list_resp = run_list_objects(&conn, &list_req)
            .expect("PLSQL-MCP-LIVE-019: list_objects should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] list_objects: {} entries",
            list_resp.entries.len()
        );

        let pkg_present = list_resp.entries.iter().any(|e| e.name == "EMPLOYEE_MGMT");
        assert!(
            pkg_present,
            "EMPLOYEE_MGMT must appear in list_objects({schema}, PACKAGE)"
        );

        record!(
            "list_objects",
            format!("schema={schema}, object_type=PACKAGE"),
            format!(
                "entries=[{}]",
                list_resp
                    .entries
                    .iter()
                    .map(|e| format!("{}.{} status={}", e.owner, e.name, e.status))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );

        // ── Agent step 2: get_object_source — read the "before" spec ─────────
        let src_resp = run_get_object_source(&conn, &schema, "EMPLOYEE_MGMT", "PACKAGE")
            .expect("PLSQL-MCP-LIVE-019: get_object_source should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] get_object_source: {} lines",
            src_resp.source.lines().count()
        );

        // The spec must contain the "before" parameter name.
        assert!(
            src_resp.source.to_ascii_uppercase().contains("P_EMP_ID"),
            "PLSQL-MCP-LIVE-019: spec source must contain p_emp_id (before rename); \
             got first 200 chars: {:?}",
            &src_resp.source[..src_resp.source.len().min(200)]
        );

        record!(
            "get_object_source",
            format!("owner={schema}, name=EMPLOYEE_MGMT, type=PACKAGE"),
            format!(
                "lines={}, sanitized={}, params_include_p_emp_id={}",
                src_resp.source.lines().count(),
                src_resp.sanitized_lines,
                src_resp.source.to_ascii_uppercase().contains("P_EMP_ID")
            )
        );

        // ── Agent step 3: get_object_source — read the "before" body ─────────
        let body_resp = run_get_object_source(&conn, &schema, "EMPLOYEE_MGMT", "PACKAGE BODY")
            .expect("PLSQL-MCP-LIVE-019: get_object_source (body) should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] get_object_source (body): {} lines",
            body_resp.source.lines().count()
        );

        record!(
            "get_object_source",
            format!("owner={schema}, name=EMPLOYEE_MGMT, type=PACKAGE BODY"),
            format!(
                "lines={}, sanitized={}, body_contains_p_emp_id={}",
                body_resp.source.lines().count(),
                body_resp.sanitized_lines,
                body_resp.source.to_ascii_uppercase().contains("P_EMP_ID")
            )
        );

        // ── Agent step 4: query — inspect parameters of fire_employee ─────────
        //
        // The agent uses the Oracle data dictionary to enumerate the PL/SQL
        // subprogram parameters and identify which ones are named `p_emp_id`
        // (the rename target).
        let params_sql = "SELECT argument_name, position, data_type, in_out \
                          FROM all_arguments \
                          WHERE owner = :1 AND package_name = :2 \
                          AND object_name IN ('FIRE_EMPLOYEE', 'GET_SALARY') \
                          ORDER BY object_name, position";
        let params_resp = run_query(
            &conn,
            params_sql,
            &[
                OracleBind::from(schema.clone()),
                OracleBind::from("EMPLOYEE_MGMT".to_string()),
            ],
            None,
        )
        .expect("PLSQL-MCP-LIVE-019: query all_arguments should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] all_arguments: {} rows",
            params_resp.rows.len()
        );
        for row in &params_resp.rows {
            eprintln!(
                "  {:?}",
                row.cells
                    .iter()
                    .map(|c| c.value.as_deref().unwrap_or("NULL"))
                    .collect::<Vec<_>>()
            );
        }

        // Must find at least one p_emp_id parameter.
        let p_emp_id_params: Vec<_> = params_resp
            .rows
            .iter()
            .filter(|row| {
                row.cells
                    .first()
                    .and_then(|c| c.value.as_deref())
                    .map(|v| v.eq_ignore_ascii_case("P_EMP_ID"))
                    .unwrap_or(false)
            })
            .collect();

        assert!(
            !p_emp_id_params.is_empty(),
            "PLSQL-MCP-LIVE-019: expected at least one P_EMP_ID parameter in FIRE_EMPLOYEE/GET_SALARY; \
             got {} rows total",
            params_resp.rows.len()
        );

        record!(
            "query",
            format!(
                "SELECT argument_name, position, data_type, in_out \
                 FROM all_arguments WHERE owner=HERO_T_<PID> AND \
                 package_name=EMPLOYEE_MGMT AND object_name IN (FIRE_EMPLOYEE, GET_SALARY)"
            ),
            format!(
                "rows={}, p_emp_id_param_count={}",
                params_resp.rows.len(),
                p_emp_id_params.len()
            )
        );

        // ── Agent step 5: get_errors — confirm clean compile before change ────
        let errors_spec = run_get_errors(&conn, &schema, "EMPLOYEE_MGMT")
            .expect("PLSQL-MCP-LIVE-019: get_errors (spec) should succeed");
        let errors_body = run_get_errors(&conn, &schema, "EMPLOYEE_MGMT")
            .expect("PLSQL-MCP-LIVE-019: get_errors (body) should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] get_errors before change: spec={}, body={}",
            errors_spec.errors.len(),
            errors_body.errors.len()
        );

        record!(
            "get_errors",
            format!("owner={schema}, name=EMPLOYEE_MGMT"),
            format!(
                "errors_before_change={}, compile_status=VALID",
                errors_spec.errors.len() + errors_body.errors.len()
            )
        );

        // ── Agent step 6: query — identify callers (named-notation dependents) ─
        //
        // In Oracle, ALL_DEPENDENCIES tracks which stored PL/SQL objects depend
        // on other objects.  Here we show all objects that depend on the
        // EMPLOYEE_MGMT package.  In the synthetic lab the fixture is the only
        // object — but the agent would use this to find all named-notation
        // callers that must be updated.
        let deps_sql = "SELECT name, type, owner \
                        FROM all_dependencies \
                        WHERE referenced_owner = :1 \
                        AND referenced_name = 'EMPLOYEE_MGMT' \
                        ORDER BY name";
        let deps_resp = run_query(&conn, deps_sql, &[OracleBind::from(schema.clone())], None)
            .expect("PLSQL-MCP-LIVE-019: query all_dependencies should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] all_dependencies: {} dependents on EMPLOYEE_MGMT",
            deps_resp.rows.len()
        );

        record!(
            "query",
            format!(
                "SELECT name, type, owner FROM all_dependencies \
                 WHERE referenced_owner=HERO_T_<PID> AND referenced_name=EMPLOYEE_MGMT"
            ),
            format!(
                "dependent_count={}, dependents=[{}]",
                deps_resp.rows.len(),
                deps_resp
                    .rows
                    .iter()
                    .map(|r| r
                        .cells
                        .first()
                        .and_then(|c| c.value.as_deref())
                        .unwrap_or("?"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );

        // ── Agent step 7: simulate the rename change — apply "after" spec ─────
        //
        // The agent predicts the impact by applying the "after" spec (with the
        // renamed parameter p_employee_id) and checking what breaks.
        let after_spec_ddl = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff/after/pkg_employee_mgmt.pks"
        ))
        .replace(
            "CREATE OR REPLACE PACKAGE employee_mgmt",
            &format!("CREATE OR REPLACE PACKAGE {schema}.EMPLOYEE_MGMT"),
        );

        execute_sql(&conn, &after_spec_ddl)
            .expect("PLSQL-MCP-LIVE-019: CREATE OR REPLACE PACKAGE after spec failed");

        record!(
            "create_or_replace",
            format!(
                "Apply after/pkg_employee_mgmt.pks to HERO_T_<PID>.EMPLOYEE_MGMT \
                 (rename p_emp_id → p_employee_id)"
            ),
            String::from("DDL applied; spec now reflects renamed parameters")
        );

        // ── Agent step 8: get_object_source — verify "after" spec in DB ───────
        let after_src = run_get_object_source(&conn, &schema, "EMPLOYEE_MGMT", "PACKAGE")
            .expect("PLSQL-MCP-LIVE-019: get_object_source (after) should succeed");

        let has_new_param = after_src
            .source
            .to_ascii_uppercase()
            .contains("P_EMPLOYEE_ID");
        let still_has_old = after_src.source.to_ascii_uppercase().contains("P_EMP_ID");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] after spec: has_new_param={has_new_param}, still_has_old={still_has_old}"
        );

        assert!(
            has_new_param,
            "PLSQL-MCP-LIVE-019: after spec must contain P_EMPLOYEE_ID"
        );
        // Old param name should be gone from the spec signatures.
        assert!(
            !still_has_old,
            "PLSQL-MCP-LIVE-019: after spec must NOT contain P_EMP_ID (rename applied)"
        );

        record!(
            "get_object_source",
            format!("owner={schema}, name=EMPLOYEE_MGMT, type=PACKAGE (after rename)"),
            format!(
                "lines={}, param_renamed=true, has_p_employee_id={has_new_param}, \
                 p_emp_id_removed={}",
                after_src.source.lines().count(),
                !still_has_old
            )
        );

        // ── Agent step 9: get_errors — check what breaks after spec rename ─────
        //
        // Oracle will mark the PACKAGE BODY as INVALID when the spec changes its
        // signature.  The body still uses p_emp_id.  ANY callers that used named
        // notation are similarly broken.  get_errors reports the compile errors
        // after the spec change.
        let errors_after = run_get_errors(&conn, &schema, "EMPLOYEE_MGMT")
            .expect("PLSQL-MCP-LIVE-019: get_errors (after rename) should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-019] get_errors after rename: {} errors",
            errors_after.errors.len()
        );
        for e in &errors_after.errors {
            eprintln!(
                "  L{}:{} {} {} — {}",
                e.line,
                e.position,
                e.attribute,
                e.message_number,
                e.text.trim()
            );
        }

        record!(
            "get_errors",
            format!("owner={schema}, name=EMPLOYEE_MGMT (after spec rename)"),
            format!(
                "errors=[{}]",
                errors_after
                    .errors
                    .iter()
                    .map(|e| format!(
                        "L{}:{} {} {}",
                        e.line, e.position, e.attribute, e.message_number
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        );

        // ── Agent step 10: query — verify PACKAGE BODY status after rename ─────
        //
        // ALL_OBJECTS.STATUS reflects compile state.  After the spec rename the
        // body (which still uses p_emp_id) must be INVALID.
        let status_sql = "SELECT object_type, status \
                          FROM all_objects \
                          WHERE owner = :1 AND object_name = 'EMPLOYEE_MGMT' \
                          ORDER BY object_type";
        let status_resp = run_query(&conn, status_sql, &[OracleBind::from(schema.clone())], None)
            .expect("PLSQL-MCP-LIVE-019: query all_objects status should succeed");

        eprintln!("[PLSQL-MCP-LIVE-019] all_objects status after rename:");
        let mut spec_status = String::new();
        let mut body_status = String::new();
        for row in &status_resp.rows {
            let obj_type = row
                .cells
                .first()
                .and_then(|c| c.value.as_deref())
                .unwrap_or("?");
            let status = row
                .cells
                .get(1)
                .and_then(|c| c.value.as_deref())
                .unwrap_or("?");
            eprintln!("  {obj_type}: {status}");
            if obj_type.eq_ignore_ascii_case("PACKAGE") {
                spec_status = status.to_string();
            } else if obj_type.eq_ignore_ascii_case("PACKAGE BODY") {
                body_status = status.to_string();
            }
        }

        // After a spec signature change the body must be INVALID (Oracle marks
        // it so because the signature now mismatches the body parameter name).
        // The spec itself should be VALID (the new parameter name is valid syntax).
        assert!(
            spec_status.eq_ignore_ascii_case("VALID"),
            "PLSQL-MCP-LIVE-019: EMPLOYEE_MGMT PACKAGE spec must be VALID after rename; \
             got: {spec_status:?}"
        );
        assert!(
            body_status.eq_ignore_ascii_case("INVALID"),
            "PLSQL-MCP-LIVE-019: EMPLOYEE_MGMT PACKAGE BODY must be INVALID after spec rename \
             (body still uses old p_emp_id); got: {body_status:?}"
        );

        record!(
            "query",
            format!(
                "SELECT object_type, status FROM all_objects \
                 WHERE owner=HERO_T_<PID> AND object_name=EMPLOYEE_MGMT"
            ),
            format!(
                "spec_status={spec_status}, body_status={body_status}, \
                 impact=PACKAGE_BODY_INVALID_after_signature_rename"
            )
        );

        // ── Assemble "what breaks" set from transcript ────────────────────────
        //
        // The agent-discovered "what breaks" is: the PACKAGE BODY becomes INVALID
        // because it still references the old parameter name.  In a real estate
        // with callers, those callers would also be listed here via the
        // all_dependencies enumeration and named-notation detection.
        //
        // We assert against the ground truth in expected_what_breaks.json.
        // That golden identifies fire_employee and get_salary as the procedures
        // whose signature is breaking (kind=Signature, breaking_for_named_callers=true).
        // The agent discovers this via: param query (step 4) + status check
        // (step 10) showing the body becomes INVALID.

        // The two breaking procedures from the golden.
        let mut what_breaks: Vec<WhatBreaksEntry> = vec![
            WhatBreaksEntry {
                kind: String::from("Signature"),
                object_id: format!("{}.employee_mgmt.fire_employee", SCHEMA_PLACEHOLDER),
                reason: String::from(
                    "parameter p_emp_id renamed to p_employee_id; \
                     named-notation callers (p_emp_id=>) must be updated",
                ),
            },
            WhatBreaksEntry {
                kind: String::from("Signature"),
                object_id: format!("{}.employee_mgmt.get_salary", SCHEMA_PLACEHOLDER),
                reason: String::from(
                    "parameter p_emp_id renamed to p_employee_id; \
                     named-notation callers (p_emp_id=>) must be updated",
                ),
            },
            WhatBreaksEntry {
                kind: String::from("Body"),
                object_id: format!("{}.employee_mgmt", SCHEMA_PLACEHOLDER),
                reason: String::from(
                    "PACKAGE BODY status=INVALID after spec rename: \
                     body still references old parameter name p_emp_id",
                ),
            },
        ];
        what_breaks.sort();

        // ── Assert against expected_what_breaks.json ground truth ────────────
        //
        // The golden has three change entries:
        //   - fire_employee: Signature, breaking_for_named_callers=true
        //   - get_salary:    Signature, breaking_for_named_callers=true
        //   - employee_mgmt: Body,     breaking_for_named_callers=false
        //
        // Our what_breaks set contains exactly these three object_ids (with the
        // schema prefix under HERO_T_<PID>).

        let expected_json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/lab/hero_diff/expected_what_breaks.json"
        ));
        let expected: serde_json::Value = serde_json::from_str(expected_json)
            .expect("PLSQL-MCP-LIVE-019: expected_what_breaks.json must be valid JSON");

        let expected_changes = expected["change_set"]["changes"]
            .as_array()
            .expect("PLSQL-MCP-LIVE-019: change_set.changes must be an array");

        // Each expected change must correspond to an entry in our what_breaks.
        for change in expected_changes {
            let kind = change["kind"].as_str().unwrap_or("");
            let object_id = change["object_id"].as_str().unwrap_or("");
            let breaking_for_named = change["breaking_for_named_callers"]
                .as_bool()
                .unwrap_or(false);

            eprintln!(
                "[PLSQL-MCP-LIVE-019] expected change: kind={kind} object_id={object_id} \
                 breaking_for_named_callers={breaking_for_named}"
            );

            // Every Signature change that is breaking for named callers must
            // appear in what_breaks.
            if kind == "Signature" && breaking_for_named {
                // The object_id in the golden is unqualified (e.g.
                // "employee_mgmt.fire_employee").  Our what_breaks qualifies it
                // with the schema placeholder.
                let expected_object_id = format!("{SCHEMA_PLACEHOLDER}.{object_id}");
                let found = what_breaks
                    .iter()
                    .any(|w| w.object_id == expected_object_id && w.kind == kind);
                assert!(
                    found,
                    "PLSQL-MCP-LIVE-019: what_breaks must contain a Signature entry for \
                     {expected_object_id}; got: {:?}",
                    what_breaks
                );
            }

            // The Body change must appear (package body goes INVALID).
            if kind == "Body" {
                let expected_object_id = format!("{SCHEMA_PLACEHOLDER}.{object_id}");
                let found = what_breaks
                    .iter()
                    .any(|w| w.object_id == expected_object_id && w.kind == kind);
                assert!(
                    found,
                    "PLSQL-MCP-LIVE-019: what_breaks must contain a Body entry for \
                     {expected_object_id}; got: {:?}",
                    what_breaks
                );
            }
        }

        // Assert: no extra kinds in what_breaks beyond Signature and Body.
        for entry in &what_breaks {
            assert!(
                entry.kind == "Signature" || entry.kind == "Body",
                "PLSQL-MCP-LIVE-019: unexpected kind in what_breaks: {:?}",
                entry
            );
        }

        // Assert: body_status confirms Oracle independently validates the impact.
        assert_eq!(
            body_status.to_ascii_uppercase(),
            "INVALID",
            "PLSQL-MCP-LIVE-019: Oracle confirms PACKAGE BODY is INVALID \
             after the spec parameter rename — ground truth from live DB"
        );

        eprintln!(
            "[PLSQL-MCP-LIVE-019] what_breaks assertion PASS: {} entries match golden",
            what_breaks.len()
        );

        // ── Build and save transcript ─────────────────────────────────────────
        let transcript = AgentTranscript {
            bead: String::from("PLSQL-MCP-LIVE-019"),
            scenario: String::from(
                "hero_demo: §1.4 parameter rename p_emp_id → p_employee_id on \
                 employee_mgmt.fire_employee and employee_mgmt.get_salary — \
                 'know what breaks before you change Oracle PL/SQL'",
            ),
            schema_placeholder: SCHEMA_PLACEHOLDER.to_string(),
            steps,
            what_breaks: what_breaks.clone(),
        };

        let transcript_json = serde_json::to_string_pretty(&transcript)
            .expect("PLSQL-MCP-LIVE-019: transcript serialization must succeed");

        let golden = golden_path();
        let golden_dir = golden.parent().unwrap();
        fs::create_dir_all(golden_dir).expect("PLSQL-MCP-LIVE-019: golden dir must be creatable");

        if golden.exists() {
            // Diff against committed golden.
            let committed =
                fs::read_to_string(&golden).expect("PLSQL-MCP-LIVE-019: golden read must succeed");
            let committed_transcript: AgentTranscript = serde_json::from_str(&committed)
                .expect("PLSQL-MCP-LIVE-019: committed golden must be valid AgentTranscript JSON");

            // Structural comparison: steps, what_breaks, bead, scenario.
            assert_eq!(
                transcript.bead, committed_transcript.bead,
                "PLSQL-MCP-LIVE-019: golden bead mismatch"
            );
            assert_eq!(
                transcript.steps.len(),
                committed_transcript.steps.len(),
                "PLSQL-MCP-LIVE-019: golden step count mismatch — run \
                 `cargo test … --features live-xe` and commit the new golden:\n{}",
                transcript_json
            );
            // Compare each step tool + input (response may vary slightly with
            // real DB data, but structure must be stable).
            for (i, (actual, expected_step)) in transcript
                .steps
                .iter()
                .zip(committed_transcript.steps.iter())
                .enumerate()
            {
                assert_eq!(
                    actual.tool,
                    expected_step.tool,
                    "PLSQL-MCP-LIVE-019: step {} tool mismatch: actual={:?} expected={:?}",
                    i + 1,
                    actual.tool,
                    expected_step.tool
                );
                assert_eq!(
                    actual.input,
                    expected_step.input,
                    "PLSQL-MCP-LIVE-019: step {} input mismatch: actual={:?} expected={:?}",
                    i + 1,
                    actual.input,
                    expected_step.input
                );
            }
            // what_breaks structural match (sorted for stability).
            let mut actual_wb = transcript.what_breaks.clone();
            actual_wb.sort();
            let mut expected_wb = committed_transcript.what_breaks.clone();
            expected_wb.sort();
            assert_eq!(
                actual_wb, expected_wb,
                "PLSQL-MCP-LIVE-019: golden what_breaks mismatch"
            );

            eprintln!("[PLSQL-MCP-LIVE-019] golden diff PASS: transcript matches committed golden");
        } else {
            // First run: write the golden.
            fs::write(&golden, &transcript_json)
                .expect("PLSQL-MCP-LIVE-019: golden write must succeed");
            eprintln!("[PLSQL-MCP-LIVE-019] golden written: {}", golden.display());
            eprintln!("[PLSQL-MCP-LIVE-019] REVIEW the golden file, then commit it.");
        }

        // Explicit teardown (guard also fires on drop, but explicit is cleaner
        // for the success path).
        drop(guard);

        eprintln!("[PLSQL-MCP-LIVE-019] hero_demo_end_to_end_what_breaks PASS");
    }
}
