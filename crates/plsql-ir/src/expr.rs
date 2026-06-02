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
    /// Expression nesting exceeded [`MAX_EXPR_DEPTH`]. A crafted
    /// flat binary chain (`a OR a OR … OR a`, ~8000 operands) or a
    /// deeply-nested paren / call / unary spine would otherwise drive
    /// `lower_expression` into linear-depth recursion that overflows
    /// the stack and aborts the analyzer (SIGABRT) — an unrecoverable
    /// DoS on untrusted PL/SQL input. At the cap we stop recursing and
    /// surface the truncation honestly as this typed reason (R13:
    /// never crash, never silently swallow uncertainty) rather than
    /// descending further.
    ExprDepthLimit,
}

/// Maximum expression-lowering recursion depth. Real well-formed
/// PL/SQL expressions nest far below this; the cap exists only so a
/// crafted flat binary chain or pathologically-nested paren / call /
/// unary spine cannot drive `lower_expression` (and the secondary
/// tree-walk consumers that re-walk the produced `Box<Expr>` chain to
/// identical depth — `collect_calls`, `collect_expr_flow`,
/// `canonicalize_expr`) into a stack-overflow / SIGABRT. Chosen high
/// enough that it never clips genuine expressions and low enough that
/// 256 frames of the walk cannot overflow even a 2 MiB tokio worker
/// stack. Mirrors the honest-degradation posture of
/// [`crate::MAX_RELOWER_DEPTH`].
pub const MAX_EXPR_DEPTH: usize = 256;

/// Lower a raw expression-source slice into an [`Expr`]. Errors
/// surface as `Expr::Raw` with a typed reason — never panic.
///
/// The public signature is depth-agnostic; the recursion budget is
/// threaded internally via [`lower_expression_depth`] so a crafted
/// flat binary chain or deep paren/call/unary spine in untrusted
/// input degrades to [`UnknownExprReason::ExprDepthLimit`] at
/// [`MAX_EXPR_DEPTH`] instead of overflowing the stack.
#[must_use]
pub fn lower_expression(source: &str) -> Expr {
    lower_expression_depth(source, 0)
}

/// Depth-bounded core of [`lower_expression`]. `depth` is the current
/// recursion depth; every internal recursion site passes `depth + 1`.
/// At `depth >= MAX_EXPR_DEPTH` we refuse to descend and return an
/// honest [`UnknownExprReason::ExprDepthLimit`] `Raw` node carrying the
/// untouched source, so the cap is surfaced as a typed degradation
/// rather than silently swallowed.
#[must_use]
fn lower_expression_depth(source: &str, depth: usize) -> Expr {
    if depth >= MAX_EXPR_DEPTH {
        return Expr::Raw {
            text: source.to_string(),
            reason: UnknownExprReason::ExprDepthLimit,
        };
    }
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
    if let Some(e) = recognise_top_level_binary(trimmed, depth) {
        return e;
    }

    // Function / procedure call shape `<name>(<args>)`.
    if let Some(e) = recognise_call(trimmed, depth) {
        return e;
    }

    // Unary `NOT` / `-` / `+` prefix.
    if let Some(e) = recognise_unary(trimmed, depth) {
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
    // Mirror `recognise_string_literal`'s `len < 2` guard. For a lone `'`,
    // `starts_with('\'')` AND `ends_with('\'')` both inspect the same single
    // byte and are true, so without this guard `&trimmed[1..0]` panics with
    // "begin > end (1 > 0)". Both quotes are single-byte ASCII so the byte
    // length comparison is correct. (oracle-ajm2.3)
    if trimmed.len() < 2 || !trimmed.starts_with('\'') || !trimmed.ends_with('\'') {
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
///
/// For the matching tier we collect **every** top-level operand run
/// in one pass and build the `Expr::Binary` spine *iteratively*
/// (right-fold, matching the leftmost-split recursive shape) so a
/// flat left-associative chain (`a OR a OR … OR a`, thousands of
/// operands) no longer drives lowering recursion linearly with
/// operand count — it would otherwise overflow the stack and abort
/// the analyzer on crafted untrusted input. `depth` backstops nested
/// paren / call / unary spines and bounds the produced tree's depth so
/// the secondary tree-walk consumers (`collect_calls`,
/// `collect_expr_flow`, `canonicalize_expr`) that re-walk the chain to
/// identical depth stay bounded too.
fn recognise_top_level_binary(text: &str, depth: usize) -> Option<Expr> {
    // Operators in *decreasing* match width so multi-char ops
    // (`<=`, `>=`, `<>`, `||`) win over single-char ones at the
    // same position.
    // `=` shares the relational tier with the comparison operators so a
    // separate, higher `&["="]` tier cannot match the `=` byte inside `<=`,
    // `>=`, or `!=` before the 2-char form is tried. Multi-char ops stay
    // ahead of single-char ones within the tier (the scan tries each op
    // left-to-right at the same byte), so `a <= b` reaches the `<` byte and
    // matches `<=` rather than splitting on the embedded `=`. (oracle-ajm2.10)
    let precedence: &[&[&str]] = &[
        &["OR"],
        &["AND"],
        &["<>", "!=", "<=", ">=", "=", "<", ">"],
        &["||"],
        &["+", "-"],
        &["*", "/"],
    ];

    for tier in precedence {
        // Collect operand segments and the operator between each
        // adjacent pair, in source order. Empty when the tier has no
        // top-level op here, so we fall through to the next tier.
        let (operands, ops) = find_all_top_level_ops(text, tier);
        if ops.is_empty() {
            continue;
        }
        // Right-fold to reproduce the leftmost-split recursive shape:
        // `a OP b OP c` → `OP(a, OP(b, c))`. Building the spine in a
        // loop (rather than via `lower_expression` re-descending the
        // tail) keeps a flat N-operand chain from costing N stack
        // frames during lowering.
        //
        // The produced spine is right-leaning, so the i-th `Binary`
        // node (counting the topmost as level `depth`) sits at tree
        // depth `depth + i`, and the left operands are the *shallow*
        // ones while the rightmost operands form the *deep* tail. To
        // keep the WHOLE produced tree — operands included — bounded by
        // `MAX_EXPR_DEPTH` (so the downstream tree-walk consumers that
        // re-walk the chain stay within the stack), we materialise only
        // the shallow prefix as real `Binary` nodes and collapse the
        // deep tail (everything from `cap` onward, joined by its ops)
        // into a single honest `ExprDepthLimit` `Raw`.
        let n = operands.len();
        // `cap` is the number of leading operands we can keep as a real
        // spine before the next `Binary` node would reach the depth cap.
        // The node wrapping `operands[idx]` sits at tree depth
        // `depth + idx`, so we may keep indices with `depth + idx <
        // MAX_EXPR_DEPTH`. There are at most `n - 1` `Binary` nodes.
        let cap = MAX_EXPR_DEPTH.saturating_sub(depth).min(n - 1);
        // Seed the fold with the deep tail. When `cap < n - 1` the tail
        // `operands[cap..]` (with the ops joining them) overflows the
        // budget and degrades honestly; otherwise the tail is just the
        // rightmost operand lowered normally.
        let mut acc = if cap < n - 1 {
            Expr::Raw {
                text: join_operand_tail(text, &operands, cap),
                reason: UnknownExprReason::ExprDepthLimit,
            }
        } else {
            // cap == n - 1: the whole chain fits. The rightmost operand
            // sits at tree depth `depth + (n - 1)`.
            lower_expression_depth(operands[n - 1], depth + n)
        };
        for idx in (0..cap).rev() {
            // `Binary` node for `operands[idx]` sits at tree depth
            // `depth + idx`; its lhs operand one level deeper.
            let lhs = lower_expression_depth(operands[idx], depth + idx + 1);
            acc = Expr::Binary {
                op: ops[idx].to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(acc),
            };
        }
        return Some(acc);
    }
    None
}

/// Collect **every** top-level (paren depth 0, outside string) split
/// point for `ops`, in source order, returning the operand runs and the
/// operators between them. Used to fold a flat binary chain iteratively
/// rather than recursively (see [`recognise_top_level_binary`]) so a
/// crafted ~8000-operand chain cannot drive lowering into linear-depth
/// recursion and overflow the stack.
///
/// The per-byte scan, the case-insensitive ASCII compare, the
/// word-boundary check for alpha operators (`AND`, `OR`), the non-ASCII
/// / in-string skips, and the empty-operand skip mirror what repeated
/// leftmost-split recursion would have produced. Scanning by byte and
/// slicing only on validated ASCII operator positions keeps every
/// produced operand slice on a UTF-8 char boundary even for content
/// with multi-byte characters.
///
/// Returns `(operands, ops)` with `operands.len() == ops.len() + 1` when
/// at least one operator matched, and `(vec![], vec![])` otherwise.
fn find_all_top_level_ops<'a, 'b>(
    text: &'a str,
    ops: &'b [&'b str],
) -> (Vec<&'a str>, Vec<&'b str>) {
    let bytes = text.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut i = 0;
    // Start of the current operand run (byte offset just past the
    // previous operator, or 0 for the first run).
    let mut seg_start = 0usize;
    let mut operands: Vec<&'a str> = Vec::new();
    let mut found_ops: Vec<&'b str> = Vec::new();
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

        let mut matched = false;
        for op in ops {
            let op_bytes = op.as_bytes();
            if i + op_bytes.len() > bytes.len() {
                continue;
            }
            let candidate_bytes = &bytes[i..i + op_bytes.len()];
            let matches = candidate_bytes
                .iter()
                .zip(op_bytes.iter())
                .all(|(&cb, &ob)| cb.to_ascii_uppercase() == ob);
            if !matches {
                continue;
            }
            // Word-boundary check for alpha operators (`AND`, `OR`).
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
            // The operand to the left of this operator and the would-be
            // operand to its right must both be non-empty after trimming
            // so e.g. a leading unary `-` is not mistaken for a binary
            // split point, and a trailing operator never yields an empty
            // operand.
            let lhs = text[seg_start..i].trim();
            let rhs = text[i + op_bytes.len()..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            operands.push(lhs);
            found_ops.push(*op);
            // Resume scanning past this operator; the next run starts here.
            i += op_bytes.len();
            seg_start = i;
            matched = true;
            break;
        }
        if !matched {
            i += 1;
        }
    }
    if found_ops.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // The final operand run from the last operator to the end.
    operands.push(text[seg_start..].trim());
    (operands, found_ops)
}

/// Reconstruct the source text of the deep operand tail
/// `operands[cap..]` (with their joining operators) for an honest
/// `ExprDepthLimit` `Raw`. The operand slices are all sub-slices of the
/// original `text`, so the tail spans from the start of `operands[cap]`
/// to the end of the last operand — recovered by byte-offset arithmetic
/// against `text` rather than re-joining with a guessed separator.
fn join_operand_tail(text: &str, operands: &[&str], cap: usize) -> String {
    debug_assert!(cap < operands.len());
    // SAFETY of offsets: every entry in `operands` is a trimmed
    // sub-slice of `text`, so `as_ptr()` arithmetic yields a valid byte
    // offset within `text`. The tail runs from the first byte of
    // `operands[cap]` to the last byte of the final operand.
    let base = text.as_ptr() as usize;
    let first = operands[cap];
    let last = operands[operands.len() - 1];
    let start = (first.as_ptr() as usize).saturating_sub(base);
    let end_rel = (last.as_ptr() as usize).saturating_sub(base) + last.len();
    let end = end_rel.min(text.len());
    if start <= end && text.is_char_boundary(start) && text.is_char_boundary(end) {
        text[start..end].to_string()
    } else {
        // Defensive fallback (should not happen): preserve the whole text.
        text.to_string()
    }
}

fn recognise_call(text: &str, depth: usize) -> Option<Expr> {
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
        .map(|s| lower_expression_depth(&s, depth + 1))
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

fn recognise_unary(text: &str, depth: usize) -> Option<Expr> {
    let upper = text.to_ascii_uppercase();
    if upper.starts_with("NOT ") {
        let len = "NOT ".len();
        return Some(Expr::Unary {
            op: "NOT".into(),
            operand: Box::new(lower_expression_depth(&text[len..], depth + 1)),
        });
    }
    if text.starts_with('-') || text.starts_with('+') {
        let op = if text.starts_with('-') { "-" } else { "+" };
        return Some(Expr::Unary {
            op: op.into(),
            operand: Box::new(lower_expression_depth(text[1..].trim(), depth + 1)),
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
    fn relational_two_char_ops_not_mis_split_on_embedded_equals() {
        // oracle-ajm2.10: a higher-precedence `&["="]` tier matched the `=`
        // byte inside `<=`/`>=`/`!=` before the relational tier was reached,
        // corrupting the LHS into a Raw node (`a <`) and op into `=`. Merging
        // `=` into the relational tier (2-char ops first) fixes the split.
        for (src, expected_op) in [("a <= b", "<="), ("a >= b", ">="), ("a != b", "!=")] {
            match lower_expression(src) {
                Expr::Binary { op, lhs, rhs } => {
                    assert_eq!(op, expected_op, "op for {src:?}");
                    assert!(
                        matches!(*lhs, Expr::Name(_)),
                        "lhs of {src:?} must lower to a Name, not Raw: {lhs:?}"
                    );
                    assert!(
                        matches!(*rhs, Expr::Name(_)),
                        "rhs of {src:?} must lower to a Name: {rhs:?}"
                    );
                }
                other => panic!("{src:?} should lower to Binary, got {other:?}"),
            }
        }
    }

    #[test]
    fn unaffected_comparison_ops_still_split_correctly() {
        // The relational-tier merge must not regress the operators that
        // already worked: `=`, `<`, `>`, `<>`.
        for (src, expected_op) in [("a = b", "="), ("a < b", "<"), ("a > b", ">"), ("a <> b", "<>")]
        {
            match lower_expression(src) {
                Expr::Binary { op, lhs, rhs } => {
                    assert_eq!(op, expected_op, "op for {src:?}");
                    assert!(matches!(*lhs, Expr::Name(_)), "lhs of {src:?}: {lhs:?}");
                    assert!(matches!(*rhs, Expr::Name(_)), "rhs of {src:?}: {rhs:?}");
                }
                other => panic!("{src:?} should lower to Binary, got {other:?}"),
            }
        }
    }

    #[test]
    fn call_on_lhs_of_le_is_preserved_for_calls_edge() {
        // oracle-ajm2.10: with the mis-split, `compute_total(x)` on the LHS of
        // `<=` became a Raw node, dropped by collect_calls — a Calls-edge false
        // negative. After the fix the LHS lowers to a Call the extractor sees.
        match lower_expression("compute_total(x) <= 10") {
            Expr::Binary { op, lhs, .. } => {
                assert_eq!(op, "<=");
                assert!(
                    matches!(*lhs, Expr::Call { .. }),
                    "LHS must lower to a Call so the Calls-edge is emitted: {lhs:?}"
                );
            }
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn datetime_lone_quote_does_not_panic() {
        // oracle-ajm2.3: `DATE'` (and the TIMESTAMP/INTERVAL/whitespace
        // variants) used to slice `&trimmed[1..0]` and panic. With the
        // `len < 2` guard the recognizer declines and lowering falls through
        // to a non-panicking Expr (Raw for these unrecognised shapes).
        for src in ["DATE'", "TIMESTAMP '", "INTERVAL '", "DATE   '"] {
            let e = lower_expression(src);
            assert!(
                !matches!(e, Expr::DateTimeLit { .. }),
                "{src:?} is not a well-formed datetime literal: {e:?}"
            );
        }
        // A well-formed literal still parses.
        assert!(matches!(
            lower_expression("DATE'2020-01-01'"),
            Expr::DateTimeLit { .. }
        ));
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

    // ---- oracle-aqum.1: expression-lowering recursion-depth cap ----

    /// Iteratively measure the maximum nesting depth of an `Expr`
    /// tree without itself recursing (so the measurement cannot
    /// stack-overflow on the very trees we are guarding against).
    fn expr_depth(root: &Expr) -> usize {
        let mut max = 0usize;
        let mut stack: Vec<(&Expr, usize)> = vec![(root, 1)];
        while let Some((e, d)) = stack.pop() {
            if d > max {
                max = d;
            }
            match e {
                Expr::Binary { lhs, rhs, .. } => {
                    stack.push((lhs, d + 1));
                    stack.push((rhs, d + 1));
                }
                Expr::Unary { operand, .. } => stack.push((operand, d + 1)),
                Expr::Call { args, .. } => {
                    for a in args {
                        stack.push((a, d + 1));
                    }
                }
                _ => {}
            }
        }
        max
    }

    #[test]
    fn flat_binary_chain_left_fold_shape_preserved() {
        // The iterative left-fold must reproduce the same right-leaning
        // tree the old leftmost-split recursion produced:
        // `a OR b OR c` → OR(a, OR(b, c)).
        match lower_expression("a OR b OR c") {
            Expr::Binary { op, lhs, rhs } => {
                assert_eq!(op, "OR");
                assert!(matches!(*lhs, Expr::Name(_)), "outer lhs is `a`: {lhs:?}");
                match *rhs {
                    Expr::Binary {
                        op: ref iop,
                        ref lhs,
                        ref rhs,
                    } => {
                        assert_eq!(iop, "OR");
                        assert!(matches!(**lhs, Expr::Name(_)), "inner lhs is `b`: {lhs:?}");
                        assert!(matches!(**rhs, Expr::Name(_)), "inner rhs is `c`: {rhs:?}");
                    }
                    other => panic!("inner rhs should be Binary, got {other:?}"),
                }
            }
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn mixed_same_tier_ops_fold_in_source_order() {
        // `a - b + c` lives in the `[+, -]` tier; the leftmost-split
        // recursion produced `-(a, +(b, c))`. The fold must keep the
        // per-position operator, not collapse them to one symbol.
        match lower_expression("a - b + c") {
            Expr::Binary { op, rhs, .. } => {
                assert_eq!(op, "-", "outer op is the leftmost `-`");
                match *rhs {
                    Expr::Binary { op: iop, .. } => assert_eq!(iop, "+"),
                    other => panic!("inner should be `+` Binary, got {other:?}"),
                }
            }
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn wide_or_chain_does_not_stack_overflow_and_is_depth_bounded() {
        // oracle-aqum.1: a crafted single assignment RHS
        // `a OR a OR … OR a` with ~1,000,000 operands previously drove
        // `lower_expression` into linear-depth recursion (spine depth ==
        // operand count − 1) that overflowed the stack and aborted the
        // analyzer (SIGABRT) — an unrecoverable DoS on untrusted PL/SQL.
        // After the fix the chain is folded iteratively and the produced
        // tree is capped at `MAX_EXPR_DEPTH`, so neither lowering nor the
        // downstream tree-walk consumers can overflow.
        let n = 1_000_000usize;
        let mut chain = String::with_capacity(n * 5);
        for i in 0..n {
            if i > 0 {
                chain.push_str(" OR ");
            }
            chain.push('a');
        }
        let lowered = lower_expression(&chain);
        // Must be a Binary spine (well-formed prefix) terminating in an
        // honest depth-limit Raw, never a panic / abort.
        assert!(
            matches!(lowered, Expr::Binary { .. }),
            "wide chain should lower to a Binary spine: {lowered:?}"
        );
        let depth = expr_depth(&lowered);
        assert!(
            depth <= MAX_EXPR_DEPTH + 1,
            "produced tree depth {depth} must stay bounded by the cap \
             (MAX_EXPR_DEPTH={MAX_EXPR_DEPTH}); an unbounded spine would \
             overflow the downstream walkers"
        );
        // The deep tail must be surfaced honestly, not silently dropped.
        assert!(
            contains_depth_limit_raw(&lowered),
            "an over-deep chain must surface an ExprDepthLimit Raw (R13)"
        );
    }

    fn contains_depth_limit_raw(root: &Expr) -> bool {
        let mut stack: Vec<&Expr> = vec![root];
        while let Some(e) = stack.pop() {
            match e {
                Expr::Raw { reason, .. } if *reason == UnknownExprReason::ExprDepthLimit => {
                    return true;
                }
                Expr::Binary { lhs, rhs, .. } => {
                    stack.push(lhs);
                    stack.push(rhs);
                }
                Expr::Unary { operand, .. } => stack.push(operand),
                Expr::Call { args, .. } => stack.extend(args.iter()),
                _ => {}
            }
        }
        false
    }

    #[test]
    fn deeply_nested_parens_degrade_to_depth_limit_not_overflow() {
        // A pathological paren spine `((((…a…))))` also drives the
        // call/unary/binary recursion; the cap must backstop it.
        let depth = MAX_EXPR_DEPTH + 50;
        let mut s = String::new();
        for _ in 0..depth {
            s.push('(');
        }
        s.push_str("a OR a");
        for _ in 0..depth {
            s.push(')');
        }
        // Wrap so the outermost parens are not stripped to a bare expr;
        // a leading `NOT ` forces the unary recursion path too.
        let src = format!("NOT {s}");
        let lowered = lower_expression(&src);
        assert!(
            expr_depth(&lowered) <= MAX_EXPR_DEPTH + 1,
            "deep paren/unary spine must stay depth-bounded: {lowered:?}"
        );
    }

    #[test]
    fn short_chain_within_budget_keeps_all_operands() {
        // A chain comfortably under the cap must NOT degrade: every
        // operand stays a real Name and no ExprDepthLimit Raw appears.
        let n = 64usize;
        let chain = vec!["x"; n].join(" OR ");
        let lowered = lower_expression(&chain);
        assert!(
            !contains_depth_limit_raw(&lowered),
            "a {n}-operand chain is well within MAX_EXPR_DEPTH and must \
             not be truncated"
        );
        // Exactly n-1 OR nodes and n Name leaves.
        let mut names = 0usize;
        let mut bins = 0usize;
        let mut stack: Vec<&Expr> = vec![&lowered];
        while let Some(e) = stack.pop() {
            match e {
                Expr::Name(_) => names += 1,
                Expr::Binary { op, lhs, rhs } => {
                    assert_eq!(op, "OR");
                    bins += 1;
                    stack.push(lhs);
                    stack.push(rhs);
                }
                other => panic!("unexpected node {other:?}"),
            }
        }
        assert_eq!(names, n, "all operands preserved");
        assert_eq!(bins, n - 1, "one OR per gap");
    }
}
