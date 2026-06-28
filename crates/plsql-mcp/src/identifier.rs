//! Oracle identifier handling for MCP tool inputs.
//!
//! Oracle stores unquoted identifiers in dictionary upper-case and stores
//! double-quoted identifiers exactly as written inside the quotes. Keep that
//! rule in one place so live tools do not silently fold quoted names.

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SqlIdentifier {
    dictionary_name: String,
    sql: String,
}

impl SqlIdentifier {
    #[must_use]
    pub(crate) fn dictionary_name(&self) -> &str {
        &self.dictionary_name
    }

    #[must_use]
    pub(crate) fn sql(&self) -> &str {
        &self.sql
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum IdentifierError {
    #[error("identifier is empty")]
    Empty,
    #[error("identifier `{identifier}` exceeds Oracle's 128-byte identifier limit")]
    TooLong { identifier: String },
    #[error("unquoted identifier `{identifier}` must start with an ASCII letter")]
    InvalidUnquotedStart { identifier: String },
    #[error("unquoted identifier `{identifier}` contains illegal character `{ch}`")]
    InvalidUnquotedChar { identifier: String, ch: char },
    #[error("quoted identifier `{identifier}` is missing its closing double quote")]
    UnterminatedQuoted { identifier: String },
    #[error("quoted identifier `{identifier}` contains an unescaped double quote")]
    InvalidQuotedEscape { identifier: String },
}

/// Normalize a user-supplied identifier for Oracle dictionary equality.
///
/// Unquoted input is trimmed and folded to upper-case; `"..."` input has its
/// outer quotes stripped, `""` collapsed to `"`, and inner case preserved.
#[must_use]
pub(crate) fn normalize_identifier(raw: &str) -> String {
    let t = raw.trim();
    if let Some(inner) = quoted_inner(t) {
        inner.replace("\"\"", "\"")
    } else {
        t.to_ascii_uppercase()
    }
}

/// Parse one owner/object identifier segment for DDL interpolation.
///
/// This returns both the dictionary key and a safely rendered SQL identifier.
/// It accepts the same Oracle surface the lookup tools accept: unquoted
/// `BILLING_PKG`-style names, or quoted `"Mixed Case"` names with doubled
/// quotes as the escape.
pub(crate) fn parse_sql_identifier(raw: &str) -> Result<SqlIdentifier, IdentifierError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if trimmed.starts_with('"') {
        return parse_quoted_identifier(trimmed);
    }
    parse_unquoted_identifier(trimmed)
}

fn parse_unquoted_identifier(trimmed: &str) -> Result<SqlIdentifier, IdentifierError> {
    if trimmed.len() > 128 {
        return Err(IdentifierError::TooLong {
            identifier: trimmed.to_string(),
        });
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(IdentifierError::Empty);
    };
    if !first.is_ascii_alphabetic() {
        return Err(IdentifierError::InvalidUnquotedStart {
            identifier: trimmed.to_string(),
        });
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '#')) {
            return Err(IdentifierError::InvalidUnquotedChar {
                identifier: trimmed.to_string(),
                ch,
            });
        }
    }
    let dictionary_name = trimmed.to_ascii_uppercase();
    Ok(SqlIdentifier {
        sql: dictionary_name.clone(),
        dictionary_name,
    })
}

fn parse_quoted_identifier(trimmed: &str) -> Result<SqlIdentifier, IdentifierError> {
    let Some(inner) = quoted_inner(trimmed) else {
        return Err(IdentifierError::UnterminatedQuoted {
            identifier: trimmed.to_string(),
        });
    };
    let mut dictionary_name = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if chars.peek() == Some(&'"') {
                let _ = chars.next();
                dictionary_name.push('"');
            } else {
                return Err(IdentifierError::InvalidQuotedEscape {
                    identifier: trimmed.to_string(),
                });
            }
        } else {
            dictionary_name.push(ch);
        }
    }
    if dictionary_name.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if dictionary_name.len() > 128 {
        return Err(IdentifierError::TooLong {
            identifier: trimmed.to_string(),
        });
    }
    let sql = format!("\"{}\"", dictionary_name.replace('"', "\"\""));
    Ok(SqlIdentifier {
        dictionary_name,
        sql,
    })
}

fn quoted_inner(raw: &str) -> Option<&str> {
    raw.strip_prefix('"')?.strip_suffix('"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_identifier_folds_unquoted_and_preserves_quoted() {
        assert_eq!(normalize_identifier("billing"), "BILLING");
        assert_eq!(normalize_identifier("  Hr  "), "HR");
        assert_eq!(normalize_identifier("\"MixedCase\""), "MixedCase");
        assert_eq!(normalize_identifier("\"with\"\"quote\""), "with\"quote");
    }

    #[test]
    fn parse_sql_identifier_renders_unquoted_dictionary_name() {
        let ident = parse_sql_identifier("billing_pkg").unwrap();
        assert_eq!(ident.dictionary_name(), "BILLING_PKG");
        assert_eq!(ident.sql(), "BILLING_PKG");
    }

    #[test]
    fn parse_sql_identifier_preserves_and_escapes_quoted_name() {
        let ident = parse_sql_identifier("\"Billing\"\"Pkg\"").unwrap();
        assert_eq!(ident.dictionary_name(), "Billing\"Pkg");
        assert_eq!(ident.sql(), "\"Billing\"\"Pkg\"");
    }

    #[test]
    fn parse_sql_identifier_rejects_malformed_names() {
        assert_eq!(parse_sql_identifier(""), Err(IdentifierError::Empty));
        assert!(matches!(
            parse_sql_identifier("1START"),
            Err(IdentifierError::InvalidUnquotedStart { .. })
        ));
        assert!(matches!(
            parse_sql_identifier("BAD;DROP"),
            Err(IdentifierError::InvalidUnquotedChar { ch: ';', .. })
        ));
        assert!(matches!(
            parse_sql_identifier("\"bad\"quote\""),
            Err(IdentifierError::InvalidQuotedEscape { .. })
        ));
        assert!(matches!(
            parse_sql_identifier("\"unterminated"),
            Err(IdentifierError::UnterminatedQuoted { .. })
        ));
    }
}
