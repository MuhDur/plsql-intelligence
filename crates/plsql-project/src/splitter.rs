//! SQL*Plus-aware statement splitter (PLSQL-WS-008).
//!
//! The splitter walks a SQL*Plus script and produces a sequence of
//! [`Statement`]s. The lexer recognises:
//!
//! * Bare SQL statements terminated by `;` at end-of-line.
//! * PL/SQL blocks (`BEGIN … END;`, `CREATE … END;`) terminated by
//!   a lone `/` on its own line.
//! * SQL*Plus commands: `PROMPT`, `SET`, `SHOW`, `SPOOL`, `ACCEPT`
//!   — single-line directives ignored by the SQL engine but
//!   preserved as `StatementKind::SqlPlusCommand` so callers (e.g.
//!   linters) can flag them.
//! * SQL*Plus file inclusion: `@`, `@@`, `START`, `RUN` — all
//!   yield `StatementKind::Include` with the referenced path.
//! * Substitution variables (`&name`, `&&name`) — the splitter
//!   doesn't expand them but records that the statement uses one
//!   so downstream consumers know the bind list is incomplete.
//!
//! Line and column ranges are recorded against the original text
//! so the caller can map a statement back to its source location.
//!
//! ## /oracle evidence
//!
//! * `CLIENT-TOOLS-REFERENCE.md` SQL*Plus first-routing →
//!   `SQLPLUS-REFERENCE.md` for the canonical command list.
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference routing —
//!   the `/` terminator and the PL/SQL block shape are spelled
//!   out in the language reference chapter on anonymous blocks.

use serde::{Deserialize, Serialize};

/// One unit produced by the splitter. Every statement carries its
/// 1-based starting line and the raw substring so callers can
/// reproduce the exact source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub kind: StatementKind,
    /// 1-based starting line in the input.
    pub line_start: u32,
    /// 1-based ending line in the input (inclusive).
    pub line_end: u32,
    /// Raw source text of the statement including the terminator.
    pub raw: String,
    /// True when the body references `&name` or `&&name` — the
    /// caller's bind list is therefore incomplete until those are
    /// resolved.
    pub has_substitution_variable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatementKind {
    /// Generic SQL terminated by `;`. PL/SQL DDL blocks terminated
    /// by `/` use `PlSqlBlock` instead.
    Sql,
    /// PL/SQL block terminated by a lone `/`. Catches anonymous
    /// blocks (`BEGIN … END;`) and `CREATE OR REPLACE` DDL of
    /// stored programs.
    PlSqlBlock,
    /// SQL*Plus single-line directive (`SET`, `PROMPT`, `SHOW`,
    /// `SPOOL`, `ACCEPT`). The command's first token is captured
    /// for filtering.
    SqlPlusCommand { command: String },
    /// File-inclusion directive (`@`, `@@`, `START`, `RUN`). Path
    /// is captured verbatim — relative path resolution is the
    /// caller's responsibility.
    Include { path: String },
}

/// Split a SQL*Plus script into individual statements. The
/// algorithm is single-pass over lines and does not allocate
/// beyond the returned vector.
#[must_use]
pub fn split_script(text: &str) -> Vec<Statement> {
    let mut out: Vec<Statement> = Vec::new();
    let mut buffer = String::new();
    let mut buffer_start_line: u32 = 1;
    let mut current_line: u32 = 0;
    let mut in_plsql = false;
    // Cross-line lexical state so a `/` line or trailing `;`
    // *inside* a string literal / block comment never splits a
    // statement mid-content (PLSQL-PROJECT split_script bug).
    let mut in_string = false;
    let mut in_block_comment = false;

    for raw_line in text.split_inclusive('\n') {
        current_line += 1;
        let line_no_terminator = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = line_no_terminator.trim();
        let at_top_level_at_line_start = !in_string && !in_block_comment;

        // Pure-comment / blank lines outside a buffered block are
        // discarded but preserve the line counter.
        if buffer.is_empty() && (trimmed.is_empty() || trimmed.starts_with("--")) {
            continue;
        }

        // SQL*Plus directives are single-line — emit immediately
        // unless we are in the middle of building a multi-line
        // statement, in which case they get folded into the body
        // (a rare case but legal in practice when an editor pastes
        // a SET inside a PL/SQL block).
        if buffer.is_empty()
            && let Some(cmd) = recognise_sqlplus_command(trimmed)
        {
            out.push(Statement {
                kind: cmd,
                line_start: current_line,
                line_end: current_line,
                raw: line_no_terminator.to_string(),
                has_substitution_variable: contains_substitution_var(line_no_terminator),
            });
            continue;
        }

        if buffer.is_empty() {
            buffer_start_line = current_line;
        }

        // Lone `/` on a line ends a PL/SQL block (or terminates a
        // SQL statement when the user explicitly wants the
        // statement to execute even though it already ended in
        // `;`).
        if trimmed == "/" && at_top_level_at_line_start {
            // Drop the slash from the buffer — it's a terminator,
            // not statement bytes. Only a `/` at top level (not
            // inside an open string / block comment) terminates.
            in_string = false;
            in_block_comment = false;
            let kind = if in_plsql {
                StatementKind::PlSqlBlock
            } else {
                StatementKind::Sql
            };
            let raw = buffer.trim_end_matches('\n').to_string();
            out.push(Statement {
                kind,
                line_start: buffer_start_line,
                line_end: current_line,
                raw,
                has_substitution_variable: contains_substitution_var(&buffer),
            });
            buffer.clear();
            in_plsql = false;
            continue;
        }

        buffer.push_str(raw_line);

        // Advance cross-line lexical state over this line and
        // learn whether it ends with a *top-level* `;` (one that
        // is not inside a string literal or block comment).
        let ends_top_level_semi =
            consume_line(line_no_terminator, &mut in_string, &mut in_block_comment);

        // Detect entry into a PL/SQL block — `BEGIN`, `DECLARE`,
        // or a CREATE-FUNCTION-style header. Cheap heuristic
        // because we are not parsing yet.
        if looks_like_plsql_opener(line_no_terminator) {
            in_plsql = true;
        }

        // A SQL statement terminator (`;` at end of line) closes
        // out the buffer ONLY when we're not in PL/SQL mode (PL/SQL
        // uses `END;` mid-block and waits for `/`) AND the `;` is
        // genuinely at top level — never one buried in a
        // multi-line string literal or comment.
        if !in_plsql && ends_top_level_semi {
            let raw = buffer.trim_end_matches('\n').to_string();
            out.push(Statement {
                kind: StatementKind::Sql,
                line_start: buffer_start_line,
                line_end: current_line,
                raw,
                has_substitution_variable: contains_substitution_var(&buffer),
            });
            buffer.clear();
            in_string = false;
            in_block_comment = false;
        }
    }

    // Flush trailing buffer as best-effort. PL/SQL blocks left
    // unterminated end up as a PlSqlBlock anyway since `in_plsql`
    // is true.
    let buffer_trimmed = buffer.trim_end_matches('\n');
    if !buffer_trimmed.is_empty() {
        let kind = if in_plsql {
            StatementKind::PlSqlBlock
        } else {
            StatementKind::Sql
        };
        out.push(Statement {
            kind,
            line_start: buffer_start_line,
            line_end: current_line,
            raw: buffer_trimmed.to_string(),
            has_substitution_variable: contains_substitution_var(&buffer),
        });
    }

    out
}

fn recognise_sqlplus_command(trimmed: &str) -> Option<StatementKind> {
    let upper = trimmed.to_ascii_uppercase();

    // File-inclusion directives.
    if let Some(rest) = trimmed.strip_prefix("@@") {
        return Some(StatementKind::Include {
            path: rest.trim().to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("@") {
        return Some(StatementKind::Include {
            path: rest.trim().to_string(),
        });
    }
    // Match START / RUN against the upper-cased line, but slice
    // the path from the ORIGINAL line so we preserve the
    // operator's filename casing on case-sensitive filesystems.
    for verb in ["START ", "RUN "] {
        if upper.starts_with(verb) {
            let path = trimmed[verb.len()..].trim();
            return Some(StatementKind::Include {
                path: path.to_string(),
            });
        }
    }

    for cmd in [
        "SET", "PROMPT", "SHOW", "SPOOL", "ACCEPT", "DEFINE", "UNDEFINE", "EXIT",
    ] {
        if upper == cmd || upper.starts_with(&format!("{cmd} ")) {
            return Some(StatementKind::SqlPlusCommand {
                command: cmd.to_string(),
            });
        }
    }
    None
}

fn looks_like_plsql_opener(line: &str) -> bool {
    let trimmed = line.trim_start().to_ascii_uppercase();
    trimmed.starts_with("BEGIN")
        || trimmed.starts_with("DECLARE")
        || trimmed.starts_with("CREATE OR REPLACE PACKAGE")
        || trimmed.starts_with("CREATE PACKAGE")
        || trimmed.starts_with("CREATE OR REPLACE PROCEDURE")
        || trimmed.starts_with("CREATE PROCEDURE")
        || trimmed.starts_with("CREATE OR REPLACE FUNCTION")
        || trimmed.starts_with("CREATE FUNCTION")
        || trimmed.starts_with("CREATE OR REPLACE TRIGGER")
        || trimmed.starts_with("CREATE TRIGGER")
        || trimmed.starts_with("CREATE OR REPLACE TYPE BODY")
        || trimmed.starts_with("CREATE TYPE BODY")
}

fn contains_substitution_var(s: &str) -> bool {
    // SQL*Plus substitution: `&name`, `&&name`, optionally
    // terminated by `.`. Skip false positives like the AND
    // operator `&&` inside a quoted string by checking for an
    // alphabetic char following the `&`.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'&' {
            let mut j = i + 1;
            if bytes[j] == b'&' && j + 1 < bytes.len() {
                j += 1;
            }
            if bytes[j].is_ascii_alphabetic() {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Consume one line's bytes, updating cross-line `in_string` /
/// `in_block_comment` state, and report whether the line ends
/// (at top level) with a `;` terminator.
///
/// Handles Oracle single-quote strings (with `''` escaped-quote),
/// `--` line comments (rest of line ignored), and multi-line
/// `/* … */` block comments. This is the fix for the
/// boundary-corruption bug: a `;` or a lone-`/` *inside* a
/// string/comment must not split the statement.
fn consume_line(line: &str, in_string: &mut bool, in_block_comment: &mut bool) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    let mut last_significant: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if *in_block_comment {
            if c == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                *in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if *in_string {
            if c == b'\'' {
                if i + 1 < b.len() && b[i + 1] == b'\'' {
                    // Escaped quote ('') — still inside the string.
                    i += 2;
                    continue;
                }
                *in_string = false;
            }
            i += 1;
            continue;
        }
        // Top level.
        if c == b'-' && i + 1 < b.len() && b[i + 1] == b'-' {
            // Line comment — the rest of the line is inert and
            // cannot contain a terminator or change string state.
            break;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            *in_block_comment = true;
            i += 2;
            continue;
        }
        if c == b'\'' {
            *in_string = true;
            i += 1;
            continue;
        }
        if !c.is_ascii_whitespace() {
            last_significant = Some(c);
        }
        i += 1;
    }
    !*in_string && !*in_block_comment && last_significant == Some(b';')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semicolon_inside_multiline_string_does_not_split() {
        // The `;` on the middle line is string content; the only
        // real terminator is the `;` after the closing quote.
        let src = "INSERT INTO t VALUES ('alpha;\nbeta; still inside\ngamma');\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1, "must be ONE statement, got {s:?}");
        assert!(matches!(s[0].kind, StatementKind::Sql));
        assert_eq!(s[0].line_start, 1);
        assert_eq!(s[0].line_end, 3);
        assert!(s[0].raw.contains("beta; still inside"));
    }

    #[test]
    fn lone_slash_inside_multiline_string_does_not_split() {
        // A line that is just `/` but inside an open string is
        // string content, not a PL/SQL terminator.
        let src = "DECLARE v VARCHAR2(99) := 'a\n/\nb';\nBEGIN NULL; END;\n/\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1, "the inner `/` must not terminate: {s:?}");
        assert!(matches!(s[0].kind, StatementKind::PlSqlBlock));
        assert!(s[0].raw.contains("'a\n/\nb'"));
    }

    #[test]
    fn semicolon_inside_block_comment_does_not_split() {
        let src = "SELECT 1 /* not a;\nterminator / either */ FROM dual;\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1, "comment `;`/`/` must not split: {s:?}");
        assert!(matches!(s[0].kind, StatementKind::Sql));
    }

    #[test]
    fn escaped_quote_keeps_string_open_until_real_close() {
        // 'it''s; ok' is a single string containing  it's; ok .
        let src = "SELECT 'it''s; ok' FROM dual;\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1, "escaped '' must not end the string: {s:?}");
    }

    #[test]
    fn single_sql_statement_terminated_by_semicolon() {
        let s = split_script("SELECT 1 FROM dual;\n");
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, StatementKind::Sql));
        assert_eq!(s[0].line_start, 1);
        assert_eq!(s[0].line_end, 1);
    }

    #[test]
    fn plsql_block_terminated_by_slash() {
        let src = "BEGIN\n  NULL;\nEND;\n/\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1, "{s:?}");
        assert!(matches!(s[0].kind, StatementKind::PlSqlBlock));
        assert_eq!(s[0].line_start, 1);
        assert_eq!(s[0].line_end, 4);
    }

    #[test]
    fn create_package_body_recognised_as_plsql() {
        let src = "CREATE OR REPLACE PACKAGE BODY pkg AS\nBEGIN NULL; END;\n/\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, StatementKind::PlSqlBlock));
    }

    #[test]
    fn sqlplus_directives_emit_their_command_token() {
        let src = "SET ECHO ON\nPROMPT Hello\nSHOW USER\nSPOOL out.log\n";
        let s = split_script(src);
        assert_eq!(s.len(), 4);
        for stmt in &s {
            assert!(
                matches!(&stmt.kind, StatementKind::SqlPlusCommand { command } if ["SET","PROMPT","SHOW","SPOOL"].contains(&command.as_str())),
                "{stmt:?}"
            );
        }
    }

    #[test]
    fn at_and_double_at_emit_include() {
        let src = "@foo.sql\n@@bar.sql\nSTART baz.sql\nRUN qux.sql\n";
        let s = split_script(src);
        assert_eq!(s.len(), 4);
        let paths: Vec<&str> = s
            .iter()
            .filter_map(|st| match &st.kind {
                StatementKind::Include { path } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(paths, vec!["foo.sql", "bar.sql", "baz.sql", "qux.sql"]);
    }

    #[test]
    fn substitution_variable_is_detected() {
        let s = split_script("SELECT &dept FROM dual;\n");
        assert_eq!(s.len(), 1);
        assert!(s[0].has_substitution_variable);
    }

    #[test]
    fn double_amp_substitution_also_detected() {
        let s = split_script("SELECT &&dept FROM dual;\n");
        assert_eq!(s.len(), 1);
        assert!(s[0].has_substitution_variable);
    }

    #[test]
    fn comment_only_lines_dropped() {
        let src = "-- header\n-- copyright\nSELECT 1 FROM dual;\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].line_start, 3);
    }

    #[test]
    fn multiple_statements_in_a_row_each_emit_independently() {
        let src = "SELECT 1 FROM dual;\nSELECT 2 FROM dual;\n";
        let s = split_script(src);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].line_start, 1);
        assert_eq!(s[1].line_start, 2);
    }

    #[test]
    fn plsql_block_with_internal_semicolons_does_not_split() {
        let src = "BEGIN\n  INSERT INTO x VALUES (1);\n  UPDATE x SET a = 2;\nEND;\n/\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, StatementKind::PlSqlBlock));
    }

    #[test]
    fn unterminated_buffer_flushed_at_end() {
        let src = "BEGIN\n  NULL;\nEND;\n"; // no trailing `/`
        let s = split_script(src);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, StatementKind::PlSqlBlock));
    }

    #[test]
    fn pure_text_without_terminator_buffered_as_sql() {
        let src = "SELECT 1 FROM dual\n";
        let s = split_script(src);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, StatementKind::Sql));
    }
}
