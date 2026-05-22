#![forbid(unsafe_code)]

//! Support-bundle exporter (PLSQL-SUPPORT-001).
//!
//! When a customer hits a bug in the plsql-intelligence engine the
//! support team needs a reproducible artefact: the run inputs, the
//! diagnostic output, the engine's version stamp, and a redacted
//! view of any sensitive content. This crate packages that artefact
//! as a [`SupportBundle`] — a self-contained, JSON-serialisable
//! data structure that callers can write to disk and ship.
//!
//! The redaction layer is **strict**: every redaction rule is
//! declared in a [`RedactionManifest`] and the redactor only ever
//! removes content matching declared patterns. The manifest carries
//! a `version` so support-side decoders can refuse bundles whose
//! redaction policy they don't recognise.
//!
//! The bundle does NOT directly read files from disk — the caller
//! supplies the inputs via [`SupportBundleBuilder`]. This keeps
//! the crate dependency-light (no `tar` / `zip` crate) and matches
//! the audit posture: every byte that ships into a bundle traces
//! back to an explicit `add_*` call.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const SCHEMA_ID: &str = "plsql.support.bundle";
const SCHEMA_VERSION: u32 = 1;
const MANIFEST_SCHEMA_VERSION: u32 = 1;

pub mod classify_literal;
pub mod encrypt;
pub mod minimize_repro;
pub mod redaction_delta;
pub mod rename;
pub mod scrub_literals;
pub mod shrink;
pub use classify_literal::{LiteralClass, LiteralClassification, classify_literal};
pub use encrypt::{
    EncryptError, EncryptedBundleEnvelope, Encryptor, NullEncryptor, encrypt_bundle,
};
pub use minimize_repro::{
    MinimizationInput, MinimizationPlan, MinimizationStep, MinimizationStrategy, MinimizeError,
    plan_minimize,
};
pub use redaction_delta::{
    DeltaConfig, DeltaStep, RedactionDeltaManifest, record_redaction_delta, verify_redaction_delta,
};
pub use rename::{DEFAULT_RESERVED, RenameStats, rename_identifiers, rename_with_reserved};
pub use scrub_literals::{ScrubStats, ScrubThresholds, scrub_literals};
pub use shrink::{Granularity, ReproOracle, ShrinkResult, shrink_lines, shrink_with_chunks};

/// The exportable bundle. Serialises to JSON; the caller decides
/// whether to wrap it in an archive (PLSQL-SUPPORT-002 will add
/// optional age/PGP encryption).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportBundle {
    pub schema_id: String,
    pub schema_version: u32,
    /// Crate / tool version that produced the bundle.
    pub tool_version: String,
    /// Timestamp from `std::time::SystemTime` formatted as
    /// `YYYY-MM-DDTHH:MM:SSZ` (UTC). Stored as a string so the
    /// bundle does not pull in a chrono dep.
    pub generated_at_utc: String,
    /// Redaction manifest version that was applied to every blob
    /// in `inputs` / `outputs` / `diagnostics` below.
    pub redaction_manifest_version: u32,
    /// A single line operator-supplied describing the issue.
    pub operator_note: Option<String>,
    /// Aggregated, redacted blobs by category.
    pub inputs: Vec<NamedBlob>,
    pub outputs: Vec<NamedBlob>,
    pub diagnostics: Vec<NamedBlob>,
    /// Environment hints (engine version, host OS, Rust toolchain).
    /// Values are stored verbatim — the caller is responsible for
    /// keeping them PII-free.
    pub environment: BTreeMap<String, String>,
}

/// A single named blob inside a `SupportBundle`. The `sha256`
/// field is computed against the redacted bytes so support can
/// confirm integrity without seeing the pre-redaction content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedBlob {
    pub name: String,
    pub redacted_content: String,
    pub sha256: String,
    /// Number of distinct redaction rules that matched at least
    /// once on this blob. Lets the operator see which categories
    /// of sensitive data were present.
    pub redactions_applied: usize,
}

/// Caller-supplied redaction policy. Every rule has a name (used
/// in audit reports) and a literal substring or regex-shaped
/// pattern that the redactor substitutes with the rule's
/// `replacement`. For PLSQL-SUPPORT-001 we keep the matcher
/// substring-only to avoid the regex crate; regex support lands
/// in -002 if needed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionManifest {
    pub version: u32,
    pub rules: Vec<RedactionRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionRule {
    pub name: String,
    /// Literal substring to redact (case-insensitive).
    pub pattern: String,
    pub replacement: String,
}

impl RedactionManifest {
    /// Convenience: an empty manifest. The bundle still records
    /// the manifest version so support can audit which policy was
    /// applied.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            version: MANIFEST_SCHEMA_VERSION,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SupportError {
    #[error("operator note is empty")]
    EmptyOperatorNote,
}

/// Builder for [`SupportBundle`]. Inputs / outputs / diagnostics
/// flow through `add_input` / `add_output` / `add_diagnostic` —
/// each call applies the manifest's redaction rules before storing
/// the blob.
#[derive(Debug)]
pub struct SupportBundleBuilder {
    bundle: SupportBundle,
    manifest: RedactionManifest,
}

impl SupportBundleBuilder {
    #[must_use]
    pub fn new(tool_version: &str, generated_at_utc: &str, manifest: RedactionManifest) -> Self {
        let mut bundle = SupportBundle {
            schema_id: SCHEMA_ID.into(),
            schema_version: SCHEMA_VERSION,
            tool_version: tool_version.into(),
            generated_at_utc: generated_at_utc.into(),
            redaction_manifest_version: manifest.version,
            ..SupportBundle::default()
        };
        bundle.environment = BTreeMap::new();
        Self { bundle, manifest }
    }

    /// Set the operator note (single-line description of the issue).
    pub fn operator_note(&mut self, note: &str) -> Result<&mut Self, SupportError> {
        let trimmed = note.trim();
        if trimmed.is_empty() {
            return Err(SupportError::EmptyOperatorNote);
        }
        self.bundle.operator_note = Some(trimmed.to_string());
        Ok(self)
    }

    pub fn environment(&mut self, key: &str, value: &str) -> &mut Self {
        self.bundle
            .environment
            .insert(key.to_string(), value.to_string());
        self
    }

    pub fn add_input(&mut self, name: &str, content: &str) -> &mut Self {
        let blob = self.redact(name, content);
        self.bundle.inputs.push(blob);
        self
    }

    pub fn add_output(&mut self, name: &str, content: &str) -> &mut Self {
        let blob = self.redact(name, content);
        self.bundle.outputs.push(blob);
        self
    }

    pub fn add_diagnostic(&mut self, name: &str, content: &str) -> &mut Self {
        let blob = self.redact(name, content);
        self.bundle.diagnostics.push(blob);
        self
    }

    #[must_use]
    pub fn build(self) -> SupportBundle {
        self.bundle
    }

    fn redact(&self, name: &str, content: &str) -> NamedBlob {
        let (redacted, hits) = apply_rules(&self.manifest, content);
        NamedBlob {
            name: name.to_string(),
            sha256: sha256_hex(redacted.as_bytes()),
            redacted_content: redacted,
            redactions_applied: hits,
        }
    }
}

/// Render an `sha256:<hex>` string from arbitrary bytes. Centralised
/// so the sha2 0.11+ digest type (`Array<u8, …>`, no `LowerHex`) is
/// formatted byte-by-byte in exactly one place (mirrors the
/// `plsql-mcp::preview` helper).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Apply every rule in `manifest` to `content`. Returns the
/// redacted text plus the count of distinct rules that hit at
/// least once.
#[must_use]
pub fn apply_rules(manifest: &RedactionManifest, content: &str) -> (String, usize) {
    let mut out = content.to_string();
    let mut hits = 0;
    for rule in &manifest.rules {
        if rule.pattern.is_empty() {
            continue;
        }
        let lower_out = out.to_ascii_lowercase();
        let lower_pat = rule.pattern.to_ascii_lowercase();
        if lower_out.contains(&lower_pat) {
            hits += 1;
            out = replace_case_insensitive(&out, &rule.pattern, &rule.replacement);
        }
    }
    (out, hits)
}

fn replace_case_insensitive(haystack: &str, pattern: &str, replacement: &str) -> String {
    if pattern.is_empty() {
        return haystack.to_string();
    }
    let lower_hay = haystack.to_ascii_lowercase();
    let lower_pat = pattern.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    while let Some(found_at) = lower_hay[cursor..].find(&lower_pat) {
        let absolute = cursor + found_at;
        out.push_str(&haystack[cursor..absolute]);
        out.push_str(replacement);
        cursor = absolute + pattern.len();
    }
    out.push_str(&haystack[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str, pattern: &str, replacement: &str) -> RedactionRule {
        RedactionRule {
            name: name.into(),
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }

    fn manifest(rules: Vec<RedactionRule>) -> RedactionManifest {
        RedactionManifest {
            version: MANIFEST_SCHEMA_VERSION,
            rules,
        }
    }

    #[test]
    fn empty_manifest_passes_content_unchanged() {
        let (out, hits) = apply_rules(&RedactionManifest::empty(), "Hello world");
        assert_eq!(out, "Hello world");
        assert_eq!(hits, 0);
    }

    #[test]
    fn substring_rule_redacts_match() {
        let m = manifest(vec![rule("schema-name", "HR.", "<REDACTED>.")]);
        let (out, hits) = apply_rules(&m, "SELECT * FROM HR.EMPLOYEES");
        assert_eq!(out, "SELECT * FROM <REDACTED>.EMPLOYEES");
        assert_eq!(hits, 1);
    }

    #[test]
    fn case_insensitive_matching() {
        let m = manifest(vec![rule("schema-name", "hr.", "<X>.")]);
        let (out, _) = apply_rules(&m, "FROM HR.EMPLOYEES, hr.dept");
        assert_eq!(out, "FROM <X>.EMPLOYEES, <X>.dept");
    }

    #[test]
    fn multiple_rules_aggregate_hits() {
        let m = manifest(vec![
            rule("schema-a", "HR.", "<A>."),
            rule("schema-b", "DBA.", "<B>."),
        ]);
        let (out, hits) = apply_rules(&m, "HR.X and DBA.Y");
        assert_eq!(out, "<A>.X and <B>.Y");
        assert_eq!(hits, 2);
    }

    #[test]
    fn empty_pattern_skipped() {
        let m = manifest(vec![rule("noop", "", "<X>")]);
        let (out, hits) = apply_rules(&m, "anything");
        assert_eq!(out, "anything");
        assert_eq!(hits, 0);
    }

    #[test]
    fn builder_emits_sha256_and_redactions_applied() {
        let m = manifest(vec![rule("schema", "HR.", "<R>.")]);
        let mut b = SupportBundleBuilder::new("1.2.3", "2026-05-15T17:00:00Z", m);
        b.operator_note("issue with view xyz").unwrap();
        b.add_input("query.sql", "SELECT * FROM HR.EMPLOYEES");
        b.add_diagnostic("trace.log", "no schema here");
        let bundle = b.build();
        assert_eq!(bundle.inputs.len(), 1);
        assert_eq!(bundle.inputs[0].redactions_applied, 1);
        assert!(bundle.inputs[0].sha256.starts_with("sha256:"));
        // Diagnostic had no matches.
        assert_eq!(bundle.diagnostics[0].redactions_applied, 0);
    }

    #[test]
    fn operator_note_rejects_empty() {
        let mut b = SupportBundleBuilder::new("1", "t", RedactionManifest::empty());
        assert!(b.operator_note("").is_err());
        assert!(b.operator_note("   ").is_err());
        assert!(b.operator_note(" ok ").is_ok());
    }

    #[test]
    fn bundle_serialises_round_trip() {
        let mut b = SupportBundleBuilder::new("1", "t", RedactionManifest::empty());
        b.operator_note("test").unwrap();
        b.environment("os", "linux");
        b.add_input("a.sql", "SELECT 1");
        let bundle = b.build();
        let json = serde_json::to_string(&bundle).unwrap();
        let back: SupportBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(back, bundle);
        assert!(json.contains("plsql.support.bundle"));
    }

    #[test]
    fn environment_keys_round_trip_in_order() {
        let mut b = SupportBundleBuilder::new("1", "t", RedactionManifest::empty());
        b.environment("os", "linux");
        b.environment("arch", "x86_64");
        b.environment("rustc", "1.85");
        let bundle = b.build();
        // BTreeMap sorts keys: arch, os, rustc.
        let keys: Vec<&String> = bundle.environment.keys().collect();
        assert_eq!(keys, vec!["arch", "os", "rustc"]);
    }

    #[test]
    fn manifest_version_persists_into_bundle() {
        let m = RedactionManifest {
            version: 42,
            rules: vec![],
        };
        let b = SupportBundleBuilder::new("1", "t", m);
        let bundle = b.build();
        assert_eq!(bundle.redaction_manifest_version, 42);
    }
}
