//! Interactive schema-name confirmation for cross-schema writes
//! (`PLSQL-MCP-LIVE-016`).
//!
//! When the active connection's principal schema does not match the
//! schema being written to (a "cross-schema write"), the live-DB
//! surface refuses to act on the previously-issued approval token
//! until the operator types the destination schema name verbatim.
//! This module is the pure validation layer: tools call
//! [`require_cross_schema_confirmation`] before invoking the
//! preview registry, and the live-DB executor checks the returned
//! [`CrossSchemaConfirmation`] before issuing the DDL.
//!
//! The function is intentionally case-insensitive on the schema
//! lookup (Oracle unquoted identifiers are stored upper-case in the
//! dictionary) but byte-exact on the operator's typed string after
//! normalisation. A leading / trailing whitespace strip is applied
//! so a copy-pasted name with a trailing newline still matches.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The decision returned by [`require_cross_schema_confirmation`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CrossSchemaDecision {
    /// The principal schema matches the target schema — no extra
    /// operator confirmation is required.
    SameSchema { schema: String },
    /// The principal schema differs from the target schema and the
    /// operator's typed confirmation matched the target verbatim.
    /// The executor should record `schema_typed` in the audit trail.
    CrossSchemaConfirmed {
        principal: String,
        target: String,
        schema_typed: String,
    },
}

/// Convenience structure returned to callers that prefer a plain
/// "confirmed" flag plus context. `CrossSchemaConfirmation::confirmed`
/// is `true` regardless of whether the write was same-schema or
/// confirmed cross-schema — the variant inside `decision` carries
/// the distinguishing detail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrossSchemaConfirmation {
    pub confirmed: bool,
    pub decision: CrossSchemaDecision,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrossSchemaError {
    #[error(
        "cross-schema write refused: operator must type the destination schema name ({target:?}) verbatim before this DDL can run"
    )]
    ConfirmationMissing { target: String },
    #[error(
        "cross-schema write refused: typed schema {typed:?} does not match destination schema {target:?}"
    )]
    ConfirmationMismatch { typed: String, target: String },
    #[error("cross-schema write refused: target schema name is empty")]
    EmptyTarget,
    #[error("cross-schema write refused: principal schema name is empty")]
    EmptyPrincipal,
}

/// Inspect a write target and decide whether the operator must
/// type the schema name to proceed.
///
/// * If `principal_schema` matches `target_schema` (case-insensitive
///   comparison against the Oracle-normalised upper case), returns
///   [`CrossSchemaDecision::SameSchema`] regardless of the value
///   passed in `operator_typed_schema`. The typed string is ignored
///   in that path — the operator does not have to type their own
///   schema name to act on it.
/// * Otherwise the operator's typed string is required and must
///   equal the target schema name after whitespace trim + uppercase.
///   A missing string yields [`CrossSchemaError::ConfirmationMissing`];
///   a non-matching string yields
///   [`CrossSchemaError::ConfirmationMismatch`].
///
/// Both schema names are validated to be non-empty.
pub fn require_cross_schema_confirmation(
    principal_schema: &str,
    target_schema: &str,
    operator_typed_schema: Option<&str>,
) -> Result<CrossSchemaConfirmation, CrossSchemaError> {
    let principal_trimmed = principal_schema.trim();
    if principal_trimmed.is_empty() {
        return Err(CrossSchemaError::EmptyPrincipal);
    }
    let target_trimmed = target_schema.trim();
    if target_trimmed.is_empty() {
        return Err(CrossSchemaError::EmptyTarget);
    }

    let principal_upper = principal_trimmed.to_ascii_uppercase();
    let target_upper = target_trimmed.to_ascii_uppercase();

    if principal_upper == target_upper {
        return Ok(CrossSchemaConfirmation {
            confirmed: true,
            decision: CrossSchemaDecision::SameSchema {
                schema: principal_upper,
            },
        });
    }

    let Some(raw) = operator_typed_schema else {
        return Err(CrossSchemaError::ConfirmationMissing {
            target: target_upper,
        });
    };
    let typed_normalised = raw.trim().to_ascii_uppercase();
    if typed_normalised != target_upper {
        return Err(CrossSchemaError::ConfirmationMismatch {
            typed: typed_normalised,
            target: target_upper,
        });
    }
    Ok(CrossSchemaConfirmation {
        confirmed: true,
        decision: CrossSchemaDecision::CrossSchemaConfirmed {
            principal: principal_upper,
            target: target_upper,
            schema_typed: typed_normalised,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_schema_does_not_require_typing() {
        let conf = require_cross_schema_confirmation("BILLING", "BILLING", None).unwrap();
        assert!(conf.confirmed);
        assert!(matches!(
            conf.decision,
            CrossSchemaDecision::SameSchema { schema } if schema == "BILLING"
        ));
    }

    #[test]
    fn same_schema_is_case_insensitive() {
        let conf = require_cross_schema_confirmation("billing", "BILLING", None).unwrap();
        assert!(conf.confirmed);
    }

    #[test]
    fn cross_schema_requires_typed_confirmation() {
        let err = require_cross_schema_confirmation("BILLING", "ANALYTICS", None).unwrap_err();
        assert!(matches!(
            err,
            CrossSchemaError::ConfirmationMissing { target } if target == "ANALYTICS"
        ));
    }

    #[test]
    fn cross_schema_accepts_byte_exact_typed_name() {
        let conf =
            require_cross_schema_confirmation("BILLING", "ANALYTICS", Some("ANALYTICS")).unwrap();
        let CrossSchemaDecision::CrossSchemaConfirmed {
            principal,
            target,
            schema_typed,
        } = conf.decision
        else {
            panic!("expected CrossSchemaConfirmed");
        };
        assert_eq!(principal, "BILLING");
        assert_eq!(target, "ANALYTICS");
        assert_eq!(schema_typed, "ANALYTICS");
    }

    #[test]
    fn cross_schema_accepts_typed_with_trim() {
        let conf = require_cross_schema_confirmation("BILLING", "ANALYTICS", Some("  analytics\n"))
            .unwrap();
        assert!(conf.confirmed);
    }

    #[test]
    fn cross_schema_rejects_misspelled_typed_name() {
        let err = require_cross_schema_confirmation("BILLING", "ANALYTICS", Some("analitycs"))
            .unwrap_err();
        let CrossSchemaError::ConfirmationMismatch { typed, target } = err else {
            panic!("expected ConfirmationMismatch");
        };
        assert_eq!(typed, "ANALITYCS");
        assert_eq!(target, "ANALYTICS");
    }

    #[test]
    fn empty_principal_or_target_rejected() {
        assert!(matches!(
            require_cross_schema_confirmation("", "ANALYTICS", None),
            Err(CrossSchemaError::EmptyPrincipal)
        ));
        assert!(matches!(
            require_cross_schema_confirmation("BILLING", "  ", Some("BILLING")),
            Err(CrossSchemaError::EmptyTarget)
        ));
    }
}
