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
    /// Per-interpolation sanitisation status. One entry per
    /// `<expr>` placeholder the recogniser collapsed out of the
    /// concatenation, in left-to-right order. `true` means the
    /// non-literal run that produced that placeholder routed
    /// through a `DBMS_ASSERT` call (bounded); `false` means it
    /// is an unsanitised interpolation. A flat
    /// [`dbms_assert_calls`] count is *not* enough to conclude
    /// the object set is bounded — a single asserted identifier
    /// alongside an unsanitised one still permits injection — so
    /// the confidence scorer compares per-interpolation coverage
    /// here rather than trusting a bare non-empty assert list.
    pub expr_interpolations: Vec<ExprInterpolation>,
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

/// One non-literal run (`<expr>` placeholder) extracted from a
/// dynamic-SQL concatenation, tagged with whether the run was
/// routed through a `DBMS_ASSERT` sanitiser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExprInterpolation {
    /// `true` iff the source text of this interpolation contains
    /// a `DBMS_ASSERT.<fn>(...)` call — i.e. the substituted
    /// identifier is bounded by the asserted name. `false` for a
    /// bare expression that flows unsanitised into the statement.
    pub sanitised: bool,
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
    let (fragments, candidate_objects, uses_binds, expr_interpolations) =
        if trimmed.starts_with("EXECUTE IMMEDIATE") {
            extract_execute_immediate(call_text)
        } else if trimmed.contains("DBMS_SQL.") {
            return Some(DynamicSqlEvidence {
                site: site.into(),
                fragments: vec![],
                uses_binds: false,
                dbms_assert_calls: detect_dbms_assert_calls(call_text),
                expr_interpolations: vec![],
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
                expr_interpolations: vec![],
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
        expr_interpolations,
        candidate_objects,
        opacity_reason,
    })
}

/// Walk an `EXECUTE IMMEDIATE 'frag1' || expr || 'frag2'` body
/// and pull out the literal fragments + the candidate objects
/// any of those fragments mention. `<expr>` is the placeholder
/// for any non-literal piece so the fragment shape stays
/// comparable across runs.
fn extract_execute_immediate(
    text: &str,
) -> (Vec<String>, Vec<CandidateObject>, bool, Vec<ExprInterpolation>) {
    let upper = text.to_ascii_uppercase();
    let Some(after_kw) = upper.find("EXECUTE IMMEDIATE") else {
        return (vec![], vec![], false, vec![]);
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
    // fragments, runs outside as `<expr>` placeholders. Each
    // non-literal run is split on its top-level `||` operators so a
    // run that concatenates several substituted expressions (with no
    // intervening string literal, e.g. `DBMS_ASSERT.X(p) || p_raw`)
    // yields one `<expr>` placeholder and one [`ExprInterpolation`]
    // per operand. Tagging each operand independently keeps
    // per-interpolation coverage honest — an unsanitised operand
    // adjacent to an asserted one is no longer hidden behind the
    // asserted one (oracle-rwjl.12). Splitting respects paren depth
    // so a `DBMS_ASSERT(a || b)` argument is never torn apart.
    let mut fragments: Vec<String> = Vec::new();
    let mut expr_interpolations: Vec<ExprInterpolation> = Vec::new();
    let mut in_string = false;
    let mut current_lit = String::new();
    let mut current_expr = String::new();
    let mut current_expr_open = false;
    let flush_expr_run =
        |run: &str, frags: &mut Vec<String>, interps: &mut Vec<ExprInterpolation>| {
            for operand in split_top_level_concat(run) {
                frags.push("<expr>".into());
                interps.push(ExprInterpolation {
                    sanitised: run_is_dbms_assert(operand),
                });
            }
        };
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if in_string {
                // Oracle escapes an embedded single quote by doubling
                // it (`''`). A doubled quote is one literal `'`, not a
                // string close/reopen — peek ahead and fold it into the
                // current literal so the fragment stays intact and we
                // don't mint the inner text as a spurious candidate
                // object.
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    current_lit.push('\'');
                    continue;
                }
                fragments.push(current_lit.clone());
                current_lit.clear();
                in_string = false;
            } else {
                if current_expr_open {
                    flush_expr_run(&current_expr, &mut fragments, &mut expr_interpolations);
                    current_expr.clear();
                    current_expr_open = false;
                }
                in_string = true;
            }
            continue;
        }
        if in_string {
            current_lit.push(c);
        } else {
            current_expr.push(c);
            if !c.is_whitespace() {
                current_expr_open = true;
            }
        }
    }
    if current_expr_open {
        flush_expr_run(&current_expr, &mut fragments, &mut expr_interpolations);
    }

    let candidate_objects = candidate_objects_from_fragments(&fragments);
    let uses_binds = using_pos.is_some();
    (fragments, candidate_objects, uses_binds, expr_interpolations)
}

/// Split a non-literal concatenation run into its top-level `||`
/// operands, returning each operand trimmed of surrounding
/// whitespace with empty operands dropped. The split honours paren
/// depth so a `||` *inside* a call argument (e.g.
/// `DBMS_ASSERT.SIMPLE_SQL_NAME(a || b)`) is not treated as an
/// operand separator. The run is always outside any SQL string
/// literal (the walker handles `'...'` separately), so a single
/// quote never appears here and only paren depth matters.
fn split_top_level_concat(run: &str) -> Vec<&str> {
    let bytes = run.as_bytes();
    let mut operands: Vec<&str> = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                let piece = run[start..i].trim();
                if !piece.is_empty() {
                    operands.push(piece);
                }
                i += 2;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let piece = run[start..].trim();
    if !piece.is_empty() {
        operands.push(piece);
    }
    operands
}

/// True iff a single top-level `||` operand of a non-literal
/// concatenation run routed its value through a *validating*
/// `DBMS_ASSERT.<fn>(...)` sanitiser. Purely textual: the caller
/// passes one operand (already split out by
/// [`split_top_level_concat`]), so a validating-assert mention means
/// *this* substituted identifier is bounded by the asserted name —
/// it can no longer claim sanitisation for an adjacent operand that
/// merely shared the same `||` run.
///
/// Recognition is gated on a VALIDATORS allowlist that mirrors
/// `plsql-ir/src/flow_intra.rs::is_dbms_assert_sanitizer` — a bare
/// `contains("DBMS_ASSERT.")` is *not* enough. `DBMS_ASSERT.NOOP` is
/// Oracle's documented identity pass-through that performs zero
/// validation and returns its argument unchanged (it is detected
/// textually by [`detect_dbms_assert_calls`] for evidence, but it is
/// **not** a sanitiser); it, and any unknown or future `DBMS_ASSERT`
/// entry point, must fall through to `false` so the operand stays
/// unsanitised and the scorer reports the injection surface honestly
/// instead of falsely claiming the object set is bounded
/// (oracle-clgt.2). An optional leading schema segment
/// (`SYS.DBMS_ASSERT.SIMPLE_SQL_NAME`) is tolerated.
fn run_is_dbms_assert(run: &str) -> bool {
    /// Validating `DBMS_ASSERT` entry points — those that actually
    /// bound the substituted name. Mirrors the IR taint engine's
    /// allowlist; NOOP is deliberately absent (identity pass-through,
    /// not a validator).
    const VALIDATORS: &[&str] = &[
        "SIMPLE_SQL_NAME",
        "QUALIFIED_SQL_NAME",
        "SCHEMA_NAME",
        "ENQUOTE_NAME",
        "SQL_OBJECT_NAME",
        "ENQUOTE_LITERAL",
    ];
    let upper = run.to_ascii_uppercase();
    // Scan every `DBMS_ASSERT.` occurrence in the operand and accept
    // the operand iff at least one names a validating function. The
    // function token is the run of identifier characters immediately
    // after the dot; comparing the *exact* token (not a prefix)
    // ensures `NOOP` and unknown entry points never match.
    let needle = "DBMS_ASSERT.";
    let mut cursor = 0;
    while let Some(rel) = upper[cursor..].find(needle) {
        let fn_start = cursor + rel + needle.len();
        let func: String = upper[fn_start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$' || *c == '#')
            .collect();
        if VALIDATORS.contains(&func.as_str()) {
            return true;
        }
        cursor = fn_start;
    }
    false
}

/// Blank out embedded single-quoted SQL string literals inside a
/// dynamic-SQL fragment, replacing each `'...'` run (and its content)
/// with spaces. The walker has already coalesced Oracle's doubled-`''`
/// escape into single `'` characters, so a `'` here opens or closes a
/// real embedded string value — that value is data, never an object
/// name, so it must not be mined as a candidate identifier (e.g. the
/// `done` in `SET note = 'done'`).
fn strip_embedded_string_literals(frag: &str) -> String {
    let mut out = String::with_capacity(frag.len());
    let mut in_literal = false;
    for c in frag.chars() {
        if c == '\'' {
            in_literal = !in_literal;
            out.push(' ');
        } else if in_literal {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract `[schema.]identifier` names from the literal fragments
/// — anything that matches the Oracle identifier shape after
/// stripping the surrounding SQL keywords.
fn candidate_objects_from_fragments(frags: &[String]) -> Vec<CandidateObject> {
    let mut out: Vec<CandidateObject> = Vec::new();
    for frag in frags {
        let frag = strip_embedded_string_literals(frag);
        let frag = frag.as_str();
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
    fn expr_interpolations_tag_per_run_sanitisation() {
        // One asserted identifier + one raw interpolation: the
        // recogniser must tag each `<expr>` run independently so the
        // confidence scorer can detect partial coverage.
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab) || ' WHERE c=''' || p_raw || '''';",
            "pkg.proc:55",
        )
        .unwrap();
        assert!(
            ev.expr_interpolations.iter().any(|e| e.sanitised),
            "DBMS_ASSERT run should be sanitised: {:?}",
            ev.expr_interpolations
        );
        assert!(
            ev.expr_interpolations.iter().any(|e| !e.sanitised),
            "bare p_raw run should be unsanitised: {:?}",
            ev.expr_interpolations
        );
    }

    #[test]
    fn expr_interpolations_all_sanitised_when_fully_asserted() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'GRANT ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p) || ' TO ' || DBMS_ASSERT.ENQUOTE_NAME(g);",
            "pkg.proc:56",
        )
        .unwrap();
        assert!(!ev.expr_interpolations.is_empty());
        assert!(
            ev.expr_interpolations.iter().all(|e| e.sanitised),
            "every interpolation should be sanitised: {:?}",
            ev.expr_interpolations
        );
    }

    /// Regression for oracle-rwjl.12: when an asserted identifier and
    /// a raw identifier are concatenated *adjacently* with `||` (no
    /// intervening string literal), the recogniser must split the run
    /// on the top-level `||` and tag each operand independently.
    /// Before the fix the whole run collapsed into a single `<expr>`
    /// tagged `sanitised:true` (a bare `contains("DBMS_ASSERT.")`),
    /// falsely reporting the unsanitised `p_raw` operand as bounded.
    #[test]
    fn adjacent_concat_run_split_per_operand_sanitisation() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab) || p_raw || ' WHERE 1=1';",
            "pkg.proc:60",
        )
        .unwrap();
        // The asserted-then-raw run must yield TWO interpolations, not
        // one collapsed run.
        assert_eq!(
            ev.expr_interpolations.len(),
            2,
            "adjacent `||` operands must each get their own interpolation: {:?}",
            ev.expr_interpolations
        );
        // One placeholder per operand keeps the documented 1:1
        // fragment↔interpolation correspondence.
        assert_eq!(
            ev.fragments.iter().filter(|f| *f == "<expr>").count(),
            2,
            "one <expr> placeholder per operand: {:?}",
            ev.fragments
        );
        assert!(
            ev.expr_interpolations.iter().any(|e| e.sanitised),
            "DBMS_ASSERT operand should be sanitised: {:?}",
            ev.expr_interpolations
        );
        assert!(
            ev.expr_interpolations.iter().any(|e| !e.sanitised),
            "bare p_raw operand must be unsanitised, not hidden behind the asserted one: {:?}",
            ev.expr_interpolations
        );
    }

    /// A `||` *inside* a DBMS_ASSERT argument must not be treated as a
    /// top-level operand separator: the whole call (with its nested
    /// concatenation) is one sanitised operand. (No embedded string
    /// literal here — the outer walker splits the run at `'...'`, so
    /// the paren-depth logic only governs literal-free runs.)
    #[test]
    fn concat_inside_assert_argument_not_split() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'DROP TABLE ' || DBMS_ASSERT.SIMPLE_SQL_NAME(p_schema || p_sep || p_tab);",
            "pkg.proc:61",
        )
        .unwrap();
        // The nested `||` lives inside the call parens, so the run is a
        // single asserted operand.
        assert_eq!(
            ev.expr_interpolations.len(),
            1,
            "nested `||` inside the assert arg must not split the run: {:?}",
            ev.expr_interpolations
        );
        assert!(
            ev.expr_interpolations[0].sanitised,
            "the lone operand is the DBMS_ASSERT call and is sanitised: {:?}",
            ev.expr_interpolations
        );
    }

    /// Regression for oracle-clgt.2: `DBMS_ASSERT.NOOP` is Oracle's
    /// documented identity pass-through — it validates nothing and
    /// returns its argument unchanged, so it must NOT mark an
    /// interpolation as sanitised. Before the fix `run_is_dbms_assert`
    /// used a bare `contains("DBMS_ASSERT.")`, so a NOOP-wrapped raw
    /// value was falsely tagged `sanitised:true` (and the scorer then
    /// claimed the object set was bounded — an injection fail-open).
    #[test]
    fn dbms_assert_noop_is_not_a_sanitiser() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || DBMS_ASSERT.NOOP(p_tab);",
            "pkg.proc:70",
        )
        .unwrap();
        // NOOP is still recorded textually for evidence...
        assert!(
            ev.dbms_assert_calls.iter().any(|c| c.function == "NOOP"),
            "NOOP should still be detected textually: {:?}",
            ev.dbms_assert_calls
        );
        // ...but it must NOT count as sanitisation.
        assert_eq!(
            ev.expr_interpolations.len(),
            1,
            "one interpolation for the NOOP-wrapped operand: {:?}",
            ev.expr_interpolations
        );
        assert!(
            !ev.expr_interpolations[0].sanitised,
            "NOOP performs no validation — the operand must be unsanitised: {:?}",
            ev.expr_interpolations
        );
    }

    /// Regression for oracle-clgt.2: an unknown / future `DBMS_ASSERT`
    /// entry point (not on the validator allowlist) must also fall
    /// through to unsanitised rather than being trusted on the bare
    /// `DBMS_ASSERT.` prefix.
    #[test]
    fn unknown_dbms_assert_fn_is_not_a_sanitiser() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'SELECT * FROM ' || DBMS_ASSERT.SOME_FUTURE_FN(p_tab);",
            "pkg.proc:71",
        )
        .unwrap();
        assert_eq!(
            ev.expr_interpolations.len(),
            1,
            "one interpolation for the wrapped operand: {:?}",
            ev.expr_interpolations
        );
        assert!(
            !ev.expr_interpolations[0].sanitised,
            "an unrecognised DBMS_ASSERT entry point must not claim sanitisation: {:?}",
            ev.expr_interpolations
        );
    }

    /// A real validating entry point reached through a schema prefix
    /// (`SYS.DBMS_ASSERT.SIMPLE_SQL_NAME`) must still be recognised as
    /// a sanitiser — the allowlist gate must not over-correct and drop
    /// genuinely cleansed values.
    #[test]
    fn schema_qualified_dbms_assert_validator_is_sanitiser() {
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'DROP TABLE ' || SYS.DBMS_ASSERT.SIMPLE_SQL_NAME(p_tab);",
            "pkg.proc:72",
        )
        .unwrap();
        assert_eq!(ev.expr_interpolations.len(), 1);
        assert!(
            ev.expr_interpolations[0].sanitised,
            "schema-qualified SIMPLE_SQL_NAME must be sanitised: {:?}",
            ev.expr_interpolations
        );
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
    fn doubled_quote_literal_stays_one_fragment() {
        // Oracle escapes an embedded `'` by doubling it. The walker
        // must fold `''` into one literal quote rather than splitting
        // the statement at each quote — otherwise the inner text gets
        // wrongly minted as a candidate object and the single literal
        // is reported as several fragments.
        let ev = recognise_dynamic_sql(
            "EXECUTE IMMEDIATE 'UPDATE t SET note = ''done'' WHERE id = 5';",
            "pkg.proc:88",
        )
        .unwrap();
        // One literal statement, doubled quotes coalesced to a single
        // embedded quote.
        assert_eq!(
            ev.fragments,
            vec!["UPDATE t SET note = 'done' WHERE id = 5"]
        );
        // Even number of quote toggles => still a constant statement.
        assert_eq!(ev.opacity_reason, OpacityReason::LiteralOnly);
        assert!(ev.expr_interpolations.is_empty());
        // The escaped literal content must NOT surface as an object.
        assert!(
            !ev.candidate_objects
                .iter()
                .any(|c| c.object.eq_ignore_ascii_case("done")),
            "escaped literal content leaked as candidate object: {:?}",
            ev.candidate_objects
        );
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
