//! PL/Scope reference diff.
//!
//! Oracle PL/Scope (`ALL_IDENTIFIERS` / `ALL_STATEMENTS`, gated by
//! `plscope_settings = 'IDENTIFIERS:ALL'`) is the database's own
//! name-resolution ground truth: for every identifier *use* site
//! it records the resolved target. This module aligns our
//! resolver's output against that ground truth so we can quantify
//! agreement, surface recall gaps (sites PL/Scope saw that we did
//! not), and surface precision gaps (sites we resolved to a
//! different target than the compiler did).
//!
//! ## Layer independence
//!
//! `plsql-symbols` deliberately does not depend on `plsql-catalog`
//! (Layer-2 crates stay independent — same rule that drives the
//! [`CatalogResolutionSource`](crate::CatalogResolutionSource)
//! shim). PL/Scope rows live in `plsql-catalog::PlScopeSnapshot`;
//! the catalog layer maps each `CompilerReference` into the local
//! [`PlScopeReference`] mirror below and feeds it here. This file
//! never names a catalog type.
//!
//! ## R13 — no silent uncertainty
//!
//! PL/Scope frequently records a use site with an *empty* target
//! (declarations, unresolved forward refs, intra-unit locals it
//! does not chase). We never collapse "unknown target" into a
//! match or a mismatch: such pairs land in dedicated
//! [`PlScopeDiff::our_unknown_target`] /
//! [`PlScopeDiff::plscope_unknown_target`] buckets with the site
//! preserved, so a consumer can see exactly where the uncertainty
//! is rather than having it disappear into the agreement rate.

use crate::ResolutionStrategy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One reference-use site as recorded by Oracle PL/Scope — a
/// local mirror of `plsql-catalog::CompilerReference`, kept here
/// so this crate stays catalog-independent. Identifier fields are
/// raw Oracle names; comparison case-folds them (PL/Scope stores
/// unquoted identifiers upper-cased).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlScopeReference {
    /// Schema that owns the unit containing the use site.
    pub owner: String,
    /// Object (package / procedure / trigger / …) the use is in.
    pub object_name: String,
    /// 1-based source line of the use site.
    pub usage_line: u32,
    /// 1-based source column of the use site.
    pub usage_column: u32,
    /// Resolved target schema, if PL/Scope recorded one.
    pub target_owner: Option<String>,
    /// Resolved target object, if PL/Scope recorded one.
    pub target_object: Option<String>,
    /// Resolved target identifier, if PL/Scope recorded one.
    pub target_identifier: Option<String>,
}

/// One reference-use site as produced by our own resolver,
/// carried with its source location so it can be aligned against
/// PL/Scope's ground truth.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OurReference {
    /// Schema that owns the unit containing the use site.
    pub owner: String,
    /// Object the use is in.
    pub object_name: String,
    /// 1-based source line of the use site.
    pub usage_line: u32,
    /// 1-based source column of the use site.
    pub usage_column: u32,
    /// Resolved target schema, if we resolved one.
    pub target_owner: Option<String>,
    /// Resolved target object, if we resolved one.
    pub target_object: Option<String>,
    /// Resolved target identifier, if we resolved one.
    pub target_identifier: Option<String>,
    /// Strategy that produced the resolution (evidence trail);
    /// `None` if we recorded the site but left it unresolved.
    pub strategy: Option<ResolutionStrategy>,
}

/// Stable identity of a use site: `(owner, object, line, column)`.
/// Owner/object are case-folded; line/column are exact.
type SiteKey = (String, String, u32, u32);

fn site_key_plscope(r: &PlScopeReference) -> SiteKey {
    (
        r.owner.to_ascii_uppercase(),
        r.object_name.to_ascii_uppercase(),
        r.usage_line,
        r.usage_column,
    )
}

fn site_key_ours(r: &OurReference) -> SiteKey {
    (
        r.owner.to_ascii_uppercase(),
        r.object_name.to_ascii_uppercase(),
        r.usage_line,
        r.usage_column,
    )
}

fn norm(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.to_ascii_uppercase())
}

/// `true` only when *both* sides carry a fully-populated target
/// triple. A `None` in any slot means "unknown", never "wildcard".
fn both_targets_present(o: &OurReference, p: &PlScopeReference) -> bool {
    o.target_owner.is_some()
        && o.target_object.is_some()
        && o.target_identifier.is_some()
        && p.target_owner.is_some()
        && p.target_object.is_some()
        && p.target_identifier.is_some()
}

fn targets_equal(o: &OurReference, p: &PlScopeReference) -> bool {
    norm(&o.target_owner) == norm(&p.target_owner)
        && norm(&o.target_object) == norm(&p.target_object)
        && norm(&o.target_identifier) == norm(&p.target_identifier)
}

/// A use site both sides recorded and agreed on (same target).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgreedReference {
    pub ours: OurReference,
    pub plscope: PlScopeReference,
}

/// A use site both sides recorded but resolved to different
/// targets — a precision signal for our resolver.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MismatchedReference {
    pub ours: OurReference,
    pub plscope: PlScopeReference,
}

/// Structured outcome of aligning our references against PL/Scope.
///
/// Every input site appears in exactly one bucket; buckets are
/// sorted by [`SiteKey`] so the report is deterministic
/// machine-output (R10/R11 stable-ordering rule).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlScopeDiff {
    /// Same site, both targets present, targets equal.
    pub agreed: Vec<AgreedReference>,
    /// Same site, both targets present, targets differ.
    pub mismatched: Vec<MismatchedReference>,
    /// Same site; PL/Scope has a target, we left it unresolved
    /// (a recall gap in our resolver — kept explicit per R13).
    pub our_unknown_target: Vec<AgreedReference>,
    /// Same site; we resolved a target, PL/Scope recorded none
    /// (e.g. PL/Scope declaration/forward-ref rows — per R13).
    pub plscope_unknown_target: Vec<AgreedReference>,
    /// Use site we produced that PL/Scope never recorded.
    pub our_only: Vec<OurReference>,
    /// Use site PL/Scope recorded that we never produced.
    pub plscope_only: Vec<PlScopeReference>,
}

/// Aggregate counts derived from a [`PlScopeDiff`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlScopeDiffSummary {
    /// Distinct use sites seen on either side.
    pub total_sites: usize,
    pub agreed: usize,
    pub mismatched: usize,
    pub our_unknown_target: usize,
    pub plscope_unknown_target: usize,
    pub our_only: usize,
    pub plscope_only: usize,
    /// `agreed / (agreed + mismatched)` — agreement restricted to
    /// the sites where *both* sides committed to a target, so
    /// unknown-target rows neither inflate nor deflate it. `None`
    /// when that denominator is zero (no comparable sites).
    pub target_agreement_rate: Option<f64>,
}

impl PlScopeDiff {
    /// Derive aggregate counts. The agreement rate intentionally
    /// excludes unknown-target buckets so it measures resolver
    /// *correctness where it committed*, not coverage.
    #[must_use]
    pub fn summary(&self) -> PlScopeDiffSummary {
        let agreed = self.agreed.len();
        let mismatched = self.mismatched.len();
        let comparable = agreed + mismatched;
        let total_sites = agreed
            + mismatched
            + self.our_unknown_target.len()
            + self.plscope_unknown_target.len()
            + self.our_only.len()
            + self.plscope_only.len();
        PlScopeDiffSummary {
            total_sites,
            agreed,
            mismatched,
            our_unknown_target: self.our_unknown_target.len(),
            plscope_unknown_target: self.plscope_unknown_target.len(),
            our_only: self.our_only.len(),
            plscope_only: self.plscope_only.len(),
            target_agreement_rate: if comparable == 0 {
                None
            } else {
                Some(agreed as f64 / comparable as f64)
            },
        }
    }
}

/// Align our resolved references against PL/Scope ground truth.
///
/// Sites are matched on `(owner, object, line, column)`. Repeat
/// rows for one site (PL/Scope emits these for chained usages)
/// are handled by their *information content*, never silently
/// (R13):
///
/// * **Identical repeat** (same site key AND same field values) —
///   carries no new information, so it is idempotently collapsed:
///   counted once, the first occurrence wins. This is dedup, not
///   a drop — re-adding the same fact cannot change any verdict.
/// * **Conflicting repeat** (same site key, *different* fields) —
///   genuinely new, contradictory information; the first row wins
///   the alignment and every later conflicting row falls through
///   to the corresponding `*_only` bucket so the conflict stays
///   visible.
#[must_use]
pub fn diff_plscope(ours: &[OurReference], theirs: &[PlScopeReference]) -> PlScopeDiff {
    let mut ours_by_site: BTreeMap<SiteKey, &OurReference> = BTreeMap::new();
    let mut our_dups: Vec<&OurReference> = Vec::new();
    for r in ours {
        if let Some(slot) = ours_by_site.get(&site_key_ours(r)) {
            if *slot != r {
                our_dups.push(r);
            }
        } else {
            ours_by_site.insert(site_key_ours(r), r);
        }
    }

    let mut theirs_by_site: BTreeMap<SiteKey, &PlScopeReference> = BTreeMap::new();
    let mut their_dups: Vec<&PlScopeReference> = Vec::new();
    for r in theirs {
        if let Some(slot) = theirs_by_site.get(&site_key_plscope(r)) {
            if *slot != r {
                their_dups.push(r);
            }
        } else {
            theirs_by_site.insert(site_key_plscope(r), r);
        }
    }

    let mut diff = PlScopeDiff::default();

    for (key, o) in &ours_by_site {
        match theirs_by_site.get(key) {
            None => diff.our_only.push((*o).clone()),
            Some(p) => {
                let pair = || AgreedReference {
                    ours: (*o).clone(),
                    plscope: (*p).clone(),
                };
                let our_has = o.target_owner.is_some()
                    || o.target_object.is_some()
                    || o.target_identifier.is_some();
                let their_has = p.target_owner.is_some()
                    || p.target_object.is_some()
                    || p.target_identifier.is_some();
                if both_targets_present(o, p) {
                    if targets_equal(o, p) {
                        diff.agreed.push(pair());
                    } else {
                        diff.mismatched.push(MismatchedReference {
                            ours: (*o).clone(),
                            plscope: (*p).clone(),
                        });
                    }
                } else if their_has && !our_has {
                    diff.our_unknown_target.push(pair());
                } else if our_has && !their_has {
                    diff.plscope_unknown_target.push(pair());
                } else if their_has && our_has {
                    // Both partial — compare what is present; a
                    // disagreement on any populated slot is a real
                    // mismatch, otherwise treat as agreement on
                    // the committed slots.
                    if targets_equal(o, p) {
                        diff.agreed.push(pair());
                    } else {
                        diff.mismatched.push(MismatchedReference {
                            ours: (*o).clone(),
                            plscope: (*p).clone(),
                        });
                    }
                } else {
                    // Neither side committed a target at all —
                    // matched site, no target evidence on either
                    // side. Count as agreement on the (empty)
                    // target rather than inventing uncertainty.
                    diff.agreed.push(pair());
                }
            }
        }
    }

    for (key, p) in &theirs_by_site {
        if !ours_by_site.contains_key(key) {
            diff.plscope_only.push((*p).clone());
        }
    }

    for d in our_dups {
        diff.our_only.push(d.clone());
    }
    for d in their_dups {
        diff.plscope_only.push(d.clone());
    }

    diff.our_only.sort_by_key(site_key_ours);
    diff.plscope_only.sort_by_key(site_key_plscope);

    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours(obj: &str, line: u32, col: u32, t: Option<(&str, &str, &str)>) -> OurReference {
        OurReference {
            owner: "HR".into(),
            object_name: obj.into(),
            usage_line: line,
            usage_column: col,
            target_owner: t.map(|x| x.0.into()),
            target_object: t.map(|x| x.1.into()),
            target_identifier: t.map(|x| x.2.into()),
            strategy: t.map(|_| ResolutionStrategy::SameSchema),
        }
    }

    fn theirs(obj: &str, line: u32, col: u32, t: Option<(&str, &str, &str)>) -> PlScopeReference {
        PlScopeReference {
            owner: "HR".into(),
            object_name: obj.into(),
            usage_line: line,
            usage_column: col,
            target_owner: t.map(|x| x.0.into()),
            target_object: t.map(|x| x.1.into()),
            target_identifier: t.map(|x| x.2.into()),
        }
    }

    #[test]
    fn identical_targets_agree() {
        let o = vec![ours("PKG", 10, 5, Some(("HR", "EMP", "SALARY")))];
        let p = vec![theirs("PKG", 10, 5, Some(("HR", "EMP", "SALARY")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.agreed.len(), 1);
        assert_eq!(d.summary().target_agreement_rate, Some(1.0));
    }

    #[test]
    fn target_disagreement_is_mismatch_not_silent() {
        let o = vec![ours("PKG", 10, 5, Some(("HR", "EMP", "SALARY")))];
        let p = vec![theirs("PKG", 10, 5, Some(("HR", "EMP", "WAGE")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.mismatched.len(), 1);
        assert_eq!(d.agreed.len(), 0);
        assert_eq!(d.summary().target_agreement_rate, Some(0.0));
    }

    #[test]
    fn case_insensitive_identifier_match() {
        let o = vec![ours("pkg", 1, 1, Some(("hr", "emp", "salary")))];
        let p = vec![theirs("PKG", 1, 1, Some(("HR", "EMP", "SALARY")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.agreed.len(), 1, "owner/object/target case must fold");
    }

    #[test]
    fn plscope_unknown_target_is_isolated_r13() {
        // We resolved it; PL/Scope recorded the site with no
        // target (declaration-style row). Must NOT count as a
        // mismatch and must NOT vanish.
        let o = vec![ours("PKG", 2, 3, Some(("HR", "EMP", "ID")))];
        let p = vec![theirs("PKG", 2, 3, None)];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.plscope_unknown_target.len(), 1);
        assert_eq!(d.mismatched.len(), 0);
        assert_eq!(d.agreed.len(), 0);
        // Excluded from the agreement denominator.
        assert_eq!(d.summary().target_agreement_rate, None);
    }

    #[test]
    fn our_unknown_target_is_isolated_r13() {
        let o = vec![ours("PKG", 2, 3, None)];
        let p = vec![theirs("PKG", 2, 3, Some(("HR", "EMP", "ID")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.our_unknown_target.len(), 1);
        assert_eq!(d.summary().total_sites, 1);
    }

    #[test]
    fn our_only_and_plscope_only_split() {
        let o = vec![ours("PKG", 1, 1, Some(("HR", "A", "X")))];
        let p = vec![theirs("PKG", 9, 9, Some(("HR", "B", "Y")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.our_only.len(), 1);
        assert_eq!(d.plscope_only.len(), 1);
        assert_eq!(d.summary().total_sites, 2);
    }

    #[test]
    fn duplicate_rows_fall_through_not_dropped() {
        let o = vec![
            ours("PKG", 1, 1, Some(("HR", "A", "X"))),
            ours("PKG", 1, 1, Some(("HR", "A", "Z"))),
        ];
        let p = vec![theirs("PKG", 1, 1, Some(("HR", "A", "X")))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.agreed.len(), 1);
        assert_eq!(d.our_only.len(), 1, "second dup must surface, not vanish");
    }

    #[test]
    fn identical_repeat_is_idempotently_collapsed_not_a_spurious_finding() {
        // An exact-duplicate row carries no new information.
        // Collapsing it (counting the site once) is correct dedup,
        // NOT a silent drop: it must not manufacture a spurious
        // our_only / mismatch, and the site count stays 1.
        let dup = ("HR", "A", "X");
        let o = vec![
            ours("PKG", 1, 1, Some(dup)),
            ours("PKG", 1, 1, Some(dup)), // byte-identical repeat
        ];
        let p = vec![theirs("PKG", 1, 1, Some(dup))];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.agreed.len(), 1);
        assert!(
            d.our_only.is_empty(),
            "identical repeat must not become our_only"
        );
        assert!(d.mismatched.is_empty());
        let s = d.summary();
        assert_eq!(s.total_sites, 1, "the same logical site is counted once");
        assert_eq!(s.agreed, 1);
    }

    #[test]
    fn identical_repeat_on_plscope_side_also_collapses() {
        let dup = ("HR", "A", "X");
        let o = vec![ours("PKG", 1, 1, Some(dup))];
        let p = vec![
            theirs("PKG", 1, 1, Some(dup)),
            theirs("PKG", 1, 1, Some(dup)), // identical repeat
        ];
        let d = diff_plscope(&o, &p);
        assert_eq!(d.agreed.len(), 1);
        assert!(
            d.plscope_only.is_empty(),
            "identical repeat must not become plscope_only"
        );
        assert_eq!(d.summary().total_sites, 1);
    }

    #[test]
    fn output_is_deterministically_sorted() {
        let o = vec![
            ours("ZPKG", 5, 1, Some(("HR", "A", "X"))),
            ours("APKG", 2, 1, Some(("HR", "B", "Y"))),
        ];
        let d = diff_plscope(&o, &[]);
        let keys: Vec<_> = d.our_only.iter().map(|r| r.object_name.clone()).collect();
        assert_eq!(keys, vec!["APKG", "ZPKG"]);
    }

    #[test]
    fn empty_inputs_yield_empty_diff() {
        let d = diff_plscope(&[], &[]);
        assert_eq!(d.summary().total_sites, 0);
        assert_eq!(d.summary().target_agreement_rate, None);
    }

    #[test]
    fn mixed_corpus_summary_counts() {
        let o = vec![
            ours("P", 1, 1, Some(("HR", "T", "C"))), // agree
            ours("P", 2, 1, Some(("HR", "T", "C"))), // mismatch
            ours("P", 3, 1, None),                   // our unknown
            ours("P", 4, 1, Some(("HR", "T", "C"))), // our_only
        ];
        let p = vec![
            theirs("P", 1, 1, Some(("HR", "T", "C"))),
            theirs("P", 2, 1, Some(("HR", "T", "D"))),
            theirs("P", 3, 1, Some(("HR", "T", "C"))),
            theirs("P", 9, 1, Some(("HR", "T", "C"))), // plscope_only
        ];
        let s = diff_plscope(&o, &p).summary();
        assert_eq!(s.agreed, 1);
        assert_eq!(s.mismatched, 1);
        assert_eq!(s.our_unknown_target, 1);
        assert_eq!(s.our_only, 1);
        assert_eq!(s.plscope_only, 1);
        assert_eq!(s.total_sites, 5);
        assert_eq!(s.target_agreement_rate, Some(0.5));
    }
}
