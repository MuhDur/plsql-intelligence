#![no_main]
//! Coverage-guided fuzz target for the MCP catalog adapter and async loader.
//!
//! Boundary: `plsql_mcp::OraclemcpCatalogConnection` maps the shared
//! `oraclemcp-db` row model into `plsql-catalog`, then
//! `load_snapshot_from_connection` drives every async dictionary loader.
//!
//! Oracle: malformed metadata, permission-like errors, missing columns, and
//! invalid schema filters may produce `Err`, but the adapter + loader chain
//! must never panic.

use asupersync::{runtime::RuntimeBuilder, Cx};
use async_trait::async_trait;
use libfuzzer_sys::fuzz_target;
use oraclemcp_db::{
    DbError, OracleBackend, OracleBind, OracleCell, OracleConnection, OracleConnectionInfo,
    OracleRow, SerializeOptions,
};
use plsql_catalog::{load_snapshot_from_connection, CatalogLoadRequest};
use plsql_mcp::OraclemcpCatalogConnection;

const MAX_LEN: usize = 64 * 1024;
const MAX_TEXT_CHARS: usize = 96;

#[derive(Debug)]
struct FuzzDbConnection {
    mode: u8,
    current_schema: Option<String>,
    server_version: Option<String>,
    column_name: String,
    oracle_type: String,
    cell_value: Option<String>,
}

impl FuzzDbConnection {
    fn from_bytes(data: &[u8]) -> Self {
        let mode = byte_at(data, 0);
        Self {
            mode,
            current_schema: optional_text(data.get(1..33).unwrap_or_default()),
            server_version: optional_text(data.get(33..65).unwrap_or_default()),
            column_name: nonblank_text(data.get(65..97).unwrap_or_default(), "FUZZ_COLUMN"),
            oracle_type: nonblank_text(data.get(97..121).unwrap_or_default(), "VARCHAR2"),
            cell_value: optional_text(data.get(121..).unwrap_or_default()),
        }
    }

    fn fuzz_row(&self) -> OracleRow {
        OracleRow {
            columns: vec![(
                self.column_name.clone(),
                OracleCell::new(self.oracle_type.clone(), self.cell_value.clone()),
            )],
        }
    }

    fn query_result(&self, sql: &str) -> Result<Vec<OracleRow>, DbError> {
        if self.mode & 0b0100_0000 != 0 && sql.len() % 7 == 0 {
            return Err(DbError::Query("fuzzed dictionary query failure".to_owned()));
        }
        if self.mode & 0b0000_0010 == 0 || sql.contains("rownum = 0") {
            return Ok(Vec::new());
        }
        Ok(vec![self.fuzz_row()])
    }
}

#[async_trait(?Send)]
impl OracleConnection for FuzzDbConnection {
    fn backend(&self) -> OracleBackend {
        OracleBackend::RustOracle
    }

    async fn ping(&self, _cx: &Cx) -> Result<(), DbError> {
        if self.mode & 0b1000_0000 != 0 {
            Err(DbError::Query("fuzzed ping failure".to_owned()))
        } else {
            Ok(())
        }
    }

    async fn describe(&self, _cx: &Cx) -> Result<OracleConnectionInfo, DbError> {
        if self.mode & 0b0010_0000 != 0 {
            return Err(DbError::Connect("fuzzed describe failure".to_owned()));
        }
        Ok(OracleConnectionInfo {
            backend: Some(OracleBackend::RustOracle),
            server_version: self.server_version.clone(),
            current_schema: self.current_schema.clone(),
            database_role: Some("PRIMARY".to_owned()),
            open_mode: Some("READ WRITE".to_owned()),
            ..OracleConnectionInfo::default()
        }
        .with_read_only_status())
    }

    async fn query_rows(
        &self,
        _cx: &Cx,
        sql: &str,
        _binds: &[OracleBind],
    ) -> Result<Vec<OracleRow>, DbError> {
        self.query_result(sql)
    }

    async fn query_rows_with_serialize_options(
        &self,
        cx: &Cx,
        sql: &str,
        binds: &[OracleBind],
        serialize_options: &SerializeOptions,
    ) -> Result<Vec<OracleRow>, DbError> {
        let _ = serialize_options;
        self.query_rows(cx, sql, binds).await
    }

    async fn execute(&self, _cx: &Cx, _sql: &str, _binds: &[OracleBind]) -> Result<u64, DbError> {
        if self.mode & 0b0001_0000 != 0 {
            Err(DbError::Execute("fuzzed execute failure".to_owned()))
        } else {
            Ok(0)
        }
    }

    async fn commit(&self, _cx: &Cx) -> Result<(), DbError> {
        Ok(())
    }

    async fn rollback(&self, _cx: &Cx) -> Result<(), DbError> {
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_LEN {
        return;
    }

    let connection = OraclemcpCatalogConnection::new(FuzzDbConnection::from_bytes(data));
    let request = if byte_at(data, 0) & 0b0000_0001 == 0 {
        CatalogLoadRequest::for_current_schema()
    } else {
        CatalogLoadRequest::for_named_schemas([nonblank_text(
            data.get(129..193).unwrap_or_default(),
            "FUZZ_SCHEMA",
        )])
    };

    let Ok(runtime) = RuntimeBuilder::current_thread().build() else {
        return;
    };
    runtime.block_on(async move {
        let Some(cx) = Cx::current() else {
            return;
        };
        let result = load_snapshot_from_connection(&cx, &connection, &request).await;
        drop(result);
    });
});

fn byte_at(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or_default()
}

fn optional_text(data: &[u8]) -> Option<String> {
    let text = bounded_text(data);
    (!text.trim().is_empty()).then_some(text)
}

fn nonblank_text(data: &[u8], fallback: &str) -> String {
    let text = bounded_text(data);
    if text.trim().is_empty() {
        fallback.to_owned()
    } else {
        text
    }
}

fn bounded_text(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect()
}
