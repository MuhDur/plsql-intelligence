//! Literal classifier (PLSQL-SUPPORT-003).
//!
//! Given the *value* of a string/numeric literal lifted out of PL/SQL
//! source, decide what kind of thing it is: credential-like, SQL-like,
//! URL-like, free-text, numeric, date/time, or unknown. Two consumers:
//!
//! 1. **Dynamic-SQL evidence** — when the engine recognises an
//!    `EXECUTE IMMEDIATE '…'` / `OPEN cur FOR '…'` site, classifying
//!    the literal lets the evidence record say "the dynamic statement
//!    is a recognisable SQL string" vs "the argument is opaque
//!    free-text the analyser cannot reason about".
//! 2. **Redaction diagnostics** — a `CredentialLike` literal is a
//!    high-priority scrub target; surfacing the class lets the
//!    support flow explain *why* a value was redacted.
//!
//! The classifier is a deterministic heuristic over the raw value —
//! no parsing, no allocation beyond a lower-cased copy. It never
//! returns a false `Unknown` when a stronger class matches; the
//! priority order is safety-first (credential beats everything so a
//! JDBC URL embedding a password is flagged as a credential, not a
//! URL).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Dynamic SQL
//!   chapter: `EXECUTE IMMEDIATE` / native dynamic SQL is the site
//!   whose literal argument this classifier scores.
//! * `LOW-LEVEL-CATALOGS.md` — `DBMS_ASSERT` is the Oracle-side
//!   guard for SQL-injectable literals; a `SqlLike` classification is
//!   the source-only signal that such a guard *should* be present.

use serde::{Deserialize, Serialize};

/// Bucket a literal falls into. `Unknown` is the explicit "no
/// confident signal" outcome — never a silent default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiteralClass {
    /// Looks like a secret: password / token / api key / connection
    /// secret. Highest scrub priority.
    CredentialLike,
    /// Recognisable SQL or PL/SQL statement text (the dynamic-SQL
    /// signal).
    SqlLike,
    /// URL, JDBC/TNS connect string, or host:port endpoint.
    UrlLike,
    /// A date or timestamp value (ISO-ish or Oracle default).
    DateTime,
    /// Pure numeric value.
    Numeric,
    /// Readable prose with no stronger signal.
    FreeText,
    /// No confident classification.
    #[default]
    Unknown,
}

/// Classification outcome: the class plus a one-line rationale so
/// diagnostics can explain the call without re-deriving it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiteralClassification {
    pub class: LiteralClass,
    /// Short, human-readable reason for the assigned class.
    pub rationale: String,
}

impl LiteralClassification {
    fn of(class: LiteralClass, rationale: impl Into<String>) -> Self {
        Self {
            class,
            rationale: rationale.into(),
        }
    }
}

/// Keywords whose presence (case-insensitively, as a whole word at
/// the start or after whitespace) marks a value as SQL-like.
const SQL_LEAD_KEYWORDS: &[&str] = &[
    "select ",
    "insert ",
    "update ",
    "delete ",
    "merge ",
    "with ",
    "begin ",
    "declare ",
    "create ",
    "alter ",
    "drop ",
    "grant ",
    "revoke ",
    "truncate ",
    "call ",
];

/// Substrings (case-insensitive) that strongly imply a secret.
const CREDENTIAL_MARKERS: &[&str] = &[
    "password=",
    "password ",
    "passwd=",
    "pwd=",
    "secret=",
    "api_key=",
    "apikey=",
    "api-key=",
    "access_token=",
    "token=",
    "client_secret=",
    "private_key",
    "begin rsa",
    "begin private key",
    "identified by ",
    "bearer ",
];

/// URL / connect-string scheme prefixes (case-insensitive).
const URL_SCHEMES: &[&str] = &[
    "http://",
    "https://",
    "ftp://",
    "jdbc:",
    "tcp://",
    "tcps://",
    "ldap://",
    "ldaps://",
    "(description=",
    "(connect_data=",
];

/// Classify a literal `value` (the *content* of the literal, without
/// the surrounding quotes). Deterministic and allocation-light.
///
/// Priority order is deliberate and safety-first:
/// `CredentialLike` > `SqlLike` > `UrlLike` > `DateTime` >
/// `Numeric` > `FreeText` > `Unknown`. A value that matches several
/// classes is reported as the highest-priority one so a redaction
/// flow never under-classifies a secret.
#[must_use]
pub fn classify_literal(value: &str) -> LiteralClassification {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return LiteralClassification::of(LiteralClass::Unknown, "empty literal");
    }
    let lower = trimmed.to_ascii_lowercase();

    if let Some(marker) = CREDENTIAL_MARKERS.iter().find(|m| lower.contains(*m)) {
        return LiteralClassification::of(
            LiteralClass::CredentialLike,
            format!("contains credential marker {marker:?}"),
        );
    }

    if SQL_LEAD_KEYWORDS
        .iter()
        .any(|kw| lower.starts_with(kw.trim_end()) && starts_with_sql_word(&lower, kw))
    {
        return LiteralClassification::of(
            LiteralClass::SqlLike,
            "begins with a SQL/PL-SQL leading keyword",
        );
    }

    if let Some(scheme) = URL_SCHEMES.iter().find(|s| lower.starts_with(*s)) {
        return LiteralClassification::of(
            LiteralClass::UrlLike,
            format!("starts with URL/connect scheme {scheme:?}"),
        );
    }

    if looks_like_datetime(trimmed) {
        return LiteralClassification::of(LiteralClass::DateTime, "matches a date/timestamp shape");
    }

    if is_numeric(trimmed) {
        return LiteralClassification::of(LiteralClass::Numeric, "pure numeric value");
    }

    if is_free_text(trimmed) {
        return LiteralClassification::of(
            LiteralClass::FreeText,
            "readable text with whitespace and no stronger signal",
        );
    }

    LiteralClassification::of(LiteralClass::Unknown, "no confident signal")
}

/// True when `lower` starts with `kw` (which ends in a space) OR is
/// exactly the keyword with no trailing argument — guards against a
/// column literally named `selected` being read as `SELECT`.
fn starts_with_sql_word(lower: &str, kw: &str) -> bool {
    let word = kw.trim_end();
    lower.strip_prefix(word).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric() && c != '_')
    })
}

/// Recognise the common Oracle / ISO date and timestamp shapes:
/// `YYYY-MM-DD`, `YYYY-MM-DD HH:MI:SS`, `DD-MON-YYYY`, optionally
/// with a fractional-second / timezone tail.
fn looks_like_datetime(value: &str) -> bool {
    let v = value.trim();
    let mut parts = v.split([' ', 'T']);
    let date = match parts.next() {
        Some(d) => d,
        None => return false,
    };
    let is_iso_date = {
        let seg: Vec<&str> = date.split('-').collect();
        seg.len() == 3
            && seg[0].len() == 4
            && seg[0].chars().all(|c| c.is_ascii_digit())
            && seg[1].len() <= 2
            && !seg[1].is_empty()
            && seg[1].chars().all(|c| c.is_ascii_digit())
            && seg[2].len() <= 2
            && !seg[2].is_empty()
            && seg[2].chars().all(|c| c.is_ascii_digit())
    };
    let is_oracle_default = {
        // DD-MON-YYYY, e.g. 15-MAY-2026.
        let seg: Vec<&str> = date.split('-').collect();
        seg.len() == 3
            && seg[0].len() <= 2
            && seg[0].chars().all(|c| c.is_ascii_digit())
            && seg[1].len() == 3
            && seg[1].chars().all(|c| c.is_ascii_alphabetic())
            && seg[2].len() == 4
            && seg[2].chars().all(|c| c.is_ascii_digit())
    };
    if !is_iso_date && !is_oracle_default {
        return false;
    }
    match parts.next() {
        None => true,
        Some(time) => {
            let core = time
                .trim_end_matches('Z')
                .split(['.', '+'])
                .next()
                .unwrap_or(time);
            let seg: Vec<&str> = core.split(':').collect();
            (2..=3).contains(&seg.len())
                && seg
                    .iter()
                    .all(|s| !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit()))
        }
    }
}

/// True for an integer / decimal / scientific numeric value with an
/// optional leading sign. Rejects values with embedded whitespace or
/// letters other than a single exponent `e`/`E`.
fn is_numeric(value: &str) -> bool {
    let v = value.trim();
    let v = v.strip_prefix(['+', '-']).unwrap_or(v);
    if v.is_empty() {
        return false;
    }
    let (mantissa, exponent) = match v.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (v, None),
    };
    let mantissa_ok = {
        let mut dots = 0;
        !mantissa.is_empty()
            && mantissa.chars().all(|c| {
                if c == '.' {
                    dots += 1;
                    dots <= 1
                } else {
                    c.is_ascii_digit()
                }
            })
            && mantissa.chars().any(|c| c.is_ascii_digit())
    };
    let exponent_ok = match exponent {
        None => true,
        Some(e) => {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
        }
    };
    mantissa_ok && exponent_ok
}

/// A value is free-text when it has at least one ASCII letter and at
/// least one whitespace-separated word boundary — i.e. it reads like
/// a sentence/phrase rather than an opaque token.
fn is_free_text(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_alphabetic()) && value.split_whitespace().count() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(v: &str) -> LiteralClass {
        classify_literal(v).class
    }

    #[test]
    fn credential_marker_beats_url() {
        // A JDBC URL embedding a password must surface as a credential
        // so the redaction flow treats it as a high-priority scrub.
        let v = "jdbc:oracle:thin:scott/tiger@db:1521/orcl?password=Sup3rSecret";
        assert_eq!(class(v), LiteralClass::CredentialLike);
    }

    #[test]
    fn identified_by_is_credential_like() {
        assert_eq!(
            class("ALTER USER hr IDENTIFIED BY h0tpassword"),
            LiteralClass::CredentialLike
        );
    }

    #[test]
    fn select_statement_is_sql_like() {
        assert_eq!(
            class("SELECT * FROM employees WHERE department_id = :d"),
            LiteralClass::SqlLike
        );
    }

    #[test]
    fn lowercase_plsql_block_is_sql_like() {
        assert_eq!(class("begin pkg.do_it(:x); end;"), LiteralClass::SqlLike);
    }

    #[test]
    fn selected_column_name_is_not_sql_like() {
        // Word-boundary guard: `selected_flag` must not read as SELECT.
        assert_ne!(class("selected_flag"), LiteralClass::SqlLike);
    }

    #[test]
    fn http_url_is_url_like() {
        assert_eq!(
            class("https://internal.example.com/callback"),
            LiteralClass::UrlLike
        );
    }

    #[test]
    fn tns_descriptor_is_url_like() {
        assert_eq!(
            class("(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=db)(PORT=1521)))"),
            LiteralClass::UrlLike
        );
    }

    #[test]
    fn iso_date_is_datetime() {
        assert_eq!(class("2026-05-15"), LiteralClass::DateTime);
    }

    #[test]
    fn iso_timestamp_is_datetime() {
        assert_eq!(class("2026-05-15 09:30:00"), LiteralClass::DateTime);
        assert_eq!(class("2026-05-15T09:30:00.123Z"), LiteralClass::DateTime);
    }

    #[test]
    fn oracle_default_date_is_datetime() {
        assert_eq!(class("15-MAY-2026"), LiteralClass::DateTime);
    }

    #[test]
    fn integer_and_float_are_numeric() {
        assert_eq!(class("1234567"), LiteralClass::Numeric);
        assert_eq!(class("-3.14159"), LiteralClass::Numeric);
        assert_eq!(class("1.5e+12"), LiteralClass::Numeric);
    }

    #[test]
    fn numeric_rejects_embedded_letters() {
        assert_ne!(class("12ab34"), LiteralClass::Numeric);
    }

    #[test]
    fn prose_is_free_text() {
        assert_eq!(
            class("Order shipped to the customer"),
            LiteralClass::FreeText
        );
    }

    #[test]
    fn opaque_token_is_unknown() {
        assert_eq!(class("X7Q"), LiteralClass::Unknown);
    }

    #[test]
    fn empty_is_unknown() {
        assert_eq!(class("   "), LiteralClass::Unknown);
    }

    #[test]
    fn rationale_is_populated() {
        let c = classify_literal("SELECT 1 FROM dual");
        assert_eq!(c.class, LiteralClass::SqlLike);
        assert!(!c.rationale.is_empty());
    }

    #[test]
    fn serde_round_trip_snake_case() {
        let c = classify_literal("password=hunter2");
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"class\":\"credential_like\""));
        let back: LiteralClassification = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    // --- Adversarial robustness (testing-fuzzing, deterministic) ---

    /// Tiny deterministic xorshift64* PRNG so the fuzz corpus is
    /// reproducible (no `rand`/`proptest` dependency — the crate is
    /// intentionally dependency-light).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    fn adversarial_corpus() -> Vec<String> {
        let mut v = vec![
            String::new(),
            " ".repeat(4096),
            "\0\u{1}\u{7f}".to_string(),
            "é".repeat(2000),
            "'".repeat(1000),
            "(".repeat(5000),
            "=".repeat(3000),
            "SELECT ".repeat(1000),
            "password=".to_string() + &"x".repeat(10_000),
            "\u{1F600}\u{200B}\u{FEFF}日本語".to_string(),
            "-3.4e-".to_string() + &"9".repeat(400),
            "DATE '".to_string() + &"9".repeat(100),
        ];
        // Random byte-ish strings of varied length.
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..2000 {
            let len = (rng.next() % 64) as usize;
            let s: String = (0..len)
                .map(|_| char::from_u32((rng.next() % 0x110000) as u32).unwrap_or('?'))
                .collect();
            v.push(s);
        }
        v
    }

    #[test]
    fn never_panics_and_always_returns_valid_classification() {
        for input in adversarial_corpus() {
            // Must not panic on any input (the classifier sees raw
            // literal content lifted from arbitrary PL/SQL source).
            let c = classify_literal(&input);
            // Invariant: every outcome carries a non-empty rationale.
            assert!(
                !c.rationale.is_empty(),
                "empty rationale for input of len {}",
                input.len()
            );
            // Invariant: classification is deterministic.
            assert_eq!(classify_literal(&input).class, c.class);
        }
    }

    #[test]
    fn credential_priority_holds_under_adversarial_prefixes() {
        // A credential marker anywhere must win over SQL/URL/text
        // regardless of surrounding noise — safety-first invariant.
        for noise in ["SELECT * FROM t WHERE ", "https://h/", "", "   "] {
            let v = format!("{noise}identified by s3cret");
            assert_eq!(
                classify_literal(&v).class,
                LiteralClass::CredentialLike,
                "credential must win for {v:?}"
            );
        }
    }
}
