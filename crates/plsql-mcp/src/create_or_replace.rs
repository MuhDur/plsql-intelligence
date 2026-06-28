//! `create_or_replace` tool.
//!
//! Full-DDL deployment surface under per-operation approval. Unlike
//! [`crate::patch`] which synthesises the `CREATE OR REPLACE PACKAGE
//! [BODY]` header for the agent, this tool accepts the complete DDL
//! verbatim — it only verifies the byte stream and shuttles it
//! through the existing preview / token machinery.
//!
//! Two modes share one entry point:
//!
//! * **dry-run** — checks the DDL parses as a CREATE OR REPLACE …
//!   shape, mints a single-use 60s-TTL approval token via
//!   [`PreviewRegistry::preview_sql`], and returns the previewed
//!   bytes for operator review.
//! * **apply** — accepts a token and the same DDL bytes; runs them
//!   through [`PreviewRegistry::verify_byte_for_byte`] and returns
//!   the verified payload for the live-DB adapter to execute.
//!
//! Recognised object kinds: PACKAGE, PACKAGE BODY, PROCEDURE,
//! FUNCTION, TRIGGER, VIEW, TYPE, TYPE BODY, SYNONYM, LIBRARY. This
//! is the same set a private-estate `execute_ddl` helper accepts; any
//! other verb is refused so a stray `DROP TABLE` cannot be smuggled
//! through this entry point.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::preview::{PreviewError, PreviewRegistry, PreviewedDdl};

/// Mode of operation. `Apply` carries the approval token minted
/// during a prior `DryRun` call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CreateOrReplaceMode {
    DryRun,
    Apply { token: String },
}

/// Input descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateOrReplaceRequest {
    pub connection: String,
    /// Operator-facing one-line summary, surfaced in the audit log
    /// and shown to the human reviewer before they spend the token.
    pub operation_summary: String,
    /// The complete DDL bytes — must begin with `CREATE OR REPLACE`
    /// (case-insensitive after leading whitespace) and name one of
    /// the supported object kinds.
    pub ddl_bytes: String,
    pub mode: CreateOrReplaceMode,
}

/// Parsed target object from a `CREATE OR REPLACE` DDL header.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateOrReplaceTarget {
    pub owner: Option<String>,
    pub object: String,
}

/// Successful response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateOrReplaceResponse {
    DryRun {
        token: String,
        connection: String,
        object_kind: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
    Apply {
        connection: String,
        object_kind: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CreateOrReplaceError {
    #[error("create_or_replace refused: connection name is empty")]
    EmptyConnection,
    #[error("create_or_replace refused: DDL bytes are empty")]
    EmptyDdl,
    #[error("create_or_replace refused: DDL must begin with `CREATE OR REPLACE`; got {leading:?}")]
    NotCreateOrReplace { leading: String },
    #[error("create_or_replace refused: object kind {kind:?} is not in the supported set")]
    UnsupportedKind { kind: String },
    #[error("create_or_replace refused: operation_summary is empty")]
    EmptySummary,
    #[error(
        "create_or_replace refused: schema-qualified object name {name:?} is malformed \
         (an empty owner or object around the `.` is ambiguous)"
    )]
    MalformedQualifiedName { name: String },
    #[error("create_or_replace preview registry error: {0}")]
    Preview(#[from] PreviewError),
}

/// The set of CREATE OR REPLACE targets this entry point allows.
///
/// `TYPE` and `TYPE BODY` are listed separately so the matcher can
/// resolve the longer phrase first (otherwise `TYPE BODY` would
/// false-match `TYPE` and lose the suffix). `PACKAGE BODY` is
/// handled the same way.
const SUPPORTED_KINDS: &[&str] = &[
    "PACKAGE BODY",
    "PACKAGE",
    "TYPE BODY",
    "TYPE",
    "PROCEDURE",
    "FUNCTION",
    "TRIGGER",
    "VIEW",
    "SYNONYM",
    "LIBRARY",
];

/// Run `create_or_replace` against the supplied [`PreviewRegistry`].
pub fn run_create_or_replace<F: FnOnce() -> String>(
    registry: &mut PreviewRegistry,
    req: CreateOrReplaceRequest,
    token_factory: F,
) -> Result<CreateOrReplaceResponse, CreateOrReplaceError> {
    if req.connection.trim().is_empty() {
        return Err(CreateOrReplaceError::EmptyConnection);
    }
    if req.ddl_bytes.trim().is_empty() {
        return Err(CreateOrReplaceError::EmptyDdl);
    }
    if req.operation_summary.trim().is_empty() {
        return Err(CreateOrReplaceError::EmptySummary);
    }

    let kind = classify_kind(&req.ddl_bytes)?;

    match req.mode {
        CreateOrReplaceMode::DryRun => {
            let token = token_factory();
            let preview: PreviewedDdl = registry.preview_sql(
                req.connection.clone(),
                req.operation_summary.clone(),
                req.ddl_bytes.clone(),
                token.clone(),
            )?;
            Ok(CreateOrReplaceResponse::DryRun {
                token: preview.token,
                connection: preview.connection,
                object_kind: kind,
                ddl_bytes: preview.ddl_bytes,
                ddl_sha256: preview.ddl_sha256,
            })
        }
        CreateOrReplaceMode::Apply { token } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let verified =
                registry.verify_byte_for_byte(&token, &req.connection, &req.ddl_bytes, now)?;
            Ok(CreateOrReplaceResponse::Apply {
                connection: verified.connection.clone(),
                object_kind: kind,
                ddl_bytes: verified.ddl_bytes.clone(),
                ddl_sha256: verified.ddl_sha256.clone(),
            })
        }
    }
}

/// Inspect the leading tokens of a DDL string to confirm it is
/// `CREATE OR REPLACE <kind>` and return the canonical `<kind>`
/// label. Public so tests and the audit module can share the same
/// classifier.
pub fn classify_kind(ddl: &str) -> Result<String, CreateOrReplaceError> {
    // Tokenize the leading header on whitespace RUNS so arbitrary spacing
    // (multiple spaces, tabs, newlines) between keywords cannot change the
    // classification. Oracle collapses any whitespace run to a single separator;
    // matching on a literal single space let `CREATE OR REPLACE PACKAGE<TAB>BODY
    // OWNER.PKG` fail the `PACKAGE BODY` prefix, fall through to `PACKAGE`, and
    // drop the BODY suffix — which then (via parse_target_schema) lost the OWNER
    // and bypassed the cross-schema write-confirmation guard.
    let upper = ddl.trim_start().to_ascii_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();

    if tokens.len() < 3 || tokens[0] != "CREATE" || tokens[1] != "OR" || tokens[2] != "REPLACE" {
        let leading = tokens.iter().take(3).copied().collect::<Vec<_>>().join(" ");
        return Err(CreateOrReplaceError::NotCreateOrReplace { leading });
    }

    // The kind is the 1-2 tokens after `CREATE OR REPLACE`. Try the two-word form
    // first (PACKAGE BODY / TYPE BODY) so the suffix is never truncated, then the
    // single-word form. Robust regardless of SUPPORTED_KINDS ordering.
    let kind_tokens = &tokens[3..];
    for take in [2usize, 1] {
        if kind_tokens.len() >= take {
            let candidate = kind_tokens[..take].join(" ");
            if SUPPORTED_KINDS.contains(&candidate.as_str()) {
                return Ok(candidate);
            }
        }
    }

    let kind = kind_tokens
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    Err(CreateOrReplaceError::UnsupportedKind { kind })
}

/// Parse the owner schema named in a `CREATE OR REPLACE … <schema>.<name>`
/// DDL header.
///
/// Returns `Ok(Some(schema))` when the object name is schema-qualified
/// (`OWNER.OBJECT`), `Ok(None)` when it is unqualified (the DDL targets
/// the current schema), and an error when the input is not a recognised
/// `CREATE OR REPLACE <kind>` shape. The returned schema is upper-cased
/// to match Oracle's dictionary normalisation of unquoted identifiers.
///
/// Used by `execute_approved` to derive the cross-schema
/// guard's `target_schema` from the byte-verified DDL rather than an
/// unverified caller-supplied field. `TRIGGER` / `VIEW` headers may
/// carry extra clauses, but the object name still immediately follows
/// the kind keyword, so the same head-token scan applies.
pub fn parse_target_schema(ddl: &str) -> Result<Option<String>, CreateOrReplaceError> {
    Ok(parse_target_object(ddl)?.owner)
}

/// Parse the target object named in a `CREATE OR REPLACE … [schema.]name`
/// DDL header.
pub fn parse_target_object(ddl: &str) -> Result<CreateOrReplaceTarget, CreateOrReplaceError> {
    let kind = classify_kind(ddl)?;

    // `CREATE OR REPLACE` occupies the first 3 whitespace-delimited tokens; the
    // (already-validated) kind occupies the next 1-2. The object name is the
    // token region that immediately follows. We must walk the ORIGINAL ddl
    // (not an upper-cased copy) so a quoted owner's case survives, and scan it
    // with full Oracle double-quoted-identifier awareness so embedded
    // whitespace / dots inside a `"..."` span are not mistaken for a token or
    // qualifier boundary.
    let skip_tokens = 3 + kind.split_whitespace().count();
    let Some(region_start) = nth_token_offset(ddl, skip_tokens) else {
        // No object-name token at all — genuinely unqualified.
        return Err(CreateOrReplaceError::MalformedQualifiedName {
            name: String::new(),
        });
    };

    // oracle-j1ep.1: extract the object-name region honouring Oracle
    // double-quoted identifiers. Inside a `"..."` span, whitespace and dots are
    // literal name content and `""` is an escaped embedded quote; the name only
    // ends at the first UNQUOTED whitespace or `(` (a PROCEDURE/FUNCTION param
    // list may abut the name with no separating whitespace, e.g. `FOO(p ...)`).
    // An unterminated quote is malformed — fail CLOSED.
    //
    // oracle-rwjl.8: the qualifier dot may be surrounded by whitespace
    // (`OWNER . OBJECT`); Oracle treats that as the same name `OWNER.OBJECT`,
    // so unquoted whitespace that merely hugs an unquoted dot does not end the
    // region. We therefore split into segments on UNQUOTED dots first, allowing
    // surrounding whitespace to be trimmed away per segment.
    let segments = match scan_qualified_name(&ddl[region_start..]) {
        Some(segments) => segments,
        None => {
            // Unterminated quote in the name region.
            return Err(CreateOrReplaceError::MalformedQualifiedName {
                name: ddl[region_start..].trim_start().to_string(),
            });
        }
    };

    match segments.as_slice() {
        // Genuinely unqualified — targets the current/principal schema.
        [object] if !object.is_empty() => Ok(CreateOrReplaceTarget {
            owner: None,
            object: object.clone(),
        }),
        // Well-formed `OWNER.OBJECT`: two non-empty segments.
        [owner, object] if !owner.is_empty() && !object.is_empty() => Ok(CreateOrReplaceTarget {
            owner: Some(owner.clone()),
            object: object.clone(),
        }),
        // `OWNER.`, `.OBJECT`, `OWNER..OBJECT`, `A.B.C`, or empty: ambiguous /
        // malformed. Fail CLOSED rather than route to the principal schema and
        // skip the operator-typed cross-schema confirmation.
        _ => Err(CreateOrReplaceError::MalformedQualifiedName {
            name: segments.join("."),
        }),
    }
}

/// Byte offset of the `n`-th whitespace-delimited token (0-based) in `s`, or
/// `None` if `s` has fewer than `n + 1` tokens. Used to locate the start of the
/// object-name region in the ORIGINAL DDL after skipping the
/// `CREATE OR REPLACE <kind>` head tokens, so the name's original case (which
/// matters for quoted identifiers) is preserved.
fn nth_token_offset(s: &str, n: usize) -> Option<usize> {
    let mut word_index = 0usize;
    let mut in_token = false;
    for (idx, ch) in s.char_indices() {
        if ch.is_whitespace() {
            in_token = false;
        } else {
            if !in_token {
                if word_index == n {
                    return Some(idx);
                }
                word_index += 1;
                in_token = true;
            }
        }
    }
    None
}

/// Scan the object-name region at the start of `region` (the slice beginning at
/// the object name) and split it into its dot-separated segments, honouring
/// Oracle double-quoted-identifier syntax.
///
/// While inside a `"..."` span, whitespace and dots are literal name content
/// and `""` is an escaped embedded quote. The name region ends at the first
/// UNQUOTED whitespace or `(`. Each segment is normalised per Oracle dictionary
/// rules: a quoted segment is unquoted (surrounding `"` stripped, `""` → `"`)
/// with its case preserved; an unquoted segment is upper-cased.
///
/// Returns the list of normalised segments, or `None` when a double quote is
/// left unterminated (a malformed name the caller must fail closed on).
/// Whitespace that merely hugs an UNQUOTED qualifier dot is trimmed away so
/// `OWNER . OBJECT` still yields `["OWNER", "OBJECT"]`.
fn scan_qualified_name(region: &str) -> Option<Vec<String>> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    // Did the segment currently being built come (at least partly) from a
    // quoted span? Quoted segments preserve case; unquoted ones are upper-cased.
    let mut current_quoted = false;
    let mut in_quote = false;

    let mut chars = region.char_indices().peekable();
    while let Some((_idx, ch)) = chars.next() {
        if in_quote {
            if ch == '"' {
                // `""` inside a quote is an escaped quote: keep one `"`.
                if matches!(chars.peek(), Some((_, '"'))) {
                    current.push('"');
                    chars.next();
                } else {
                    in_quote = false;
                }
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' => {
                in_quote = true;
                current_quoted = true;
            }
            // Unquoted dot: segment boundary. Push the finished segment, start
            // the next one, and consume any whitespace run hugging the dot's
            // right-hand side (`OWNER. OBJECT`) so it is not mistaken for the
            // end of the name region.
            '.' => {
                segments.push(normalise_segment(&current, current_quoted));
                current.clear();
                current_quoted = false;
                while matches!(chars.peek(), Some((_, c)) if c.is_whitespace()) {
                    chars.next();
                }
            }
            // Unquoted whitespace ends the name region UNLESS it merely hugs an
            // unquoted dot. Peek past the run: if the next non-whitespace char
            // is a dot, the whitespace is part of a spaced qualifier and is
            // skipped (the dot itself is left for the `.` arm to handle on the
            // next iteration); otherwise the name region is complete.
            c if c.is_whitespace() => {
                if skip_ws_before_dot(&mut chars) {
                    continue;
                }
                break;
            }
            // Unquoted `(` abuts a parameter list — the name ends here.
            '(' => break,
            other => current.push(other),
        }
    }

    if in_quote {
        // Unterminated quote — malformed.
        return None;
    }

    // Flush the final segment. A trailing unquoted dot (`OWNER.`) leaves an
    // empty final segment, which the caller treats as malformed.
    segments.push(normalise_segment(&current, current_quoted));
    Some(segments)
}

/// Normalise one identifier segment: a quoted segment keeps its exact case; an
/// unquoted one is upper-cased and trimmed of the whitespace that may have
/// hugged a spaced qualifier dot.
fn normalise_segment(raw: &str, quoted: bool) -> String {
    if quoted {
        // The segment text was already accumulated without its surrounding
        // quotes and with `""` collapsed to `"`; preserve case verbatim. A
        // quoted segment is never trimmed (whitespace inside `"..."` is part of
        // the identifier).
        raw.to_string()
    } else {
        raw.trim().to_ascii_uppercase()
    }
}

/// Peek across a run of unquoted whitespace (the current char was whitespace
/// and already consumed) and report whether the next non-whitespace char is an
/// unquoted dot. When it is, the iterator is advanced to *just before* that dot
/// (the whitespace run is consumed, the dot is left for the caller's `.` arm to
/// turn into a segment boundary). Otherwise the iterator is left untouched so
/// the caller can stop at the name boundary.
fn skip_ws_before_dot(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> bool {
    // Look ahead on a clone so a non-dot outcome leaves the real cursor put.
    let mut lookahead = chars.clone();
    while matches!(lookahead.peek(), Some((_, c)) if c.is_whitespace()) {
        lookahead.next();
    }
    if matches!(lookahead.peek(), Some((_, '.'))) {
        // Commit: consume only the whitespace run, leaving the dot in place.
        *chars = lookahead;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(t: &'static str) -> impl FnOnce() -> String {
        move || t.to_string()
    }

    fn billing_view_req() -> CreateOrReplaceRequest {
        CreateOrReplaceRequest {
            connection: "billing-dev".into(),
            operation_summary: "replace view billing.invoice_summary".into(),
            ddl_bytes: "CREATE OR REPLACE VIEW BILLING.INVOICE_SUMMARY AS SELECT id FROM invoice;"
                .into(),
            mode: CreateOrReplaceMode::DryRun,
        }
    }

    #[test]
    fn dry_run_mints_token_and_classifies_view() -> Result<(), String> {
        let mut registry = PreviewRegistry::new();
        let response = run_create_or_replace(&mut registry, billing_view_req(), fixed("tok-v"))
            .map_err(|err| err.to_string())?;
        let CreateOrReplaceResponse::DryRun {
            token,
            object_kind,
            ddl_sha256,
            ..
        } = response
        else {
            return Err(String::from("expected DryRun"));
        };
        assert_eq!(token, "tok-v");
        assert_eq!(object_kind, "VIEW");
        assert!(ddl_sha256.starts_with("sha256:"));
        Ok(())
    }

    #[test]
    fn apply_returns_verified_bytes() -> Result<(), String> {
        let mut registry = PreviewRegistry::new();
        let dry = billing_view_req();
        let _ = run_create_or_replace(&mut registry, dry.clone(), fixed("tok-a"))
            .map_err(|err| err.to_string())?;
        let mut apply = dry;
        apply.mode = CreateOrReplaceMode::Apply {
            token: "tok-a".into(),
        };
        let response = run_create_or_replace(&mut registry, apply, fixed("nope"))
            .map_err(|err| err.to_string())?;
        let CreateOrReplaceResponse::Apply {
            object_kind,
            ddl_bytes,
            ..
        } = response
        else {
            return Err(String::from("expected Apply"));
        };
        assert_eq!(object_kind, "VIEW");
        assert!(ddl_bytes.contains("BILLING.INVOICE_SUMMARY"));
        Ok(())
    }

    #[test]
    fn apply_rejects_diverged_bytes() {
        let mut registry = PreviewRegistry::new();
        let dry = billing_view_req();
        let _ = run_create_or_replace(&mut registry, dry.clone(), fixed("tok-d")).unwrap();
        let mut apply = dry;
        apply.mode = CreateOrReplaceMode::Apply {
            token: "tok-d".into(),
        };
        apply.ddl_bytes.push_str(" -- drift");
        let err = run_create_or_replace(&mut registry, apply, fixed("x")).unwrap_err();
        assert!(matches!(
            err,
            CreateOrReplaceError::Preview(PreviewError::DdlMismatch { .. })
        ));
    }

    #[test]
    fn rejects_non_create_or_replace_input() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_view_req();
        req.ddl_bytes = "DROP TABLE billing.invoice_summary;".into();
        let err = run_create_or_replace(&mut registry, req, fixed("x")).unwrap_err();
        assert!(matches!(
            err,
            CreateOrReplaceError::NotCreateOrReplace { .. }
        ));
    }

    #[test]
    fn rejects_unsupported_kind() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_view_req();
        // CONTEXT is a real Oracle CREATE form but outside the supported set.
        req.ddl_bytes = "CREATE OR REPLACE CONTEXT my_ctx USING my_pkg;".into();
        let err = run_create_or_replace(&mut registry, req, fixed("x")).unwrap_err();
        assert!(matches!(err, CreateOrReplaceError::UnsupportedKind { .. }));
    }

    #[test]
    fn empty_inputs_rejected() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_view_req();
        req.connection = "  ".into();
        assert_eq!(
            run_create_or_replace(&mut registry, req, fixed("x")).unwrap_err(),
            CreateOrReplaceError::EmptyConnection
        );

        let mut req = billing_view_req();
        req.ddl_bytes = "  \n".into();
        assert_eq!(
            run_create_or_replace(&mut registry, req, fixed("x")).unwrap_err(),
            CreateOrReplaceError::EmptyDdl
        );

        let mut req = billing_view_req();
        req.operation_summary = "".into();
        assert_eq!(
            run_create_or_replace(&mut registry, req, fixed("x")).unwrap_err(),
            CreateOrReplaceError::EmptySummary
        );
    }

    #[test]
    fn classifies_package_body_longest_match_first() {
        let kind =
            classify_kind("CREATE OR REPLACE PACKAGE BODY billing.invoice_pkg AS\nBEGIN\nEND;")
                .unwrap();
        assert_eq!(kind, "PACKAGE BODY");
    }

    #[test]
    fn classifies_type_body_longest_match_first() {
        let kind =
            classify_kind("CREATE OR REPLACE TYPE BODY billing.invoice_t AS\nBEGIN\nEND;").unwrap();
        assert_eq!(kind, "TYPE BODY");
    }

    #[test]
    fn two_word_kind_survives_arbitrary_whitespace() {
        // Regression: extra/non-space whitespace between the two kind words must
        // NOT truncate `PACKAGE BODY` / `TYPE BODY` to `PACKAGE` / `TYPE`. Before
        // the whitespace-run tokenization, a tab or double space dropped the BODY
        // suffix and (via parse_target_schema) the owner — a cross-schema bypass.
        for sep in ["  ", "\t", "\n", " \t ", "\u{0c}"] {
            let ddl = format!(
                "CREATE OR REPLACE PACKAGE{sep}BODY billing.invoice_pkg AS BEGIN NULL; END;"
            );
            assert_eq!(
                classify_kind(&ddl).unwrap(),
                "PACKAGE BODY",
                "PACKAGE{sep:?}BODY must classify as PACKAGE BODY"
            );
            // …and the owner must still be extracted (the bypass is the schema loss).
            assert_eq!(
                parse_target_schema(&ddl).unwrap(),
                Some("BILLING".to_string()),
                "owner must survive PACKAGE{sep:?}BODY spacing (no cross-schema bypass)"
            );
        }
        // Likewise extra spacing before the schema-qualified name must not drop it.
        let ddl = "CREATE OR REPLACE TYPE BODY   acct.balance_t AS BEGIN NULL; END;";
        assert_eq!(classify_kind(ddl).unwrap(), "TYPE BODY");
        assert_eq!(parse_target_schema(ddl).unwrap(), Some("ACCT".to_string()));
    }

    #[test]
    fn classifies_each_supported_kind() {
        for kind in [
            "PACKAGE",
            "PROCEDURE",
            "FUNCTION",
            "TRIGGER",
            "VIEW",
            "TYPE",
            "SYNONYM",
            "LIBRARY",
        ] {
            let ddl = format!("create or replace {} foo AS SELECT 1 FROM dual;", kind);
            let got = classify_kind(&ddl).unwrap();
            assert_eq!(got, kind);
        }
    }

    #[test]
    fn parse_target_schema_extracts_qualified_owner() {
        // oracle-jy0w: the owner schema is parsed straight from the
        // verified DDL header, upper-cased.
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE PACKAGE BODY ANALYTICS.INVOICE_PKG AS\nBEGIN\nEND;"
            )
            .unwrap(),
            Some("ANALYTICS".to_string())
        );
        assert_eq!(
            parse_target_schema("create or replace view billing.v AS SELECT 1 FROM dual;").unwrap(),
            Some("BILLING".to_string())
        );
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE PROCEDURE ops.do_it(p IN NUMBER) AS BEGIN NULL; END;"
            )
            .unwrap(),
            Some("OPS".to_string())
        );
    }

    #[test]
    fn parse_target_schema_returns_none_for_unqualified() {
        assert_eq!(
            parse_target_schema("CREATE OR REPLACE PACKAGE BODY INVOICE_PKG AS\nBEGIN\nEND;")
                .unwrap(),
            None
        );
        assert_eq!(
            parse_target_schema(
                "create or replace function f RETURN NUMBER AS BEGIN RETURN 1; END;"
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn parse_target_schema_rejects_non_create_or_replace() {
        assert!(parse_target_schema("DROP TABLE billing.t;").is_err());
    }

    #[test]
    fn parse_target_schema_normalises_whitespace_around_qualifier_dot() {
        // oracle-rwjl.8: Oracle treats `OWNER . OBJECT`, `OWNER .OBJECT`, and
        // `OWNER. OBJECT` as the same qualified name `OWNER.OBJECT`. A per-token
        // scan that looked only at the token immediately after the kind saw a
        // bare `ANALYTICS` (no dot) and misclassified the write as same-schema,
        // silently skipping the operator-typed cross-schema confirmation while
        // Oracle still routed the object to ANALYTICS. All spaced forms must
        // now yield the owner.
        for spaced in [
            "CREATE OR REPLACE PACKAGE BODY ANALYTICS . PKG AS BEGIN NULL; END;",
            "CREATE OR REPLACE PACKAGE BODY ANALYTICS .PKG AS BEGIN NULL; END;",
            "CREATE OR REPLACE PACKAGE BODY ANALYTICS. PKG AS BEGIN NULL; END;",
        ] {
            assert_eq!(
                parse_target_schema(spaced).unwrap(),
                Some("ANALYTICS".to_string()),
                "spaced qualifier {spaced:?} must resolve the owner"
            );
        }
        // The same normalisation must happen BEFORE the `(` cut so a
        // PROCEDURE parameter list cannot hide the spaced dot.
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE PROCEDURE OPS . DO_IT(p IN NUMBER) AS BEGIN NULL; END;"
            )
            .unwrap(),
            Some("OPS".to_string())
        );
    }

    #[test]
    fn parse_target_schema_fails_closed_on_malformed_qualified_name() {
        // A dot is present but the owner or object around it is empty, or the
        // name has more than two parts. Treating any of these as unqualified
        // would route it to the principal schema and skip the cross-schema
        // confirmation, so it must fail CLOSED instead.
        for malformed in [
            // Leading dot — empty owner.
            "CREATE OR REPLACE VIEW .V AS SELECT 1 FROM dual;",
            // Leading dot with the qualifier space normalised away.
            "CREATE OR REPLACE VIEW . V AS SELECT 1 FROM dual;",
            // Double dot — empty middle segment.
            "CREATE OR REPLACE PACKAGE BODY ANALYTICS..PKG AS BEGIN NULL; END;",
            // Three-part name — not a valid CREATE OR REPLACE target.
            "CREATE OR REPLACE VIEW A.B.C AS SELECT 1 FROM dual;",
        ] {
            assert!(
                matches!(
                    parse_target_schema(malformed),
                    Err(CreateOrReplaceError::MalformedQualifiedName { .. })
                ),
                "malformed qualified name {malformed:?} must fail closed, got {:?}",
                parse_target_schema(malformed)
            );
        }
    }

    #[test]
    fn parse_target_schema_resolves_quoted_owner_with_embedded_whitespace() {
        // oracle-j1ep.1: a quoted owner whose identifier contains embedded
        // whitespace (`"My Schema".PKG`) must NOT be misclassified as
        // unqualified. The old split_whitespace pipeline tokenised the quoted
        // owner into `"MY` / `SCHEMA".PKG`, saw `"MY` (no dot), and returned
        // Ok(None) — routing the write to the principal schema and waving the
        // cross-schema typed-confirmation gate through (a fail-open clone of
        // the unquoted-spaced / tab forms already fixed for rwjl.8).
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE PACKAGE BODY \"My Schema\".PKG AS BEGIN NULL; END;"
            )
            .unwrap(),
            Some("My Schema".to_string()),
            "quoted owner with embedded space must resolve (case preserved), not fail open"
        );
        // PROCEDURE form where the param list abuts the quoted-owner name.
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE PROCEDURE \"My Schema\".DO_IT(p IN NUMBER) AS BEGIN NULL; END;"
            )
            .unwrap(),
            Some("My Schema".to_string())
        );
        // Embedded dot inside the quoted owner is literal name content, not a
        // qualifier — still a single owner segment, not a three-part name.
        assert_eq!(
            parse_target_schema("CREATE OR REPLACE VIEW \"A.B\".V AS SELECT 1 FROM dual;").unwrap(),
            Some("A.B".to_string())
        );
    }

    #[test]
    fn parse_target_schema_preserves_quoted_segment_case_and_unescapes() {
        // A quoted identifier is case-sensitive in Oracle; the resolved owner
        // must keep its exact case rather than the dictionary upper-case used
        // for unquoted names.
        assert_eq!(
            parse_target_schema("CREATE OR REPLACE VIEW \"lower_owner\".V AS SELECT 1 FROM dual;")
                .unwrap(),
            Some("lower_owner".to_string())
        );
        // `""` inside a quoted identifier is one escaped embedded double quote.
        assert_eq!(
            parse_target_schema(
                "CREATE OR REPLACE VIEW \"Weird\"\"Owner\".V AS SELECT 1 FROM dual;"
            )
            .unwrap(),
            Some("Weird\"Owner".to_string())
        );
        // An unquoted owner stays upper-cased (dictionary normalisation).
        assert_eq!(
            parse_target_schema("CREATE OR REPLACE VIEW billing.v AS SELECT 1 FROM dual;").unwrap(),
            Some("BILLING".to_string())
        );
    }

    #[test]
    fn parse_target_schema_quoted_unqualified_is_none() {
        // A quoted name with NO qualifier dot targets the current schema —
        // Ok(None), not a malformed name.
        assert_eq!(
            parse_target_schema("CREATE OR REPLACE VIEW \"My View\" AS SELECT 1 FROM dual;")
                .unwrap(),
            None
        );
    }

    #[test]
    fn parse_target_schema_fails_closed_on_unterminated_quote() {
        // An unterminated double quote in the object-name region is malformed;
        // it must fail CLOSED rather than swallow the rest of the header as a
        // single (quoted) owner and route past the cross-schema gate.
        assert!(
            matches!(
                parse_target_schema("CREATE OR REPLACE VIEW \"My Schema.V AS SELECT 1 FROM dual;"),
                Err(CreateOrReplaceError::MalformedQualifiedName { .. })
            ),
            "unterminated quote must fail closed, got {:?}",
            parse_target_schema("CREATE OR REPLACE VIEW \"My Schema.V AS SELECT 1 FROM dual;")
        );
    }

    #[test]
    fn parse_target_schema_spaced_quoted_qualifier_dot() {
        // The spaced-qualifier normalisation (rwjl.8) and quoted-identifier
        // awareness (j1ep.1) must compose: a quoted owner followed by a spaced
        // dot still resolves the owner.
        for spaced in [
            "CREATE OR REPLACE PACKAGE BODY \"My Schema\" . PKG AS BEGIN NULL; END;",
            "CREATE OR REPLACE PACKAGE BODY \"My Schema\" .PKG AS BEGIN NULL; END;",
            "CREATE OR REPLACE PACKAGE BODY \"My Schema\". PKG AS BEGIN NULL; END;",
        ] {
            assert_eq!(
                parse_target_schema(spaced).unwrap(),
                Some("My Schema".to_string()),
                "spaced quoted qualifier {spaced:?} must resolve the owner"
            );
        }
    }

    #[test]
    fn classify_kind_is_case_insensitive() {
        assert_eq!(
            classify_kind("create or replace view foo AS SELECT 1 FROM dual;").unwrap(),
            "VIEW"
        );
        assert_eq!(
            classify_kind("Create Or Replace Package Body foo AS BEGIN NULL; END;").unwrap(),
            "PACKAGE BODY"
        );
    }
}
