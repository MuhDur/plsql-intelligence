//! `create_or_replace` tool (`PLSQL-MCP-LIVE-014`).
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
    let trimmed = ddl.trim_start();
    let upper = trimmed.to_ascii_uppercase();

    let after_create_or_replace = upper
        .strip_prefix("CREATE OR REPLACE ")
        .or_else(|| upper.strip_prefix("CREATE OR REPLACE\t"))
        .or_else(|| upper.strip_prefix("CREATE OR REPLACE\n"));

    let Some(rest) = after_create_or_replace else {
        let leading = upper
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        return Err(CreateOrReplaceError::NotCreateOrReplace { leading });
    };

    let rest = rest.trim_start();
    // Look for the longest supported kind prefix.
    for kind in SUPPORTED_KINDS {
        // A match requires that the next character after the kind
        // is whitespace or end-of-string — otherwise `PACKAGE` would
        // match `PACKAGES` (no such object, but the principle holds).
        if let Some(after) = rest.strip_prefix(kind)
            && (after.is_empty() || after.starts_with(|c: char| c.is_whitespace()))
        {
            return Ok((*kind).to_string());
        }
    }

    let kind = rest
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    Err(CreateOrReplaceError::UnsupportedKind { kind })
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
