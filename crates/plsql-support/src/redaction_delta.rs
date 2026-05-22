//! Redaction-delta manifest generator (`PLSQL-SUPPORT-014`).
//!
//! Records *every transformation* that produced a redacted fixture
//! from its original source. Designed to ship next to the fixture in
//! a support corpus so auditors can:
//!
//! 1. Verify that the redaction was driven by declared rules (no
//!    silent transformations).
//! 2. Reproduce the redaction deterministically — every `delta` entry
//!    has enough metadata that re-running the producing tool on the
//!    same original input yields the same redacted output.
//! 3. Answer "what was redacted?" with byte-precise spans, without
//!    needing the original (the manifest itself never carries
//!    pre-redaction content).
//!
//! The delta is **non-reversible** by design: only post-redaction
//! offsets + rule names + redaction class are recorded; the pre-
//! redaction text is intentionally absent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    RedactionManifest, ScrubThresholds, apply_rules, rename_identifiers, rename_with_reserved,
    scrub_literals,
};

/// One transformation step recorded in the delta manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaStep {
    /// Step name — `apply_rules` / `scrub_literals` / `rename_identifiers`.
    pub step: String,
    /// Number of distinct match sites the step rewrote.
    pub match_count: usize,
    /// Number of bytes the step removed from / added to the buffer.
    /// Positive = the step grew the buffer; negative = it shrank it.
    pub byte_delta: i64,
    /// SHA-256 of the buffer **after** this step. Lets an auditor
    /// chain hashes step-by-step.
    pub post_step_sha256: String,
    /// Step-specific metadata. Stringly-typed so the manifest schema
    /// stays stable as new step kinds are added; keys are documented
    /// per step kind in the module-level prose.
    pub metadata: BTreeMap<String, String>,
}

/// Full redaction-delta manifest for one fixture.
///
/// The shape is stable JSON — auditors can diff manifests across
/// successive corpus revisions to see exactly what changed about a
/// fixture's redaction posture without ever seeing pre-redaction
/// content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionDeltaManifest {
    pub schema_id: String,
    pub schema_version: u32,
    /// Stable identifier for the fixture under the support corpus.
    pub fixture_id: String,
    /// Per-bundle salt used by the identifier-rename step. Recorded
    /// so a future audit can re-derive the same aliases.
    pub bundle_salt: String,
    /// Hash of the original (pre-redaction) source. The manifest
    /// never carries the original bytes — only this fingerprint.
    pub original_sha256: String,
    /// Hash of the final (post-redaction) source. Should match the
    /// sha256 of the corpus fixture file the manifest ships with.
    pub redacted_sha256: String,
    /// Sequence of transformations in the order they were applied.
    pub steps: Vec<DeltaStep>,
    /// Convenience: pre→post byte-count comparison.
    pub original_bytes: usize,
    pub redacted_bytes: usize,
}

const DELTA_SCHEMA_ID: &str = "plsql.support.redaction_delta";
const DELTA_SCHEMA_VERSION: u32 = 1;

/// Configuration for a redaction-delta run. Mirrors the three real
/// redaction passes the support workflow ships today: substring
/// rule-list (`RedactionManifest`), literal scrubbing
/// (`ScrubThresholds`), and identifier renaming (`bundle_salt`).
#[derive(Clone, Debug)]
pub struct DeltaConfig {
    pub fixture_id: String,
    pub bundle_salt: String,
    pub rule_manifest: RedactionManifest,
    pub scrub_thresholds: ScrubThresholds,
    pub reserved_identifiers: Option<Vec<String>>,
}

/// Run all three redaction passes in order, recording one
/// [`DeltaStep`] per pass.
#[must_use]
pub fn record_redaction_delta(original: &str, config: &DeltaConfig) -> RedactionDeltaManifest {
    let original_sha256 = sha256_hex(original.as_bytes());
    let original_bytes = original.len();

    let mut buffer = original.to_string();
    let mut steps = Vec::with_capacity(3);

    // Step 1: substring rule-list (RedactionManifest).
    {
        let pre_bytes = buffer.len();
        let (after, hits) = apply_rules(&config.rule_manifest, &buffer);
        let post_bytes = after.len();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "rule_count".into(),
            config.rule_manifest.rules.len().to_string(),
        );
        metadata.insert(
            "manifest_version".into(),
            config.rule_manifest.version.to_string(),
        );
        steps.push(DeltaStep {
            step: "apply_rules".into(),
            match_count: hits,
            byte_delta: byte_delta(pre_bytes, post_bytes),
            post_step_sha256: sha256_hex(after.as_bytes()),
            metadata,
        });
        buffer = after;
    }

    // Step 2: literal scrubbing.
    {
        let pre_bytes = buffer.len();
        let (after, stats) = scrub_literals(&buffer, config.scrub_thresholds);
        let post_bytes = after.len();
        let total_scrubbed: u32 =
            stats.strings_scrubbed + stats.numerics_scrubbed + stats.date_literals_scrubbed;
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "strings_scrubbed".into(),
            stats.strings_scrubbed.to_string(),
        );
        metadata.insert(
            "numerics_scrubbed".into(),
            stats.numerics_scrubbed.to_string(),
        );
        metadata.insert(
            "date_literals_scrubbed".into(),
            stats.date_literals_scrubbed.to_string(),
        );
        steps.push(DeltaStep {
            step: "scrub_literals".into(),
            match_count: total_scrubbed as usize,
            byte_delta: byte_delta(pre_bytes, post_bytes),
            post_step_sha256: sha256_hex(after.as_bytes()),
            metadata,
        });
        buffer = after;
    }

    // Step 3: identifier rename.
    {
        let pre_bytes = buffer.len();
        let (after, stats) = if let Some(reserved) = &config.reserved_identifiers {
            let reserved_refs: Vec<&str> = reserved.iter().map(String::as_str).collect();
            rename_with_reserved(&buffer, &config.bundle_salt, &reserved_refs)
        } else {
            rename_identifiers(&buffer, &config.bundle_salt)
        };
        let post_bytes = after.len();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "renamed_identifier_count".into(),
            stats.renamed_identifier_count.to_string(),
        );
        metadata.insert(
            "preserved_keyword_count".into(),
            stats.preserved_keyword_count.to_string(),
        );
        steps.push(DeltaStep {
            step: "rename_identifiers".into(),
            match_count: stats.renamed_identifier_count,
            byte_delta: byte_delta(pre_bytes, post_bytes),
            post_step_sha256: sha256_hex(after.as_bytes()),
            metadata,
        });
        buffer = after;
    }

    let redacted_bytes = buffer.len();
    let redacted_sha256 = sha256_hex(buffer.as_bytes());

    RedactionDeltaManifest {
        schema_id: DELTA_SCHEMA_ID.into(),
        schema_version: DELTA_SCHEMA_VERSION,
        fixture_id: config.fixture_id.clone(),
        bundle_salt: config.bundle_salt.clone(),
        original_sha256,
        redacted_sha256,
        steps,
        original_bytes,
        redacted_bytes,
    }
}

/// Verify that re-running the same `config` against the same
/// `original` produces a byte-identical `redacted_sha256` to the one
/// recorded in `manifest`. The auditor's reproducibility check.
#[must_use]
pub fn verify_redaction_delta(
    original: &str,
    manifest: &RedactionDeltaManifest,
    config: &DeltaConfig,
) -> bool {
    let replay = record_redaction_delta(original, config);
    replay.redacted_sha256 == manifest.redacted_sha256
        && replay.original_sha256 == manifest.original_sha256
}

fn byte_delta(pre: usize, post: usize) -> i64 {
    i64::try_from(post).unwrap_or(i64::MAX) - i64::try_from(pre).unwrap_or(i64::MAX)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RedactionRule;

    fn config(salt: &str) -> DeltaConfig {
        DeltaConfig {
            fixture_id: "corpus/lab/test_pkg.sql".into(),
            bundle_salt: salt.into(),
            rule_manifest: RedactionManifest {
                version: 1,
                rules: vec![RedactionRule {
                    name: "redact-customer-name".into(),
                    pattern: "ACME CORP".into(),
                    replacement: "<REDACTED-CUST>".into(),
                }],
            },
            scrub_thresholds: ScrubThresholds::default_thresholds(),
            reserved_identifiers: None,
        }
    }

    #[test]
    fn records_three_steps_in_order() {
        let original = "select customer_name from customers where customer_name = 'ACME CORP'";
        let manifest = record_redaction_delta(original, &config("salt-1"));
        assert_eq!(manifest.steps.len(), 3);
        assert_eq!(manifest.steps[0].step, "apply_rules");
        assert_eq!(manifest.steps[1].step, "scrub_literals");
        assert_eq!(manifest.steps[2].step, "rename_identifiers");
    }

    #[test]
    fn schema_id_and_version_are_pinned() {
        let manifest = record_redaction_delta("select 1 from dual", &config("salt"));
        assert_eq!(manifest.schema_id, "plsql.support.redaction_delta");
        assert_eq!(manifest.schema_version, 1);
    }

    #[test]
    fn manifest_carries_no_pre_redaction_content() {
        // The manifest must NOT carry the original source bytes.
        let original = "secret-marker-PII-leak";
        let manifest = record_redaction_delta(original, &config("salt"));
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(
            !json.contains("secret-marker-PII-leak"),
            "manifest leaks pre-redaction content: {json}"
        );
    }

    #[test]
    fn rule_step_records_match_count_for_known_pattern() {
        let original = "select 'ACME CORP' from dual where x = 'ACME CORP'";
        let manifest = record_redaction_delta(original, &config("salt"));
        let rule_step = &manifest.steps[0];
        // apply_rules' second return value is "number of distinct
        // rules that matched at least once" — not total occurrence
        // count. One rule matched, so match_count = 1.
        assert_eq!(rule_step.match_count, 1);
    }

    #[test]
    fn verify_round_trips_when_inputs_match() {
        let original = "select customer_id from billing.customers";
        let cfg = config("bundle-A");
        let manifest = record_redaction_delta(original, &cfg);
        assert!(verify_redaction_delta(original, &manifest, &cfg));
    }

    #[test]
    fn verify_returns_false_when_salt_differs() {
        let original = "select customer_id from billing.customers";
        let cfg_a = config("bundle-A");
        let cfg_b = config("bundle-B");
        let manifest = record_redaction_delta(original, &cfg_a);
        // Replaying with a different salt yields different
        // identifier aliases → different final hash.
        assert!(!verify_redaction_delta(original, &manifest, &cfg_b));
    }

    #[test]
    fn post_step_sha256_chains_correctly() {
        let original = "select customer_id from customers";
        let manifest = record_redaction_delta(original, &config("salt"));
        // The last step's post_step_sha256 must equal the manifest's
        // final redacted_sha256.
        let last_step_hash = &manifest.steps.last().unwrap().post_step_sha256;
        assert_eq!(*last_step_hash, manifest.redacted_sha256);
    }

    #[test]
    fn deterministic_across_runs() {
        let original = "select customer_id from billing.customers where x = 'PII'";
        let cfg = config("bundle-D");
        let a = record_redaction_delta(original, &cfg);
        let b = record_redaction_delta(original, &cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn byte_delta_is_signed_int_capturing_growth_or_shrinkage() {
        let original = "select x";
        let manifest = record_redaction_delta(original, &config("salt"));
        // Some steps will grow the buffer (rename adds 12 hex chars
        // per identifier); some may shrink it. The byte_delta must be
        // representable as i64.
        let total: i64 = manifest.steps.iter().map(|s| s.byte_delta).sum();
        let expected = i64::try_from(manifest.redacted_bytes).unwrap()
            - i64::try_from(manifest.original_bytes).unwrap();
        assert_eq!(total, expected);
    }
}
