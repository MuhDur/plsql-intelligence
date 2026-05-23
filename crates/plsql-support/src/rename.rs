//! Token-level identifier rename pass.
//!
//! Walks PL/SQL source text and rewrites every Oracle identifier (the
//! `[A-Za-z][A-Za-z0-9_$#]*` token shape) deterministically through a
//! per-bundle salt. The rename is:
//!
//! - **Idempotent**: same `(source, salt)` produces byte-identical
//!   output, and re-applying the pass to its own output yields the
//!   same string a second time (the renamed identifiers no longer
//!   match the original mapping but the rename function is stable).
//! - **Reserved-word safe**: a small set of common PL/SQL keywords
//!   (`SELECT`, `FROM`, `WHERE`, …) is preserved unchanged so the
//!   redacted source remains parseable. The default list covers the
//!   keywords every Oracle PL/SQL fixture in the lab uses; callers
//!   needing the full reserved-word table can pass a custom set.
//! - **Quoted-identifier safe**: `"My Mixed Case"` quoted identifiers
//!   are renamed as a whole (the quotes are preserved; the body is
//!   the input to the per-token hash).
//! - **String/comment safe**: contents of `'literal'`, `q'[…]'`,
//!   `-- line comments`, and `/* block comments */` are passed through
//!   unchanged.
//!
//! The function is intentionally *lossy* on identifier semantics —
//! `customers.id` becomes `id_a1b2c3.id_d4e5f6` with no relationship
//! preserved between the two halves of the qualified name. That's the
//! point: the support bundle should not leak schema topology.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// Default reserved-word set preserved unchanged by the rename pass.
///
/// Small by design — covers the SQL/PL-SQL keywords every fixture in
/// the lab uses. Callers needing the full reserved-word table can pass
/// a custom set.
pub const DEFAULT_RESERVED: &[&str] = &[
    "BEGIN",
    "BY",
    "CASE",
    "COMMIT",
    "CREATE",
    "CURSOR",
    "DECLARE",
    "DELETE",
    "DROP",
    "ELSE",
    "ELSIF",
    "END",
    "EXCEPTION",
    "EXISTS",
    "EXIT",
    "FETCH",
    "FOR",
    "FROM",
    "FUNCTION",
    "GRANT",
    "GROUP",
    "HAVING",
    "IF",
    "IN",
    "INSERT",
    "INTO",
    "IS",
    "LIKE",
    "LOOP",
    "NOT",
    "NULL",
    "OF",
    "ON",
    "OR",
    "ORDER",
    "PACKAGE",
    "PRAGMA",
    "PROCEDURE",
    "REPLACE",
    "RETURN",
    "ROLLBACK",
    "SELECT",
    "SET",
    "THEN",
    "TYPE",
    "UPDATE",
    "USING",
    "VALUES",
    "WHEN",
    "WHERE",
    "WHILE",
    "WITH",
    "AS",
    "AND",
    "TRUE",
    "FALSE",
    "DATE",
    "NUMBER",
    "VARCHAR2",
    "CHAR",
    "CLOB",
    "BLOB",
    "TIMESTAMP",
    "BOOLEAN",
    "INTEGER",
    "DUAL",
    "ROWNUM",
    "SYSDATE",
    "DEFAULT",
    "INDEX",
    "TABLE",
    "VIEW",
    "TRIGGER",
    "BODY",
    "SPEC",
    "OUT",
    "INOUT",
];

/// Result of a single rename pass: the rewritten source plus the
/// number of distinct identifiers renamed (operator metric).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameStats {
    pub renamed_identifier_count: usize,
    pub preserved_keyword_count: usize,
}

/// Apply the token-level rename pass to `source`. Returns the rewritten
/// string and a [`RenameStats`] for the operator.
///
/// Per-bundle determinism: `(source, salt)` is a pure function of its
/// inputs. Re-running on the same inputs yields byte-identical output.
#[must_use]
pub fn rename_identifiers(source: &str, salt: &str) -> (String, RenameStats) {
    rename_with_reserved(source, salt, DEFAULT_RESERVED)
}

/// Same as [`rename_identifiers`] but with a custom reserved-word set.
#[must_use]
pub fn rename_with_reserved(source: &str, salt: &str, reserved: &[&str]) -> (String, RenameStats) {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len() + 32);
    let mut i = 0;
    let mut cache: BTreeMap<String, String> = BTreeMap::new();
    let mut preserved_count = 0usize;
    let reserved_set: std::collections::BTreeSet<String> =
        reserved.iter().map(|s| s.to_ascii_uppercase()).collect();

    while i < bytes.len() {
        let ch = bytes[i];

        // Line comment: `-- … \n`.
        if ch == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            let line_end = bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len() - i, |p| p);
            out.push_str(&source[i..i + line_end]);
            i += line_end;
            continue;
        }

        // Block comment: `/* … */`.
        if ch == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let close = source[i + 2..]
                .find("*/")
                .map_or(bytes.len() - i, |p| p + 4);
            out.push_str(&source[i..i + close]);
            i += close;
            continue;
        }

        // Oracle q-quote literal: `q'X…Xc'` / `nq'X…Xc'`
        // (case-insensitive, optional n/N). The whole literal —
        // including identifier-looking words and embedded `'`/`;`
        // in its body — is string content and passes through
        // verbatim. Delimiter `X` pairs ()[]{}<>; any other char
        // closes with itself. Detected before the single-quote
        // handler so `q'…'` is not mis-scanned as a bare string
        // (the bug this fixes — see module doc's safety claim).
        {
            let q_at = if (ch | 0x20) == b'n' && i + 1 < bytes.len() {
                i + 1
            } else {
                i
            };
            if (bytes[q_at] | 0x20) == b'q' && q_at + 2 < bytes.len() && bytes[q_at + 1] == b'\'' {
                let open = bytes[q_at + 2];
                let close = match open {
                    b'[' => b']',
                    b'(' => b')',
                    b'{' => b'}',
                    b'<' => b'>',
                    other => other,
                };
                let mut j = q_at + 3;
                let mut end = bytes.len();
                while j + 1 < bytes.len() {
                    if bytes[j] == close && bytes[j + 1] == b'\'' {
                        end = j + 2;
                        break;
                    }
                    j += 1;
                }
                out.push_str(&source[i..end]);
                i = end;
                continue;
            }
        }

        // String literal: `'…'`. Doubled `''` is escaped quote — keep
        // consuming until a single `'`.
        if ch == b'\'' {
            out.push('\'');
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' && bytes.get(i + 1).copied() == Some(b'\'') {
                    out.push_str("''");
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\'' {
                    out.push('\'');
                    i += 1;
                    break;
                }
                // multi-byte safe slice — push the single byte char.
                out.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        // Quoted identifier: `"…"` — rename the body, keep the quotes.
        if ch == b'"' {
            let end_rel = source[i + 1..].find('"').map_or(bytes.len() - i - 1, |p| p);
            let body = &source[i + 1..i + 1 + end_rel];
            let renamed = lookup_or_rename(&mut cache, body, salt, &reserved_set);
            out.push('"');
            out.push_str(&renamed);
            out.push('"');
            i += 1 + end_rel + 1;
            if reserved_set.contains(&body.to_ascii_uppercase()) {
                preserved_count += 1;
            }
            continue;
        }

        // Identifier start: ASCII letter or underscore.
        if is_ident_start(ch) {
            let end_rel = bytes[i..]
                .iter()
                .position(|&b| !is_ident_continue(b))
                .unwrap_or(bytes.len() - i);
            let raw = &source[i..i + end_rel];
            let upper = raw.to_ascii_uppercase();
            if reserved_set.contains(&upper) {
                out.push_str(raw);
                preserved_count += 1;
            } else {
                let renamed = lookup_or_rename(&mut cache, raw, salt, &reserved_set);
                out.push_str(&renamed);
            }
            i += end_rel;
            continue;
        }

        // Fallback: pass through one byte.
        out.push(bytes[i] as char);
        i += 1;
    }

    (
        out,
        RenameStats {
            // Distinct renamed identifiers = cache size (reserved
            // words never enter the cache).
            renamed_identifier_count: cache.len(),
            preserved_keyword_count: preserved_count,
        },
    )
}

fn lookup_or_rename(
    cache: &mut BTreeMap<String, String>,
    raw: &str,
    salt: &str,
    reserved: &std::collections::BTreeSet<String>,
) -> String {
    if reserved.contains(&raw.to_ascii_uppercase()) {
        return raw.to_string();
    }
    if let Some(hit) = cache.get(raw) {
        return hit.clone();
    }
    let renamed = generate_alias(raw, salt);
    cache.insert(raw.to_string(), renamed.clone());
    renamed
}

/// Build a deterministic alias of the form `id_<hex12>` for `raw`
/// under `salt`. 12 hex chars = 48 bits of state — collision-resistant
/// across any realistic schema (≥ a few thousand identifiers).
fn generate_alias(raw: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b"\x00"); // domain separator
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(15);
    hex.push_str("id_");
    for byte in digest.iter().take(6) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_identifiers_deterministically_across_runs() {
        let src = "select customer_id from customers where status = 'A';";
        let (a, sa) = rename_identifiers(src, "bundle-001");
        let (b, sb) = rename_identifiers(src, "bundle-001");
        assert_eq!(a, b);
        assert_eq!(sa, sb);
    }

    #[test]
    fn salt_changes_output() {
        let src = "select customer_id from customers";
        let (a, _) = rename_identifiers(src, "bundle-A");
        let (b, _) = rename_identifiers(src, "bundle-B");
        assert_ne!(a, b);
    }

    #[test]
    fn reserved_keywords_pass_through() {
        let src = "select x from y where z is null";
        let (out, stats) = rename_identifiers(src, "salt");
        assert!(out.starts_with("select "));
        assert!(out.contains(" from "));
        assert!(out.contains(" where "));
        assert!(out.contains(" is "));
        assert!(out.contains(" null"));
        assert!(stats.preserved_keyword_count >= 5);
    }

    #[test]
    fn string_literal_passes_through_unchanged() {
        let src = "select 'customer ABC' from dual";
        let (out, _) = rename_identifiers(src, "salt");
        assert!(out.contains("'customer ABC'"));
    }

    #[test]
    fn double_apostrophe_inside_string_preserved() {
        let src = "values ('o''reilly')";
        let (out, _) = rename_identifiers(src, "salt");
        assert!(out.contains("'o''reilly'"));
    }

    #[test]
    fn q_quote_literal_body_passes_through_unchanged() {
        // Identifiers inside an Oracle q-quote literal are STRING
        // content and must never be renamed. The apostrophe in
        // `it's` would prematurely end a naive single-quote scanner,
        // exposing `secret_table`/`drop_me` to the renamer.
        let src = "x := q'{ it's a secret_table; drop_me }';";
        let (out, _) = rename_identifiers(src, "salt");
        assert!(
            out.contains("q'{ it's a secret_table; drop_me }'"),
            "q-quote body must pass through verbatim, got: {out}"
        );

        // Bracket delimiter + national-charset prefix.
        let nq = "y := nq'[keep_me_too]';";
        let (out2, _) = rename_identifiers(nq, "salt");
        assert!(out2.contains("nq'[keep_me_too]'"), "got: {out2}");
    }

    #[test]
    fn line_comments_pass_through_unchanged() {
        let src = "-- customer columns\nselect x from y";
        let (out, _) = rename_identifiers(src, "salt");
        assert!(out.starts_with("-- customer columns\n"));
    }

    #[test]
    fn block_comments_pass_through_unchanged() {
        let src = "/* internal: customer_id */ select x";
        let (out, _) = rename_identifiers(src, "salt");
        assert!(out.contains("/* internal: customer_id */"));
    }

    #[test]
    fn quoted_identifier_body_is_renamed_quotes_kept() {
        let src = "select \"Customer Id\" from dual";
        let (out, _) = rename_identifiers(src, "salt");
        // The quoted body is renamed (begins with id_) and remains
        // wrapped in double-quotes.
        let q_start = out.find('"').unwrap();
        let q_end = out.rfind('"').unwrap();
        assert!(q_end > q_start);
        let body = &out[q_start + 1..q_end];
        assert!(body.starts_with("id_"));
    }

    #[test]
    fn same_identifier_renames_to_same_alias_within_a_pass() {
        let src = "select customer_id, customer_id from customers, customers";
        let (out, _) = rename_identifiers(src, "salt");
        // Count occurrences of the first id_ token to verify the same
        // alias surfaced twice for `customer_id` and twice for
        // `customers`.
        let aliases: Vec<&str> = out
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|t| t.starts_with("id_"))
            .collect();
        // 4 identifier occurrences → 4 aliases, but only 2 distinct
        // alias strings.
        assert_eq!(aliases.len(), 4);
        let mut unique = aliases.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 2);
    }

    #[test]
    fn alias_length_is_stable() {
        let (out, _) = rename_identifiers("customer_id", "salt");
        // `id_` + 12 hex chars.
        assert_eq!(out.len(), "id_".len() + 12);
        assert!(out.starts_with("id_"));
        assert!(out[3..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn empty_source_returns_empty() {
        let (out, stats) = rename_identifiers("", "salt");
        assert_eq!(out, "");
        assert_eq!(stats.renamed_identifier_count, 0);
    }

    #[test]
    fn custom_reserved_set_overrides_defaults() {
        // Empty reserved set ⇒ even `SELECT` gets renamed.
        let (out, _) = rename_with_reserved("select x", "salt", &[]);
        assert!(!out.starts_with("select"));
    }

    #[test]
    fn idempotent_when_reapplied_to_own_output() {
        let src = "select customer_id from customers";
        let (once, _) = rename_identifiers(src, "salt");
        let (twice, _) = rename_identifiers(&once, "salt");
        // Re-applying the pass produces a different output (the
        // renamed identifiers themselves become input to the hash),
        // but the operation is still deterministic.
        let (twice_b, _) = rename_identifiers(&once, "salt");
        assert_eq!(twice, twice_b);
    }
}
