//! Table / alias resolution for embedded SQL.
//!
//! `sql_sem` defines the empty [`SqlStatementModel`] shape.
//! This module is the population pass: given the raw SQL text of
//! a `SELECT` / `INSERT` / `UPDATE` / `DELETE` / `MERGE`, it
//! builds the `AliasScope` (alias → table) and classifies every
//! table reference as read or write so the lineage layer can
//! emit precise column-level edges later.
//!
//! The recogniser is heuristic and line-shaped (no full SQL
//! parser — that is the parser crate's job). It handles the
//! common shapes the lab corpus exercises:
//!
//! * `FROM t [alias]`, `FROM s.t [alias]`, comma-joined lists.
//! * `JOIN t [alias] ON …` (any join keyword).
//! * `INSERT INTO t`, `UPDATE t [alias] SET`, `DELETE FROM t`.
//! * `MERGE INTO t [alias] USING s [alias]`.
//!
//! Bare aliases default to the table name when the FROM clause
//! supplied none. Subquery contents are not descended into here
//! (the lineage layer recurses; this pass models the top level).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — embedded
//!   SQL defers to the SQL Language Reference for the FROM /
//!   JOIN / alias grammar.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_TAB_COLUMNS` is the server-side authority a later
//!   pass cross-checks the resolved `(schema, table)` pairs
//!   against.

use crate::sql_sem::{SqlSemanticVerb, SqlStatementModel, TableUsageKind, TableUse};

/// Resolve table + alias structure from a single embedded SQL
/// statement's raw text. Returns a populated
/// [`SqlStatementModel`] (reads / writes columns are left for
/// the column-resolution pass; this pass fills `tables` +
/// `alias_scope` + `verb`).
#[must_use]
pub fn resolve_sql(raw: &str) -> SqlStatementModel {
    let upper = raw.trim_start().to_ascii_uppercase();
    let verb = classify_verb(&upper);
    let mut model = SqlStatementModel {
        verb,
        ..SqlStatementModel::default()
    };

    match verb {
        SqlSemanticVerb::Select => {
            collect_from_and_joins(&upper, raw, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Insert => {
            for (s, t, a) in tables_after_keyword(&upper, raw, "INTO") {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
            // INSERT … SELECT — the sub-select FROM is a read.
            collect_from_and_joins(&upper, raw, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Update => {
            for (s, t, a) in tables_after_keyword(&upper, raw, "UPDATE") {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
            collect_from_and_joins(&upper, raw, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Delete => {
            for (s, t, a) in tables_after_keyword(&upper, raw, "FROM") {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
        }
        SqlSemanticVerb::MergeUpdate
        | SqlSemanticVerb::MergeInsert
        | SqlSemanticVerb::MergeDelete => {
            for (s, t, a) in tables_after_keyword(&upper, raw, "INTO") {
                add(&mut model, s, t, a, TableUsageKind::ReadWrite);
            }
            for (s, t, a) in tables_after_keyword(&upper, raw, "USING") {
                add(&mut model, s, t, a, TableUsageKind::Read);
            }
        }
    }
    model
}

fn classify_verb(upper: &str) -> SqlSemanticVerb {
    if upper.starts_with("INSERT") {
        SqlSemanticVerb::Insert
    } else if upper.starts_with("UPDATE") {
        SqlSemanticVerb::Update
    } else if upper.starts_with("DELETE") {
        SqlSemanticVerb::Delete
    } else if upper.starts_with("MERGE") {
        SqlSemanticVerb::MergeUpdate
    } else {
        SqlSemanticVerb::Select
    }
}

fn collect_from_and_joins(
    upper: &str,
    raw: &str,
    model: &mut SqlStatementModel,
    usage: TableUsageKind,
) {
    for (s, t, a) in tables_after_keyword(upper, raw, "FROM") {
        add(model, s, t, a, usage);
    }
    for (s, t, a) in tables_after_keyword(upper, raw, "JOIN") {
        add(model, s, t, a, usage);
    }
}

fn add(
    model: &mut SqlStatementModel,
    schema: Option<String>,
    table: String,
    alias: String,
    usage: TableUsageKind,
) {
    if table.is_empty() || table == "DUAL" {
        return;
    }
    let schema_str = schema.clone().unwrap_or_default();
    // Bind the alias (or the table name itself if no alias) into
    // the scope so column resolution can map qualifiers later.
    let alias_key = if alias.is_empty() {
        table.clone()
    } else {
        alias.clone()
    };
    model.alias_scope_bind(&alias_key, &schema_str, &table);
    // Don't double-record the same (schema, table, usage) triple.
    if !model
        .tables
        .iter()
        .any(|tu| tu.schema == schema_str && tu.table == table && tu.usage == usage)
    {
        model.tables.push(TableUse {
            schema: schema_str,
            table,
            alias,
            usage,
        });
    }
}

/// Pull `[schema.]table [alias]` triples following each whole-word
/// occurrence of `keyword`. Comma-separated lists after `FROM`
/// are walked. Stops a table run at a SQL clause keyword.
fn tables_after_keyword(
    upper: &str,
    raw: &str,
    keyword: &str,
) -> Vec<(Option<String>, String, String)> {
    const STOP: &[&str] = &[
        "WHERE",
        "GROUP",
        "ORDER",
        "HAVING",
        "SET",
        "ON",
        "USING",
        "WHEN",
        "VALUES",
        "SELECT",
        "CONNECT",
        "START",
        "UNION",
        "MINUS",
        "INTERSECT",
        "FETCH",
        "OFFSET",
    ];
    let mut out = Vec::new();
    let bytes = upper.as_bytes();
    let kw = keyword.to_ascii_uppercase();
    let mut search = 0;
    while let Some(rel) = upper[search..].find(&kw) {
        let abs = search + rel;
        search = abs + kw.len();
        let prev_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + kw.len();
        let next_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if !(prev_ok && next_ok) {
            continue;
        }
        // Tokenise the run after the keyword until a STOP word.
        let mut i = after;
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            // Read a [schema.]table token.
            let tok_start = i;
            while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
                i += 1;
            }
            if i == tok_start {
                break;
            }
            let token = &raw[tok_start..i];
            let token_upper = token.to_ascii_uppercase();
            if STOP.contains(&token_upper.as_str()) || token.starts_with('(') {
                break;
            }
            let (schema, table) = match token_upper.rsplit_once('.') {
                Some((s, t)) if !t.is_empty() => (Some(s.to_string()), t.to_string()),
                _ => (None, token_upper.clone()),
            };
            // Optional alias: next token if it's not a STOP word
            // or another join/clause keyword.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut alias = String::new();
            if i < bytes.len() && is_ident_byte(bytes[i]) {
                let a_start = i;
                while i < bytes.len() && is_ident_byte(bytes[i]) {
                    i += 1;
                }
                let cand = raw[a_start..i].to_string();
                let cand_upper = cand.to_ascii_uppercase();
                if STOP.contains(&cand_upper.as_str())
                    || cand_upper == "JOIN"
                    || cand_upper == "INNER"
                    || cand_upper == "LEFT"
                    || cand_upper == "RIGHT"
                    || cand_upper == "FULL"
                    || cand_upper == "CROSS"
                {
                    // Not an alias — rewind so the outer loop sees
                    // the clause keyword and stops.
                    i = a_start;
                } else if cand_upper == "AS" {
                    // `t AS alias` — consume AS, take the next token
                    // as the alias.
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let real_start = i;
                    while i < bytes.len() && is_ident_byte(bytes[i]) {
                        i += 1;
                    }
                    alias = raw[real_start..i].to_string();
                } else {
                    alias = cand;
                }
            }
            out.push((schema, table, alias));
            // After the first table for non-FROM keywords, stop.
            if keyword != "FROM" {
                break;
            }
            // For FROM, continue only across commas.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b',' {
                break;
            }
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#'
}

impl SqlStatementModel {
    fn alias_scope_bind(&mut self, alias: &str, schema: &str, table: &str) {
        // Reuse AliasScope's shadow-on-duplicate behaviour.
        let mut scope = std::mem::take(&mut self.alias_scope);
        scope.bind(alias, schema, table);
        self.alias_scope = scope;
    }
}

// Re-export so callers don't have to reach into sql_sem for the
// scope type when consuming a resolved model.
pub use crate::sql_sem::AliasScope as ResolvedAliasScope;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_single_table_with_alias() {
        let m = resolve_sql("SELECT e.id INTO v FROM employees e WHERE e.id = 1");
        assert_eq!(m.verb, SqlSemanticVerb::Select);
        assert_eq!(m.tables.len(), 1);
        assert_eq!(m.tables[0].table, "EMPLOYEES");
        assert_eq!(m.tables[0].alias, "e");
        assert_eq!(m.tables[0].usage, TableUsageKind::Read);
        assert_eq!(m.alias_scope.resolve("e"), Some(("", "EMPLOYEES")));
    }

    #[test]
    fn select_schema_qualified_table() {
        let m = resolve_sql("SELECT 1 INTO v FROM hr.employees");
        assert_eq!(m.tables[0].schema, "HR");
        assert_eq!(m.tables[0].table, "EMPLOYEES");
    }

    #[test]
    fn select_comma_joined_list() {
        let m = resolve_sql("SELECT 1 INTO v FROM a, b, c WHERE a.x = b.x");
        let names: Vec<&str> = m.tables.iter().map(|t| t.table.as_str()).collect();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"B"));
        assert!(names.contains(&"C"));
    }

    #[test]
    fn join_tables_collected() {
        let m = resolve_sql("SELECT 1 INTO v FROM employees e JOIN departments d ON e.dept = d.id");
        let names: Vec<&str> = m.tables.iter().map(|t| t.table.as_str()).collect();
        assert!(names.contains(&"EMPLOYEES"));
        assert!(names.contains(&"DEPARTMENTS"));
        assert!(m.tables.iter().all(|t| t.usage == TableUsageKind::Read));
    }

    #[test]
    fn insert_into_is_write_subselect_is_read() {
        let m = resolve_sql("INSERT INTO summary SELECT id FROM employees");
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "SUMMARY" && t.usage == TableUsageKind::Write)
        );
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "EMPLOYEES" && t.usage == TableUsageKind::Read)
        );
    }

    #[test]
    fn update_with_alias_is_write() {
        let m = resolve_sql("UPDATE employees e SET e.salary = e.salary * 1.1");
        assert_eq!(m.verb, SqlSemanticVerb::Update);
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "EMPLOYEES" && t.usage == TableUsageKind::Write)
        );
    }

    #[test]
    fn delete_from_is_write() {
        let m = resolve_sql("DELETE FROM stale WHERE id < 100");
        assert_eq!(m.verb, SqlSemanticVerb::Delete);
        assert_eq!(m.tables[0].table, "STALE");
        assert_eq!(m.tables[0].usage, TableUsageKind::Write);
    }

    #[test]
    fn merge_into_is_readwrite_using_is_read() {
        let m = resolve_sql(
            "MERGE INTO target t USING source s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET t.v = s.v",
        );
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "TARGET" && t.usage == TableUsageKind::ReadWrite)
        );
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "SOURCE" && t.usage == TableUsageKind::Read)
        );
    }

    #[test]
    fn as_alias_form_parsed() {
        let m = resolve_sql("SELECT 1 INTO v FROM employees AS emp");
        assert_eq!(m.tables[0].alias, "emp");
        assert_eq!(m.alias_scope.resolve("emp"), Some(("", "EMPLOYEES")));
    }

    #[test]
    fn dual_filtered_out() {
        let m = resolve_sql("SELECT SYSDATE INTO v FROM dual");
        assert!(m.tables.is_empty());
    }

    #[test]
    fn alias_scope_resolves_qualifier() {
        let m = resolve_sql("SELECT e.name INTO v FROM hr.employees e");
        assert_eq!(m.alias_scope.resolve("e"), Some(("HR", "EMPLOYEES")));
    }

    #[test]
    fn no_table_keyword_yields_empty_model() {
        let m = resolve_sql("SELECT 1 INTO v FROM dual");
        assert_eq!(m.verb, SqlSemanticVerb::Select);
        assert!(m.tables.is_empty());
    }
}
