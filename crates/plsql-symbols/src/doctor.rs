//! Doctor surface for symbol resolution.
//!
//! Aggregates a stream of [`ResolutionReport`]s into a
//! [`SymbolResolutionDoctorReport`] so an operator can ask: "is name
//! resolution healthy across my corpus?". Follows the project-wide
//! doctor convention (stable JSON, three-state posture, per-condition
//! remediation_hints).
//!
//! Inputs: typically the resolver's per-reference reports produced
//! during an `AnalysisRun` (Layer 2.5 engine orchestration; the
//! engine writes them into the run's diagnostic stream).

use serde::{Deserialize, Serialize};

use crate::report::{ResolutionOutcome, ResolutionReport};
use crate::resolve_refs::{ResolutionStrategy, UnresolvedReason};

fn bump<K: Copy + PartialEq>(counts: &mut Vec<(K, usize)>, key: K) {
    if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == key) {
        entry.1 += 1;
    } else {
        counts.push((key, 1));
    }
}

/// Aggregated resolution-health report.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolResolutionDoctorReport {
    pub schema_id: String,
    pub schema_version: u32,
    /// Total references processed by the resolver.
    pub references_total: usize,
    /// References that resolved cleanly.
    pub resolved: usize,
    /// References that did not resolve (any UnresolvedReason).
    pub unresolved: usize,
    /// Resolved / total as a percentage (0..=100).
    pub resolve_percent: u32,
    /// Per-strategy success histogram, sorted by hit-count desc.
    pub by_strategy: Vec<StrategyHistogramRow>,
    /// Per-UnresolvedReason histogram, sorted by count desc.
    pub by_unresolved_reason: Vec<UnresolvedHistogramRow>,
    /// Three-state posture. Clean (>=95% resolved, no
    /// SynonymChainTooLong), Caution (>=50%), Unknown otherwise.
    pub posture: SymbolPosture,
    /// One-line operator hints.
    pub remediation_hints: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyHistogramRow {
    pub strategy: ResolutionStrategy,
    pub hits: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedHistogramRow {
    pub reason: UnresolvedReason,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum SymbolPosture {
    Clean,
    #[default]
    Caution,
    Unknown,
}

const SCHEMA_ID: &str = "plsql.symbols.resolution_doctor";
const SCHEMA_VERSION: u32 = 1;

/// Build the doctor report from a slice of [`ResolutionReport`]s.
///
/// `O(n)` over the input — each report is inspected once. Output is
/// deterministic (histograms are sorted by count desc then key asc).
#[must_use]
pub fn doctor_report(reports: &[ResolutionReport]) -> SymbolResolutionDoctorReport {
    let mut resolved = 0usize;
    let mut unresolved = 0usize;
    let mut by_strategy_counts: Vec<(ResolutionStrategy, usize)> = Vec::new();
    let mut by_reason_counts: Vec<(UnresolvedReason, usize)> = Vec::new();
    let mut has_synonym_chain_too_long = false;

    for report in reports {
        match &report.outcome {
            ResolutionOutcome::Resolved { strategy, .. } => {
                resolved += 1;
                bump(&mut by_strategy_counts, *strategy);
            }
            ResolutionOutcome::Unresolved { reason } => {
                unresolved += 1;
                bump(&mut by_reason_counts, *reason);
                if matches!(reason, UnresolvedReason::SynonymChainTooLong) {
                    has_synonym_chain_too_long = true;
                }
            }
        }
    }

    let total = reports.len();
    let resolve_percent: u32 = (resolved * 100)
        .checked_div(total)
        .map(|p| u32::try_from(p).unwrap_or(u32::MAX))
        .unwrap_or(0);

    let mut by_strategy: Vec<StrategyHistogramRow> = by_strategy_counts
        .into_iter()
        .map(|(strategy, hits)| StrategyHistogramRow { strategy, hits })
        .collect();
    by_strategy.sort_by(|a, b| {
        b.hits
            .cmp(&a.hits)
            .then_with(|| format!("{:?}", a.strategy).cmp(&format!("{:?}", b.strategy)))
    });

    let mut by_unresolved_reason: Vec<UnresolvedHistogramRow> = by_reason_counts
        .into_iter()
        .map(|(reason, count)| UnresolvedHistogramRow { reason, count })
        .collect();
    by_unresolved_reason.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| format!("{:?}", a.reason).cmp(&format!("{:?}", b.reason)))
    });

    let posture = classify_posture(total, resolve_percent, has_synonym_chain_too_long);
    let remediation_hints = build_remediation_hints(
        total,
        resolve_percent,
        unresolved,
        has_synonym_chain_too_long,
    );

    SymbolResolutionDoctorReport {
        schema_id: SCHEMA_ID.into(),
        schema_version: SCHEMA_VERSION,
        references_total: total,
        resolved,
        unresolved,
        resolve_percent,
        by_strategy,
        by_unresolved_reason,
        posture,
        remediation_hints,
    }
}

fn classify_posture(
    total: usize,
    resolve_percent: u32,
    has_synonym_chain_too_long: bool,
) -> SymbolPosture {
    if total == 0 {
        return SymbolPosture::Unknown;
    }
    if has_synonym_chain_too_long {
        // Synonym-chain-too-long is a red flag regardless of overall
        // percentage — it can mean a circular synonym chain in the
        // schema, which is a deployment bug.
        return SymbolPosture::Caution;
    }
    if resolve_percent >= 95 {
        SymbolPosture::Clean
    } else if resolve_percent >= 50 {
        SymbolPosture::Caution
    } else {
        SymbolPosture::Unknown
    }
}

fn build_remediation_hints(
    total: usize,
    resolve_percent: u32,
    unresolved: usize,
    has_synonym_chain_too_long: bool,
) -> Vec<String> {
    let mut hints = Vec::new();
    if total == 0 {
        hints.push(String::from(
            "No references in the input — confirm the resolver was actually invoked over the corpus.",
        ));
        return hints;
    }
    if has_synonym_chain_too_long {
        hints.push(String::from(
            "At least one synonym chain exceeded MAX_SYNONYM_HOPS — likely a circular synonym definition; inspect ALL_SYNONYMS for the chain.",
        ));
    }
    if unresolved > 0 {
        hints.push(format!(
            "{unresolved} reference(s) unresolved — see `by_unresolved_reason` for the per-reason breakdown.",
        ));
    }
    if resolve_percent < 50 {
        hints.push(String::from(
            "Less than half of references resolve — likely a missing catalog snapshot or an incomplete DeclTable; verify the engine wired both upstream inputs.",
        ));
    } else if resolve_percent < 95 {
        hints.push(format!(
            "Resolution coverage is {resolve_percent}% — push past 95% before claiming production-ready.",
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Evidence, ResolutionReport, StrategyResult, StrategyTraceEntry};
    use crate::resolve_refs::ResolutionStrategy;
    use plsql_core::{Confidence, ConfidenceLevel};
    use plsql_ir::{DeclId, DeclKind};

    fn resolved(reference: &str, strategy: ResolutionStrategy) -> ResolutionReport {
        ResolutionReport {
            reference: reference.into(),
            outcome: ResolutionOutcome::Resolved {
                decl: DeclId::new(1),
                kind: DeclKind::Procedure,
                strategy,
            },
            strategy_trace: vec![StrategyTraceEntry {
                strategy,
                result: StrategyResult::Hit,
                evidence: Evidence {
                    summary: "hit".into(),
                    candidates: vec![],
                },
            }],
            confidence: Confidence::new(ConfidenceLevel::High, None),
        }
    }

    fn unresolved(reference: &str, reason: UnresolvedReason) -> ResolutionReport {
        ResolutionReport {
            reference: reference.into(),
            outcome: ResolutionOutcome::Unresolved { reason },
            strategy_trace: vec![],
            confidence: Confidence::new(ConfidenceLevel::Opaque, None),
        }
    }

    #[test]
    fn empty_input_yields_unknown_posture() {
        let report = doctor_report(&[]);
        assert_eq!(report.references_total, 0);
        assert_eq!(report.posture, SymbolPosture::Unknown);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("No references"))
        );
    }

    #[test]
    fn fully_resolved_input_yields_clean_posture() {
        let reports: Vec<_> = (0..20)
            .map(|i| resolved(&format!("r{i}"), ResolutionStrategy::Local))
            .collect();
        let report = doctor_report(&reports);
        assert_eq!(report.resolved, 20);
        assert_eq!(report.resolve_percent, 100);
        assert_eq!(report.posture, SymbolPosture::Clean);
        assert!(report.remediation_hints.is_empty());
    }

    #[test]
    fn synonym_chain_too_long_forces_caution_regardless_of_percent() {
        let mut reports: Vec<_> = (0..19)
            .map(|i| resolved(&format!("r{i}"), ResolutionStrategy::Local))
            .collect();
        reports.push(unresolved("r_bad", UnresolvedReason::SynonymChainTooLong));
        let report = doctor_report(&reports);
        // 19/20 = 95% which would normally be Clean — but the chain
        // flag forces Caution because a circular synonym is a bug.
        assert_eq!(report.resolve_percent, 95);
        assert_eq!(report.posture, SymbolPosture::Caution);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("circular synonym"))
        );
    }

    #[test]
    fn synonym_chain_too_long_overrides_even_at_full_resolution() {
        // Stronger boundary than the 95% case: with 100% of refs
        // resolved, posture would be Clean — but a single
        // SynonymChainTooLong must still force Caution. This pins the
        // ordering: the chain check runs *before* the percentage
        // check in classify_posture.
        let mut reports: Vec<_> = (0..50)
            .map(|i| resolved(&format!("ok{i}"), ResolutionStrategy::SameSchema))
            .collect();
        reports.push(unresolved("loop", UnresolvedReason::SynonymChainTooLong));
        let report = doctor_report(&reports);
        // 50/51 ≈ 98% (>=95, the Clean threshold) yet:
        assert!(report.resolve_percent >= 95);
        assert_eq!(report.posture, SymbolPosture::Caution);
    }

    #[test]
    fn by_strategy_histogram_sorts_by_hits_desc() {
        let reports = vec![
            resolved("a", ResolutionStrategy::SameSchema),
            resolved("b", ResolutionStrategy::SameSchema),
            resolved("c", ResolutionStrategy::SameSchema),
            resolved("d", ResolutionStrategy::Local),
        ];
        let report = doctor_report(&reports);
        assert_eq!(report.by_strategy.len(), 2);
        assert!(report.by_strategy[0].hits > report.by_strategy[1].hits);
    }

    #[test]
    fn by_unresolved_reason_histogram_present() {
        let reports = vec![
            unresolved("a", UnresolvedReason::NotDeclaredInScope),
            unresolved("b", UnresolvedReason::NotDeclaredInScope),
            unresolved("c", UnresolvedReason::SynonymTargetMissing),
        ];
        let report = doctor_report(&reports);
        assert_eq!(report.by_unresolved_reason.len(), 2);
        assert_eq!(report.by_unresolved_reason[0].count, 2);
    }

    #[test]
    fn schema_id_and_version_pinned() {
        let report = doctor_report(&[resolved("x", ResolutionStrategy::Local)]);
        assert_eq!(report.schema_id, "plsql.symbols.resolution_doctor");
        assert_eq!(report.schema_version, 1);
    }

    #[test]
    fn caution_posture_at_partial_resolution() {
        let mut reports: Vec<_> = (0..7)
            .map(|i| resolved(&format!("r{i}"), ResolutionStrategy::Local))
            .collect();
        reports.extend(
            (0..3).map(|i| unresolved(&format!("u{i}"), UnresolvedReason::NotDeclaredInScope)),
        );
        let report = doctor_report(&reports);
        assert_eq!(report.resolve_percent, 70);
        assert_eq!(report.posture, SymbolPosture::Caution);
    }
}
