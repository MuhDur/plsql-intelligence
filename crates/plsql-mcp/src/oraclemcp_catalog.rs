//! Adapter seam from the shared `oraclemcp-db` Oracle foundation into
//! `plsql-catalog`'s catalog-shaped rows.
//!
//! This module deliberately lives in `plsql-mcp`: the offline catalog crate
//! must not depend on `oraclemcp-db`, `oraclemcp-guard`, or the MCP runtime.
//! The adapter is async and `Cx`-first like `oraclemcp-db`; the C.2 catalog
//! trait migration will make this implement the catalog trait directly without
//! adding a per-round-trip `block_on` bridge.

use asupersync::Cx;
use oraclemcp_db::SerializeOptions;
use plsql_catalog::{
    CatalogError, OracleBackend as CatalogBackend, OracleBind as CatalogBind,
    OracleCell as CatalogCell, OracleConnectionInfo as CatalogConnectionInfo,
    OracleRow as CatalogRow,
};

/// MCP-side adapter over an `oraclemcp-db` connection.
#[derive(Debug)]
pub struct OraclemcpCatalogConnection<C> {
    inner: C,
    serialize_options: SerializeOptions,
}

impl<C> OraclemcpCatalogConnection<C> {
    /// Wrap an existing upstream connection with default serialization caps.
    #[must_use]
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            serialize_options: SerializeOptions::default(),
        }
    }

    /// Wrap an existing upstream connection with explicit serialization caps.
    #[must_use]
    pub fn with_serialize_options(inner: C, serialize_options: SerializeOptions) -> Self {
        Self {
            inner,
            serialize_options,
        }
    }

    /// Borrow the upstream `oraclemcp-db` connection.
    #[must_use]
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Borrow the serialization caps used for query delegation.
    #[must_use]
    pub fn serialize_options(&self) -> &SerializeOptions {
        &self.serialize_options
    }
}

impl OraclemcpCatalogConnection<oraclemcp_db::RustOracleConnection> {
    /// Open a real pure-Rust thin Oracle connection via `oraclemcp-db`.
    pub async fn connect(
        cx: &Cx,
        options: oraclemcp_db::OracleConnectOptions,
    ) -> Result<Self, CatalogError> {
        let connection = oraclemcp_db::RustOracleConnection::connect(cx, options)
            .await
            .map_err(map_db_error)?;
        Ok(Self::new(connection))
    }
}

impl<C> OraclemcpCatalogConnection<C>
where
    C: oraclemcp_db::OracleConnection,
{
    /// The catalog-facing backend identifier for this adapter.
    #[must_use]
    pub fn backend(&self) -> CatalogBackend {
        map_backend(self.inner.backend())
    }

    /// Round-trip the upstream connection.
    pub async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
        self.inner.ping(cx).await.map_err(map_db_error)
    }

    /// Return catalog-shaped connection metadata.
    pub async fn describe(&self, cx: &Cx) -> Result<CatalogConnectionInfo, CatalogError> {
        self.inner
            .describe(cx)
            .await
            .map(map_connection_info)
            .map_err(map_db_error)
    }

    /// Run a positional-bind query and return catalog-shaped rows.
    pub async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<Vec<CatalogRow>, CatalogError> {
        let upstream_binds = map_binds(params)?;
        self.inner
            .query_rows_with_serialize_options(cx, sql, &upstream_binds, &self.serialize_options)
            .await
            .map(map_rows)
            .map_err(map_db_error)
    }

    /// Run a positional-bind statement through the upstream connection.
    pub async fn execute(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<u64, CatalogError> {
        let upstream_binds = map_binds(params)?;
        self.inner
            .execute(cx, sql, &upstream_binds)
            .await
            .map_err(map_db_error)
    }
}

fn map_backend(backend: oraclemcp_db::OracleBackend) -> CatalogBackend {
    match backend {
        oraclemcp_db::OracleBackend::RustOracle => CatalogBackend::OracleRs,
        _ => CatalogBackend::OracleRs,
    }
}

fn map_connection_info(info: oraclemcp_db::OracleConnectionInfo) -> CatalogConnectionInfo {
    CatalogConnectionInfo {
        backend: info
            .backend
            .map(map_backend)
            .unwrap_or(CatalogBackend::OracleRs),
        // `oraclemcp-db` deliberately keeps connect material out of
        // `OracleConnectionInfo`; Phase D's live runtime owns named profile
        // state and can fill this from the profile registry when needed.
        connect_string: String::new(),
        current_schema: info.current_schema,
        server_version: info.server_version.unwrap_or_default(),
        db_name: String::new(),
        db_domain: String::new(),
        service_name: String::new(),
        instance_name: String::new(),
        server_type: info.database_role.unwrap_or_default(),
        max_identifier_length: 128,
        max_open_cursors: 0,
    }
}

fn map_binds(params: &[CatalogBind]) -> Result<Vec<oraclemcp_db::OracleBind>, CatalogError> {
    params
        .iter()
        .map(|param| match param {
            CatalogBind::String(value) => Ok(oraclemcp_db::OracleBind::String(value.clone())),
            CatalogBind::I64(value) => Ok(oraclemcp_db::OracleBind::I64(*value)),
            CatalogBind::U64(value) => {
                let signed =
                    i64::try_from(*value).map_err(|_| CatalogError::InvalidColumnValue {
                        column: String::from("bind"),
                        expected: "u64 <= i64::MAX for oraclemcp-db positional bind",
                        value: value.to_string(),
                    })?;
                Ok(oraclemcp_db::OracleBind::I64(signed))
            }
            CatalogBind::Bool(value) => Ok(oraclemcp_db::OracleBind::Bool(*value)),
        })
        .collect()
}

fn map_rows(rows: Vec<oraclemcp_db::OracleRow>) -> Vec<CatalogRow> {
    rows.into_iter().map(map_row).collect()
}

fn map_row(row: oraclemcp_db::OracleRow) -> CatalogRow {
    let mut mapped = CatalogRow::default();
    for (name, cell) in row.columns {
        mapped.columns.insert(
            name.to_ascii_uppercase(),
            CatalogCell::new(cell.oracle_type, cell.value),
        );
    }
    mapped
}

fn map_db_error(err: oraclemcp_db::DbError) -> CatalogError {
    CatalogError::OracleBackendError {
        backend: CatalogBackend::OracleRs,
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::RuntimeBuilder;

    #[derive(Debug)]
    struct FakeDbConnection;

    fn run_async<F, T>(f: F) -> T
    where
        F: Future<Output = T>,
    {
        let runtime = RuntimeBuilder::current_thread()
            .build()
            .expect("current-thread runtime");
        runtime.block_on(async {
            let _ = Cx::current().expect("block_on installs a request Cx");
            f.await
        })
    }

    #[async_trait::async_trait(?Send)]
    impl oraclemcp_db::OracleConnection for FakeDbConnection {
        fn backend(&self) -> oraclemcp_db::OracleBackend {
            oraclemcp_db::OracleBackend::RustOracle
        }

        async fn ping(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn describe(
            &self,
            _cx: &Cx,
        ) -> Result<oraclemcp_db::OracleConnectionInfo, oraclemcp_db::DbError> {
            Ok(oraclemcp_db::OracleConnectionInfo {
                backend: Some(oraclemcp_db::OracleBackend::RustOracle),
                connection_strategy: Some(String::from("single_session")),
                server_version: Some(String::from("23ai")),
                current_schema: Some(String::from("BILLING")),
                database_role: Some(String::from("PRIMARY")),
                ..oraclemcp_db::OracleConnectionInfo::default()
            })
        }

        async fn query_rows(
            &self,
            _cx: &Cx,
            sql: &str,
            binds: &[oraclemcp_db::OracleBind],
        ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
            assert_eq!(sql, "select :1, :2, :3 from dual");
            assert_eq!(
                binds,
                &[
                    oraclemcp_db::OracleBind::String(String::from("BILLING")),
                    oraclemcp_db::OracleBind::I64(42),
                    oraclemcp_db::OracleBind::Bool(true),
                ]
            );
            Ok(vec![oraclemcp_db::OracleRow {
                columns: vec![
                    (
                        String::from("owner"),
                        oraclemcp_db::OracleCell::new("VARCHAR2", Some(String::from("BILLING"))),
                    ),
                    (
                        String::from("object_count"),
                        oraclemcp_db::OracleCell::new("NUMBER", Some(String::from("42"))),
                    ),
                    (
                        String::from("source_text"),
                        oraclemcp_db::OracleCell::new("CLOB", Some(String::from("body"))),
                    ),
                ],
            }])
        }

        async fn execute(
            &self,
            _cx: &Cx,
            _sql: &str,
            _binds: &[oraclemcp_db::OracleBind],
        ) -> Result<u64, oraclemcp_db::DbError> {
            Ok(1)
        }

        async fn commit(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn rollback(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }
    }

    #[test]
    fn adapter_maps_oraclemcp_rows_to_catalog_rows() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection);
            let rows = adapter
                .query_rows(
                    &cx,
                    "select :1, :2, :3 from dual",
                    &[
                        CatalogBind::from("BILLING"),
                        CatalogBind::from(42_u64),
                        CatalogBind::from(true),
                    ],
                )
                .await
                .expect("query rows");

            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.text("OWNER"), Some("BILLING"));
            assert_eq!(row.text("owner"), Some("BILLING"));
            assert_eq!(row.parse_u64("object_count").expect("count"), 42);
            assert_eq!(
                row.cell("source_text").expect("source cell").oracle_type,
                "CLOB"
            );
        });
    }

    #[test]
    fn adapter_maps_connection_metadata_to_catalog_shape() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection);
            let info = adapter.describe(&cx).await.expect("describe");

            assert_eq!(info.backend, CatalogBackend::OracleRs);
            assert_eq!(info.current_schema.as_deref(), Some("BILLING"));
            assert_eq!(info.server_version, "23ai");
            assert_eq!(info.server_type, "PRIMARY");
        });
    }

    #[test]
    fn adapter_rejects_u64_binds_that_oraclemcp_db_cannot_represent() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection);
            let err = adapter
                .query_rows(&cx, "select :1 from dual", &[CatalogBind::U64(u64::MAX)])
                .await
                .expect_err("out-of-range u64 bind should be rejected");

            assert!(err.to_string().contains("u64 <= i64::MAX"));
        });
    }
}
