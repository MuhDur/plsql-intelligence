//! `gate <changeset>` (PLSQL-CICD-006).
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
//! # this list (e.g. you want to block on opaque dynamic SQL).
//! blocking_unknown_reasons = ["OpaqueDynamicSql", "DbLinkReference"]
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

/// Load a policy from a TOML string. The CLI bead (PLSQL-CICD-007)
/// pairs this with a file reader.
pub fn parse_policy(toml_text: &str) -> Result<GatePolicy, GateError> {
    toml::from_str(toml_text).map_err(|e| GateError::Parse(e.to_string()))
}

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
    /// Stable HTML marker the downstream comment-poster (`PLSQL-CICD-016`)
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
#[must_use]
pub fn run_gate(prediction: &InvalidationPrediction, policy: &GatePolicy) -> GateDecision {
    let mut failures: Vec<GateFailure> = Vec::new();

    if let Some(cap) = policy.max_invalidations {
        let observed = prediction.predicted_invalidations.len() as u32;
        if observed > cap {
            failures.push(GateFailure::InvalidationsExceeded { cap, observed });
        }
    }

    for blocked in &policy.blocked_kinds {
        let observed_count = prediction
            .predicted_invalidations
            .iter()
            .filter(|p| p.object_type.eq_ignore_ascii_case(blocked))
            .count() as u32;
        if observed_count > 0 {
            failures.push(GateFailure::BlockedKindHit {
                kind: blocked.clone(),
                observed_count,
            });
        }
    }

    if let Some(floor) = policy.min_confidence {
        let floor_level = floor.as_level();
        let observed_count = prediction
            .predicted_invalidations
            .iter()
            .filter(|p| confidence_below_floor(&p.confidence.level, floor_level))
            .count() as u32;
        if observed_count > 0 {
            failures.push(GateFailure::ConfidenceBelowFloor {
                floor,
                observed_count,
            });
        }
    }

    for blocked_reason in &policy.blocking_unknown_reasons {
        let observed_count = prediction
            .uncertainties
            .iter()
            .filter(|u| unknown_reason_name(&u.reason) == blocked_reason.as_str())
            .count() as u32;
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
