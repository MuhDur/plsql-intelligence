//! Token tape types.
//!
//! The token tape is the **lossless** representation of the source.  Every
//! token carries a byte-offset span; trivia (whitespace, comments) is
//! preserved verbatim in a side-table.  Round-tripping is:
//!
//! ```text
//! reconstruct(token_tape(input)) == input   // byte-for-byte
//! ```
//!
//! This contract is enforced by the proptest in `tests/conformance.rs`.

use plsql_core::Span;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TokenKind
// ---------------------------------------------------------------------------

/// Discriminator for a syntactic token.
///
/// The set is deliberately coarse at this layer — backends map their
/// internal token vocabulary into these kinds.  The mapping is
/// backend-private (R20).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TokenKind {
    // Literals
    /// A string literal (`'hello'`, `q'[...]'`).
    StringLiteral,
    /// A numeric literal (`42`, `3.14`, `1e-3`).
    NumericLiteral,
    /// A quoted identifier (`"My_Table"`).
    QuotedIdentifier,

    // Keywords
    /// A PL/SQL or SQL keyword (`SELECT`, `BEGIN`, `PACKAGE`).
    Keyword,
    /// A built-in Oracle function name treated as keyword contextually.
    BuiltIn,

    // Identifiers
    /// An unquoted identifier (`EMPLOYEES`, `v_count`).
    Identifier,

    // Punctuation / delimiters
    /// A semicolon (`;`).
    Semicolon,
    /// A forward slash (`/`) — statement terminator in SQL*Plus.
    Slash,
    /// A dot (`.`).
    Dot,
    /// A comma (`,`).
    Comma,
    /// An opening parenthesis (`(`).
    LParen,
    /// A closing parenthesis (`)`).
    RParen,
    /// An assignment operator (`:=`).
    Assign,
    /// The fat arrow (`=>`).
    Arrow,
    /// The pipe-pipe concatenation (`||`).
    Concat,
    /// Any other operator (`+`, `-`, `*`, `/`, `=`, `<`, `>`, etc.).
    Operator,
    /// An `@` or `@@` include directive.
    IncludeDirective,
    /// A `/` on a line by itself (SQL*Plus statement terminator).
    StatementTerminator,

    // Error
    /// The backend could not classify this token.
    Unknown,
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// A single syntactic token in the token tape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Token {
    /// What kind of token this is.
    pub kind: TokenKind,
    /// Byte-offset span in the original source.
    pub span: Span,
    /// The raw source text of this token (verbatim).
    pub text: String,
}

impl Token {
    #[must_use]
    pub fn new(kind: TokenKind, span: Span, text: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            text: text.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Trivia
// ---------------------------------------------------------------------------

/// A piece of trivia — whitespace, comments, or other non-token source text
/// that must be preserved for lossless round-tripping.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Trivia {
    /// Horizontal or vertical whitespace.
    Whitespace(String),
    /// A single-line comment (`-- ...`).
    LineComment(String),
    /// A block comment (`/* ... */`).
    BlockComment(String),
}

// ---------------------------------------------------------------------------
// TriviaTable
// ---------------------------------------------------------------------------

/// Maps each token index to the trivia that **precedes** it.
///
/// Index `i` in this table holds the trivia between token `i-1` and token `i`.
/// Index `0` holds leading trivia (before the first token).
/// Trailing trivia (after the last token) is stored at index `tokens.len()`.
///
/// This is a sparse mapping: if a token has no preceding trivia, the entry is
/// an empty `Vec`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TriviaTable {
    /// `leading[i]` = trivia preceding the i-th token.  Trailing trivia goes
    /// at index `tokens.len()`.
    pub leading: Vec<Vec<Trivia>>,
}

impl TriviaTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            leading: Vec::new(),
        }
    }

    /// Push a trivia entry for the given token index.
    pub fn push(&mut self, token_index: usize, trivia: Trivia) {
        while self.leading.len() <= token_index {
            self.leading.push(Vec::new());
        }
        self.leading[token_index].push(trivia);
    }

    /// Get the trivia preceding the given token index.
    #[must_use]
    pub fn get(&self, token_index: usize) -> &[Trivia] {
        self.leading.get(token_index).map_or(&[], |v| v.as_slice())
    }

    /// Total number of trivia entries across all tokens.
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.leading.iter().map(Vec::len).sum()
    }
}

// ---------------------------------------------------------------------------
// TokenTape
// ---------------------------------------------------------------------------

/// An ordered sequence of tokens representing the full lexed source.
///
/// Combined with the [`TriviaTable`], this allows perfect reconstruction
/// of the original source text.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenTape {
    pub tokens: Vec<Token>,
}

impl TokenTape {
    #[must_use]
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Push a token onto the tape.
    pub fn push(&mut self, token: Token) {
        self.tokens.push(token);
    }

    /// Reconstruct the original source text from the token tape + trivia.
    ///
    /// This is the lossless round-trip function.  For a valid tape:
    ///
    /// ```text
    /// reconstruct(tape, trivia) == original_source
    /// ```
    #[must_use]
    pub fn reconstruct(&self, trivia: &TriviaTable) -> String {
        let mut out = String::new();
        for (i, token) in self.tokens.iter().enumerate() {
            // Emit preceding trivia
            for t in trivia.get(i) {
                match t {
                    Trivia::Whitespace(s) | Trivia::LineComment(s) | Trivia::BlockComment(s) => {
                        out.push_str(s)
                    }
                }
            }
            // Emit the token itself
            out.push_str(&token.text);
        }
        // Trailing trivia (after last token)
        for t in trivia.get(self.tokens.len()) {
            match t {
                Trivia::Whitespace(s) | Trivia::LineComment(s) | Trivia::BlockComment(s) => {
                    out.push_str(s)
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position};

    fn span(start: u32, len: u32) -> Span {
        Span::new(
            FileId::new(0),
            Position::new(1, 1, start),
            Position::new(1, 1, start + len),
        )
    }

    #[test]
    fn empty_tape_reconstructs_to_empty_string() {
        let tape = TokenTape::new();
        let trivia = TriviaTable::new();
        assert_eq!(tape.reconstruct(&trivia), "");
    }

    #[test]
    fn single_token_no_trivia() {
        let mut tape = TokenTape::new();
        tape.push(Token::new(TokenKind::Keyword, span(0, 6), "SELECT"));
        let trivia = TriviaTable::new();
        assert_eq!(tape.reconstruct(&trivia), "SELECT");
    }

    #[test]
    fn reconstruct_with_leading_and_inter_token_trivia() {
        let mut tape = TokenTape::new();
        tape.push(Token::new(TokenKind::Keyword, span(2, 6), "SELECT"));
        tape.push(Token::new(TokenKind::Identifier, span(9, 4), "name"));
        tape.push(Token::new(TokenKind::Keyword, span(14, 4), "FROM"));
        tape.push(Token::new(TokenKind::Identifier, span(19, 5), "users"));
        tape.push(Token::new(TokenKind::Semicolon, span(24, 1), ";"));

        let mut trivia = TriviaTable::new();
        // Leading whitespace before SELECT
        trivia.push(0, Trivia::Whitespace("  ".to_string()));
        // Whitespace between SELECT and name
        trivia.push(1, Trivia::Whitespace(" ".to_string()));
        // Whitespace between name and FROM
        trivia.push(2, Trivia::Whitespace(" ".to_string()));
        // Whitespace between FROM and users
        trivia.push(3, Trivia::Whitespace(" ".to_string()));
        // No trivia before semicolon

        assert_eq!(tape.reconstruct(&trivia), "  SELECT name FROM users;");
    }

    #[test]
    fn reconstruct_preserves_comments() {
        let mut tape = TokenTape::new();
        tape.push(Token::new(TokenKind::Keyword, span(12, 6), "SELECT"));
        tape.push(Token::new(TokenKind::NumericLiteral, span(19, 1), "1"));
        tape.push(Token::new(TokenKind::Semicolon, span(20, 1), ";"));

        let mut trivia = TriviaTable::new();
        trivia.push(0, Trivia::LineComment("-- pick one\n".to_string()));
        trivia.push(1, Trivia::Whitespace(" ".to_string()));

        assert_eq!(tape.reconstruct(&trivia), "-- pick one\nSELECT 1;");
    }

    #[test]
    fn trivia_table_total_count() {
        let mut trivia = TriviaTable::new();
        trivia.push(0, Trivia::Whitespace(" ".to_string()));
        trivia.push(2, Trivia::Whitespace(" ".to_string()));
        trivia.push(2, Trivia::LineComment("-- x".to_string()));
        assert_eq!(trivia.total_count(), 3);
    }

    #[test]
    fn trivia_table_get_out_of_bounds_returns_empty() {
        let trivia = TriviaTable::new();
        assert_eq!(trivia.get(999), &[] as &[Trivia]);
    }
}
