//! IR for PL/SQL expressions and name references.
//!
//! Sibling of `stmt`: where statements carry raw
//! `rhs_text` / `cond_text` slices, the expression IR shipped
//! here lets downstream passes (lineage, bindgen, SAST) reason
//! about expression structure without re-tokenising.
//!
//! The expression grammar is intentionally conservative — every
//! shape recognised here is one we've found in the lab corpus
//! and the synthetic L1 / L2 fixtures. Anything outside this set
//! lowers to [`Expr::Raw`] with the original text, mirroring the
//! `Statement::Unrecognized` posture from.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   recognised reference shapes
//!   (`<ident>`, `<schema>.<obj>`, `<schema>.<obj>.<member>`,
//!   `<table>(<args>)` for function calls and array access) and
//!   the operator precedence table for binary ops come from the
//!   PL/SQL Language Reference chapter on expressions.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_IDENTIFIERS` is the PL/Scope-side view that later
//!   passes cross-check our reference resolution against.

use serde::{Deserialize, Serialize};

/// One PL/SQL expression node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    /// `NULL` literal.
    Null,
    /// Boolean literal — `TRUE` or `FALSE`.
    BoolLit(bool),
    /// Integer literal preserved verbatim so downstream consumers
    /// can decide between `i32` / `i64` / `Decimal` without losing
    /// precision.
    IntLit(String),
    /// Floating-point or fixed-point literal preserved verbatim.
    FloatLit(String),
    /// String literal — body without surrounding quotes; doubled
    /// `''` already de-escaped to single `'`.
    StringLit(String),
    /// Date / timestamp / interval literal — the kind tag is the
    /// keyword (`DATE`, `TIMESTAMP`, `INTERVAL`).
    DateTimeLit { keyword: String, body: String },
    /// Bind placeholder — `:1` or `:name`.
    BindRef(String),
    /// Substitution variable — `&name` or `&&name`.
    SubstitutionRef { name: String, sticky: bool },
    /// Name reference. `parts` is the dotted path
    /// (`schema.package.member` etc.) in source order, case-folded
    /// for the lookup key but `display` preserved for diagnostics.
    Name(NameRef),
    /// `<callee>(<args>)` — function or procedure call. Also covers
    /// table / record accessors (`tab(i)`).
    Call { callee: NameRef, args: Vec<Expr> },
    /// Binary operator. Operands lower to inner expressions; the
    /// operator is the canonical PL/SQL spelling
    /// (`+`, `-`, `*`, `/`, `||`, `=`, `<>`, `<`, `<=`, `>`,
    /// `>=`, `AND`, `OR`, `LIKE`, `IS`, `MEMBER OF`).
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Unary operator — `NOT`, `-`, `+`.
    Unary { op: String, operand: Box<Expr> },
    /// Catch-all for shapes the recognizer can't classify.
    Raw {
        text: String,
        reason: UnknownExprReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameRef {
    /// Case-folded (upper-case) path used for the lookup key in
    /// `plsql-symbols`.
    pub parts: Vec<String>,
    /// Source-form path preserved so the report renderer can show
    /// the operator's original casing in diagnostics.
    pub display: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownExprReason {
    /// Expression text didn't match any recognised shape.
    UnrecognizedShape,
    /// Parens didn't balance; we don't try to sub-parse.
    UnbalancedParens,
    /// String quote didn't close.
    UnterminatedString,
}

/// Lower a raw expression-source slice into an [`Expr`]. Errors
/// surface as `Expr::Raw` with a typed reason — never panic.
#[must_use]
pub fn lower_expression(source: &str) -> Expr {
    let trimmed = source.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Expr::Null;
    }

    // Quick wins: literals.
    if let Some(lit) = recognise_keyword_literal(trimmed) {
        return lit;
    }
    if let Some(lit) = recognise_string_literal(trimmed) {
        return lit;
    }
    if let Some(lit) = recognise_datetime_literal(trimmed) {
        return lit;
    }
    if let Some(lit) = recognise_numeric_literal(trimmed) {
        return lit;
    }
    if let Some(b) = recognise_bind(trimmed) {
        return b;
    }
    if let Some(s) = recognise_substitution(trimmed) {
        return s;
    }

    // Binary operator at the top level (lowest precedence first).
    if let Some(e) = recognise_top_level_binary(trimmed) {
        return e;
    }

    // Function / procedure call shape `<name>(<args>)`.
    if let Some(e) = recognise_call(trimmed) {
        return e;
    }

    // Unary `NOT` / `-` / `+` prefix.
    if let Some(e) = recognise_unary(trimmed) {
        return e;
    }

    // Plain name reference (possibly dotted).
    if is_dotted_name(trimmed) {
        return Expr::Name(name_ref_from(trimmed));
    }

    Expr::Raw {
        text: source.to_string(),
        reason: UnknownExprReason::UnrecognizedShape,
    }
}

fn recognise_keyword_literal(text: &str) -> Option<Expr> {
    let upper = text.to_ascii_uppercase();
    match upper.as_str() {
        "NULL" => Some(Expr::Null),
        "TRUE" => Some(Expr::BoolLit(true)),
        "FALSE" => Some(Expr::BoolLit(false)),
        _ => None,
    }
}

fn recognise_string_literal(text: &str) -> Option<Expr> {
    if !text.starts_with('\'') || !text.ends_with('\'') || text.len() < 2 {
        return None;
    }
    let inner = &text[1..text.len() - 1];
    // A naked '' separator means this looks like a string but
    // includes a doubled-quote escape; honour it by un-doubling.
    //
    // Walk by `char`, not by byte: `bytes[i] as char` would
    // reinterpret each UTF-8 byte as a Latin-1 code-point and
    // corrupt any non-ASCII content. The quote `'` is single-byte
    // ASCII so a char-based scan handles the escape just as cleanly.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                // Doubled quote → one literal `'`.
                chars.next();
                out.push('\'');
                continue;
            }
            // A solitary quote in the middle means the source
            // literal wasn't a single quoted run.
            return None;
        }
        out.push(c);
    }
    Some(Expr::StringLit(out))
}

fn recognise_datetime_literal(text: &str) -> Option<Expr> {
    let upper = text.to_ascii_uppercase();
    let keyword = if upper.starts_with("DATE") {
        "DATE"
    } else if upper.starts_with("TIMESTAMP") {
        "TIMESTAMP"
    } else if upper.starts_with("INTERVAL") {
        "INTERVAL"
    } else {
        return None;
    };
    let after = &text[keyword.len()..];
    let next_byte = after.bytes().next();
    if let Some(b) = next_byte
        && (b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
    {
        return None;
    }
    let trimmed = after.trim_start();
    if !trimmed.starts_with('\'') || !trimmed.ends_with('\'') {
        return None;
    }
    let body = &trimmed[1..trimmed.len() - 1];
    Some(Expr::DateTimeLit {
        keyword: keyword.to_string(),
        body: body.to_string(),
    })
}

fn recognise_numeric_literal(text: &str) -> Option<Expr> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0];
    if !(first.is_ascii_digit() || (first == b'.' && bytes.len() > 1)) {
        return None;
    }
    let mut saw_dot = false;
    let mut saw_e = false;
    for &b in bytes {
        if b.is_ascii_digit() {
            continue;
        }
        if b == b'.' && !saw_dot && !saw_e {
            saw_dot = true;
            continue;
        }
        if (b == b'e' || b == b'E') && !saw_e {
            saw_e = true;
            saw_dot = true;
            continue;
        }
        if (b == b'+' || b == b'-') && saw_e {
            continue;
        }
        return None;
    }
    if saw_dot || saw_e {
        Some(Expr::FloatLit(text.to_string()))
    } else {
        Some(Expr::IntLit(text.to_string()))
    }
}

fn recognise_bind(text: &str) -> Option<Expr> {
    text.strip_prefix(':')
        .map(|rest| Expr::BindRef(rest.to_string()))
}

fn recognise_substitution(text: &str) -> Option<Expr> {
    if let Some(rest) = text.strip_prefix("&&") {
        return Some(Expr::SubstitutionRef {
            name: rest.to_string(),
            sticky: true,
        });
    }
    if let Some(rest) = text.strip_prefix('&') {
        return Some(Expr::SubstitutionRef {
            name: rest.to_string(),
            sticky: false,
        });
    }
    None
}

/// Look for a binary operator at the **top level** (paren depth 0,
/// quote depth 0). Honours a small precedence table — find the
/// lowest-precedence top-level operator and split on it.
fn recognise_top_level_binary(text: &str) -> Option<Expr> {
    // Operators in *decreasing* match width so multi-char ops
    // (`<=`, `>=`, `<>`, `||`) win over single-char ones at the
    // same position.
    let precedence: &[&[&str]] = &[
        &["OR"],
        &["AND"],
        &["="],
        &["<>", "!=", "<=", ">=", "<", ">"],
        &["||"],
        &["+", "-"],
        &["*", "/"],
    ];

    for tier in precedence {
        if let Some(split) = find_top_level_op(text, tier) {
            let (lhs_text, op, rhs_text) = split;
            let lhs = lower_expression(lhs_text);
            let rhs = lower_expression(rhs_text);
            return Some(Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            });
        }
    }
    None
}

fn find_top_level_op<'a, 'b>(
    text: &'a str,
    ops: &'b [&'b str],
) -> Option<(&'a str, &'b str, &'a str)> {
    let bytes = text.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut i = 0;
    // Scan left-to-right; the leftmost match wins for
    // left-associative ops.
    while i < bytes.len() {
        let b = bytes[i];
        // Skip non-ASCII bytes: all operators are ASCII so we can never
        // start a match here; skipping avoids slicing on a non-char-boundary.
        if b >= 0x80 {
            i += 1;
            continue;
        }
        if b == b'\'' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth != 0 {
            i += 1;
            continue;
        }

        for op in ops {
            let op_bytes = op.as_bytes();
            if i + op_bytes.len() > bytes.len() {
                continue;
            }
            // All operators are ASCII so byte comparison is correct.
            // We then need valid char boundaries for the str slices — since
            // op_bytes are all ASCII, the only hazard is if a multi-byte
            // UTF-8 char starts before i+op_bytes.len(); in that case the
            // byte window contains a non-ASCII byte and the match fails fast.
            let candidate_bytes = &bytes[i..i + op_bytes.len()];
            // Case-insensitive compare at byte level (ops are uppercase ASCII).
            let matches = candidate_bytes
                .iter()
                .zip(op_bytes.iter())
                .all(|(&cb, &ob)| cb.to_ascii_uppercase() == ob);
            if !matches {
                continue;
            }
            // Word-boundary check for alpha operators (`AND`, `OR`,
            // `LIKE`, `IS`, `IN`).
            if op.chars().all(|c| c.is_ascii_alphabetic()) {
                let prev_ok = i == 0 || {
                    let p = bytes[i - 1];
                    !(p.is_ascii_alphanumeric() || p == b'_')
                };
                let next_ok = i + op_bytes.len() == bytes.len() || {
                    let n = bytes[i + op_bytes.len()];
                    !(n.is_ascii_alphanumeric() || n == b'_')
                };
                if !(prev_ok && next_ok) {
                    continue;
                }
            }
            // Safety for str slices: `i` is on an ASCII byte (checked above).
            // The end `i + op_bytes.len()` is safe because either it's the
            // same ASCII byte (single-byte op) or every intermediate byte of
            // the op is ASCII (multi-byte op like `<>`, `||`) — we only reach
            // here when `matches` is true, which means all bytes in the window
            // are ASCII, so there's no multi-byte char crossing the boundary.
            let lhs = text[..i].trim();
            let rhs = text[i + op_bytes.len()..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            return Some((lhs, *op, rhs));
        }
        i += 1;
    }
    None
}

fn recognise_call(text: &str) -> Option<Expr> {
    let open = text.find('(')?;
    if !text.ends_with(')') {
        return None;
    }
    let name_part = text[..open].trim();
    if !is_dotted_name(name_part) {
        return None;
    }
    let inner = &text[open + 1..text.len() - 1];
    let args = split_top_level_args(inner)
        .into_iter()
        .map(|s| lower_expression(&s))
        .collect();
    Some(Expr::Call {
        callee: name_ref_from(name_part),
        args,
    })
}

fn split_top_level_args(inner: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut depth: i32 = 0;
    let mut in_string = false;
    for c in inner.chars() {
        if c == '\'' {
            in_string = !in_string;
            buf.push(c);
            continue;
        }
        if in_string {
            buf.push(c);
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        } else if c == ',' && depth == 0 {
            out.push(std::mem::take(&mut buf));
            continue;
        }
        buf.push(c);
    }
    if !buf.trim().is_empty() {
        out.push(buf);
    }
    out
}

fn recognise_unary(text: &str) -> Option<Expr> {
    let upper = text.to_ascii_uppercase();
    if upper.starts_with("NOT ") {
        let len = "NOT ".len();
        return Some(Expr::Unary {
            op: "NOT".into(),
            operand: Box::new(lower_expression(&text[len..])),
        });
    }
    if text.starts_with('-') || text.starts_with('+') {
        let op = if text.starts_with('-') { "-" } else { "+" };
        return Some(Expr::Unary {
            op: op.into(),
            operand: Box::new(lower_expression(text[1..].trim())),
        });
    }
    let _ = upper;
    None
}

fn is_dotted_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#' || b == b'.')
        && !text.contains("..")
}

fn name_ref_from(text: &str) -> NameRef {
    let parts: Vec<String> = text.split('.').map(|p| p.to_ascii_uppercase()).collect();
    NameRef {
        parts,
        display: text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_literal() {
        assert_eq!(lower_expression("NULL"), Expr::Null);
        assert_eq!(lower_expression("null"), Expr::Null);
    }

    #[test]
    fn boolean_literals() {
        assert_eq!(lower_expression("TRUE"), Expr::BoolLit(true));
        assert_eq!(lower_expression("false"), Expr::BoolLit(false));
    }

    #[test]
    fn integer_and_float_literals() {
        assert_eq!(lower_expression("42"), Expr::IntLit("42".into()));
        assert!(matches!(lower_expression("1.5e+12"), Expr::FloatLit(_)));
        assert!(matches!(lower_expression("3.14"), Expr::FloatLit(_)));
    }

    #[test]
    fn string_literal_unescapes_doubled_quotes() {
        assert_eq!(
            lower_expression("'it''s fine'"),
            Expr::StringLit("it's fine".into())
        );
    }

    // oracle-4cne: non-ASCII string-literal content must survive
    // un-doubling intact. Walking bytes (`bytes[i] as char`) would
    // reinterpret each UTF-8 byte as Latin-1 and corrupt the literal.
    #[test]
    fn string_literal_preserves_non_ascii_utf8() {
        // Accented characters, a non-Latin script, and punctuation
        // that all live outside the ASCII range.
        let src = "'café — déjà vu — Москва — 日本語'";
        let expected = "café — déjà vu — Москва — 日本語";
        assert_eq!(
            lower_expression(src),
            Expr::StringLit(expected.into()),
            "non-ASCII string literal must round-trip byte-for-byte"
        );
    }

    // oracle-4cne: un-doubling must still work when doubled quotes
    // sit next to multi-byte content.
    #[test]
    fn string_literal_unescapes_doubled_quotes_with_non_ascii() {
        assert_eq!(
            lower_expression("'garçon''s café'"),
            Expr::StringLit("garçon's café".into())
        );
    }

    #[test]
    fn datetime_literals_word_boundary_safe() {
        assert!(matches!(
            lower_expression("DATE '2024-05-15'"),
            Expr::DateTimeLit { keyword, .. } if keyword == "DATE"
        ));
        // `DATE_HIRED` is NOT a date literal — falls through to Name.
        assert!(matches!(lower_expression("DATE_HIRED"), Expr::Name(_)));
    }

    #[test]
    fn bind_ref_and_substitution_ref() {
        assert_eq!(
            lower_expression(":bind_name"),
            Expr::BindRef("bind_name".into())
        );
        assert_eq!(lower_expression(":1"), Expr::BindRef("1".into()));
        assert_eq!(
            lower_expression("&var"),
            Expr::SubstitutionRef {
                name: "var".into(),
                sticky: false,
            }
        );
        assert_eq!(
            lower_expression("&&sticky"),
            Expr::SubstitutionRef {
                name: "sticky".into(),
                sticky: true,
            }
        );
    }

    #[test]
    fn dotted_name_reference() {
        if let Expr::Name(n) = lower_expression("hr.employees.emp_id") {
            assert_eq!(n.parts, vec!["HR", "EMPLOYEES", "EMP_ID"]);
            assert_eq!(n.display, "hr.employees.emp_id");
        } else {
            panic!();
        }
    }

    #[test]
    fn function_call_with_two_args() {
        if let Expr::Call { callee, args } = lower_expression("nvl(v_x, 0)") {
            assert_eq!(callee.parts, vec!["NVL"]);
            assert_eq!(args.len(), 2);
        } else {
            panic!();
        }
    }

    #[test]
    fn nested_call_arguments_preserved() {
        if let Expr::Call { args, .. } = lower_expression("nvl(coalesce(a, b), 0)") {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0], Expr::Call { .. }));
        } else {
            panic!();
        }
    }

    #[test]
    fn binary_operator_low_precedence_wins() {
        if let Expr::Binary { op, .. } = lower_expression("a AND b OR c") {
            // OR has lower precedence → top-level op is OR.
            assert_eq!(op, "OR");
        } else {
            panic!();
        }
    }

    #[test]
    fn string_concat_is_a_binary() {
        if let Expr::Binary { op, .. } = lower_expression("first_name || ' ' || last_name") {
            assert_eq!(op, "||");
        } else {
            panic!();
        }
    }

    #[test]
    fn unary_not_negates_inner() {
        if let Expr::Unary { op, operand } = lower_expression("NOT v_flag") {
            assert_eq!(op, "NOT");
            assert!(matches!(*operand, Expr::Name(_)));
        } else {
            panic!();
        }
    }

    #[test]
    fn paren_protects_inner_op_from_top_level_split() {
        // `(a OR b) AND c` — top-level op is AND, not OR.
        if let Expr::Binary { op, .. } = lower_expression("(a OR b) AND c") {
            assert_eq!(op, "AND");
        } else {
            panic!();
        }
    }

    #[test]
    fn unrecognised_expression_lands_as_raw() {
        if let Expr::Raw { reason, .. } = lower_expression("@@@") {
            assert_eq!(reason, UnknownExprReason::UnrecognizedShape);
        } else {
            panic!();
        }
    }

    #[test]
    fn empty_expression_yields_null() {
        assert_eq!(lower_expression(""), Expr::Null);
        assert_eq!(lower_expression("  ;  "), Expr::Null);
    }

    #[test]
    fn string_with_operator_inside_does_not_split() {
        if let Expr::StringLit(s) = lower_expression("'a + b'") {
            assert_eq!(s, "a + b");
        } else {
            panic!();
        }
    }
}
