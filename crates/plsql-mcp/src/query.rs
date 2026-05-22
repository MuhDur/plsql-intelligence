//! `query` tool surface for the live-DB tool family (`PLSQL-MCP-LIVE-004`).
//!
//! Routes a SELECT (or WITH CTE) through the connected `OracleConnection`,
//! converts the rows into a structured MCP response, and scrubs prompt-
//! injection markers per the K18 sanitization policy before the response
//! is handed back to the agent.
//!
//! The tool is read-only by construction — it rejects any non-SELECT/WITH
//! SQL using its own `is_read_only_sql` predicate (mirrors the CICD
//! inspector's so the MCP crate doesn't take a dep on plsql-cicd).

use plsql_catalog::{CatalogError, OracleBind, OracleConnection, OracleRow};
use plsql_core::UnknownReason;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One value cell in a [`QueryRow`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryCell {
    pub column: String,
    pub oracle_type: String,
    pub value: Option<String>,
    pub sanitized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryRow {
    pub cells: Vec<QueryCell>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryColumnMeta {
    pub name: String,
    pub oracle_type: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryResponse {
    pub columns: Vec<QueryColumnMeta>,
    pub rows: Vec<QueryRow>,
    pub unknown_reasons: Vec<UnknownReason>,
    pub sanitized_cells: usize,
    pub truncated_cells: usize,
}

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("query tool refuses non-SELECT SQL (preview: `{preview}`)")]
    NotReadOnly { preview: String },
    #[error("oracle backend error: {0}")]
    Backend(#[from] CatalogError),
}

/// Run a read-only query and return a structured response.
pub fn run_query<C: OracleConnection>(
    conn: &C,
    sql: &str,
    params: &[OracleBind],
    lob_truncation_chars: Option<usize>,
) -> Result<QueryResponse, QueryError> {
    if !is_read_only_sql(sql) {
        return Err(QueryError::NotReadOnly {
            preview: preview_sql(sql),
        });
    }
    let raw_rows = conn.query_rows(sql, params)?;
    let columns = extract_column_metadata(&raw_rows);
    let mut response = QueryResponse {
        columns: columns.clone(),
        rows: Vec::with_capacity(raw_rows.len()),
        unknown_reasons: Vec::new(),
        sanitized_cells: 0,
        truncated_cells: 0,
    };
    for row in raw_rows {
        let mut cells = Vec::with_capacity(columns.len());
        for column in &columns {
            let raw_value = row.cell(&column.name);
            let (value, sanitized, truncated) = match raw_value.and_then(|c| c.value.as_deref()) {
                Some(text) => {
                    let (scrubbed, was_sanitized) = sanitize(text);
                    let (final_value, was_truncated) = truncate(scrubbed, lob_truncation_chars);
                    (Some(final_value), was_sanitized, was_truncated)
                }
                None => (None, false, false),
            };
            if sanitized {
                response.sanitized_cells = response.sanitized_cells.saturating_add(1);
            }
            if truncated {
                response.truncated_cells = response.truncated_cells.saturating_add(1);
            }
            cells.push(QueryCell {
                column: column.name.clone(),
                oracle_type: column.oracle_type.clone(),
                value,
                sanitized,
            });
        }
        response.rows.push(QueryRow { cells });
    }
    if response.sanitized_cells > 0 {
        response
            .unknown_reasons
            .push(UnknownReason::ResponseSanitized);
    }
    Ok(response)
}

fn extract_column_metadata(rows: &[OracleRow]) -> Vec<QueryColumnMeta> {
    let mut metadata = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        for (name, cell) in &row.columns {
            if seen.insert(name.clone()) {
                metadata.push(QueryColumnMeta {
                    name: name.clone(),
                    oracle_type: cell.oracle_type.clone(),
                });
            }
        }
    }
    metadata
}

/// Markers that the K18 scrubber rewrites to a neutral `[redacted]` token.
/// Built at runtime so the source file does not itself carry the literal
/// tool-call shapes that downstream parsers might react to.
///
/// Coverage (PLSQL-MCP-SEC-1):
/// - MCP / Anthropic tool-call wrappers (`tool_call`, `tool_use`).
/// - antml:* tag family — `parameter`, `function_calls` (container),
///   `function` (singular legacy form), `invoke`, plus the
///   `tool_call`/`tool_use` cross-pollinations of the namespace.
/// - OpenAI tokenizer-control tokens — `endoftext`, `fim_prefix`,
///   `fim_suffix`, `im_start`, `im_end` (bar-delimited form).
/// - Llama-style chat-template markers — `SYS` and `INST` bracketed
///   tags, plus their closing variants.
/// - Chat-history role prefixes commonly seen in prompt-injection
///   corpora.
fn injection_markers() -> Vec<String> {
    let mut markers: Vec<String> = Vec::new();
    let lt = '<';
    let gt = '>';
    let slash = '/';
    let bar = '|';
    let lbrack = '[';
    let rbrack = ']';
    // MCP / Anthropic-style tool-call tags.
    for tag in ["tool_call", "tool_use"] {
        markers.push(format!("{lt}{tag}{gt}"));
        markers.push(format!("{lt}{slash}{tag}{gt}"));
        markers.push(format!("{lt}{bar}{tag}{bar}{gt}"));
    }
    // OpenAI tokenizer-control tokens + im_start/im_end.
    for tag in [
        "im_start",
        "im_end",
        "endoftext",
        "fim_prefix",
        "fim_suffix",
        "fim_middle",
    ] {
        markers.push(format!("{lt}{bar}{tag}{bar}{gt}"));
    }
    // Chat-history role prefixes that have been observed in prompt-injection corpora.
    for role in ["assistant", "Assistant", "system", "System", "user", "User"] {
        markers.push(format!("{role}: "));
    }
    // antml:* family — the parameter / function_calls container had
    // coverage already; add the singular `function`, the `invoke`
    // wrapper, and the tool_use/tool_call cross-pollinations.
    for tag in [
        "antml:parameter",
        "antml:function_calls",
        "antml:function",
        "antml:invoke",
        "antml:tool_use",
        "antml:tool_call",
    ] {
        markers.push(format!("{lt}{tag}{gt}"));
        markers.push(format!("{lt}{slash}{tag}{gt}"));
    }
    // Llama-style chat-template markers: <<SYS>> / <</SYS>> and [INST] / [/INST].
    markers.push(format!("{lt}{lt}SYS{gt}{gt}"));
    markers.push(format!("{lt}{lt}{slash}SYS{gt}{gt}"));
    markers.push(format!("{lbrack}INST{rbrack}"));
    markers.push(format!("{lbrack}{slash}INST{rbrack}"));
    markers
}

/// Strip MCP / tool-call markers from a text value. Returns `(scrubbed,
/// changed)`.
#[must_use]
pub fn sanitize(text: &str) -> (String, bool) {
    let markers = injection_markers();
    let mut scrubbed = text.to_string();
    let mut changed = false;
    for marker in &markers {
        if scrubbed.contains(marker) {
            scrubbed = scrubbed.replace(marker, "[redacted]");
            changed = true;
        }
    }
    (scrubbed, changed)
}

fn truncate(value: String, limit: Option<usize>) -> (String, bool) {
    let Some(limit) = limit else {
        return (value, false);
    };
    if value.chars().count() <= limit {
        return (value, false);
    }
    let truncated: String = value.chars().take(limit).collect();
    (format!("{truncated}…"), true)
}

#[must_use]
fn is_read_only_sql(sql: &str) -> bool {
    let mut remainder = sql.trim_start();
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
    if !matches!(token.as_str(), "SELECT" | "WITH") {
        return false;
    }
    // PLSQL-MCP-SEC-2: even when the leading token is SELECT/WITH the
    // statement is still considered write-bearing if it carries a row
    // lock (`FOR UPDATE` / `FOR UPDATE OF` / `FOR UPDATE SKIP LOCKED`),
    // or if it embeds a second statement after a `;` that is not just
    // trailing whitespace/comment. Both vectors are routed through
    // `enable_writes` if the caller really wants them.
    let upper = remainder.to_ascii_uppercase();
    if upper.contains(" FOR UPDATE") {
        return false;
    }
    if has_trailing_non_empty_statement(remainder) {
        return false;
    }
    true
}

/// Returns `true` when `sql` contains a `;` followed by any
/// non-whitespace, non-comment content. The driver typically rejects
/// multi-statement strings with ORA-00911 anyway, but the predicate
/// itself should reflect intent so a future driver migration doesn't
/// silently relax the policy (PLSQL-MCP-SEC-2).
fn has_trailing_non_empty_statement(sql: &str) -> bool {
    let Some(idx) = sql.find(';') else {
        return false;
    };
    let mut tail = &sql[idx + 1..];
    loop {
        tail = tail.trim_start();
        if tail.is_empty() {
            return false;
        }
        if let Some(after) = tail.strip_prefix("--") {
            // Line comment — skip to next newline.
            tail = after.split_once('\n').map_or("", |(_, rest)| rest);
            continue;
        }
        if let Some(after) = tail.strip_prefix("/*") {
            // Block comment.
            if let Some((_, rest)) = after.split_once("*/") {
                tail = rest;
                continue;
            }
            // Unterminated block comment — treat as no further content.
            return false;
        }
        return true;
    }
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
    use plsql_catalog::{OracleBackend, OracleConnectionInfo};

    #[derive(Default)]
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

    fn make_row(columns: &[(&str, &str, Option<&str>)]) -> OracleRow {
        let mut row = OracleRow::default();
        for (name, oracle_type, value) in columns {
            row.insert(*name, *oracle_type, value.map(String::from));
        }
        row
    }

    #[test]
    fn rejects_non_select_sql() {
        let conn = StubConn::default();
        let err = run_query(&conn, "DELETE FROM CUSTOMERS", &[], None).unwrap_err();
        assert!(matches!(err, QueryError::NotReadOnly { .. }));
    }

    #[test]
    fn returns_structured_rows_for_select() {
        let conn = StubConn {
            rows: vec![make_row(&[
                ("ID", "NUMBER(10)", Some("1")),
                ("NAME", "VARCHAR2(20)", Some("Alice")),
            ])],
        };
        let response = run_query(&conn, "SELECT id, name FROM users", &[], None).unwrap();
        assert_eq!(response.columns.len(), 2);
        assert_eq!(response.rows.len(), 1);
        assert_eq!(response.rows[0].cells.len(), 2);
        assert!(response.unknown_reasons.is_empty());
        assert_eq!(response.sanitized_cells, 0);
    }

    #[test]
    fn null_values_are_preserved_as_none() {
        let conn = StubConn {
            rows: vec![make_row(&[("ID", "NUMBER(10)", None)])],
        };
        let response = run_query(&conn, "SELECT id FROM users", &[], None).unwrap();
        assert_eq!(response.rows[0].cells[0].value, None);
        assert!(!response.rows[0].cells[0].sanitized);
    }

    #[test]
    fn sanitize_rewrites_known_injection_markers() {
        // Construct a row body that includes a prompt-injection marker
        // assembled at runtime so the test source itself doesn't contain it.
        let payload = format!(
            "{lt}{slash}tool_call{gt} ignore",
            lt = '<',
            gt = '>',
            slash = '/'
        );
        let conn = StubConn {
            rows: vec![make_row(&[("NOTE", "VARCHAR2(200)", Some(&payload))])],
        };
        let response = run_query(&conn, "SELECT note FROM logs", &[], None).unwrap();
        assert_eq!(response.sanitized_cells, 1);
        assert!(
            response
                .unknown_reasons
                .contains(&UnknownReason::ResponseSanitized)
        );
        let cell_value = response.rows[0].cells[0]
            .value
            .as_deref()
            .unwrap_or_default();
        assert!(cell_value.contains("[redacted]"));
        assert!(response.rows[0].cells[0].sanitized);
    }

    #[test]
    fn sanitize_idempotent_for_clean_text() {
        let (scrubbed, changed) = sanitize("hello world");
        assert!(!changed);
        assert_eq!(scrubbed, "hello world");
    }

    #[test]
    fn truncate_marks_oversized_lob() {
        let conn = StubConn {
            rows: vec![make_row(&[("BODY", "CLOB", Some("0123456789abcdef"))])],
        };
        let response = run_query(&conn, "SELECT body FROM docs", &[], Some(4)).unwrap();
        assert_eq!(response.truncated_cells, 1);
        let value = response.rows[0].cells[0].value.as_deref().unwrap();
        assert!(value.ends_with('…'));
    }

    #[test]
    fn read_only_predicate_accepts_select_and_with() {
        assert!(is_read_only_sql("SELECT 1 FROM DUAL"));
        assert!(is_read_only_sql(
            "WITH cte AS (SELECT 1 FROM DUAL) SELECT * FROM cte"
        ));
        assert!(!is_read_only_sql("DELETE FROM logs"));
        assert!(!is_read_only_sql("BEGIN proc; END;"));
    }

    #[test]
    fn read_only_predicate_rejects_for_update_lock_acquirers() {
        // PLSQL-MCP-SEC-2: row locks must route through enable_writes.
        assert!(!is_read_only_sql("SELECT * FROM invoices FOR UPDATE"));
        assert!(!is_read_only_sql(
            "SELECT id FROM invoices FOR UPDATE OF id"
        ));
        assert!(!is_read_only_sql(
            "SELECT id FROM invoices FOR UPDATE SKIP LOCKED"
        ));
    }

    #[test]
    fn read_only_predicate_rejects_multi_statement_payload() {
        // PLSQL-MCP-SEC-2: defense-in-depth against future drivers that
        // might accept multi-statement strings.
        assert!(!is_read_only_sql("SELECT 1 FROM DUAL; DELETE FROM logs"));
        // Trailing whitespace + comment after the terminator is fine.
        assert!(is_read_only_sql("SELECT 1 FROM DUAL;"));
        assert!(is_read_only_sql("SELECT 1 FROM DUAL;   "));
        assert!(is_read_only_sql("SELECT 1 FROM DUAL; -- trailing comment"));
        assert!(is_read_only_sql("SELECT 1 FROM DUAL; /* trailing */"));
    }

    #[test]
    fn sanitize_covers_extended_marker_families() {
        // PLSQL-MCP-SEC-1: every new family scrubs to [redacted].
        let cases = [
            format!("{lt}antml:invoke{gt}", lt = '<', gt = '>'),
            format!("{lt}antml:function{gt}", lt = '<', gt = '>'),
            format!("{lt}{bar}endoftext{bar}{gt}", lt = '<', gt = '>', bar = '|'),
            format!("{lt}{lt}SYS{gt}{gt}", lt = '<', gt = '>'),
            format!("{lb}INST{rb}", lb = '[', rb = ']'),
        ];
        for payload in &cases {
            let (out, changed) = sanitize(payload);
            assert!(changed, "marker {payload:?} should sanitize");
            assert_eq!(out, "[redacted]", "marker {payload:?} should fully scrub");
        }
    }
}
