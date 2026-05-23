//! SARIF 2.1.0 output formatter.
//!
//! Renders a [`ScanReport`](crate::ScanReport) as a SARIF 2.1.0
//! log (OASIS `sarif-schema-2.1.0`) so findings load into GitHub
//! code-scanning, Azure DevOps, and any SARIF viewer.
//!
//! Scope: the SARIF subset that round-trips findings faithfully —
//! `runs[].tool.driver.{name,version,rules[]}` and
//! `runs[].results[]` with `partialFingerprints` taken from
//! [`fingerprint`](crate::fingerprint) so a baseline survives
//! line drift (the SAST-028 contract). Rule-skipped diagnostics
//! are *not* SARIF results (they are absence-of-analysis, not
//! violations); they remain in `ScanReport.skipped` for the
//! human/robot-JSON surfaces, so nothing is silently dropped
//! (R13). Output is deterministic — results preserve the
//! already-sorted `ScanReport` order and `rules[]` is sorted by
//! id.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Finding, ScanReport, Severity, fingerprint};

const SARIF_SCHEMA: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifDriver {
    pub name: String,
    #[serde(rename = "semanticVersion")]
    pub semantic_version: String,
    pub rules: Vec<SarifReportingDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifReportingDescriptor {
    pub id: String,
    #[serde(rename = "shortDescription")]
    pub short_description: SarifMessage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifResult {
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub level: String,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
    #[serde(rename = "partialFingerprints")]
    pub partial_fingerprints: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    pub artifact_location: SarifArtifactLocation,
    pub region: SarifRegion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SarifRegion {
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "byteOffset")]
    pub byte_offset: u32,
    #[serde(rename = "byteLength")]
    pub byte_length: u32,
}

/// SARIF `level`: `error` for must-fix (Critical/High),
/// `warning` for Medium, `note` for Low/Info. SARIF has no
/// `critical`, so severity nuance also rides in the message.
fn sarif_level(sev: Severity) -> &'static str {
    match sev {
        Severity::Critical | Severity::High => "error",
        Severity::Medium => "warning",
        Severity::Low | Severity::Info => "note",
    }
}

fn result_of(f: &Finding) -> SarifResult {
    let fp = fingerprint(f);
    let mut pf = BTreeMap::new();
    pf.insert("primary".to_string(), fp.primary);
    pf.insert("location".to_string(), fp.location);
    let (s, e) = f.location.byte_span;
    SarifResult {
        rule_id: f.rule_id.clone(),
        level: sarif_level(f.severity).to_string(),
        message: SarifMessage {
            text: f.message.clone(),
        },
        locations: vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: f.location.file.clone(),
                },
                region: SarifRegion {
                    // SARIF startLine is 1-based and must be >= 1;
                    // findings with no precise line (catalog/DDL
                    // facts) report line 0 — clamp to 1 so the
                    // log validates, the byte span stays exact.
                    start_line: f.location.line.max(1),
                    byte_offset: s,
                    byte_length: e.saturating_sub(s),
                },
            },
        }],
        partial_fingerprints: pf,
    }
}

/// Render `report` as a SARIF 2.1.0 log produced by `tool_name`
/// at `tool_version`.
#[must_use]
pub fn to_sarif(report: &ScanReport, tool_name: &str, tool_version: &str) -> SarifLog {
    // Distinct rule ids (sorted) -> reportingDescriptor[].
    let mut rule_ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
    rule_ids.sort_unstable();
    rule_ids.dedup();
    let rules = rule_ids
        .into_iter()
        .map(|id| SarifReportingDescriptor {
            id: id.to_string(),
            short_description: SarifMessage {
                text: format!("plsql-sast rule {id}"),
            },
        })
        .collect();

    // Results preserve ScanReport's already-deterministic order.
    let results = report.findings.iter().map(result_of).collect();

    SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: tool_name.to_string(),
                    semantic_version: tool_version.to_string(),
                    rules,
                },
            },
            results,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Finding, finding};

    fn report_with(findings: Vec<Finding>) -> ScanReport {
        ScanReport {
            findings,
            skipped: vec![],
            rules_run: 1,
            rules_gated: 0,
        }
    }

    #[test]
    fn empty_report_is_valid_sarif() {
        let log = to_sarif(&ScanReport::default(), "plsql-sast", "0.1.0");
        assert_eq!(log.version, "2.1.0");
        assert!(log.schema.contains("sarif-schema-2.1.0"));
        assert_eq!(log.runs.len(), 1);
        assert!(log.runs[0].results.is_empty());
        assert!(log.runs[0].tool.driver.rules.is_empty());
    }

    #[test]
    fn finding_maps_to_result_with_fingerprints() {
        let log = to_sarif(
            &report_with(vec![finding(
                "SEC001",
                Severity::Critical,
                "tainted reaches EXECUTE IMMEDIATE",
                "hr/proc.sql",
                12,
                (40, 60),
            )]),
            "plsql-sast",
            "0.1.0",
        );
        let r = &log.runs[0].results[0];
        assert_eq!(r.rule_id, "SEC001");
        assert_eq!(r.level, "error");
        assert_eq!(r.locations[0].physical_location.region.start_line, 12);
        assert_eq!(r.locations[0].physical_location.region.byte_offset, 40);
        assert_eq!(r.locations[0].physical_location.region.byte_length, 20);
        assert!(r.partial_fingerprints.contains_key("primary"));
        assert!(r.partial_fingerprints.contains_key("location"));
        // rule metadata emitted once.
        assert_eq!(log.runs[0].tool.driver.rules.len(), 1);
        assert_eq!(log.runs[0].tool.driver.rules[0].id, "SEC001");
    }

    #[test]
    fn severity_maps_to_sarif_level() {
        assert_eq!(sarif_level(Severity::Critical), "error");
        assert_eq!(sarif_level(Severity::High), "error");
        assert_eq!(sarif_level(Severity::Medium), "warning");
        assert_eq!(sarif_level(Severity::Low), "note");
        assert_eq!(sarif_level(Severity::Info), "note");
    }

    #[test]
    fn line_zero_is_clamped_so_log_validates() {
        let log = to_sarif(
            &report_with(vec![finding(
                "SEC006",
                Severity::High,
                "GRANT TO PUBLIC",
                "g.sql",
                0,
                (0, 0),
            )]),
            "t",
            "0",
        );
        assert_eq!(log.runs[0].results[0].physical_location_start_line(), 1);
    }

    #[test]
    fn rules_are_deduped_and_sorted() {
        let log = to_sarif(
            &report_with(vec![
                finding("SEC006", Severity::High, "a", "f", 1, (0, 1)),
                finding("SEC001", Severity::Critical, "b", "f", 2, (0, 1)),
                finding("SEC006", Severity::High, "c", "f", 3, (0, 1)),
            ]),
            "t",
            "0",
        );
        let ids: Vec<&str> = log.runs[0]
            .tool
            .driver
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids, vec!["SEC001", "SEC006"]);
        assert_eq!(log.runs[0].results.len(), 3, "every finding is a result");
    }

    #[test]
    fn sarif_round_trips_through_json() {
        let log = to_sarif(
            &report_with(vec![finding(
                "SEC002",
                Severity::Medium,
                "m",
                "f.sql",
                3,
                (1, 9),
            )]),
            "plsql-sast",
            "1.2.3",
        );
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("\"$schema\""));
        assert!(json.contains("\"version\":\"2.1.0\""));
        assert!(json.contains("\"partialFingerprints\""));
        let back: SarifLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, log);
    }

    // Small test-only accessor to keep the assertion readable.
    impl SarifResult {
        fn physical_location_start_line(&self) -> u32 {
            self.locations[0].physical_location.region.start_line
        }
    }
}
