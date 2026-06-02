//! Reads / Writes edge extraction at the table level.
//!
//! Walks the embedded SQL statements in a lowered body and pulls
//! out the table-level read / write dependencies. The dependency-
//! graph layer turns each [`TableAccess`] into a `Reads` or
//! `Writes` edge; this module does the extraction from the
//! `Statement::Sql` raw text + the `SqlStatementModel` shape.
//!
//! Read / write classification follows the SQL verb:
//!
//! * `SELECT … FROM t` → Read of `t`.
//! * `INSERT INTO t` → Write of `t`; any sub-SELECT is a Read.
//! * `UPDATE t SET …` → Write of `t`; the WHERE/SET sub-selects
//!   are Reads.
//! * `DELETE FROM t` → Write of `t`.
//! * `MERGE INTO t USING s` → Write of `t`, Read of `s`.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   embedded-SQL DML grammar defers to the SQL Language
//!   Reference; the verb→access mapping above is the standard
//!   read/write classification.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_DEPENDENCIES` (`DEPENDENCY_TYPE = HARD`) is the
//!   server-side mirror; the depgraph cross-checks these edges
//!   against it.

use serde::{Deserialize, Serialize};

use crate::stmt::{SqlVerb, Statement};

/// One table-level access pulled from an embedded SQL statement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableAccess {
    /// Optional schema prefix (`HR.EMPLOYEES`).
    pub schema: Option<String>,
    /// Table / view / synonym name, case-folded.
    pub table: String,
    pub access: AccessKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessKind {
    Read,
    Write,
}

/// Extract table-level Read/Write accesses from every embedded
/// SQL statement in `stmts`.
///
/// Backwards-compatible wrapper around
/// [`extract_table_accesses_bounded`]: the recursion is
/// depth-guarded so a malformed unit whose
/// re-lowered `IF`/`LOOP` body fails to shrink can never
/// stack-overflow. Callers that need to surface the typed
/// [`plsql_core::UnknownReason::AnalysisRecursionLimit`]
/// degradation should call [`extract_table_accesses_bounded`].
#[must_use]
pub fn extract_table_accesses(stmts: &[Statement]) -> Vec<TableAccess> {
    extract_table_accesses_bounded(stmts).0
}

/// Depth-bounded variant of [`extract_table_accesses`]. Returns the
/// extracted accesses plus a [`RecursionOutcome`] recording whether
/// a nested body was abandoned at the recursion-depth cap. The
/// caller must emit an honest typed diagnostic when
/// `outcome.limit_hit` (R13 — never silently truncate).
#[must_use]
pub fn extract_table_accesses_bounded(
    stmts: &[Statement],
) -> (Vec<TableAccess>, crate::RecursionOutcome) {
    let mut out: Vec<TableAccess> = Vec::new();
    let mut outcome = crate::RecursionOutcome::default();
    walk_table_accesses(stmts, 0, &mut out, &mut outcome);
    (dedup(out), outcome)
}

fn walk_table_accesses(
    stmts: &[Statement],
    depth: usize,
    out: &mut Vec<TableAccess>,
    outcome: &mut crate::RecursionOutcome,
) {
    // Recurse into a re-lowered control-flow body only while we
    // have depth budget. At the cap we record the truncation and
    // stop descending — never silently drop, never recurse
    // unbounded (which stack-overflows on a non-shrinking slice).
    macro_rules! recurse_body {
        ($text:expr) => {{
            if depth + 1 >= crate::MAX_RELOWER_DEPTH {
                outcome.note_truncated();
            } else {
                let lowered = crate::lower_statement_body($text);
                walk_table_accesses(&lowered, depth + 1, out, outcome);
            }
        }};
    }
    for stmt in stmts {
        match stmt {
            Statement::Sql { verb, raw_text } => {
                accesses_from_sql(*verb, raw_text, out);
            }
            Statement::If {
                arms,
                else_body_text,
            } => {
                for arm in arms {
                    recurse_body!(&arm.body_text);
                }
                if let Some(eb) = else_body_text {
                    recurse_body!(eb);
                }
            }
            Statement::ForLoop {
                range_text,
                body_text,
                ..
            } => {
                // A cursor FOR loop — `FOR r IN (SELECT … FROM t)
                // LOOP …` — reads the table(s) in its range
                // sub-SELECT. Strip the outer parens and re-lower so
                // those Read edges are not silently dropped
                // (oracle-xckj). A numeric range (`1..10`) lowers to
                // nothing verb-gated, so it is harmless to walk.
                if let Some(inner) = parenthesised_query(range_text) {
                    recurse_body!(inner);
                }
                recurse_body!(body_text);
            }
            Statement::WhileLoop { body_text, .. } | Statement::BareLoop { body_text } => {
                recurse_body!(body_text);
            }
            Statement::NestedBlock { body_text } => {
                // Anonymous `BEGIN … END` / `DECLARE … END` sub-block — a
                // common idiom for scoped exception handling. Any DML inside
                // it (`BEGIN UPDATE t SET … ; END;`) must still surface a
                // Read/Write edge, or the routine shows no dependency on `t`
                // with no diagnostic emitted. Strip the wrapper and re-lower
                // the inner statements, mirroring
                // `calls.rs::walk_call_sites`. Only recurse when the stripped
                // slice differs from the original so the depth-guarded
                // `recurse_body!` cannot spin on a non-stripping slice (the
                // cap already bounds a non-shrinking one).
                let inner = crate::calls::strip_block_wrapper(body_text);
                if inner != body_text.as_str() {
                    recurse_body!(inner);
                }
            }
            _ => {}
        }
    }
}

/// If `range_text` is a parenthesised query — the cursor form of a
/// `FOR` loop range, `(SELECT … FROM t)` — return the inner text
/// (parens stripped). A numeric / bounded range (`1..10`,
/// `REVERSE 1..n`) is not parenthesised and yields `None`.
fn parenthesised_query(range_text: &str) -> Option<&str> {
    let trimmed = range_text.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.trim())
}

fn accesses_from_sql(verb: SqlVerb, raw: &str, out: &mut Vec<TableAccess>) {
    let upper = raw.to_ascii_uppercase();
    match verb {
        SqlVerb::Select => {
            for t in tables_after(&upper, raw, "FROM") {
                push(out, t, AccessKind::Read);
            }
            for t in tables_after(&upper, raw, "JOIN") {
                push(out, t, AccessKind::Read);
            }
        }
        SqlVerb::Insert => {
            for t in tables_after(&upper, raw, "INTO") {
                push(out, t, AccessKind::Write);
            }
            // Sub-SELECT inside the INSERT is a read.
            for t in tables_after(&upper, raw, "FROM") {
                push(out, t, AccessKind::Read);
            }
        }
        SqlVerb::Update => {
            for t in tables_after(&upper, raw, "UPDATE") {
                push(out, t, AccessKind::Write);
            }
            for t in tables_after(&upper, raw, "FROM") {
                push(out, t, AccessKind::Read);
            }
        }
        SqlVerb::Delete => {
            // The DELETE target is the identifier immediately after the
            // `DELETE` keyword (the `FROM` is optional in Oracle:
            // `DELETE employees WHERE …` and `DELETE FROM employees WHERE …`
            // are both valid and write the same table). Deriving the target
            // only from `FROM` silently dropped the Write of a FROM-less
            // DELETE, so the dependency graph recorded no write on the
            // destroyed table and a cross-schema FROM-less DELETE was never
            // flagged DEP001 (oracle-j1ep.2).
            //
            // Trailing `FROM`/`JOIN` tables that are NOT the target come from a
            // WHERE sub-SELECT — e.g. `DELETE t WHERE id IN (SELECT id FROM
            // staging)` — and are READS, not writes. Pushing them as Writes
            // would mint a spurious `Writes staging` edge with reversed
            // data-flow direction (oracle-rwjl.6).
            let target = delete_target(raw);
            let target_folded = target.as_deref().map(folded_name);
            if let Some(t) = target {
                push(out, t, AccessKind::Write);
            }
            // The explicit `DELETE FROM t` form surfaces the target once in the
            // FROM list; consume that single occurrence so it is not also
            // tagged Read, but tag every other FROM table (sub-SELECT sources)
            // as a Read.
            let mut target_consumed = false;
            for t in tables_after(&upper, raw, "FROM") {
                if !target_consumed && Some(folded_name(&t)) == target_folded {
                    target_consumed = true;
                    continue;
                }
                push(out, t, AccessKind::Read);
            }
            for t in tables_after(&upper, raw, "JOIN") {
                push(out, t, AccessKind::Read);
            }
        }
        SqlVerb::Merge => {
            for t in tables_after(&upper, raw, "INTO") {
                push(out, t, AccessKind::Write);
            }
            for t in tables_after(&upper, raw, "USING") {
                push(out, t, AccessKind::Read);
            }
        }
    }
}

/// Case-fold a raw `[schema.]table` token the way [`push`] does, so a
/// FROM-clause table can be compared against the DELETE target.
fn folded_name(raw_name: &str) -> String {
    raw_name.to_ascii_uppercase()
}

/// The write target of a `DELETE`: the `[schema.]table` identifier
/// immediately after the `DELETE` keyword, skipping an optional leading
/// `FROM`. Oracle accepts both `DELETE t …` and `DELETE FROM t …`; the
/// table written is the same. Returns `None` only when no identifier
/// follows (a malformed statement the caller leaves unresolved rather
/// than guessing).
fn delete_target(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    // Skip the leading `DELETE` keyword.
    let mut i = 0;
    while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
        i += 1;
    }
    i = skip_ws(bytes, i);
    // Skip an optional `FROM` keyword (whole-word). Compare on the raw
    // byte slice so a multi-byte identifier immediately after `DELETE`
    // (`DELETE é★ …`) can never slice across a UTF-8 char boundary:
    // here `i` is anchored to the start of an arbitrary user token, not
    // to a found ASCII delimiter, so a blind `raw[i..i + 4]` could land
    // inside a codepoint and panic (oracle-y54x.2 char-boundary fix).
    if bytes[i..]
        .get(..4)
        .is_some_and(|w| w.eq_ignore_ascii_case(b"FROM"))
        && (i + 4 >= bytes.len() || !is_ident_byte(bytes[i + 4]))
    {
        i = skip_ws(bytes, i + 4);
    }
    // Read the `[schema.]table` token.
    let start = i;
    while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
        i += 1;
    }
    if i > start {
        Some(raw[start..i].to_string())
    } else {
        None
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Pull the identifier(s) immediately following each occurrence
/// of `keyword` (whole-word) in `raw`. Stops at the first
/// non-identifier token, so `FROM hr.employees e WHERE …` yields
/// `hr.employees`.
fn tables_after(upper: &str, raw: &str, keyword: &str) -> Vec<String> {
    let mut out = Vec::new();
    let kw = keyword.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut search = 0;
    while let Some(rel) = upper[search..].find(&kw) {
        let abs = search + rel;
        search = abs + kw.len();
        // Whole-word check.
        let prev_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after = abs + kw.len();
        let next_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if !(prev_ok && next_ok) {
            continue;
        }
        // Skip whitespace, then read the identifier (allowing
        // dotted `schema.table`).
        let mut i = after;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
            i += 1;
        }
        if i > start {
            out.push(raw[start..i].to_string());
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#'
}

fn push(out: &mut Vec<TableAccess>, raw_name: String, access: AccessKind) {
    let folded = raw_name.to_ascii_uppercase();
    let (schema, table) = match folded.rsplit_once('.') {
        Some((s, t)) if !t.is_empty() => (Some(s.to_string()), t.to_string()),
        _ => (None, folded),
    };
    if table.is_empty() || table == "DUAL" {
        return;
    }
    out.push(TableAccess {
        schema,
        table,
        access,
    });
}

/// A table written AND read in the same body keeps both entries
/// (the depgraph wants both edges); identical (schema, table,
/// access) triples dedupe.
fn dedup(mut v: Vec<TableAccess>) -> Vec<TableAccess> {
    let mut seen: std::collections::BTreeSet<(Option<String>, String, AccessKind)> =
        std::collections::BTreeSet::new();
    v.retain(|a| seen.insert((a.schema.clone(), a.table.clone(), a.access)));
    v
}

impl PartialOrd for AccessKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for AccessKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower_statement_body;

    #[test]
    fn select_from_is_a_read() {
        let s = lower_statement_body("SELECT id INTO v FROM employees;");
        let a = extract_table_accesses(&s);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].table, "EMPLOYEES");
        assert_eq!(a[0].access, AccessKind::Read);
    }

    #[test]
    fn delete_with_multibyte_first_token_does_not_panic() {
        // oracle-y54x.2: delete_target()'s optional-FROM probe did a blind
        // `raw[i..i + 4]` slice anchored at the start of the user token after
        // `DELETE`. A multi-byte first token (`DELETE é★ …`) put `i + 4` inside a
        // UTF-8 codepoint and panicked ("not a char boundary"). The byte-level
        // `bytes[i..].get(..4)` check is char-boundary-safe — extraction must
        // complete without panicking.
        let s = lower_statement_body("DELETE é★ WHERE x = 1;");
        let _ = extract_table_accesses(&s);
        let s2 = lower_statement_body("DELETE é★"); // no trailing tokens
        let _ = extract_table_accesses(&s2);
    }

    #[test]
    fn insert_into_is_a_write() {
        let s = lower_statement_body("INSERT INTO audit_log VALUES (1, 2);");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "AUDIT_LOG" && x.access == AccessKind::Write)
        );
    }

    #[test]
    fn insert_select_records_write_and_read() {
        let s =
            lower_statement_body("INSERT INTO summary SELECT dept_id, COUNT(*) FROM employees;");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "SUMMARY" && x.access == AccessKind::Write)
        );
        assert!(
            a.iter()
                .any(|x| x.table == "EMPLOYEES" && x.access == AccessKind::Read)
        );
    }

    #[test]
    fn update_is_a_write() {
        let s = lower_statement_body("UPDATE employees SET salary = salary * 1.1;");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "EMPLOYEES" && x.access == AccessKind::Write)
        );
    }

    #[test]
    fn delete_from_is_a_write() {
        let s = lower_statement_body("DELETE FROM stale_rows WHERE id < 100;");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "STALE_ROWS" && x.access == AccessKind::Write)
        );
    }

    // oracle-rwjl.6: a DELETE whose WHERE reads a staging table via a
    // subquery must tag the DELETE target as Write and the subquery table as
    // Read — never Write. The old DELETE arm pushed every `FROM` table as a
    // Write, minting a spurious `Writes STAGING` edge with reversed data-flow.
    #[test]
    fn delete_with_where_subquery_target_is_write_subquery_is_read() {
        let s = lower_statement_body("DELETE FROM t WHERE id IN (SELECT id FROM staging);");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "T" && x.access == AccessKind::Write),
            "DELETE target T must be a Write: {a:?}"
        );
        assert!(
            a.iter()
                .any(|x| x.table == "STAGING" && x.access == AccessKind::Read),
            "WHERE sub-SELECT table STAGING must be a Read: {a:?}"
        );
        assert!(
            !a.iter()
                .any(|x| x.table == "STAGING" && x.access == AccessKind::Write),
            "STAGING must NEVER be classified as a Write: {a:?}"
        );
    }

    // oracle-j1ep.2: Oracle's `FROM` is optional in a DELETE. A FROM-less
    // `DELETE employees WHERE …` writes EMPLOYEES exactly like
    // `DELETE FROM employees`, but the old arm derived the target only from a
    // `FROM` token and so produced no Write — the dependency graph recorded no
    // write on the destroyed table. The target must be the identifier right
    // after the DELETE keyword.
    #[test]
    fn from_less_delete_is_a_write() {
        let s = lower_statement_body("DELETE employees WHERE id = 5;");
        let a = extract_table_accesses(&s);
        assert_eq!(a.len(), 1, "exactly one access expected: {a:?}");
        assert_eq!(a[0].table, "EMPLOYEES");
        assert_eq!(a[0].schema, None);
        assert_eq!(a[0].access, AccessKind::Write);
    }

    // oracle-j1ep.2: a schema-qualified FROM-less DELETE writes the qualified
    // table, so the cross-schema write surface is visible to DEP001 downstream.
    #[test]
    fn from_less_qualified_delete_is_a_write() {
        let s = lower_statement_body("DELETE hr.audit_log WHERE ts < SYSDATE - 30;");
        let a = extract_table_accesses(&s);
        assert_eq!(a.len(), 1, "exactly one access expected: {a:?}");
        assert_eq!(a[0].schema.as_deref(), Some("HR"));
        assert_eq!(a[0].table, "AUDIT_LOG");
        assert_eq!(a[0].access, AccessKind::Write);
    }

    // oracle-j1ep.2 + oracle-rwjl.6: a FROM-less DELETE whose WHERE reads a
    // staging table via a subquery must tag the target Write and the subquery
    // table Read — never Write, and never miss the target.
    #[test]
    fn from_less_delete_with_where_subquery_target_write_subquery_read() {
        let s = lower_statement_body("DELETE t WHERE id IN (SELECT id FROM staging);");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "T" && x.access == AccessKind::Write),
            "FROM-less DELETE target T must be a Write: {a:?}"
        );
        assert!(
            a.iter()
                .any(|x| x.table == "STAGING" && x.access == AccessKind::Read),
            "WHERE sub-SELECT table STAGING must be a Read: {a:?}"
        );
        assert!(
            !a.iter()
                .any(|x| x.table == "STAGING" && x.access == AccessKind::Write),
            "STAGING must NEVER be classified as a Write: {a:?}"
        );
    }

    #[test]
    fn merge_writes_target_reads_source() {
        let s = lower_statement_body(
            "MERGE INTO target t USING source s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET t.v = s.v;",
        );
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "TARGET" && x.access == AccessKind::Write)
        );
        assert!(
            a.iter()
                .any(|x| x.table == "SOURCE" && x.access == AccessKind::Read)
        );
    }

    #[test]
    fn schema_qualified_table_split() {
        let s = lower_statement_body("SELECT 1 INTO v FROM hr.employees;");
        let a = extract_table_accesses(&s);
        assert_eq!(a[0].schema.as_deref(), Some("HR"));
        assert_eq!(a[0].table, "EMPLOYEES");
    }

    #[test]
    fn dual_is_filtered_out() {
        let s = lower_statement_body("SELECT SYSDATE INTO v FROM dual;");
        let a = extract_table_accesses(&s);
        assert!(a.is_empty());
    }

    #[test]
    fn loop_body_dml_recursed() {
        let s = lower_statement_body("FOR i IN 1..10 LOOP INSERT INTO log VALUES (i); END LOOP;");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "LOG" && x.access == AccessKind::Write)
        );
    }

    // oracle-xckj: a cursor FOR loop reads its iterated table via the
    // range sub-SELECT. `walk_table_accesses` must walk `range_text`,
    // not just `body_text`, or that Read edge is silently dropped.
    #[test]
    fn cursor_for_loop_range_select_table_is_read() {
        let s = lower_statement_body(
            "FOR r IN (SELECT id FROM src) LOOP \
             INSERT INTO dst VALUES (r.id); \
             END LOOP;",
        );
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "SRC" && x.access == AccessKind::Read),
            "cursor-FOR-loop range sub-SELECT read of SRC must be extracted: {a:?}"
        );
        // The body write must still be picked up.
        assert!(
            a.iter()
                .any(|x| x.table == "DST" && x.access == AccessKind::Write),
            "loop body write of DST must still be extracted: {a:?}"
        );
    }

    // oracle-xckj: a numeric range yields no tables and must not
    // produce spurious accesses.
    #[test]
    fn numeric_range_for_loop_yields_no_extra_tables() {
        let s = lower_statement_body("FOR i IN 1..10 LOOP NULL; END LOOP;");
        let a = extract_table_accesses(&s);
        assert!(a.is_empty(), "numeric range must not invent tables: {a:?}");
    }

    #[test]
    fn duplicate_access_triples_dedupe() {
        let s = lower_statement_body("SELECT 1 INTO a FROM t; SELECT 2 INTO b FROM t;");
        let acc = extract_table_accesses(&s);
        // Two reads of T collapse to one.
        assert_eq!(acc.iter().filter(|x| x.table == "T").count(), 1);
    }

    #[test]
    fn join_tables_are_reads() {
        let s = lower_statement_body(
            "SELECT 1 INTO v FROM employees e JOIN departments d ON e.dept = d.id;",
        );
        let a = extract_table_accesses(&s);
        assert!(a.iter().any(|x| x.table == "EMPLOYEES"));
        assert!(a.iter().any(|x| x.table == "DEPARTMENTS"));
        assert!(a.iter().all(|x| x.access == AccessKind::Read));
    }

    #[test]
    fn serde_round_trip() {
        let s = lower_statement_body("SELECT 1 INTO v FROM t;");
        let a = extract_table_accesses(&s);
        let json = serde_json::to_string(&a[0]).unwrap();
        let back: TableAccess = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a[0]);
        assert!(json.contains("\"access\":\"read\""));
    }

    // oracle-v4wa: the non-shrinking `FOR UPDATE` BareLoop (see the
    // matching test in `calls.rs`) recursed unbounded through
    // `extract_table_accesses` → `lower_statement_body` and aborted
    // the process. The bounded walk must terminate and report it.
    #[test]
    fn non_shrinking_for_update_terminates_and_reports_limit() {
        let stmts = vec![Statement::BareLoop {
            body_text: "FOR UPDATE".to_string(),
        }];
        let (accesses, outcome) = extract_table_accesses_bounded(&stmts);
        assert!(
            outcome.limit_hit,
            "non-shrinking BareLoop must trip the depth cap, \
             outcome={outcome:?}, accesses={accesses:?}"
        );
        assert!(outcome.truncated_bodies >= 1);
        let _ = extract_table_accesses(&stmts);
    }

    // oracle-hrzg.3: a DML statement inside an anonymous BEGIN…END
    // sub-block must still surface a Read/Write edge. Before the
    // NestedBlock arm in `walk_table_accesses`, the `_ => {}` catch-all
    // dropped the whole block, so a routine writing a table only via a
    // nested block showed no Write edge — silently, with no diagnostic.
    #[test]
    fn nested_block_update_yields_write_edge() {
        let s = lower_statement_body("BEGIN UPDATE secret_table SET x = 1 WHERE id = 9; END;");
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "SECRET_TABLE" && x.access == AccessKind::Write),
            "a nested-block UPDATE must surface a Write of SECRET_TABLE: {a:?}"
        );
    }

    // oracle-hrzg.3: a DECLARE…END sub-block (the other anonymous-block
    // shape) with DML must also surface its accesses.
    #[test]
    fn nested_declare_block_dml_yields_edges() {
        let s = lower_statement_body(
            "DECLARE v NUMBER; BEGIN INSERT INTO audit_log SELECT id FROM staging; END;",
        );
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "AUDIT_LOG" && x.access == AccessKind::Write),
            "nested-block INSERT target must be a Write: {a:?}"
        );
        assert!(
            a.iter()
                .any(|x| x.table == "STAGING" && x.access == AccessKind::Read),
            "nested-block sub-SELECT must be a Read: {a:?}"
        );
    }

    // oracle-hrzg.3: an IF arm whose body is a nested BEGIN…END DML block
    // exercises the parse-tree re-lower path (an arm body re-lowered to a
    // NestedBlock), confirming the gap is closed there too, not just at
    // the top level.
    #[test]
    fn if_arm_nested_block_dml_yields_edges() {
        let s = lower_statement_body(
            "IF p_flag = 1 THEN BEGIN UPDATE accounts SET bal = 0 WHERE id = 1; END; END IF;",
        );
        let a = extract_table_accesses(&s);
        assert!(
            a.iter()
                .any(|x| x.table == "ACCOUNTS" && x.access == AccessKind::Write),
            "an IF-arm nested-block UPDATE must surface a Write of ACCOUNTS: {a:?}"
        );
    }
}
