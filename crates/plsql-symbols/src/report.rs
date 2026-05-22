//! `Resolution` reporting w/ strategy trace + Evidence records
//! (PLSQL-SYM-006).
//!
//! Wraps the `ResolvedRef` payload from PLSQL-SYM-002 + SYM-003
//! with a verbose audit trail: which strategies the resolver
//! tried before it succeeded, which decl ids were considered and
//! rejected, and a structured `Evidence` record per candidate so
//! the report can explain *why* a hit landed. Reporting is the
//! shape lineage / SAST consume when they need to defend an
//! edge claim against an operator review.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Name
//!   Resolution chapter governs strategy ordering; the report's
//!   `strategy_trace` mirrors that ordering verbatim.

use serde::{Deserialize, Serialize};

use plsql_core::{Confidence, ConfidenceLevel};
use plsql_ir::{DeclId, DeclKind};

use crate::resolve_refs::{ResolutionStrategy, ResolvedRef, UnresolvedReason};

/// Full resolution audit record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionReport {
    /// The reference the resolver looked up, dotted-string form.
    pub reference: String,
    /// The terminal `ResolvedRef` decision.
    pub outcome: ResolutionOutcome,
    /// One trace entry per strategy the resolver consulted, in
    /// the order they were tried. The successful strategy is the
    /// last entry; earlier entries are misses w/ Evidence.
    pub strategy_trace: Vec<StrategyTraceEntry>,
    /// Confidence the resolver attaches to the outcome. Drives
    /// the lineage layer's edge confidence.
    pub confidence: Confidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResolutionOutcome {
    Resolved {
        decl: DeclId,
        kind: DeclKind,
        strategy: ResolutionStrategy,
    },
    Unresolved {
        reason: UnresolvedReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTraceEntry {
    pub strategy: ResolutionStrategy,
    pub result: StrategyResult,
    pub evidence: Evidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyResult {
    /// This strategy produced the winning resolution.
    Hit,
    /// Strategy ran but didn't find a candidate.
    Miss,
    /// Strategy was skipped (precondition didn't hold — e.g.
    /// strategy 5 fires only when parts.len() >= 2).
    Skipped,
}

/// Structured evidence about why a strategy fired or missed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Human-readable summary suitable for inclusion in audit
    /// output.
    pub summary: String,
    /// Optional decl ids the strategy considered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<DeclId>,
}

impl ResolutionReport {
    /// Build a successful report from a `Resolved` reference and
    /// the strategy trace the resolver assembled along the way.
    #[must_use]
    pub fn resolved(
        reference: &str,
        decl: DeclId,
        kind: DeclKind,
        strategy: ResolutionStrategy,
        trace: Vec<StrategyTraceEntry>,
    ) -> Self {
        let confidence = confidence_for_strategy(strategy);
        Self {
            reference: reference.to_string(),
            outcome: ResolutionOutcome::Resolved {
                decl,
                kind,
                strategy,
            },
            strategy_trace: trace,
            confidence,
        }
    }

    /// Build an unresolved report. The trace records which
    /// strategies were tried before resolution gave up.
    #[must_use]
    pub fn unresolved(
        reference: &str,
        reason: UnresolvedReason,
        trace: Vec<StrategyTraceEntry>,
    ) -> Self {
        Self {
            reference: reference.to_string(),
            outcome: ResolutionOutcome::Unresolved { reason },
            strategy_trace: trace,
            confidence: Confidence {
                level: ConfidenceLevel::Low,
                explanation: Some(format!(
                    "reference {reference:?} did not resolve under any strategy ({reason:?})"
                )),
            },
        }
    }
}

/// Confidence assigned by strategy: local / package-internal
/// resolution is High; same-schema is High; synonym-followed is
/// Medium (one indirection); schema-qualified across schemas is
/// Medium (privilege check deferred to the catalog layer).
#[must_use]
pub fn confidence_for_strategy(strategy: ResolutionStrategy) -> Confidence {
    let (level, why) = match strategy {
        ResolutionStrategy::Local => (
            ConfidenceLevel::High,
            "resolved against local scope (parameter / local variable / cursor)",
        ),
        ResolutionStrategy::PackageInternal => (
            ConfidenceLevel::High,
            "resolved against package-internal scope",
        ),
        ResolutionStrategy::SameSchema => (
            ConfidenceLevel::High,
            "resolved against same-schema declarations",
        ),
        ResolutionStrategy::SynonymFollowed => {
            (ConfidenceLevel::Medium, "resolved via synonym indirection")
        }
        ResolutionStrategy::SchemaQualified => (
            ConfidenceLevel::Medium,
            "resolved against cross-schema declarations; ALL_TAB_PRIVS check deferred to catalog layer",
        ),
    };
    Confidence {
        level,
        explanation: Some(why.into()),
    }
}

/// Convenience: convert a bare [`ResolvedRef`] into a
/// `ResolutionReport` with an empty strategy trace and the
/// strategy-derived confidence. Used by callers that don't
/// maintain a per-strategy trace themselves.
#[must_use]
pub fn report_from_resolved(reference: &str, resolved: &ResolvedRef) -> ResolutionReport {
    match resolved {
        ResolvedRef::Resolved {
            decl,
            kind,
            strategy,
        } => ResolutionReport {
            reference: reference.to_string(),
            outcome: ResolutionOutcome::Resolved {
                decl: *decl,
                kind: *kind,
                strategy: *strategy,
            },
            strategy_trace: vec![StrategyTraceEntry {
                strategy: *strategy,
                result: StrategyResult::Hit,
                evidence: Evidence {
                    summary: format!("resolved via {strategy:?}"),
                    candidates: vec![*decl],
                },
            }],
            confidence: confidence_for_strategy(*strategy),
        },
        ResolvedRef::Unresolved { reason } => ResolutionReport {
            reference: reference.to_string(),
            outcome: ResolutionOutcome::Unresolved { reason: *reason },
            strategy_trace: vec![],
            confidence: Confidence {
                level: ConfidenceLevel::Low,
                explanation: Some(format!("unresolved: {reason:?}")),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_ir::DeclId;

    fn decl(id: u64) -> DeclId {
        DeclId::new(id)
    }

    #[test]
    fn confidence_local_is_high() {
        let c = confidence_for_strategy(ResolutionStrategy::Local);
        assert_eq!(c.level, ConfidenceLevel::High);
        assert!(c.explanation.as_deref().unwrap().contains("local"));
    }

    #[test]
    fn confidence_synonym_followed_is_medium() {
        assert_eq!(
            confidence_for_strategy(ResolutionStrategy::SynonymFollowed).level,
            ConfidenceLevel::Medium
        );
    }

    #[test]
    fn confidence_schema_qualified_is_medium() {
        assert_eq!(
            confidence_for_strategy(ResolutionStrategy::SchemaQualified).level,
            ConfidenceLevel::Medium
        );
    }

    #[test]
    fn report_from_resolved_carries_strategy() {
        let r = ResolvedRef::Resolved {
            decl: decl(7),
            kind: DeclKind::Table,
            strategy: ResolutionStrategy::SameSchema,
        };
        let rep = report_from_resolved("hr.employees", &r);
        match rep.outcome {
            ResolutionOutcome::Resolved {
                strategy: ResolutionStrategy::SameSchema,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(rep.confidence.level, ConfidenceLevel::High);
        assert_eq!(rep.strategy_trace.len(), 1);
        assert_eq!(rep.strategy_trace[0].result, StrategyResult::Hit);
    }

    #[test]
    fn report_from_unresolved_drops_low_confidence_and_empty_trace() {
        let r = ResolvedRef::Unresolved {
            reason: UnresolvedReason::NotDeclaredInScope,
        };
        let rep = report_from_resolved("nope", &r);
        match rep.outcome {
            ResolutionOutcome::Unresolved {
                reason: UnresolvedReason::NotDeclaredInScope,
            } => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(rep.confidence.level, ConfidenceLevel::Low);
        assert!(rep.strategy_trace.is_empty());
    }

    #[test]
    fn resolved_constructor_consumes_trace() {
        let trace = vec![
            StrategyTraceEntry {
                strategy: ResolutionStrategy::Local,
                result: StrategyResult::Miss,
                evidence: Evidence {
                    summary: "no local declarations of `x`".into(),
                    candidates: vec![],
                },
            },
            StrategyTraceEntry {
                strategy: ResolutionStrategy::SameSchema,
                result: StrategyResult::Hit,
                evidence: Evidence {
                    summary: "matched HR.X".into(),
                    candidates: vec![decl(42)],
                },
            },
        ];
        let rep = ResolutionReport::resolved(
            "x",
            decl(42),
            DeclKind::Table,
            ResolutionStrategy::SameSchema,
            trace,
        );
        assert_eq!(rep.strategy_trace.len(), 2);
        assert_eq!(rep.strategy_trace[0].result, StrategyResult::Miss);
        assert_eq!(rep.strategy_trace[1].result, StrategyResult::Hit);
    }

    #[test]
    fn report_serde_round_trip() {
        let r = ResolvedRef::Resolved {
            decl: decl(1),
            kind: DeclKind::Package,
            strategy: ResolutionStrategy::PackageInternal,
        };
        let rep = report_from_resolved("billing_pkg", &r);
        let json = serde_json::to_string(&rep).unwrap();
        let back: ResolutionReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rep);
        // snake_case wire tags.
        assert!(json.contains("\"outcome\":\"resolved\""));
        assert!(json.contains("\"strategy\":\"package_internal\""));
    }
}
