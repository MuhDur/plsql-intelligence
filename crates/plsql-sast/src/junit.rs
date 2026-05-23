//! JUnit XML output formatter.
//!
//! Renders a [`ScanReport`](crate::ScanReport) as a JUnit-style
//! XML document so SAST results show up in CI test panes
//! (GitLab, Jenkins, Azure Pipelines, Buildkite) that ingest
//! JUnit but not SARIF.
//!
//! Mapping:
//! * each [`Finding`](crate::Finding) → a `<testcase>` with a
//!   `<failure>` (so the build is red on a real violation);
//! * each [`RuleSkippedDiagnostic`](crate::RuleSkippedDiagnostic)
//!   → a `<testcase>` with a `<skipped>` element — JUnit's
//!   native "not asserted" channel, so an R13 skip is *visible*
//!   in CI rather than silently absent;
//! * `tests` / `failures` / `skipped` counts on `<testsuite>`
//!   are exact.
//!
//! Output is deterministic: cases follow the already-sorted
//! `ScanReport` order. All dynamic text is XML-escaped.

use crate::ScanReport;

/// Minimal, allocation-light XML text/attribute escaper. JUnit
/// readers are strict, so `&<>"'` are all escaped (attribute and
/// element-content safe with one routine).
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip control chars XML 1.0 forbids (keep \t \n \r).
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Render `report` as a JUnit XML document under one
/// `<testsuite name=…>`.
#[must_use]
pub fn to_junit_xml(report: &ScanReport, suite_name: &str) -> String {
    let failures = report.findings.len();
    let skipped = report.skipped.len();
    let tests = failures + skipped;

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<testsuites>\n");
    x.push_str(&format!(
        "  <testsuite name=\"{}\" tests=\"{tests}\" failures=\"{failures}\" skipped=\"{skipped}\">\n",
        xml_escape(suite_name)
    ));

    for f in &report.findings {
        let case_name = format!("{}: {}:{}", f.rule_id, f.location.file, f.location.line);
        x.push_str(&format!(
            "    <testcase name=\"{}\" classname=\"{}\">\n",
            xml_escape(&case_name),
            xml_escape(&f.rule_id)
        ));
        x.push_str(&format!(
            "      <failure message=\"{}\" type=\"{:?}\">{}</failure>\n",
            xml_escape(&f.message),
            f.severity,
            xml_escape(&format!(
                "{} at {}:{} [{}..{}]",
                f.message,
                f.location.file,
                f.location.line,
                f.location.byte_span.0,
                f.location.byte_span.1
            ))
        ));
        x.push_str("    </testcase>\n");
    }

    for s in &report.skipped {
        let case_name = format!("{}: {}", s.rule_id, s.unit);
        x.push_str(&format!(
            "    <testcase name=\"{}\" classname=\"{}\">\n",
            xml_escape(&case_name),
            xml_escape(&s.rule_id)
        ));
        x.push_str(&format!(
            "      <skipped message=\"{:?}: {}\"/>\n",
            s.reason,
            xml_escape(&s.detail)
        ));
        x.push_str("    </testcase>\n");
    }

    x.push_str("  </testsuite>\n");
    x.push_str("</testsuites>\n");
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuleSkippedDiagnostic, ScanReport, Severity, SkipReason, finding};

    #[test]
    fn empty_report_is_well_formed_zero_counts() {
        let xml = to_junit_xml(&ScanReport::default(), "plsql-sast");
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("tests=\"0\" failures=\"0\" skipped=\"0\""));
        assert!(xml.contains("name=\"plsql-sast\""));
        assert!(xml.trim_end().ends_with("</testsuites>"));
    }

    #[test]
    fn finding_becomes_failure_case() {
        let r = ScanReport {
            findings: vec![finding(
                "SEC001",
                Severity::Critical,
                "tainted",
                "hr.sql",
                12,
                (3, 9),
            )],
            ..ScanReport::default()
        };
        let xml = to_junit_xml(&r, "s");
        assert!(xml.contains("tests=\"1\" failures=\"1\" skipped=\"0\""));
        assert!(xml.contains("classname=\"SEC001\""));
        assert!(xml.contains("<failure message=\"tainted\" type=\"Critical\">"));
        assert!(xml.contains("hr.sql:12 [3..9]"));
    }

    #[test]
    fn skipped_diagnostic_becomes_skipped_case_visible_in_ci() {
        let r = ScanReport {
            skipped: vec![RuleSkippedDiagnostic {
                rule_id: "SEC001".into(),
                unit: "hr.proc".into(),
                reason: SkipReason::OpaqueConstruct,
                detail: "DBMS_SQL".into(),
            }],
            ..ScanReport::default()
        };
        let xml = to_junit_xml(&r, "s");
        assert!(xml.contains("tests=\"1\" failures=\"0\" skipped=\"1\""));
        assert!(xml.contains("<skipped message=\"OpaqueConstruct: DBMS_SQL\"/>"));
    }

    #[test]
    fn xml_special_chars_are_escaped() {
        let r = ScanReport {
            findings: vec![finding(
                "R&D",
                Severity::High,
                "a < b && c > \"d\" 'e'",
                "f.sql",
                1,
                (0, 1),
            )],
            ..ScanReport::default()
        };
        let xml = to_junit_xml(&r, "su<i>te");
        assert!(xml.contains("name=\"su&lt;i&gt;te\""));
        assert!(xml.contains("&amp;&amp;"));
        assert!(xml.contains("&lt; b"));
        assert!(xml.contains("&quot;d&quot;"));
        assert!(xml.contains("&apos;e&apos;"));
        assert!(!xml.contains("a < b"), "raw < must not leak");
    }

    #[test]
    fn counts_sum_findings_and_skips() {
        let r = ScanReport {
            findings: vec![
                finding("A", Severity::Low, "x", "f", 1, (0, 1)),
                finding("B", Severity::High, "y", "f", 2, (0, 1)),
            ],
            skipped: vec![RuleSkippedDiagnostic {
                rule_id: "C".into(),
                unit: "u".into(),
                reason: SkipReason::MissingFlowFacts,
                detail: "d".into(),
            }],
            ..ScanReport::default()
        };
        let xml = to_junit_xml(&r, "s");
        assert!(xml.contains("tests=\"3\" failures=\"2\" skipped=\"1\""));
    }

    #[test]
    fn control_characters_are_stripped() {
        let r = ScanReport {
            findings: vec![finding(
                "R",
                Severity::Info,
                "bad\u{0007}bell\u{0000}nul\ttab-ok",
                "f",
                1,
                (0, 1),
            )],
            ..ScanReport::default()
        };
        let xml = to_junit_xml(&r, "s");
        assert!(xml.contains("badbellnul\ttab-ok"));
        assert!(!xml.contains('\u{0007}'));
        assert!(!xml.contains('\u{0000}'));
    }
}
