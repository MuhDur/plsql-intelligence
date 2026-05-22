//! Property test (spec §8): for arbitrary diagnostic inputs,
//! capture never panics and every GapRecord round-trips through its
//! versioned envelope (`PLSQL-USR-001`).

use plsql_accretion::{GapRecordEnvelope, capture_gaps_with_commit};
use plsql_core::{Diagnostic, Position, Severity, Span, UnknownReason};
use plsql_engine::AnalysisRun;
use proptest::prelude::*;

fn arb_code() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("PARSE-ANTLR4RUST-001".to_string()),
        Just("IR_UNCLASSIFIED_DECL".to_string()),
        Just("IR_DDL_NOT_LOWERED".to_string()),
        Just("SOME-OTHER-CODE".to_string()),
        "[A-Z][A-Z0-9_-]{0,12}",
    ]
}

fn arb_reason() -> impl Strategy<Value = Option<UnknownReason>> {
    prop_oneof![
        Just(None),
        Just(Some(UnknownReason::WrappedSource)),
        Just(Some(UnknownReason::DynamicSqlOpaque)),
        Just(Some(UnknownReason::UnsupportedDialectFeature)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn capture_never_panics_and_round_trips(
        code in arb_code(),
        reason in arb_reason(),
        s_off in 0u32..100_000,
        len in 0u32..5_000,
        s_line in 1u32..5_000,
        n_lines in 0u32..5_000,
        // freeform message text that could (must not) leak
        msg in ".{0,64}",
    ) {
        let mut diag = Diagnostic::new(&code, Severity::Warn, &msg);
        diag.primary_span = Some(Span::new(
            plsql_core::FileId::new(0),
            Position::new(s_line, 1, s_off),
            Position::new(s_line.saturating_add(n_lines), 1, s_off.saturating_add(len)),
        ));
        if let Some(r) = reason {
            diag.unknown_reasons.push(r);
        }

        let run = AnalysisRun {
            diagnostics: vec![diag],
            ..AnalysisRun::default()
        };

        // Must not panic.
        let recs = capture_gaps_with_commit(&run, "abc123");
        let env = GapRecordEnvelope::new(recs);

        // Round-trip through the versioned envelope.
        let json = env.to_robot_json().expect("serialize");
        let back: GapRecordEnvelope =
            serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(&back, &env);
        prop_assert!(back.is_gap_record_schema());

        // Determinism: a second capture is byte-identical.
        let env2 = GapRecordEnvelope::new(capture_gaps_with_commit(&run, "abc123"));
        prop_assert_eq!(env.to_robot_json().unwrap(), env2.to_robot_json().unwrap());
    }
}
