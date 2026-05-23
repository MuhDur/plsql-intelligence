//! Dynamic SQL evidence model.
//!
//! PL/SQL programs that build SQL at runtime via `EXECUTE
//! IMMEDIATE`, `DBMS_SQL`, or `OPEN cursor FOR <text>` create
//! opaque dependency edges: the parser-time analysis can see
//! the call site but not necessarily the resulting query. The
//! engine records what it can — string fragments, bind usage,
//! whether the operator wrapped the dynamic text in
//! `DBMS_ASSERT` for sanitisation, and the candidate object
//! names the fragments mention — and surfaces the rest as
//! `UnknownReason::DynamicSqlOpaque` (R13).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference —
//!   Dynamic SQL chapter (Native vs DBMS_SQL) drives the
//!   call-site shapes the recogniser walks.
//! * `LOW-LEVEL-CATALOGS.md` Supplied Package Buckets —
//!   `DBMS_ASSERT.SIMPLE_SQL_NAME / SCHEMA_NAME /
//!   ENQUOTE_NAME / SQL_OBJECT_NAME / ENQUOTE_LITERAL` are
//!   the canonical sanitisation entry points; detecting their
//!   use lets the engine soften the opacity verdict.

use serde::{Deserialize, Serialize};

/// One observed dynamic-SQL call site.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicSqlEvidence {
    /// Source location for the report (file path + 1-based line).
    pub site: String,
    /// The dynamic SQL fragments the engine extracted — string
    /// literals concatenated with `||`, with non-literal
    /// fragments replaced by `<expr>` placeholders so the
    /// fragment shape stays comparable.
    pub fragments: Vec<String>,
    /// True iff the call passed at least one bind variable
    /// (`USING …`). Bound variables let the engine treat the
    /// site as parameterised rather than fully opaque.
    pub uses_binds: bool,
    /// `DBMS_ASSERT` sanitisation functions detected wrapping
    /// the dynamic text or its substituted identifiers. The
    /// presence of any of these lets the SAST layer downgrade
    /// an "unbounded injection" verdict to "sanitised dynamic".
    pub dbms_assert_calls: Vec<DbmsAssertCall>,
    /// Candidate object names the recogniser inferred from the
    /// fragments (everything that pattern-matches
    /// `[schema.]identifier`).
    pub candidate_objects: Vec<CandidateObject>,
    /// What we know about why this call is dynamic. Used by R13
    /// reporting so the operator sees the reason in audit
    /// output.
    pub opacity_reason: OpacityReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbmsAssertCall {
    /// One of the `DBMS_ASSERT` supplied-package entry points
    /// (SIMPLE_SQL_NAME / SCHEMA_NAME / ENQUOTE_NAME /
    /// SQL_OBJECT_NAME / ENQUOTE_LITERAL / NOOP).
    pub function: String,
    /// Verbatim argument the call passed in, useful for
    /// confirming the operator passed the same input that
    /// becomes part of the dynamic statement.
    pub argument: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateObject {
    /// Optional schema prefix (`HR.EMPLOYEES`).
    pub schema: Option<String>,
    /// Object name candidate.
    pub object: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpacityReason {
    /// Every fragment is a literal — the recogniser can treat
    /// the assembled string as one constant statement.
    LiteralOnly,
    /// At least one fragment is non-literal — the assembled
    /// SQL depends on runtime expression values.
    ContainsExpression,
    /// The recogniser saw `DBMS_SQL.PARSE` or a `DBMS_SQL`
    /// call chain — fully opaque from the source view.
    DbmsSqlChain,
    /// `OPEN <cursor> FOR <ref-cursor-expression>` — the cursor
    /// definition is whatever the bind value provides.
    RefCursorBind,
}

/// Recognise a single dynamic-SQL call site from its raw call
/// text plus the surrounding source location. Returns
/// `Some(evidence)` when a dynamic-SQL shape was recognised,
/// `None` otherwise so the caller can keep walking.
pub fn recognise_dynamic_sql(call_text: &str, site: &str) -> Option<DynamicSqlEvidence> {
    let upper = call_text.to_ascii_uppercase();
    let trimmed = upper.trim_start();
    let (fragments, candidate_objects, uses_binds) = if trimmed.starts_with("EXECUTE IMMEDIATE") {
        extract_execute_immediate(call_text)
    } else if trimmed.contains("DBMS_SQL.") {
        return Some(DynamicSqlEvidence {
            site: site.into(),
            fragments: vec![],
            uses_binds: false,
            dbms_assert_calls: detect_dbms_assert_calls(call_text),
            candidate_objects: vec![],
            opacity_reason: OpacityReason::DbmsSqlChain,
        });
    } else if trimmed.starts_with("OPEN ")
        && trimmed.contains("FOR ")
        && !trimmed.contains("FOR SELECT")
        && !trimmed.contains("FOR INSERT")
        && !trimmed.contains("FOR UPDATE")
    {
        return Some(DynamicSqlEvidence {
            site: site.into(),
            fragments: vec![],
            uses_binds: trimmed.contains("USING "),
            dbms_assert_calls: detect_dbms_assert_calls(call_text),
            candidate_objects: vec![],
            opacity_reason: OpacityReason::RefCursorBind,
        });
    } else {
        return None;
    };
    let opacity_reason = if fragments.iter().any(|f| f.contains("<expr>")) {
        OpacityReason::ContainsExpression
    } else {
        OpacityReason::LiteralOnly
    };
    Some(DynamicSqlEvidence {
        site: site.into(),
        fragments,
        uses_binds,
        dbms_assert_calls: detect_dbms_assert_calls(call_text),
        candidate_objects,
        opacity_reason,
    })
}

/// Walk an `EXECUTE IMMEDIATE 'frag1' || expr || 'frag2'` body
/// and pull out the literal fragments + the candidate objects
/// any of those fragments mention. `<expr>` is the placeholder
/// for any non-literal piece so the fragment shape stays
/// comparable across runs.
fn extract_execute_immediate(text: &str) -> (Vec<String>, Vec<CandidateObject>, bool) {
    let upper = text.to_ascii_uppercase();
    let Some(after_kw) = upper.find("EXECUTE IMMEDIATE") else {
        return (vec![], vec![], false);
    };
    let rest = &text[after_kw + "EXECUTE IMMEDIATE".len()..];
    let using_pos = rest.to_ascii_uppercase().find(" USING ");
    let body = if let Some(p) = using_pos {
        &rest[..p]
    } else {
        rest
    };
    let body = body.trim().trim_end_matches(';').trim();

    // Walk by character; collect runs inside `'...'` as literal
    // fragments, runs outside as `<expr>` placeholders (collapsed
    // around `||`).
    let mut fragments: Vec<String> = Vec::new();
    let mut in_string = false;
    let mut current_lit = String::new();
    let mut current_expr_open = false;
    for c in body.chars() {
        if c == '\'' {
            if in_string {
                fragments.push(current_lit.clone());
                current_lit.clear();
                in_string = false;
            } else {
                if current_expr_open {
                    fragments.push("<expr>".into());
                    current_expr_open = false;
                }
                in_string = true;
            }
            continue;
        }
        if in_string {
            current_lit.push(c);
        } else if !c.is_whitespace() {
            current_expr_open = true;
        }
    }
    if current_expr_open {
        fragments.push("<expr>".into());
    }

    let candidate_objects = candidate_objects_from_fragments(&fragments);
    let uses_binds = using_pos.is_some();
    (fragments, candidate_objects, uses_binds)
}

/// Extract `[schema.]identifier` names from the literal fragments
/// — anything that matches the Oracle identifier shape after
/// stripping the surrounding SQL keywords.
fn candidate_objects_from_fragments(frags: &[String]) -> Vec<CandidateObject> {
    let mut out: Vec<CandidateObject> = Vec::new();
    for frag in frags {
        for word in frag.split(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#' || c == '.')
        }) {
            let word = word.trim().trim_matches('.');
            if word.is_empty() {
                continue;
            }
            if !word
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                continue;
            }
            // Skip common SQL keywords we don't want to surface
            // as candidate object names.
            let upper = word.to_ascii_uppercase();
            if matches!(
                upper.as_str(),
                "SELECT"
                    | "FROM"
                    | "WHERE"
                    | "INSERT"
                    | "INTO"
                    | "VALUES"
                    | "UPDATE"
                    | "SET"
                    | "DELETE"
                    | "MERGE"
                    | "AND"
                    | "OR"
                    | "NULL"
                    | "TRUE"
                    | "FALSE"
                    | "INNER"
                    | "OUTER"
                    | "LEFT"
                    | "RIGHT"
                    | "JOIN"
                    | "ON"
                    | "AS"
                    | "USING"
                    | "ORDER"
                    | "BY"
                    | "GROUP"
                    | "HAVING"
                    | "DUAL"
            ) {
                continue;
            }
            let (schema, object) = match word.rsplit_once('.') {
                Some((s, o)) if !o.is_empty() => (Some(s.to_string()), o.to_string()),
                _ => (None, word.to_string()),
            };
            let cand = CandidateObject { schema, object };
            if !out.contains(&cand) {
                out.push(cand);
            }
        }
    }
    out
}

/// Detect any `DBMS_ASSERT.<fn>(<arg>)` calls in the dynamic-SQL
/// surround. Identification is purely textual — confirmation that
/// the operator routed an identifier through the supplied package
/// before substituting it into the dynamic text.
fn detect_dbms_assert_calls(text: &str) -> Vec<DbmsAssertCall> {
    let upper = text.to_ascii_uppercase();
    let mut out: Vec<DbmsAssertCall> = Vec::new();
    for fname in [
        "SIMPLE_SQL_NAME",
        "SCHEMA_NAME",
        "ENQUOTE_NAME",
        "SQL_OBJECT_NAME",
        "ENQUOTE_LITERAL",
        "NOOP",
    ] {
        let needle = format!("DBMS_ASSERT.{fname}");
        let mut cursor = 0;
        while let Some(rel) = upper[cursor..].find(&needle) {
            let abs = cursor + rel + needle.len();
            cursor = abs;
            // Capture the argument inside the parentheses up to
            // the matching close-paren.
            let rest = &text[abs..];
            if !rest.trim_start().starts_with('(') {
                continue;
            }
            let open = rest.find('(').unwrap();
            let close = rest[open + 1..].find(')').map(|i| open + 1 + i);
            let Some(close) = close else { continue };
            let arg = rest[open + 1..close].trim().to_string();
            out.push(DbmsAssertCall {
                function: fname.into(),
                argument: arg,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_only_execute_immediate_recognised() {
        let ev = recognise_dynamic_sql("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';", "pkg.proc:42")
            .unwrap();
        assert_eq!(ev.fragments, vec!["SELECT 1 FROM dual"]);
        assert_eq!(ev.opacity_reason, OpacityReason::LiteralOnly);
        assert!(!ev.uses_binds);
    }

    #[test]
    fn execute_immediate_with_concat_marks_contains_expression() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || tbl_name || ' WHERE id = 1';",
            "pkg.proc:1",
        )
        .unwrap();
        assert_eq!(ev.opacity_reason, OpacityReason::ContainsExpression);
        // <expr> placeholder lands between the literal fragments.
        assert!(ev.fragments.iter().any(|f| f == "<expr>"));
        // Candidate objects extracted from literal fragments.
        assert!(ev.candidate_objects.iter().any(|c| c.object == "id"));
    }

    #[test]
    fn execute_immediate_with_using_marks_bind_usage() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'UPDATE t SET x = :1' USING v_x;",
            "pkg.proc:7",
        )
        .unwrap();
        assert!(ev.uses_binds);
    }

    #[test]
    fn dbms_sql_chain_classified_as_opaque() {
        let ev = recognise_dynamic_sql("v_cursor := DBMS_SQL.OPEN_CURSOR;", "pkg.proc:11").unwrap();
        assert_eq!(ev.opacity_reason, OpacityReason::DbmsSqlChain);
    }

    #[test]
    fn ref_cursor_open_for_text_classified_as_ref_cursor_bind() {
        let ev = recognise_dynamic_sql("OPEN v_cursor FOR v_dynamic_sql USING v_x;", "pkg.proc:13")
            .unwrap();
        assert_eq!(ev.opacity_reason, OpacityReason::RefCursorBind);
        assert!(ev.uses_binds);
    }

    #[test]
    fn open_cursor_for_static_select_not_classified_as_dynamic() {
        let r = recognise_dynamic_sql("OPEN v_cursor FOR SELECT id FROM t;", "pkg.proc:14");
        assert!(r.is_none());
    }

    #[test]
    fn dbms_assert_calls_detected() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'DROP TABLE ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab);",
            "pkg.proc:22",
        )
        .unwrap();
        let f = &ev.dbms_assert_calls;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].function, "SIMPLE_SQL_NAME");
        assert_eq!(f[0].argument, "p_tab");
    }

    #[test]
    fn candidate_objects_skip_sql_keywords() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT id, name FROM hr.employees WHERE id = :1' USING v_id;",
            "pkg.proc:30",
        )
        .unwrap();
        let names: Vec<&str> = ev
            .candidate_objects
            .iter()
            .map(|c| c.object.as_str())
            .collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
        assert!(names.iter().any(|n| n.contains("employees")));
        // Keywords filtered out.
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("SELECT")));
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("FROM")));
    }

    #[test]
    fn non_dynamic_input_returns_none() {
        assert!(recognise_dynamic_sql("v_x := 0;", "pkg.proc:1").is_none());
    }

    #[test]
    fn evidence_serde_round_trip() {
        let ev =
            recognise_dynamic_sql("EXECUTE IMMEDIATE 'SELECT 1 FROM dual';", "pkg.proc:1").unwrap();
        let json = serde_json::to_string(&ev).unwrap();
        let back: DynamicSqlEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev);
        assert!(json.contains("\"opacity_reason\":\"literal_only\""));
    }
}
