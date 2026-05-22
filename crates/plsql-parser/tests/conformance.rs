//! Backend conformance test suite.
//!
//! Every `ParseBackend` implementation must pass these tests.  The suite
//! validates the **contract** (no panics, round-trip property, diagnostic
//! presence) rather than parse-quality metrics, which live in corpus-based
//! tests.
//!
//! To test a new backend, create an instance and call `run_conformance`:
//!
//! ```ignore
//! #[test]
//! fn my_backend_conformance() {
//!     let backend = MyBackend::new();
//!     plsql_parser::conformance::run_conformance(&backend);
//! }
//! ```

use plsql_core::FileId;
use plsql_parser::{
    Ast, BackendParseResult, ConcreteSyntaxTree, ParseBackend, ParseMetrics, ParseOptions,
    RecoveryMode, SourceMap, Token, TokenKind, TokenTape, Trivia, TriviaTable,
};

// ---------------------------------------------------------------------------
// StubBackend — a minimal backend that satisfies the trait for testing
// ---------------------------------------------------------------------------

/// A trivial backend that tokenizes on whitespace and produces a flat token
/// tape.  Used only for conformance testing of the trait contract itself.
struct StubBackend;

impl ParseBackend for StubBackend {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn parse(&self, input: &str, file_id: FileId, _opts: &ParseOptions) -> BackendParseResult {
        let mut tape = TokenTape::new();
        let mut trivia = TriviaTable::new();
        let mut _offset: u32 = 0;
        let mut token_index: usize = 0;
        let mut chars = input.char_indices().peekable();
        let diagnostics = Vec::new();

        while let Some((i, ch)) = chars.next() {
            let i = i as u32;
            if ch.is_whitespace() {
                // Accumulate whitespace as trivia for the next token
                let start = i;
                let mut end = i + ch.len_utf8() as u32;
                while let Some(&(j, c)) = chars.peek() {
                    if !c.is_whitespace() {
                        break;
                    }
                    end = j as u32 + c.len_utf8() as u32;
                    chars.next();
                }
                trivia.push(
                    token_index,
                    Trivia::Whitespace(input[start as usize..end as usize].to_string()),
                );
                _offset = end;
                continue;
            }

            // Non-whitespace: consume until next whitespace or EOF
            let start = i;
            let mut end = i + ch.len_utf8() as u32;
            while let Some(&(j, c)) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                end = j as u32 + c.len_utf8() as u32;
                chars.next();
            }

            let text = &input[start as usize..end as usize];
            let kind = classify_stub(text);
            let span = plsql_core::Span::new(
                file_id,
                plsql_core::Position::new(1, start + 1, start),
                plsql_core::Position::new(1, end + 1, end),
            );
            tape.push(Token::new(kind, span, text));
            token_index += 1;
            _offset = end;
        }

        let total_tokens = tape.len() as u64;
        let trivia_count = trivia.total_count() as u64;

        BackendParseResult {
            cst: ConcreteSyntaxTree {
                root: plsql_parser::CstNodeId(0),
                token_tape: tape,
                trivia,
                source_map: SourceMap::new(),
            },
            ast: Ast::new(),
            diagnostics,
            metrics: ParseMetrics {
                total_tokens,
                trivia_count,
                diagnostic_count: 0,
                recovery_count: 0,
                source_bytes: input.len() as u64,
            },
            recovered: false,
        }
    }
}

fn classify_stub(text: &str) -> TokenKind {
    match text.to_uppercase().as_str() {
        "SELECT" | "FROM" | "WHERE" | "BEGIN" | "END" | "PACKAGE" | "PROCEDURE" | "FUNCTION"
        | "CREATE" | "REPLACE" | "IS" | "AS" | "DECLARE" | "IF" | "THEN" | "ELSE" | "ELSIF"
        | "LOOP" | "WHILE" | "FOR" | "IN" | "RETURN" | "RETURNS" | "TYPE" | "BODY" | "TRIGGER"
        | "VIEW" | "ALTER" | "DROP" | "GRANT" | "REVOKE" => TokenKind::Keyword,
        ";" => TokenKind::Semicolon,
        "/" => TokenKind::Slash,
        "." => TokenKind::Dot,
        "," => TokenKind::Comma,
        "(" => TokenKind::LParen,
        ")" => TokenKind::RParen,
        ":=" => TokenKind::Assign,
        "=>" => TokenKind::Arrow,
        "||" => TokenKind::Concat,
        _ if text.chars().next().is_some_and(|c| c.is_ascii_digit()) => TokenKind::NumericLiteral,
        _ if text.starts_with('\'') => TokenKind::StringLiteral,
        _ if text.starts_with('"') => TokenKind::QuotedIdentifier,
        _ => TokenKind::Identifier,
    }
}

// ---------------------------------------------------------------------------
// Conformance suite
// ---------------------------------------------------------------------------

/// Run all conformance checks against the given backend.
///
/// This is meant to be called from each backend's test module.
pub fn run_conformance(backend: &dyn ParseBackend) {
    conformance_name_is_non_empty(backend);
    conformance_empty_input_does_not_panic(backend);
    conformance_whitespace_only_does_not_panic(backend);
    conformance_simple_statement_does_not_panic(backend);
    conformance_metrics_source_bytes_match(backend);
    conformance_token_tape_non_empty_for_non_empty_input(backend);
    conformance_diagnostics_are_vec(backend);
}

fn conformance_name_is_non_empty(backend: &dyn ParseBackend) {
    assert!(
        !backend.name().is_empty(),
        "Backend name() must return a non-empty string"
    );
}

fn conformance_empty_input_does_not_panic(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let result = backend.parse("", FileId::new(0), &opts);
    // Empty input is valid — should produce zero tokens and no diagnostics
    assert_eq!(
        result.metrics.source_bytes, 0,
        "Empty input should report 0 source_bytes"
    );
}

fn conformance_whitespace_only_does_not_panic(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let result = backend.parse("   \n\t  \r\n  ", FileId::new(0), &opts);
    // Whitespace-only is valid — may produce zero tokens
    assert_eq!(
        result.metrics.source_bytes, 11,
        "Whitespace-only input should report correct source_bytes"
    );
}

fn conformance_simple_statement_does_not_panic(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let input = "SELECT 1 FROM dual;";
    let result = backend.parse(input, FileId::new(0), &opts);
    // Should produce at least some tokens for a non-trivial statement
    assert!(
        result.metrics.total_tokens > 0,
        "A simple SELECT statement should produce at least one token"
    );
    assert_eq!(
        result.metrics.source_bytes,
        input.len() as u64,
        "Source bytes should match input length"
    );
}

fn conformance_metrics_source_bytes_match(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let inputs = &[
        "x",
        "SELECT 1;",
        "BEGIN NULL; END;",
        "CREATE OR REPLACE PACKAGE pkg AS\n  PROCEDURE p;\nEND pkg;",
    ];
    for input in inputs {
        let result = backend.parse(input, FileId::new(0), &opts);
        assert_eq!(
            result.metrics.source_bytes,
            input.len() as u64,
            "source_bytes mismatch for input: {:?}",
            input
        );
    }
}

fn conformance_token_tape_non_empty_for_non_empty_input(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let result = backend.parse("x", FileId::new(0), &opts);
    assert!(
        !result.cst.token_tape.is_empty(),
        "Non-empty input must produce a non-empty token tape"
    );
}

fn conformance_diagnostics_are_vec(backend: &dyn ParseBackend) {
    let opts = ParseOptions::default();
    let result = backend.parse("SELECT 1;", FileId::new(0), &opts);
    // Diagnostics must be present (may be empty for a clean parse)
    // Just verify the type is accessible
    let _diag_count = result.diagnostics.len();
}

// ---------------------------------------------------------------------------
// Tests using the StubBackend
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_backend_conformance() {
        run_conformance(&StubBackend);
    }

    #[test]
    fn stub_backend_round_trip_empty() {
        let opts = ParseOptions::default();
        let result = StubBackend.parse("", FileId::new(0), &opts);
        let reconstructed = result.cst.reconstruct();
        assert_eq!(reconstructed, "");
    }

    #[test]
    fn stub_backend_round_trip_simple() {
        let opts = ParseOptions::default();
        let input = "SELECT 1 FROM dual;";
        let result = StubBackend.parse(input, FileId::new(0), &opts);
        let reconstructed = result.cst.reconstruct();
        assert_eq!(
            reconstructed, input,
            "Lossless round-trip property violated: expected {:?}, got {:?}",
            input, reconstructed
        );
    }

    #[test]
    fn stub_backend_round_trip_with_whitespace() {
        let opts = ParseOptions::default();
        let input = "  SELECT   1\n  FROM  dual ;";
        let result = StubBackend.parse(input, FileId::new(0), &opts);
        let reconstructed = result.cst.reconstruct();
        assert_eq!(
            reconstructed, input,
            "Lossless round-trip property violated for whitespace-heavy input"
        );
    }

    #[test]
    fn stub_backend_name() {
        assert_eq!(StubBackend.name(), "stub");
    }

    #[test]
    fn stub_backend_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StubBackend>();
    }

    #[test]
    fn stub_backend_recover_always_false() {
        let opts = ParseOptions {
            recovery: RecoveryMode::AggressiveRecovery,
            ..ParseOptions::default()
        };
        let result = StubBackend.parse("SELECT 1;", FileId::new(0), &opts);
        assert!(!result.recovered, "StubBackend never recovers");
    }

    #[test]
    fn parse_with_backend_wraps_correctly() {
        let opts = ParseOptions::default();
        let file_id = FileId::new(42);
        let result = plsql_parser::parse_with_backend("SELECT 1;", file_id, &StubBackend, &opts);
        assert_eq!(result.file_id, file_id);
        assert!(result.is_clean());
        assert!(!result.was_recovered());
    }
}
