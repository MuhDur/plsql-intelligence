//! Grammar-position-preserving privacy scrub (spec §2.2,
//! `PLSQL-USR-001`).
//!
//! The P2 builder originally re-synthesised a minimised candidate
//! with `plsql_support::scrub_literals(ScrubThresholds::strict())`
//! plus an identifier rename. That blanket scrub is **not**
//! grammar-position-preserving: collapsing a `NUMBER` literal to the
//! word `NUM`, a string body to `<SCRUBBED>`, or changing token
//! boundaries flips the ANTLR parse far enough that the
//! *fine-grained* `(diag_code, antlr_rule_path, signature)` triple
//! the [`SignatureOracle`] honestly re-checks no longer reproduces.
//! Result: only the coarsest `text_scan>create` class minimised; the
//! ~30 structured classes (`unit_statement>create_table`, …) could
//! not get a privacy-proven fixture.
//!
//! This module replaces the blanket scrub with a **token-class
//! preserving** re-synthesis:
//!
//! 1. Tokenise the candidate with the project's real ANTLR backend
//!    (the same lexer the analysis pipeline uses) — we only ever
//!    read the backend-independent [`plsql_parser::tokens`] surface
//!    (`TokenKind`, `Span`, verbatim `text`); **no ANTLR type
//!    crosses the crate boundary** (R20). The backend lives behind
//!    `plsql-parser-antlr`'s `antlr-codegen` feature, exactly as the
//!    engine consumes it.
//! 2. For every token, emit a replacement of the **same lexical
//!    class** so it re-lexes to the same [`TokenKind`] in the same
//!    position:
//!    * `Identifier` → `id_<hash12>` (a valid identifier).
//!    * `QuotedIdentifier` → `"id_<hash12>"` (quoting preserved).
//!    * `StringLiteral` → `'sx_<hash8>'` (same `'` delimiter, a
//!      fixed-length-class synthetic body).
//!    * `NumericLiteral` → a fixed synthetic numeral of the same
//!      numeric subtype (int → `7`, float → `7.0`) so it lexes as
//!      the same NUMBER.
//!    * keyword / built-in / punctuation / operator → **verbatim**
//!      (grammar constants, never estate data).
//!    * `Unknown` → conservatively treated as an identifier-class
//!      synthetic (privacy-safe: nothing original survives).
//! 3. Comments are *already* stripped upstream (`strip_comments`);
//!    any residual comment trivia is dropped to a single space here
//!    (pure leak vector, parse-irrelevant). Whitespace trivia is
//!    kept verbatim — it is not estate data and preserves the exact
//!    inter-token layout the lexer saw.
//!
//! **Determinism (I-DETERMINISM).** The synthetic for a token is a
//! pure function of `(lexical-class, pinned salt, original token
//! text)`. The same original token always maps to the same synthetic
//! (consistent renaming) so intra-fixture references still resolve
//! and the parse tree is structurally identical. Same input + commit
//! ⇒ byte-identical output. No RNG, no wall-clock.
//!
//! **Privacy (I-PRIVACY).** The output contains, by construction,
//! only: grammar keywords/built-ins (constants), punctuation,
//! operators, whitespace, and `id_`/`sx_`/numeric synthetic aliases
//! whose bytes are a one-way hash of the salt — never the original
//! text. The independent residue scan in `fixture.rs` re-proves this
//! positively (every surviving word ∈ the synthetic/keyword
//! allowlist) and the redaction-delta manifest proves the buffer is
//! a deterministic replay. If anything cannot be proven the fixture
//! is discarded, never stored.

use plsql_core::FileId;
use plsql_parser::tokens::TokenKind;
use plsql_parser::{ParseOptions, parse_with_backend};
use plsql_parser_antlr::Antlr4RustBackend;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// The pinned scrub salt. Same rationale as the rename salt in
/// `fixture.rs`: a fixed salt is required for I-DETERMINISM and is
/// **not** a privacy weakness — every synthetic body is a one-way
/// `sha256(salt ‖ class ‖ original)` truncation and the privacy
/// proof independently verifies zero original-byte residue.
const SCRUB_SALT: &str = "plsql.usr.tokscrub.v1";

/// Lexical class of a token, collapsing the [`TokenKind`] vocabulary
/// to the few buckets that drive synthesis. Two original tokens that
/// share a class **and** their original text get the same synthetic
/// (consistent renaming → parse tree stays structurally identical).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Class {
    /// Bare identifier — `id_<hash12>`.
    Ident,
    /// `"quoted identifier"` — `"id_<hash12>"`.
    QuotedIdent,
    /// `'string literal'` — `'sx_<hash8>'`.
    Str,
    /// Numeric literal — fixed synthetic numeral, subtype-preserving.
    Num,
}

/// Map a [`TokenKind`] to either "keep verbatim" (`None` — grammar
/// constant, not estate) or an estate-bearing [`Class`] to
/// synthesise.
fn classify(kind: TokenKind) -> Option<Class> {
    match kind {
        // Estate-bearing — must be re-synthesised.
        TokenKind::Identifier | TokenKind::Unknown => Some(Class::Ident),
        TokenKind::QuotedIdentifier => Some(Class::QuotedIdent),
        TokenKind::StringLiteral => Some(Class::Str),
        TokenKind::NumericLiteral => Some(Class::Num),
        // Grammar constants — keyword names, built-in names,
        // punctuation, operators. These are part of the language,
        // never estate data, and must survive verbatim so the parse
        // position is byte-identical.
        TokenKind::Keyword
        | TokenKind::BuiltIn
        | TokenKind::Semicolon
        | TokenKind::Slash
        | TokenKind::Dot
        | TokenKind::Comma
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::Assign
        | TokenKind::Arrow
        | TokenKind::Concat
        | TokenKind::Operator
        | TokenKind::IncludeDirective
        | TokenKind::StatementTerminator => None,
    }
}

/// `sha256(salt ‖ \0 ‖ tag ‖ \0 ‖ raw)` truncated to `nibbles` hex
/// chars. One-way; the original `raw` is unrecoverable from the
/// output, and a fixed salt keeps it deterministic.
fn hash_hex(tag: &str, raw: &str, nibbles: usize) -> String {
    let mut h = Sha256::new();
    h.update(SCRUB_SALT.as_bytes());
    h.update(b"\x00");
    h.update(tag.as_bytes());
    h.update(b"\x00");
    h.update(raw.as_bytes());
    let digest = h.finalize();
    let mut s = String::with_capacity(nibbles);
    for b in digest {
        if s.len() >= nibbles {
            break;
        }
        s.push_str(&format!("{b:02x}"));
    }
    s.truncate(nibbles);
    s
}

/// `true` iff the numeric literal is a floating/real subtype (has a
/// `.`, an exponent, or a trailing `f`/`d` precision marker) versus a
/// plain integer. Preserving the subtype keeps the token lexing as
/// the same NUMBER alternative the grammar saw.
fn is_float_numeral(raw: &str) -> bool {
    let r = raw.trim();
    r.contains('.') || r.contains(['e', 'E']) || r.ends_with(['f', 'F', 'd', 'D'])
}

/// Build the same-class synthetic for one original token.
///
/// Consistent: the returned string is a pure function of
/// `(class, original_text)`, so the same original token always
/// yields the same synthetic. The synthetic is guaranteed to lex as
/// the **same** [`TokenKind`] (an identifier where an identifier was,
/// a same-delimiter string where a string was, a same-subtype
/// numeral where a number was) — that is what preserves the ANTLR
/// parse position and therefore the fine-grained `antlr_rule_path`.
fn synthesise(class: Class, original: &str) -> String {
    match class {
        Class::Ident => format!("id_{}", hash_hex("ident", original, 12)),
        Class::QuotedIdent => format!("\"id_{}\"", hash_hex("qident", original, 12)),
        Class::Str => format!("'sx_{}'", hash_hex("str", original, 8)),
        Class::Num => {
            if is_float_numeral(original) {
                "7.0".to_string()
            } else {
                "7".to_string()
            }
        }
    }
}

/// Tokenise `src` with the real ANTLR backend and re-synthesise it
/// token-by-token into a grammar-position-preserving, privacy-safe
/// buffer.
///
/// Returns `None` iff the backend produced **no** tokens (an empty
/// or all-trivia source) — the caller treats that exactly like a
/// scrub that lost the repro (honest discard), never a panic.
///
/// The reconstruction walks the token tape and, for index `i`,
/// emits the trivia preceding token `i` (whitespace verbatim;
/// comment trivia, which should already be gone, flattened to a
/// single space as a defensive leak guard) followed by either the
/// token's verbatim text (grammar constant) or its same-class
/// synthetic (estate-bearing). Because the token sequence, the kinds,
/// and the inter-token whitespace are all preserved, the result
/// re-lexes and re-parses to the **same** grammar position — the
/// `SignatureOracle` (unchanged) re-checks and accepts it.
#[must_use]
pub fn structure_preserving_scrub(src: &str) -> Option<String> {
    let backend = Antlr4RustBackend::new();
    let result = parse_with_backend(src, FileId::new(0), &backend, &ParseOptions::default());
    let tape = &result.cst.token_tape;
    let trivia = &result.cst.trivia;
    if tape.is_empty() {
        return None;
    }

    // Consistent-rename memo: (class, original-text) → synthetic, so
    // the same original token maps to the same synthetic everywhere
    // (intra-fixture references still resolve; parse tree identical).
    let mut memo: HashMap<(Class, String), String> = HashMap::new();
    let mut out = String::with_capacity(src.len());

    let emit_trivia = |out: &mut String, idx: usize| {
        for t in trivia.get(idx) {
            match t {
                plsql_parser::tokens::Trivia::Whitespace(w) => out.push_str(w),
                // Comments are stripped upstream; if any survived
                // they are a pure leak vector and parse-irrelevant —
                // collapse to a single space (never copy the bytes).
                plsql_parser::tokens::Trivia::LineComment(_)
                | plsql_parser::tokens::Trivia::BlockComment(_) => out.push(' '),
            }
        }
    };

    for (i, tok) in tape.tokens.iter().enumerate() {
        emit_trivia(&mut out, i);
        match classify(tok.kind) {
            // Grammar constant — survives verbatim (keyword names,
            // punctuation, operators are the language, not estate).
            None => out.push_str(&tok.text),
            // Estate-bearing — replace with a same-lexical-class
            // synthetic, consistently.
            Some(class) => {
                let key = (class, tok.text.clone());
                let syn = memo
                    .entry(key)
                    .or_insert_with(|| synthesise(class, &tok.text))
                    .clone();
                out.push_str(&syn);
            }
        }
    }
    // Trailing trivia (after the last token).
    emit_trivia(&mut out, tape.tokens.len());

    Some(out)
}

/// The grammar-constant name of a [`TokenKind`] — a fixed language
/// constant, **never** estate data. This is the alphabet of the
/// spec §2.1 "token-kind sequence" (`PLSQL-USR-001`). It carries
/// zero source bytes (no text, no width, no offset) by construction:
/// the value is a function of the *kind discriminant only*.
#[must_use]
fn token_kind_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::StringLiteral => "STR",
        TokenKind::NumericLiteral => "NUM",
        TokenKind::QuotedIdentifier => "QID",
        TokenKind::Keyword => "KW",
        TokenKind::BuiltIn => "BI",
        TokenKind::Identifier => "ID",
        TokenKind::Semicolon => "SEMI",
        TokenKind::Slash => "SLASH",
        TokenKind::Dot => "DOT",
        TokenKind::Comma => "COMMA",
        TokenKind::LParen => "LP",
        TokenKind::RParen => "RP",
        TokenKind::Assign => "ASSIGN",
        TokenKind::Arrow => "ARROW",
        TokenKind::Concat => "CONCAT",
        TokenKind::Operator => "OP",
        TokenKind::IncludeDirective => "INC",
        TokenKind::StatementTerminator => "TERM",
        TokenKind::Unknown => "UNK",
    }
}

/// Tokenise `src` with the real ANTLR lexer (the same backend the
/// analysis pipeline and the privacy scrub use) and return the
/// deterministic sequence of grammar-constant **token-KIND** names
/// — the spec §2.1 "token-kind sequence, never text" / §2[C]
/// "token-shape hash" input (`PLSQL-USR-001`).
///
/// **I-PRIVACY.** The output contains, by construction, *only*
/// [`token_kind_name`] constants (`KW`, `ID`, `STR`, …). No token
/// text, no literal value, no identifier byte, and — deliberately —
/// **no span width, line count, or byte offset** ever appears. It is
/// a pure function of the lexer's `TokenKind` discriminant stream.
///
/// **I-DETERMINISM.** The ANTLR lexer is deterministic; the same
/// `src` always yields the same kind stream, so the same input +
/// commit ⇒ a byte-identical shape (and therefore a byte-identical
/// signature).
///
/// **Minimisation stability (the whole point of the §2[C] fix).**
/// Because the shape is derived from a *canonical construct skeleton*
/// (the caller passes the grammar-keyword skeleton implied by the
/// gap's `antlr_rule_path`, never the variable estate block), ddmin
/// narrowing the surrounding estate text cannot change it: the
/// signature is now a true gap-class identifier, not a block-size
/// fingerprint.
///
/// Returns `None` iff the backend produced **no** tokens (empty /
/// all-trivia input) — the caller treats that as the absent-shape
/// sentinel, never a panic.
#[must_use]
pub fn token_kind_shape(src: &str) -> Option<Vec<String>> {
    let backend = Antlr4RustBackend::new();
    let result = parse_with_backend(src, FileId::new(0), &backend, &ParseOptions::default());
    let tape = &result.cst.token_tape;
    if tape.is_empty() {
        return None;
    }
    Some(
        tape.tokens
            .iter()
            .map(|t| token_kind_name(t.kind).to_string())
            .collect(),
    )
}

/// One token's privacy verdict for the residue proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokVerdict {
    /// A grammar constant (keyword / built-in / punctuation /
    /// operator) — part of the language, never estate data, allowed
    /// to survive verbatim.
    GrammarConstant,
    /// An estate-class token (identifier / quoted-id / string /
    /// number). Carries its verbatim `text` so the caller can prove
    /// it is a synthetic alias (and **not** an original byte).
    EstateClass(String),
}

/// Tokenise `buf` with the real ANTLR lexer and return one
/// [`TokVerdict`] per on-channel token, in order.
///
/// This is the authoritative, **wordlist-free** input to the
/// I-PRIVACY residue proof: the keyword/identifier judgment is the
/// real lexer's, not a hand-maintained reserved-word list (the lab
/// `DEFAULT_RESERVED` subset is far smaller than the grammar's
/// keyword vocabulary, so a wordlist scan wrongly flags
/// legitimately-surviving keywords like `TABLE`/`VARCHAR2`/`SYSDATE`
/// as residue). The caller asserts every [`TokVerdict::EstateClass`]
/// body is a synthetic `id_`/`sx_`/numeral alias — anything else is
/// an original-byte leak and the fixture is discarded.
///
/// Returns `None` iff the buffer produced no tokens.
#[must_use]
pub fn token_verdicts(buf: &str) -> Option<Vec<TokVerdict>> {
    let backend = Antlr4RustBackend::new();
    let result = parse_with_backend(buf, FileId::new(0), &backend, &ParseOptions::default());
    let tape = &result.cst.token_tape;
    if tape.is_empty() {
        return None;
    }
    Some(
        tape.tokens
            .iter()
            .map(|t| match classify(t.kind) {
                None => TokVerdict::GrammarConstant,
                Some(_) => TokVerdict::EstateClass(t.text.clone()),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_buckets_estate_vs_grammar() {
        assert_eq!(classify(TokenKind::Identifier), Some(Class::Ident));
        assert_eq!(classify(TokenKind::Unknown), Some(Class::Ident));
        assert_eq!(
            classify(TokenKind::QuotedIdentifier),
            Some(Class::QuotedIdent)
        );
        assert_eq!(classify(TokenKind::StringLiteral), Some(Class::Str));
        assert_eq!(classify(TokenKind::NumericLiteral), Some(Class::Num));
        // Grammar constants never synthesised.
        assert_eq!(classify(TokenKind::Keyword), None);
        assert_eq!(classify(TokenKind::BuiltIn), None);
        assert_eq!(classify(TokenKind::Semicolon), None);
        assert_eq!(classify(TokenKind::LParen), None);
        assert_eq!(classify(TokenKind::Operator), None);
    }

    #[test]
    fn synthetic_is_deterministic_and_consistent() {
        let a = synthesise(Class::Ident, "customers_pii");
        let b = synthesise(Class::Ident, "customers_pii");
        assert_eq!(a, b, "same original ⇒ same synthetic (consistent)");
        assert!(a.starts_with("id_") && a.len() == 15, "{a}");
        // Different originals ⇒ (almost surely) different synthetics.
        assert_ne!(a, synthesise(Class::Ident, "billing_acct"));
        // Quoted identifier keeps its quoting.
        let q = synthesise(Class::QuotedIdent, "My Col");
        assert!(q.starts_with("\"id_") && q.ends_with('"'), "{q}");
        // String keeps the single-quote delimiter, fixed body shape.
        let s = synthesise(Class::Str, "ZZSECRETZZ-9988");
        assert!(s.starts_with("'sx_") && s.ends_with('\''), "{s}");
        assert!(!s.contains("ZZSECRET"), "no original bytes: {s}");
    }

    #[test]
    fn numeral_subtype_preserved() {
        assert_eq!(synthesise(Class::Num, "4111111111111111"), "7");
        assert_eq!(synthesise(Class::Num, "3.14159"), "7.0");
        assert_eq!(synthesise(Class::Num, "1e-3"), "7.0");
        assert!(is_float_numeral("2.5"));
        assert!(is_float_numeral("6.022E23"));
        assert!(!is_float_numeral("42"));
    }

    #[test]
    fn scrub_preserves_structure_and_drops_secrets() {
        let src = "CREATE TABLE customers_pii (\n  id NUMBER DEFAULT 4111111111111111,\n  tag VARCHAR2(40) DEFAULT 'ZZSECRETZZ-9988-7766'\n);\n";
        let out = structure_preserving_scrub(src).expect("tokenises");
        // No planted secret bytes survive.
        assert!(!out.contains("customers_pii"), "{out}");
        assert!(!out.contains("4111111111111111"), "{out}");
        assert!(!out.contains("ZZSECRETZZ"), "{out}");
        // Grammar keywords/punctuation survive verbatim — the parse
        // position is preserved.
        assert!(out.contains("CREATE") && out.contains("TABLE"), "{out}");
        assert!(out.contains("NUMBER") && out.contains("DEFAULT"), "{out}");
        assert!(
            out.contains('(') && out.contains(')') && out.contains(';'),
            "{out}"
        );
        // The identifier slot is now a synthetic alias.
        assert!(out.contains("id_"), "{out}");
        // Determinism: byte-identical on a second run.
        assert_eq!(structure_preserving_scrub(src), Some(out));
    }

    #[test]
    fn empty_or_trivia_only_yields_none() {
        assert_eq!(structure_preserving_scrub(""), None);
        assert_eq!(structure_preserving_scrub("   \n  "), None);
    }
}
