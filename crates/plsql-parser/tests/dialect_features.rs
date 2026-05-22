//! Integration tests for PLSQL-DIALECT-002 — verify the dialect-mismatch
//! diagnostic surface covers SQL `BOOLEAN`, PL/SQL `VECTOR`, `SPARSE VECTOR`,
//! vector arithmetic, and the package `RESETTABLE` clause.
//!
//! The ANTLR grammar does not yet emit dedicated tokens for any of these
//! features (PLSQL-PARSE-* beads are still landing), so the tests here focus
//! on the diagnostic surface introduced in PLSQL-DIALECT-003:
//! `unsupported_dialect_feature_diagnostic` and
//! `unsupported_dialect_feature_remediation`. As soon as the grammar emits
//! tokens for any of these constructs the same helpers can be plugged in at
//! the lowering site without further test churn.

use plsql_core::{FileId, OracleFeature, Position, Severity, Span, UnknownReason};
use plsql_parser::{
    OracleTargetVersion, UNSUPPORTED_DIALECT_FEATURE_CODE, unsupported_dialect_feature_diagnostic,
    unsupported_dialect_feature_remediation,
};

fn fake_span() -> Span {
    Span::new(
        FileId::new(0),
        Position::new(1, 1, 0),
        Position::new(1, 8, 7),
    )
}

#[test]
fn sql_boolean_diagnostic_targets_pre_23ai_versions() {
    for target in [
        OracleTargetVersion::Oracle11g,
        OracleTargetVersion::Oracle12c,
        OracleTargetVersion::Oracle19c,
        OracleTargetVersion::Oracle21c,
    ] {
        let diagnostic = unsupported_dialect_feature_diagnostic(
            OracleFeature::SqlBoolean23ai,
            target,
            Some(fake_span()),
        )
        .expect("diagnostic should fire on a pre-23ai target");
        assert_eq!(diagnostic.code, UNSUPPORTED_DIALECT_FEATURE_CODE);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(diagnostic.message.contains("SQL `BOOLEAN`"));
        let help = diagnostic.help.as_deref().expect("help text");
        assert!(help.contains("Oracle 23ai or later"));
        assert!(help.contains("NUMBER(1)"));
        assert!(
            diagnostic
                .unknown_reasons
                .contains(&UnknownReason::UnsupportedDialectFeature)
        );
    }

    assert!(
        unsupported_dialect_feature_diagnostic(
            OracleFeature::SqlBoolean23ai,
            OracleTargetVersion::Oracle23ai,
            None,
        )
        .is_none(),
        "23ai target supports SQL BOOLEAN — no diagnostic should fire"
    );
}

#[test]
fn plsql_vector_diagnostic_targets_pre_23ai_versions() {
    let diagnostic = unsupported_dialect_feature_diagnostic(
        OracleFeature::PlsqlVector23ai,
        OracleTargetVersion::Oracle19c,
        Some(fake_span()),
    )
    .expect("diagnostic should fire");
    assert!(diagnostic.message.contains("VECTOR"));
    let help = diagnostic.help.as_deref().expect("help text");
    assert!(help.contains("Oracle 23ai or later"));
    assert!(help.contains("CLOB"));
}

#[test]
fn sparse_vector_diagnostic_targets_pre_26ai_versions() {
    for target in [
        OracleTargetVersion::Oracle19c,
        OracleTargetVersion::Oracle21c,
        OracleTargetVersion::Oracle23ai,
    ] {
        let diagnostic =
            unsupported_dialect_feature_diagnostic(OracleFeature::SparseVector26ai, target, None)
                .expect("SPARSE VECTOR requires 26ai");
        assert!(diagnostic.message.contains("SPARSE VECTOR"));
        let help = diagnostic.help.as_deref().expect("help text");
        assert!(help.contains("Oracle 26ai or later"));
        // SPARSE VECTOR shares the vector-family workaround copy.
        assert!(help.contains("CLOB"));
    }
}

#[test]
fn vector_arithmetic_diagnostic_targets_pre_26ai_versions() {
    let diagnostic = unsupported_dialect_feature_diagnostic(
        OracleFeature::VectorArithmetic26ai,
        OracleTargetVersion::Oracle23ai,
        None,
    )
    .expect("vector arithmetic requires 26ai");
    assert!(diagnostic.message.contains("vector arithmetic"));
    let help = diagnostic.help.as_deref().expect("help text");
    assert!(help.contains("Oracle 26ai or later"));
}

#[test]
fn package_resettable_diagnostic_targets_pre_26ai_versions() {
    for target in [
        OracleTargetVersion::Oracle19c,
        OracleTargetVersion::Oracle21c,
        OracleTargetVersion::Oracle23ai,
    ] {
        let diagnostic = unsupported_dialect_feature_diagnostic(
            OracleFeature::PackageResettable26ai,
            target,
            None,
        )
        .expect("RESETTABLE requires 26ai");
        assert!(diagnostic.message.contains("RESETTABLE"));
        let help = diagnostic.help.as_deref().expect("help text");
        assert!(help.contains("Oracle 26ai or later"));
        assert!(help.contains("initialization routine"));
    }
}

#[test]
fn remediation_strings_are_stable_per_feature() {
    // Same (feature, target) pair must produce byte-identical remediation
    // strings — gives lineage / SAST / CI gates a stable text surface to
    // compare against.
    let first = unsupported_dialect_feature_remediation(
        OracleFeature::SqlBoolean23ai,
        OracleTargetVersion::Oracle19c,
    );
    let second = unsupported_dialect_feature_remediation(
        OracleFeature::SqlBoolean23ai,
        OracleTargetVersion::Oracle19c,
    );
    assert_eq!(first, second);
}

#[test]
fn diagnostics_for_full_feature_matrix_are_unique() {
    // The bead's feature list, plus the underlying earliest-version table,
    // means every (feature, target) combination outside support must produce
    // a distinct (message, help) tuple — no copy-pasted text leaking across
    // features.
    let features = [
        OracleFeature::SqlBoolean23ai,
        OracleFeature::PlsqlVector23ai,
        OracleFeature::SparseVector26ai,
        OracleFeature::VectorArithmetic26ai,
        OracleFeature::PackageResettable26ai,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for feature in features {
        let diagnostic =
            unsupported_dialect_feature_diagnostic(feature, OracleTargetVersion::Oracle19c, None)
                .expect("19c target should not support any of these features");
        let key = (diagnostic.message.clone(), diagnostic.help.clone());
        assert!(
            seen.insert(key),
            "feature {feature:?} produced duplicate (message, help) — dialect copy is too generic"
        );
    }
    assert_eq!(seen.len(), features.len());
}
