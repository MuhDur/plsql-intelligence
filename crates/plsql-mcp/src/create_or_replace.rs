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

    let kind = kind_tokens.iter().take(2).copied().collect::<Vec<_>>().join(" ");
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
    let kind = classify_kind(ddl)?;

    // Re-tokenize on whitespace runs exactly as classify_kind did, so spacing
    // cannot shift which token is the object name. `CREATE OR REPLACE` occupies
    // tokens[0..3]; the (already-validated) kind occupies the next 1-2 tokens;
    // the object name is the token that immediately follows.
    let upper = ddl.trim_start().to_ascii_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();
    let name_idx = 3 + kind.split_whitespace().count();

    let Some(name_raw) = tokens.get(name_idx) else {
        return Ok(None);
    };
    // The object name ends at the first `(` — a PROCEDURE/FUNCTION parameter list
    // may abut the name with no separating whitespace (e.g. `FOO(p IN NUMBER)`).
    let name_token = name_raw.split('(').next().unwrap_or("");
    if name_token.is_empty() {
        return Ok(None);
    }
    // `OWNER.OBJECT` ⇒ the owner is everything before the first `.`.
    match name_token.split_once('.') {
        Some((owner, object)) if !owner.is_empty() && !object.is_empty() => {
            Ok(Some(owner.to_string()))
        }
        // No dot, or a malformed dotted form — treat as unqualified.
        _ => Ok(None),
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
    fn dry_run_mints_token_and_classifies_view() {
        let mut registry = PreviewRegistry::new();
        let response =
            run_create_or_replace(&mut registry, billing_view_req(), fixed("tok-v")).unwrap();
        let CreateOrReplaceResponse::DryRun {
            token,
            object_kind,
            ddl_sha256,
            ..
        } = response
        else {
            panic!("expected DryRun");
        };
        assert_eq!(token, "tok-v");
        assert_eq!(object_kind, "VIEW");
        assert!(ddl_sha256.starts_with("sha256:"));
    }

    #[test]
    fn apply_returns_verified_bytes() {
        let mut registry = PreviewRegistry::new();
        let dry = billing_view_req();
        let _ = run_create_or_replace(&mut registry, dry.clone(), fixed("tok-a")).unwrap();
        let mut apply = dry;
        apply.mode = CreateOrReplaceMode::Apply {
            token: "tok-a".into(),
        };
        let response = run_create_or_replace(&mut registry, apply, fixed("nope")).unwrap();
        let CreateOrReplaceResponse::Apply {
            object_kind,
            ddl_bytes,
            ..
        } = response
        else {
            panic!("expected Apply");
        };
        assert_eq!(object_kind, "VIEW");
        assert!(ddl_bytes.contains("BILLING.INVOICE_SUMMARY"));
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
            let ddl = format!("CREATE OR REPLACE PACKAGE{sep}BODY billing.invoice_pkg AS BEGIN NULL; END;");
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
            parse_target_schema("create or replace view billing.v AS SELECT 1 FROM dual;")
                .unwrap(),
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
            parse_target_schema(
                "CREATE OR REPLACE PACKAGE BODY INVOICE_PKG AS\nBEGIN\nEND;"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_target_schema("create or replace function f RETURN NUMBER AS BEGIN RETURN 1; END;")
                .unwrap(),
            None
        );
    }

    #[test]
    fn parse_target_schema_rejects_non_create_or_replace() {
        assert!(parse_target_schema("DROP TABLE billing.t;").is_err());
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
