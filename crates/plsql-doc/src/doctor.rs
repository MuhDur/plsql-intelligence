//! Doctor surface for [`DocSet`].
//!
//! Aggregates per-object documentation coverage so a developer can ask:
//! "is my schema's doc-comment surface complete?" The shape follows the
//! project-wide doctor convention (one `*DoctorReport`, stable JSON,
//! `posture` triad, `remediation_hints`).
//!
//! Coverage classes (per object):
//! - **Documented**: at least one untagged comment OR a `summary`.
//! - **TaggedOnly**: tagged comments (`@param` etc.) but no free-form
//!   description and no `summary`.
//! - **Undocumented**: no comments at all.

use serde::{Deserialize, Serialize};

use crate::{DocComment, DocSet, ObjectDoc};

/// Aggregated documentation-coverage report for a [`DocSet`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocCoverageReport {
    /// Total documented + tagged-only + undocumented objects.
    pub objects_total: usize,
    /// Objects with at least one untagged comment or a non-empty summary.
    pub documented: usize,
    /// Objects whose only commentary is tagged (e.g. only `@param`).
    pub tagged_only: usize,
    /// Objects with zero comments AND no summary.
    pub undocumented: usize,
    /// Documented ÷ objects_total as a percentage (0..=100).
    pub documented_percent: u32,
    /// Per-kind coverage breakdown, sorted by kind for stable output.
    pub by_kind: Vec<KindCoverageRow>,
    /// `object_id`s of undocumented objects (sorted; truncated to
    /// `UNDOCUMENTED_LIST_LIMIT` so very large schemas don't blow up
    /// the report payload).
    pub undocumented_sample: Vec<String>,
    /// Overall posture — `Clean` (≥95%), `Caution` (≥50%), `Unknown`
    /// otherwise. Operator hint, not a hard threshold.
    pub posture: DocPosture,
    /// One-line operator hints derived from the counts.
    pub remediation_hints: Vec<String>,
}

/// Per-kind row in the coverage breakdown.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct KindCoverageRow {
    pub kind: String,
    pub total: usize,
    pub documented: usize,
    pub tagged_only: usize,
    pub undocumented: usize,
}

/// Three-state posture for the documentation surface. Default is
/// `Caution` so a fresh `DocCoverageReport::default()` doesn't accidentally
/// claim documentation is clean.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DocPosture {
    /// `documented_percent >= 95`.
    Clean,
    /// `documented_percent >= 50` and < 95.
    #[default]
    Caution,
    /// `documented_percent < 50`, or no objects at all.
    Unknown,
}

/// Hard cap on the size of `undocumented_sample` so a wide schema
/// doesn't bloat the report payload. The full list is reproducible
/// from the [`DocSet`] itself.
pub const UNDOCUMENTED_LIST_LIMIT: usize = 100;

/// Build a [`DocCoverageReport`] from a [`DocSet`].
#[must_use]
pub fn doctor_report(set: &DocSet) -> DocCoverageReport {
    let mut documented = 0usize;
    let mut tagged_only = 0usize;
    let mut undocumented = 0usize;
    let mut by_kind: std::collections::BTreeMap<String, KindCoverageRow> =
        std::collections::BTreeMap::new();
    let mut undocumented_ids: Vec<String> = Vec::new();

    for obj in &set.objects {
        let class = classify(obj);
        let row = by_kind
            .entry(obj.kind.to_lowercase())
            .or_insert_with(|| KindCoverageRow {
                kind: obj.kind.to_lowercase(),
                ..KindCoverageRow::default()
            });
        row.total += 1;
        match class {
            CoverageClass::Documented => {
                documented += 1;
                row.documented += 1;
            }
            CoverageClass::TaggedOnly => {
                tagged_only += 1;
                row.tagged_only += 1;
            }
            CoverageClass::Undocumented => {
                undocumented += 1;
                row.undocumented += 1;
                undocumented_ids.push(obj.object_id.clone());
            }
        }
    }

    let objects_total = set.objects.len();
    let documented_percent: u32 = (documented * 100)
        .checked_div(objects_total)
        .map(|p| u32::try_from(p).unwrap_or(u32::MAX))
        .unwrap_or(0);

    let posture = classify_posture(objects_total, documented_percent);
    undocumented_ids.sort();
    let undocumented_sample = undocumented_ids
        .into_iter()
        .take(UNDOCUMENTED_LIST_LIMIT)
        .collect::<Vec<_>>();

    let remediation_hints =
        build_remediation_hints(objects_total, documented_percent, undocumented, tagged_only);

    DocCoverageReport {
        objects_total,
        documented,
        tagged_only,
        undocumented,
        documented_percent,
        by_kind: by_kind.into_values().collect(),
        undocumented_sample,
        posture,
        remediation_hints,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoverageClass {
    Documented,
    TaggedOnly,
    Undocumented,
}

fn classify(obj: &ObjectDoc) -> CoverageClass {
    let has_summary = obj
        .summary
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let has_untagged = obj.comments.iter().any(is_untagged);
    let has_any_tagged = obj.comments.iter().any(|c| c.tag.is_some());

    if has_summary || has_untagged {
        CoverageClass::Documented
    } else if has_any_tagged {
        CoverageClass::TaggedOnly
    } else {
        CoverageClass::Undocumented
    }
}

fn is_untagged(comment: &DocComment) -> bool {
    comment.tag.is_none() && !comment.text.trim().is_empty()
}

fn classify_posture(objects_total: usize, documented_percent: u32) -> DocPosture {
    if objects_total == 0 {
        return DocPosture::Unknown;
    }
    if documented_percent >= 95 {
        DocPosture::Clean
    } else if documented_percent >= 50 {
        DocPosture::Caution
    } else {
        DocPosture::Unknown
    }
}

fn build_remediation_hints(
    objects_total: usize,
    documented_percent: u32,
    undocumented: usize,
    tagged_only: usize,
) -> Vec<String> {
    let mut hints = Vec::new();
    if objects_total == 0 {
        hints.push(String::from(
            "DocSet is empty — confirm the doc-comment extractor ran over the source corpus.",
        ));
        return hints;
    }
    if undocumented > 0 {
        hints.push(format!(
            "{undocumented} object(s) have no doc-comments — see `undocumented_sample` for object_ids."
        ));
    }
    if tagged_only > 0 {
        hints.push(format!(
            "{tagged_only} object(s) have tagged comments but no free-form description — \
             add a one-line summary or untagged header to lift the doc-coverage class."
        ));
    }
    if documented_percent < 50 {
        hints.push(String::from(
            "Less than half the schema is documented — prioritise public-facing packages first.",
        ));
    } else if documented_percent < 95 {
        hints.push(format!(
            "Documentation coverage is {documented_percent}% — push past 95% to clear the gate."
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocComment, DocSet, ObjectDoc};

    fn doc_with(
        id: &str,
        kind: &str,
        summary: Option<&str>,
        untagged: bool,
        tagged: bool,
    ) -> ObjectDoc {
        let mut comments = Vec::new();
        if untagged {
            comments.push(DocComment {
                tag: None,
                text: "free-form body".into(),
                source_span: None,
            });
        }
        if tagged {
            comments.push(DocComment {
                tag: Some("param".into()),
                text: "p_x value".into(),
                source_span: None,
            });
        }
        ObjectDoc {
            object_id: id.into(),
            name: id.to_uppercase(),
            kind: kind.into(),
            summary: summary.map(str::to_string),
            comments,
            source_span: None,
        }
    }

    #[test]
    fn empty_set_yields_unknown_posture_with_empty_hint() {
        let report = doctor_report(&DocSet::default());
        assert_eq!(report.objects_total, 0);
        assert_eq!(report.documented_percent, 0);
        assert_eq!(report.posture, DocPosture::Unknown);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("DocSet is empty"))
        );
    }

    #[test]
    fn fully_documented_set_yields_clean_posture() {
        let set = DocSet {
            objects: (0..20)
                .map(|i| doc_with(&format!("x.y{i}"), "package", None, true, false))
                .collect(),
        };
        let report = doctor_report(&set);
        assert_eq!(report.objects_total, 20);
        assert_eq!(report.documented, 20);
        assert_eq!(report.undocumented, 0);
        assert_eq!(report.documented_percent, 100);
        assert_eq!(report.posture, DocPosture::Clean);
        assert!(report.remediation_hints.is_empty());
    }

    #[test]
    fn tagged_only_objects_are_counted_separately() {
        let set = DocSet {
            objects: vec![
                doc_with("x.a", "package", None, false, true),
                doc_with("x.b", "package", None, false, true),
                doc_with("x.c", "package", None, true, false),
            ],
        };
        let report = doctor_report(&set);
        assert_eq!(report.tagged_only, 2);
        assert_eq!(report.documented, 1);
        assert_eq!(report.undocumented, 0);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("free-form description"))
        );
    }

    #[test]
    fn undocumented_sample_is_sorted_and_capped() {
        let set = DocSet {
            objects: (0..150)
                .map(|i| doc_with(&format!("z{i:04}.x"), "table", None, false, false))
                .collect(),
        };
        let report = doctor_report(&set);
        assert_eq!(report.objects_total, 150);
        assert_eq!(report.undocumented, 150);
        assert_eq!(report.documented_percent, 0);
        assert_eq!(report.posture, DocPosture::Unknown);
        // Cap at 100 + sorted lexicographically.
        assert_eq!(report.undocumented_sample.len(), UNDOCUMENTED_LIST_LIMIT);
        assert_eq!(report.undocumented_sample[0], "z0000.x");
        let prev_pairs: Vec<_> = report
            .undocumented_sample
            .windows(2)
            .map(|w| w[0].clone().cmp(&w[1].clone()))
            .collect();
        assert!(prev_pairs.iter().all(|c| *c != std::cmp::Ordering::Greater));
    }

    #[test]
    fn by_kind_breakdown_aggregates_counts_per_kind() {
        let set = DocSet {
            objects: vec![
                doc_with("x.a", "package", Some("s"), false, false),
                doc_with("x.b", "package", None, false, false),
                doc_with("x.c", "table", None, true, false),
                doc_with("x.d", "view", None, false, false),
            ],
        };
        let report = doctor_report(&set);
        // Three kinds, sorted by kind.
        let kinds: Vec<&str> = report.by_kind.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(kinds, vec!["package", "table", "view"]);
        let pkg = report.by_kind.iter().find(|r| r.kind == "package").unwrap();
        assert_eq!(pkg.total, 2);
        assert_eq!(pkg.documented, 1);
        assert_eq!(pkg.undocumented, 1);
    }

    #[test]
    fn summary_alone_promotes_to_documented() {
        let set = DocSet {
            objects: vec![doc_with("x.a", "package", Some("one-liner"), false, false)],
        };
        let report = doctor_report(&set);
        assert_eq!(report.documented, 1);
        assert_eq!(report.posture, DocPosture::Clean);
    }

    #[test]
    fn caution_posture_at_partial_coverage() {
        let set = DocSet {
            objects: (0..10)
                .map(|i| {
                    let documented = i < 7;
                    doc_with(
                        &format!("x.{i}"),
                        "package",
                        None,
                        documented,
                        !documented && (i % 2 == 0),
                    )
                })
                .collect(),
        };
        let report = doctor_report(&set);
        assert_eq!(report.documented_percent, 70);
        assert_eq!(report.posture, DocPosture::Caution);
    }
}
