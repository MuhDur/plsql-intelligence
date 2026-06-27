//! Read-only Oracle connection layer for the CI/CD cascade.
//!
//! `predict --mode catalog-aware` / `live-snapshot` and `verify` need to
//! consult the target database for object existence and current DDL.
//! `CicdOracleInspector` wraps the same `plsql_catalog::OracleConnection`
//! trait the catalog snapshot loader uses, but adds a hard guard that
//! rejects any non-`SELECT` / non-`BEGIN ... END` SQL — `predict` /
//! `verify` must never mutate the customer's DB through this layer.
//!
//! For each enabled inspector call we return a small structured result
//! and emit `CicdError::DisallowedWriteSqlInInspector` for any caller that
//! passes a DDL/DML body.

use asupersync::Cx;
use plsql_catalog::{
    DbmsMetadataDdl, ObjectType, OracleBind, OracleConnection,
    fetch_dbms_metadata_ddl as catalog_fetch_dbms_metadata_ddl,
};

use crate::CicdError;

/// A read-only inspector wrapped around an [`OracleConnection`]. All calls
/// must end up running through one of the two safe paths:
///
/// - [`Self::query_rows`] for `SELECT` / `WITH` statements,
/// - [`Self::fetch_dbms_metadata_ddl`] for `DBMS_METADATA.GET_DDL` /
///   `GET_XML` per-object reads.
///
/// Direct DDL/DML through the inspector is rejected with
/// [`CicdError::DisallowedWriteSqlInInspector`] — this is enforced at the
/// helper boundary, not just by convention.
pub struct CicdOracleInspector<'conn, C: OracleConnection> {
    cx: &'conn Cx,
    conn: &'conn C,
}

impl<'conn, C: OracleConnection> CicdOracleInspector<'conn, C> {
    /// Build a new inspector for an already-connected `conn`.
    pub fn new(cx: &'conn Cx, conn: &'conn C) -> Self {
        Self { cx, conn }
    }

    /// Run a read-only query and return its rows. Refuses statements that
    /// are not `SELECT` / `WITH` (DDL, DML, anonymous PL/SQL blocks).
    pub async fn query_rows(
        &self,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<Vec<plsql_catalog::OracleRow>, CicdError> {
        if !is_read_only_sql(sql) {
            return Err(CicdError::DisallowedWriteSqlInInspector {
                preview: preview_sql(sql),
            });
        }
        self.conn
            .query_rows(self.cx, sql, params)
            .await
            .map_err(|err| CicdError::OracleBackendError {
                message: err.to_string(),
            })
    }

    /// Fetch the current `DBMS_METADATA.GET_DDL` / `GET_XML` payload for a
    /// single object. Mirrors `plsql_catalog::fetch_dbms_metadata_ddl` but
    /// stamped with the CICD error surface so callers don't need to import
    /// the catalog error type.
    pub async fn fetch_dbms_metadata_ddl(
        &self,
        object_type: ObjectType,
        name: &str,
        owner: &str,
    ) -> Result<Option<DbmsMetadataDdl>, CicdError> {
        catalog_fetch_dbms_metadata_ddl(self.cx, self.conn, object_type, name, owner)
            .await
            .map_err(|err| CicdError::OracleBackendError {
                message: err.to_string(),
            })
    }

    /// Cheap check that an object exists in the target DB. Returns
    /// `Ok(true)` only when `ALL_OBJECTS` has exactly one matching row.
    pub async fn object_exists(
        &self,
        owner: &str,
        name: &str,
        object_type: ObjectType,
    ) -> Result<bool, CicdError> {
        let Some(dbms_type) = plsql_catalog::object_type_to_dbms_metadata_value(object_type) else {
            return Ok(false);
        };
        // DBMS_METADATA type strings line up 1:1 with ALL_OBJECTS.OBJECT_TYPE
        // strings except that we use a space-separated form upstream.
        let object_type_text = dbms_type.replace('_', " ");
        let rows = self
            .query_rows(
                "select 1 from all_objects \
                 where owner = :1 and object_name = :2 and object_type = :3 and rownum = 1",
                &[
                    OracleBind::from(owner.to_string()),
                    OracleBind::from(name.to_string()),
                    OracleBind::from(object_type_text),
                ],
            )
            .await?;
        Ok(!rows.is_empty())
    }
}

/// Statically classify `sql` as a read-only statement (`SELECT` or `WITH`
/// CTE). Strips leading whitespace + block comments before classifying.
#[must_use]
pub fn is_read_only_sql(sql: &str) -> bool {
    let mut remainder = sql.trim_start();
    // Strip leading SQL block comments to reach the verb token.
    while remainder.starts_with("/*") {
        if let Some(end) = remainder.find("*/") {
            remainder = remainder[end + 2..].trim_start();
        } else {
            return false;
        }
    }
    let token = remainder
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(token.as_str(), "SELECT" | "WITH")
}

fn preview_sql(sql: &str) -> String {
    let trimmed = sql.trim();
    let mut preview: String = trimmed.chars().take(72).collect();
    if trimmed.len() > 72 {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::{Cx, runtime::RuntimeBuilder};
    use plsql_catalog::{
        OracleBackend, OracleBind, OracleConnection, OracleConnectionInfo, OracleRow,
    };
    use std::future::Future;

    #[derive(Default)]
    struct StubConn {
        rows: Vec<OracleRow>,
    }

    #[async_trait::async_trait(?Send)]
    impl OracleConnection for StubConn {
        fn backend(&self) -> OracleBackend {
            OracleBackend::RustOracle
        }
        async fn ping(&self, cx: &Cx) -> Result<(), plsql_catalog::CatalogError> {
            let _ = cx;
            Ok(())
        }
        async fn describe(
            &self,
            cx: &Cx,
        ) -> Result<OracleConnectionInfo, plsql_catalog::CatalogError> {
            let _ = cx;
            Ok(OracleConnectionInfo {
                backend: OracleBackend::RustOracle,
                connect_string: String::from("//localhost/XE"),
                current_schema: Some(String::from("BILLING")),
                server_version: String::from("23.0.0.0.0"),
                db_name: String::from("XE"),
                db_domain: String::new(),
                service_name: String::from("XE"),
                instance_name: String::from("xe"),
                server_type: String::from("Dedicated"),
                max_identifier_length: 128,
                max_open_cursors: 500,
            })
        }
        async fn query_rows(
            &self,
            cx: &Cx,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<Vec<OracleRow>, plsql_catalog::CatalogError> {
            let _ = cx;
            Ok(self.rows.clone())
        }
        async fn execute(
            &self,
            cx: &Cx,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<u64, plsql_catalog::CatalogError> {
            let _ = cx;
            Ok(0)
        }
    }

    fn run_inspector_future<F: Future>(future: F) -> F::Output {
        RuntimeBuilder::current_thread()
            .build()
            .expect("test asupersync runtime")
            .block_on(future)
    }

    #[test]
    fn is_read_only_sql_accepts_select_and_with() {
        assert!(is_read_only_sql("SELECT 1 FROM DUAL"));
        assert!(is_read_only_sql("  select 1 from dual"));
        assert!(is_read_only_sql(
            "WITH cte AS (SELECT 1 FROM DUAL) SELECT * FROM cte"
        ));
        assert!(is_read_only_sql("/* hint */ select 1 from dual"));
    }

    #[test]
    fn is_read_only_sql_rejects_ddl_dml_and_anonymous_blocks() {
        assert!(!is_read_only_sql("INSERT INTO FOO VALUES (1)"));
        assert!(!is_read_only_sql("UPDATE FOO SET A = 1"));
        assert!(!is_read_only_sql("DELETE FROM FOO"));
        assert!(!is_read_only_sql("CREATE TABLE FOO (A NUMBER)"));
        assert!(!is_read_only_sql("ALTER TABLE FOO ADD B NUMBER"));
        assert!(!is_read_only_sql("DROP TABLE FOO"));
        assert!(!is_read_only_sql("GRANT SELECT ON FOO TO PUBLIC"));
        assert!(!is_read_only_sql("BEGIN proc; END;"));
        assert!(!is_read_only_sql("/* unterminated comment"));
    }

    #[test]
    fn inspector_query_rows_rejects_writes() {
        let stub = StubConn::default();
        let err = run_inspector_future(async {
            let cx = Cx::current().expect("test runtime installs a request Cx");
            let inspector = CicdOracleInspector::new(&cx, &stub);
            inspector.query_rows("DELETE FROM CUSTOMERS", &[]).await
        })
        .unwrap_err();
        assert!(matches!(
            err,
            CicdError::DisallowedWriteSqlInInspector { .. }
        ));
    }

    #[test]
    fn inspector_query_rows_accepts_selects() {
        let stub = StubConn::default();
        let rows = run_inspector_future(async {
            let cx = Cx::current().expect("test runtime installs a request Cx");
            let inspector = CicdOracleInspector::new(&cx, &stub);
            inspector.query_rows("SELECT 1 FROM DUAL", &[]).await
        })
        .expect("select ok");
        assert!(rows.is_empty());
    }
}
