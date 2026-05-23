//! P1 acceptance tests for stage [A].
//!
//! These are the spec's named gates; none may be weakened to pass.

use plsql_accretion::{GapRecordEnvelope, RepairClass, capture_gaps_with_commit, is_repairable};
use plsql_core::{Diagnostic, Position, Severity, Span, UnknownReason};
use plsql_engine::AnalysisRun;

/// FileId(0) span over an arbitrary geometry. Offsets are *not*
/// source — `Span` carries no text by construction.
fn span(start_off: u32, end_off: u32, start_line: u32, end_line: u32) -> Span {
    Span::new(
        plsql_core::FileId::new(0),
        Position::new(start_line, 1, start_off),
        Position::new(end_line, 1, end_off),
    )
}

fn run_with(diags: Vec<Diagnostic>) -> AnalysisRun {
    AnalysisRun {
        parser_backend: "antlr4rust".to_string(),
        diagnostics: diags,
        ..AnalysisRun::default()
    }
}

#[test]
fn gap_record_is_deterministic() {
    let run = run_with(vec![
        Diagnostic::new("PARSE-ANTLR4RUST-001", Severity::Error, "syntax error")
            .with_primary_span(span(10, 24, 3, 3)),
        Diagnostic::new("IR_DDL_NOT_LOWERED", Severity::Info, "ddl not lowered")
            .with_primary_span(span(0, 200, 1, 9)),
    ]);

    let a = GapRecordEnvelope::new(capture_gaps_with_commit(&run, "deadbeef"));
    let b = GapRecordEnvelope::new(capture_gaps_with_commit(&run, "deadbeef"));

    let sa = a.to_robot_json().expect("serialize a");
    let sb = b.to_robot_json().expect("serialize b");
    assert_eq!(sa, sb, "same run+commit must yield byte-identical output");
    assert!(a.is_gap_record_schema());
    assert_eq!(a.envelope.payload.len(), 2);
}

#[test]
fn no_source_bytes_in_gap_record() {
    // Plant a unique secret + a realistic PII identifier in the
    // diagnostic message. (Source never reaches a GapRecord; the
    // message is the closest a real diagnostic gets to text — assert
    // even *that* cannot leak.)
    let secret = "ZZSECRETZZ";
    let pii = "customers_pii";
    let diag = Diagnostic::new(
        "PARSE-ANTLR4RUST-001",
        Severity::Error,
        format!("syntax error near {secret} in table {pii}"),
    )
    .with_primary_span(span(100, 140, 5, 5));

    let run = run_with(vec![diag]);
    let env = GapRecordEnvelope::new(capture_gaps_with_commit(&run, "cafef00d"));
    let json = env.to_robot_json().expect("serialize");

    assert!(
        !json.contains(secret),
        "I-PRIVACY VIOLATION: secret token leaked into GapRecord: {json}"
    );
    assert!(
        !json.contains(pii),
        "I-PRIVACY VIOLATION: PII identifier leaked into GapRecord: {json}"
    );
    // span_shape must be kinds/geometry markers only.
    let rec = &env.envelope.payload[0];
    for marker in &rec.span_shape {
        assert!(
            marker
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "span_shape marker {marker:?} is not a KIND class token"
        );
    }
}

#[test]
fn repair_class_mapping() {
    let grammar = Diagnostic::new("PARSE-ANTLR4RUST-001", Severity::Error, "x");
    assert_eq!(RepairClass::classify(&grammar), RepairClass::Grammar);

    let low1 = Diagnostic::new("IR_UNCLASSIFIED_DECL", Severity::Warn, "x");
    assert_eq!(RepairClass::classify(&low1), RepairClass::Lowering);

    let low2 = Diagnostic::new("IR_DDL_NOT_LOWERED", Severity::Info, "x");
    assert_eq!(RepairClass::classify(&low2), RepairClass::Lowering);

    let typed = Diagnostic::new("IR_UNCLASSIFIED_DECL", Severity::Warn, "x")
        .with_unknown_reason(UnknownReason::WrappedSource);
    // A typed UnknownReason dominates → TypedDegradation.
    assert_eq!(RepairClass::classify(&typed), RepairClass::TypedDegradation);

    let other = Diagnostic::new("SOME-OTHER-CODE", Severity::Info, "x");
    assert_eq!(RepairClass::classify(&other), RepairClass::Unrepairable);
}

#[test]
fn only_repairable_diagnostics_are_captured() {
    let run = run_with(vec![
        Diagnostic::new("PARSE-ANTLR4RUST-001", Severity::Error, "a"),
        Diagnostic::new("SOME-INFO", Severity::Info, "b"),
        Diagnostic::new("X", Severity::Warn, "c")
            .with_unknown_reason(UnknownReason::DbLinkRemoteObject),
    ]);
    let recs = capture_gaps_with_commit(&run, "00");
    assert_eq!(recs.len(), 2, "non-repairable diagnostic must be dropped");
    assert!(is_repairable(&run.diagnostics[0]));
    assert!(!is_repairable(&run.diagnostics[1]));
    assert!(is_repairable(&run.diagnostics[2]));
}

#[test]
fn unknown_reason_variant_name_only() {
    let run = run_with(vec![
        Diagnostic::new("IR_UNCLASSIFIED_DECL", Severity::Warn, "x")
            .with_unknown_reason(UnknownReason::WrappedSource),
    ]);
    let recs = capture_gaps_with_commit(&run, "0");
    assert_eq!(recs[0].unknown_reason.as_deref(), Some("WrappedSource"));
    assert_eq!(recs[0].repair_class, RepairClass::TypedDegradation);
}
