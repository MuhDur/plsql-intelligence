//! `get_object_source`, `get_clob`, and `get_errors` tools.
//!
//! All three are read-only ALL_*/USER_* queries. Every one routes its
//! textual output through the same K18 sanitizer used by `query`.
//! `get_object_source` and `get_clob` scrub their primary source/CLOB
//! payload; `get_errors` returns a structured shape from `USER_ERRORS` /
//! `ALL_ERRORS` so agents can reason about line / column / position
//! numerically, but its free-text fields (owner / object_name /
//! object_type / attribute / text) are still attacker-influenceable —
//! Oracle echoes identifier names into `ALL_ERRORS.TEXT` — so they are
//! K18-sanitized exactly like the source/CLOB paths before they can reach
//! an agent.

use asupersync::Cx;
use plsql_catalog::{CatalogError, OracleBind, OracleConnection, OracleRow};
use plsql_core::UnknownReason;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::query::sanitize;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetObjectSourceResponse {
    pub owner: String,
    pub object_name: String,
    pub object_type: String,
    /// Source body assembled from `ALL_SOURCE` (LINE-ordered).
    pub source: String,
    /// Number of cells the K18 scrubber rewrote (line-level granularity).
    pub sanitized_lines: usize,
    /// `UnknownReason::ResponseSanitized` is appended whenever
    /// `sanitized_lines > 0`.
    pub unknown_reasons: Vec<UnknownReason>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetClobResponse {
    pub text: String,
    pub sanitized: bool,
    pub truncated: bool,
    pub unknown_reasons: Vec<UnknownReason>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObjectError {
    pub owner: String,
    pub object_name: String,
    pub object_type: String,
    pub line: u32,
    pub position: u32,
    pub attribute: String,
    pub message_number: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetErrorsResponse {
    pub errors: Vec<ObjectError>,
    /// `UnknownReason::ResponseSanitized` is appended whenever the K18
    /// scrubber rewrote any free-text field on any error row.
    pub unknown_reasons: Vec<UnknownReason>,
}

#[derive(Debug, Error)]
pub enum SourceToolError {
    #[error("oracle backend error: {0}")]
    Backend(#[from] CatalogError),
}

/// `get_object_source(owner, object_name, object_type)` — reads
/// `ALL_SOURCE` ordered by `LINE` and reassembles the body. Runs K18
/// sanitization per line so individual bad lines can be redacted without
/// dropping the surrounding source.
pub async fn run_get_object_source<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    owner: &str,
    object_name: &str,
    object_type: &str,
) -> Result<GetObjectSourceResponse, SourceToolError> {
    let sql = "select line, text from all_source \
               where owner = :1 and name = :2 and type = :3 \
               order by line";
    let params = vec![
        OracleBind::from(owner.to_string()),
        OracleBind::from(object_name.to_string()),
        OracleBind::from(object_type.to_string()),
    ];
    let rows = conn.query_rows(cx, sql, &params).await?;
    let mut sanitized_lines = 0usize;
    let mut buffer = String::new();
    for row in &rows {
        let line = row.text("TEXT").unwrap_or("");
        let (scrubbed, was_sanitized) = sanitize(line);
        if was_sanitized {
            sanitized_lines = sanitized_lines.saturating_add(1);
        }
        buffer.push_str(&scrubbed);
        // `ALL_SOURCE.TEXT` rows usually already end with a newline; preserve
        // shape but ensure round-trip stability.
        if !scrubbed.ends_with('\n') {
            buffer.push('\n');
        }
    }
    let mut unknown_reasons = Vec::new();
    if sanitized_lines > 0 {
        unknown_reasons.push(UnknownReason::ResponseSanitized);
    }
    Ok(GetObjectSourceResponse {
        owner: owner.to_string(),
        object_name: object_name.to_string(),
        object_type: object_type.to_string(),
        source: buffer,
        sanitized_lines,
        unknown_reasons,
    })
}

/// `get_clob(sql, params, max_chars)` — read-only CLOB fetcher. The agent
/// supplies a one-row SELECT that projects a single CLOB column; the tool
/// applies K18 sanitization + optional truncation.
pub async fn run_get_clob<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    sql: &str,
    params: &[OracleBind],
    max_chars: Option<usize>,
) -> Result<GetClobResponse, SourceToolError> {
    let rows = conn.query_rows(cx, sql, params).await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(GetClobResponse::default());
    };
    // Pick the first non-null cell on the row.
    let Some((_, cell)) = row.columns.iter().next() else {
        return Ok(GetClobResponse::default());
    };
    let raw = cell.value.clone().unwrap_or_default();
    let (scrubbed, was_sanitized) = sanitize(&raw);
    let (final_value, was_truncated) = match max_chars {
        Some(limit) if scrubbed.chars().count() > limit => {
            let mut truncated: String = scrubbed.chars().take(limit).collect();
            truncated.push('…');
            (truncated, true)
        }
        _ => (scrubbed, false),
    };
    let mut unknown_reasons = Vec::new();
    if was_sanitized {
        unknown_reasons.push(UnknownReason::ResponseSanitized);
    }
    Ok(GetClobResponse {
        text: final_value,
        sanitized: was_sanitized,
        truncated: was_truncated,
        unknown_reasons,
    })
}

/// `get_errors(owner, object_name)` — read `ALL_ERRORS` for the given
/// object and return structured rows. When `owner` is empty the tool
/// targets the current schema via `USER_ERRORS`.
pub async fn run_get_errors<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    owner: &str,
    object_name: &str,
) -> Result<GetErrorsResponse, SourceToolError> {
    let trimmed_owner = owner.trim();
    let rows = if trimmed_owner.is_empty() {
        conn.query_rows(
            cx,
            "select user as owner, name, type, line, position, attribute, message_number, text \
             from user_errors where name = :1 order by sequence",
            &[OracleBind::from(object_name.to_string())],
        )
        .await?
    } else {
        conn.query_rows(
            cx,
            "select owner, name, type, line, position, attribute, message_number, text \
             from all_errors where owner = :1 and name = :2 order by sequence",
            &[
                OracleBind::from(trimmed_owner.to_string()),
                OracleBind::from(object_name.to_string()),
            ],
        )
        .await?
    };

    // Free-text fields carry attacker-influenceable content (Oracle echoes
    // schema-owner-controlled identifier names into `ALL_ERRORS.TEXT`), so
    // route each one through the K18 sanitizer exactly as the source/CLOB
    // paths do. The numeric line/position/message_number fields are parsed
    // integers and need no scrubbing. `field()` ORs each cell's `changed`
    // flag into a per-response indicator so a single scrubbed cell anywhere
    // surfaces `ResponseSanitized`.
    let mut any_sanitized = false;
    let mut field = |row: &OracleRow, column: &str| -> String {
        let (scrubbed, was_sanitized) = sanitize(row.text(column).unwrap_or(""));
        if was_sanitized {
            any_sanitized = true;
        }
        scrubbed
    };

    let mut errors = Vec::with_capacity(rows.len());
    for row in &rows {
        errors.push(ObjectError {
            owner: field(row, "OWNER"),
            object_name: field(row, "NAME"),
            object_type: field(row, "TYPE"),
            line: row.text("LINE").and_then(|t| t.parse().ok()).unwrap_or(0),
            position: row
                .text("POSITION")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0),
            attribute: field(row, "ATTRIBUTE"),
            message_number: row
                .text("MESSAGE_NUMBER")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0),
            text: field(row, "TEXT"),
        });
    }
    let mut unknown_reasons = Vec::new();
    if any_sanitized {
        unknown_reasons.push(UnknownReason::ResponseSanitized);
    }
    Ok(GetErrorsResponse {
        errors,
        unknown_reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::RuntimeBuilder;
    use plsql_catalog::{OracleBackend, OracleConnectionInfo};
    use std::future::Future;

    #[derive(Default, Clone)]
    struct StubConn {
        rows: Vec<OracleRow>,
    }

    #[async_trait::async_trait(?Send)]
    impl OracleConnection for StubConn {
        fn backend(&self) -> OracleBackend {
            OracleBackend::RustOracle
        }
        async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
            let _ = cx;
            Ok(())
        }
        async fn describe(&self, cx: &Cx) -> Result<OracleConnectionInfo, CatalogError> {
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
        ) -> Result<Vec<OracleRow>, CatalogError> {
            let _ = cx;
            Ok(self.rows.clone())
        }
        async fn execute(
            &self,
            cx: &Cx,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<u64, CatalogError> {
            let _ = cx;
            Ok(0)
        }
    }

    fn run_source_future<F: Future>(future: F) -> F::Output {
        RuntimeBuilder::current_thread()
            .build()
            .expect("test asupersync runtime")
            .block_on(future)
    }

    fn get_object_source_for_test<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
        object_type: &str,
    ) -> Result<GetObjectSourceResponse, SourceToolError> {
        run_source_future(async {
            let cx = Cx::current().expect("test runtime installs a request Cx");
            run_get_object_source(&cx, conn, owner, object_name, object_type).await
        })
    }

    fn get_clob_for_test<C: OracleConnection>(
        conn: &C,
        sql: &str,
        params: &[OracleBind],
        max_chars: Option<usize>,
    ) -> Result<GetClobResponse, SourceToolError> {
        run_source_future(async {
            let cx = Cx::current().expect("test runtime installs a request Cx");
            run_get_clob(&cx, conn, sql, params, max_chars).await
        })
    }

    fn get_errors_for_test<C: OracleConnection>(
        conn: &C,
        owner: &str,
        object_name: &str,
    ) -> Result<GetErrorsResponse, SourceToolError> {
        run_source_future(async {
            let cx = Cx::current().expect("test runtime installs a request Cx");
            run_get_errors(&cx, conn, owner, object_name).await
        })
    }

    fn source_row(line: &str, text: &str) -> OracleRow {
        let mut row = OracleRow::default();
        row.insert("LINE", "NUMBER", Some(line.to_string()));
        row.insert("TEXT", "VARCHAR2(4000)", Some(text.to_string()));
        row
    }

    fn error_row(line: u32, attribute: &str, text: &str) -> OracleRow {
        let mut row = OracleRow::default();
        row.insert("OWNER", "VARCHAR2(128)", Some(String::from("BILLING")));
        row.insert("NAME", "VARCHAR2(128)", Some(String::from("BILLING_PKG")));
        row.insert("TYPE", "VARCHAR2(30)", Some(String::from("PACKAGE")));
        row.insert("LINE", "NUMBER", Some(line.to_string()));
        row.insert("POSITION", "NUMBER", Some(String::from("4")));
        row.insert("ATTRIBUTE", "VARCHAR2(9)", Some(attribute.to_string()));
        row.insert("MESSAGE_NUMBER", "NUMBER", Some(String::from("942")));
        row.insert("TEXT", "VARCHAR2(4000)", Some(text.to_string()));
        row
    }

    #[test]
    fn get_object_source_reassembles_lines_in_order() {
        let conn = StubConn {
            rows: vec![
                source_row("1", "PACKAGE BODY billing_pkg AS\n"),
                source_row("2", "  PROCEDURE step;\n"),
                source_row("3", "END billing_pkg;\n"),
            ],
        };
        let response =
            get_object_source_for_test(&conn, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert!(response.source.starts_with("PACKAGE BODY billing_pkg"));
        assert!(response.source.contains("PROCEDURE step;"));
        assert!(response.source.trim_end().ends_with("END billing_pkg;"));
        assert_eq!(response.sanitized_lines, 0);
        assert!(response.unknown_reasons.is_empty());
    }

    #[test]
    fn get_object_source_marks_sanitized_lines() {
        // Construct an injection line at runtime so the source file does
        // not itself carry the literal pattern.
        let tainted_line = format!(
            "{lt}{slash}tool_call{gt} payload\n",
            lt = '<',
            gt = '>',
            slash = '/'
        );
        let conn = StubConn {
            rows: vec![source_row("1", &tainted_line)],
        };
        let response =
            get_object_source_for_test(&conn, "BILLING", "BILLING_PKG", "PACKAGE BODY").unwrap();
        assert_eq!(response.sanitized_lines, 1);
        assert!(
            response
                .unknown_reasons
                .contains(&UnknownReason::ResponseSanitized)
        );
        assert!(response.source.contains("[redacted]"));
    }

    #[test]
    fn get_clob_truncates_and_marks_truncated() {
        let mut row = OracleRow::default();
        row.insert("CLOB_VALUE", "CLOB", Some(String::from("0123456789abcdef")));
        let conn = StubConn { rows: vec![row] };
        let response = get_clob_for_test(&conn, "select clob_value from x", &[], Some(4)).unwrap();
        assert!(response.truncated);
        assert!(response.text.ends_with('…'));
        assert_eq!(response.text.chars().count(), 5); // 4 visible + ellipsis
    }

    #[test]
    fn get_clob_empty_result_is_safe() {
        let conn = StubConn::default();
        let response = get_clob_for_test(&conn, "select clob_value from x", &[], None).unwrap();
        assert!(response.text.is_empty());
        assert!(!response.sanitized);
        assert!(!response.truncated);
    }

    #[test]
    fn get_errors_returns_structured_rows() {
        let conn = StubConn {
            rows: vec![
                error_row(2, "ERROR", "PLS-00201: identifier 'FOO' must be declared"),
                error_row(5, "WARNING", "PLW-07203: parameter ..."),
            ],
        };
        let response = get_errors_for_test(&conn, "BILLING", "BILLING_PKG").unwrap();
        assert_eq!(response.errors.len(), 2);
        assert_eq!(response.errors[0].line, 2);
        assert_eq!(response.errors[0].attribute, "ERROR");
        assert_eq!(response.errors[1].line, 5);
        assert_eq!(response.errors[1].attribute, "WARNING");
        assert_eq!(response.errors[0].message_number, 942);
    }

    #[test]
    fn get_errors_routes_to_user_errors_when_owner_blank() {
        // Empty owner string → tool issues USER_ERRORS query.
        let conn = StubConn::default();
        let response = get_errors_for_test(&conn, "", "BILLING_PKG").unwrap();
        assert!(response.errors.is_empty());
        assert!(response.unknown_reasons.is_empty());
    }

    #[test]
    fn get_errors_clean_rows_emit_no_sanitized_reason() {
        let conn = StubConn {
            rows: vec![error_row(2, "ERROR", "PLS-00201: identifier 'FOO'")],
        };
        let response = get_errors_for_test(&conn, "BILLING", "BILLING_PKG").unwrap();
        assert_eq!(response.errors.len(), 1);
        assert!(
            response.unknown_reasons.is_empty(),
            "benign rows must not trip the sanitized flag"
        );
    }

    #[test]
    fn get_errors_sanitizes_free_text_fields() {
        // A compromised schema owner can embed tool-call markup in an
        // identifier name that Oracle echoes verbatim into ALL_ERRORS.TEXT
        // (and ATTRIBUTE). Assemble the injection shape at runtime so the
        // source file does not itself carry the literal pattern, then plant
        // it in the TEXT and ATTRIBUTE columns. The K18 sanitizer must
        // neutralize every angle bracket and the response must flag
        // ResponseSanitized.
        let tainted_text = format!(
            "PLS-00201: identifier {lt}{slash}tool_call{gt} must be declared",
            lt = '<',
            gt = '>',
            slash = '/'
        );
        let tainted_attribute = format!("{lt}system{gt}", lt = '<', gt = '>');
        let conn = StubConn {
            rows: vec![error_row(2, &tainted_attribute, &tainted_text)],
        };
        let response = get_errors_for_test(&conn, "BILLING", "BILLING_PKG").unwrap();
        assert_eq!(response.errors.len(), 1);
        let row = &response.errors[0];
        // No surviving ASCII angle brackets anywhere in the free-text fields.
        for field in [
            &row.text,
            &row.attribute,
            &row.owner,
            &row.object_name,
            &row.object_type,
        ] {
            assert!(
                !field.contains('<') && !field.contains('>'),
                "free-text field retained a raw angle bracket: {field:?}"
            );
        }
        // The structural pass leaves the visible fullwidth look-alikes.
        assert!(row.text.contains('\u{FF1C}') || row.text.contains("[redacted]"));
        assert!(
            response
                .unknown_reasons
                .contains(&UnknownReason::ResponseSanitized),
            "scrubbed free-text must surface ResponseSanitized"
        );
    }
}
