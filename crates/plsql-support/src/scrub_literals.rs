//! Literal scrubbing pass (PLSQL-SUPPORT-013).
//!
//! Walks PL/SQL source text and replaces every long string literal,
//! numeric literal, and date literal with a placeholder. Used after
//! the identifier-rename pass (SUPPORT-012) to make sure even the
//! literal values left in the source can't leak customer data.
//!
//! The pass is parameterised by a [`ScrubThresholds`] struct so a
//! support flow can opt for stricter defaults than what SUPPORT-001
//! ships. The default thresholds are:
//!
//! * `string_min_len = 8` — strings shorter than 8 characters are
//!   left alone (NLS literals, small flags, single-letter codes).
//! * `numeric_min_digits = 4` — numbers smaller than 4 digits are
//!   left alone (loop counters, small ordinals).
//! * `date_literals_scrubbed = true` — `DATE '2024-…'`,
//!   `TIMESTAMP '…'`, and `q'[…]'` quoted literals are always
//!   scrubbed regardless of length.
//!
//! Stricter-than-default: callers can pass `ScrubThresholds {
//! string_min_len: 0, numeric_min_digits: 0, … }` to scrub every
//! literal, useful when shipping a bundle to an external auditor.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Literals
//!   chapter governs the string / number / date / interval shapes
//!   we walk here.
//! * `LOW-LEVEL-CATALOGS.md` — `DBMS_ASSERT.ENQUOTE_LITERAL` is the
//!   Oracle-side equivalent for quoting a value safely; the
//!   scrub pass is the source-only counterpart.

use serde::{Deserialize, Serialize};

/// Caller-supplied thresholds. Defaults match the SUPPORT-001
/// posture; stricter modes use `0` for every threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrubThresholds {
    pub string_min_len: usize,
    pub numeric_min_digits: usize,
    pub date_literals_scrubbed: bool,
}

impl ScrubThresholds {
    /// Default thresholds — protect long values, leave small ones
    /// alone for readability.
    #[must_use]
    pub fn default_thresholds() -> Self {
        Self {
            string_min_len: 8,
            numeric_min_digits: 4,
            date_literals_scrubbed: true,
        }
    }

    /// Strictest setting — scrub every literal regardless of size.
    /// Useful when shipping a bundle to an external auditor.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            string_min_len: 0,
            numeric_min_digits: 0,
            date_literals_scrubbed: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrubStats {
    pub strings_scrubbed: u32,
    pub numerics_scrubbed: u32,
    pub date_literals_scrubbed: u32,
}

/// Scrub `source` against `thresholds`. Returns the rewritten text
/// plus the per-kind hit counts so the caller can audit how much
/// was redacted.
#[must_use]
pub fn scrub_literals(source: &str, thresholds: ScrubThresholds) -> (String, ScrubStats) {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut stats = ScrubStats::default();
    let mut i = 0;

    while i < bytes.len() {
        // Pass through comments verbatim.
        if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-') {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push_str(&source[start..i]);
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2;
            } else {
                // Unterminated block comment. `i` may now land in
                // the middle of a multibyte UTF-8 sequence; emit
                // the rest of the source verbatim from the
                // comment's start to avoid a non-char-boundary
                // slice panic (code-review caught this).
                out.push_str(&source[start..]);
                i = bytes.len();
                continue;
            }
            out.push_str(&source[start..i]);
            continue;
        }

        // DATE 'YYYY-MM-DD' / TIMESTAMP '…' / INTERVAL '…' '…'
        // literals. Cheap match: look at the keyword position
        // case-insensitively. When the flag is disabled we still
        // need to detect the shape so the inner string isn't
        // double-scrubbed by the generic string-literal path.
        if let Some(end) = match_date_literal(&source[i..]) {
            if thresholds.date_literals_scrubbed {
                out.push_str("DATE '<SCRUBBED>'");
                stats.date_literals_scrubbed += 1;
            } else {
                out.push_str(&source[i..i + end]);
            }
            i += end;
            continue;
        }

        // q'[…]' literal — Oracle's bracket-delimited quote form.
        if bytes[i] == b'q' || bytes[i] == b'Q' {
            if let Some(end) = match_q_literal(&source[i..]) {
                out.push_str("q'[<SCRUBBED>]'");
                stats.strings_scrubbed += 1;
                i += end;
                continue;
            }
        }

        // String literal `'…'` with doubled-`''` escape.
        if bytes[i] == b'\'' {
            let mut j = i + 1;
            let mut inner = 0_usize;
            let mut terminated = false;
            while j < bytes.len() {
                if bytes[j] == b'\'' {
                    if bytes.get(j + 1) == Some(&b'\'') {
                        inner += 2;
                        j += 2;
                        continue;
                    }
                    j += 1;
                    terminated = true;
                    break;
                }
                inner += 1;
                j += 1;
            }
            // Security: an *unterminated* string literal has no
            // determinable length and may be a truncated/adversarial
            // fragment hiding a short secret below the scrub
            // threshold. Leaking customer data is strictly worse than
            // over-scrubbing, so always scrub when unterminated
            // regardless of `inner` vs `string_min_len`.
            if !terminated || inner >= thresholds.string_min_len {
                out.push_str("'<SCRUBBED>'");
                stats.strings_scrubbed += 1;
            } else {
                out.push_str(&source[i..j]);
            }
            i = j;
            continue;
        }

        // Numeric literal: digits + optional `.` + optional `eN`
        // (case-insensitive).
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let len = i - start;
            let digit_count = source[start..i]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .count();
            if digit_count >= thresholds.numeric_min_digits {
                out.push_str("<NUM>");
                stats.numerics_scrubbed += 1;
            } else {
                out.push_str(&source[start..start + len]);
            }
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    (out, stats)
}

/// Match `DATE '…'` / `TIMESTAMP '…'` / `INTERVAL '…' UNIT`.
/// Returns the inclusive end offset in bytes from the start of
/// `text`, or `None` if there is no match.
///
/// Word-boundary check: the keyword must be followed by ASCII
/// whitespace, end-of-string, or a delimiter — NOT another
/// identifier character. Without this, a user-defined identifier
/// like `DATE_HIRED('2024-01-01')` would be silently corrupted
/// into `DATE '<SCRUBBED>'` (code-review caught this; see
/// `match_date_literal_skips_identifier_with_keyword_prefix` test).
fn match_date_literal(text: &str) -> Option<usize> {
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
    // Word boundary: the char immediately after the keyword must
    // not be alphanumeric / `_` / `$` / `#` — otherwise it's part
    // of a user identifier.
    let next = after.bytes().next();
    if let Some(b) = next
        && (b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
    {
        return None;
    }
    let trimmed = after.trim_start();
    if !trimmed.starts_with('\'') {
        return None;
    }
    let lead_offset = keyword.len() + (after.len() - trimmed.len());
    let body_start = lead_offset + 1;
    let bytes = text.as_bytes();
    let mut j = body_start;
    while j < bytes.len() {
        if bytes[j] == b'\'' {
            j += 1;
            break;
        }
        j += 1;
    }
    Some(j)
}

/// Match Oracle's `q'X…Xc'` alternative-quote literal. The opening
/// delimiter `X` may be ANY single character except whitespace; for
/// `(`, `[`, `{`, `<` the closing delimiter is the mirror, otherwise
/// it is the same character. Security: handling only bracket pairs
/// (the old behavior) let `q'!secret!'`, `q'#…#'`, `q'|…|'` fall
/// through to the plain-string scanner and leak short content;
/// matching the full Oracle grammar closes that redaction gap.
/// Returns the inclusive end offset.
fn match_q_literal(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    if (bytes[0] != b'q' && bytes[0] != b'Q') || bytes[1] != b'\'' {
        return None;
    }
    let open = bytes[2];
    let close = match open {
        b'[' => b']',
        b'(' => b')',
        b'{' => b'}',
        b'<' => b'>',
        // Per Oracle: the delimiter cannot be a space, tab, or
        // newline. Any other character is a valid same-character
        // delimiter (e.g. `q'!…!'`, `q'#…#'`, `q'|…|'`).
        b' ' | b'\t' | b'\n' | b'\r' => return None,
        other => other,
    };
    let mut j = 3;
    while j + 1 < bytes.len() {
        if bytes[j] == close && bytes[j + 1] == b'\'' {
            return Some(j + 2);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_string_literal_scrubbed_by_default() {
        let src = "x := 'this is a long literal'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("'<SCRUBBED>'"));
        assert_eq!(stats.strings_scrubbed, 1);
    }

    #[test]
    fn short_string_left_alone_by_default() {
        let src = "x := 'Y'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("'Y'"));
        assert_eq!(stats.strings_scrubbed, 0);
    }

    #[test]
    fn strict_mode_scrubs_short_strings() {
        let src = "x := 'Y'";
        let (out, _) = scrub_literals(src, ScrubThresholds::strict());
        assert!(out.contains("'<SCRUBBED>'"));
    }

    #[test]
    fn long_numeric_scrubbed() {
        let src = "x := 1234567";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("<NUM>"));
        assert!(!out.contains("1234567"));
        assert_eq!(stats.numerics_scrubbed, 1);
    }

    #[test]
    fn short_numeric_left_alone() {
        let src = "FOR i IN 1..10 LOOP NULL; END LOOP;";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("1..10"));
    }

    #[test]
    fn float_with_exponent_scrubbed() {
        let src = "v := 1.5e+12";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("<NUM>"));
    }

    #[test]
    fn date_literal_scrubbed_by_default() {
        let src = "v := DATE '2024-05-15'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("DATE '<SCRUBBED>'"));
        assert!(!out.contains("2024-05-15"));
        assert_eq!(stats.date_literals_scrubbed, 1);
    }

    #[test]
    fn timestamp_literal_recognised() {
        let src = "v := TIMESTAMP '2024-05-15 09:00:00'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(!out.contains("2024"));
        assert_eq!(stats.date_literals_scrubbed, 1);
    }

    #[test]
    fn q_literal_scrubbed() {
        let src = "v := q'[hello (world) more text]'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("q'[<SCRUBBED>]'"));
        assert_eq!(stats.strings_scrubbed, 1);
    }

    #[test]
    fn unterminated_short_string_is_scrubbed_not_leaked() {
        // Security regression: a truncated/adversarial fragment with
        // an unterminated string holding a short secret must NOT be
        // emitted raw just because it is below string_min_len.
        let src = "v := 'sekret";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(!out.contains("sekret"), "secret leaked: {out}");
        assert!(out.contains("'<SCRUBBED>'"));
        assert_eq!(stats.strings_scrubbed, 1);
    }

    #[test]
    fn terminated_short_string_still_left_alone() {
        // The unterminated guard must not regress the normal
        // short-string passthrough.
        let src = "x := 'Y'";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("'Y'"));
        assert_eq!(stats.strings_scrubbed, 0);
    }

    #[test]
    fn q_literal_with_non_bracket_delimiters_scrubbed() {
        // Security regression: Oracle allows any non-space char as
        // the q-literal delimiter. Previously `!`/`#`/`|` fell
        // through to the plain-string scanner and could leak short
        // content.
        for src in [
            "v := q'!hidden secret value!'",
            "v := q'#another secret#'",
            "v := q'|piped secret value|'",
            "v := q'XsecretX'",
        ] {
            let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
            assert!(out.contains("q'[<SCRUBBED>]'"), "{src} -> {out}");
            assert!(!out.contains("secret"), "secret leaked for {src}: {out}");
            assert_eq!(stats.strings_scrubbed, 1, "{src}");
        }
    }

    #[test]
    fn q_literal_whitespace_delimiter_is_not_treated_as_q_literal() {
        // `q' ...'` with a space delimiter is not a valid Oracle
        // q-literal; it must not be matched as one (it degrades to
        // the plain-string path, which still scrubs long content).
        let src = "v := q' this is just a long normal string after q'";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(!out.contains("q'[<SCRUBBED>]'"));
    }

    #[test]
    fn doubled_apostrophe_inside_string_handled() {
        let src = "v := 'it''s fine and rather long'";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("'<SCRUBBED>'"));
        assert!(!out.contains("it''s"));
    }

    #[test]
    fn line_comment_preserved() {
        let src = "x := 1; -- and here is a longer secret literal hint";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("-- and here is a longer secret literal hint"));
    }

    #[test]
    fn block_comment_preserved() {
        let src = "x := 1; /* and a secret hint here */";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("/* and a secret hint here */"));
    }

    #[test]
    fn match_date_literal_skips_identifier_with_keyword_prefix() {
        // Regression for code-review finding: `DATE_HIRED('2024-01-01')`
        // must NOT be matched as a date literal.
        let src = "v := DATE_HIRED('2024-01-01');";
        let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("DATE_HIRED("));
        assert!(!out.contains("DATE '<SCRUBBED>'"));
        // Inner string is short → not scrubbed.
        assert_eq!(stats.date_literals_scrubbed, 0);
    }

    #[test]
    fn match_date_literal_word_boundary_for_timestamp_and_interval() {
        let cases = ["TIMESTAMP_COL('foo')", "INTERVAL_DAYS('bar')"];
        for src in cases {
            let (out, stats) = scrub_literals(src, ScrubThresholds::default_thresholds());
            assert!(out.starts_with(&src[..4]), "{src} → {out}");
            assert_eq!(stats.date_literals_scrubbed, 0, "{src}");
        }
    }

    #[test]
    fn unterminated_block_comment_with_multibyte_chars_does_not_panic() {
        // Regression for code-review finding: walking raw bytes
        // through a non-terminated comment can land `i` mid-UTF-8;
        // the slice path now copies the rest of the source
        // verbatim instead of attempting `source[start..i]` at a
        // non-char boundary.
        let src = "x := 1; /* 漢字 こんにちは and then no terminator";
        let (out, _) = scrub_literals(src, ScrubThresholds::default_thresholds());
        assert!(out.contains("漢字"));
    }

    #[test]
    fn date_literals_can_be_disabled() {
        let src = "v := DATE '2024-05-15'";
        let t = ScrubThresholds {
            date_literals_scrubbed: false,
            ..ScrubThresholds::default_thresholds()
        };
        let (out, stats) = scrub_literals(src, t);
        assert!(out.contains("2024-05-15"));
        assert_eq!(stats.date_literals_scrubbed, 0);
    }
}
