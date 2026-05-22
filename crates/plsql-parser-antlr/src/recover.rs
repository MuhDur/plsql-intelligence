//! Error recovery at statement boundaries.
//!
//! When the parser encounters syntax it cannot classify, recovery skips
//! forward to the next statement boundary (`;` or `/` on its own line)
//! and emits a [`Diagnostic`] with the error span.  Parsing then
//! continues from the next statement.
//!
//! This satisfies plan §7.3: "Recover at statement boundaries (`;` and
//! `/` delimiters)", "Continue past a malformed PL/SQL block to parse
//! the next block in the same file", and "Surface a `Diagnostic` per
//! error with source span."

use plsql_core::{Diagnostic, FileId, Position, Severity, Span};

/// Result of a recovery attempt.
#[derive(Debug)]
pub struct RecoveryResult {
    /// Position in the source after the recovered statement boundary.
    pub recovered_at: usize,
    /// Diagnostic emitted for the recovered region, if any.
    pub diagnostic: Option<Diagnostic>,
}

/// Skip forward from `start` to the next statement boundary.
///
/// A statement boundary is defined as:
/// - A `;` character (most PL/SQL statement terminators)
/// - A `/` on its own line (SQL*Plus statement terminator)
///
/// Returns the position *after* the boundary character and an optional
/// diagnostic describing the recovered region.
///
/// If no boundary is found before EOF, returns `bytes.len()` and a
/// diagnostic covering the rest of the file.
pub fn recover_to_statement_boundary(
    bytes: &[u8],
    start: usize,
    file_id: FileId,
) -> RecoveryResult {
    let len = bytes.len();
    if start >= len {
        return RecoveryResult {
            recovered_at: len,
            diagnostic: None,
        };
    }

    let mut i = start;
    let mut depth = 0; // track BEGIN/END nesting

    while i < len {
        // Skip single-line comments
        if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Skip block comments
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Skip Oracle alternative-quoting (q-quote) literals:
        // q'X...X'  /  nq'X...X'  (case-insensitive, optional n/N).
        // The body may contain `;`, apostrophes, and newlines — none
        // of which are statement boundaries. Delimiter `X` pairs
        // ( )=, [ ]=, { }=, < >; any other char closes with itself.
        // Guarded so identifiers ending in q/n (e.g. `acquire`) do
        // not false-trigger: the q-quote must start a token.
        {
            let prev_is_ident =
                i > start && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let q_at = if (bytes[i] | 0x20) == b'n' && i + 1 < len {
                i + 1
            } else {
                i
            };
            if !prev_is_ident
                && (bytes[q_at] | 0x20) == b'q'
                && q_at + 2 < len
                && bytes[q_at + 1] == b'\''
            {
                let open = bytes[q_at + 2];
                let close = match open {
                    b'[' => b']',
                    b'(' => b')',
                    b'{' => b'}',
                    b'<' => b'>',
                    other => other,
                };
                let mut j = q_at + 3;
                let mut closed = false;
                while j + 1 < len {
                    if bytes[j] == close && bytes[j + 1] == b'\'' {
                        j += 2;
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                // Terminated → resume right after the closing `X'`.
                // Unterminated → consume to EOF (no spurious boundary
                // inside an open literal).
                i = if closed { j } else { len };
                continue;
            }
        }

        // Skip string literals (single-quoted)
        if bytes[i] == b'\'' {
            i += 1;
            while i < len {
                if bytes[i] == b'\'' {
                    if i + 1 < len && bytes[i + 1] == b'\'' {
                        i += 2; // escaped quote
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }

        // Track BEGIN/END nesting
        if matches_kw_at(bytes, i, b"BEGIN") {
            depth += 1;
            i += 5;
            continue;
        }
        if matches_kw_at(bytes, i, b"END") {
            if depth > 0 {
                depth -= 1;
            }
            i += 3;
            continue;
        }

        // Statement terminator: ;
        if bytes[i] == b';' {
            if depth == 0 {
                let recovered_at = i + 1;
                let diag = make_recovery_diagnostic(bytes, start, i, file_id);
                return RecoveryResult {
                    recovered_at,
                    diagnostic: Some(diag),
                };
            }
            i += 1;
            continue;
        }

        // SQL*Plus / terminator (newline + / + newline or EOF)
        if bytes[i] == b'/' {
            let is_sol = i == 0 || bytes[i - 1] == b'\n';
            let is_eol = i + 1 >= len || bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r';
            if is_sol && is_eol && depth == 0 {
                let recovered_at = i + 1;
                let diag = make_recovery_diagnostic(bytes, start, i, file_id);
                return RecoveryResult {
                    recovered_at,
                    diagnostic: Some(diag),
                };
            }
        }

        i += 1;
    }

    // EOF reached without finding a boundary
    let diag = make_recovery_diagnostic(bytes, start, len - 1, file_id);
    RecoveryResult {
        recovered_at: len,
        diagnostic: Some(diag),
    }
}

/// Case-insensitive keyword match at a byte position (word-boundary check).
fn matches_kw_at(bytes: &[u8], pos: usize, keyword: &[u8]) -> bool {
    let end = pos + keyword.len();
    if end > bytes.len() {
        return false;
    }
    let candidate = &bytes[pos..end];
    // Word boundary: next char must not be alphanumeric
    if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
        return false;
    }
    candidate.eq_ignore_ascii_case(keyword)
}

/// Create a diagnostic for the recovered region.
fn make_recovery_diagnostic(bytes: &[u8], start: usize, end: usize, file_id: FileId) -> Diagnostic {
    let recovered_text = String::from_utf8_lossy(&bytes[start..=end.min(bytes.len() - 1)]);
    let preview: String = recovered_text.chars().take(60).collect();

    let span = Span::new(
        file_id,
        Position::new(1, start as u32 + 1, start as u32),
        Position::new(1, end as u32 + 2, end as u32 + 1),
    );

    Diagnostic::new(
        "PARSE-RECOVERY-001",
        Severity::Warn,
        format!(
            "Recovered at statement boundary after unparseable text: {}{}",
            preview,
            if recovered_text.len() > 60 { "..." } else { "" }
        ),
    )
    .with_primary_span(span)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::recover_to_statement_boundary;
    use plsql_core::{FileId, Severity};

    fn fid() -> FileId {
        FileId::new(0)
    }

    #[test]
    fn recover_at_semicolon() {
        let src = b"garbage here; valid stuff";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 13); // after ;
        assert!(result.diagnostic.is_some());
        let diag = result.diagnostic.expect("recovery produced a diagnostic");
        assert_eq!(diag.code, "PARSE-RECOVERY-001");
        assert_eq!(diag.severity, Severity::Warn);
    }

    #[test]
    fn recover_at_slash_on_own_line() {
        let src = b"TYPE BODY foo AS\n  MEMBER FUNCTION x RETURN NUMBER IS BEGIN RETURN 1; END;\n/\nCREATE PROCEDURE p IS BEGIN NULL; END;\n";
        // Start at 0 — should recover at the / on its own line
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 74); // after the /
        assert!(result.diagnostic.is_some());
    }

    #[test]
    fn recover_skips_single_line_comments() {
        let src = b"bad -- this is a comment\n; rest";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 26); // after ;
    }

    #[test]
    fn recover_skips_block_comments() {
        let src = b"bad /* block\ncomment */; rest";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 24); // after ;
    }

    #[test]
    fn recover_skips_string_literals() {
        let src = b"bad 'hello; world'; rest";
        let result = recover_to_statement_boundary(src, 0, fid());
        // The ; inside the string should be skipped
        assert_eq!(result.recovered_at, 19); // after the second ;
    }

    #[test]
    fn recover_skips_escaped_quotes() {
        let src = b"bad 'it''s a ; test'; rest";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 21); // after ;
    }

    #[test]
    fn recover_respects_begin_end_depth() {
        let src = b"bad BEGIN NULL; END; rest";
        // BEGIN at depth 0 -> depth 1, ; doesn't terminate, END -> depth 0, ; terminates
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 20); // after the final ;
    }

    #[test]
    fn no_recovery_at_eof() {
        let src = b"no boundary here";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 16);
        assert!(result.diagnostic.is_some());
    }

    #[test]
    fn empty_input_no_diagnostic() {
        let src = b"";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(result.recovered_at, 0);
        assert!(result.diagnostic.is_none());
    }

    #[test]
    fn start_beyond_length() {
        let src = b"short";
        let result = recover_to_statement_boundary(src, 100, fid());
        assert_eq!(result.recovered_at, 5);
        assert!(result.diagnostic.is_none());
    }

    #[test]
    fn diagnostic_has_correct_span() {
        let src = b"BAD STUFF; next";
        let result = recover_to_statement_boundary(src, 0, fid());
        let diag = result.diagnostic.expect("recovery produced a diagnostic");
        let span = diag.primary_span.expect("diagnostic has a primary span");
        assert_eq!(span.start.offset, 0);
        assert_eq!(span.end.offset, 10); // recovered_at is after ;
    }

    #[test]
    fn diagnostic_preview_truncated() {
        let garbage = "A".repeat(100);
        let src = format!("{garbage}; rest");
        let result = recover_to_statement_boundary(src.as_bytes(), 0, fid());
        let diag = result.diagnostic.expect("recovery produced a diagnostic");
        assert!(diag.message.contains("..."));
        assert!(diag.message.len() < 200); // preview is truncated
    }

    #[test]
    fn recover_continues_parsing_after_recovery() {
        // The key test: malformed input followed by valid input
        let src = "NOT VALID PL/SQL AT ALL;\nCREATE OR REPLACE PACKAGE valid_pkg AS\n  PROCEDURE p;\nEND valid_pkg;\n";
        let bytes = src.as_bytes();

        // First: recover from the garbage
        let result1 = recover_to_statement_boundary(bytes, 0, fid());
        assert!(result1.diagnostic.is_some());
        let recovered_pos = result1.recovered_at;

        // Second: at recovered_pos we should see "CREATE"
        let rest = &bytes[recovered_pos..];
        let rest_str = std::str::from_utf8(rest).expect("recovered tail is valid UTF-8");
        assert!(
            rest_str.trim_start().starts_with("CREATE"),
            "After recovery, should see CREATE. Got: {:?}",
            &rest_str[..30.min(rest_str.len())]
        );
    }

    #[test]
    fn integration_with_lowerer() {
        // Simulate: parse source with a malformed decl, then a valid one
        let src = "BADDECL STUFF HERE;\nCREATE OR REPLACE PACKAGE good_pkg AS\n  PROCEDURE x;\nEND good_pkg;\n";

        // Step 1: Recovery scanner encounters non-CREATE text at pos=0
        // It should recover at the ; and emit a diagnostic
        let bytes = src.as_bytes();
        let recovery = recover_to_statement_boundary(bytes, 0, fid());
        assert!(recovery.diagnostic.is_some());
        assert_eq!(recovery.recovered_at, 19); // after ;

        // Step 2: From recovered position, scan for declarations
        let rest = &src[recovery.recovered_at..];
        let ast = crate::lower::lower_source(rest, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        assert!(matches!(
            ast.root.declarations[0],
            plsql_parser::ast::AstDecl::PackageSpec { ref name, .. } if name == "good_pkg"
        ));
    }

    #[test]
    fn recover_skips_q_quote_string_literals() {
        // Oracle alternative-quoting: q'<...>' is a string. A `;`
        // (and an embedded apostrophe) inside it must NOT be treated
        // as a statement boundary — recovery must land on the real
        // `;` after the closing `>'`.
        //                0123456789012345678 9
        let src = b"bad q'< a'b ; c >'; rest";
        let result = recover_to_statement_boundary(src, 0, fid());
        assert_eq!(
            result.recovered_at, 19,
            "the ; inside q'<...>' must be skipped; only the ; after >' terminates"
        );

        // Bracket-style delimiter q'[...]' likewise.
        let bracket = b"x q'[oops; ok]'; tail";
        let r2 = recover_to_statement_boundary(bracket, 0, fid());
        assert_eq!(r2.recovered_at, 16);

        // National-charset prefix nq'!...!' with same-char delimiter.
        let national = b"y nq'!a; b!'; z";
        let r3 = recover_to_statement_boundary(national, 0, fid());
        assert_eq!(r3.recovered_at, 13);
    }
}
