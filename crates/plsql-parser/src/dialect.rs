//! Dialect-mismatch diagnostics for the parser (PLSQL-DIALECT-003).
//!
//! When the grammar recognizes a token that maps to an `OracleFeature` that is
//! not available on the run's `OracleTargetVersion`, the parser emits a
//! `Diagnostic` with the stable code [`UNSUPPORTED_DIALECT_FEATURE_CODE`],
//! carrying an [`UnknownReason::UnsupportedDialectFeature`] tag and a
//! version-aware remediation hint.
//!
//! Per `plan.md` R13 (no uncertainty silently dropped), every dialect-mismatch
//! is a typed blind spot, never a panic.

use plsql_core::{Diagnostic, OracleFeature, OracleVersion, Severity, Span, UnknownReason};

use crate::OracleTargetVersion;

/// Stable diagnostic code emitted whenever the parser encounters an
/// `OracleFeature` token outside its target version.
pub const UNSUPPORTED_DIALECT_FEATURE_CODE: &str = "PARSE_UNSUPPORTED_DIALECT_FEATURE";

/// Returns the earliest Oracle version that supports `feature`.
///
/// Mirrors the per-version `default_features()` table in
/// `plsql_core::OracleVersion`.
#[must_use]
pub fn earliest_supporting_version(feature: OracleFeature) -> OracleVersion {
    match feature {
        OracleFeature::SqlMacros | OracleFeature::PolymorphicTableFunctions => {
            OracleVersion::Oracle21c
        }
        OracleFeature::SqlBoolean23ai
        | OracleFeature::PlsqlVector23ai
        | OracleFeature::JsonRelationalDuality23ai => OracleVersion::Oracle23ai,
        OracleFeature::BinaryVector26ai
        | OracleFeature::SparseVector26ai
        | OracleFeature::VectorArithmetic26ai
        | OracleFeature::PackageResettable26ai
        | OracleFeature::MultilingualEngineCallSpecs => OracleVersion::Oracle26ai,
    }
}

/// Human-friendly label for an `OracleFeature`, used in diagnostic messages.
#[must_use]
pub fn feature_label(feature: OracleFeature) -> &'static str {
    match feature {
        OracleFeature::SqlBoolean23ai => "SQL `BOOLEAN`",
        OracleFeature::PlsqlVector23ai => "PL/SQL `VECTOR`",
        OracleFeature::BinaryVector26ai => "`BINARY VECTOR`",
        OracleFeature::SparseVector26ai => "`SPARSE VECTOR`",
        OracleFeature::VectorArithmetic26ai => "vector arithmetic operators",
        OracleFeature::PackageResettable26ai => "package `RESETTABLE` clause",
        OracleFeature::JsonRelationalDuality23ai => "JSON relational duality",
        OracleFeature::SqlMacros => "SQL macros",
        OracleFeature::PolymorphicTableFunctions => "polymorphic table functions",
        OracleFeature::MultilingualEngineCallSpecs => "multilingual engine call specs",
    }
}

fn target_version_label(target: OracleTargetVersion) -> &'static str {
    match target {
        OracleTargetVersion::Oracle11g => "Oracle 11g",
        OracleTargetVersion::Oracle12c => "Oracle 12c",
        OracleTargetVersion::Oracle19c => "Oracle 19c",
        OracleTargetVersion::Oracle21c => "Oracle 21c",
        OracleTargetVersion::Oracle23ai => "Oracle 23ai",
        OracleTargetVersion::Oracle26ai => "Oracle 26ai",
    }
}

fn version_label(version: OracleVersion) -> &'static str {
    match version {
        OracleVersion::Oracle11g => "Oracle 11g",
        OracleVersion::Oracle12c => "Oracle 12c",
        OracleVersion::Oracle19c => "Oracle 19c",
        OracleVersion::Oracle21c => "Oracle 21c",
        OracleVersion::Oracle23ai => "Oracle 23ai",
        OracleVersion::Oracle26ai => "Oracle 26ai",
    }
}

fn target_supports_feature(target: OracleTargetVersion, feature: OracleFeature) -> bool {
    let target_version = match target {
        OracleTargetVersion::Oracle11g => OracleVersion::Oracle11g,
        OracleTargetVersion::Oracle12c => OracleVersion::Oracle12c,
        OracleTargetVersion::Oracle19c => OracleVersion::Oracle19c,
        OracleTargetVersion::Oracle21c => OracleVersion::Oracle21c,
        OracleTargetVersion::Oracle23ai => OracleVersion::Oracle23ai,
        OracleTargetVersion::Oracle26ai => OracleVersion::Oracle26ai,
    };
    target_version.default_features().contains(&feature)
}

/// Build a version-aware remediation hint for `feature` against `target`.
///
/// Examples:
/// - target 19c, feature SQL BOOLEAN → "SQL `BOOLEAN` is available in Oracle
///   23ai or later. Either upgrade the target version, or use NUMBER(1) with
///   a CHECK constraint."
/// - target 23ai, feature SPARSE VECTOR → "SPARSE VECTOR is available in
///   Oracle 26ai or later. Either upgrade the target version, or model the
///   sparse storage explicitly."
#[must_use]
pub fn unsupported_dialect_feature_remediation(
    feature: OracleFeature,
    target: OracleTargetVersion,
) -> String {
    let label = feature_label(feature);
    let earliest = earliest_supporting_version(feature);
    let earliest_label = version_label(earliest);
    let target_label = target_version_label(target);
    let mut hint = format!(
        "{label} is available in {earliest_label} or later, but the parse target is {target_label}. Either raise the `parse_options.oracle_version` to a version that supports it, or rewrite the source to avoid this construct."
    );
    if let Some(extra) = workaround_hint(feature) {
        hint.push(' ');
        hint.push_str(extra);
    }
    hint
}

/// Returns a feature-specific concrete workaround suggestion appended to the
/// version remediation, if one is known.
fn workaround_hint(feature: OracleFeature) -> Option<&'static str> {
    match feature {
        OracleFeature::SqlBoolean23ai => Some(
            "Workaround: model the column as `NUMBER(1)` with a `CHECK (col IN (0,1))` constraint.",
        ),
        OracleFeature::PlsqlVector23ai
        | OracleFeature::BinaryVector26ai
        | OracleFeature::SparseVector26ai
        | OracleFeature::VectorArithmetic26ai => Some(
            "Workaround: store the vector as a CLOB / BLOB and compute distances in PL/SQL until upgrade.",
        ),
        OracleFeature::PackageResettable26ai => Some(
            "Workaround: avoid `RESETTABLE` on the package; reset state explicitly in an initialization routine.",
        ),
        OracleFeature::JsonRelationalDuality23ai => Some(
            "Workaround: model the duality view as a regular view plus an INSTEAD OF trigger until upgrade.",
        ),
        OracleFeature::SqlMacros => Some(
            "Workaround: expand the macro manually in callers, or wrap it in a function (with a perf cost).",
        ),
        OracleFeature::PolymorphicTableFunctions => Some(
            "Workaround: write a per-shape table function until polymorphic table functions are available.",
        ),
        OracleFeature::MultilingualEngineCallSpecs => Some(
            "Workaround: use the equivalent Java / external procedure call spec for the older target.",
        ),
    }
}

/// Build a `Diagnostic` reporting that `feature` is not available on the
/// `parse_options.oracle_version` target.
///
/// Returns `None` when the feature *is* supported on the target — callers can
/// use this as both a check and a builder.
#[must_use]
pub fn unsupported_dialect_feature_diagnostic(
    feature: OracleFeature,
    target: OracleTargetVersion,
    span: Option<Span>,
) -> Option<Diagnostic> {
    if target_supports_feature(target, feature) {
        return None;
    }
    let label = feature_label(feature);
    let target_label = target_version_label(target);
    let mut diagnostic = Diagnostic::new(
        UNSUPPORTED_DIALECT_FEATURE_CODE,
        Severity::Error,
        format!("{label} is not supported when parsing against {target_label}"),
    );
    diagnostic.primary_span = span;
    diagnostic.help = Some(unsupported_dialect_feature_remediation(feature, target));
    diagnostic
        .unknown_reasons
        .push(UnknownReason::UnsupportedDialectFeature);
    Some(diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position};

    #[test]
    fn earliest_version_table_matches_oracle_version_defaults() {
        // Spot-check a few features against the core version table.
        assert_eq!(
            earliest_supporting_version(OracleFeature::SqlBoolean23ai),
            OracleVersion::Oracle23ai
        );
        assert_eq!(
            earliest_supporting_version(OracleFeature::SqlMacros),
            OracleVersion::Oracle21c
        );
        assert_eq!(
            earliest_supporting_version(OracleFeature::BinaryVector26ai),
            OracleVersion::Oracle26ai
        );
    }

    #[test]
    fn diagnostic_emitted_for_unsupported_feature_on_lower_target() {
        let span = Some(Span::new(
            FileId::new(0),
            Position::new(1, 1, 0),
            Position::new(1, 11, 10),
        ));
        let diagnostic = unsupported_dialect_feature_diagnostic(
            OracleFeature::SqlBoolean23ai,
            OracleTargetVersion::Oracle19c,
            span,
        )
        .expect("expected diagnostic for 23ai feature on 19c target");

        assert_eq!(diagnostic.code, UNSUPPORTED_DIALECT_FEATURE_CODE);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(diagnostic.message.contains("SQL `BOOLEAN`"));
        assert!(diagnostic.message.contains("Oracle 19c"));
        let help = diagnostic.help.as_deref().expect("help");
        assert!(help.contains("Oracle 23ai or later"));
        assert!(help.contains("NUMBER(1)"));
        assert_eq!(diagnostic.primary_span, span);
        assert_eq!(diagnostic.unknown_reasons.len(), 1);
        assert!(matches!(
            diagnostic.unknown_reasons[0],
            UnknownReason::UnsupportedDialectFeature
        ));
    }

    #[test]
    fn no_diagnostic_when_target_supports_feature() {
        assert!(
            unsupported_dialect_feature_diagnostic(
                OracleFeature::SqlBoolean23ai,
                OracleTargetVersion::Oracle23ai,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn vector_workarounds_consolidate_under_one_hint() {
        for feature in [
            OracleFeature::PlsqlVector23ai,
            OracleFeature::BinaryVector26ai,
            OracleFeature::SparseVector26ai,
            OracleFeature::VectorArithmetic26ai,
        ] {
            let hint =
                unsupported_dialect_feature_remediation(feature, OracleTargetVersion::Oracle19c);
            assert!(
                hint.contains("CLOB"),
                "feature {feature:?} hint missing workaround"
            );
        }
    }

    #[test]
    fn remediation_lists_earliest_version_label() {
        let hint = unsupported_dialect_feature_remediation(
            OracleFeature::PackageResettable26ai,
            OracleTargetVersion::Oracle23ai,
        );
        assert!(hint.contains("Oracle 26ai or later"));
        assert!(hint.contains("Oracle 23ai"));
        assert!(hint.contains("RESETTABLE"));
    }

    #[test]
    fn all_features_have_workaround_hints() {
        for feature in [
            OracleFeature::SqlBoolean23ai,
            OracleFeature::PlsqlVector23ai,
            OracleFeature::BinaryVector26ai,
            OracleFeature::SparseVector26ai,
            OracleFeature::VectorArithmetic26ai,
            OracleFeature::PackageResettable26ai,
            OracleFeature::JsonRelationalDuality23ai,
            OracleFeature::SqlMacros,
            OracleFeature::PolymorphicTableFunctions,
            OracleFeature::MultilingualEngineCallSpecs,
        ] {
            let hint =
                unsupported_dialect_feature_remediation(feature, OracleTargetVersion::Oracle11g);
            assert!(
                hint.to_lowercase().contains("workaround"),
                "feature {feature:?} should carry a workaround hint"
            );
        }
    }

    #[test]
    fn earliest_version_is_consistent_with_core_default_features() {
        // The pre-existing spot-check only asserts 3 literals. The
        // real invariant binding these two hand-maintained tables in
        // different crates: for EVERY feature and version,
        // `default_features(v)` contains `f` IFF `v` is at or after
        // `earliest_supporting_version(f)`. Catches drift in both
        // directions (core adds a feature to a version without
        // updating dialect.rs, or vice-versa).
        let ordered = [
            OracleVersion::Oracle11g,
            OracleVersion::Oracle12c,
            OracleVersion::Oracle19c,
            OracleVersion::Oracle21c,
            OracleVersion::Oracle23ai,
            OracleVersion::Oracle26ai,
        ];
        let features = [
            OracleFeature::SqlBoolean23ai,
            OracleFeature::PlsqlVector23ai,
            OracleFeature::BinaryVector26ai,
            OracleFeature::SparseVector26ai,
            OracleFeature::VectorArithmetic26ai,
            OracleFeature::PackageResettable26ai,
            OracleFeature::JsonRelationalDuality23ai,
            OracleFeature::SqlMacros,
            OracleFeature::PolymorphicTableFunctions,
            OracleFeature::MultilingualEngineCallSpecs,
        ];
        let idx = |v: OracleVersion| ordered.iter().position(|x| *x == v).unwrap();

        for f in features {
            let earliest = earliest_supporting_version(f);
            for &v in &ordered {
                let expected = idx(v) >= idx(earliest);
                assert_eq!(
                    v.default_features().contains(&f),
                    expected,
                    "{f:?}: default_features({v:?}).contains == {} but earliest is {earliest:?}",
                    !expected
                );
            }
        }
    }
}
