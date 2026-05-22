//! `patch_package` tool (`PLSQL-MCP-LIVE-012`).
//!
//! Targeted REPLACE-based package edit that mirrors a private-estate
//! `oracle-mcp patch_package` flow. The tool runs in two modes:
//!
//! * **dry-run** — synthesises the `CREATE OR REPLACE PACKAGE [BODY]`
//!   DDL bytes from the supplied source, mints a single-use approval
//!   token through [`PreviewRegistry::preview_sql`], and returns the
//!   token + previewed DDL for operator review. No network call is
//!   issued.
//! * **apply** — accepts a previously-minted token, verifies the
//!   supplied DDL bytes byte-for-byte against the previewed payload
//!   (`PreviewRegistry::verify_byte_for_byte`), then asks the caller
//!   to execute the DDL through their `live-db` adapter. We return
//!   the verified payload so the executor cannot accidentally run a
//!   different statement.
//!
//! The module is intentionally pure: it does not touch a live Oracle
//! handle. The actual `EXECUTE IMMEDIATE` (or equivalent driver call)
//! is the responsibility of `PLSQL-MCP-LIVE-014` (`create_or_replace`)
//! plus a future bead that wires both into the per-tool engine.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::preview::{PreviewError, PreviewRegistry, PreviewedDdl};

/// Which half of the package is being patched. The two halves are
/// stored as separate Oracle objects (the spec lives in `ALL_SOURCE`
/// with `TYPE='PACKAGE'`, the body with `TYPE='PACKAGE BODY'`), so
/// `patch_package` always targets exactly one of them per call.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePart {
    Spec,
    Body,
}

impl PackagePart {
    fn ddl_keyword(self) -> &'static str {
        match self {
            Self::Spec => "PACKAGE",
            Self::Body => "PACKAGE BODY",
        }
    }
}

/// Mode of operation for `patch_package`. `Apply` carries the
/// approval token minted by a prior `DryRun` call so the same struct
/// can be serialised across the tool surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PatchMode {
    /// First pass — preview the synthesised DDL and mint an approval token.
    DryRun,
    /// Second pass — execute the previously-previewed DDL.
    Apply { token: String },
}

/// Input descriptor for the tool. `schema` and `package` are
/// validated for the simple SQL-name shape that Oracle's
/// `DBMS_ASSERT.SIMPLE_SQL_NAME` enforces; we reject anything that
/// does not start with a letter or contains characters outside
/// `[A-Z0-9_$#]` (case-insensitive). This is the same posture used
/// across the catalog layer (R0/R3 references in plan §13A).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PatchPackageRequest {
    pub connection: String,
    pub schema: String,
    pub package: String,
    pub part: PackagePart,
    /// The complete replacement source (the package keyword + the
    /// body of the spec/body). The DDL header `CREATE OR REPLACE
    /// <part> <schema>.<package>` is synthesised by `patch_package`
    /// itself so the agent does not have to worry about that detail.
    pub source: String,
    pub mode: PatchMode,
}

/// Successful response — either the dry-run preview or the verified
/// apply payload that the executor should run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchPackageResponse {
    /// Dry-run preview: the synthesised DDL bytes plus the minted
    /// approval token (60s TTL).
    DryRun {
        token: String,
        connection: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
    /// Apply: byte-verified DDL ready for the live-DB adapter to
    /// execute. The token has not yet been consumed — the executor
    /// must call [`PreviewRegistry::consume`] on success.
    Apply {
        connection: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchPackageError {
    #[error("patch_package refused: schema name {0:?} is not a simple SQL name")]
    InvalidSchema(String),
    #[error("patch_package refused: package name {0:?} is not a simple SQL name")]
    InvalidPackage(String),
    #[error("patch_package refused: connection name is empty")]
    EmptyConnection,
    #[error("patch_package refused: replacement source is empty")]
    EmptyBody,
    #[error("patch_package preview registry error: {0}")]
    Preview(#[from] PreviewError),
}

/// Synthesise the canonical `CREATE OR REPLACE PACKAGE` (or `BODY`)
/// DDL bytes from a request. Exposed so unit tests and the live-DB
/// adapter share one source of truth for the wire format.
#[must_use]
pub fn synthesise_ddl(req: &PatchPackageRequest) -> String {
    let trimmed = req.source.trim_start_matches('\n');
    format!(
        "CREATE OR REPLACE {kw} {schema}.{package} AS\n{body}",
        kw = req.part.ddl_keyword(),
        schema = req.schema,
        package = req.package,
        body = trimmed,
    )
}

/// Run `patch_package` against the supplied [`PreviewRegistry`].
///
/// `token_factory` mints a unique opaque token for the dry-run path;
/// integration code wires this to the same source the rest of the
/// MCP server uses (typically a random UUID). Tests pass a counter
/// closure for determinism.
pub fn run_patch_package<F: FnOnce() -> String>(
    registry: &mut PreviewRegistry,
    req: PatchPackageRequest,
    token_factory: F,
) -> Result<PatchPackageResponse, PatchPackageError> {
    if req.connection.trim().is_empty() {
        return Err(PatchPackageError::EmptyConnection);
    }
    if req.source.trim().is_empty() {
        return Err(PatchPackageError::EmptyBody);
    }
    if !is_simple_sql_name(&req.schema) {
        return Err(PatchPackageError::InvalidSchema(req.schema.clone()));
    }
    if !is_simple_sql_name(&req.package) {
        return Err(PatchPackageError::InvalidPackage(req.package.clone()));
    }

    let ddl_bytes = synthesise_ddl(&req);

    match req.mode {
        PatchMode::DryRun => {
            let summary = format!(
                "patch_package {} {}.{}",
                req.part.ddl_keyword(),
                req.schema,
                req.package
            );
            let token = token_factory();
            let preview: PreviewedDdl = registry.preview_sql(
                req.connection.clone(),
                summary,
                ddl_bytes.clone(),
                token.clone(),
            )?;
            Ok(PatchPackageResponse::DryRun {
                token: preview.token,
                connection: preview.connection,
                ddl_bytes: preview.ddl_bytes,
                ddl_sha256: preview.ddl_sha256,
            })
        }
        PatchMode::Apply { token } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let verified =
                registry.verify_byte_for_byte(&token, &req.connection, &ddl_bytes, now)?;
            Ok(PatchPackageResponse::Apply {
                connection: verified.connection.clone(),
                ddl_bytes: verified.ddl_bytes.clone(),
                ddl_sha256: verified.ddl_sha256.clone(),
            })
        }
    }
}

/// Bare-bones `DBMS_ASSERT.SIMPLE_SQL_NAME` check. Accepts an
/// unquoted Oracle identifier: a letter followed by letters, digits,
/// or any of `_ $ #`. Up to 30 chars (Oracle 11g and earlier) or
/// 128 chars (12c+) — we cap at 128 since the catalog crate already
/// validates length elsewhere. Reject empty input.
fn is_simple_sql_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    let first = bytes[0];
    if !first.is_ascii_alphabetic() {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_token(s: &'static str) -> impl FnOnce() -> String {
        move || s.to_string()
    }

    fn billing_request() -> PatchPackageRequest {
        PatchPackageRequest {
            connection: "billing-dev".into(),
            schema: "BILLING".into(),
            package: "INVOICE_PKG".into(),
            part: PackagePart::Body,
            source: "BEGIN\n  NULL;\nEND INVOICE_PKG;\n".into(),
            mode: PatchMode::DryRun,
        }
    }

    #[test]
    fn dry_run_mints_token_and_returns_synthesised_ddl() {
        let mut registry = PreviewRegistry::new();
        let req = billing_request();
        let response =
            run_patch_package(&mut registry, req.clone(), fixed_token("tok-dry")).unwrap();
        let PatchPackageResponse::DryRun {
            token,
            connection,
            ddl_bytes,
            ddl_sha256,
        } = response
        else {
            panic!("expected DryRun");
        };
        assert_eq!(token, "tok-dry");
        assert_eq!(connection, "billing-dev");
        assert!(ddl_bytes.starts_with("CREATE OR REPLACE PACKAGE BODY BILLING.INVOICE_PKG AS"));
        assert!(ddl_bytes.contains("END INVOICE_PKG;"));
        assert!(ddl_sha256.starts_with("sha256:"));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn dry_run_handles_spec_part() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_request();
        req.part = PackagePart::Spec;
        req.source = "PROCEDURE x; END INVOICE_PKG;\n".into();
        let response = run_patch_package(&mut registry, req, fixed_token("tok-spec")).unwrap();
        if let PatchPackageResponse::DryRun { ddl_bytes, .. } = response {
            assert!(ddl_bytes.starts_with("CREATE OR REPLACE PACKAGE BILLING.INVOICE_PKG AS"));
            assert!(!ddl_bytes.starts_with("CREATE OR REPLACE PACKAGE BODY"));
        } else {
            panic!("expected DryRun");
        }
    }

    #[test]
    fn apply_verifies_against_previewed_bytes() {
        let mut registry = PreviewRegistry::new();
        let dry = billing_request();
        let _ = run_patch_package(&mut registry, dry.clone(), fixed_token("tok-app")).unwrap();

        let mut apply = dry;
        apply.mode = PatchMode::Apply {
            token: "tok-app".into(),
        };
        let response =
            run_patch_package(&mut registry, apply, fixed_token("never-called")).unwrap();
        let PatchPackageResponse::Apply { ddl_bytes, .. } = response else {
            panic!("expected Apply");
        };
        assert!(ddl_bytes.contains("BILLING.INVOICE_PKG"));
    }

    #[test]
    fn apply_rejects_when_source_diverged_from_preview() {
        let mut registry = PreviewRegistry::new();
        let dry = billing_request();
        let _ = run_patch_package(&mut registry, dry.clone(), fixed_token("tok-div")).unwrap();

        // Apply with a different body — byte-for-byte verify must fail.
        let mut apply = dry;
        apply.mode = PatchMode::Apply {
            token: "tok-div".into(),
        };
        apply.source = "BEGIN NULL; END INVOICE_PKG; -- changed\n".into();
        let err = run_patch_package(&mut registry, apply, fixed_token("nope")).unwrap_err();
        assert!(matches!(
            err,
            PatchPackageError::Preview(PreviewError::DdlMismatch { .. })
        ));
    }

    #[test]
    fn apply_rejects_unknown_token() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_request();
        req.mode = PatchMode::Apply {
            token: "ghost".into(),
        };
        let err = run_patch_package(&mut registry, req, fixed_token("x")).unwrap_err();
        assert!(matches!(
            err,
            PatchPackageError::Preview(PreviewError::TokenNotFound)
        ));
    }

    #[test]
    fn invalid_schema_or_package_rejected() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_request();
        req.schema = "1bad".into();
        let err = run_patch_package(&mut registry, req, fixed_token("x")).unwrap_err();
        assert!(matches!(err, PatchPackageError::InvalidSchema(_)));

        let mut req = billing_request();
        req.package = "drop;".into();
        let err = run_patch_package(&mut registry, req, fixed_token("x")).unwrap_err();
        assert!(matches!(err, PatchPackageError::InvalidPackage(_)));
    }

    #[test]
    fn empty_connection_or_body_rejected() {
        let mut registry = PreviewRegistry::new();
        let mut req = billing_request();
        req.connection = "  ".into();
        let err = run_patch_package(&mut registry, req, fixed_token("x")).unwrap_err();
        assert_eq!(err, PatchPackageError::EmptyConnection);

        let mut req = billing_request();
        req.source = "   \n".into();
        let err = run_patch_package(&mut registry, req, fixed_token("x")).unwrap_err();
        assert_eq!(err, PatchPackageError::EmptyBody);
    }

    #[test]
    fn synthesise_ddl_is_byte_stable() {
        let req = billing_request();
        let a = synthesise_ddl(&req);
        let b = synthesise_ddl(&req);
        assert_eq!(a, b);
    }

    #[test]
    fn is_simple_sql_name_matches_expected_shape() {
        assert!(is_simple_sql_name("X"));
        assert!(is_simple_sql_name("BILLING_PKG"));
        assert!(is_simple_sql_name("PKG$WITH#SIGILS"));
        assert!(!is_simple_sql_name("1BAD"));
        assert!(!is_simple_sql_name(""));
        assert!(!is_simple_sql_name("BAD;NAME"));
        assert!(!is_simple_sql_name("with space"));
    }
}

// ---------------------------------------------------------------------------
// patch_view (PLSQL-MCP-LIVE-013)
// ---------------------------------------------------------------------------

/// Input descriptor for `patch_view`. The view body (the
/// `SELECT … FROM …` text following `AS`) is supplied verbatim; the
/// `CREATE OR REPLACE VIEW <schema>.<name> AS` header is synthesised
/// for the agent, matching the patch_package convention.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PatchViewRequest {
    pub connection: String,
    pub schema: String,
    pub view: String,
    /// Replacement view body — typically `SELECT … FROM …`. The
    /// `AS` is not included; we add it.
    pub query: String,
    pub mode: PatchMode,
}

/// Successful response — same two-shape envelope as patch_package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchViewResponse {
    DryRun {
        token: String,
        connection: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
    Apply {
        connection: String,
        ddl_bytes: String,
        ddl_sha256: String,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchViewError {
    #[error("patch_view refused: schema name {0:?} is not a simple SQL name")]
    InvalidSchema(String),
    #[error("patch_view refused: view name {0:?} is not a simple SQL name")]
    InvalidView(String),
    #[error("patch_view refused: connection name is empty")]
    EmptyConnection,
    #[error("patch_view refused: replacement query is empty")]
    EmptyQuery,
    #[error("patch_view preview registry error: {0}")]
    Preview(#[from] PreviewError),
}

/// Synthesise the canonical `CREATE OR REPLACE VIEW` DDL bytes
/// from a request.
#[must_use]
pub fn synthesise_view_ddl(req: &PatchViewRequest) -> String {
    let trimmed = req.query.trim_start_matches('\n');
    format!(
        "CREATE OR REPLACE VIEW {schema}.{view} AS\n{body}",
        schema = req.schema,
        view = req.view,
        body = trimmed,
    )
}

/// Run `patch_view` against the supplied [`PreviewRegistry`]. Shape
/// matches `run_patch_package`: dry-run mints a token, apply
/// byte-for-byte verifies.
pub fn run_patch_view<F: FnOnce() -> String>(
    registry: &mut PreviewRegistry,
    req: PatchViewRequest,
    token_factory: F,
) -> Result<PatchViewResponse, PatchViewError> {
    if req.connection.trim().is_empty() {
        return Err(PatchViewError::EmptyConnection);
    }
    if req.query.trim().is_empty() {
        return Err(PatchViewError::EmptyQuery);
    }
    if !is_simple_sql_name(&req.schema) {
        return Err(PatchViewError::InvalidSchema(req.schema.clone()));
    }
    if !is_simple_sql_name(&req.view) {
        return Err(PatchViewError::InvalidView(req.view.clone()));
    }

    let ddl_bytes = synthesise_view_ddl(&req);

    match req.mode {
        PatchMode::DryRun => {
            let summary = format!("patch_view {}.{}", req.schema, req.view);
            let token = token_factory();
            let preview: PreviewedDdl =
                registry.preview_sql(req.connection.clone(), summary, ddl_bytes.clone(), token)?;
            Ok(PatchViewResponse::DryRun {
                token: preview.token,
                connection: preview.connection,
                ddl_bytes: preview.ddl_bytes,
                ddl_sha256: preview.ddl_sha256,
            })
        }
        PatchMode::Apply { token } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let verified =
                registry.verify_byte_for_byte(&token, &req.connection, &ddl_bytes, now)?;
            Ok(PatchViewResponse::Apply {
                connection: verified.connection.clone(),
                ddl_bytes: verified.ddl_bytes.clone(),
                ddl_sha256: verified.ddl_sha256.clone(),
            })
        }
    }
}

#[cfg(test)]
mod patch_view_tests {
    use super::*;

    fn fixed(t: &'static str) -> impl FnOnce() -> String {
        move || t.to_string()
    }

    fn req() -> PatchViewRequest {
        PatchViewRequest {
            connection: "billing-dev".into(),
            schema: "BILLING".into(),
            view: "INVOICE_SUMMARY".into(),
            query: "SELECT id, total FROM invoice;".into(),
            mode: PatchMode::DryRun,
        }
    }

    #[test]
    fn dry_run_mints_token_and_synthesises_view_header() {
        let mut registry = PreviewRegistry::new();
        let r = run_patch_view(&mut registry, req(), fixed("tok-v")).unwrap();
        let PatchViewResponse::DryRun {
            token,
            ddl_bytes,
            ddl_sha256,
            ..
        } = r
        else {
            panic!("expected DryRun");
        };
        assert_eq!(token, "tok-v");
        assert!(
            ddl_bytes.starts_with("CREATE OR REPLACE VIEW BILLING.INVOICE_SUMMARY AS"),
            "{ddl_bytes}"
        );
        assert!(ddl_bytes.contains("SELECT id, total FROM invoice;"));
        assert!(ddl_sha256.starts_with("sha256:"));
    }

    #[test]
    fn apply_verifies_byte_for_byte() {
        let mut registry = PreviewRegistry::new();
        let _ = run_patch_view(&mut registry, req(), fixed("tok-a")).unwrap();
        let mut apply = req();
        apply.mode = PatchMode::Apply {
            token: "tok-a".into(),
        };
        let r = run_patch_view(&mut registry, apply, fixed("nope")).unwrap();
        assert!(matches!(r, PatchViewResponse::Apply { .. }));
    }

    #[test]
    fn apply_rejects_drifted_query() {
        let mut registry = PreviewRegistry::new();
        let _ = run_patch_view(&mut registry, req(), fixed("tok-d")).unwrap();
        let mut apply = req();
        apply.mode = PatchMode::Apply {
            token: "tok-d".into(),
        };
        apply.query.push_str(" -- changed");
        let err = run_patch_view(&mut registry, apply, fixed("x")).unwrap_err();
        assert!(matches!(
            err,
            PatchViewError::Preview(PreviewError::DdlMismatch { .. })
        ));
    }

    #[test]
    fn invalid_inputs_rejected() {
        let mut registry = PreviewRegistry::new();
        let mut r = req();
        r.schema = "1bad".into();
        assert!(matches!(
            run_patch_view(&mut registry, r, fixed("x")),
            Err(PatchViewError::InvalidSchema(_))
        ));

        let mut r = req();
        r.view = "drop;".into();
        assert!(matches!(
            run_patch_view(&mut registry, r, fixed("x")),
            Err(PatchViewError::InvalidView(_))
        ));

        let mut r = req();
        r.connection = "  ".into();
        assert!(matches!(
            run_patch_view(&mut registry, r, fixed("x")),
            Err(PatchViewError::EmptyConnection)
        ));

        let mut r = req();
        r.query = "".into();
        assert!(matches!(
            run_patch_view(&mut registry, r, fixed("x")),
            Err(PatchViewError::EmptyQuery)
        ));
    }

    #[test]
    fn synthesise_view_ddl_is_byte_stable() {
        let r = req();
        assert_eq!(synthesise_view_ddl(&r), synthesise_view_ddl(&r));
    }
}
