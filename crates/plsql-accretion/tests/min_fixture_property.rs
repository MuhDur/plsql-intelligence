//! P2 property test (spec §8 property row, `PLSQL-USR-001`).
//!
//! For arbitrary synthetic inputs that produce a repairable
//! `GapRecord`, `build_min_fixture` either returns a fixture that is
//! (a) ≤ max_bytes, (b) still reproduces the identical signature,
//! (c) privacy-proven (no planted column-name fragment survives) —
//! OR returns `Err` and persists nothing. It must never panic.

use plsql_accretion::{DEFAULT_MAX_BYTES, build_min_fixture, capture_gaps_with_commit};
use plsql_engine::{AnalysisRequest, analyze_project};
use proptest::prelude::*;

fn analyze_first_gap(source: &str, tag: &str) -> Option<plsql_accretion::GapRecord> {
    let dir = std::env::temp_dir().join(format!("plsql-usr-prop-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("a.sql"), source).ok()?;
    let mut req = AnalysisRequest {
        project_root: dir.clone(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = analyze_project(req).ok()?;
    capture_gaps_with_commit(&run, "prop").into_iter().next()
}

proptest! {
    // Kept modest: each case spins the full engine in a temp dir.
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn build_min_fixture_is_total_and_safe(
        ncols in 1usize..6,
        // identifier-shaped column stems; ascii-lower so they are
        // valid Oracle identifiers and a meaningful privacy target.
        stem in "[a-z]{3,10}",
        width in 8u32..120,
    ) {
        // Build a synthetic CREATE TABLE (reliably emits a real
        // IR_DDL_NOT_LOWERED gap). The stem is a planted secret-ish
        // identifier fragment.
        let cols: Vec<String> = (0..ncols)
            .map(|i| format!("{stem}_col{i} VARCHAR2({width})"))
            .collect();
        let source = format!("CREATE TABLE {stem}_tbl ({});\n", cols.join(", "));

        let Some(gap) = analyze_first_gap(&source, &format!("{stem}{ncols}{width}")) else {
            // No repairable gap for this shape — nothing to assert.
            return Ok(());
        };

        // Must never panic; either a valid fixture or an honest Err.
        match build_min_fixture(&source, &gap, DEFAULT_MAX_BYTES) {
            Ok(fx) => {
                // (a) within cap
                prop_assert!(fx.source.len() <= DEFAULT_MAX_BYTES);
                // (b) carries the byte-identical target signature
                prop_assert_eq!(fx.signature.clone(), gap.signature.clone());
                // (c) privacy: the planted identifier stem must not
                //     survive in the stored fixture.
                prop_assert!(
                    !fx.source.contains(&stem),
                    "planted stem {:?} leaked: {}",
                    stem, fx.source
                );
                // (c') privacy: not even the table/column suffixes
                //       — every estate-bearing token is a same-class
                //       synthetic alias.
                prop_assert!(
                    !fx.source.contains(&format!("{stem}_tbl")),
                    "planted table name leaked: {}", fx.source
                );
                // proof object is the single deterministic
                // structure-preserving scrub step (no longer the old
                // apply_rules→scrub→rename triple).
                prop_assert_eq!(fx.redaction_manifest.steps.len(), 1);
                prop_assert_eq!(
                    fx.redaction_manifest.steps[0].step.clone(),
                    "structure_preserving_token_scrub".to_string()
                );
                prop_assert!(fx.privacy_proof_id().is_ok());
                // parse-position preserved: the stored synthetic
                // fixture re-captures the byte-identical fine-grained
                // (code, rule-path, signature) the target carried.
                if let Some(reg) = analyze_first_gap(
                    &fx.source, &format!("recap{stem}{ncols}{width}"),
                ) {
                    prop_assert_eq!(reg.signature, gap.signature.clone());
                    prop_assert_eq!(
                        reg.antlr_rule_path.clone(),
                        gap.antlr_rule_path.clone(),
                        "structure-preserving scrub must keep the ANTLR rule path"
                    );
                }
            }
            Err(_) => {
                // Honest discard — acceptable. (Builder never
                // persists; persistence is a separate explicit call
                // the loop makes only on Ok.)
            }
        }
    }
}
