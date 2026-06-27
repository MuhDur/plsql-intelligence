//! Interactive schema-name confirmation for cross-schema writes.
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
//! The function follows Oracle dictionary identity: unquoted
//! identifiers are stored upper-case and are compared case-insensitively
//! at the prompt, while quoted identifiers are case-sensitive and must
//! be typed exactly as resolved from the verified DDL. A leading /
//! trailing whitespace strip is applied so a copy-pasted name with a
//! trailing newline still matches.

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

fn is_unquoted_dictionary_schema_name(schema: &str) -> bool {
    let mut chars = schema.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|ch| {
            ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '$' | '#')
        })
}

fn schema_prompt_value_for_target(raw: &str, target_schema: &str) -> String {
    let trimmed = raw.trim();
    if is_unquoted_dictionary_schema_name(target_schema) {
        trimmed.to_ascii_uppercase()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn schema_name_matches_target(raw: &str, target_schema: &str) -> bool {
    let target_trimmed = target_schema.trim();
    schema_prompt_value_for_target(raw, target_trimmed) == target_trimmed
}

/// Inspect a write target and decide whether the operator must
/// type the schema name to proceed.
///
/// * If `principal_schema` matches `target_schema` under the target's
///   Oracle identity semantics, returns
///   [`CrossSchemaDecision::SameSchema`] regardless of the value
///   passed in `operator_typed_schema`. The typed string is ignored
///   in that path — the operator does not have to type their own
///   schema name to act on it.
/// * Otherwise the operator's typed string is required and must
///   equal the target schema name after whitespace trim plus
///   upper-casing only when the target is an unquoted dictionary
///   name. A missing string yields [`CrossSchemaError::ConfirmationMissing`];
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

    let target_prompt = schema_prompt_value_for_target(target_trimmed, target_trimmed);
    let principal_prompt = schema_prompt_value_for_target(principal_trimmed, target_trimmed);

    if principal_prompt == target_prompt {
        return Ok(CrossSchemaConfirmation {
            confirmed: true,
            decision: CrossSchemaDecision::SameSchema {
                schema: target_prompt,
            },
        });
    }

    let Some(raw) = operator_typed_schema else {
        return Err(CrossSchemaError::ConfirmationMissing {
            target: target_prompt,
        });
    };
    let typed_prompt = schema_prompt_value_for_target(raw, target_trimmed);
    if typed_prompt != target_prompt {
        return Err(CrossSchemaError::ConfirmationMismatch {
            typed: typed_prompt,
            target: target_prompt,
        });
    }
    Ok(CrossSchemaConfirmation {
        confirmed: true,
        decision: CrossSchemaDecision::CrossSchemaConfirmed {
            principal: principal_prompt,
            target: target_prompt,
            schema_typed: typed_prompt,
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
    fn unquoted_same_schema_is_case_insensitive() {
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
        } = &conf.decision
        else {
            assert!(
                matches!(
                    conf.decision,
                    CrossSchemaDecision::CrossSchemaConfirmed { .. }
                ),
                "expected CrossSchemaConfirmed"
            );
            return;
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
    fn quoted_schema_same_schema_is_case_sensitive() {
        let conf = require_cross_schema_confirmation("lower_owner", "lower_owner", None).unwrap();
        assert!(matches!(
            conf.decision,
            CrossSchemaDecision::SameSchema { schema } if schema == "lower_owner"
        ));

        let err =
            require_cross_schema_confirmation("LOWER_OWNER", "lower_owner", None).unwrap_err();
        assert!(matches!(
            err,
            CrossSchemaError::ConfirmationMissing { target } if target == "lower_owner"
        ));
    }

    #[test]
    fn quoted_schema_confirmation_requires_exact_case_after_trim() {
        let err = require_cross_schema_confirmation("BILLING", "lower_owner", Some("LOWER_OWNER"))
            .unwrap_err();
        assert!(matches!(
            err,
            CrossSchemaError::ConfirmationMismatch { typed, target }
                if typed == "LOWER_OWNER" && target == "lower_owner"
        ));

        let conf =
            require_cross_schema_confirmation("BILLING", "lower_owner", Some(" lower_owner\n"))
                .unwrap();
        let CrossSchemaDecision::CrossSchemaConfirmed {
            principal,
            target,
            schema_typed,
        } = &conf.decision
        else {
            assert!(
                matches!(
                    conf.decision,
                    CrossSchemaDecision::CrossSchemaConfirmed { .. }
                ),
                "expected CrossSchemaConfirmed"
            );
            return;
        };
        assert_eq!(principal, "BILLING");
        assert_eq!(target, "lower_owner");
        assert_eq!(schema_typed, "lower_owner");
    }

    #[test]
    fn quoted_schema_with_non_identifier_characters_is_exact() {
        let err = require_cross_schema_confirmation("BILLING", "My Schema", Some("MY SCHEMA"))
            .unwrap_err();
        assert!(matches!(
            err,
            CrossSchemaError::ConfirmationMismatch { typed, target }
                if typed == "MY SCHEMA" && target == "My Schema"
        ));

        let conf = require_cross_schema_confirmation("BILLING", "A.B", Some("A.B")).unwrap();
        assert!(conf.confirmed);
    }

    #[test]
    fn cross_schema_rejects_misspelled_typed_name() {
        let err = require_cross_schema_confirmation("BILLING", "ANALYTICS", Some("analitycs"))
            .unwrap_err();
        assert!(matches!(
            err,
            CrossSchemaError::ConfirmationMismatch { ref typed, ref target }
                if typed == "ANALITYCS" && target == "ANALYTICS"
        ));
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
