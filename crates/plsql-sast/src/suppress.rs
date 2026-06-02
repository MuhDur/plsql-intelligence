//! Finding suppression.
//!
//! Two suppression channels, applied *after* the scan so a
//! suppressed finding is never lost — it moves to a dedicated
//! bucket with the reason recorded (R13: suppression is an
//! audited decision, not a silent drop):
//!
//! 1. **Config** — a [`SuppressionConfig`] of rule ids ignored
//!    project-wide or per source path.
//! 2. **Inline source comments**
//!    * `-- plsql-scan:ignore RULE[,RULE…]` — suppresses the
//!      listed rules on the **same source line** as the comment.
//!    * `-- plsql-scan:ignore-next-line RULE[,RULE…]` —
//!      suppresses them on the **following** line.
//!    * The token `*` (or `all`) suppresses any rule.
//!
//! The comment marker is matched case-insensitively and may sit
//! after code (`x := 1; -- plsql-scan:ignore SEC001`).
//!
//! ## Span-less (line-0) findings
//!
//! Many real findings are catalog/DDL-derived and carry no
//! precise source line (`location.line == 0`) — e.g. a
//! `GRANT … TO PUBLIC` (SEC006) attributed to the whole unit.
//! Line-keyed inline matching can never reach those, so for a
//! line-0 finding we fall back to **file-scoped** matching: any
//! inline `plsql-scan:ignore`/`ignore-next-line` directive in the
//! finding's file that names the rule suppresses it (the
//! directive's own line is recorded for the audit trail). This
//! keeps inline suppression honest for the span-less findings that
//! make up most real catalog/DDL results, without the user having
//! to guess a line number that does not exist.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Finding, ScanReport};

const MARKER: &str = "plsql-scan:";

/// Rule ids to suppress without a source annotation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressionConfig {
    /// Suppressed for every file.
    pub global_rules: BTreeSet<String>,
    /// Suppressed only for a specific project-relative path.
    pub per_path_rules: BTreeMap<String, BTreeSet<String>>,
}

/// Why a finding was suppressed (kept for the audit trail).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SuppressionReason {
    ConfigGlobal,
    ConfigPerPath,
    InlineSameLine { comment_line: u32 },
    InlineNextLine { comment_line: u32 },
}

/// A finding that was produced then suppressed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressedFinding {
    pub finding: Finding,
    pub reason: SuppressionReason,
}

/// Result of applying suppressions: what survived + the audited
/// list of what was suppressed and why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressionOutcome {
    pub kept: ScanReport,
    pub suppressed: Vec<SuppressedFinding>,
}

/// One parsed inline directive.
struct Directive {
    rules: RuleSet,
    reason: SuppressionReason,
}

/// Per-file inline directives parsed from source.
#[derive(Default)]
struct InlineIndex {
    /// suppressed line number → directives that target it.
    by_line: BTreeMap<u32, Vec<Directive>>,
}

#[derive(Default)]
struct RuleSet {
    all: bool,
    ids: BTreeSet<String>,
}

impl RuleSet {
    fn matches(&self, rule_id: &str) -> bool {
        self.all || self.ids.contains(rule_id)
    }
    fn add(&mut self, token: &str) {
        if token == "*" || token.eq_ignore_ascii_case("all") {
            self.all = true;
        } else {
            self.ids.insert(token.to_string());
        }
    }
}

fn parse_rule_list(rest: &str) -> RuleSet {
    let mut rs = RuleSet::default();
    for tok in rest
        .split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        rs.add(tok);
    }
    rs
}

/// Scan one file's source for inline directives. 1-based lines.
fn index_source(source: &str) -> InlineIndex {
    let mut idx = InlineIndex::default();
    for (i, raw) in source.lines().enumerate() {
        let line_no = (i + 1) as u32;
        let lower = raw.to_ascii_lowercase();
        let Some(mpos) = lower.find(MARKER) else {
            continue;
        };
        // Require it to be inside a `--` comment (Oracle line
        // comment). Anything before `--` is code we ignore.
        let Some(cpos) = raw.find("--") else {
            continue;
        };
        if mpos < cpos {
            continue;
        }
        let after = raw[mpos + MARKER.len()..].trim();
        let after_lower = after.to_ascii_lowercase();
        if let Some(rest) = after_lower.strip_prefix("ignore-next-line") {
            let rules = parse_rule_list(&after[after.len() - rest.len()..]);
            idx.by_line.entry(line_no + 1).or_default().push(Directive {
                rules,
                reason: SuppressionReason::InlineNextLine {
                    comment_line: line_no,
                },
            });
        } else if let Some(rest) = after_lower.strip_prefix("ignore") {
            let rules = parse_rule_list(&after[after.len() - rest.len()..]);
            idx.by_line.entry(line_no).or_default().push(Directive {
                rules,
                reason: SuppressionReason::InlineSameLine {
                    comment_line: line_no,
                },
            });
        }
    }
    idx
}

/// Resolve an inline suppression reason for `f` against one file's
/// parsed directives.
///
/// * A finding with a precise 1-based line matches a directive
///   targeting exactly that line (same-line / next-line).
/// * A **span-less** finding (`location.line == 0` — catalog/DDL
///   facts that point at the whole unit) cannot key on a line, so
///   it falls back to *file-scoped* matching: the first directive
///   in the file (lowest comment line) whose rule set covers
///   `f.rule_id` suppresses it. The directive's own comment line
///   is preserved in the recorded reason for the audit trail.
fn inline_reason_for(idx: &InlineIndex, f: &Finding) -> Option<SuppressionReason> {
    if f.location.line != 0 {
        return idx.by_line.get(&f.location.line).and_then(|dirs| {
            dirs.iter()
                .find(|d| d.rules.matches(&f.rule_id))
                .map(|d| d.reason.clone())
        });
    }
    // Span-less finding: scan every directive in the file in line
    // order (BTreeMap iterates ascending) so the audit trail is
    // deterministic — the earliest matching directive wins.
    idx.by_line
        .values()
        .flatten()
        .find(|d| d.rules.matches(&f.rule_id))
        .map(|d| d.reason.clone())
}

fn config_reason(config: &SuppressionConfig, f: &Finding) -> Option<SuppressionReason> {
    if config.global_rules.contains(&f.rule_id) {
        return Some(SuppressionReason::ConfigGlobal);
    }
    if let Some(rules) = config.per_path_rules.get(&f.location.file) {
        if rules.contains(&f.rule_id) {
            return Some(SuppressionReason::ConfigPerPath);
        }
    }
    None
}

/// Apply config + inline suppressions to `report`. `sources`
/// maps a finding's `location.file` to that file's text (omit a
/// file to apply config-only suppression to it).
#[must_use]
pub fn apply_suppressions(
    report: &ScanReport,
    config: &SuppressionConfig,
    sources: &BTreeMap<String, String>,
) -> SuppressionOutcome {
    let indexes: BTreeMap<&String, InlineIndex> = sources
        .iter()
        .map(|(path, src)| (path, index_source(src)))
        .collect();

    let mut kept = ScanReport {
        findings: Vec::new(),
        skipped: report.skipped.clone(),
        rules_run: report.rules_run,
        rules_gated: report.rules_gated,
    };
    let mut suppressed: Vec<SuppressedFinding> = Vec::new();

    for f in &report.findings {
        if let Some(reason) = config_reason(config, f) {
            suppressed.push(SuppressedFinding {
                finding: f.clone(),
                reason,
            });
            continue;
        }
        let inline_reason = indexes
            .get(&f.location.file)
            .and_then(|idx| inline_reason_for(idx, f));
        if let Some(reason) = inline_reason {
            suppressed.push(SuppressedFinding {
                finding: f.clone(),
                reason,
            });
            continue;
        }
        kept.findings.push(f.clone());
    }

    suppressed.sort_by(|a, b| {
        (
            &a.finding.rule_id,
            &a.finding.location.file,
            a.finding.location.line,
        )
            .cmp(&(
                &b.finding.rule_id,
                &b.finding.location.file,
                b.finding.location.line,
            ))
    });
    SuppressionOutcome { kept, suppressed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScanReport, Severity, finding};

    fn report(fs: Vec<Finding>) -> ScanReport {
        ScanReport {
            findings: fs,
            ..ScanReport::default()
        }
    }

    fn srcmap(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn config_global_suppresses_but_records() {
        let r = report(vec![finding(
            "SEC006",
            Severity::High,
            "m",
            "a.sql",
            5,
            (0, 1),
        )]);
        let cfg = SuppressionConfig {
            global_rules: ["SEC006".to_string()].into_iter().collect(),
            ..SuppressionConfig::default()
        };
        let out = apply_suppressions(&r, &cfg, &BTreeMap::new());
        assert!(out.kept.findings.is_empty());
        assert_eq!(out.suppressed.len(), 1);
        assert_eq!(out.suppressed[0].reason, SuppressionReason::ConfigGlobal);
    }

    #[test]
    fn config_per_path_only_targets_that_file() {
        let r = report(vec![
            finding("R", Severity::Low, "m", "x.sql", 1, (0, 1)),
            finding("R", Severity::Low, "m", "y.sql", 1, (0, 1)),
        ]);
        let mut per = BTreeMap::new();
        per.insert("x.sql".to_string(), ["R".to_string()].into_iter().collect());
        let cfg = SuppressionConfig {
            per_path_rules: per,
            ..SuppressionConfig::default()
        };
        let out = apply_suppressions(&r, &cfg, &BTreeMap::new());
        assert_eq!(out.kept.findings.len(), 1);
        assert_eq!(out.kept.findings[0].location.file, "y.sql");
        assert_eq!(out.suppressed[0].reason, SuppressionReason::ConfigPerPath);
    }

    #[test]
    fn inline_same_line_ignore_suppresses_that_line() {
        let src = "line1\nbad_call(); -- plsql-scan:ignore SEC001\nline3\n";
        let r = report(vec![finding(
            "SEC001",
            Severity::Critical,
            "inj",
            "f.sql",
            2,
            (0, 1),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert!(out.kept.findings.is_empty());
        assert_eq!(out.suppressed.len(), 1);
    }

    #[test]
    fn inline_next_line_suppresses_following_line() {
        let src = "-- plsql-scan:ignore-next-line SEC001\nbad_call();\nok();\n";
        let r = report(vec![
            finding("SEC001", Severity::Critical, "inj", "f.sql", 2, (0, 1)),
            finding("SEC001", Severity::Critical, "inj", "f.sql", 3, (0, 1)),
        ]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert_eq!(out.kept.findings.len(), 1, "only line 2 suppressed");
        assert_eq!(out.kept.findings[0].location.line, 3);
        assert_eq!(out.suppressed.len(), 1);
        assert_eq!(
            out.suppressed[0].reason,
            SuppressionReason::InlineNextLine { comment_line: 1 },
            "audit trail records the directive's own line"
        );
    }

    #[test]
    fn wildcard_token_suppresses_any_rule() {
        let src = "stuff -- plsql-scan:ignore *\n";
        let r = report(vec![
            finding("SEC001", Severity::High, "a", "f.sql", 1, (0, 1)),
            finding("QUAL003", Severity::Low, "b", "f.sql", 1, (0, 1)),
        ]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert!(out.kept.findings.is_empty());
        assert_eq!(out.suppressed.len(), 2);
    }

    #[test]
    fn non_matching_rule_is_not_suppressed() {
        let src = "x -- plsql-scan:ignore SEC001\n";
        let r = report(vec![finding(
            "SEC006",
            Severity::High,
            "m",
            "f.sql",
            1,
            (0, 1),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert_eq!(
            out.kept.findings.len(),
            1,
            "directive named a different rule"
        );
        assert!(out.suppressed.is_empty());
    }

    #[test]
    fn marker_outside_a_comment_is_ignored() {
        // `plsql-scan:` appears in a string literal, not a `--`
        // comment — must not suppress.
        let src = "v := 'plsql-scan:ignore SEC001';\n";
        let r = report(vec![finding(
            "SEC001",
            Severity::Critical,
            "m",
            "f.sql",
            1,
            (0, 1),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert_eq!(out.kept.findings.len(), 1);
    }

    #[test]
    fn comma_and_space_separated_rule_lists_parse() {
        let src = "z -- plsql-scan:ignore SEC001, SEC006 QUAL003\n";
        let r = report(vec![
            finding("SEC001", Severity::High, "a", "f.sql", 1, (0, 1)),
            finding("SEC006", Severity::High, "b", "f.sql", 1, (0, 1)),
            finding("QUAL003", Severity::Low, "c", "f.sql", 1, (0, 1)),
            finding("SEC002", Severity::Medium, "d", "f.sql", 1, (0, 1)),
        ]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert_eq!(out.kept.findings.len(), 1);
        assert_eq!(out.kept.findings[0].rule_id, "SEC002");
        assert_eq!(out.suppressed.len(), 3);
    }

    #[test]
    fn suppressed_list_is_deterministically_sorted() {
        let src = "-- plsql-scan:ignore *\n";
        let r = report(vec![
            finding("SEC006", Severity::High, "a", "f.sql", 1, (0, 1)),
            finding("SEC001", Severity::High, "b", "f.sql", 1, (0, 1)),
        ]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        let ids: Vec<&str> = out
            .suppressed
            .iter()
            .map(|s| s.finding.rule_id.as_str())
            .collect();
        assert_eq!(ids, vec!["SEC001", "SEC006"]);
    }

    #[test]
    fn inline_ignore_suppresses_span_less_line0_finding() {
        // Regression for oracle-qm3q.16: fact-driven rules (SEC006,
        // SEC001, …) emit findings at `location.line == 0` because
        // catalog/DDL facts carry no precise span. Line-keyed inline
        // matching could never reach them, so an inline
        // `plsql-scan:ignore` directive was silently inert for every
        // real finding. A line-0 finding must now fall back to
        // file-scoped matching.
        let src = "-- file header\nGRANT SELECT ON hr.t TO PUBLIC; -- plsql-scan:ignore SEC006\n";
        let r = report(vec![finding(
            "SEC006",
            Severity::High,
            "`GRANT SELECT ON hr.t TO PUBLIC` exposes hr.t to every database account",
            "grants.sql",
            0, // span-less: real fact-driven findings point at the unit
            (0, 0),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("grants.sql", src)]),
        );
        assert!(
            out.kept.findings.is_empty(),
            "line-0 finding must be suppressed by a file-scoped inline directive"
        );
        assert_eq!(out.suppressed.len(), 1);
        assert_eq!(
            out.suppressed[0].reason,
            SuppressionReason::InlineSameLine { comment_line: 2 },
            "audit trail records the directive's own comment line"
        );
    }

    #[test]
    fn inline_ignore_line0_does_not_suppress_unnamed_rule() {
        // File-scoped fallback must still respect the rule set: a
        // directive naming a different rule leaves the line-0 finding
        // intact (fail-closed — never over-suppress).
        let src = "x; -- plsql-scan:ignore SEC001\n";
        let r = report(vec![finding(
            "SEC006",
            Severity::High,
            "grant to public",
            "f.sql",
            0,
            (0, 0),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert_eq!(
            out.kept.findings.len(),
            1,
            "directive named SEC001, not SEC006"
        );
        assert!(out.suppressed.is_empty());
    }

    #[test]
    fn inline_ignore_line0_wildcard_suppresses_any_rule() {
        // A `*` directive anywhere in the file suppresses a span-less
        // finding regardless of rule id.
        let src = "-- plsql-scan:ignore *\n";
        let r = report(vec![finding(
            "SEC006",
            Severity::High,
            "grant to public",
            "f.sql",
            0,
            (0, 0),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        assert!(out.kept.findings.is_empty());
        assert_eq!(out.suppressed.len(), 1);
    }

    #[test]
    fn outcome_round_trips_through_json() {
        let src = "q -- plsql-scan:ignore SEC001\n";
        let r = report(vec![finding(
            "SEC001",
            Severity::Critical,
            "m",
            "f.sql",
            1,
            (0, 1),
        )]);
        let out = apply_suppressions(
            &r,
            &SuppressionConfig::default(),
            &srcmap(&[("f.sql", src)]),
        );
        let json = serde_json::to_string(&out).unwrap();
        let back: SuppressionOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back, out);
    }
}
