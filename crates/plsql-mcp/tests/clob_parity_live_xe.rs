//! Integration test: X.2 large-CLOB differential parity between the temporary
//! thick catalog driver and the `oraclemcp-db` thin catalog adapter.
//!
//! This is the gate that lets Phase C retire the legacy `oracle` / ODPI-C path:
//! a real Oracle XE object whose DBMS_METADATA output exceeds
//! `oraclemcp-db`'s agent-facing default CLOB cap must extract identically
//! through the catalog adapter's uncapped serialization path.
//!
//! ```sh
//! LD_LIBRARY_PATH=/tmp/instantclient_23_7 \
//! PLSQL_XE_SYSTEM_PASSWORD='...' \
//! cargo test -p plsql-mcp --features live-xe \
//!   --test clob_parity_live_xe -- --nocapture
//! ```

#[cfg(not(feature = "live-xe"))]
#[test]
fn clob_parity_live_xe_is_feature_gated() {
    let live_xe = false;
    assert!(!live_xe, "live-xe feature gate is off by default");
}

#[cfg(feature = "live-xe")]
mod live {
    use asupersync::{Cx, runtime::RuntimeBuilder, runtime::reactor};
    use oraclemcp_db::{
        OracleBind as ThinBind, OracleConnection as ThinOracleConnection, OracleSessionIdentity,
        SerializeOptions,
    };
    use plsql_catalog::{
        CatalogError, CatalogLoadRequest, CatalogObject, CatalogSnapshot, DbmsMetadataDdl,
        OracleBind as CatalogBind, OracleConnectOptions as ThickConnectOptions,
        OracleConnection as CatalogOracleConnection, RustOracleConnection,
        load_snapshot_from_connection, populate_dbms_metadata_ddl,
    };
    use plsql_mcp::OraclemcpCatalogConnection;
    use std::future::Future;

    const CONNECT_STRING: &str = "//localhost:1521/FREEPDB1";
    const SYSTEM_USER: &str = "SYSTEM";
    const FIXTURE_OWNER: &str = "DEMO";
    const FIXTURE_PACKAGE: &str = "PLSQL_X2_CLOB_PARITY";
    const START_MARKER: &str = "PLSQL_X2_CLOB_PARITY_START";
    const END_MARKER: &str = "PLSQL_X2_CLOB_PARITY_END";

    fn required_env(name: &str) -> String {
        std::env::var(name).expect("required live-XE credential env var is not set")
    }

    fn run_with_cx<F, Fut, T>(body: F) -> T
    where
        F: FnOnce(Cx) -> Fut,
        Fut: Future<Output = T>,
    {
        let reactor = reactor::create_reactor().expect("native reactor");
        let runtime = RuntimeBuilder::current_thread()
            .with_reactor(reactor)
            .build()
            .expect("live-xe asupersync runtime");
        runtime.block_on(async move {
            let cx = Cx::current().expect("live-xe runtime installs a request Cx");
            body(cx).await
        })
    }

    fn thick_options() -> ThickConnectOptions {
        ThickConnectOptions::new(
            SYSTEM_USER,
            required_env("PLSQL_XE_SYSTEM_PASSWORD"),
            CONNECT_STRING,
        )
        .with_module("plsql-mcp-x2-clob-parity")
        .with_action("oracle-plsql-converge-0lnu.15.3")
        .with_client_identifier("plsql-mcp-x2")
    }

    fn thin_options() -> oraclemcp_db::OracleConnectOptions {
        oraclemcp_db::OracleConnectOptions {
            connect_string: CONNECT_STRING.to_owned(),
            username: Some(SYSTEM_USER.to_owned()),
            password: Some(required_env("PLSQL_XE_SYSTEM_PASSWORD")),
            session_identity: Some(OracleSessionIdentity {
                module: Some("plsql-mcp-x2-clob-parity".to_owned()),
                action: Some("oracle-plsql-converge-0lnu.15.3".to_owned()),
                client_identifier: Some("plsql-mcp-x2".to_owned()),
                ..OracleSessionIdentity::default()
            }),
            ..oraclemcp_db::OracleConnectOptions::default()
        }
    }

    fn build_large_package_ddl() -> String {
        let default_cap = SerializeOptions::default().max_lob_chars;
        let mut ddl = format!(
            "CREATE OR REPLACE PACKAGE {FIXTURE_OWNER}.{FIXTURE_PACKAGE} AUTHID DEFINER AS\n"
        );
        ddl.push_str(&format!(
            "  c_marker_start CONSTANT VARCHAR2(64) := '{START_MARKER}';\n"
        ));
        for index in 0..512 {
            ddl.push_str(&format!(
                "  c_payload_{index:04} CONSTANT VARCHAR2(96) := \
                 'PLSQL_X2_PAYLOAD_{index:04}_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';\n"
            ));
        }
        ddl.push_str(&format!(
            "  c_marker_end CONSTANT VARCHAR2(64) := '{END_MARKER}';\n"
        ));
        ddl.push_str(&format!("END {FIXTURE_PACKAGE};"));

        assert!(
            ddl.chars().count() > default_cap + 8_192,
            "fixture DDL must exceed the oraclemcp-db default CLOB cap"
        );
        ddl
    }

    async fn install_large_package_fixture(
        cx: &Cx,
        conn: &RustOracleConnection,
    ) -> Result<(), CatalogError> {
        conn.execute(cx, &build_large_package_ddl(), &[]).await?;
        let rows = conn
            .query_rows(
                cx,
                "select status \
                 from all_objects \
                 where owner = :1 \
                   and object_name = :2 \
                   and object_type = 'PACKAGE'",
                &[
                    CatalogBind::from(FIXTURE_OWNER.to_owned()),
                    CatalogBind::from(FIXTURE_PACKAGE.to_owned()),
                ],
            )
            .await?;
        let status = rows
            .first()
            .and_then(|row| row.text("STATUS"))
            .unwrap_or("<missing>");
        assert_eq!(status, "VALID", "large package fixture must compile");
        Ok(())
    }

    async fn load_snapshot_with_ddl<C: CatalogOracleConnection>(
        cx: &Cx,
        conn: &C,
    ) -> Result<CatalogSnapshot, CatalogError> {
        let request = CatalogLoadRequest::for_named_schemas([FIXTURE_OWNER]);
        let mut snapshot = load_snapshot_from_connection(cx, conn, &request).await?;
        populate_dbms_metadata_ddl(cx, conn, &mut snapshot).await?;
        Ok(snapshot)
    }

    fn fixture_package_ddl<'a>(
        snapshot: &'a CatalogSnapshot,
        label: &str,
    ) -> Result<&'a DbmsMetadataDdl, String> {
        for (owner_symbol, schema) in &snapshot.schemas {
            let Some(owner) = snapshot.interner.resolve(owner_symbol.symbol()) else {
                continue;
            };
            if !owner.eq_ignore_ascii_case(FIXTURE_OWNER) {
                continue;
            }
            for object in schema.objects.values() {
                let CatalogObject::Package(package) = object else {
                    continue;
                };
                let Some(name) = snapshot.interner.resolve(package.common.name.symbol()) else {
                    continue;
                };
                if name.eq_ignore_ascii_case(FIXTURE_PACKAGE) {
                    return package
                        .common
                        .ddl
                        .as_ref()
                        .ok_or_else(|| format!("{label}: fixture package missing DDL payload"));
                }
            }
        }
        Err(format!(
            "{label}: fixture package was not present in extracted snapshot"
        ))
    }

    async fn assert_default_oraclemcp_lob_path_would_truncate(
        cx: &Cx,
        thin: &OraclemcpCatalogConnection<oraclemcp_db::RustOracleConnection>,
    ) {
        let rows = thin
            .inner()
            .query_rows(
                cx,
                "select dbms_metadata.get_ddl(:1, :2, :3) as ddl_text from dual",
                &[
                    ThinBind::from("PACKAGE"),
                    ThinBind::from(FIXTURE_PACKAGE),
                    ThinBind::from(FIXTURE_OWNER),
                ],
            )
            .await
            .expect("default oraclemcp-db DBMS_METADATA query failed");
        let cell = rows
            .first()
            .and_then(|row| row.cell("DDL_TEXT"))
            .expect("default oraclemcp-db result missing DDL_TEXT");
        let default_cap = SerializeOptions::default().max_lob_chars;
        let fetched_chars = cell
            .text()
            .expect("default oraclemcp-db DDL_TEXT was NULL")
            .chars()
            .count();
        let source_chars = cell
            .source_length
            .expect("default oraclemcp-db LOB cell must carry original length");

        assert_eq!(
            fetched_chars, default_cap,
            "upstream default path should fetch exactly the default CLOB cap"
        );
        assert!(
            source_chars > default_cap,
            "fixture must be larger than the upstream default CLOB cap"
        );
    }

    #[test]
    fn large_clob_catalog_extraction_matches_thick_driver() {
        run_with_cx(|cx| async move {
            let thick = RustOracleConnection::connect(thick_options())
                .expect("legacy thick SYSTEM connection to local XE must succeed");
            install_large_package_fixture(&cx, &thick)
                .await
                .expect("large-CLOB fixture installation failed");

            let thin = OraclemcpCatalogConnection::connect(&cx, thin_options())
                .await
                .expect("oraclemcp-db thin SYSTEM connection to local XE must succeed");

            assert_default_oraclemcp_lob_path_would_truncate(&cx, &thin).await;

            let thick_snapshot = load_snapshot_with_ddl(&cx, &thick)
                .await
                .expect("legacy thick catalog extraction failed");
            let thin_snapshot = load_snapshot_with_ddl(&cx, &thin)
                .await
                .expect("oraclemcp-db catalog extraction failed");

            let thick_ddl =
                fixture_package_ddl(&thick_snapshot, "thick").expect("thick fixture package DDL");
            let thin_ddl =
                fixture_package_ddl(&thin_snapshot, "thin").expect("thin fixture package DDL");
            let default_cap = SerializeOptions::default().max_lob_chars;

            assert!(
                thick_ddl.ddl_text.chars().count() > default_cap,
                "thick DBMS_METADATA DDL must exceed the default CLOB cap"
            );
            assert!(
                thin_ddl.ddl_text.chars().count() > default_cap,
                "thin catalog adapter must return uncapped DBMS_METADATA DDL"
            );
            assert!(thin_ddl.ddl_text.contains(START_MARKER));
            assert!(thin_ddl.ddl_text.contains(END_MARKER));
            assert_eq!(thin_ddl.normalized_ddl, thick_ddl.normalized_ddl);
            assert_eq!(thin_ddl.xml_text, thick_ddl.xml_text);

            eprintln!(
                "[PLSQL-X2] default_cap={} thick_ddl_chars={} thin_ddl_chars={} xml_chars={}",
                default_cap,
                thick_ddl.ddl_text.chars().count(),
                thin_ddl.ddl_text.chars().count(),
                thin_ddl
                    .xml_text
                    .as_ref()
                    .map_or(0, |xml| xml.chars().count())
            );
        });
    }
}
