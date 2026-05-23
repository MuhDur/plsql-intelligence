//! SAST scan harness.
//!
//! Loads the Layer-2 analysis outputs (a `FactStore`, a
//! per-unit `FlowEnv`, and a completeness snapshot mapped from
//! the engine's `AnalysisRun`), then drives a rule registry:
//!
//! 1. **Completeness gate** — a rule whose
//!    [`minimum_completeness`](crate::Rule::minimum_completeness)
//!    is not satisfied is *not run*; a typed
//!    [`RuleSkippedDiagnostic`] is recorded instead (R13 — the
//!    gap is visible, never a silent false-negative).
//! 2. **Required-facts gate** — a rule that declares
//!    [`required_facts`](crate::Rule::required_facts) the store
//!    cannot supply is skipped with
//!    [`SkipReason::MissingFlowFacts`].
//! 3. **Scan** — surviving rules run over every unit; findings
//!    and per-unit skips are aggregated.
//!
//! The aggregated [`ScanReport`] is deterministically ordered so
//! it is stable machine output (R10/R11) regardless of the
//! registry or unit iteration order.
//!
//! ## Layer hygiene
//!
//! The harness never names the engine crate. It takes a small
//! [`CompletenessSnapshot`] mirror of the few `AnalysisRun`
//! fields rules gate on — same shim convention used elsewhere
//! (`CatalogResolutionSource`, `PlScopeReference`) so Layer 3
//! does not pull in the orchestration layer. The `plsql-engine`
//! CLI (which already depends on both) maps `AnalysisRun ->
//! CompletenessSnapshot` at the call site.

use plsql_ir::{FactStore, FlowEnv, FlowQuery};
use serde::{Deserialize, Serialize};

use crate::{Finding, Rule, RuleSkippedDiagnostic, ScanContext, Severity, SkipReason};

/// The handful of `AnalysisRun` completeness fields a rule can
/// gate on. Mapped in by the caller (engine CLI) — keeps this
/// crate's deps at Layer 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompletenessSnapshot {
    pub catalog_available: bool,
    pub plscope_available: bool,
    pub files_total: usize,
    pub files_recovered: usize,
}

impl CompletenessSnapshot {
    /// Fraction of files that were parser-recovered. `0.0` when
    /// no files were analysed (vacuously clean).
    #[must_use]
    pub fn recovered_ratio(&self) -> f32 {
        if self.files_total == 0 {
            0.0
        } else {
            self.files_recovered as f32 / self.files_total as f32
        }
    }
}

/// One unit (routine / object) the harness scans, paired with
/// its intra-procedural flow environment.
pub struct ScanUnit<'a> {
    pub unit_logical_id: &'a str,
    pub source_file: &'a str,
    pub flow: &'a FlowEnv,
}

/// Aggregated, deterministically-ordered result of one scan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub skipped: Vec<RuleSkippedDiagnostic>,
    /// Rules that passed both gates and were executed.
    pub rules_run: usize,
    /// Rules gated out before scanning (completeness or facts).
    pub rules_gated: usize,
}

impl ScanReport {
    /// Highest finding severity, if any.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}

/// Stable id used for a project-wide (not per-unit) skip such as
/// a completeness or required-facts gate.
const RUN_SCOPE_UNIT: &str = "<analysis-run>";

fn skip_reason_rank(r: SkipReason) -> u8 {
    match r {
        SkipReason::MissingFlowFacts => 0,
        SkipReason::OpaqueConstruct => 1,
        SkipReason::SuppressedByAnnotation => 2,
        SkipReason::PreconditionUnmet => 3,
    }
}

/// Drive `rules` over `units`, honoring each rule's
/// completeness + required-fact preconditions.
#[must_use]
pub fn run_scan(
    rules: &[Box<dyn Rule>],
    units: &[ScanUnit<'_>],
    facts: &FactStore,
    completeness: &CompletenessSnapshot,
) -> ScanReport {
    let mut report = ScanReport::default();

    for rule in rules {
        // Gate 1: completeness.
        if let Some(detail) = rule.minimum_completeness().unmet_against(completeness) {
            report.skipped.push(RuleSkippedDiagnostic {
                rule_id: rule.id().to_string(),
                unit: RUN_SCOPE_UNIT.to_string(),
                reason: SkipReason::PreconditionUnmet,
                detail,
            });
            report.rules_gated += 1;
            continue;
        }

        // Gate 2: required facts. Missing *any* required kind
        // gates the rule — running it blind would emit unsound
        // findings or hide real ones.
        let missing_kind = rule
            .required_facts()
            .iter()
            .find(|k| facts.by_kind(**k).next().is_none());
        if let Some(kind) = missing_kind {
            report.skipped.push(RuleSkippedDiagnostic {
                rule_id: rule.id().to_string(),
                unit: RUN_SCOPE_UNIT.to_string(),
                reason: SkipReason::MissingFlowFacts,
                detail: format!("no facts of required kind {kind:?}"),
            });
            report.rules_gated += 1;
            continue;
        }

        // Both gates passed — run over every unit.
        report.rules_run += 1;
        for unit in units {
            let ctx = ScanContext::new(
                unit.unit_logical_id,
                unit.source_file,
                FlowQuery::new(unit.flow),
                facts,
            );
            let out = rule.scan(&ctx);
            report.findings.extend(out.findings);
            report.skipped.extend(out.skipped);
        }
    }

    // Deterministic ordering — stable machine output regardless
    // of registry / unit iteration order.
    report.findings.sort_by(|a, b| {
        (
            &a.rule_id,
            &a.location.file,
            a.location.line,
            a.location.byte_span,
        )
            .cmp(&(
                &b.rule_id,
                &b.location.file,
                b.location.line,
                b.location.byte_span,
            ))
    });
    report.skipped.sort_by(|a, b| {
        (&a.rule_id, &a.unit, skip_reason_rank(a.reason), &a.detail).cmp(&(
            &b.rule_id,
            &b.unit,
            skip_reason_rank(b.reason),
            &b.detail,
        ))
    });

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompletenessRequirement, RuleOutput, finding};
    use plsql_ir::{FactKind, FactPayload, FactProvenance, mint_fact};

    struct AlwaysFinds;
    impl Rule for AlwaysFinds {
        fn id(&self) -> &'static str {
            "SAST-TEST-001"
        }
        fn default_severity(&self) -> Severity {
            Severity::High
        }
        fn description(&self) -> &'static str {
            "always finds"
        }
        fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
            RuleOutput::default().finding(finding(
                self.id(),
                Severity::High,
                "x",
                ctx.source_file,
                1,
                (0, 1),
            ))
        }
    }

    struct NeedsCatalog;
    impl Rule for NeedsCatalog {
        fn id(&self) -> &'static str {
            "SAST-TEST-CAT"
        }
        fn default_severity(&self) -> Severity {
            Severity::Medium
        }
        fn description(&self) -> &'static str {
            "needs catalog"
        }
        fn minimum_completeness(&self) -> CompletenessRequirement {
            CompletenessRequirement {
                requires_catalog: true,
                ..CompletenessRequirement::default()
            }
        }
        fn scan(&self, _ctx: &ScanContext<'_>) -> RuleOutput {
            RuleOutput::default().finding(finding(
                self.id(),
                Severity::Medium,
                "cat",
                "f",
                1,
                (0, 1),
            ))
        }
    }

    struct NeedsPrivilegeFacts;
    impl Rule for NeedsPrivilegeFacts {
        fn id(&self) -> &'static str {
            "SAST-TEST-FACT"
        }
        fn default_severity(&self) -> Severity {
            Severity::Low
        }
        fn description(&self) -> &'static str {
            "needs privilege facts"
        }
        fn required_facts(&self) -> &'static [FactKind] {
            &[FactKind::Privilege]
        }
        fn scan(&self, _ctx: &ScanContext<'_>) -> RuleOutput {
            RuleOutput::default().finding(finding(self.id(), Severity::Low, "fact", "f", 1, (0, 1)))
        }
    }

    fn one_unit<'a>(env: &'a FlowEnv) -> Vec<ScanUnit<'a>> {
        vec![ScanUnit {
            unit_logical_id: "hr.proc",
            source_file: "hr/proc.sql",
            flow: env,
        }]
    }

    #[test]
    fn rule_with_met_preconditions_runs() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot::default();
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysFinds)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        assert_eq!(r.rules_run, 1);
        assert_eq!(r.rules_gated, 0);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.max_severity(), Some(Severity::High));
    }

    #[test]
    fn completeness_gate_skips_not_silently() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot {
            catalog_available: false,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(NeedsCatalog)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        assert_eq!(r.rules_run, 0);
        assert_eq!(r.rules_gated, 1);
        assert!(r.findings.is_empty(), "must NOT run blind");
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, SkipReason::PreconditionUnmet);
        assert_eq!(r.skipped[0].rule_id, "SAST-TEST-CAT");
    }

    #[test]
    fn completeness_gate_passes_when_catalog_present() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot {
            catalog_available: true,
            ..CompletenessSnapshot::default()
        };
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(NeedsCatalog)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        assert_eq!(r.rules_run, 1);
        assert_eq!(r.findings.len(), 1);
    }

    #[test]
    fn required_facts_gate_skips_when_store_empty() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot::default();
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(NeedsPrivilegeFacts)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        assert_eq!(r.rules_gated, 1);
        assert_eq!(r.skipped[0].reason, SkipReason::MissingFlowFacts);
        assert!(r.skipped[0].detail.contains("Privilege"));
    }

    #[test]
    fn required_facts_gate_passes_when_kind_present() {
        let env = FlowEnv::default();
        let mut facts = FactStore::default();
        facts.push(mint_fact(
            FactProvenance {
                component: "plsql-sast-test".to_string(),
                component_version: "0".to_string(),
                run_id: String::new(),
            },
            FactPayload::Privilege {
                grantee: "R".to_string(),
                privilege: "SELECT".to_string(),
                on: "T".to_string(),
            },
        ));
        let snap = CompletenessSnapshot::default();
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(NeedsPrivilegeFacts)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        assert_eq!(r.rules_run, 1, "Privilege facts present -> rule runs");
        assert_eq!(r.findings.len(), 1);
    }

    #[test]
    fn recovered_ratio_ceiling_enforced() {
        let snap = CompletenessSnapshot {
            files_total: 10,
            files_recovered: 6,
            ..CompletenessSnapshot::default()
        };
        let req = CompletenessRequirement {
            max_recovered_ratio: Some(0.5),
            ..CompletenessRequirement::default()
        };
        assert!(req.unmet_against(&snap).is_some());
        let ok = CompletenessSnapshot {
            files_total: 10,
            files_recovered: 4,
            ..CompletenessSnapshot::default()
        };
        assert!(req.unmet_against(&ok).is_none());
    }

    #[test]
    fn output_is_deterministic_regardless_of_registry_order() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot::default();
        let a: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysFinds), Box::new(NeedsPrivilegeFacts)];
        let b: Vec<Box<dyn Rule>> = vec![Box::new(NeedsPrivilegeFacts), Box::new(AlwaysFinds)];
        let ra = run_scan(&a, &one_unit(&env), &facts, &snap);
        let rb = run_scan(&b, &one_unit(&env), &facts, &snap);
        assert_eq!(ra, rb, "aggregate must be order-independent");
    }

    #[test]
    fn report_round_trips_through_json() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let snap = CompletenessSnapshot::default();
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysFinds)];
        let r = run_scan(&rules, &one_unit(&env), &facts, &snap);
        let json = serde_json::to_string(&r).unwrap();
        let back: ScanReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
