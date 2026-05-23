//! `--baseline` mode for incremental adoption.
//!
//! A team turning SAST on against a large existing codebase cannot
//! fix every pre-existing finding at once. The baseline workflow
//! lets them *accept* the current set of findings as known debt and
//! gate CI only on **new** findings:
//!
//! 1. `build_baseline(&ScanReport)` snapshots the current findings'
//!    stable [`fingerprint`](crate::fingerprint) `primary` keys into
//!    a serde-able [`Baseline`] they commit to the repo.
//! 2. On every later scan, `apply_baseline(&ScanReport, &Baseline)`
//!    partitions findings into *new* (fail CI) vs *baselined*
//!    (already-known debt, suppressed) and reports how many of the
//!    baseline entries were *fixed* (so the baseline can be
//!    tightened — adoption ratchets forward, never backward).
//!
//! Identity is the fingerprint `primary` (rule + normalized
//! finding), deliberately **not** `location`: a baselined finding
//! stays baselined when surrounding code shifts its line, so the
//! baseline does not churn on unrelated edits.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Finding, ScanReport, fingerprint};

/// Committed baseline of accepted (known-debt) findings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub schema_id: String,
    pub schema_version: u32,
    /// Sorted, de-duplicated `FindingFingerprint::primary` keys.
    pub accepted: Vec<String>,
}

const SCHEMA_ID: &str = "plsql.sast.baseline";
const SCHEMA_VERSION: u32 = 1;

/// Snapshot every finding in `report` as accepted known debt.
#[must_use]
pub fn build_baseline(report: &ScanReport) -> Baseline {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for f in &report.findings {
        set.insert(fingerprint(f).primary);
    }
    Baseline {
        schema_id: SCHEMA_ID.to_string(),
        schema_version: SCHEMA_VERSION,
        accepted: set.into_iter().collect(),
    }
}

/// Result of applying a baseline to a fresh scan.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaselineResult {
    /// Findings NOT in the baseline — CI should fail on these.
    pub new_findings: Vec<Finding>,
    /// Count of findings suppressed because they were baselined.
    pub suppressed: usize,
    /// Baseline entries with no matching finding this run — these
    /// were fixed; the baseline can be tightened to drop them.
    pub fixed: Vec<String>,
}

impl BaselineResult {
    /// `true` when there is no new (non-baselined) finding.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.new_findings.is_empty()
    }
}

/// Partition `report` against `baseline`: surface only findings
/// whose fingerprint `primary` is not accepted, count suppressed
/// ones, and list baseline entries that no longer occur (fixed).
#[must_use]
pub fn apply_baseline(report: &ScanReport, baseline: &Baseline) -> BaselineResult {
    let accepted: BTreeSet<&str> = baseline.accepted.iter().map(String::as_str).collect();
    let mut new_findings = Vec::new();
    let mut suppressed = 0usize;
    let mut still_present: BTreeSet<String> = BTreeSet::new();

    for f in &report.findings {
        let key = fingerprint(f).primary;
        if accepted.contains(key.as_str()) {
            suppressed += 1;
            still_present.insert(key);
        } else {
            new_findings.push(f.clone());
        }
    }

    let fixed: Vec<String> = baseline
        .accepted
        .iter()
        .filter(|k| !still_present.contains(k.as_str()))
        .cloned()
        .collect();

    BaselineResult {
        new_findings,
        suppressed,
        fixed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Finding, FindingLocation, ScanReport, Severity};
    use plsql_core::{Confidence, ConfidenceLevel};

    fn fnd(rule: &str, msg: &str, line: u32) -> Finding {
        Finding {
            rule_id: rule.to_string(),
            severity: Severity::Medium,
            message: msg.to_string(),
            location: FindingLocation {
                file: "a.sql".to_string(),
                line,
                byte_span: (0, 0),
            },
            confidence: Confidence {
                level: ConfidenceLevel::High,
                explanation: None,
            },
            remediation: None,
        }
    }

    fn report(findings: Vec<Finding>) -> ScanReport {
        ScanReport {
            findings,
            ..ScanReport::default()
        }
    }

    #[test]
    fn build_baseline_captures_all_fingerprints_sorted_unique() {
        let b = build_baseline(&report(vec![
            fnd("SEC001", "x", 1),
            fnd("SEC001", "x", 1),
            fnd("QUAL002", "y", 2),
        ]));
        assert_eq!(b.schema_id, "plsql.sast.baseline");
        // Two distinct fingerprints (dup collapsed), sorted.
        assert_eq!(b.accepted.len(), 2);
        let mut s = b.accepted.clone();
        s.sort();
        assert_eq!(b.accepted, s);
    }

    #[test]
    fn apply_baseline_surfaces_only_new_findings() {
        let base = build_baseline(&report(vec![fnd("SEC001", "old", 1)]));
        let r = apply_baseline(
            &report(vec![fnd("SEC001", "old", 1), fnd("QUAL002", "new", 9)]),
            &base,
        );
        assert_eq!(r.new_findings.len(), 1);
        assert_eq!(r.new_findings[0].rule_id, "QUAL002");
        assert_eq!(r.suppressed, 1);
        assert!(!r.is_clean());
    }

    #[test]
    fn baselined_finding_stays_suppressed_when_line_moves() {
        // Identity is `primary` (rule + normalized finding), not
        // location — a line shift must not un-baseline it.
        let base = build_baseline(&report(vec![fnd("SEC001", "same", 10)]));
        let r = apply_baseline(&report(vec![fnd("SEC001", "same", 999)]), &base);
        assert_eq!(r.suppressed, 1);
        assert!(
            r.is_clean(),
            "line move must not resurface a baselined finding"
        );
    }

    #[test]
    fn fixed_entries_are_reported_for_ratcheting() {
        let base = build_baseline(&report(vec![
            fnd("SEC001", "kept", 1),
            fnd("QUAL002", "fixed", 2),
        ]));
        let r = apply_baseline(&report(vec![fnd("SEC001", "kept", 1)]), &base);
        assert_eq!(r.fixed.len(), 1, "the QUAL002 finding was fixed");
        assert!(r.is_clean());
    }

    #[test]
    fn empty_baseline_passes_everything_through_as_new() {
        let r = apply_baseline(&report(vec![fnd("SEC001", "x", 1)]), &Baseline::default());
        assert_eq!(r.new_findings.len(), 1);
        assert_eq!(r.suppressed, 0);
    }

    #[test]
    fn serde_round_trip_stable_schema() {
        let b = build_baseline(&report(vec![fnd("SEC001", "x", 1)]));
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("\"schema_id\":\"plsql.sast.baseline\""));
        let back: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }
}
