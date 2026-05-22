//! Round-trip proptest for the token tape.
//!
//! The lossless contract (plan §7.4): for any file that lexes successfully,
//! `reconstruct(token_tape(input)) == input` byte-for-byte.
//!
//! This module uses `proptest` to generate random plausible PL/SQL source
//! strings, tokenize them into a [`TokenTape`] + [`TriviaTable`], and
//! verify that reconstruction recovers the original source exactly.

use plsql_core::{FileId, Position, Span};
use plsql_parser::tokens::{Token, TokenKind, TokenTape, Trivia, TriviaTable};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// A PL/SQL keyword.
fn keyword_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("SELECT"),
        Just("FROM"),
        Just("WHERE"),
        Just("BEGIN"),
        Just("END"),
        Just("IF"),
        Just("THEN"),
        Just("ELSE"),
        Just("LOOP"),
        Just("CREATE"),
        Just("REPLACE"),
        Just("PACKAGE"),
        Just("PROCEDURE"),
        Just("FUNCTION"),
        Just("RETURN"),
        Just("IS"),
        Just("AS"),
        Just("DECLARE"),
        Just("TYPE"),
        Just("TABLE"),
        Just("NUMBER"),
        Just("VARCHAR2"),
        Just("BOOLEAN"),
    ]
}

/// A simple identifier: letter followed by alphanumeric/underscore.
fn identifier_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,15}"
}

/// A punctuation token.
fn punct_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just(";"),
        Just("("),
        Just(")"),
        Just(","),
        Just("."),
        Just(":="),
        Just("=>"),
        Just("||"),
        Just("+"),
        Just("-"),
        Just("*"),
        Just("="),
        Just("<"),
        Just(">"),
        Just("/"),
    ]
}

/// A whitespace character.
fn ws_char_strategy() -> impl Strategy<Value = char> {
    prop_oneof![Just(' '), Just('\t'), Just('\n'), Just('\r')]
}

/// Whitespace: one or more whitespace characters.
fn whitespace_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(ws_char_strategy(), 1..4).prop_map(|v| v.into_iter().collect())
}

/// A comment.
fn comment_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        ("--", "[^\n]{0,30}").prop_map(|(p, c)| format!("{p}{c}")),
        ("/*", "[^*]{0,30}").prop_map(|(o, c)| format!("{o}{c}*/")),
    ]
}

/// A single "token element" — either a keyword, identifier, or punctuation.
fn token_element_strategy() -> impl Strategy<Value = (String, TokenKind)> {
    prop_oneof![
        keyword_strategy().prop_map(|k| (k.to_string(), TokenKind::Keyword)),
        identifier_strategy().prop_map(|i| (i, TokenKind::Identifier)),
        punct_strategy().prop_map(|p| (p.to_string(), TokenKind::Operator)),
    ]
}

/// A "trivia element" — whitespace or comment.
fn trivia_element_strategy() -> impl Strategy<Value = Trivia> {
    prop_oneof![
        whitespace_strategy().prop_map(Trivia::Whitespace),
        comment_strategy().prop_map(Trivia::LineComment),
    ]
}

/// Generate a plausible PL/SQL source string as a sequence of
/// (trivia?, token, trivia?, token, ...) elements.
fn source_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        (
            prop::option::of(trivia_element_strategy()),
            token_element_strategy(),
            prop::option::of(trivia_element_strategy()),
        ),
        1..20,
    )
    .prop_map(|elements| {
        let mut out = String::new();
        for (pre, (text, _kind), post) in elements {
            if let Some(t) = pre {
                push_trivia(&mut out, &t);
            }
            out.push_str(&text);
            if let Some(t) = post {
                push_trivia(&mut out, &t);
            }
        }
        out
    })
}

fn push_trivia(out: &mut String, t: &Trivia) {
    match t {
        Trivia::Whitespace(s) | Trivia::LineComment(s) | Trivia::BlockComment(s) => {
            out.push_str(s);
        }
    }
}

// ---------------------------------------------------------------------------
// Simple whitespace-boundary lexer
// ---------------------------------------------------------------------------

/// A minimal lexer that splits source text at whitespace boundaries.
///
/// Every non-whitespace run becomes a token; every whitespace run becomes
/// trivia.  This lexer is intentionally simple — it exists to test the
/// TokenTape round-trip property, not to parse PL/SQL correctly.
fn simple_lex(source: &str) -> (TokenTape, TriviaTable) {
    let mut tape = TokenTape::new();
    let mut trivia = TriviaTable::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;
    let mut token_index = 0;

    while pos < len {
        // Collect whitespace as trivia
        if bytes[pos].is_ascii_whitespace() {
            let start = pos;
            while pos < len && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let text = &source[start..pos];
            trivia.push(token_index, Trivia::Whitespace(text.to_string()));
            continue;
        }

        // Collect non-whitespace as a token
        let start = pos;
        while pos < len && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let text = &source[start..pos];
        let kind = classify_token(text);
        let span = Span::new(
            FileId::new(0),
            Position::new(1, start as u32 + 1, start as u32),
            Position::new(1, pos as u32 + 1, pos as u32),
        );
        tape.push(Token::new(kind, span, text));
        token_index += 1;
    }

    (tape, trivia)
}

fn classify_token(text: &str) -> TokenKind {
    match text.to_uppercase().as_str() {
        "SELECT" | "FROM" | "WHERE" | "BEGIN" | "END" | "IF" | "THEN" | "ELSE" | "LOOP"
        | "CREATE" | "REPLACE" | "PACKAGE" | "PROCEDURE" | "FUNCTION" | "RETURN" | "IS" | "AS"
        | "DECLARE" | "TYPE" | "TABLE" | "NUMBER" | "VARCHAR2" | "BOOLEAN" => TokenKind::Keyword,
        ";" => TokenKind::Semicolon,
        "/" => TokenKind::Slash,
        "." => TokenKind::Dot,
        "," => TokenKind::Comma,
        "(" => TokenKind::LParen,
        ")" => TokenKind::RParen,
        ":=" => TokenKind::Assign,
        "=>" => TokenKind::Arrow,
        "||" => TokenKind::Concat,
        _ => TokenKind::Identifier,
    }
}

// ---------------------------------------------------------------------------
// Proptest
// ---------------------------------------------------------------------------

proptest! {
    /// The core lossless round-trip property.
    ///
    /// For any source text that our simple lexer can tokenize,
    /// `reconstruct(token_tape, trivia) == source` byte-for-byte.
    #[test]
    fn round_trip_reconstruct_equals_source(source in source_strategy()) {
        let (tape, trivia) = simple_lex(&source);
        let reconstructed = tape.reconstruct(&trivia);
        prop_assert_eq!(
            &reconstructed, &source,
            "\nRound-trip failed.\nOriginal:      {:?}\nReconstructed: {:?}\nTokens: {}",
            source, reconstructed, tape.len()
        );
    }

    /// Property: token tape length is always <= source length (each token
    /// is at least 1 byte).
    #[test]
    fn token_count_bounded_by_source_length(source in source_strategy()) {
        let (tape, _trivia) = simple_lex(&source);
        prop_assert!(
            tape.len() <= source.len(),
            "Token count {} exceeds source length {}",
            tape.len(), source.len()
        );
    }

    /// Property: empty source produces empty tape and round-trips.
    #[test]
    fn empty_source_produces_empty_tape(_seed in 0u32..1) {
        let (tape, trivia) = simple_lex("");
        prop_assert!(tape.is_empty());
        prop_assert_eq!(trivia.total_count(), 0);
        prop_assert_eq!(tape.reconstruct(&trivia), "");
    }

    /// Property: single non-whitespace character always round-trips.
    #[test]
    fn single_char_round_trips(ch in "[a-zA-Z0-9;.,()+\\-*/]") {
        let (tape, trivia) = simple_lex(&ch);
        prop_assert_eq!(tape.reconstruct(&trivia), ch);
    }

    /// Property: whitespace-only source always round-trips.
    #[test]
    fn whitespace_only_round_trips(ws in "[ \t\n\r]+") {
        let (tape, trivia) = simple_lex(&ws);
        prop_assert_eq!(tape.reconstruct(&trivia), ws);
    }

    /// Property: trailing whitespace is preserved.
    #[test]
    fn trailing_whitespace_preserved(
        tokens in prop::collection::vec("[a-zA-Z]+", 1..5),
        trailing in "[ \t\n]+"
    ) {
        let mut source = tokens.join(" ");
        source.push_str(&trailing);
        let (tape, trivia) = simple_lex(&source);
        prop_assert_eq!(tape.reconstruct(&trivia), source);
    }

    /// Property: leading whitespace is preserved.
    #[test]
    fn leading_whitespace_preserved(
        leading in "[ \t\n]+",
        tokens in prop::collection::vec("[a-zA-Z]+", 1..5)
    ) {
        let mut source = leading;
        source.push_str(&tokens.join(" "));
        let (tape, trivia) = simple_lex(&source);
        prop_assert_eq!(tape.reconstruct(&trivia), source);
    }

    /// Property: multiple whitespace tokens between words are preserved.
    #[test]
    fn multi_whitespace_preserved(
        w1 in "[a-zA-Z]+",
        ws in "[ \t\n]{2,5}",
        w2 in "[a-zA-Z]+"
    ) {
        let source = format!("{w1}{ws}{w2}");
        let (tape, trivia) = simple_lex(&source);
        prop_assert_eq!(tape.reconstruct(&trivia), source);
    }
}
