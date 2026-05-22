#![forbid(unsafe_code)]

use plsql_core::{Diagnostic, Evidence, JsonExportable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;

pub const ROBOT_JSON_FORMAT: &str = "plsql-robot-json";
pub const REDACTED_TEXT: &str = "[REDACTED]";

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl SchemaVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaDescriptor {
    pub id: &'static str,
    pub version: SchemaVersion,
    pub description: &'static str,
}

pub const ROBOT_JSON_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.output.robot_json",
    version: SchemaVersion::new(1, 0, 0),
    description: "Generic machine-readable envelope for plsql-intelligence CLIs",
};

pub const DIAGNOSTIC_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.output.diagnostics",
    version: SchemaVersion::new(1, 0, 0),
    description: "Diagnostic report envelope wrapping plsql-core diagnostics",
};

pub const EVIDENCE_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.output.evidence",
    version: SchemaVersion::new(1, 0, 0),
    description: "Structured evidence envelope wrapping plsql-core evidence records",
};

pub const OUTPUT_SCHEMAS: [SchemaDescriptor; 3] =
    [ROBOT_JSON_SCHEMA, DIAGNOSTIC_SCHEMA, EVIDENCE_SCHEMA];

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RobotJsonEnvelope<T> {
    pub format: String,
    pub schema_id: String,
    pub schema_version: SchemaVersion,
    pub payload: T,
}

impl<T> RobotJsonEnvelope<T> {
    #[must_use]
    #[instrument(level = "trace", skip(payload))]
    pub fn new(schema: SchemaDescriptor, payload: T) -> Self {
        Self {
            format: String::from(ROBOT_JSON_FORMAT),
            schema_id: String::from(schema.id),
            schema_version: schema.version,
            payload,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn matches_schema(&self, schema: SchemaDescriptor) -> bool {
        self.schema_id == schema.id && self.schema_version == schema.version
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEnvelope {
    #[serde(flatten)]
    pub envelope: RobotJsonEnvelope<Vec<Diagnostic>>,
}

impl DiagnosticEnvelope {
    #[must_use]
    #[instrument(level = "trace", skip(diagnostics))]
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            envelope: RobotJsonEnvelope::new(DIAGNOSTIC_SCHEMA, diagnostics),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, policy))]
    pub fn redacted(&self, policy: &RedactionPolicy) -> Self {
        let diagnostics = self
            .envelope
            .payload
            .iter()
            .map(|diagnostic| policy.redact_diagnostic(diagnostic))
            .collect();
        Self::new(diagnostics)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EvidenceEnvelope {
    #[serde(flatten)]
    pub envelope: RobotJsonEnvelope<Vec<Evidence>>,
}

impl EvidenceEnvelope {
    #[must_use]
    #[instrument(level = "trace", skip(evidence))]
    pub fn new(evidence: Vec<Evidence>) -> Self {
        Self {
            envelope: RobotJsonEnvelope::new(EVIDENCE_SCHEMA, evidence),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, policy))]
    pub fn redacted(&self, policy: &RedactionPolicy) -> Self {
        let evidence = self
            .envelope
            .payload
            .iter()
            .map(|entry| policy.redact_evidence(entry))
            .collect();
        Self::new(evidence)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RedactionPolicy {
    pub redact_freeform_text: bool,
    pub strip_attributes: bool,
    pub keep_source_spans: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            redact_freeform_text: false,
            strip_attributes: false,
            keep_source_spans: true,
        }
    }
}

impl RedactionPolicy {
    #[must_use]
    #[instrument(level = "trace", skip(self, diagnostic))]
    pub fn redact_diagnostic(&self, diagnostic: &Diagnostic) -> Diagnostic {
        let mut redacted = diagnostic.clone();
        if self.redact_freeform_text {
            redacted.message = String::from(REDACTED_TEXT);
            redacted.help = redacted.help.as_ref().map(|_| String::from(REDACTED_TEXT));
            redacted.related_spans.iter_mut().for_each(|label| {
                label.label = String::from(REDACTED_TEXT);
            });
        }
        if !self.keep_source_spans {
            redacted.primary_span = None;
            redacted.related_spans.clear();
        }
        redacted.evidence = diagnostic
            .evidence
            .iter()
            .map(|evidence| self.redact_evidence(evidence))
            .collect();
        redacted
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, evidence))]
    pub fn redact_evidence(&self, evidence: &Evidence) -> Evidence {
        let mut redacted = evidence.clone();
        if self.redact_freeform_text {
            redacted.summary = String::from(REDACTED_TEXT);
            redacted.notes.iter_mut().for_each(|note| {
                *note = String::from(REDACTED_TEXT);
            });
            redacted.spans.iter_mut().for_each(|label| {
                label.label = String::from(REDACTED_TEXT);
            });
        }
        if self.strip_attributes {
            redacted.attributes.clear();
        }
        if !self.keep_source_spans {
            redacted.spans.clear();
        }
        redacted
    }
}

#[instrument(level = "trace", skip(value))]
pub fn envelope_to_json_value<T>(value: &RobotJsonEnvelope<T>) -> serde_json::Result<Value>
where
    T: JsonExportable,
{
    serde_json::to_value(value)
}

pub fn envelope_from_json_value<T>(value: Value) -> serde_json::Result<RobotJsonEnvelope<T>>
where
    T: JsonExportable,
{
    serde_json::from_value(value)
}

// ---------------------------------------------------------------------------
// Orphan candidate types (LIN-018) — §13.8 Orphan Candidates Report
// ---------------------------------------------------------------------------

/// Confidence tier for an orphan candidate classification.
///
/// Higher tiers mean stronger evidence that the object is truly unused.
/// Reports MUST NOT collapse these into a single scalar — the tier is the
/// trust signal, per §1.5 Evidence UX.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrphanConfidenceTier {
    /// Strong evidence of non-use: no inbound references in code, catalog,
    /// or dependency graph. Observation window met with AUDIT-based monitoring.
    HighConfidenceUnused,
    /// Probable non-use: no inbound code references, but catalog/dependency
    /// evidence is incomplete (missing catalog, wrapped sources, dynamic SQL).
    LikelyUnused,
    /// Ambiguous: some references exist but are indirect (synonyms, public
    /// grants, role-mediated access) or behind dynamic SQL sites.
    MaybeUnused,
    /// Cannot determine: insufficient data (missing catalog, missing source,
    /// wrapped code, DB-link boundary).
    Inconclusive,
}

/// An object identified as a potential orphan — candidate for cleanup.
///
/// Part of the orphan-candidates report (§13.8). Every candidate carries a
/// confidence tier and evidence list. Reports MUST pair each candidate with
/// a concrete remediation step (AUDIT statement, not DROP script).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrphanCandidate {
    /// Logical object identifier (schema.object).
    pub object_id: String,
    /// Object kind (TABLE, VIEW, PACKAGE, PROCEDURE, FUNCTION, SEQUENCE,
    /// TYPE, TRIGGER, SYNONYM, INDEX).
    pub kind: String,
    /// Last observed usage timestamp, if available. String for flexibility
    /// (ISO-8601 or Oracle's native format).
    pub last_used: Option<String>,
    /// Structured evidence explaining why this object is a candidate.
    /// Each entry is a human-readable reason string.
    pub evidence: Vec<String>,
    /// Confidence tier for this classification.
    pub confidence: OrphanConfidenceTier,
}

/// A complete orphan candidates report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrphanCandidatesReport {
    /// Candidates grouped by confidence tier (not a scalar score — §1.5).
    pub candidates: Vec<OrphanCandidate>,
    /// Total objects examined.
    pub objects_examined: usize,
    /// Objects with at least one inbound reference.
    pub objects_with_references: usize,
    /// Observation window applied (e.g. "30d", "60d", "90d").
    pub observation_window: Option<String>,
}

#[cfg(test)]
mod tests {
    use plsql_core::{Confidence, ConfidenceLevel, Diagnostic, Evidence, FileId, Position, Span};
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::{
        DiagnosticEnvelope, EvidenceEnvelope, OUTPUT_SCHEMAS, OrphanCandidate,
        OrphanCandidatesReport, OrphanConfidenceTier, REDACTED_TEXT, ROBOT_JSON_SCHEMA,
        RedactionPolicy, RobotJsonEnvelope, SchemaVersion, envelope_from_json_value,
        envelope_to_json_value,
    };

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    struct TrivialPayload {
        ok: bool,
    }

    #[test]
    fn robot_json_round_trips_trivial_payloads() {
        let payload = TrivialPayload { ok: true };
        let envelope = RobotJsonEnvelope::new(ROBOT_JSON_SCHEMA, payload);
        let value = envelope_to_json_value(&envelope);
        assert!(value.is_ok());

        let reparsed = value.and_then(envelope_from_json_value::<TrivialPayload>);
        assert!(reparsed.is_ok());

        let reparsed = reparsed.unwrap_or_else(|_| {
            RobotJsonEnvelope::new(ROBOT_JSON_SCHEMA, TrivialPayload { ok: false })
        });
        assert_eq!(reparsed.schema_version, SchemaVersion::new(1, 0, 0));
        assert!(reparsed.matches_schema(ROBOT_JSON_SCHEMA));
        assert!(reparsed.payload.ok);
    }

    #[test]
    fn output_schema_registry_is_stable_and_complete() {
        assert_eq!(OUTPUT_SCHEMAS.len(), 3);
        assert_eq!(OUTPUT_SCHEMAS[0].id, "plsql.output.robot_json");
        assert_eq!(OUTPUT_SCHEMAS[1].version, SchemaVersion::new(1, 0, 0));
        assert_eq!(
            OUTPUT_SCHEMAS[2].description,
            "Structured evidence envelope wrapping plsql-core evidence records"
        );
    }

    #[test]
    fn diagnostic_envelope_redaction_preserves_structure() {
        let span = Span::new(
            FileId::new(2),
            Position::new(3, 1, 15),
            Position::new(3, 6, 20),
        );
        let diagnostic = Diagnostic::new("CAT001", plsql_core::Severity::Warn, "bad catalog row")
            .with_primary_span(span)
            .with_help("refresh the snapshot")
            .with_evidence(
                Evidence::new("CAT-EVIDENCE", "saw inconsistent owner")
                    .with_note("owner column empty")
                    .with_attribute("row", json!(7))
                    .with_confidence(Confidence::new(
                        ConfidenceLevel::Medium,
                        Some(String::from("catalog probe recovered")),
                    )),
            );

        let policy = RedactionPolicy {
            redact_freeform_text: true,
            strip_attributes: true,
            keep_source_spans: false,
        };
        let envelope = DiagnosticEnvelope::new(vec![diagnostic]).redacted(&policy);

        assert_eq!(envelope.envelope.payload.len(), 1);
        assert_eq!(envelope.envelope.payload[0].message, REDACTED_TEXT);
        assert_eq!(envelope.envelope.payload[0].primary_span, None);
        assert_eq!(
            envelope.envelope.payload[0].evidence[0].summary,
            REDACTED_TEXT
        );
        assert!(
            envelope.envelope.payload[0].evidence[0]
                .attributes
                .is_empty()
        );
    }

    #[test]
    fn redaction_scrubs_freeform_text_while_keeping_spans() {
        // Security contract for the realistic "scrub text, keep
        // positions for debugging" policy — the path the structure
        // test does NOT exercise (it clears spans). A regression
        // that leaked a span/help/note label would pass that test
        // but must fail this one.
        let span = Span::new(
            FileId::new(4),
            Position::new(7, 2, 40),
            Position::new(7, 9, 47),
        );
        // Synthetic freeform marker (not a credential — just a
        // unique token we assert never survives redaction).
        let sensitive = "FREEFORM_LEAK_CANARY_xyzzy";
        let diagnostic = Diagnostic::new("SEC001", plsql_core::Severity::Error, sensitive)
            .with_primary_span(span)
            .with_help(sensitive)
            .with_related_span(plsql_core::SpanLabel::new(sensitive, span))
            .with_evidence(
                Evidence::new("E1", sensitive)
                    .with_note(sensitive)
                    .with_span(plsql_core::SpanLabel::new(sensitive, span)),
            );

        let policy = RedactionPolicy {
            redact_freeform_text: true,
            strip_attributes: false,
            keep_source_spans: true,
        };
        let out = DiagnosticEnvelope::new(vec![diagnostic]).redacted(&policy);
        let d = &out.envelope.payload[0];

        assert_eq!(d.message, REDACTED_TEXT);
        assert_eq!(d.help.as_deref(), Some(REDACTED_TEXT));
        // Spans kept (positions retained) but their freeform labels
        // scrubbed — no `secret` substring survives anywhere.
        assert_eq!(d.primary_span, Some(span));
        assert_eq!(d.related_spans.len(), 1);
        assert_eq!(d.related_spans[0].label, REDACTED_TEXT);
        assert_eq!(d.related_spans[0].span, span);
        let ev = &d.evidence[0];
        assert_eq!(ev.summary, REDACTED_TEXT);
        assert_eq!(ev.notes, vec![String::from(REDACTED_TEXT)]);
        assert_eq!(ev.spans.len(), 1);
        assert_eq!(ev.spans[0].label, REDACTED_TEXT);

        let json = serde_json::to_string(&out).expect("envelope serializes");
        assert!(
            !json.contains("xyzzy"),
            "redacted envelope must not leak the secret in any field"
        );
    }

    #[test]
    fn evidence_envelope_uses_stable_schema_id() {
        let envelope = EvidenceEnvelope::new(vec![Evidence::new("SYM001", "resolved")]);

        assert_eq!(envelope.envelope.schema_id, "plsql.output.evidence");
        assert_eq!(
            envelope.envelope.schema_version,
            SchemaVersion::new(1, 0, 0)
        );
    }

    #[test]
    fn orphan_candidate_roundtrip_json() {
        let report = OrphanCandidatesReport {
            candidates: vec![
                OrphanCandidate {
                    object_id: "billing.legacy_pkg".into(),
                    kind: "PACKAGE".into(),
                    last_used: Some("2024-01-15T10:30:00Z".into()),
                    evidence: vec![
                        "No inbound call edges in dependency graph".into(),
                        "No PL/Scope references found".into(),
                        "AUDIT monitored for 90 days with zero hits".into(),
                    ],
                    confidence: OrphanConfidenceTier::HighConfidenceUnused,
                },
                OrphanCandidate {
                    object_id: "billing.temp_reports".into(),
                    kind: "TABLE".into(),
                    last_used: None,
                    evidence: vec![
                        "No DML edges in dependency graph".into(),
                        "Missing catalog metadata (wrapped source)".into(),
                    ],
                    confidence: OrphanConfidenceTier::LikelyUnused,
                },
                OrphanCandidate {
                    object_id: "billing.util_fn".into(),
                    kind: "FUNCTION".into(),
                    last_used: Some("2025-12-01".into()),
                    evidence: vec![
                        "Called only via public synonym — may be used externally".into(),
                    ],
                    confidence: OrphanConfidenceTier::MaybeUnused,
                },
                OrphanCandidate {
                    object_id: "billing.remote_pkg".into(),
                    kind: "PACKAGE".into(),
                    last_used: None,
                    evidence: vec!["Object on DB-link boundary — cannot determine usage".into()],
                    confidence: OrphanConfidenceTier::Inconclusive,
                },
            ],
            objects_examined: 150,
            objects_with_references: 120,
            observation_window: Some("90d".into()),
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let back: OrphanCandidatesReport = serde_json::from_str(&json).unwrap();

        assert_eq!(back.candidates.len(), 4);
        assert_eq!(back.objects_examined, 150);
        assert_eq!(back.objects_with_references, 120);
        assert_eq!(back.observation_window, Some("90d".into()));

        assert_eq!(
            back.candidates[0].confidence,
            OrphanConfidenceTier::HighConfidenceUnused
        );
        assert_eq!(
            back.candidates[1].confidence,
            OrphanConfidenceTier::LikelyUnused
        );
        assert_eq!(
            back.candidates[2].confidence,
            OrphanConfidenceTier::MaybeUnused
        );
        assert_eq!(
            back.candidates[3].confidence,
            OrphanConfidenceTier::Inconclusive
        );

        // Verify tagged serde
        assert!(json.contains("high_confidence_unused"));
        assert!(json.contains("likely_unused"));
        assert!(json.contains("maybe_unused"));
        assert!(json.contains("inconclusive"));

        // Verify evidence roundtrips
        assert_eq!(back.candidates[0].evidence.len(), 3);
        assert_eq!(
            back.candidates[0].last_used,
            Some("2024-01-15T10:30:00Z".into())
        );
    }

    #[test]
    fn orphan_tier_serde_rename() {
        let json = serde_json::to_string(&OrphanConfidenceTier::HighConfidenceUnused).unwrap();
        assert_eq!(json, "\"high_confidence_unused\"");
        let json = serde_json::to_string(&OrphanConfidenceTier::Inconclusive).unwrap();
        assert_eq!(json, "\"inconclusive\"");
    }
}
