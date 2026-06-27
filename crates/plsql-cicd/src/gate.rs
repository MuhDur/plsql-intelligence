//! `gate <changeset>`.
//!
//! Applies a `.plsql-cicd-policy.toml` policy file to an
//! [`InvalidationPrediction`] and decides whether the deployment
//! is allowed to proceed.
//!
//! The gate is the second half of the
//! `predict` → `gate` → `verify` → `apply` pipeline described in
//! plan.md §16. `predict` collects the facts; `gate` enforces the
//! organisation-level rules (max invalidations, blocked object
//! kinds, required confidence floor). Each rejection records the
//! exact policy clause that fired so CI can surface a one-line
//! reason without re-running the prediction.
//!
//! ## Policy schema
//!
//! ```toml
//! # Maximum number of predicted invalidations the change is
//! # allowed to cause. Default: no cap.
//! max_invalidations = 100
//!
//! # Object kinds the change cannot touch. Listed as Oracle dictionary
//! # strings ("PACKAGE BODY", "TRIGGER", etc.).
//! blocked_kinds = ["TRIGGER"]
//!
//! # Minimum confidence the prediction must hold to be accepted.
//! # One of: low | medium | high.
//! min_confidence = "medium"
//!
//! # Refuse to gate if any uncertainty record carries a reason in
//! # this list (e.g. you want to block on opaque dynamic SQL). Each
//! # entry must be a canonical `UnknownReason` variant name as emitted
//! # by the analyzer (`DynamicSqlOpaque`, `DbLinkRemoteObject`, …); an
//! # unrecognised name is rejected at parse time rather than silently
//! # never matching.
//! blocking_unknown_reasons = ["DynamicSqlOpaque", "DbLinkRemoteObject"]
//! ```
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — for the
//!   `ALL_DEPENDENCIES` STATUS column that drives the
//!   invalidation cascade.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families — the
//!   `ALL_OBJECTS.OBJECT_TYPE` strings we compare `blocked_kinds`
//!   against come from this table.

use plsql_core::ConfidenceLevel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::InvalidationPrediction;

const SCHEMA_ID: &str = "plsql.cicd.gate_decision";
const SCHEMA_VERSION: u32 = 1;

/// Gate policy loaded from `.plsql-cicd-policy.toml`. All fields
/// are optional — an empty file means "allow everything".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GatePolicy {
    pub max_invalidations: Option<u32>,
    #[serde(default)]
    pub blocked_kinds: Vec<String>,
    pub min_confidence: Option<MinConfidence>,
    #[serde(default)]
    pub blocking_unknown_reasons: Vec<String>,
}

/// Minimum confidence floor. Maps to `plsql_core::ConfidenceLevel`
/// at evaluation time. Stored as lowercase string in TOML so the
/// policy file stays human-friendly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinConfidence {
    Low,
    Medium,
    High,
}

impl MinConfidence {
    fn as_level(self) -> ConfidenceLevel {
        match self {
            Self::Low => ConfidenceLevel::Low,
            Self::Medium => ConfidenceLevel::Medium,
            Self::High => ConfidenceLevel::High,
        }
    }
}

/// Output of [`run_gate`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateDecision {
    pub schema_id: String,
    pub schema_version: u32,
    pub allowed: bool,
    pub failures: Vec<GateFailure>,
    pub policy_summary: GatePolicySummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum GateFailure {
    /// `max_invalidations` was exceeded.
    InvalidationsExceeded { cap: u32, observed: u32 },
    /// At least one predicted invalidation hit a kind listed in
    /// `blocked_kinds`.
    BlockedKindHit { kind: String, observed_count: u32 },
    /// At least one predicted invalidation came in below the
    /// configured confidence floor.
    ConfidenceBelowFloor {
        floor: MinConfidence,
        observed_count: u32,
    },
    /// An uncertainty record carried a reason that the policy lists
    /// under `blocking_unknown_reasons`.
    BlockingUnknownReasonHit { reason: String, observed_count: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatePolicySummary {
    pub max_invalidations: Option<u32>,
    pub blocked_kinds: Vec<String>,
    pub min_confidence: Option<MinConfidence>,
    pub blocking_unknown_reasons: Vec<String>,
}

#[derive(Debug, Error)]
pub enum GateError {
    #[error("policy file read failure: {0}")]
    Io(String),
    #[error("policy file parse failure: {0}")]
    Parse(String),
}

/// Load a policy from a TOML string. The CLI pairs this with a file
/// reader.
///
/// After deserialization the policy is validated: every entry of
/// `blocking_unknown_reasons` must be a canonical [`UnknownReason`]
/// variant name (the exact strings [`unknown_reason_name`] emits). An
/// unrecognised name — e.g. a typo or a stale `"OpaqueDynamicSql"`
/// copied from old docs — is a [`GateError::Parse`] rather than an
/// entry that silently never matches and lets the gate fail open
/// (oracle-qm3q.7).
pub fn parse_policy(toml_text: &str) -> Result<GatePolicy, GateError> {
    let policy: GatePolicy =
        toml::from_str(toml_text).map_err(|e| GateError::Parse(e.to_string()))?;
    validate_policy(&policy)?;
    Ok(policy)
}

/// Reject policies that name an unknown-reason matcher string the
/// analyzer never emits. Without this guard a mistyped
/// `blocking_unknown_reasons` entry would compare-not-equal to every
/// emitted reason, so the rule never fires and the gate fails open
/// while the operator believes the changeset is blocked.
fn validate_policy(policy: &GatePolicy) -> Result<(), GateError> {
    for reason in &policy.blocking_unknown_reasons {
        if !is_known_reason_name(reason) {
            return Err(GateError::Parse(format!(
                "blocking_unknown_reasons contains unknown reason {reason:?}; \
                 valid reasons are: {valid}",
                valid = ALL_REASON_NAMES.join(", "),
            )));
        }
    }
    Ok(())
}

/// `true` when `name` is a canonical [`UnknownReason`] variant name as
/// emitted by [`unknown_reason_name`]. Matching is exact and
/// case-sensitive — the policy file stores the variant name verbatim.
#[must_use]
fn is_known_reason_name(name: &str) -> bool {
    ALL_REASON_NAMES.contains(&name)
}

/// Every canonical [`UnknownReason`] name a policy may list under
/// `blocking_unknown_reasons`. Kept in lock-step with
/// [`unknown_reason_name`] by the `all_reason_names_match_emitter`
/// test, which fails if a new enum variant is added without a matching
/// entry here.
const ALL_REASON_NAMES: &[&str] = &[
    "DynamicSqlOpaque",
    "DbLinkRemoteObject",
    "WrappedSource",
    "MissingCatalogObject",
    "MissingPackageBody",
    "ConditionalCompilationBranch",
    "EditionedObject",
    "InvokerRightsRuntimeResolution",
    "RuntimeGrantOrRole",
    "UnsupportedDialectFeature",
    "ParserRecoveryRegion",
    "AnalysisRecursionLimit",
    "ResponseSanitized",
];

// `PLSQL-CICD-014` (oracle-vvxw): PR-comment JSON output. The shape is
// a typed envelope with format/schema_id/schema_version (matching the
// rest of the plsql-output `RobotJsonEnvelope` family) plus a small
// `pr_comment` block carrying the human-readable bits a downstream CI
// adapter needs to render a stable Markdown comment.
const PR_COMMENT_SCHEMA_ID: &str = "plsql.cicd.gate_pr_comment";
const PR_COMMENT_SCHEMA_VERSION: u32 = 1;

/// Renderable summary of a [`GateDecision`] designed for a CI/PR
/// adapter. Wraps the decision in a stable JSON envelope and adds a
/// `pr_comment` block with operator-facing fields: a one-line verdict,
/// a Markdown body fragment, and a stable HTML marker so subsequent
/// comments on the same PR can be idempotently updated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrCommentEnvelope {
    pub format: String,
    pub schema_id: String,
    pub schema_version: u32,
    pub pr_comment: PrComment,
    pub decision: GateDecision,
}

/// Human-readable summary fields keyed off the gate decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrComment {
    /// `pass` / `fail` — a one-token verdict for the CI badge.
    pub verdict: String,
    /// A single line summarising the decision. Safe to use as a PR
    /// check-run title (≤ 100 chars).
    pub headline: String,
    /// Stable HTML marker the downstream comment-poster
    /// keys on for idempotent updates: one comment per PR, edited in
    /// place across runs.
    pub html_marker: String,
    /// Markdown body fragment. The post-pr-comment adapter wraps this
    /// with the `html_marker` and posts it.
    pub body_md: String,
}

/// Build a [`PrCommentEnvelope`] from a [`GateDecision`]. The shape is
/// deterministic — same decision → byte-identical JSON output — so CI
/// adapters can diff payloads across runs without spurious noise.
#[must_use]
pub fn render_pr_comment(decision: &GateDecision) -> PrCommentEnvelope {
    let verdict = if decision.allowed { "pass" } else { "fail" };
    let failure_count = decision.failures.len();
    let headline = if decision.allowed {
        String::from("plsql cicd gate: PASS — no policy violations")
    } else {
        format!("plsql cicd gate: FAIL — {failure_count} policy violation(s)")
    };
    let body_md = build_pr_comment_body(decision, &headline);
    PrCommentEnvelope {
        format: String::from("robot-json"),
        schema_id: String::from(PR_COMMENT_SCHEMA_ID),
        schema_version: PR_COMMENT_SCHEMA_VERSION,
        pr_comment: PrComment {
            verdict: String::from(verdict),
            headline,
            // Stable HTML marker the poster matches against
            // (PLSQL-CICD-016 idempotent comment update). Includes
            // the schema version so a marker bump is visible to the
            // matcher when we evolve the comment shape.
            html_marker: format!("<!-- plsql-cicd:gate v{PR_COMMENT_SCHEMA_VERSION} -->"),
            body_md,
        },
        decision: decision.clone(),
    }
}

fn build_pr_comment_body(decision: &GateDecision, headline: &str) -> String {
    let mut body = String::new();
    body.push_str("## ");
    body.push_str(headline);
    body.push_str("\n\n");
    if decision.allowed {
        body.push_str("No policy violations were detected. ");
        body.push_str("This PR is clear of the configured CI/CD gate.\n");
        return body;
    }
    body.push_str("The CI gate refused this changeset for the following reasons:\n\n");
    for (idx, failure) in decision.failures.iter().enumerate() {
        body.push_str(&format!("{n}. ", n = idx + 1));
        match failure {
            GateFailure::InvalidationsExceeded { cap, observed } => {
                body.push_str(&format!(
                    "`max_invalidations` exceeded — observed {observed}, cap {cap}.\n"
                ));
            }
            GateFailure::BlockedKindHit {
                kind,
                observed_count,
            } => {
                body.push_str(&format!(
                    "`blocked_kinds` hit — `{kind}` appeared {observed_count} time(s).\n"
                ));
            }
            GateFailure::ConfidenceBelowFloor {
                floor,
                observed_count,
            } => {
                body.push_str(&format!(
                    "`min_confidence` floor `{floor:?}` undercut — {observed_count} prediction(s).\n"
                ));
            }
            GateFailure::BlockingUnknownReasonHit {
                reason,
                observed_count,
            } => {
                body.push_str(&format!(
                    "`blocking_unknown_reasons` hit — `{reason}` appeared {observed_count} time(s).\n"
                ));
            }
        }
    }
    body.push_str("\n_Policy summary_: ");
    body.push_str(&format!(
        "max_invalidations={:?}, blocked_kinds={:?}, min_confidence={:?}, blocking_unknown_reasons={:?}.\n",
        decision.policy_summary.max_invalidations,
        decision.policy_summary.blocked_kinds,
        decision.policy_summary.min_confidence,
        decision.policy_summary.blocking_unknown_reasons,
    ));
    body
}

/// Apply `policy` to `prediction` and return a `GateDecision`. The
/// decision is `allowed: false` if any rule fires, with one
/// `GateFailure` per rule (we collect them all so the operator sees
/// every reason in one CI run rather than fixing-then-re-running).
///
/// Scope note (oracle-qm3q.7 / .17): `max_invalidations` counts every
/// row in `prediction.predicted_invalidations`. Plain `predict` emits
/// direct (`distance == 1`) invalidations only; `predict_with_lineage`
/// may include transitive (`distance > 1`) impact rows, and those are
/// counted by the same length check.
#[must_use]
pub fn run_gate(prediction: &InvalidationPrediction, policy: &GatePolicy) -> GateDecision {
    let mut failures: Vec<GateFailure> = Vec::new();

    if let Some(cap) = policy.max_invalidations {
        // **Saturating cast (oracle-kxb3).** `.len()` is `usize` and
        // on 64-bit targets a >u32::MAX-item batch would wrap to a
        // small u32 with the legacy `as u32` cast, silently bypassing
        // the cap. Saturate to `u32::MAX` so the gate's safety logic
        // still fires (the policy's `Option<u32>` cap is preserved
        // for backward compatibility; widening it would change the
        // serialized shape downstream callers depend on).
        let observed = u32::try_from(prediction.predicted_invalidations.len()).unwrap_or(u32::MAX);
        if observed > cap {
            failures.push(GateFailure::InvalidationsExceeded { cap, observed });
        }
    }

    for blocked in &policy.blocked_kinds {
        let observed_count = u32::try_from(
            prediction
                .predicted_invalidations
                .iter()
                .filter(|p| p.object_type.eq_ignore_ascii_case(blocked))
                .count(),
        )
        .unwrap_or(u32::MAX);
        if observed_count > 0 {
            failures.push(GateFailure::BlockedKindHit {
                kind: blocked.clone(),
                observed_count,
            });
        }
    }

    if let Some(floor) = policy.min_confidence {
        let floor_level = floor.as_level();
        let observed_count = u32::try_from(
            prediction
                .predicted_invalidations
                .iter()
                .filter(|p| confidence_below_floor(&p.confidence.level, floor_level))
                .count(),
        )
        .unwrap_or(u32::MAX);
        if observed_count > 0 {
            failures.push(GateFailure::ConfidenceBelowFloor {
                floor,
                observed_count,
            });
        }
    }

    for blocked_reason in &policy.blocking_unknown_reasons {
        let observed_count = u32::try_from(
            prediction
                .uncertainties
                .iter()
                .filter(|u| unknown_reason_name(&u.reason) == blocked_reason.as_str())
                .count(),
        )
        .unwrap_or(u32::MAX);
        if observed_count > 0 {
            failures.push(GateFailure::BlockingUnknownReasonHit {
                reason: blocked_reason.clone(),
                observed_count,
            });
        }
    }

    GateDecision {
        schema_id: SCHEMA_ID.into(),
        schema_version: SCHEMA_VERSION,
        allowed: failures.is_empty(),
        failures,
        policy_summary: GatePolicySummary {
            max_invalidations: policy.max_invalidations,
            blocked_kinds: policy.blocked_kinds.clone(),
            min_confidence: policy.min_confidence,
            blocking_unknown_reasons: policy.blocking_unknown_reasons.clone(),
        },
    }
}

fn confidence_below_floor(observed: &ConfidenceLevel, floor: ConfidenceLevel) -> bool {
    confidence_rank(*observed) < confidence_rank(floor)
}

fn confidence_rank(c: ConfidenceLevel) -> u8 {
    match c {
        ConfidenceLevel::Opaque => 0,
        ConfidenceLevel::Low => 1,
        ConfidenceLevel::Medium => 2,
        ConfidenceLevel::High => 3,
    }
}

fn unknown_reason_name(reason: &plsql_core::UnknownReason) -> &'static str {
    use plsql_core::UnknownReason as R;
    match reason {
        R::DynamicSqlOpaque => "DynamicSqlOpaque",
        R::DbLinkRemoteObject => "DbLinkRemoteObject",
        R::WrappedSource => "WrappedSource",
        R::MissingCatalogObject => "MissingCatalogObject",
        R::MissingPackageBody => "MissingPackageBody",
        R::ConditionalCompilationBranch => "ConditionalCompilationBranch",
        R::EditionedObject => "EditionedObject",
        R::InvokerRightsRuntimeResolution => "InvokerRightsRuntimeResolution",
        R::RuntimeGrantOrRole => "RuntimeGrantOrRole",
        R::UnsupportedDialectFeature => "UnsupportedDialectFeature",
        R::ParserRecoveryRegion => "ParserRecoveryRegion",
        R::AnalysisRecursionLimit => "AnalysisRecursionLimit",
        R::ResponseSanitized => "ResponseSanitized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PredictMode, PredictedInvalidation, UncertaintyRecord};
    use plsql_core::{
        Confidence, ConfidenceLevel, ObjectName, SchemaName, SymbolId, UnknownReason,
    };

    fn pi(kind: &str, level: ConfidenceLevel) -> PredictedInvalidation {
        PredictedInvalidation {
            owner: SchemaName::from(SymbolId::new(1)),
            name: ObjectName::from(SymbolId::new(2)),
            object_type: kind.into(),
            reason: crate::InvalidationReason::Other {
                description: "test".into(),
            },
            confidence: Confidence {
                level,
                explanation: Some("test".into()),
            },
            distance: 1,
        }
    }

    fn prediction(rows: Vec<PredictedInvalidation>) -> InvalidationPrediction {
        InvalidationPrediction {
            mode: PredictMode::SourceOnly,
            predicted_invalidations: rows,
            ..Default::default()
        }
    }

    #[test]
    fn empty_policy_allows_everything() {
        let pred = prediction(vec![pi("PACKAGE BODY", ConfidenceLevel::High)]);
        let policy = GatePolicy::default();
        let decision = run_gate(&pred, &policy);
        assert!(decision.allowed);
        assert!(decision.failures.is_empty());
    }

    #[test]
    fn max_invalidations_caps_the_change() {
        let pred = prediction(vec![
            pi("PACKAGE BODY", ConfidenceLevel::High),
            pi("VIEW", ConfidenceLevel::High),
            pi("FUNCTION", ConfidenceLevel::High),
        ]);
        let policy = GatePolicy {
            max_invalidations: Some(2),
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
        assert!(matches!(
            &d.failures[0],
            GateFailure::InvalidationsExceeded {
                cap: 2,
                observed: 3
            }
        ));
    }

    #[test]
    fn max_invalidations_cap_is_inclusive_boundary() {
        // Commercial release-gate contract: `max_invalidations` is
        // the inclusive maximum — observed == cap must PASS, only
        // observed > cap blocks. Locks the `> cap` comparison so a
        // refactor to `>= cap` (false-block at the limit) or a
        // looser comparison (false-allow over the limit) is caught.
        let at_cap = prediction(vec![
            pi("PACKAGE BODY", ConfidenceLevel::High),
            pi("VIEW", ConfidenceLevel::High),
        ]);
        let policy = GatePolicy {
            max_invalidations: Some(2),
            ..GatePolicy::default()
        };
        let d = run_gate(&at_cap, &policy);
        assert!(
            d.allowed,
            "observed == cap (2 == 2) must be allowed; failures: {:#?}",
            d.failures
        );
        assert!(d.failures.is_empty());

        // One over the cap blocks (the other side of the boundary).
        let over = prediction(vec![
            pi("PACKAGE BODY", ConfidenceLevel::High),
            pi("VIEW", ConfidenceLevel::High),
            pi("FUNCTION", ConfidenceLevel::High),
        ]);
        assert!(!run_gate(&over, &policy).allowed);

        // A zero cap blocks any invalidation at all.
        let zero = GatePolicy {
            max_invalidations: Some(0),
            ..GatePolicy::default()
        };
        assert!(!run_gate(&at_cap, &zero).allowed);
    }

    #[test]
    fn blocked_kind_fires_with_count() {
        let pred = prediction(vec![
            pi("TRIGGER", ConfidenceLevel::High),
            pi("TRIGGER", ConfidenceLevel::High),
            pi("VIEW", ConfidenceLevel::High),
        ]);
        let policy = GatePolicy {
            blocked_kinds: vec!["TRIGGER".into()],
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
        assert!(
            matches!(
                &d.failures[0],
                GateFailure::BlockedKindHit { kind, observed_count }
                    if kind == "TRIGGER" && *observed_count == 2
            ),
            "expected BlockedKindHit(TRIGGER, 2), got {:?}",
            d.failures[0]
        );
    }

    #[test]
    fn blocked_kind_match_is_case_insensitive() {
        let pred = prediction(vec![pi("trigger", ConfidenceLevel::High)]);
        let policy = GatePolicy {
            blocked_kinds: vec!["TRIGGER".into()],
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
    }

    #[test]
    fn min_confidence_floor_rejects_low_rows() {
        let pred = prediction(vec![
            pi("VIEW", ConfidenceLevel::High),
            pi("VIEW", ConfidenceLevel::Low),
        ]);
        let policy = GatePolicy {
            min_confidence: Some(MinConfidence::Medium),
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
        assert!(matches!(
            &d.failures[0],
            GateFailure::ConfidenceBelowFloor {
                floor: MinConfidence::Medium,
                observed_count: 1,
            }
        ));
    }

    #[test]
    fn blocking_unknown_reasons_fires_on_match() {
        let mut pred = prediction(vec![pi("VIEW", ConfidenceLevel::High)]);
        pred.uncertainties.push(UncertaintyRecord {
            reason: UnknownReason::DynamicSqlOpaque,
            affected_owner: None,
            affected_name: None,
            description: "test".into(),
        });
        let policy = GatePolicy {
            blocking_unknown_reasons: vec!["DynamicSqlOpaque".into()],
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
        assert!(matches!(
            &d.failures[0],
            GateFailure::BlockingUnknownReasonHit { reason, observed_count }
                if reason == "DynamicSqlOpaque" && *observed_count == 1
        ));
    }

    #[test]
    fn all_rules_collected_in_one_pass() {
        let pred = prediction(vec![
            pi("TRIGGER", ConfidenceLevel::Low),
            pi("VIEW", ConfidenceLevel::Low),
        ]);
        let policy = GatePolicy {
            max_invalidations: Some(1),
            blocked_kinds: vec!["TRIGGER".into()],
            min_confidence: Some(MinConfidence::High),
            ..GatePolicy::default()
        };
        let d = run_gate(&pred, &policy);
        assert!(!d.allowed);
        assert_eq!(d.failures.len(), 3, "{:#?}", d.failures);
    }

    #[test]
    fn parse_policy_round_trip() {
        let toml = "max_invalidations = 50\nblocked_kinds = [\"TRIGGER\"]\nmin_confidence = \"medium\"\nblocking_unknown_reasons = [\"DynamicSqlOpaque\"]\n";
        let p = parse_policy(toml).unwrap();
        assert_eq!(p.max_invalidations, Some(50));
        assert_eq!(p.blocked_kinds, vec!["TRIGGER"]);
        assert_eq!(p.min_confidence, Some(MinConfidence::Medium));
        assert_eq!(p.blocking_unknown_reasons, vec!["DynamicSqlOpaque"]);
    }

    #[test]
    fn parse_policy_rejects_unknown_keys() {
        let toml = "max_invalidations = 50\nfoobar = \"bad\"\n";
        let err = parse_policy(toml).unwrap_err();
        assert!(matches!(err, GateError::Parse(_)));
    }

    /// **oracle-qm3q.7 regression — fail-open on a stale reason name.**
    /// The old docstring advertised `blocking_unknown_reasons =
    /// ["OpaqueDynamicSql", ...]`, but the analyzer emits
    /// `DynamicSqlOpaque`. An operator copying that example got a
    /// matcher string that never equals any emitted reason, so the
    /// rule never fired and opaque dynamic SQL sailed through the gate
    /// while the operator believed it was blocked. `parse_policy` must
    /// now reject the unknown name at load time instead.
    #[test]
    fn parse_policy_rejects_stale_blocking_reason_name() {
        // The exact stale name from the pre-fix docstring.
        let toml = "blocking_unknown_reasons = [\"OpaqueDynamicSql\"]\n";
        let err = parse_policy(toml).unwrap_err();
        let parse_msg = match err {
            GateError::Parse(msg) => Some(msg),
            GateError::Io(_) => None,
        };
        assert!(parse_msg.is_some(), "expected GateError::Parse");
        let msg = parse_msg.unwrap_or_default();
        assert!(
            msg.contains("OpaqueDynamicSql"),
            "error should name the offending entry: {msg}"
        );
        assert!(
            msg.contains("DynamicSqlOpaque"),
            "error should list the valid canonical names: {msg}"
        );

        // A second-typo reason ("DbLinkReference" — also from the old
        // docstring; only `DbLinkRemoteObject` is real) is rejected too.
        assert!(matches!(
            parse_policy("blocking_unknown_reasons = [\"DbLinkReference\"]\n").unwrap_err(),
            GateError::Parse(_)
        ));
    }

    /// Every canonical reason name `unknown_reason_name` can emit must
    /// be accepted by `parse_policy`. This is the "passes after the
    /// fix" half of the regression and guards against the validator
    /// rejecting a name the matcher actually produces.
    #[test]
    fn parse_policy_accepts_every_canonical_reason_name() {
        for name in ALL_REASON_NAMES {
            let toml = format!("blocking_unknown_reasons = [{name:?}]\n");
            let policy = parse_policy(&toml).ok();
            assert!(policy.is_some(), "canonical reason {name:?} must parse");
            let Some(policy) = policy else {
                continue;
            };
            assert_eq!(policy.blocking_unknown_reasons, vec![(*name).to_string()]);
        }
    }

    /// **Anti-drift guard.** `ALL_REASON_NAMES` (used by the validator)
    /// and `unknown_reason_name` (used by the matcher) must agree on
    /// every `UnknownReason` variant. Iterating an explicit list of all
    /// variants makes adding a variant without updating both sites a
    /// compile error (non-exhaustive match) or a test failure — so the
    /// docstring/matcher/validator can never silently diverge again.
    #[test]
    fn all_reason_names_match_emitter() {
        use plsql_core::UnknownReason as R;
        // Exhaustive: a new variant forces an update here (no `_` arm).
        let all_variants = [
            R::DynamicSqlOpaque,
            R::DbLinkRemoteObject,
            R::WrappedSource,
            R::MissingCatalogObject,
            R::MissingPackageBody,
            R::ConditionalCompilationBranch,
            R::EditionedObject,
            R::InvokerRightsRuntimeResolution,
            R::RuntimeGrantOrRole,
            R::UnsupportedDialectFeature,
            R::ParserRecoveryRegion,
            R::AnalysisRecursionLimit,
            R::ResponseSanitized,
        ];
        // Compile-time exhaustiveness: this match must cover every
        // variant, mirroring `unknown_reason_name`.
        for v in &all_variants {
            let _: () = match v {
                R::DynamicSqlOpaque
                | R::DbLinkRemoteObject
                | R::WrappedSource
                | R::MissingCatalogObject
                | R::MissingPackageBody
                | R::ConditionalCompilationBranch
                | R::EditionedObject
                | R::InvokerRightsRuntimeResolution
                | R::RuntimeGrantOrRole
                | R::UnsupportedDialectFeature
                | R::ParserRecoveryRegion
                | R::AnalysisRecursionLimit
                | R::ResponseSanitized => (),
            };
        }

        // Every emitted name is in the allow-list…
        let emitted: Vec<&'static str> = all_variants
            .iter()
            .map(|v| unknown_reason_name(v))
            .collect();
        for name in &emitted {
            assert!(
                is_known_reason_name(name),
                "{name} is emitted by unknown_reason_name but missing from ALL_REASON_NAMES"
            );
        }
        // …and the allow-list has no extras the matcher cannot emit.
        for name in ALL_REASON_NAMES {
            assert!(
                emitted.contains(name),
                "{name} is in ALL_REASON_NAMES but unknown_reason_name never emits it"
            );
        }
        assert_eq!(emitted.len(), ALL_REASON_NAMES.len());
    }

    #[test]
    fn decision_envelope_carries_schema_id() {
        let d = run_gate(&prediction(vec![]), &GatePolicy::default());
        assert_eq!(d.schema_id, "plsql.cicd.gate_decision");
        assert_eq!(d.schema_version, 1);
    }

    // PLSQL-CICD-014 (oracle-vvxw): pr-comment-json envelope tests.

    #[test]
    fn pr_comment_envelope_passes_on_empty_decision() {
        let d = run_gate(&prediction(vec![]), &GatePolicy::default());
        let env = render_pr_comment(&d);
        assert_eq!(env.format, "robot-json");
        assert_eq!(env.schema_id, "plsql.cicd.gate_pr_comment");
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.pr_comment.verdict, "pass");
        assert!(env.pr_comment.headline.contains("PASS"));
        assert!(env.pr_comment.html_marker.contains("plsql-cicd:gate"));
        assert!(env.pr_comment.body_md.contains("No policy violations"));
    }

    #[test]
    fn pr_comment_envelope_fails_with_per_failure_lines() {
        let pred = prediction(vec![
            pi("TRIGGER", ConfidenceLevel::High),
            pi("TRIGGER", ConfidenceLevel::High),
            pi("PACKAGE BODY", ConfidenceLevel::Low),
        ]);
        let policy = GatePolicy {
            max_invalidations: Some(2),
            blocked_kinds: vec!["TRIGGER".into()],
            min_confidence: Some(MinConfidence::Medium),
            blocking_unknown_reasons: Vec::new(),
        };
        let d = run_gate(&pred, &policy);
        let env = render_pr_comment(&d);
        assert_eq!(env.pr_comment.verdict, "fail");
        assert!(env.pr_comment.headline.contains("FAIL"));
        assert!(env.pr_comment.body_md.contains("max_invalidations"));
        assert!(env.pr_comment.body_md.contains("blocked_kinds"));
        assert!(env.pr_comment.body_md.contains("min_confidence"));
        // body lists exactly the failures (3 numbered lines).
        let numbered = env
            .pr_comment
            .body_md
            .lines()
            .filter(|l| l.starts_with("1. ") || l.starts_with("2. ") || l.starts_with("3. "))
            .count();
        assert_eq!(numbered, d.failures.len());
    }

    #[test]
    fn pr_comment_envelope_is_deterministic_for_identical_input() {
        let pred = prediction(vec![pi("VIEW", ConfidenceLevel::Low)]);
        let policy = GatePolicy {
            min_confidence: Some(MinConfidence::High),
            ..GatePolicy::default()
        };
        let decision = run_gate(&pred, &policy);
        let a = serde_json::to_string(&render_pr_comment(&decision)).unwrap();
        let b = serde_json::to_string(&render_pr_comment(&decision)).unwrap();
        assert_eq!(a, b, "PR-comment envelope must be byte-stable");
    }

    /// **Saturating cast regression.** The four
    /// `len()/count()` narrowings in [`run_gate`] used to be a plain
    /// `as u32` cast — on a 64-bit target, a >u32::MAX-item batch
    /// would wrap to a small u32 and silently bypass the cap. We
    /// cannot allocate 4B items in a unit test; instead pin the
    /// saturation arithmetic itself, which is the load-bearing piece
    /// the fix replaces the truncating cast with.
    #[test]
    fn saturating_cast_does_not_wrap_at_u32_boundary() {
        // 2^32 — the smallest usize that overflows u32.
        let just_over: usize = (u32::MAX as usize).saturating_add(1);
        // The legacy `as u32` cast wraps to 0 here; the fix saturates.
        let saturated = u32::try_from(just_over).unwrap_or(u32::MAX);
        assert_eq!(
            saturated,
            u32::MAX,
            "the (u32::MAX + 1)-item case must saturate to u32::MAX, never wrap to 0"
        );
        // Equally explicit upper bound.
        let saturated_max = u32::try_from(usize::MAX).unwrap_or(u32::MAX);
        assert_eq!(saturated_max, u32::MAX);

        // Sanity: in-range values still round-trip losslessly.
        for n in [0_u32, 1, 100, u32::MAX] {
            assert_eq!(u32::try_from(n as usize).unwrap_or(u32::MAX), n);
        }
    }

    #[test]
    fn pr_comment_envelope_html_marker_is_stable_across_versions() {
        // The marker is keyed on the schema-version so a marker bump is
        // visible to the comment poster. Pin the current shape so a
        // future version bump trips the test and forces a deliberate
        // PLSQL-CICD-016 update.
        let d = run_gate(&prediction(vec![]), &GatePolicy::default());
        let env = render_pr_comment(&d);
        assert_eq!(env.pr_comment.html_marker, "<!-- plsql-cicd:gate v1 -->");
    }
}
