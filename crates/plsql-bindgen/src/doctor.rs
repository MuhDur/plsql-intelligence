//! Doctor surface for [`BindingPlan`] — bindings-coverage report.
//!
//! Aggregates per-routine binding-emission status into a stable JSON
//! shape so a developer can ask: "is my package well-supported by
//! plsql-bindgen?". Follows the project-wide doctor convention.
//!
//! Coverage tiers (per routine):
//! - **Emitted**: zero diagnostics targeting this routine — generator
//!   produced a Rust wrapper.
//! - **EmittedWithCaveats**: at least one `Caveat`-severity
//!   diagnostic; wrapper emitted but the operator should review.
//! - **Skipped**: at least one `Skip`-severity diagnostic; no wrapper
//!   emitted (unsupported construct).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{BindingDiagnostic, BindingPlan, BindingSeverity};

/// Aggregated bindings-coverage report for a [`BindingPlan`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BindingsCoverageReport {
    pub package_id: String,
    pub package_name: String,
    /// Total routines in the plan.
    pub routines_total: usize,
    /// Routines whose wrapper was emitted clean (no caveats / skips).
    pub emitted_clean: usize,
    /// Routines emitted with at least one caveat-severity diagnostic.
    pub emitted_with_caveats: usize,
    /// Routines the generator refused to emit a wrapper for (one or
    /// more skip-severity diagnostics).
    pub skipped: usize,
    /// Emitted ÷ total as a percentage (0..=100).
    pub emit_percent: u32,
    /// Per-diagnostic-code breakdown, sorted by `count` desc then
    /// code asc for stable output.
    pub by_code: Vec<CodeCountRow>,
    /// Sample (first N) of routines the generator skipped, sorted by
    /// routine name for stable output.
    pub skipped_routines_sample: Vec<String>,
    /// Overall posture — `Clean` (≥95% emitted, no skips), `Caution`
    /// (≥50% emitted), `Unknown` otherwise.
    pub posture: BindingsPosture,
    /// One-line operator hints.
    pub remediation_hints: Vec<String>,
}

/// Per-diagnostic-code count row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodeCountRow {
    pub code: String,
    pub count: usize,
    /// `Skip` or `Warn` (whichever the diagnostic-code's
    /// `severity()` resolves to). Recorded so callers can split the
    /// histogram into emission-blocker vs caveat-only buckets.
    pub severity: BindingSeverity,
}

/// Three-state posture for the binding plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum BindingsPosture {
    /// ≥95% routines emitted and zero skips.
    Clean,
    /// ≥50% emitted and < 95%, OR some skips present but most
    /// routines still emit.
    #[default]
    Caution,
    /// < 50% emitted, OR plan empty.
    Unknown,
}

/// Hard cap on `skipped_routines_sample`.
pub const SKIPPED_SAMPLE_LIMIT: usize = 50;

/// Build a [`BindingsCoverageReport`] from a [`BindingPlan`].
#[must_use]
pub fn coverage_report(plan: &BindingPlan) -> BindingsCoverageReport {
    // Per-routine state is keyed by BINDING INDEX, not by name: PL/SQL
    // packages legally carry overloaded subprograms sharing one name (lib.rs
    // documents `name` as "as it appears in the package"), so a name-keyed map
    // would collapse N overloads into one shared state and miscount the
    // clean/skip/emit split. A routine-targeted diagnostic names a routine, so
    // it fans out to EVERY binding sharing that name (the report cannot tell
    // which overload the diagnostic meant); each index is then tallied exactly
    // once so `emitted + skipped == routines_total` always holds.
    let mut states: Vec<RoutineState> = vec![RoutineState::default(); plan.routines.len()];
    let mut indices_by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (idx, routine) in plan.routines.iter().enumerate() {
        indices_by_name
            .entry(routine.name.as_str())
            .or_default()
            .push(idx);
    }
    let mut plan_level_diagnostics = Vec::new();

    for diagnostic in &plan.diagnostics {
        match diagnostic.routine.as_deref() {
            Some(name) => {
                if let Some(indices) = indices_by_name.get(name) {
                    for &idx in indices {
                        states[idx].record(diagnostic);
                    }
                }
                // A diagnostic naming a routine absent from the plan targets
                // no binding; it is intentionally dropped from the per-routine
                // tally (it still shows up in the by-code histogram below).
            }
            None => plan_level_diagnostics.push(diagnostic),
        }
    }

    let mut emitted_clean = 0usize;
    let mut emitted_with_caveats = 0usize;
    let mut skipped = 0usize;
    let mut skipped_routines: Vec<String> = Vec::new();

    for (idx, routine) in plan.routines.iter().enumerate() {
        let state = states[idx];
        if state.has_skip {
            skipped += 1;
            skipped_routines.push(routine.name.clone());
        } else if state.has_caveat {
            emitted_with_caveats += 1;
        } else {
            emitted_clean += 1;
        }
    }
    skipped_routines.sort();
    skipped_routines.dedup();
    let skipped_routines_sample = skipped_routines
        .into_iter()
        .take(SKIPPED_SAMPLE_LIMIT)
        .collect::<Vec<_>>();

    let routines_total = plan.routines.len();
    let emitted = emitted_clean + emitted_with_caveats;
    let emit_percent: u32 = (emitted * 100)
        .checked_div(routines_total)
        .map(|p| u32::try_from(p).unwrap_or(u32::MAX))
        .unwrap_or(0);

    // Per-code histogram.
    let mut code_counts: BTreeMap<&str, (usize, BindingSeverity)> = BTreeMap::new();
    for diagnostic in &plan.diagnostics {
        let entry = code_counts
            .entry(diagnostic.code.as_str())
            .or_insert((0, diagnostic.severity));
        entry.0 += 1;
        // Bump severity to Skip if any diagnostic for this code is a
        // Skip — stronger guarantee for the reader.
        if matches!(diagnostic.severity, BindingSeverity::Skip) {
            entry.1 = BindingSeverity::Skip;
        }
    }
    let mut by_code: Vec<CodeCountRow> = code_counts
        .into_iter()
        .map(|(code, (count, severity))| CodeCountRow {
            code: code.to_string(),
            count,
            severity,
        })
        .collect();
    by_code.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.code.cmp(&b.code)));

    let posture = classify_posture(routines_total, emit_percent, skipped);
    let remediation_hints = build_remediation_hints(
        routines_total,
        emit_percent,
        skipped,
        emitted_with_caveats,
        plan_level_diagnostics.len(),
    );

    BindingsCoverageReport {
        package_id: plan.package_id.clone(),
        package_name: plan.package_name.clone(),
        routines_total,
        emitted_clean,
        emitted_with_caveats,
        skipped,
        emit_percent,
        by_code,
        skipped_routines_sample,
        posture,
        remediation_hints,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RoutineState {
    has_skip: bool,
    has_caveat: bool,
}

impl RoutineState {
    fn record(&mut self, d: &BindingDiagnostic) {
        match d.severity {
            BindingSeverity::Skip => self.has_skip = true,
            BindingSeverity::Warn => self.has_caveat = true,
            _ => {}
        }
    }
}

fn classify_posture(total: usize, emit_percent: u32, skipped: usize) -> BindingsPosture {
    if total == 0 {
        return BindingsPosture::Unknown;
    }
    if emit_percent >= 95 && skipped == 0 {
        BindingsPosture::Clean
    } else if emit_percent >= 50 {
        BindingsPosture::Caution
    } else {
        BindingsPosture::Unknown
    }
}

fn build_remediation_hints(
    total: usize,
    emit_percent: u32,
    skipped: usize,
    caveats: usize,
    plan_diagnostics: usize,
) -> Vec<String> {
    let mut hints = Vec::new();
    if total == 0 {
        hints.push(String::from(
            "BindingPlan is empty — confirm the catalog snapshot includes the requested package and that ALL_PROCEDURES / ALL_ARGUMENTS were captured.",
        ));
        return hints;
    }
    if skipped > 0 {
        hints.push(format!(
            "{skipped} routine(s) skipped — see `skipped_routines_sample`; each carries a BG_UNSUPPORTED_* diagnostic with a manual-workaround pointer.",
        ));
    }
    if caveats > 0 {
        hints.push(format!(
            "{caveats} routine(s) emitted with caveats — review the per-routine diagnostics before relying on the generated wrapper.",
        ));
    }
    if plan_diagnostics > 0 {
        hints.push(format!(
            "{plan_diagnostics} plan-level diagnostic(s) emitted (no routine target) — these usually point at package-wide unsupported constructs (e.g. wrapped package body).",
        ));
    }
    if emit_percent < 50 {
        hints.push(String::from(
            "Less than half the package emits cleanly — consider whether the unsupported subset is actually used by callers, or restructure to lift the SQL-side wrappers.",
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BindingDiagnostic, BindingDiagnosticCode, BindingPlan, ParameterBinding, ParameterMode,
        RoutineBinding, RoutineKind, RustTypeRef,
    };

    fn routine(name: &str) -> RoutineBinding {
        RoutineBinding {
            name: name.into(),
            kind: RoutineKind::Procedure,
            parameters: vec![ParameterBinding {
                name: "p_x".into(),
                mode: ParameterMode::In,
                rust_type: RustTypeRef {
                    path: "i64".into(),
                    nullable: false,
                },
                has_default: false,
            }],
            return_type: None,
            autonomous_transaction: false,
        }
    }

    fn diag(routine: Option<&str>, code: BindingDiagnosticCode) -> BindingDiagnostic {
        BindingDiagnostic::new_unsupported(code, routine.map(String::from), None)
    }

    #[test]
    fn empty_plan_yields_unknown_posture() {
        let plan = BindingPlan {
            package_id: "x.empty".into(),
            package_name: "EMPTY".into(),
            routines: vec![],
            diagnostics: vec![],
        };
        let r = coverage_report(&plan);
        assert_eq!(r.routines_total, 0);
        assert_eq!(r.posture, BindingsPosture::Unknown);
        assert!(
            r.remediation_hints
                .iter()
                .any(|h| h.contains("BindingPlan is empty"))
        );
    }

    #[test]
    fn clean_plan_with_no_diagnostics_yields_clean_posture() {
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: (0..10).map(|i| routine(&format!("r{i}"))).collect(),
            diagnostics: vec![],
        };
        let r = coverage_report(&plan);
        assert_eq!(r.emitted_clean, 10);
        assert_eq!(r.emit_percent, 100);
        assert_eq!(r.posture, BindingsPosture::Clean);
        assert!(r.remediation_hints.is_empty());
    }

    #[test]
    fn skip_diagnostic_moves_routine_to_skipped_bucket() {
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: vec![routine("r_clean"), routine("r_unsupported")],
            diagnostics: vec![diag(
                Some("r_unsupported"),
                BindingDiagnosticCode::RefCursor,
            )],
        };
        let r = coverage_report(&plan);
        assert_eq!(r.emitted_clean, 1);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.emit_percent, 50);
        assert_eq!(r.skipped_routines_sample, vec!["r_unsupported"]);
        assert!(
            r.remediation_hints
                .iter()
                .any(|h| h.contains("routine(s) skipped"))
        );
    }

    #[test]
    fn caveat_diagnostic_moves_routine_to_caveats_bucket() {
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: vec![routine("r_caveat")],
            diagnostics: vec![diag(
                Some("r_caveat"),
                BindingDiagnosticCode::AutonomousTransaction,
            )],
        };
        let r = coverage_report(&plan);
        assert_eq!(r.emitted_clean, 0);
        assert_eq!(r.emitted_with_caveats, 1);
        assert_eq!(r.skipped, 0);
        assert!(
            r.remediation_hints
                .iter()
                .any(|h| h.contains("emitted with caveats"))
        );
    }

    #[test]
    fn by_code_histogram_sorts_by_count_desc_then_code_asc() {
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: (0..4).map(|i| routine(&format!("r{i}"))).collect(),
            diagnostics: vec![
                diag(Some("r0"), BindingDiagnosticCode::RefCursor),
                diag(Some("r1"), BindingDiagnosticCode::RefCursor),
                diag(Some("r2"), BindingDiagnosticCode::RefCursor),
                diag(Some("r3"), BindingDiagnosticCode::PipelinedFunction),
            ],
        };
        let r = coverage_report(&plan);
        assert!(r.by_code.len() >= 2);
        // RefCursor (3) before PipelinedFunction (1).
        assert!(r.by_code[0].count > r.by_code[1].count);
    }

    #[test]
    fn plan_level_diagnostics_surface_as_remediation_hint() {
        let plan = BindingPlan {
            package_id: "x.wrapped_pkg".into(),
            package_name: "WRAPPED_PKG".into(),
            routines: vec![routine("r_clean")],
            diagnostics: vec![diag(None, BindingDiagnosticCode::WrappedPackageBody)],
        };
        let r = coverage_report(&plan);
        assert!(
            r.remediation_hints
                .iter()
                .any(|h| h.contains("plan-level diagnostic"))
        );
    }

    #[test]
    fn skipped_routines_sample_caps_at_limit() {
        let plan = BindingPlan {
            package_id: "x.big".into(),
            package_name: "BIG".into(),
            routines: (0..(SKIPPED_SAMPLE_LIMIT + 20))
                .map(|i| routine(&format!("r{i:04}")))
                .collect(),
            diagnostics: (0..(SKIPPED_SAMPLE_LIMIT + 20))
                .map(|i| diag(Some(&format!("r{i:04}")), BindingDiagnosticCode::RefCursor))
                .collect(),
        };
        let r = coverage_report(&plan);
        assert_eq!(r.skipped, SKIPPED_SAMPLE_LIMIT + 20);
        assert_eq!(r.skipped_routines_sample.len(), SKIPPED_SAMPLE_LIMIT);
        // Sorted lexicographically.
        assert_eq!(r.skipped_routines_sample[0], "r0000");
    }

    #[test]
    fn caution_posture_at_partial_emission() {
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: (0..10).map(|i| routine(&format!("r{i}"))).collect(),
            diagnostics: (0..3)
                .map(|i| diag(Some(&format!("r{i}")), BindingDiagnosticCode::RefCursor))
                .collect(),
        };
        let r = coverage_report(&plan);
        assert_eq!(r.emit_percent, 70);
        assert_eq!(r.posture, BindingsPosture::Caution);
    }

    // --- oracle-rwjl.16: per-routine state must be keyed by binding INDEX, not
    // by name, so overloaded PL/SQL packages (legal duplicate routine names)
    // are reported per-binding. The concrete observable defect of the old
    // name-keyed map was a duplicate name in `skipped_routines_sample`; the
    // emitted/skipped tally still sums to routines_total either way. ---

    #[test]
    fn overloaded_skipped_routine_is_not_duplicated_in_sample() {
        // [hire, hire, list] with a Skip targeting "hire". The two same-named
        // bindings both count as skipped (a name-keyed diagnostic cannot pick
        // one overload), but the sample must list "hire" only ONCE — the old
        // name-keyed read-back pushed "hire" for each binding, yielding the
        // duplicate ["hire", "hire"].
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: vec![routine("hire"), routine("hire"), routine("list")],
            diagnostics: vec![diag(Some("hire"), BindingDiagnosticCode::RefCursor)],
        };
        let r = coverage_report(&plan);
        // The invariant always holds: emitted + skipped == total.
        assert_eq!(r.emitted_clean + r.emitted_with_caveats + r.skipped, 3);
        // Sample carries no duplicate name.
        assert_eq!(
            r.skipped_routines_sample,
            vec!["hire"],
            "overloaded skipped routine must appear once: {:?}",
            r.skipped_routines_sample
        );
        // The unrelated `list` binding stays clean.
        assert!(r.emitted_clean >= 1);
    }

    #[test]
    fn overload_does_not_bleed_state_onto_distinctly_named_routine() {
        // Two overloads named `hire` plus a `list`; only `list` carries a
        // Skip. The `hire` bindings must NOT inherit `list`'s state (a name
        // collision on the per-name map could only mis-key same-named entries,
        // but this guards the index keying explicitly).
        let plan = BindingPlan {
            package_id: "x.pkg".into(),
            package_name: "PKG".into(),
            routines: vec![routine("hire"), routine("hire"), routine("list")],
            diagnostics: vec![diag(Some("list"), BindingDiagnosticCode::RefCursor)],
        };
        let r = coverage_report(&plan);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.emitted_clean, 2, "both `hire` overloads must be clean");
        assert_eq!(r.skipped_routines_sample, vec!["list"]);
    }
}
