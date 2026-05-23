#![forbid(unsafe_code)]

//! `plsql-sast` — static-analysis security rule engine
//! (Layer 3).
//!
//! This module defines the rule-engine *contract*: the [`Rule`]
//! trait every check implements, the [`ScanContext`] a rule
//! reads from, the [`Finding`] it emits, and the
//! [`RuleSkippedDiagnostic`] it records when it cannot run
//! soundly (R13 — no silent drops). Concrete rules implement
//! `Rule`; the engine driver walks a registry and aggregates results.
//!
//! Layering: Layer 3 depends on Layer 2's `plsql-ir`
//! (`FlowQuery`, `FactStore`) — never the reverse. The
//! `ScanContext` borrows the Layer 2 outputs so a rule never
//! re-derives flow.
//!
//! ## /oracle evidence
//!
//! * `SECURITY-OPTIONS-REFERENCE.md` — the SAST rule families
//!   (SQL injection, privilege escalation, definer-rights
//!   misuse) map to the security-options reference.
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — the
//!   constructs a rule inspects (dynamic SQL, AUTHID, grants).

use plsql_core::{Confidence, ConfidenceLevel};
use plsql_ir::{FactKind, FactStore, FlowQuery};
use serde::{Deserialize, Serialize};

pub mod baseline;
pub mod doctor;
pub mod false_positive;
pub mod fingerprint;
pub mod harness;
pub mod junit;
pub mod rules;
pub mod rules_qual;
pub mod sarif;
pub mod suppress;
pub use baseline::{Baseline, BaselineResult, apply_baseline, build_baseline};
pub use doctor::{RuleFiringHistogram, RuleFiringRow, rule_firing_histogram};
pub use false_positive::{FalsePositiveReport, NegativeCase, measure_false_positives};
pub use fingerprint::{FindingFingerprint, fingerprint};
pub use harness::{CompletenessSnapshot, ScanReport, ScanUnit, run_scan};
pub use junit::to_junit_xml;
pub use rules::{
    Dep001CrossSchemaWrite, Perf001CursorForLoopBulkCollect, Perf002CursorForLoopForall,
    Perf003IsNullOnIndexedColumn, Qual002LogWithoutReraise, Qual003UnboundedBulkCollect,
    Qual005DeprecatedFeature, Qual006MutatingTableTrigger, Qual007DmlInFunction,
    Qual008DeterministicMisuse, Sec001ExecuteImmediateInjection, Sec002DbmsSqlParse,
    Sec003HardcodedCredentials, Sec004InvokerRights, Sec005SensitivePublicSynonym,
    Sec006GrantToPublic, Sec007RefCursorReturn, Style001MissingInstrumentation,
};
pub use rules_qual::{Qual001WhenOthersThenNull, Qual004TxnControlInHandler};
pub use sarif::{SarifLog, to_sarif};
pub use suppress::{
    SuppressedFinding, SuppressionConfig, SuppressionOutcome, SuppressionReason, apply_suppressions,
};

/// Severity of a [`Finding`]. Ordered: `Info < Low < Medium <
/// High < Critical`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Where a finding points. 1-based line; `byte_span` is the
/// half-open `[start, end)` offset into the source file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingLocation {
    pub file: String,
    pub line: u32,
    pub byte_span: (u32, u32),
}

/// A single rule violation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable rule identifier (`SAST-INJECTION-001`, …).
    pub rule_id: String,
    pub severity: Severity,
    /// One-line human-readable summary.
    pub message: String,
    pub location: FindingLocation,
    /// Confidence the rule attaches — drives the report's
    /// must-fix vs review-queue partition.
    pub confidence: Confidence,
    /// Optional remediation hint the report renders verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

/// Recorded when a rule cannot run soundly on a unit (missing
/// flow facts, opaque dynamic SQL, parser-recovered region).
/// R13: never a silent skip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSkippedDiagnostic {
    pub rule_id: String,
    /// Logical id of the unit the rule was asked to scan.
    pub unit: String,
    pub reason: SkipReason,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The flow facts the rule needs were not computed (e.g.
    /// the unit failed to parse / lower).
    MissingFlowFacts,
    /// The construct the rule targets was opaque (dynamic SQL
    /// through DBMS_SQL, db-link, wrapped source).
    OpaqueConstruct,
    /// The rule explicitly opted out for this unit (e.g. a
    /// `@sast:ignore` annotation).
    SuppressedByAnnotation,
    /// The rule's preconditions were not met (wrong object kind).
    PreconditionUnmet,
}

/// What a [`Rule`] returns from one scan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleOutput {
    pub findings: Vec<Finding>,
    pub skipped: Vec<RuleSkippedDiagnostic>,
}

impl RuleOutput {
    pub fn finding(mut self, f: Finding) -> Self {
        self.findings.push(f);
        self
    }
    pub fn skip(mut self, s: RuleSkippedDiagnostic) -> Self {
        self.skipped.push(s);
        self
    }
    /// Highest severity among findings, if any.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}

/// Read-only view a rule gets for one unit. Borrows Layer 2
/// outputs; a rule never mutates analysis state.
pub struct ScanContext<'a> {
    /// Logical id of the routine / object under scan.
    pub unit_logical_id: &'a str,
    /// Project-relative source path (for `Finding.location`).
    pub source_file: &'a str,
    /// Flow query facade (FLOW-005) — taint / string-shape.
    pub flow: FlowQuery<'a>,
    /// Normalized fact store (FACT-001) for declaration /
    /// reference / edge lookups.
    pub facts: &'a FactStore,
}

impl<'a> ScanContext<'a> {
    #[must_use]
    pub fn new(
        unit_logical_id: &'a str,
        source_file: &'a str,
        flow: FlowQuery<'a>,
        facts: &'a FactStore,
    ) -> Self {
        Self {
            unit_logical_id,
            source_file,
            flow,
            facts,
        }
    }

    /// Helper: build a `RuleSkippedDiagnostic` scoped to this
    /// unit so rules don't repeat the boilerplate.
    #[must_use]
    pub fn skip(&self, rule_id: &str, reason: SkipReason, detail: &str) -> RuleSkippedDiagnostic {
        RuleSkippedDiagnostic {
            rule_id: rule_id.to_string(),
            unit: self.unit_logical_id.to_string(),
            reason,
            detail: detail.to_string(),
        }
    }
}

/// Minimum analysis completeness a rule needs before the harness
/// will run it. A rule whose evidence depends on the catalog (or
/// PL/Scope, or a low parser-recovery ratio) declares it here;
/// the harness gates on it and records a typed skip rather than
/// running the rule on insufficient inputs and emitting noise or
/// false negatives (R13 — the gap is reported, never silent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompletenessRequirement {
    /// Rule needs a resolved data-dictionary catalog.
    pub requires_catalog: bool,
    /// Rule needs PL/Scope identifier data.
    pub requires_plscope: bool,
    /// Reject the run if more than this fraction of files were
    /// parser-recovered (`None` = no ceiling).
    pub max_recovered_ratio: Option<f32>,
}

impl CompletenessRequirement {
    /// Return `Some(reason)` if `snapshot` fails this requirement.
    #[must_use]
    pub fn unmet_against(&self, snapshot: &CompletenessSnapshot) -> Option<String> {
        if self.requires_catalog && !snapshot.catalog_available {
            return Some("rule requires catalog; analysis ran without one".to_string());
        }
        if self.requires_plscope && !snapshot.plscope_available {
            return Some("rule requires PL/Scope data; not available".to_string());
        }
        if let Some(max) = self.max_recovered_ratio {
            let r = snapshot.recovered_ratio();
            if r > max {
                return Some(format!(
                    "parser-recovered ratio {r:.3} exceeds rule ceiling {max:.3}"
                ));
            }
        }
        None
    }
}

/// The contract every SAST check implements. Rules are pure
/// functions of the `ScanContext` — no I/O, no global state —
/// so the engine can run them in any order / in parallel.
pub trait Rule {
    /// Stable identifier, e.g. `SAST-INJECTION-001`.
    fn id(&self) -> &'static str;
    /// Default severity findings carry (a rule may downgrade
    /// per-finding via the returned `Finding.severity`).
    fn default_severity(&self) -> Severity;
    /// One-line description for `--list-rules` / docs.
    fn description(&self) -> &'static str;
    /// Fact families the rule needs present in the `FactStore`.
    /// If the store has zero facts of *any* required kind the
    /// harness skips the rule with [`SkipReason::MissingFlowFacts`]
    /// instead of letting it run blind. Default: none required.
    fn required_facts(&self) -> &'static [FactKind] {
        &[]
    }
    /// Minimum analysis completeness this rule needs. Default:
    /// no requirement (runs on any run).
    fn minimum_completeness(&self) -> CompletenessRequirement {
        CompletenessRequirement::default()
    }
    /// Run the rule against one unit.
    fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput;
}

/// Convenience constructor for a finding with the rule's default
/// severity + a `High`-confidence stamp (rules lower confidence
/// explicitly when the evidence is heuristic).
#[must_use]
pub fn finding(
    rule_id: &str,
    severity: Severity,
    message: &str,
    file: &str,
    line: u32,
    byte_span: (u32, u32),
) -> Finding {
    Finding {
        rule_id: rule_id.to_string(),
        severity,
        message: message.to_string(),
        location: FindingLocation {
            file: file.to_string(),
            line,
            byte_span,
        },
        confidence: Confidence {
            level: ConfidenceLevel::High,
            explanation: None,
        },
        remediation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_ir::{FactStore, FlowEnv, FlowQuery};

    struct AlwaysFinds;
    impl Rule for AlwaysFinds {
        fn id(&self) -> &'static str {
            "SAST-TEST-001"
        }
        fn default_severity(&self) -> Severity {
            Severity::High
        }
        fn description(&self) -> &'static str {
            "test rule that always reports"
        }
        fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
            RuleOutput::default().finding(finding(
                self.id(),
                self.default_severity(),
                "always",
                ctx.source_file,
                1,
                (0, 4),
            ))
        }
    }

    struct AlwaysSkips;
    impl Rule for AlwaysSkips {
        fn id(&self) -> &'static str {
            "SAST-TEST-002"
        }
        fn default_severity(&self) -> Severity {
            Severity::Medium
        }
        fn description(&self) -> &'static str {
            "test rule that always skips"
        }
        fn scan(&self, ctx: &ScanContext<'_>) -> RuleOutput {
            RuleOutput::default().skip(ctx.skip(
                self.id(),
                SkipReason::OpaqueConstruct,
                "dynamic SQL via DBMS_SQL",
            ))
        }
    }

    fn ctx_fixture<'a>(env: &'a FlowEnv, facts: &'a FactStore) -> ScanContext<'a> {
        ScanContext::new("hr.proc", "hr/proc.sql", FlowQuery::new(env), facts)
    }

    #[test]
    fn severity_orders_correctly() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Info < Severity::Low);
    }

    #[test]
    fn rule_that_finds_produces_finding() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let ctx = ctx_fixture(&env, &facts);
        let out = AlwaysFinds.scan(&ctx);
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].rule_id, "SAST-TEST-001");
        assert_eq!(out.findings[0].severity, Severity::High);
        assert_eq!(out.max_severity(), Some(Severity::High));
    }

    #[test]
    fn rule_that_skips_records_diagnostic() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let ctx = ctx_fixture(&env, &facts);
        let out = AlwaysSkips.scan(&ctx);
        assert!(out.findings.is_empty());
        assert_eq!(out.skipped.len(), 1);
        assert_eq!(out.skipped[0].reason, SkipReason::OpaqueConstruct);
        assert_eq!(out.skipped[0].unit, "hr.proc");
        assert_eq!(out.max_severity(), None);
    }

    #[test]
    fn rule_trait_object_dispatch_works() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let ctx = ctx_fixture(&env, &facts);
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysFinds), Box::new(AlwaysSkips)];
        let mut total_findings = 0;
        let mut total_skips = 0;
        for r in &rules {
            let o = r.scan(&ctx);
            total_findings += o.findings.len();
            total_skips += o.skipped.len();
        }
        assert_eq!(total_findings, 1);
        assert_eq!(total_skips, 1);
    }

    #[test]
    fn finding_default_confidence_is_high() {
        let f = finding("R", Severity::Low, "m", "f.sql", 3, (1, 2));
        assert_eq!(f.confidence.level, ConfidenceLevel::High);
        assert!(f.remediation.is_none());
    }

    #[test]
    fn finding_serde_round_trip_snake_case() {
        let f = finding("R", Severity::Critical, "m", "f.sql", 1, (0, 9));
        let json = serde_json::to_string(&f).unwrap();
        let back: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
        assert!(json.contains("\"severity\":\"critical\""));
    }

    #[test]
    fn skip_diagnostic_serde_round_trip() {
        let s = RuleSkippedDiagnostic {
            rule_id: "R".into(),
            unit: "u".into(),
            reason: SkipReason::MissingFlowFacts,
            detail: "no facts".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: RuleSkippedDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        assert!(json.contains("missing_flow_facts"));
    }

    #[test]
    fn rule_metadata_accessible_without_scan() {
        let r = AlwaysFinds;
        assert_eq!(r.id(), "SAST-TEST-001");
        assert_eq!(r.default_severity(), Severity::High);
        assert!(r.description().contains("test rule"));
    }

    #[test]
    fn context_skip_helper_scopes_to_unit() {
        let env = FlowEnv::default();
        let facts = FactStore::default();
        let ctx = ctx_fixture(&env, &facts);
        let s = ctx.skip("R", SkipReason::PreconditionUnmet, "wrong kind");
        assert_eq!(s.unit, "hr.proc");
        assert_eq!(s.reason, SkipReason::PreconditionUnmet);
    }

    #[test]
    fn rule_output_builder_chains() {
        let o = RuleOutput::default()
            .finding(finding("R", Severity::Low, "a", "f", 1, (0, 1)))
            .finding(finding("R", Severity::High, "b", "f", 2, (1, 2)));
        assert_eq!(o.findings.len(), 2);
        assert_eq!(o.max_severity(), Some(Severity::High));
    }
}
