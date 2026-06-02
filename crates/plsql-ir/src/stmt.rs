//! IR for PL/SQL statement bodies.
//!
//! Adds the [`Statement`] enum and a heuristic lowering pass that
//! turns a raw statement-body source slice into a sequence of IR
//! statements. The full AST→IR lowering will wire `lower_statement`
//! against the actual parser tree once (statement-
//! body lowering in the parser) lands. Until then, this module
//! ships:
//!
//! 1. The complete IR enum so downstream consumers (analysis
//!    passes, lineage, bindings) can program against a stable
//!    surface today.
//! 2. A line-shaped heuristic classifier used by the engine's
//!    source-only fallback path — sufficient for the lab corpus's
//!    common-case statements (assignment, control flow, raise,
//!    return, exit, null, EXECUTE IMMEDIATE, simple SQL).
//!
//! Both surfaces honour R13 (typed UnknownReason) by emitting
//! [`Statement::Unrecognized`] with a reason discriminant when the
//! recognizer cannot classify a line.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   recognised statement shapes (`IF / ELSIF / ELSE`, `LOOP`,
//!   `FOR i IN …`, `WHILE`, `RAISE`, `RETURN`, `EXECUTE
//!   IMMEDIATE`, SQL statements) match the PL/SQL Language
//!   Reference chapter on statements.
//! * `LOW-LEVEL-CATALOGS.md` — the supplied-package bucket
//!   anchors `DBMS_OUTPUT` / `DBMS_SCHEDULER` usage that may
//!   appear in EXECUTE IMMEDIATE bodies.

use serde::{Deserialize, Serialize};

/// One PL/SQL statement, in source order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Statement {
    /// `NULL;` — the PL/SQL no-op.
    Null,
    /// `target := expr;` — captures the LHS target name and the
    /// raw RHS expression text. Sub-expression lowering happens
    /// in a later pass.
    Assignment { target: String, rhs_text: String },
    /// `IF cond THEN … [ELSIF …] [ELSE …] END IF;`. We capture the
    /// condition text per arm + the body source slice; full body
    /// lowering re-enters `lower_statement_body` on each slice once
    /// the parser wires it.
    If {
        arms: Vec<IfArm>,
        else_body_text: Option<String>,
    },
    /// `LOOP … END LOOP;` (bare loop).
    BareLoop { body_text: String },
    /// `FOR <ident> IN <range> LOOP … END LOOP;` — captures the
    /// iterator name + the range text.
    ForLoop {
        iterator: String,
        range_text: String,
        body_text: String,
    },
    /// `WHILE cond LOOP … END LOOP;`.
    WhileLoop {
        cond_text: String,
        body_text: String,
    },
    /// `RAISE [exception_name];`.
    Raise { exception: Option<String> },
    /// `RETURN [expr];`.
    Return { value_text: Option<String> },
    /// `EXIT [WHEN cond];`.
    Exit { when_text: Option<String> },
    /// `EXECUTE IMMEDIATE 'sql' [USING binds] [INTO targets];`.
    /// The lowering captures the SQL literal verbatim plus a
    /// boolean for whether the call had bind variables.
    ExecuteImmediate {
        sql_literal: String,
        has_bind_variables: bool,
    },
    /// A SQL statement embedded in PL/SQL (`SELECT … INTO`,
    /// `INSERT`, `UPDATE`, `DELETE`, `MERGE`). The verb is
    /// captured plus the raw text so downstream lineage can walk
    /// the tables it touches.
    Sql { verb: SqlVerb, raw_text: String },
    /// Anonymous nested block — `[DECLARE …] BEGIN … END;` inside
    /// the surrounding body.
    NestedBlock { body_text: String },
    /// `COMMIT;` / `ROLLBACK [TO …];` / `SAVEPOINT …;` — captured
    /// as a single kind because the engine treats them uniformly
    /// for now.
    TransactionControl { verb: String },
    /// Statement the recognizer could not classify. The
    /// `unknown_reason` discriminant feeds R13 reporting so the
    /// engine never silently drops a line.
    Unrecognized {
        raw_text: String,
        unknown_reason: UnknownStatementReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfArm {
    pub cond_text: String,
    pub body_text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlVerb {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownStatementReason {
    /// The line did not match any recognised statement shape.
    UnrecognizedKeyword,
    /// The line started a block-shaped statement (e.g. `IF`,
    /// `LOOP`) but the recognizer could not find the matching
    /// terminator before the body ended.
    UnterminatedBlock,
    /// The line is a comment or a label and was not surfaced as
    /// a statement.
    NonStatement,
}

/// Lower a raw statement-body source slice (i.e. the bytes
/// between `BEGIN` and `END` of a routine) into a vector of
/// IR statements. The recognizer is line-shaped:
///
/// 1. Split on `;` keeping the terminator with each chunk.
/// 2. Trim whitespace + comments.
/// 3. Classify by leading keyword (case-insensitive).
///
/// The pass is intentionally conservative — anything it can't
/// confidently classify lands as `Statement::Unrecognized` with
/// `UnrecognizedKeyword` so downstream analysis sees the source
/// text rather than silently dropping it.
#[must_use]
pub fn lower_statement_body(source: &str) -> Vec<Statement> {
    let mut out: Vec<Statement> = Vec::new();
    for chunk in split_statements(source) {
        let stripped = strip_comments(&chunk.text).trim().to_string();
        if stripped.is_empty() {
            continue;
        }
        if chunk.unterminated {
            // R13: the splitter reached end-of-body with an open
            // block (`IF`/`LOOP`/`BEGIN`/`CASE` never matched its
            // terminator). Surface it as a typed diagnostic instead
            // of letting a downstream classifier silently mis-parse
            // a half-block.
            out.push(Statement::Unrecognized {
                raw_text: stripped,
                unknown_reason: UnknownStatementReason::UnterminatedBlock,
            });
            continue;
        }
        out.push(classify(&stripped));
    }
    out
}

/// One chunk produced by [`split_statements`] — the raw source text
/// plus whether the chunk was emitted because the body ended while a
/// block was still open (R13: the splitter never silently truncates).
struct StatementChunk {
    text: String,
    /// `true` when this chunk was a block opener (`IF`/`LOOP`/
    /// `BEGIN`/`CASE`) whose matching terminator was never found
    /// before end-of-body.
    unterminated: bool,
}

/// Split `source` on `;` honouring nested `BEGIN … END;` blocks
/// **and** matching `IF … END IF;` / `LOOP … END LOOP;` /
/// `CASE … END CASE;` so an inner semicolon doesn't tear apart a
/// control-flow body. The result preserves the trailing semicolon
/// (or end-keyword) on each chunk so downstream classifiers can see
/// it.
///
/// Depth is incremented on every block opener — `BEGIN`, `IF`,
/// `LOOP` (the keyword that introduces a bare / `FOR` / `WHILE`
/// loop), and `CASE` — and decremented on the matching terminator.
/// A bare `END` (block end) decrements; `END IF` / `END LOOP` /
/// `END CASE` also decrement (one per matching opener) — so the
/// three opener families stay balanced. `;` only splits at depth 0.
///
/// If end-of-body is reached with `depth > 0` the still-open chunk
/// is flagged `unterminated` so [`lower_statement_body`] can emit a
/// typed [`UnknownStatementReason::UnterminatedBlock`] (R13).
fn split_statements(source: &str) -> Vec<StatementChunk> {
    let mut out: Vec<StatementChunk> = Vec::new();
    let mut depth: i32 = 0;
    let mut buffer = String::new();
    let upper_chars: Vec<char> = source.chars().map(|c| c.to_ascii_uppercase()).collect();
    let mut i = 0;
    let chars: Vec<char> = source.chars().collect();
    while i < chars.len() {
        // Opaque spans — string literals (`'…''…'`), Oracle q-quotes
        // (`q'X…X'`), and comments — are copied through verbatim WITHOUT
        // scanning: a `;` inside a literal/comment is not a statement boundary,
        // and a `BEGIN`/`END` inside one must not move the block-depth counter.
        // (split_statements runs on the raw body before strip_comments, so it
        // must skip comments itself.)
        if let Some(end) = opaque_span_end(&chars, i) {
            for &ch in &chars[i..end] {
                buffer.push(ch);
            }
            i = end;
            continue;
        }
        let c = chars[i];
        // `END IF` / `END LOOP` / `END CASE` must be matched before a
        // bare `END`, otherwise the bare-`END` arm would consume the
        // `END` and the depth bookkeeping would double-count.
        if let Some(consumed) = consume_end_keyword(&upper_chars, i) {
            depth = (depth - 1).max(0);
            for &ch in chars.iter().skip(i).take(consumed) {
                buffer.push(ch);
            }
            i += consumed;
            continue;
        }
        // Track block depth by matching whole opener keywords.
        if let Some(consumed) = consume_any_keyword(&upper_chars, i, &["BEGIN", "IF", "LOOP", "CASE"])
        {
            depth += 1;
            for &ch in chars.iter().skip(i).take(consumed) {
                buffer.push(ch);
            }
            i += consumed;
            continue;
        }
        buffer.push(c);
        if c == ';' && depth == 0 {
            out.push(StatementChunk {
                text: std::mem::take(&mut buffer),
                unterminated: false,
            });
        }
        i += 1;
    }
    if !buffer.trim().is_empty() {
        out.push(StatementChunk {
            text: buffer,
            // depth > 0 ⇒ a block opener never met its terminator.
            unterminated: depth > 0,
        });
    }
    out
}

/// If a string literal begins at `chars[i]` — a single-quoted `'…''…'`
/// literal (doubled `''` escapes) or an Oracle alternative-quoting
/// `q'X…X'` / `nq'X…X'` literal — return the index one past its end. An
/// unterminated literal consumes to end-of-input so no `;`/keyword inside an
/// open literal is ever treated as a boundary. `prev` (the char before `i`)
/// guards the q-quote so an identifier ending in q/n (e.g. `acquire`) does not
/// false-trigger. Mirrors the canonical scanner in plsql-parser-antlr::recover.
fn string_literal_end(chars: &[char], i: usize) -> Option<usize> {
    let len = chars.len();
    if i >= len {
        return None;
    }
    // q-quote: optional leading n/N, then q/Q, then `'`, then the delimiter.
    let prev_is_ident = i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_');
    let q_at = if chars[i].eq_ignore_ascii_case(&'n') && i + 1 < len {
        i + 1
    } else {
        i
    };
    if !prev_is_ident
        && chars[q_at].eq_ignore_ascii_case(&'q')
        && q_at + 2 < len
        && chars[q_at + 1] == '\''
    {
        let open = chars[q_at + 2];
        let close = match open {
            '[' => ']',
            '(' => ')',
            '{' => '}',
            '<' => '>',
            other => other,
        };
        let mut j = q_at + 3;
        while j + 1 < len {
            if chars[j] == close && chars[j + 1] == '\'' {
                return Some(j + 2);
            }
            j += 1;
        }
        return Some(len); // unterminated → consume to EOF
    }
    // Single-quoted string literal with doubled-`''` escape.
    if chars[i] == '\'' {
        let mut j = i + 1;
        while j < len {
            if chars[j] == '\'' {
                if j + 1 < len && chars[j + 1] == '\'' {
                    j += 2; // escaped ''
                } else {
                    return Some(j + 1);
                }
            } else {
                j += 1;
            }
        }
        return Some(len); // unterminated → consume to EOF
    }
    None
}

/// If an opaque span — a string literal (see [`string_literal_end`]) or a
/// comment (`-- …` line / `/* … */` block) — begins at `chars[i]`, return the
/// index one past its end. Used by [`split_statements`] to copy such spans
/// through verbatim so their contents never affect `;`-splitting or block depth.
fn opaque_span_end(chars: &[char], i: usize) -> Option<usize> {
    if let Some(end) = string_literal_end(chars, i) {
        return Some(end);
    }
    let len = chars.len();
    // Line comment `-- …` (up to and including the newline).
    if chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
        let mut j = i + 2;
        while j < len && chars[j] != '\n' {
            j += 1;
        }
        if j < len {
            j += 1; // include the terminating newline
        }
        return Some(j);
    }
    // Block comment `/* … */`.
    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
        let mut j = i + 2;
        while j < len {
            if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
                return Some(j + 2);
            }
            j += 1;
        }
        return Some(len); // unterminated
    }
    None
}

/// Match a block terminator at `pos`: `END IF`, `END LOOP`,
/// `END CASE`, or a bare `END`. Returns the number of chars to
/// consume (covering the optional whitespace + sub-keyword) so the
/// caller can copy the whole terminator into the current chunk.
fn consume_end_keyword(chars: &[char], pos: usize) -> Option<usize> {
    let end = consume_keyword(chars, pos, "END")?;
    // Look past `END` and any run of whitespace for a sub-keyword.
    let mut j = pos + end;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    for sub in ["IF", "LOOP", "CASE"] {
        if let Some(sub_len) = consume_keyword(chars, j, sub) {
            return Some(j + sub_len - pos);
        }
    }
    // Bare `END` (terminates BEGIN…END).
    Some(end)
}

/// Match the first whole keyword from `keywords` at `pos`.
fn consume_any_keyword(chars: &[char], pos: usize, keywords: &[&str]) -> Option<usize> {
    keywords
        .iter()
        .find_map(|kw| consume_keyword(chars, pos, kw))
}

fn consume_keyword(chars: &[char], pos: usize, keyword: &str) -> Option<usize> {
    let kw: Vec<char> = keyword.chars().collect();
    if pos + kw.len() > chars.len() {
        return None;
    }
    for (j, k) in kw.iter().enumerate() {
        if chars[pos + j] != *k {
            return None;
        }
    }
    // Boundary check: the char immediately after must NOT be
    // alphanumeric / `_` / `$` / `#` and the char immediately
    // before must be whitespace / start of input / non-ident.
    if pos > 0 {
        let prev = chars[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == '_' || prev == '$' || prev == '#' {
            return None;
        }
    }
    if pos + kw.len() < chars.len() {
        let next = chars[pos + kw.len()];
        if next.is_ascii_alphanumeric() || next == '_' || next == '$' || next == '#' {
            return None;
        }
    }
    Some(kw.len())
}

fn strip_comments(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        // A string literal is opaque: copy it through verbatim so a `--` or
        // `/*` *inside* a quoted (or q-quoted) literal is never mistaken for a
        // comment and stripped — which would corrupt the statement's SQL text.
        if let Some(end) = string_literal_end(&chars, i) {
            for &ch in &chars[i..end] {
                out.push(ch);
            }
            i = end;
            continue;
        }
        let c = chars[i];
        // Line comment `-- …`: drop it but keep the newline (line structure).
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            if i < chars.len() {
                out.push('\n');
                i += 1;
            }
            continue;
        }
        // Block comment `/* … */`: drop it entirely.
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whole-word prefix test: `trimmed` begins with `keyword` AND the char
/// immediately after the keyword is a word boundary (not `[A-Za-z0-9_$#]`).
///
/// The keyword classifiers below all gated on a bare `starts_with`, so a
/// local whose name merely *starts* with a verb — `null_count`,
/// `return_val`, `update_stats`, `delete_flag`, `commit_seq`, … — was
/// swallowed by the wrong arm (e.g. `null_count := 5;` → `Statement::Null`,
/// `update_stats(p_id);` → `Statement::Sql{Update}`). The dropped statement
/// then vanished from `flow_intra::walk` (assignments) or minted a phantom
/// DML edge (calls), and any user-tainted value laundered through such a
/// local was never recorded — a taint fail-open. Requiring a trailing word
/// boundary makes the verb match only the real keyword. (oracle-rwjl.3)
fn starts_with_keyword(trimmed: &str, keyword: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix(keyword) else {
        return false;
    };
    match rest.chars().next() {
        None => true,
        Some(c) => !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#'),
    }
}

/// Byte offset of the first top-level `:=` in `text` — one that is NOT inside
/// a string literal (single-quoted or q-quoted). A top-level `:=` makes the
/// statement unambiguously an assignment, regardless of what verb its LHS
/// local happens to start with, so `classify` checks this BEFORE the keyword
/// classifiers. (oracle-rwjl.3)
fn top_level_assign_pos(text: &str) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    // Map char index -> byte offset so the returned position indexes `text`.
    let mut byte_off = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if let Some(end) = string_literal_end(&chars, i) {
            // Skip the opaque literal span verbatim.
            for &ch in &chars[i..end] {
                byte_off += ch.len_utf8();
            }
            i = end;
            continue;
        }
        if chars[i] == ':' && chars.get(i + 1) == Some(&'=') {
            return Some(byte_off);
        }
        byte_off += chars[i].len_utf8();
        i += 1;
    }
    None
}

fn classify(text: &str) -> Statement {
    let upper = text.to_ascii_uppercase();
    let trimmed = upper.trim();
    // BODY-INTRODUCING constructs are classified FIRST: an IF / LOOP / FOR /
    // WHILE / BEGIN / DECLARE statement legitimately contains a `:=` inside its
    // BODY (`IF c THEN v := p; END IF;`), so the top-level `:=` test below must
    // not run before them or it would swallow the whole construct. With the
    // word-boundary `starts_with_keyword`, `if_count := 1` / `for_idx := 0`
    // etc. do NOT match these — they fall through to the assignment test.
    // (oracle-rwjl.3)
    if starts_with_keyword(trimmed, "EXECUTE IMMEDIATE") {
        let after = &text[17..];
        let sql_literal = extract_quoted(after).unwrap_or_default();
        let has_bind_variables = after.to_ascii_uppercase().contains("USING ");
        return Statement::ExecuteImmediate {
            sql_literal,
            has_bind_variables,
        };
    }
    if starts_with_keyword(trimmed, "IF") {
        return classify_if(text);
    }
    if starts_with_keyword(trimmed, "LOOP")
        || starts_with_keyword(trimmed, "FOR")
        || starts_with_keyword(trimmed, "WHILE")
    {
        return classify_loop(text);
    }
    if starts_with_keyword(trimmed, "BEGIN") || starts_with_keyword(trimmed, "DECLARE") {
        return Statement::NestedBlock {
            body_text: text.to_string(),
        };
    }
    // A top-level `:=` (outside any string literal) makes the statement
    // unambiguously an assignment — emit it BEFORE the non-body verb
    // classifiers so a verb-prefixed LHS local (`return_val := …`,
    // `commit_count := …`) is never mis-swallowed. Slice the ORIGINAL `text`
    // so case is preserved. (oracle-rwjl.3)
    if let Some(pos) = top_level_assign_pos(text) {
        let lhs = &text[..pos];
        let rhs = &text[pos + 2..];
        return Statement::Assignment {
            target: lhs.trim().to_string(),
            rhs_text: rhs.trim().trim_end_matches(';').trim().to_string(),
        };
    }
    if starts_with_keyword(trimmed, "NULL") {
        return Statement::Null;
    }
    if starts_with_keyword(trimmed, "COMMIT")
        || starts_with_keyword(trimmed, "ROLLBACK")
        || starts_with_keyword(trimmed, "SAVEPOINT")
    {
        let verb = trimmed.split_whitespace().next().unwrap_or("").to_string();
        return Statement::TransactionControl { verb };
    }
    if starts_with_keyword(trimmed, "RAISE") {
        let rest = text[5..].trim().trim_end_matches(';').trim();
        let exception = if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        };
        return Statement::Raise { exception };
    }
    if starts_with_keyword(trimmed, "RETURN") {
        let rest = text[6..].trim().trim_end_matches(';').trim();
        let value_text = if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        };
        return Statement::Return { value_text };
    }
    if starts_with_keyword(trimmed, "EXIT") {
        let rest = text[4..].trim().trim_end_matches(';').trim();
        let when_text = rest
            .strip_prefix("WHEN")
            .or_else(|| rest.strip_prefix("when"))
            .map(|s| s.trim().to_string());
        return Statement::Exit { when_text };
    }
    for verb in ["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE"] {
        if starts_with_keyword(trimmed, verb) {
            let kind = match verb {
                "SELECT" => SqlVerb::Select,
                "INSERT" => SqlVerb::Insert,
                "UPDATE" => SqlVerb::Update,
                "DELETE" => SqlVerb::Delete,
                "MERGE" => SqlVerb::Merge,
                _ => unreachable!(),
            };
            return Statement::Sql {
                verb: kind,
                raw_text: text.to_string(),
            };
        }
    }
    Statement::Unrecognized {
        raw_text: text.to_string(),
        unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
    }
}

fn classify_if(text: &str) -> Statement {
    // Very small parser: split arms by `ELSIF` / `ELSE`, ending at
    // `END IF`. The result is structural — `body_text` retains the
    // raw inter-arm slice so a recursive `lower_statement_body`
    // can re-enter it later.
    let upper = text.to_ascii_uppercase();
    let end_pos = upper.rfind("END IF").unwrap_or(upper.len());
    let body = &text[..end_pos];
    // Skip the leading "IF " token.
    let after_if = &body[3..];
    let mut arms: Vec<IfArm> = Vec::new();
    let mut else_body_text: Option<String> = None;
    // `cond_start` points just past the keyword that introduces the
    // current arm's condition: `IF` for the first arm, `ELSIF` for
    // every subsequent one. Each loop iteration handles exactly ONE
    // arm — capture its condition, slice its body up to the next
    // ELSIF/ELSE, push a single IfArm — then advance.
    let mut cond_start = 0usize;
    while let Some(then_pos) = find_keyword(after_if, "THEN", cond_start) {
        let cond_text = after_if[cond_start..then_pos].trim().to_string();
        let body_start = then_pos + 4;
        let next_arm = find_any_keyword(after_if, &["ELSIF", "ELSE"], body_start);
        let body_end = next_arm.map_or(after_if.len(), |(p, _)| p);
        let body_text = after_if
            .get(body_start..body_end)
            .unwrap_or("")
            .trim()
            .to_string();
        arms.push(IfArm {
            cond_text,
            body_text,
        });
        match next_arm {
            // `ELSIF` — start the next arm's condition just past it.
            Some((pos, "ELSIF")) => cond_start = pos + 5,
            // `ELSE` — the trailing arm has no condition; its body
            // runs to the end (`END IF` was already trimmed off).
            Some((pos, _)) => {
                let else_text = after_if.get(pos + 4..).unwrap_or("").trim().to_string();
                else_body_text = Some(else_text);
                break;
            }
            None => break,
        }
    }
    Statement::If {
        arms,
        else_body_text,
    }
}

fn classify_loop(text: &str) -> Statement {
    let upper = text.to_ascii_uppercase();
    if upper.starts_with("FOR ") {
        let in_pos = find_keyword(text, "IN", 4);
        let loop_pos = find_keyword(text, "LOOP", in_pos.unwrap_or(0));
        let end_loop = upper.rfind("END LOOP").unwrap_or(text.len());
        if let (Some(in_p), Some(loop_p)) = (in_pos, loop_pos) {
            let iterator = text[4..in_p].trim().to_string();
            let range_text = text[in_p + 2..loop_p].trim().to_string();
            let body = text
                .get(loop_p + 4..end_loop)
                .unwrap_or("")
                .trim()
                .to_string();
            return Statement::ForLoop {
                iterator,
                range_text,
                body_text: body,
            };
        }
    }
    if upper.starts_with("WHILE ") {
        let loop_pos = find_keyword(text, "LOOP", 6);
        let end_loop = upper.rfind("END LOOP").unwrap_or(text.len());
        if let Some(loop_p) = loop_pos {
            let cond_text = text[6..loop_p].trim().to_string();
            let body = text
                .get(loop_p + 4..end_loop)
                .unwrap_or("")
                .trim()
                .to_string();
            return Statement::WhileLoop {
                cond_text,
                body_text: body,
            };
        }
    }
    let upper = text.to_ascii_uppercase();
    let body = if let Some(end_pos) = upper.rfind("END LOOP") {
        text[4..end_pos].trim().to_string()
    } else {
        text.trim_start_matches("LOOP")
            .trim_start_matches("loop")
            .trim()
            .to_string()
    };
    Statement::BareLoop { body_text: body }
}

fn extract_quoted(text: &str) -> Option<String> {
    let mut iter = text.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\'' {
            let mut buf = String::new();
            while let Some(nc) = iter.next() {
                if nc == '\'' {
                    // Oracle doubled-`''` escape: a `'` immediately followed by
                    // another `'` is a single literal `'`, not the end of the
                    // literal. Mirror `string_literal_end`'s handling so the
                    // captured SQL text is not truncated at the first inner
                    // escaped quote (e.g. EXECUTE IMMEDIATE 'SELECT ''x''…').
                    // (oracle-ajm2.20)
                    if iter.peek() == Some(&'\'') {
                        iter.next();
                        buf.push('\'');
                        continue;
                    }
                    return Some(buf);
                }
                buf.push(nc);
            }
            return Some(buf);
        }
    }
    None
}

fn find_keyword(text: &str, keyword: &str, start: usize) -> Option<usize> {
    let upper = text.to_ascii_uppercase();
    let kw_upper = keyword.to_ascii_uppercase();
    // Clamp to a char boundary so the slice `upper[search_from..]` never panics.
    let mut search_from = upper
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= start)
        .unwrap_or(upper.len());
    while search_from <= upper.len() {
        let Some(rel) = upper[search_from..].find(&kw_upper) else {
            break;
        };
        let abs = search_from + rel;
        if is_word_boundary(&upper, abs, abs + kw_upper.len()) {
            return Some(abs);
        }
        // Advance by the full char at `abs` so `search_from` always lands
        // on a char boundary. Advancing by 1 byte would panic on the next
        // slice if `abs` is inside a multi-byte UTF-8 code-point.
        search_from = abs + upper[abs..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn find_any_keyword(text: &str, keywords: &[&str], start: usize) -> Option<(usize, &'static str)> {
    static ELSIF: &str = "ELSIF";
    static ELSE: &str = "ELSE";
    let upper = text.to_ascii_uppercase();
    let mut best: Option<(usize, &'static str)> = None;
    for kw in keywords {
        let kw_upper = kw.to_ascii_uppercase();
        // Clamp to a char boundary so the slice `upper[search_from..]` never panics.
        let mut search_from = upper
            .char_indices()
            .map(|(i, _)| i)
            .find(|&i| i >= start)
            .unwrap_or(upper.len());
        while search_from <= upper.len() {
            let Some(rel) = upper[search_from..].find(&kw_upper) else {
                break;
            };
            let abs = search_from + rel;
            if is_word_boundary(&upper, abs, abs + kw_upper.len()) {
                let tag: &'static str = match kw_upper.as_str() {
                    "ELSIF" => ELSIF,
                    "ELSE" => ELSE,
                    _ => continue,
                };
                if best.is_none_or(|(b, _)| abs < b) {
                    best = Some((abs, tag));
                }
                break;
            }
            // Advance by the full char at `abs` so `search_from` always lands
            // on a char boundary. Advancing by 1 byte would panic on the next
            // slice if `abs` is inside a multi-byte UTF-8 code-point.
            search_from = abs + upper[abs..].chars().next().map_or(1, char::len_utf8);
        }
    }
    best
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();
    let prev_ok = start == 0 || {
        let b = bytes[start - 1];
        !(b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
    };
    let next_ok = end >= bytes.len() || {
        let b = bytes[end];
        !(b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
    };
    prev_ok && next_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_statement_classified() {
        let r = lower_statement_body("NULL;");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], Statement::Null);
    }

    #[test]
    fn assignment_captures_target_and_rhs() {
        let r = lower_statement_body("v_x := 42;");
        match &r[0] {
            Statement::Assignment { target, rhs_text } => {
                assert_eq!(target, "v_x");
                assert_eq!(rhs_text, "42");
            }
            other => panic!("expected Assignment, got {other:?}"),
        }
    }

    // oracle-rwjl.3: an assignment whose LHS local merely STARTS with a verb
    // keyword (`return_val`, `null_count`, `update_x`, `delete_flag`,
    // `commit_count`, `exit_code`, `raise_amount`, `select_idx`, …) must be
    // classified as `Statement::Assignment` — NOT swallowed by the keyword
    // classifier. The bare `starts_with("RETURN")` etc. used to misclassify
    // these, dropping the assignment from flow_intra::walk (a taint
    // fail-open) and minting phantom DML edges.
    #[test]
    fn verb_prefixed_assignment_is_an_assignment_not_a_keyword() {
        for (input, want_target, want_rhs) in [
            ("return_val := p_user;", "return_val", "p_user"),
            ("null_count := 5;", "null_count", "5"),
            ("update_x := p_user;", "update_x", "p_user"),
            ("delete_flag := 1;", "delete_flag", "1"),
            ("commit_count := 1;", "commit_count", "1"),
            ("exit_code := 0;", "exit_code", "0"),
            ("raise_amount := 100;", "raise_amount", "100"),
            ("select_idx := 7;", "select_idx", "7"),
            ("merge_key := p_user;", "merge_key", "p_user"),
            ("insert_seq := 3;", "insert_seq", "3"),
            ("savepoint_id := 2;", "savepoint_id", "2"),
            ("rollback_count := 9;", "rollback_count", "9"),
        ] {
            let r = lower_statement_body(input);
            match &r[0] {
                Statement::Assignment { target, rhs_text } => {
                    assert_eq!(target, want_target, "target for {input:?}");
                    assert_eq!(rhs_text, want_rhs, "rhs for {input:?}");
                }
                other => panic!("expected Assignment for {input:?}, got {other:?}"),
            }
        }
    }

    // oracle-rwjl.3: a real keyword statement (verb at a true word boundary)
    // must still classify as the keyword, not regress to Unrecognized.
    #[test]
    fn real_keyword_statements_still_classify() {
        assert_eq!(lower_statement_body("NULL;")[0], Statement::Null);
        assert!(matches!(
            lower_statement_body("RETURN 1;")[0],
            Statement::Return { .. }
        ));
        assert!(matches!(
            lower_statement_body("DELETE FROM t WHERE id = 1;")[0],
            Statement::Sql {
                verb: SqlVerb::Delete,
                ..
            }
        ));
        assert!(matches!(
            lower_statement_body("COMMIT;")[0],
            Statement::TransactionControl { .. }
        ));
    }

    #[test]
    fn raise_with_named_exception() {
        let r = lower_statement_body("RAISE no_data_found;");
        assert!(
            matches!(&r[0], Statement::Raise { exception } if exception.as_deref() == Some("no_data_found"))
        );
    }

    #[test]
    fn bare_raise_classified() {
        let r = lower_statement_body("RAISE;");
        assert!(matches!(&r[0], Statement::Raise { exception: None }));
    }

    #[test]
    fn return_with_value() {
        let r = lower_statement_body("RETURN v_sum;");
        assert!(
            matches!(&r[0], Statement::Return { value_text } if value_text.as_deref() == Some("v_sum"))
        );
    }

    #[test]
    fn return_without_value() {
        let r = lower_statement_body("RETURN;");
        assert!(matches!(&r[0], Statement::Return { value_text: None }));
    }

    #[test]
    fn exit_when_cond() {
        let r = lower_statement_body("EXIT WHEN i > 10;");
        assert!(
            matches!(&r[0], Statement::Exit { when_text } if when_text.as_deref() == Some("i > 10"))
        );
    }

    #[test]
    fn execute_immediate_with_binds_detected() {
        let r = lower_statement_body("EXECUTE IMMEDIATE 'UPDATE t SET a = :1' USING v_a;");
        match &r[0] {
            Statement::ExecuteImmediate {
                sql_literal,
                has_bind_variables,
            } => {
                assert_eq!(sql_literal, "UPDATE t SET a = :1");
                assert!(*has_bind_variables);
            }
            other => panic!("expected ExecuteImmediate, got {other:?}"),
        }
    }

    #[test]
    fn execute_immediate_honors_doubled_quote_escape() {
        // oracle-ajm2.20: `extract_quoted` returned at the first lone `'`,
        // truncating the captured literal at an inner doubled-`''` escape
        // (`'SELECT ''x'' FROM dual'` -> "SELECT "). Mirroring
        // `string_literal_end`'s `''` handling captures the full SQL with the
        // escapes un-doubled to single quotes.
        let r = lower_statement_body("EXECUTE IMMEDIATE 'SELECT ''x'' FROM dual';");
        match &r[0] {
            Statement::ExecuteImmediate { sql_literal, .. } => {
                assert_eq!(sql_literal, "SELECT 'x' FROM dual");
            }
            other => panic!("expected ExecuteImmediate, got {other:?}"),
        }
    }

    #[test]
    fn execute_immediate_without_binds() {
        let r = lower_statement_body("EXECUTE IMMEDIATE 'ALTER SESSION SET …';");
        if let Statement::ExecuteImmediate {
            has_bind_variables, ..
        } = &r[0]
        {
            assert!(!has_bind_variables);
        } else {
            panic!("{r:?}");
        }
    }

    #[test]
    fn sql_verbs_classified() {
        for (verb, src) in [
            ("SELECT", "SELECT * INTO v_row FROM t;"),
            ("INSERT", "INSERT INTO t VALUES (1);"),
            ("UPDATE", "UPDATE t SET x = 1;"),
            ("DELETE", "DELETE FROM t WHERE id = 1;"),
            (
                "MERGE",
                "MERGE INTO t USING s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET x = s.x;",
            ),
        ] {
            let r = lower_statement_body(src);
            assert!(matches!(&r[0], Statement::Sql { .. }), "{verb}: {r:?}");
        }
    }

    #[test]
    fn transaction_control_classified() {
        for src in ["COMMIT;", "ROLLBACK;", "SAVEPOINT s1;"] {
            let r = lower_statement_body(src);
            assert!(
                matches!(&r[0], Statement::TransactionControl { .. }),
                "{src}: {r:?}"
            );
        }
    }

    #[test]
    fn comment_only_chunks_dropped() {
        let r = lower_statement_body("-- header\n-- still here\nNULL;");
        assert_eq!(r.len(), 1);
        assert!(matches!(r[0], Statement::Null));
    }

    #[test]
    fn unrecognised_line_surfaces_with_typed_reason() {
        let r = lower_statement_body("xyz_unknown_keyword;");
        match &r[0] {
            Statement::Unrecognized {
                unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nested_block_passes_through() {
        let r = lower_statement_body("BEGIN NULL; END;");
        assert!(matches!(r[0], Statement::NestedBlock { .. }));
    }

    #[test]
    fn multiple_statements_split_at_top_level_semicolons() {
        let src = "v_x := 1; v_y := 2; NULL;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn semicolon_inside_string_literal_is_not_a_boundary() {
        // A `;` inside a single-quoted literal must not split the statement.
        let r = lower_statement_body("v_msg := 'a; b; c'; NULL;");
        assert_eq!(r.len(), 2, "the assignment (with its literal) + NULL");
    }

    #[test]
    fn block_keywords_inside_string_literal_do_not_move_depth() {
        // 'BEGIN'/'END' inside a literal are data, not block keywords — the
        // top-level `;` after the assignment must still split into two.
        let r = lower_statement_body("v_msg := 'BEGIN x END;'; v_y := 2;");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn q_quote_with_embedded_end_and_semicolon_is_opaque() {
        // q'{ … END; … }' is a single literal: neither a split nor a depth move.
        let r = lower_statement_body("v_sql := q'{SELECT 1; END;}'; NULL;");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn semicolon_inside_line_comment_is_not_a_boundary() {
        // split runs on the raw body before strip_comments, so it must skip
        // comments: the `;`s inside the trailing comment do not split.
        let r = lower_statement_body("v_x := 1; -- trailing; comment; here\nNULL;");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn semicolon_inside_block_comment_is_not_a_boundary() {
        let r = lower_statement_body("v_x := 1 /* a; b; c */ + 2; NULL;");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn comment_markers_inside_string_literal_are_preserved() {
        // strip_comments must NOT treat `--` / `/*` inside a literal as a comment;
        // stripping the literal's body would corrupt the statement's SQL text.
        let r = lower_statement_body("v_msg := 'keep -- this and /* this */ too';");
        assert_eq!(r.len(), 1);
        let dbg = format!("{:?}", r[0]);
        assert!(
            dbg.contains("keep -- this") && dbg.contains("/* this */"),
            "comment-like content inside the literal must survive strip_comments: {dbg}"
        );
    }

    #[test]
    fn for_loop_captures_iterator_and_range() {
        let r = lower_statement_body("FOR i IN 1..10 LOOP NULL; END LOOP;");
        match &r[0] {
            Statement::ForLoop {
                iterator,
                range_text,
                ..
            } => {
                assert_eq!(iterator, "i");
                assert_eq!(range_text, "1..10");
            }
            other => panic!("{other:?}"),
        }
    }

    // oracle-hbhm: split_statements must depth-track IF…END IF so an
    // inner `;` does not tear a multi-statement IF body into separate
    // top-level statements. Before the fix this produced 3 statements
    // (the leaked UPDATE + bogus `END IF;`) instead of one If.
    #[test]
    fn multi_statement_if_body_is_one_statement() {
        let src = "IF p_flag = 1 THEN \
                   INSERT INTO audit_log VALUES (1); \
                   UPDATE accounts SET bal = 0; \
                   END IF;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1, "IF body must not be torn apart: {r:?}");
        match &r[0] {
            Statement::If { arms, .. } => {
                assert_eq!(arms.len(), 1);
                // Both inner DML statements stay inside the arm body.
                assert!(arms[0].body_text.to_ascii_uppercase().contains("INSERT"));
                assert!(arms[0].body_text.to_ascii_uppercase().contains("UPDATE"));
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    // oracle-hbhm: split_statements must depth-track LOOP…END LOOP so
    // an inner `;` does not tear a multi-statement loop body apart.
    #[test]
    fn multi_statement_loop_body_is_one_statement() {
        let src = "FOR r IN 1..10 LOOP \
                   INSERT INTO dst VALUES (r); \
                   DELETE FROM stale WHERE id = r; \
                   END LOOP;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1, "LOOP body must not be torn apart: {r:?}");
        match &r[0] {
            Statement::ForLoop { body_text, .. } => {
                assert!(body_text.to_ascii_uppercase().contains("INSERT"));
                assert!(body_text.to_ascii_uppercase().contains("DELETE"));
            }
            other => panic!("expected ForLoop, got {other:?}"),
        }
    }

    // oracle-hbhm: a bare LOOP…END LOOP with internal `;` must also
    // survive splitting.
    #[test]
    fn multi_statement_bare_loop_body_is_one_statement() {
        let src = "LOOP v_x := 1; v_y := 2; EXIT WHEN v_x > 5; END LOOP;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1, "bare LOOP body must not be torn apart: {r:?}");
        assert!(matches!(r[0], Statement::BareLoop { .. }));
    }

    // oracle-hbhm: nested IF inside a LOOP — both openers must be
    // depth-tracked together.
    #[test]
    fn nested_if_inside_loop_stays_one_statement() {
        let src = "FOR i IN 1..3 LOOP \
                   IF i > 1 THEN do_a(i); ELSE do_b(i); END IF; \
                   log_iter(i); \
                   END LOOP;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1, "nested IF/LOOP must not be torn apart: {r:?}");
        assert!(matches!(r[0], Statement::ForLoop { .. }));
    }

    // oracle-hbhm: an unterminated IF (no `END IF`) must degrade with
    // a typed diagnostic, never silently (R13).
    #[test]
    fn unterminated_if_block_degrades_with_typed_reason() {
        let src = "IF a THEN foo(); bar();";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1, "unterminated IF stays one chunk: {r:?}");
        match &r[0] {
            Statement::Unrecognized {
                unknown_reason: UnknownStatementReason::UnterminatedBlock,
                ..
            } => {}
            other => panic!("expected Unrecognized/UnterminatedBlock, got {other:?}"),
        }
    }

    // oracle-ina8: classify_if must emit exactly one arm per ELSIF,
    // never phantom duplicate arms re-using the first condition.
    #[test]
    fn multi_elsif_if_has_no_phantom_arms() {
        let src = "IF a THEN NULL ELSIF b THEN NULL ELSIF c THEN NULL ELSE NULL END IF";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1);
        match &r[0] {
            Statement::If {
                arms,
                else_body_text,
            } => {
                let conds: Vec<&str> =
                    arms.iter().map(|a| a.cond_text.as_str()).collect();
                assert_eq!(
                    conds,
                    vec!["a", "b", "c"],
                    "expected exactly 3 arms a/b/c, got {arms:?}"
                );
                assert_eq!(else_body_text.as_deref(), Some("NULL"));
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    // oracle-ina8: a multi-ELSIF IF whose arms carry real bodies must
    // keep each body attached to the correct condition.
    #[test]
    fn multi_elsif_if_keeps_bodies_with_conditions() {
        let src = "IF a THEN s1; ELSIF b THEN s2; ELSIF c THEN s3; ELSE s4; END IF;";
        let r = lower_statement_body(src);
        assert_eq!(r.len(), 1);
        match &r[0] {
            Statement::If { arms, .. } => {
                assert_eq!(arms.len(), 3);
                assert_eq!(arms[0].cond_text, "a");
                assert_eq!(arms[0].body_text, "s1;");
                assert_eq!(arms[1].cond_text, "b");
                assert_eq!(arms[1].body_text, "s2;");
                assert_eq!(arms[2].cond_text, "c");
                assert_eq!(arms[2].body_text, "s3;");
            }
            other => panic!("expected If, got {other:?}"),
        }
    }
}
