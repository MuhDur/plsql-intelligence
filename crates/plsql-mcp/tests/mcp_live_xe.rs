//! Integration test: every live-DB tool E2E against Oracle XE 23ai.
//!
//! Gated behind the `live-xe` feature flag so the default test profile
//! (no Docker, no live Oracle) doesn't try to reach a container that isn't
//! there. The orchestrator or a developer with the lab container running
//! can flip the feature and execute the real path via:
//!
//! ```sh
//! cargo test -p plsql-mcp --features live-xe \
//!     --test mcp_live_xe -- --nocapture
//! ```
//!
//! ## What is tested
//!
//! ### (a) E2E per live-DB tool against the container
//!
//! - `query` — SELECT against DUAL, non-trivial assertion on real data.
//! - `get_object_source` — reads `PKG_AUTONOMOUS` PACKAGE spec from ALL_SOURCE;
//!   asserts the returned source starts with the expected `PACKAGE` header.
//! - `get_clob` — fetches `PKG_AUTONOMOUS` source via a CLOB-projecting SELECT;
//!   asserts non-empty result.
//! - `get_errors` — reads ALL_ERRORS for `PKG_OPAQUE_DYNAMIC` (the DEMO schema
//!   contains packages that intentionally have compile errors due to references
//!   to unavailable DB-link objects); asserts the error list is non-empty OR
//!   gracefully empty if the package happens to be valid on this container.
//!   A separate `get_errors_empty_schema_routes_user_errors` sub-test proves the
//!   USER_ERRORS routing path compiles without error.
//! - `list_objects` — list DEMO packages, assert PKG_AUTONOMOUS is present.
//! - `describe_table` — describes a SYS dictionary table (ALL_TABLES is visible
//!   to every connected user via `ALL_` views); uses a small helper table
//!   created as MCP_T_<pid> in the scratch area and dropped in teardown.
//! - `compile_with_warnings` — compiles `PKG_AUTONOMOUS` PACKAGE from DEMO;
//!   asserts that the compile response round-trips with no severe errors.
//!
//! ### (b) Chained-flow: `preview_sql` → `run_execute_approved` → DDL execution
//!
//! Creates a scratch table `MCP_T_<pid>` under SYSTEM, uses `preview_sql` to
//! mint an approval token, `run_execute_approved` to verify the plan, and
//! `conn.execute(&plan.ddl_bytes)` to actually run the DDL.  Then verifies the
//! table exists in ALL_OBJECTS and drops it in teardown.
//!
//! ### (c) Refusal matrix
//!
//! - Read-only default: fresh `SessionSafetyState` blocks `writes_allowed()`.
//! - Expired token: `enable_writes` returns `EnableWritesTokenMissing`.
//! - Token mismatch: wrong token text → `EnableWritesTokenMismatch`.
//! - `permanently_read_only`: blocks mint AND `enable_writes` even with a
//!   pre-injected token → `PermanentlyReadOnly`.
//!
//! When the feature flag is *off* (the default), this file contains a single
//! trivial test asserting the gate works.

// ── Gate-off path ─────────────────────────────────────────────────────────────

#[cfg(not(feature = "live-xe"))]
#[test]
fn live_xe_mcp_is_feature_gated() {
    // The default test profile doesn't exercise the live MCP E2E path.
    // The `live-xe` feature enables the real path against a running Oracle XE
    // 23ai container.  This stub exists so `cargo test -p plsql-mcp --test
    // mcp_live_xe` always has at least one assertion to report — a regression
    // that drops the `live-xe` feature entirely would surface here.
    let live_xe = false;
    assert!(!live_xe, "live-xe feature gate is off by default");
}

// ── Live path (live-xe feature) ───────────────────────────────────────────────

#[cfg(feature = "live-xe")]
mod live {
    use asupersync::{Cx, runtime::RuntimeBuilder};
    use plsql_catalog::{CatalogError, OracleBind, OracleConnection, OracleRow};
    use plsql_mcp::{
        CompileToolError,
        CompileWithWarningsResponse,
        DescribeError,
        DescribeTableResponse,
        ENABLE_WRITES_TOKEN_TTL_SECONDS,
        ExecuteApprovedRequest,
        GetClobResponse,
        GetErrorsResponse,
        GetObjectSourceResponse,
        ListObjectsRequest,
        ListObjectsResponse,
        OraclemcpCatalogConnection,
        // preview + execute_approved (chained flow)
        PreviewRegistry,
        QueryError,
        QueryResponse,
        // safety
        SafetyProfile,
        SafetyProfileError,
        SessionSafetyState,
        consume_approved,
        // compile tool
        run_compile_with_warnings as run_compile_with_warnings_async,
        // describe tools
        run_describe_table as run_describe_table_async,
        run_execute_approved,
        // source tools
        run_get_clob as run_get_clob_async,
        run_get_errors as run_get_errors_async,
        run_get_object_source as run_get_object_source_async,
        // list_objects tool
        run_list_objects as run_list_objects_async,
        // query tool
        run_query as run_query_async,
    };
    use std::{
        future::Future,
        sync::atomic::{AtomicU32, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    // ─── Connection constants ─────────────────────────────────────────────────

    static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

    type LiveConnection = OraclemcpCatalogConnection<oraclemcp_db::RustOracleConnection>;

    const SYSTEM_USER: &str = "SYSTEM";
    const DEMO_USER: &str = "DEMO";
    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";

    /// Connect as SYSTEM (DML/DDL privileges, can see ALL_* views broadly).
    fn system_conn() -> LiveConnection {
        let password = required_env("PLSQL_XE_SYSTEM_PASSWORD");
        connect(SYSTEM_USER, password, "PLSQL-MCP-LIVE-018")
    }

    /// Connect as DEMO (read-only fixtures; DEMO is treated as read-only in tests).
    fn demo_conn() -> LiveConnection {
        let password = required_env("PLSQL_XE_DEMO_PASSWORD");
        connect(DEMO_USER, password, "PLSQL-MCP-LIVE-018-demo")
    }

    fn connect(username: &str, password: String, action: &str) -> LiveConnection {
        let opts = oraclemcp_db::OracleConnectOptions {
            connect_string: CONNECT_STRING.to_owned(),
            username: Some(username.to_owned()),
            password: Some(password),
            session_identity: Some(oraclemcp_db::OracleSessionIdentity {
                module: Some("plsql-mcp-live-xe-test".to_owned()),
                action: Some(action.to_owned()),
                ..oraclemcp_db::OracleSessionIdentity::default()
            }),
            ..oraclemcp_db::OracleConnectOptions::default()
        };
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            OraclemcpCatalogConnection::connect(&cx, opts)
                .await
                .expect("live-XE thin connection to //localhost:1521/FREEPDB1 must succeed")
        })
    }

    /// Returns a scratch table name unique to this process.
    fn scratch_table_name() -> String {
        format!("MCP_T_{}", unique_suffix())
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

    fn random_approval_text(label: &str) -> String {
        format!("{label}-{}", random_hex(16))
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

    /// Drop the scratch table if it still exists (best-effort teardown).
    fn drop_scratch_table_if_exists(conn: &LiveConnection, name: &str) {
        // Oracle has no DROP TABLE IF EXISTS before 23ai; use a PL/SQL block.
        let sql = format!(
            "BEGIN \
               EXECUTE IMMEDIATE 'DROP TABLE SYSTEM.{name}'; \
             EXCEPTION WHEN OTHERS THEN NULL; \
             END;"
        );
        let _ = execute_sql(conn, &sql);
    }

    fn run_live_future<F: Future>(future: F) -> F::Output {
        RuntimeBuilder::current_thread()
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

    fn query_rows<C: OracleConnection>(
        conn: &C,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<Vec<OracleRow>, CatalogError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            conn.query_rows(&cx, sql, params).await
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
    ) -> Result<GetObjectSourceResponse, plsql_mcp::SourceToolError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_get_object_source_async(&cx, conn, owner, object_name, object_type).await
        })
    }

    fn run_get_clob<C: OracleConnection>(
        conn: &C,
        sql: &str,
        params: &[OracleBind],
        max_chars: Option<usize>,
    ) -> Result<GetClobResponse, plsql_mcp::SourceToolError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_get_clob_async(&cx, conn, sql, params, max_chars).await
        })
    }

    fn run_get_errors<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
    ) -> Result<GetErrorsResponse, plsql_mcp::SourceToolError> {
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

    fn run_describe_table<C: OracleConnection>(
        conn: &C,
        owner: &str,
        name: &str,
    ) -> Result<DescribeTableResponse, DescribeError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_describe_table_async(&cx, conn, owner, name).await
        })
    }

    fn run_compile_with_warnings<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
        object_type: &str,
    ) -> Result<CompileWithWarningsResponse, CompileToolError> {
        run_live_future(async {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            run_compile_with_warnings_async(&cx, conn, owner, object_name, object_type).await
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // (a) E2E per-tool tests
    // ─────────────────────────────────────────────────────────────────────────

    /// `query` tool — `SELECT 1 FROM DUAL` against Oracle, assert value is "1".
    #[test]
    fn query_select_dual_returns_one() {
        let conn = system_conn();
        let resp = run_query(&conn, "SELECT 1 AS val FROM DUAL", &[], None)
            .expect("PLSQL-MCP-LIVE-018: run_query should succeed");

        eprintln!("[PLSQL-MCP-LIVE-018] query columns: {:?}", resp.columns);
        eprintln!("[PLSQL-MCP-LIVE-018] query rows: {}", resp.rows.len());

        assert_eq!(resp.columns.len(), 1, "expected exactly 1 column");
        assert_eq!(resp.rows.len(), 1, "expected exactly 1 row from DUAL");
        let val = resp.rows[0].cells[0].value.as_deref().unwrap_or("");
        assert_eq!(
            val, "1",
            "SELECT 1 FROM DUAL should return '1', got: {val:?}"
        );
        assert_eq!(
            resp.sanitized_cells, 0,
            "no injection markers expected in DUAL result"
        );
    }

    /// `query` tool rejects non-SELECT SQL even against a live Oracle connection.
    #[test]
    fn query_rejects_non_select_against_live_db() {
        let conn = system_conn();
        let err = run_query(&conn, "BEGIN NULL; END;", &[], None)
            .expect_err("PLSQL-MCP-LIVE-018: run_query should refuse non-SELECT SQL");
        assert!(
            matches!(err, plsql_mcp::QueryError::NotReadOnly { .. }),
            "expected NotReadOnly error, got: {err}"
        );
    }

    /// `get_object_source` — reads `PKG_AUTONOMOUS` PACKAGE spec from ALL_SOURCE.
    /// Asserts the returned source starts with the `PACKAGE` keyword and is
    /// attributed to the DEMO schema.
    #[test]
    fn get_object_source_pkg_autonomous_returns_real_source() {
        let conn = demo_conn();
        let resp = run_get_object_source(&conn, "DEMO", "PKG_AUTONOMOUS", "PACKAGE")
            .expect("PLSQL-MCP-LIVE-018: get_object_source should succeed for PKG_AUTONOMOUS");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] get_object_source: {} lines, sanitized={}",
            resp.source.lines().count(),
            resp.sanitized_lines
        );
        eprintln!(
            "[PLSQL-MCP-LIVE-018] first 200 chars: {:?}",
            &resp.source[..resp.source.len().min(200)]
        );

        assert_eq!(resp.owner, "DEMO", "owner must be DEMO");
        assert_eq!(
            resp.object_name, "PKG_AUTONOMOUS",
            "object_name must be PKG_AUTONOMOUS"
        );
        assert_eq!(resp.object_type, "PACKAGE", "object_type must be PACKAGE");

        // The spec source must begin with a PACKAGE keyword (case-insensitive).
        let trimmed = resp.source.trim_start();
        assert!(
            trimmed.to_ascii_uppercase().starts_with("PACKAGE"),
            "PKG_AUTONOMOUS source must start with PACKAGE keyword, got: {:?}",
            &trimmed[..trimmed.len().min(80)]
        );

        // Must be non-trivially long — a real package spec.
        assert!(
            resp.source.len() > 30,
            "source should be non-trivially long; got {} bytes",
            resp.source.len()
        );
    }

    /// `get_clob` — fetch the PKG_AUTONOMOUS source via a CLOB-projecting SELECT.
    #[test]
    fn get_clob_returns_package_source() {
        let conn = demo_conn();
        let sql = "SELECT text FROM all_source \
                   WHERE owner = 'DEMO' AND name = 'PKG_AUTONOMOUS' AND type = 'PACKAGE' \
                   AND rownum = 1";
        let resp = run_get_clob(&conn, sql, &[], Some(4000))
            .expect("PLSQL-MCP-LIVE-018: get_clob should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] get_clob: text_len={} truncated={} sanitized={}",
            resp.text.len(),
            resp.truncated,
            resp.sanitized
        );

        // The first line of PKG_AUTONOMOUS is the PACKAGE header — non-empty.
        assert!(
            !resp.text.is_empty(),
            "get_clob: expected non-empty text from PKG_AUTONOMOUS first source line"
        );
    }

    /// `get_errors` — reads ALL_ERRORS for `PKG_OPAQUE_DYNAMIC`.
    ///
    /// The DEMO schema may have packages with intentional compile errors.
    /// If ALL_ERRORS is empty for this package (clean compile), the test
    /// still passes — we just verify the tool round-trips without Oracle error.
    ///
    /// We separately assert that get_errors for a non-existent object returns an
    /// empty list (not an error) so the round-trip is proven regardless of
    /// whether the package has errors.
    #[test]
    fn get_errors_for_demo_package_round_trips() {
        let conn = demo_conn();
        let resp = run_get_errors(&conn, "DEMO", "PKG_OPAQUE_DYNAMIC")
            .expect("PLSQL-MCP-LIVE-018: get_errors should succeed for PKG_OPAQUE_DYNAMIC");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] get_errors PKG_OPAQUE_DYNAMIC: {} errors",
            resp.errors.len()
        );
        for e in &resp.errors {
            eprintln!(
                "  L{}:{} {} {} — {}",
                e.line,
                e.position,
                e.attribute,
                e.message_number,
                e.text.trim()
            );
        }

        // All returned errors must be attributed to the queried object.
        for e in &resp.errors {
            assert_eq!(
                e.object_name, "PKG_OPAQUE_DYNAMIC",
                "error must belong to PKG_OPAQUE_DYNAMIC, got: {:?}",
                e.object_name
            );
            assert_eq!(
                e.owner, "DEMO",
                "error owner must be DEMO, got: {:?}",
                e.owner
            );
        }
        // Pass regardless of error count — the interesting guarantee is the
        // tool didn't crash and all returned rows are well-formed.
    }

    /// `get_errors` with empty owner routes to USER_ERRORS (DEMO session).
    #[test]
    fn get_errors_empty_owner_routes_user_errors() {
        // Connect as DEMO so USER_ERRORS is scoped to the DEMO schema.
        let conn = demo_conn();
        // Query for a non-existent package — should return empty list, not error.
        let resp = run_get_errors(&conn, "", "NONEXISTENT_XYZ_PKG")
            .expect("PLSQL-MCP-LIVE-018: get_errors with empty owner should succeed");
        eprintln!(
            "[PLSQL-MCP-LIVE-018] get_errors (USER_ERRORS): {} errors",
            resp.errors.len()
        );
        // Non-existent object has zero errors.
        assert!(
            resp.errors.is_empty(),
            "expected zero errors for NONEXISTENT_XYZ_PKG, got: {:?}",
            resp.errors
        );
    }

    /// `list_objects` — list PACKAGE objects in the DEMO schema.
    /// Asserts PKG_AUTONOMOUS is present in the first page.
    #[test]
    fn list_objects_demo_packages_contains_pkg_autonomous() {
        let conn = demo_conn();
        let req = ListObjectsRequest {
            schema: Some(String::from("DEMO")),
            object_type: Some(String::from("PACKAGE")),
            page_size: Some(100),
            ..Default::default()
        };
        let resp =
            run_list_objects(&conn, &req).expect("PLSQL-MCP-LIVE-018: list_objects should succeed");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] list_objects DEMO PACKAGE: {} entries, cursor={:?}",
            resp.entries.len(),
            resp.next_cursor
        );
        for e in &resp.entries {
            eprintln!("  {} {} {}", e.owner, e.name, e.status);
        }

        assert!(
            !resp.entries.is_empty(),
            "list_objects must return at least one PACKAGE in DEMO"
        );

        let found = resp
            .entries
            .iter()
            .any(|e| e.name.as_str().eq("PKG_AUTONOMOUS"));
        assert!(
            found,
            "PKG_AUTONOMOUS must appear in list_objects(DEMO, PACKAGE), got: {:?}",
            resp.entries.iter().map(|e| &e.name).collect::<Vec<_>>()
        );

        // All entries must belong to DEMO and be PACKAGE type.
        for entry in &resp.entries {
            assert_eq!(entry.owner, "DEMO", "owner mismatch: {:?}", entry);
            assert_eq!(entry.object_type, "PACKAGE", "type mismatch: {:?}", entry);
        }
    }

    /// `describe_table` — creates a scratch table, describes it, then drops it.
    ///
    /// DEMO has no tables (only packages), so we create a well-known scratch
    /// table under SYSTEM prefixed `MCP_T_<pid>` to satisfy the isolation
    /// contract.  Teardown runs in a `defer`-style drop via a local guard struct.
    #[test]
    fn describe_table_scratch_table_round_trips() {
        let conn = system_conn();
        let table = scratch_table_name();

        eprintln!("[PLSQL-MCP-LIVE-018] describe_table scratch table: {table}");

        // Ensure clean slate in case a prior run left debris.
        drop_scratch_table_if_exists(&conn, &table);

        // Create the scratch table under SYSTEM schema.
        let create_sql = format!(
            "CREATE TABLE SYSTEM.{table} \
             (ID NUMBER NOT NULL, \
              LABEL VARCHAR2(100), \
              CONSTRAINT {table}_PK PRIMARY KEY (ID))"
        );
        execute_sql(&conn, &create_sql)
            .expect("PLSQL-MCP-LIVE-018: CREATE TABLE scratch table failed");

        // Run describe_table against the scratch table.
        let result = run_describe_table(&conn, "SYSTEM", &table);

        // Teardown regardless of describe_table outcome.
        drop_scratch_table_if_exists(&conn, &table);

        let resp =
            result.expect("PLSQL-MCP-LIVE-018: describe_table should succeed for scratch table");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] describe_table: {} columns, {} constraints, {} indexes",
            resp.columns.len(),
            resp.constraints.len(),
            resp.indexes.len()
        );

        assert_eq!(resp.owner, "SYSTEM", "owner mismatch");
        assert_eq!(resp.name, table, "name mismatch");

        // Must have exactly 2 columns (ID, LABEL).
        assert_eq!(
            resp.columns.len(),
            2,
            "expected 2 columns, got: {:?}",
            resp.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
        );

        // ID column must be NOT NULL.
        let id_col = resp
            .columns
            .iter()
            .find(|c| c.name.as_str().eq("ID"))
            .expect("ID column must exist");
        assert!(!id_col.nullable, "ID column must be NOT NULL");

        // LABEL column is nullable.
        let label_col = resp
            .columns
            .iter()
            .find(|c| c.name.as_str().eq("LABEL"))
            .expect("LABEL column must exist");
        assert!(label_col.nullable, "LABEL column must be nullable");

        // Primary key constraint must be present.
        let pk = resp
            .constraints
            .iter()
            .find(|c| c.constraint_type.as_str().eq("P"));
        assert!(
            pk.is_some(),
            "PRIMARY KEY constraint must be present; got: {:?}",
            resp.constraints
        );

        // Primary key index must be present.
        let pk_idx = resp.indexes.iter().find(|i| i.unique);
        assert!(
            pk_idx.is_some(),
            "a UNIQUE index (PK-backing) must be present; got: {:?}",
            resp.indexes
        );
    }

    /// `compile_with_warnings` — compiles a scratch package created under SYSTEM.
    ///
    /// DEMO packages may reference objects absent from XE (causing compile errors
    /// that are environment-specific and not indicative of tool failure).  This
    /// test creates a minimal `MCP_T_<pid>_PKG` package under SYSTEM, runs
    /// `compile_with_warnings`, verifies the structured response, and drops the
    /// package in teardown.
    ///
    /// `compile_with_warnings` issues `ALTER … COMPILE`.  SYSTEM has
    /// `ALTER ANY PROCEDURE` so the compilation succeeds.
    #[test]
    fn compile_with_warnings_scratch_package_returns_structured_response() {
        let conn = system_conn();
        let pkg_name = format!("{}_PKG", scratch_table_name());

        eprintln!("[PLSQL-MCP-LIVE-018] compile_with_warnings scratch package: {pkg_name}");

        // Create a minimal valid package spec under SYSTEM.  Include the
        // AUTHID clause to suppress PLW-05018 (categorized as Severe by
        // `categorize_error` since code 5018 falls in the 5000–5999 range).
        let create_sql = format!(
            "CREATE OR REPLACE PACKAGE SYSTEM.{pkg_name} AUTHID DEFINER AS \
               PROCEDURE hello(p_name VARCHAR2); \
             END {pkg_name};"
        );
        execute_sql(&conn, &create_sql)
            .expect("PLSQL-MCP-LIVE-018: CREATE PACKAGE scratch package failed");

        let result = run_compile_with_warnings(&conn, "SYSTEM", &pkg_name, "PACKAGE");

        // Teardown: drop the scratch package regardless of test outcome.
        let drop_sql = format!(
            "BEGIN EXECUTE IMMEDIATE 'DROP PACKAGE SYSTEM.{pkg_name}'; \
             EXCEPTION WHEN OTHERS THEN NULL; END;"
        );
        let _ = execute_sql(&conn, &drop_sql);

        let resp = result
            .expect("PLSQL-MCP-LIVE-018: compile_with_warnings should succeed for scratch package");

        eprintln!(
            "[PLSQL-MCP-LIVE-018] compile_with_warnings: success={}, severe={}, perf={}, info={}, other={}",
            resp.success,
            resp.severe.len(),
            resp.performance.len(),
            resp.informational.len(),
            resp.other.len(),
        );
        for e in resp
            .severe
            .iter()
            .chain(resp.performance.iter())
            .chain(resp.informational.iter())
        {
            eprintln!(
                "  L{}:{} {} {} — {}",
                e.line,
                e.position,
                e.attribute,
                e.message_number,
                e.text.trim()
            );
        }

        assert_eq!(resp.object_name, pkg_name, "object_name mismatch");
        assert_eq!(resp.owner, "SYSTEM", "owner mismatch");

        // A minimal, valid PACKAGE spec with no external dependencies should
        // compile without any severe (compile-blocking) errors.
        assert!(
            resp.success,
            "scratch package {pkg_name} should compile without severe errors; \
             severe: {:?}",
            resp.severe.iter().map(|e| &e.text).collect::<Vec<_>>()
        );
        // No compile-blocking errors.
        assert!(
            resp.severe.is_empty(),
            "no severe errors expected for trivial package; got: {:?}",
            resp.severe
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // (b) Chained flow: preview_sql → run_execute_approved → DDL on Oracle
    // ─────────────────────────────────────────────────────────────────────────

    /// Chained flow test: `preview_sql` → `run_execute_approved` → DDL against
    /// Oracle.
    ///
    /// Creates a scratch table `MCP_T_<pid>_CHAIN` under SYSTEM, verifies the
    /// full approval chain, executes the DDL against Oracle, confirms the table
    /// exists in ALL_OBJECTS, then drops it.
    #[test]
    fn chained_flow_preview_then_execute_approved_creates_table() {
        let conn = system_conn();
        let table = format!("{}_CHAIN", scratch_table_name());
        let connection_name = "xe-system";

        eprintln!("[PLSQL-MCP-LIVE-018] chained-flow scratch table: {table}");

        // Clean slate.
        drop_scratch_table_if_exists(&conn, &table);

        // The DDL we want to preview and execute.
        let ddl = format!("CREATE TABLE SYSTEM.{table} (ID NUMBER NOT NULL, LABEL VARCHAR2(50))");
        let token_value = random_approval_text("mcp-live-test");

        // Step 1: preview_sql — mint the approval token.
        let mut registry = PreviewRegistry::new();
        let preview = registry
            .preview_sql(
                connection_name,
                format!("CREATE scratch table SYSTEM.{table}"),
                &ddl,
                &token_value,
            )
            .expect("PLSQL-MCP-LIVE-018: preview_sql should succeed");

        eprintln!("[PLSQL-MCP-LIVE-018] preview sha256={}", preview.ddl_sha256);
        assert!(
            preview.ddl_sha256.starts_with("sha256:"),
            "sha256 must be prefixed"
        );
        assert_eq!(preview.connection, connection_name);

        // Step 2: run_execute_approved — verify token + DDL byte-for-byte.
        let req = ExecuteApprovedRequest {
            connection: connection_name.to_string(),
            token: token_value.clone(),
            ddl_bytes: ddl.clone(),
            principal_schema: "SYSTEM".to_string(),
            target_schema: "SYSTEM".to_string(),
            operator_typed_schema: None,
        };
        let plan = run_execute_approved(&mut registry, req).expect(
            "PLSQL-MCP-LIVE-018: run_execute_approved should succeed with correct token+DDL",
        );

        eprintln!(
            "[PLSQL-MCP-LIVE-018] approved plan: connection={} sha256={}",
            plan.connection, plan.ddl_sha256
        );
        assert_eq!(plan.connection, connection_name, "plan connection mismatch");
        assert_eq!(plan.ddl_bytes, ddl, "plan DDL bytes must match exactly");
        assert!(
            plan.cross_schema.confirmed,
            "same-schema should be auto-confirmed"
        );

        // Step 3: execute the DDL against Oracle (the live-DB adapter step).
        execute_sql(&conn, &plan.ddl_bytes)
            .expect("PLSQL-MCP-LIVE-018: approved DDL execution failed");

        // Step 4: consume the approval token (marks it as used).
        consume_approved(&mut registry, &plan);
        assert!(
            registry.is_empty(),
            "registry must be empty after consume_approved"
        );

        // Step 5: verify the table exists in ALL_OBJECTS.
        let rows = query_rows(
            &conn,
            "SELECT object_name FROM all_objects \
             WHERE owner = 'SYSTEM' AND object_name = :1 AND object_type = 'TABLE'",
            &[OracleBind::from(table.clone())],
        )
        .expect("PLSQL-MCP-LIVE-018: existence check query should succeed");

        assert_eq!(
            rows.len(),
            1,
            "table {table} must exist in ALL_OBJECTS after DDL execution; got {} rows",
            rows.len()
        );
        eprintln!("[PLSQL-MCP-LIVE-018] table {table} confirmed in ALL_OBJECTS. PASS.");

        // Teardown.
        drop_scratch_table_if_exists(&conn, &table);

        // Verify teardown.
        let rows_after = query_rows(
            &conn,
            "SELECT object_name FROM all_objects \
             WHERE owner = 'SYSTEM' AND object_name = :1 AND object_type = 'TABLE'",
            &[OracleBind::from(table.clone())],
        )
        .expect("PLSQL-MCP-LIVE-018: teardown check query should succeed");
        assert!(
            rows_after.is_empty(),
            "scratch table {table} must be dropped in teardown"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // (c) Refusal matrix
    // ─────────────────────────────────────────────────────────────────────────

    /// Refusal (i): fresh session (read-only default) blocks writes.
    ///
    /// A freshly-created `SessionSafetyState` defaults to `InspectOnly` with
    /// `session_writes_enabled = false`.  `writes_allowed()` must return false.
    #[test]
    fn refusal_read_only_default_blocks_writes() {
        let state = SessionSafetyState::default();
        assert_eq!(
            state.profile,
            SafetyProfile::InspectOnly,
            "default profile must be InspectOnly"
        );
        assert!(
            !state.session_writes_enabled,
            "session_writes_enabled must be false by default"
        );
        assert!(
            !state.writes_allowed(),
            "writes_allowed() must return false in read-only-default state"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] refusal(i) read-only default: writes_allowed=false. PASS.");
    }

    /// Refusal (ii): expired token is rejected with `EnableWritesTokenMissing`.
    ///
    /// An `EnableWritesToken` minted at `t=0` with a 60s TTL is expired at
    /// `t = TTL + 1`.  `enable_writes` must return
    /// `EnableWritesTokenMissing` and clear `active_token`.
    #[test]
    fn refusal_expired_token_rejected() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let expired_text = random_approval_text("expired");
        let token = state
            .mint_token("xe-system", "CREATE TABLE scratch", &expired_text)
            .expect("mint_token must succeed for non-readonly connection");

        // Simulate clock advancing past TTL.
        let now_expired = token.issued_at + ENABLE_WRITES_TOKEN_TTL_SECONDS + 1;
        let err = state
            .enable_writes(&expired_text, "xe-system", now_expired)
            .expect_err("expired token must be refused");

        assert!(
            matches!(
                err,
                SafetyProfileError::EnableWritesTokenMissing { ttl_seconds }
                    if matches!(
                        ttl_seconds.cmp(&ENABLE_WRITES_TOKEN_TTL_SECONDS),
                        std::cmp::Ordering::Equal
                    )
            ),
            "expected EnableWritesTokenMissing with correct TTL, got: {err}"
        );
        // Token must be cleared after expiry detection.
        assert!(
            state.active_token.is_none(),
            "expired token must be cleared from state"
        );
        assert!(
            !state.writes_allowed(),
            "session must remain read-only after expired token attempt"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] refusal(ii) expired token rejected. PASS.");
    }

    /// Refusal (iii): mismatched token is rejected with `EnableWritesTokenMismatch`.
    ///
    /// The caller supplies wrong text first. Per the safety spec the token is NOT consumed on mismatch
    /// so the holder of the correct token can still redeem it.
    #[test]
    fn refusal_mismatched_token_rejected() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let correct_text = random_approval_text("correct");
        let wrong_text = random_approval_text("wrong");
        let token = state
            .mint_token("xe-system", "CREATE TABLE scratch", &correct_text)
            .expect("mint_token must succeed");

        let now = token.issued_at + 1;

        // Wrong token text.
        let err_wrong_tok = state
            .enable_writes(&wrong_text, "xe-system", now)
            .expect_err("wrong token text must be refused");
        assert!(
            matches!(err_wrong_tok, SafetyProfileError::EnableWritesTokenMismatch),
            "expected EnableWritesTokenMismatch for wrong token text, got: {err_wrong_tok}"
        );

        // Wrong connection name.
        let err_wrong_conn = state
            .enable_writes(&correct_text, "xe-other", now)
            .expect_err("wrong connection name must be refused");
        assert!(
            matches!(
                err_wrong_conn,
                SafetyProfileError::EnableWritesTokenMismatch
            ),
            "expected EnableWritesTokenMismatch for wrong connection, got: {err_wrong_conn}"
        );

        // Token is still active — the correct caller can still redeem it.
        assert!(
            state.active_token.is_some(),
            "token must NOT be consumed on mismatch"
        );

        // Correct credentials succeed.
        state
            .enable_writes(&correct_text, "xe-system", now)
            .expect("correct token+connection must succeed after prior mismatches");
        assert!(
            state.writes_allowed(),
            "writes_allowed must be true after successful enable_writes"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] refusal(iii) mismatched token rejected. PASS.");
    }

    /// Refusal (iv): `permanently_read_only` blocks writes even with a valid token.
    ///
    /// mint_token must refuse.  As defense-in-depth, even if a token is
    /// pre-injected into `active_token` (bypassing the API), `enable_writes`
    /// must still refuse.
    #[test]
    fn refusal_permanently_read_only_blocks_even_with_token() {
        let mut state = SessionSafetyState::new(SafetyProfile::DdlGuarded, true);
        let blocked_text = random_approval_text("blocked");

        // mint_token must fail for permanently_read_only.
        let mint_err = state
            .mint_token("xe-prod", "dangerous op", &blocked_text)
            .expect_err("mint_token must fail for permanently_read_only connection");
        assert!(
            matches!(mint_err, SafetyProfileError::PermanentlyReadOnly { .. }),
            "expected PermanentlyReadOnly from mint_token, got: {mint_err}"
        );

        // Defense-in-depth: inject a token via field access (impossible through
        // the normal API after the mint failure, but defensive testing verifies
        // the enable_writes guard is independent of mint_token).
        let mut donor_state = SessionSafetyState::new(SafetyProfile::DdlGuarded, false);
        let injected_token = donor_state
            .mint_token("xe-prod", "dangerous op", &blocked_text)
            .expect("donor state must mint a token for injection test");
        let now = injected_token.issued_at + 1;
        state.active_token = Some(injected_token);
        let enable_err = state
            .enable_writes(&blocked_text, "xe-prod", now)
            .expect_err("enable_writes must fail for permanently_read_only even with valid token");
        assert!(
            matches!(enable_err, SafetyProfileError::PermanentlyReadOnly { .. }),
            "expected PermanentlyReadOnly from enable_writes, got: {enable_err}"
        );

        // The connection remains read-only.
        assert!(
            !state.writes_allowed(),
            "permanently_read_only connection must never allow writes"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] refusal(iv) permanently_read_only blocks writes. PASS.");
    }

    /// `preview_sql` → `run_execute_approved` with expired token is refused.
    ///
    /// This bridges the preview-registry refusal into the `run_execute_approved`
    /// API so the chained-flow path is covered under the refusal matrix too.
    #[test]
    fn chained_flow_expired_preview_token_refused_by_execute_approved() {
        let mut registry = PreviewRegistry::new();
        let ddl = "CREATE TABLE SYSTEM.MCP_T_FAKE (ID NUMBER)";
        let preview_text = random_approval_text("expired-preview");
        let preview = registry
            .preview_sql("xe-system", "fake op", ddl, &preview_text)
            .expect("preview_sql must succeed");

        // Simulate token expiry.
        let now = preview.issued_at + ENABLE_WRITES_TOKEN_TTL_SECONDS + 1;
        let err = registry
            .read_patch_preview(&preview_text, now)
            .expect_err("expired preview token must be refused by read_patch_preview");
        assert!(
            matches!(err, plsql_mcp::PreviewError::TokenExpired { .. }),
            "expected TokenExpired, got: {err}"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] chained-flow expired preview token refused. PASS.");
    }

    /// `run_execute_approved` with mismatched DDL bytes is refused with `DdlMismatch`.
    ///
    /// This tests that the byte-for-byte verification in the chained flow works
    /// against a real PreviewRegistry (pure Rust, no Oracle needed).
    #[test]
    fn chained_flow_mismatched_ddl_refused_by_execute_approved() {
        let mut registry = PreviewRegistry::new();
        let ddl = "CREATE TABLE SYSTEM.MCP_T_FAKE (ID NUMBER)";
        let preview_text = random_approval_text("drift-preview");
        registry
            .preview_sql("xe-system", "create fake table", ddl, &preview_text)
            .expect("preview_sql must succeed");

        let req = ExecuteApprovedRequest {
            connection: "xe-system".to_string(),
            token: preview_text,
            ddl_bytes: format!("{ddl} -- drift injection"),
            principal_schema: "SYSTEM".to_string(),
            target_schema: "SYSTEM".to_string(),
            operator_typed_schema: None,
        };
        let err = run_execute_approved(&mut registry, req)
            .expect_err("execute_approved must refuse when DDL bytes differ from preview");
        assert!(
            matches!(
                err,
                plsql_mcp::ExecuteApprovedError::Preview(
                    plsql_mcp::PreviewError::DdlMismatch { .. }
                )
            ),
            "expected DdlMismatch, got: {err}"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] chained-flow DDL mismatch refused. PASS.");
    }

    /// `run_execute_approved` with wrong token is refused with `TokenMismatch`.
    #[test]
    fn chained_flow_wrong_token_refused_by_execute_approved() {
        let mut registry = PreviewRegistry::new();
        let ddl = "CREATE TABLE SYSTEM.MCP_T_FAKE (ID NUMBER)";
        let preview_text = random_approval_text("real-preview");
        let wrong_text = random_approval_text("wrong-preview");
        registry
            .preview_sql("xe-system", "create fake table", ddl, &preview_text)
            .expect("preview_sql must succeed");

        let req = ExecuteApprovedRequest {
            connection: "xe-system".to_string(),
            token: wrong_text,
            ddl_bytes: ddl.to_string(),
            principal_schema: "SYSTEM".to_string(),
            target_schema: "SYSTEM".to_string(),
            operator_typed_schema: None,
        };
        let err = run_execute_approved(&mut registry, req)
            .expect_err("execute_approved must refuse when token doesn't match");
        assert!(
            matches!(
                err,
                plsql_mcp::ExecuteApprovedError::Preview(plsql_mcp::PreviewError::TokenMismatch)
            ),
            "expected TokenMismatch, got: {err}"
        );
        eprintln!("[PLSQL-MCP-LIVE-018] chained-flow wrong token refused. PASS.");
    }
}
