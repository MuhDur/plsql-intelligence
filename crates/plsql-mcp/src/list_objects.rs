//! `list_objects` tool.
//!
//! Issues a paged `ALL_OBJECTS` (falling back to `USER_OBJECTS` if the
//! caller is scoped to the connected schema) query with optional
//! type / name-pattern / schema filters. Pagination is cursor-based so
//! large Oracle estates don't blow up the MCP frame: every response
//! carries a `next_cursor: Option<String>` that the agent feeds into the
//! next call.

use plsql_catalog::{CatalogError, OracleBind, OracleConnection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::describe::normalize_identifier;

/// Request shape consumed by the `list_objects` tool.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListObjectsRequest {
    /// Optional Oracle object-type filter (`TABLE` / `VIEW` / `PACKAGE` /
    /// ...). Matched exactly; pass `None` for any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    /// Optional Oracle `LIKE` name pattern (`BILLING_%` etc.). Matched
    /// case-insensitively against `OBJECT_NAME`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
    /// Optional schema filter (`OWNER` column). When `None` the tool
    /// queries `USER_OBJECTS` (current schema only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Maximum rows in this page (capped at `MAX_PAGE_SIZE`). Defaults to
    /// `DEFAULT_PAGE_SIZE`.
    #[serde(default)]
    pub page_size: Option<usize>,
    /// Opaque cursor from a prior response. `None` starts a fresh page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Maximum rows the tool will return in a single page regardless of what
/// the agent requested.
pub const MAX_PAGE_SIZE: usize = 500;

/// Default page size when the caller does not specify one.
pub const DEFAULT_PAGE_SIZE: usize = 100;

/// One row of the `list_objects` response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListObjectsEntry {
    pub owner: String,
    pub name: String,
    pub object_type: String,
    pub status: String,
    pub last_ddl_time: Option<String>,
}

/// Tool response.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListObjectsResponse {
    pub entries: Vec<ListObjectsEntry>,
    /// Cursor for the next page; `None` when no further rows exist.
    pub next_cursor: Option<String>,
    /// SQL the tool issued. Surfaced so the agent / operator can audit the
    /// query without re-deriving it from the tool input.
    pub issued_sql: String,
}

#[derive(Debug, Error)]
pub enum ListObjectsError {
    #[error("oracle backend error: {0}")]
    Backend(#[from] CatalogError),
    #[error("invalid cursor: {message}")]
    InvalidCursor { message: String },
}

/// Run the `list_objects` tool against `conn`.
pub fn run_list_objects<C: OracleConnection>(
    conn: &C,
    request: &ListObjectsRequest,
) -> Result<ListObjectsResponse, ListObjectsError> {
    let cursor = decode_cursor(request.cursor.as_deref())?;
    let page_size = request
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let (sql, params) = build_query(request, &cursor, page_size + 1);
    let rows = conn.query_rows(&sql, &params)?;
    let mut entries = Vec::with_capacity(rows.len().min(page_size));
    let mut last_seen: Option<CursorState> = None;
    for (index, row) in rows.iter().enumerate() {
        if index >= page_size {
            break;
        }
        let owner = row.text("OWNER").unwrap_or("").to_string();
        let name = row.text("OBJECT_NAME").unwrap_or("").to_string();
        let object_type = row.text("OBJECT_TYPE").unwrap_or("").to_string();
        let status = row.text("STATUS").unwrap_or("").to_string();
        let last_ddl_time = row.text("LAST_DDL_TIME_ISO").map(String::from);
        last_seen = Some(CursorState {
            owner: owner.clone(),
            name: name.clone(),
        });
        entries.push(ListObjectsEntry {
            owner,
            name,
            object_type,
            status,
            last_ddl_time,
        });
    }
    let next_cursor = if rows.len() > page_size {
        last_seen.map(encode_cursor)
    } else {
        None
    };
    Ok(ListObjectsResponse {
        entries,
        next_cursor,
        issued_sql: sql,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CursorState {
    owner: String,
    name: String,
}

fn encode_cursor(state: CursorState) -> String {
    // Cursors are positional `OWNER\u{0001}OBJECT_NAME` strings — opaque
    // to the agent but trivially debuggable by an operator.
    format!("{}\u{0001}{}", state.owner, state.name)
}

fn decode_cursor(cursor: Option<&str>) -> Result<Option<CursorState>, ListObjectsError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let mut parts = cursor.splitn(2, '\u{0001}');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if owner.is_empty() || name.is_empty() {
        return Err(ListObjectsError::InvalidCursor {
            message: format!("cursor `{cursor}` is not in `<owner>\\u0001<name>` form"),
        });
    }
    Ok(Some(CursorState {
        owner: owner.to_string(),
        name: name.to_string(),
    }))
}

fn build_query(
    request: &ListObjectsRequest,
    cursor: &Option<CursorState>,
    fetch_count: usize,
) -> (String, Vec<OracleBind>) {
    let mut params: Vec<OracleBind> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();
    let use_dba_dictionary = request.schema.is_some();
    let source_table = if use_dba_dictionary {
        "all_objects"
    } else {
        "user_objects"
    };

    // Oracle stores unquoted identifiers upper-cased; fold the schema/object-type
    // filters the same way so natural lowercase input (`schema:'billing'`,
    // `object_type:'table'`) resolves instead of returning an empty page
    // indistinguishable from an empty schema, and match the name pattern
    // case-insensitively as documented (oracle-da9j.5).
    if let Some(schema) = &request.schema {
        where_clauses.push(format!("owner = :{}", params.len() + 1));
        params.push(OracleBind::from(normalize_identifier(schema)));
    }
    if let Some(object_type) = &request.object_type {
        where_clauses.push(format!("object_type = :{}", params.len() + 1));
        params.push(OracleBind::from(normalize_identifier(object_type)));
    }
    if let Some(pattern) = &request.name_pattern {
        where_clauses.push(format!("upper(object_name) like :{}", params.len() + 1));
        params.push(OracleBind::from(pattern.to_ascii_uppercase()));
    }
    if let Some(state) = cursor {
        // Use the owner+name tuple as a stable seek predicate.
        let owner_index = params.len() + 1;
        let name_index = params.len() + 2;
        if use_dba_dictionary {
            where_clauses.push(format!(
                "(owner > :{owner_index} or (owner = :{owner_index} and object_name > :{name_index}))"
            ));
            params.push(OracleBind::from(state.owner.clone()));
            params.push(OracleBind::from(state.name.clone()));
        } else {
            // user_objects path: only ONE bind (the name) is pushed, and it
            // lands at owner_index (= params.len()+1). Referencing name_index
            // (params.len()+2) here left the pushed bind unreferenced and made
            // the seek placeholder ALIAS the `fetch first :N` bind pushed
            // immediately afterward (same K+2 slot) — so the cursor seek bound
            // the page-size value instead of the last name, breaking
            // user_objects pagination. Use owner_index to match the bind
            // (oracle-b6yl.3).
            where_clauses.push(format!("object_name > :{owner_index}"));
            params.push(OracleBind::from(state.name.clone()));
        }
    }

    let where_block = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("where {}", where_clauses.join(" and "))
    };

    let owner_select = if use_dba_dictionary {
        "owner"
    } else {
        "user as owner"
    };
    let order_block = if use_dba_dictionary {
        "order by owner, object_name"
    } else {
        "order by object_name"
    };

    let sql = format!(
        "select {owner_select}, object_name, object_type, status, \
         to_char(last_ddl_time, 'YYYY-MM-DD\"T\"HH24:MI:SS') as last_ddl_time_iso \
         from {source_table} {where_block} {order_block} fetch first :{fetch_index} rows only",
        fetch_index = params.len() + 1
    );
    params.push(OracleBind::from(
        i64::try_from(fetch_count).unwrap_or(i64::MAX),
    ));
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_catalog::{OracleBackend, OracleConnectionInfo, OracleRow};

    #[derive(Default, Clone)]
    struct StubConn {
        rows: Vec<OracleRow>,
    }

    impl OracleConnection for StubConn {
        fn backend(&self) -> OracleBackend {
            OracleBackend::RustOracle
        }
        fn ping(&self) -> Result<(), CatalogError> {
            Ok(())
        }
        fn describe(&self) -> Result<OracleConnectionInfo, CatalogError> {
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
        fn query_rows(
            &self,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<Vec<OracleRow>, CatalogError> {
            Ok(self.rows.clone())
        }
        fn execute(&self, _sql: &str, _params: &[OracleBind]) -> Result<u64, CatalogError> {
            Ok(0)
        }
    }

    fn row(
        owner: &str,
        name: &str,
        object_type: &str,
        status: &str,
        last_ddl: Option<&str>,
    ) -> OracleRow {
        let mut row = OracleRow::default();
        row.insert("OWNER", "VARCHAR2(128)", Some(owner.to_string()));
        row.insert("OBJECT_NAME", "VARCHAR2(128)", Some(name.to_string()));
        row.insert("OBJECT_TYPE", "VARCHAR2(30)", Some(object_type.to_string()));
        row.insert("STATUS", "VARCHAR2(7)", Some(status.to_string()));
        row.insert(
            "LAST_DDL_TIME_ISO",
            "VARCHAR2(19)",
            last_ddl.map(String::from),
        );
        row
    }

    #[test]
    fn returns_no_cursor_when_page_fits() {
        let conn = StubConn {
            rows: vec![
                row(
                    "BILLING",
                    "INVOICES",
                    "TABLE",
                    "VALID",
                    Some("2026-05-01T13:14:15"),
                ),
                row("BILLING", "CUSTOMERS", "TABLE", "VALID", None),
            ],
        };
        let request = ListObjectsRequest {
            schema: Some(String::from("BILLING")),
            page_size: Some(5),
            ..Default::default()
        };
        let response = run_list_objects(&conn, &request).unwrap();
        assert_eq!(response.entries.len(), 2);
        assert!(response.next_cursor.is_none());
    }

    #[test]
    fn returns_cursor_when_more_rows_exist() {
        // 3 rows for a page size of 2 (the function asks the backend for
        // page_size + 1 to detect overflow).
        let conn = StubConn {
            rows: vec![
                row("BILLING", "INVOICES", "TABLE", "VALID", None),
                row("BILLING", "CUSTOMERS", "TABLE", "VALID", None),
                row("BILLING", "ORDERS", "TABLE", "VALID", None),
            ],
        };
        let request = ListObjectsRequest {
            schema: Some(String::from("BILLING")),
            page_size: Some(2),
            ..Default::default()
        };
        let response = run_list_objects(&conn, &request).unwrap();
        assert_eq!(response.entries.len(), 2);
        assert!(response.next_cursor.is_some());
        // The cursor encodes the second row's (owner, name) tuple.
        let cursor = response.next_cursor.unwrap();
        assert!(cursor.contains("BILLING"));
        assert!(cursor.contains("CUSTOMERS"));
    }

    #[test]
    fn decode_cursor_rejects_malformed_input() {
        let request = ListObjectsRequest {
            schema: Some(String::from("BILLING")),
            cursor: Some(String::from("only-one-half")),
            ..Default::default()
        };
        let err = run_list_objects(&StubConn::default(), &request).unwrap_err();
        assert!(matches!(err, ListObjectsError::InvalidCursor { .. }));
    }

    #[test]
    fn page_size_is_capped_at_max() {
        let conn = StubConn::default();
        let request = ListObjectsRequest {
            schema: Some(String::from("BILLING")),
            page_size: Some(10_000),
            ..Default::default()
        };
        // Even with an oversized request, no rows are pumped through. The
        // function should still run (no panic) and return empty entries.
        let response = run_list_objects(&conn, &request).unwrap();
        assert!(response.entries.is_empty());
        assert!(response.next_cursor.is_none());
    }

    #[test]
    fn schema_filter_uses_all_objects_otherwise_user_objects() {
        // With schema filter set we expect "all_objects" in the issued SQL.
        let conn = StubConn::default();
        let with_schema = run_list_objects(
            &conn,
            &ListObjectsRequest {
                schema: Some(String::from("BILLING")),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(with_schema.issued_sql.contains("from all_objects"));

        let without_schema = run_list_objects(
            &conn,
            &ListObjectsRequest {
                page_size: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(without_schema.issued_sql.contains("from user_objects"));
    }

    #[test]
    fn object_type_and_name_filter_appear_in_issued_sql() {
        let conn = StubConn::default();
        let response = run_list_objects(
            &conn,
            &ListObjectsRequest {
                schema: Some(String::from("BILLING")),
                object_type: Some(String::from("VIEW")),
                name_pattern: Some(String::from("BILLING_%")),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(response.issued_sql.contains("object_type ="));
        assert!(response.issued_sql.contains("upper(object_name) like"));
    }

    #[test]
    fn lowercase_filters_are_folded_to_dictionary_case() {
        // oracle-da9j.5: natural lowercase input must resolve against the
        // upper-cased data-dictionary values (BILLING/TABLE), and the name
        // pattern matches case-insensitively — otherwise an agent copying
        // `billing.invoices` from lowercase source gets an empty page.
        let (sql, params) = build_query(
            &ListObjectsRequest {
                schema: Some(String::from("billing")),
                object_type: Some(String::from("table")),
                name_pattern: Some(String::from("inv_%")),
                ..Default::default()
            },
            &None,
            6,
        );
        assert!(params.contains(&OracleBind::String("BILLING".into())), "{params:?}");
        assert!(params.contains(&OracleBind::String("TABLE".into())), "{params:?}");
        assert!(params.contains(&OracleBind::String("INV_%".into())), "{params:?}");
        assert!(sql.contains("upper(object_name) like"));
        // A double-quoted schema keeps its exact case (Oracle case-sensitive).
        let (_, quoted) = build_query(
            &ListObjectsRequest {
                schema: Some(String::from("\"MixedCase\"")),
                ..Default::default()
            },
            &None,
            6,
        );
        assert!(quoted.contains(&OracleBind::String("MixedCase".into())), "{quoted:?}");
    }

    #[test]
    fn user_objects_cursor_seek_binds_its_own_index_not_the_fetch_index() {
        // oracle-b6yl.3: on the no-schema (user_objects) cursor path exactly ONE
        // seek bind (the last name) is pushed, landing at :1; the `fetch first`
        // bind lands at :2. The seek placeholder must reference :1 (its own
        // bind), NOT :2 — referencing :2 aliased the page-size bind and silently
        // broke user_objects pagination. Every other cursor test sets a schema
        // (all_objects/two-bind path), so this locks the user_objects coverage.
        let request = ListObjectsRequest {
            schema: None,
            page_size: Some(2),
            ..Default::default()
        };
        let cursor = Some(CursorState {
            owner: String::from("BILLING"),
            name: String::from("INVOICES"),
        });
        let (sql, params) = build_query(&request, &cursor, 3);
        assert!(sql.contains("from user_objects"), "{sql}");
        assert!(
            sql.contains("object_name > :1"),
            "seek must bind :1 (its own pushed name): {sql}"
        );
        assert!(
            sql.contains("fetch first :2 rows only"),
            "fetch must be a distinct :2: {sql}"
        );
        assert!(
            !sql.contains("object_name > :2"),
            "seek must NOT alias the fetch bind: {sql}"
        );
        assert_eq!(params.len(), 2, "one seek bind + one fetch bind");
    }
}
