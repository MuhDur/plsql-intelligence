//! `describe_table`, `describe_view`, `describe_trigger`, `describe_index`
//! tools.
//!
//! Each tool returns a structured response — never free-text — so agents can
//! reason about columns, constraints, indexes, comments, and partition state
//! programmatically. All four tools share an `OracleConnection` shim and
//! issue parameterized queries against `ALL_*` dictionary views.

use plsql_catalog::{CatalogError, OracleBind, OracleConnection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One column row of a describe_table / describe_view response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_expression: Option<String>,
    pub position: u32,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeConstraint {
    pub name: String,
    pub constraint_type: String,
    pub columns: Vec<String>,
    pub search_condition: Option<String>,
    pub referenced_owner: Option<String>,
    pub referenced_table: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeIndex {
    pub name: String,
    pub unique: bool,
    pub index_type: String,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeTableResponse {
    pub owner: String,
    pub name: String,
    pub table_comment: Option<String>,
    pub partitioned: bool,
    pub partition_count: u32,
    pub columns: Vec<DescribeColumn>,
    pub constraints: Vec<DescribeConstraint>,
    pub indexes: Vec<DescribeIndex>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeViewResponse {
    pub owner: String,
    pub name: String,
    pub view_comment: Option<String>,
    pub read_only: Option<bool>,
    pub columns: Vec<DescribeColumn>,
    /// Truncated SELECT text (first `text_preview_chars` characters).
    pub query_preview: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeTriggerResponse {
    pub owner: String,
    pub name: String,
    pub trigger_type: String,
    pub triggering_event: String,
    pub base_object_owner: String,
    pub base_object_name: String,
    pub status: String,
    pub when_clause: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeIndexResponse {
    pub owner: String,
    pub name: String,
    pub table_owner: String,
    pub table_name: String,
    pub unique: bool,
    pub index_type: String,
    pub status: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Error)]
pub enum DescribeError {
    #[error("oracle backend error: {0}")]
    Backend(#[from] CatalogError),
    #[error("object `{owner}.{name}` not found in ALL_OBJECTS as `{object_type}`")]
    NotFound {
        owner: String,
        name: String,
        object_type: String,
    },
}

/// `describe_table(owner, name)` — emits column, constraint, index,
/// comment, and partition info for a single TABLE.
pub fn run_describe_table<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<DescribeTableResponse, DescribeError> {
    let columns = load_columns(conn, owner, name)?;
    let comment = load_table_comment(conn, owner, name)?;
    let constraints = load_constraints(conn, owner, name)?;
    let indexes = load_indexes_for_table(conn, owner, name)?;
    let (partitioned, partition_count) = load_partition_info(conn, owner, name)?;
    if columns.is_empty() && comment.is_none() && !partitioned {
        return Err(DescribeError::NotFound {
            owner: owner.to_string(),
            name: name.to_string(),
            object_type: String::from("TABLE"),
        });
    }
    Ok(DescribeTableResponse {
        owner: owner.to_string(),
        name: name.to_string(),
        table_comment: comment,
        partitioned,
        partition_count,
        columns,
        constraints,
        indexes,
    })
}

pub fn run_describe_view<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
    text_preview_chars: Option<usize>,
) -> Result<DescribeViewResponse, DescribeError> {
    let columns = load_columns(conn, owner, name)?;
    let view_row = conn.query_rows(
        "select text_vc, read_only from all_views where owner = :1 and view_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let Some(row) = view_row.into_iter().next() else {
        return Err(DescribeError::NotFound {
            owner: owner.to_string(),
            name: name.to_string(),
            object_type: String::from("VIEW"),
        });
    };
    let text = row.text("TEXT_VC").map(String::from);
    let read_only = row.text("READ_ONLY").map(|v| v.eq_ignore_ascii_case("Y"));
    let comment = load_table_comment(conn, owner, name)?;
    let query_preview = match (text, text_preview_chars) {
        (Some(t), Some(limit)) if t.chars().count() > limit => {
            let mut truncated: String = t.chars().take(limit).collect();
            truncated.push('…');
            Some(truncated)
        }
        (Some(t), _) => Some(t),
        (None, _) => None,
    };
    Ok(DescribeViewResponse {
        owner: owner.to_string(),
        name: name.to_string(),
        view_comment: comment,
        read_only,
        columns,
        query_preview,
    })
}

pub fn run_describe_trigger<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<DescribeTriggerResponse, DescribeError> {
    let rows = conn.query_rows(
        "select trigger_type, triggering_event, table_owner, table_name, status, when_clause \
         from all_triggers where owner = :1 and trigger_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let Some(row) = rows.into_iter().next() else {
        return Err(DescribeError::NotFound {
            owner: owner.to_string(),
            name: name.to_string(),
            object_type: String::from("TRIGGER"),
        });
    };
    Ok(DescribeTriggerResponse {
        owner: owner.to_string(),
        name: name.to_string(),
        trigger_type: row.text("TRIGGER_TYPE").unwrap_or("").to_string(),
        triggering_event: row.text("TRIGGERING_EVENT").unwrap_or("").to_string(),
        base_object_owner: row.text("TABLE_OWNER").unwrap_or("").to_string(),
        base_object_name: row.text("TABLE_NAME").unwrap_or("").to_string(),
        status: row.text("STATUS").unwrap_or("").to_string(),
        when_clause: row.text("WHEN_CLAUSE").map(String::from),
    })
}

pub fn run_describe_index<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<DescribeIndexResponse, DescribeError> {
    let header = conn.query_rows(
        "select table_owner, table_name, uniqueness, index_type, status \
         from all_indexes where owner = :1 and index_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let Some(row) = header.into_iter().next() else {
        return Err(DescribeError::NotFound {
            owner: owner.to_string(),
            name: name.to_string(),
            object_type: String::from("INDEX"),
        });
    };
    let column_rows = conn.query_rows(
        "select column_name from all_ind_columns where index_owner = :1 and index_name = :2 \
         order by column_position",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let columns = column_rows
        .into_iter()
        .filter_map(|r| r.text("COLUMN_NAME").map(String::from))
        .collect();
    Ok(DescribeIndexResponse {
        owner: owner.to_string(),
        name: name.to_string(),
        table_owner: row.text("TABLE_OWNER").unwrap_or("").to_string(),
        table_name: row.text("TABLE_NAME").unwrap_or("").to_string(),
        unique: row
            .text("UNIQUENESS")
            .map(|u| u.eq_ignore_ascii_case("UNIQUE"))
            .unwrap_or(false),
        index_type: row.text("INDEX_TYPE").unwrap_or("").to_string(),
        status: row.text("STATUS").unwrap_or("").to_string(),
        columns,
    })
}

fn load_columns<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<Vec<DescribeColumn>, DescribeError> {
    let rows = conn.query_rows(
        "select c.column_name, c.data_type, c.nullable, c.data_default_vc as default_expression, \
                c.column_id as position, m.comments \
         from all_tab_columns c \
         left join all_col_comments m on m.owner = c.owner and m.table_name = c.table_name \
                                    and m.column_name = c.column_name \
         where c.owner = :1 and c.table_name = :2 order by c.column_id",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    Ok(rows
        .into_iter()
        .map(|row| DescribeColumn {
            name: row.text("COLUMN_NAME").unwrap_or("").to_string(),
            data_type: row.text("DATA_TYPE").unwrap_or("").to_string(),
            nullable: row
                .text("NULLABLE")
                .map(|v| !v.eq_ignore_ascii_case("N"))
                .unwrap_or(true),
            default_expression: row.text("DEFAULT_EXPRESSION").map(String::from),
            position: row
                .text("POSITION")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0),
            comment: row.text("COMMENTS").map(String::from),
        })
        .collect())
}

fn load_table_comment<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<Option<String>, DescribeError> {
    let rows = conn.query_rows(
        "select comments from all_tab_comments where owner = :1 and table_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| r.text("COMMENTS").map(String::from)))
}

fn load_constraints<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<Vec<DescribeConstraint>, DescribeError> {
    let rows = conn.query_rows(
        "select c.constraint_name, c.constraint_type, c.search_condition_vc, \
                c.r_owner, c.r_constraint_name, cc.column_name \
         from all_constraints c \
         left join all_cons_columns cc on cc.owner = c.owner \
                                     and cc.constraint_name = c.constraint_name \
         where c.owner = :1 and c.table_name = :2 \
         order by c.constraint_name, cc.position",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let mut current: Option<DescribeConstraint> = None;
    let mut out: Vec<DescribeConstraint> = Vec::new();
    for row in rows {
        let name = row.text("CONSTRAINT_NAME").unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let cons_type = row.text("CONSTRAINT_TYPE").unwrap_or("").to_string();
        let column = row.text("COLUMN_NAME").map(String::from);
        match current.as_mut() {
            Some(existing) if existing.name == name => {
                if let Some(col) = column {
                    existing.columns.push(col);
                }
            }
            _ => {
                if let Some(existing) = current.take() {
                    out.push(existing);
                }
                current = Some(DescribeConstraint {
                    name,
                    constraint_type: cons_type,
                    columns: column.into_iter().collect(),
                    search_condition: row.text("SEARCH_CONDITION_VC").map(String::from),
                    referenced_owner: row.text("R_OWNER").map(String::from),
                    referenced_table: row.text("R_CONSTRAINT_NAME").map(String::from),
                });
            }
        }
    }
    if let Some(c) = current {
        out.push(c);
    }
    Ok(out)
}

fn load_indexes_for_table<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<Vec<DescribeIndex>, DescribeError> {
    let rows = conn.query_rows(
        "select i.index_name, i.uniqueness, i.index_type, ic.column_name \
         from all_indexes i \
         left join all_ind_columns ic on ic.index_owner = i.owner and ic.index_name = i.index_name \
         where i.table_owner = :1 and i.table_name = :2 \
         order by i.index_name, ic.column_position",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let mut current: Option<DescribeIndex> = None;
    let mut out: Vec<DescribeIndex> = Vec::new();
    for row in rows {
        let index_name = row.text("INDEX_NAME").unwrap_or("").to_string();
        if index_name.is_empty() {
            continue;
        }
        let unique = row
            .text("UNIQUENESS")
            .map(|u| u.eq_ignore_ascii_case("UNIQUE"))
            .unwrap_or(false);
        let index_type = row.text("INDEX_TYPE").unwrap_or("").to_string();
        let column = row.text("COLUMN_NAME").map(String::from);
        match current.as_mut() {
            Some(existing) if existing.name == index_name => {
                if let Some(col) = column {
                    existing.columns.push(col);
                }
            }
            _ => {
                if let Some(existing) = current.take() {
                    out.push(existing);
                }
                current = Some(DescribeIndex {
                    name: index_name,
                    unique,
                    index_type,
                    columns: column.into_iter().collect(),
                });
            }
        }
    }
    if let Some(c) = current {
        out.push(c);
    }
    Ok(out)
}

fn load_partition_info<C: OracleConnection>(
    conn: &C,
    owner: &str,
    name: &str,
) -> Result<(bool, u32), DescribeError> {
    // `PARTITIONED` lives on ALL_TABLES; `PARTITION_COUNT` lives on
    // ALL_PART_TABLES.  Query ALL_TABLES first to discover whether the table
    // is partitioned at all, then fetch the count from ALL_PART_TABLES only
    // when it is — avoiding the ORA-00904 that results from projecting a
    // non-existent `PARTITIONED` column from ALL_PART_TABLES.
    let table_rows = conn.query_rows(
        "select partitioned from all_tables where owner = :1 and table_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let Some(table_row) = table_rows.into_iter().next() else {
        return Ok((false, 0));
    };
    let partitioned = table_row
        .text("PARTITIONED")
        .map(|v| v.eq_ignore_ascii_case("YES"))
        .unwrap_or(false);
    if !partitioned {
        return Ok((false, 0));
    }
    let part_rows = conn.query_rows(
        "select partition_count from all_part_tables where owner = :1 and table_name = :2",
        &[
            OracleBind::from(owner.to_string()),
            OracleBind::from(name.to_string()),
        ],
    )?;
    let count = part_rows
        .into_iter()
        .next()
        .and_then(|r| r.text("PARTITION_COUNT").and_then(|t| t.parse().ok()))
        .unwrap_or(0);
    Ok((partitioned, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_catalog::{OracleBackend, OracleConnectionInfo, OracleRow};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RouterStub {
        // Maps a SQL-fragment substring to canned rows.
        routes: Mutex<HashMap<String, Vec<OracleRow>>>,
    }

    impl RouterStub {
        fn add(&self, fragment: &str, rows: Vec<OracleRow>) {
            self.routes
                .lock()
                .unwrap()
                .insert(fragment.to_string(), rows);
        }
    }

    impl OracleConnection for RouterStub {
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
            sql: &str,
            _params: &[OracleBind],
        ) -> Result<Vec<OracleRow>, CatalogError> {
            let routes = self.routes.lock().unwrap();
            for (fragment, rows) in routes.iter() {
                if sql.contains(fragment.as_str()) {
                    return Ok(rows.clone());
                }
            }
            Ok(Vec::new())
        }
        fn execute(&self, _sql: &str, _params: &[OracleBind]) -> Result<u64, CatalogError> {
            Ok(0)
        }
    }

    fn row(columns: &[(&str, &str)]) -> OracleRow {
        let mut row = OracleRow::default();
        for (name, value) in columns {
            row.insert(*name, "VARCHAR2(4000)", Some(value.to_string()));
        }
        row
    }

    #[test]
    fn describe_table_assembles_columns_constraints_indexes_partition() {
        let conn = RouterStub::default();
        conn.add(
            "from all_tab_columns c",
            vec![
                row(&[
                    ("COLUMN_NAME", "INVOICE_ID"),
                    ("DATA_TYPE", "NUMBER"),
                    ("NULLABLE", "N"),
                    ("DEFAULT_EXPRESSION", ""),
                    ("POSITION", "1"),
                    ("COMMENTS", "primary key"),
                ]),
                row(&[
                    ("COLUMN_NAME", "AMOUNT"),
                    ("DATA_TYPE", "NUMBER"),
                    ("NULLABLE", "Y"),
                    ("DEFAULT_EXPRESSION", "0"),
                    ("POSITION", "2"),
                    ("COMMENTS", ""),
                ]),
            ],
        );
        conn.add(
            "from all_tab_comments",
            vec![row(&[("COMMENTS", "billing invoices")])],
        );
        conn.add(
            "from all_constraints c",
            vec![row(&[
                ("CONSTRAINT_NAME", "INVOICES_PK"),
                ("CONSTRAINT_TYPE", "P"),
                ("SEARCH_CONDITION_VC", ""),
                ("R_OWNER", ""),
                ("R_CONSTRAINT_NAME", ""),
                ("COLUMN_NAME", "INVOICE_ID"),
            ])],
        );
        conn.add(
            "from all_indexes i",
            vec![
                row(&[
                    ("INDEX_NAME", "INVOICES_PK_IDX"),
                    ("UNIQUENESS", "UNIQUE"),
                    ("INDEX_TYPE", "NORMAL"),
                    ("COLUMN_NAME", "INVOICE_ID"),
                ]),
                row(&[
                    ("INDEX_NAME", "INVOICES_AMT_IDX"),
                    ("UNIQUENESS", "NONUNIQUE"),
                    ("INDEX_TYPE", "NORMAL"),
                    ("COLUMN_NAME", "AMOUNT"),
                ]),
            ],
        );
        // `load_partition_info` now issues two queries: first to ALL_TABLES
        // for `PARTITIONED`, then (when partitioned=YES) to ALL_PART_TABLES
        // for `PARTITION_COUNT`.  Use a fragment that is unique to the
        // partition query and does not accidentally match the tab_columns or
        // tab_comments query strings.
        conn.add(
            "partitioned from all_tables",
            vec![row(&[("PARTITIONED", "YES")])],
        );
        conn.add(
            "partition_count from all_part_tables",
            vec![row(&[("PARTITION_COUNT", "12")])],
        );

        let response = run_describe_table(&conn, "BILLING", "INVOICES").unwrap();
        assert_eq!(response.columns.len(), 2);
        assert_eq!(response.columns[0].name, "INVOICE_ID");
        assert!(!response.columns[0].nullable);
        assert_eq!(response.columns[0].comment.as_deref(), Some("primary key"));
        assert_eq!(response.table_comment.as_deref(), Some("billing invoices"));
        assert!(response.partitioned);
        assert_eq!(response.partition_count, 12);
        assert_eq!(response.constraints.len(), 1);
        assert_eq!(response.indexes.len(), 2);
        let pk = response
            .indexes
            .iter()
            .find(|i| i.name.ends_with("PK_IDX"))
            .unwrap();
        assert!(pk.unique);
    }

    #[test]
    fn describe_table_returns_not_found_for_unknown_object() {
        let conn = RouterStub::default();
        let err = run_describe_table(&conn, "BILLING", "MISSING").unwrap_err();
        assert!(matches!(err, DescribeError::NotFound { .. }));
    }

    #[test]
    fn describe_view_returns_columns_read_only_and_truncated_query() {
        let conn = RouterStub::default();
        conn.add(
            "from all_tab_columns c",
            vec![row(&[
                ("COLUMN_NAME", "TOTAL_DUE"),
                ("DATA_TYPE", "NUMBER"),
                ("NULLABLE", "Y"),
                ("DEFAULT_EXPRESSION", ""),
                ("POSITION", "1"),
                ("COMMENTS", ""),
            ])],
        );
        conn.add(
            "from all_views",
            vec![row(&[
                (
                    "TEXT_VC",
                    "select invoice_id, sum(amount) total_due from invoices group by invoice_id",
                ),
                ("READ_ONLY", "Y"),
            ])],
        );
        conn.add(
            "from all_tab_comments",
            vec![row(&[("COMMENTS", "summary view")])],
        );

        let response = run_describe_view(&conn, "BILLING", "INVOICE_SUMMARY", Some(20)).unwrap();
        assert_eq!(response.read_only, Some(true));
        assert_eq!(response.view_comment.as_deref(), Some("summary view"));
        assert!(response.query_preview.as_deref().unwrap().ends_with('…'));
    }

    #[test]
    fn describe_trigger_returns_structured_attributes() {
        let conn = RouterStub::default();
        conn.add(
            "from all_triggers",
            vec![row(&[
                ("TRIGGER_TYPE", "BEFORE EACH ROW"),
                ("TRIGGERING_EVENT", "INSERT OR UPDATE"),
                ("TABLE_OWNER", "BILLING"),
                ("TABLE_NAME", "INVOICES"),
                ("STATUS", "ENABLED"),
                ("WHEN_CLAUSE", ":new.amount >= 0"),
            ])],
        );
        let response = run_describe_trigger(&conn, "BILLING", "INVOICES_BIU").unwrap();
        assert_eq!(response.trigger_type, "BEFORE EACH ROW");
        assert_eq!(response.base_object_name, "INVOICES");
        assert_eq!(response.when_clause.as_deref(), Some(":new.amount >= 0"));
    }

    #[test]
    fn describe_index_returns_columns_and_uniqueness() {
        let conn = RouterStub::default();
        conn.add(
            "from all_indexes",
            vec![row(&[
                ("TABLE_OWNER", "BILLING"),
                ("TABLE_NAME", "INVOICES"),
                ("UNIQUENESS", "UNIQUE"),
                ("INDEX_TYPE", "NORMAL"),
                ("STATUS", "VALID"),
            ])],
        );
        conn.add(
            "from all_ind_columns",
            vec![
                row(&[("COLUMN_NAME", "INVOICE_ID")]),
                row(&[("COLUMN_NAME", "CUSTOMER_ID")]),
            ],
        );
        let response = run_describe_index(&conn, "BILLING", "INVOICES_PK_IDX").unwrap();
        assert!(response.unique);
        assert_eq!(response.columns, vec!["INVOICE_ID", "CUSTOMER_ID"]);
    }
}
