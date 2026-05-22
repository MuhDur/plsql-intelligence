//! Per-rule firing histogram doctor (`PLSQL-SAST-026`).
//!
//! Aggregates a [`ScanReport`](crate::ScanReport) into a stable,
//! serde-able per-rule view so an operator can answer "which rules
//! fired, how often, how severe, and which were gated/skipped?"
//! without re-reading the raw finding list. Follows the
//! project-wide doctor convention: one stable shape, deterministic
//! ordering, derivable in `O(n)` from the report.

use serde::{Deserialize, Serialize};

use crate::{ScanReport, Severity};

/// One row of the per-rule histogram.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleFiringRow {
    pub rule_id: String,
    /// Number of findings this rule emitted.
    pub findings: usize,
    /// Highest severity among this rule's findings (`None` when the
    /// rule only appears in the skipped list).
    pub max_severity: Option<Severity>,
    /// Number of units on which this rule was skipped (R13 typed
    /// skip — missing facts, opaque construct, …).
    pub skipped: usize,
}

/// Aggregated firing histogram for one scan.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleFiringHistogram {
    pub schema_id: String,
    pub schema_version: u32,
    /// Per-rule rows, sorted by `findings` desc then `rule_id` asc
    /// (deterministic).
    pub rows: Vec<RuleFiringRow>,
    pub total_findings: usize,
    pub total_skipped: usize,
    /// Rules the harness gated before running (insufficient
    /// completeness / missing required facts).
    pub rules_gated: usize,
}

const SCHEMA_ID: &str = "plsql.sast.rule_firing_histogram";
const SCHEMA_VERSION: u32 = 1;

/// Build the per-rule firing histogram from a [`ScanReport`].
///
/// Every `rule_id` that appears in `findings` or `skipped` gets
/// exactly one row. `O(n)` over findings + skipped.
#[must_use]
pub fn rule_firing_histogram(report: &ScanReport) -> RuleFiringHistogram {
    // (rule_id) -> (findings, max_severity, skipped), insertion-order
    // independent because we sort at the end.
    let mut acc: Vec<(String, usize, Option<Severity>, usize)> = Vec::new();

    let slot = |acc: &mut Vec<(String, usize, Option<Severity>, usize)>, id: &str| -> usize {
        if let Some(i) = acc.iter().position(|(r, ..)| r == id) {
            i
        } else {
            acc.push((id.to_string(), 0, None, 0));
            acc.len() - 1
        }
    };

    for f in &report.findings {
        let i = slot(&mut acc, &f.rule_id);
        acc[i].1 += 1;
        acc[i].2 = Some(match acc[i].2 {
            Some(cur) => cur.max(f.severity),
            None => f.severity,
        });
    }
    for s in &report.skipped {
        let i = slot(&mut acc, &s.rule_id);
        acc[i].3 += 1;
    }

    let mut rows: Vec<RuleFiringRow> = acc
        .into_iter()
        .map(|(rule_id, findings, max_severity, skipped)| RuleFiringRow {
            rule_id,
            findings,
            max_severity,
            skipped,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.findings
            .cmp(&a.findings)
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });

    RuleFiringHistogram {
        schema_id: SCHEMA_ID.to_string(),
        schema_version: SCHEMA_VERSION,
        total_findings: report.findings.len(),
        total_skipped: report.skipped.len(),
        rules_gated: report.rules_gated,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Finding, FindingLocation, RuleSkippedDiagnostic, SkipReason};
    use plsql_core::{Confidence, ConfidenceLevel};

    fn fnd(rule: &str, sev: Severity) -> Finding {
        Finding {
            rule_id: rule.to_string(),
            severity: sev,
            message: "m".to_string(),
            location: FindingLocation {
                file: "f".to_string(),
                line: 0,
                byte_span: (0, 0),
            },
            confidence: Confidence {
                level: ConfidenceLevel::High,
                explanation: None,
            },
            remediation: None,
        }
    }

    fn skip(rule: &str) -> RuleSkippedDiagnostic {
        RuleSkippedDiagnostic {
            rule_id: rule.to_string(),
            unit: "u".to_string(),
            reason: SkipReason::MissingFlowFacts,
            detail: "d".to_string(),
        }
    }

    fn report(findings: Vec<Finding>, skipped: Vec<RuleSkippedDiagnostic>) -> ScanReport {
        ScanReport {
            findings,
            skipped,
            ..ScanReport::default()
        }
    }

    #[test]
    fn empty_report_yields_empty_histogram() {
        let h = rule_firing_histogram(&report(vec![], vec![]));
        assert!(h.rows.is_empty());
        assert_eq!(h.total_findings, 0);
        assert_eq!(h.schema_id, "plsql.sast.rule_firing_histogram");
        assert_eq!(h.schema_version, 1);
    }

    #[test]
    fn counts_and_max_severity_per_rule() {
        let h = rule_firing_histogram(&report(
            vec![
                fnd("SEC001", Severity::Critical),
                fnd("SEC001", Severity::Low),
                fnd("QUAL002", Severity::Medium),
            ],
            vec![skip("PERF001")],
        ));
        let sec = h.rows.iter().find(|r| r.rule_id == "SEC001").unwrap();
        assert_eq!(sec.findings, 2);
        assert_eq!(sec.max_severity, Some(Severity::Critical));
        let perf = h.rows.iter().find(|r| r.rule_id == "PERF001").unwrap();
        assert_eq!(perf.findings, 0);
        assert_eq!(perf.skipped, 1);
        assert_eq!(perf.max_severity, None);
        assert_eq!(h.total_findings, 3);
        assert_eq!(h.total_skipped, 1);
    }

    #[test]
    fn rows_sorted_by_findings_desc_then_rule_id() {
        let h = rule_firing_histogram(&report(
            vec![
                fnd("BBB", Severity::Low),
                fnd("AAA", Severity::Low),
                fnd("AAA", Severity::Low),
            ],
            vec![],
        ));
        // AAA has 2 findings → first; BBB has 1 → second.
        assert_eq!(h.rows[0].rule_id, "AAA");
        assert_eq!(h.rows[0].findings, 2);
        assert_eq!(h.rows[1].rule_id, "BBB");
    }

    #[test]
    fn tie_break_is_rule_id_ascending() {
        let h = rule_firing_histogram(&report(
            vec![fnd("ZZZ", Severity::Low), fnd("AAA", Severity::Low)],
            vec![],
        ));
        assert_eq!(h.rows[0].rule_id, "AAA");
        assert_eq!(h.rows[1].rule_id, "ZZZ");
    }

    #[test]
    fn serde_round_trip_stable_schema() {
        let h = rule_firing_histogram(&report(vec![fnd("SEC001", Severity::High)], vec![]));
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"schema_id\":\"plsql.sast.rule_firing_histogram\""));
        let back: RuleFiringHistogram = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }
}
