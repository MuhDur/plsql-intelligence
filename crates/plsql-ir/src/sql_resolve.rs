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

use crate::is_ident_byte;
use crate::sql_sem::{SqlSemanticVerb, SqlStatementModel, TableUsageKind, TableUse};

/// Resolve table + alias structure from a single embedded SQL
/// statement's raw text. Returns a populated
/// [`SqlStatementModel`] (reads / writes columns are left for
/// the column-resolution pass; this pass fills `tables` +
/// `alias_scope` + `verb`).
#[must_use]
pub fn resolve_sql(raw: &str) -> SqlStatementModel {
    // `upper` is derived from the *trimmed* text, so the byte offsets the
    // tokenizer computes against `upper` only line up with a buffer that is
    // also trimmed. Slicing the untrimmed `raw` with those offsets shifts
    // every table/alias by the leading-whitespace length (e.g.
    // "    SELECT id FROM Employees emp" -> table "ROM EMPLO", alias "ees").
    // Bind the trimmed slice once and thread it everywhere `raw` was used.
    // (ASCII whitespace is single-byte, so `trimmed`/`upper` share offsets.)
    // (oracle-ajm2.18)
    let trimmed = raw.trim_start();
    // Mask single-quoted string-literal CONTENTS in the scan buffer so a clause
    // keyword buried in a literal (`INSERT INTO log VALUES ('read FROM cache')`)
    // cannot mint a phantom table use. Masking preserves byte length, so the
    // `trimmed[..]` slices for the emitted schema/table/alias stay offset-aligned
    // (oracle-qbqf.2). The leading verb is never inside a literal, so
    // classify_verb is unaffected.
    let upper = crate::fact_emit::mask_string_literals(&trimmed.to_ascii_uppercase());
    let verb = classify_verb(&upper);
    let mut model = SqlStatementModel {
        verb,
        ..SqlStatementModel::default()
    };

    match verb {
        SqlSemanticVerb::Select => {
            collect_from_and_joins(&upper, trimmed, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Insert => {
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "INTO") {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
            // INSERT … SELECT — the sub-select FROM is a read.
            collect_from_and_joins(&upper, trimmed, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Update => {
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "UPDATE") {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
            collect_from_and_joins(&upper, trimmed, &mut model, TableUsageKind::Read);
        }
        SqlSemanticVerb::Delete => {
            // The DELETE target is the identifier immediately after the
            // `DELETE` keyword — the `FROM` is optional in Oracle, so
            // `DELETE employees WHERE …` and `DELETE FROM employees WHERE …`
            // write the same table. Deriving the target only from `FROM`
            // silently produced no write model for a FROM-less DELETE
            // (oracle-j1ep.2). Trailing `FROM`/`JOIN` tables that are NOT the
            // target come from a WHERE sub-SELECT and are Reads — tagging them
            // Write reverses the data-flow direction (oracle-rwjl.6).
            let target = delete_target(&upper, trimmed);
            let target_key = target.as_ref().map(|(s, t, _)| (s.clone(), t.clone()));
            if let Some((s, t, a)) = target {
                add(&mut model, s, t, a, TableUsageKind::Write);
            }
            let mut target_consumed = false;
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "FROM") {
                if !target_consumed && target_key.as_ref() == Some(&(s.clone(), t.clone())) {
                    target_consumed = true;
                    continue;
                }
                add(&mut model, s, t, a, TableUsageKind::Read);
            }
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "JOIN") {
                add(&mut model, s, t, a, TableUsageKind::Read);
            }
        }
        SqlSemanticVerb::MergeUpdate
        | SqlSemanticVerb::MergeInsert
        | SqlSemanticVerb::MergeDelete => {
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "INTO") {
                add(&mut model, s, t, a, TableUsageKind::ReadWrite);
            }
            for (s, t, a) in tables_after_keyword(&upper, trimmed, "USING") {
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

/// The `(schema, table, alias)` write target of a `DELETE`: the
/// `[schema.]table [alias]` immediately after the `DELETE` keyword,
/// skipping an optional leading `FROM`. Oracle accepts both
/// `DELETE t …` and `DELETE FROM t …`; the table written is the same.
/// `upper` is the case-folded buffer, `raw` the (trimmed) original they
/// share offsets with — schema/table are returned case-folded to match
/// [`tables_after_keyword`], the alias is preserved verbatim.
fn delete_target(upper: &str, raw: &str) -> Option<(Option<String>, String, String)> {
    let bytes = upper.as_bytes();
    // Skip the leading `DELETE` keyword.
    let mut i = 0;
    while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Skip an optional `FROM` keyword (whole-word). Compare on the raw
    // byte slice (`bytes` == `upper.as_bytes()`) so a multi-byte
    // identifier immediately after `DELETE` (`DELETE é★ …`) can never
    // slice across a UTF-8 char boundary: `i` is anchored to the start
    // of an arbitrary user token here, not to a found ASCII delimiter,
    // so a blind `upper[i..i + 4]` could land inside a codepoint and
    // panic (oracle-y54x.3 char-boundary fix).
    if bytes[i..]
        .get(..4)
        .is_some_and(|w| w.eq_ignore_ascii_case(b"FROM"))
        && (i + 4 >= bytes.len() || !is_ident_byte(bytes[i + 4]))
    {
        i += 4;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    // Read the `[schema.]table` token.
    let start = i;
    while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
        i += 1;
    }
    if i == start {
        return None;
    }
    let token_upper = upper[start..i].to_string();
    // Optional alias (the next identifier token, unless it is `SET`).
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut alias = String::new();
    if i < bytes.len() && is_ident_byte(bytes[i]) {
        let a_start = i;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        let cand_upper = upper[a_start..i].to_string();
        // `WHERE`/`SET`/`RETURNING` start the rest of the statement, not an
        // alias. Anything else after the target table is the alias.
        if cand_upper != "WHERE" && cand_upper != "SET" && cand_upper != "RETURNING" {
            alias = raw[a_start..i].to_string();
        }
    }
    let (schema, table) = match token_upper.rsplit_once('.') {
        Some((s, t)) if !t.is_empty() => (Some(s.to_string()), t.to_string()),
        _ => (None, token_upper),
    };
    Some((schema, table, alias))
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
    fn leading_whitespace_does_not_shift_table_and_alias_offsets() {
        // oracle-ajm2.18: `upper` was derived from the trimmed text but the
        // tokenizer sliced the *untrimmed* `raw`, so leading whitespace shifted
        // every offset (table -> "ROM EMPLO", alias -> "ees"). Threading the
        // trimmed slice keeps offsets aligned with the buffer they index.
        let m = resolve_sql("    SELECT id FROM Employees emp");
        assert_eq!(m.tables.len(), 1, "{:?}", m.tables);
        assert_eq!(m.tables[0].table, "EMPLOYEES");
        assert_eq!(m.tables[0].alias, "emp");
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
    fn clause_keyword_inside_string_literal_is_not_a_phantom_table() {
        // oracle-qbqf.2: a FROM keyword buried in a string literal must not mint
        // a phantom table use (the scan buffer masks string-literal contents).
        let m = resolve_sql("INSERT INTO log VALUES ('read FROM cache')");
        assert!(
            !m.tables.iter().any(|t| t.table == "CACHE"),
            "FROM inside a literal must not mint a phantom CACHE read: {:?}",
            m.tables
        );
        assert!(
            m.tables.iter().any(|t| t.table == "LOG" && t.usage == TableUsageKind::Write),
            "the real INSERT target LOG must still be a Write: {:?}",
            m.tables
        );
    }

    #[test]
    fn delete_with_multibyte_first_token_does_not_panic() {
        // oracle-y54x.3: delete_target()'s optional-FROM probe did a blind
        // `upper[i..i + 4]` slice anchored at the start of the user token after
        // `DELETE`. A multi-byte first token (`DELETE é★ …`) put `i + 4` inside a
        // UTF-8 codepoint and panicked ("not a char boundary"). The byte-level
        // `bytes[i..].get(..4)` check is char-boundary-safe. Resolving must not
        // panic; the exact table extracted is irrelevant here.
        let _ = resolve_sql("DELETE é★ WHERE x = 1");
        let _ = resolve_sql("DELETE é★"); // no trailing tokens after the target
    }

    // oracle-rwjl.6: a DELETE whose WHERE reads a staging table via a
    // subquery must tag the target Write and the subquery table Read — never
    // Write. The old DELETE arm tagged every `FROM` triple Write.
    #[test]
    fn delete_with_where_subquery_target_write_subquery_read() {
        let m = resolve_sql("DELETE FROM t WHERE id IN (SELECT id FROM staging)");
        assert_eq!(m.verb, SqlSemanticVerb::Delete);
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "T" && t.usage == TableUsageKind::Write),
            "DELETE target T must be Write: {:?}",
            m.tables
        );
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "STAGING" && t.usage == TableUsageKind::Read),
            "WHERE sub-SELECT table STAGING must be Read: {:?}",
            m.tables
        );
        assert!(
            !m.tables
                .iter()
                .any(|t| t.table == "STAGING" && t.usage == TableUsageKind::Write),
            "STAGING must NEVER be Write: {:?}",
            m.tables
        );
    }

    // oracle-j1ep.2: Oracle's `FROM` is optional in a DELETE. A FROM-less
    // `DELETE employees WHERE …` resolves the same write model as
    // `DELETE FROM employees`. Deriving the target only from `FROM` produced
    // an empty model for the FROM-less form.
    #[test]
    fn from_less_delete_resolves_write_target() {
        let m = resolve_sql("DELETE employees WHERE id = 5");
        assert_eq!(m.verb, SqlSemanticVerb::Delete);
        assert_eq!(m.tables.len(), 1, "{:?}", m.tables);
        assert_eq!(m.tables[0].table, "EMPLOYEES");
        assert_eq!(m.tables[0].usage, TableUsageKind::Write);
    }

    #[test]
    fn from_less_qualified_delete_resolves_schema() {
        let m = resolve_sql("DELETE hr.audit_log WHERE ts < SYSDATE");
        assert_eq!(m.verb, SqlSemanticVerb::Delete);
        assert_eq!(m.tables[0].schema, "HR");
        assert_eq!(m.tables[0].table, "AUDIT_LOG");
        assert_eq!(m.tables[0].usage, TableUsageKind::Write);
    }

    // oracle-j1ep.2 + oracle-rwjl.6: FROM-less DELETE target is a Write, the
    // WHERE sub-SELECT table is a Read — never Write.
    #[test]
    fn from_less_delete_subquery_target_write_subquery_read() {
        let m = resolve_sql("DELETE t WHERE id IN (SELECT id FROM staging)");
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "T" && t.usage == TableUsageKind::Write),
            "FROM-less DELETE target T must be Write: {:?}",
            m.tables
        );
        assert!(
            m.tables
                .iter()
                .any(|t| t.table == "STAGING" && t.usage == TableUsageKind::Read),
            "WHERE sub-SELECT table STAGING must be Read: {:?}",
            m.tables
        );
        assert!(
            !m.tables
                .iter()
                .any(|t| t.table == "STAGING" && t.usage == TableUsageKind::Write),
            "STAGING must NEVER be Write: {:?}",
            m.tables
        );
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
