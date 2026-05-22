#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use miette::SourceSpan;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use tracing::instrument;

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            #[instrument(level = "trace")]
            pub fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn get(self) -> u64 {
                self.0
            }
        }
    };
}

macro_rules! interned_name {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(SymbolId);

        impl $name {
            #[must_use]
            #[instrument(level = "trace")]
            pub fn new(symbol: SymbolId) -> Self {
                Self(symbol)
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn symbol(self) -> SymbolId {
                self.0
            }
        }

        impl From<SymbolId> for $name {
            fn from(value: SymbolId) -> Self {
                Self::new(value)
            }
        }
    };
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct FileId(u32);

impl FileId {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn get(self) -> u32 {
        self.0
    }
}

numeric_id!(AnalysisRunId);
numeric_id!(SymbolId);
numeric_id!(ObjectId);
numeric_id!(ColumnId);
numeric_id!(MemberId);

interned_name!(SchemaName);
interned_name!(UserName);
interned_name!(EditionName);
interned_name!(RoleName);
interned_name!(ObjectName);
interned_name!(ColumnName);
interned_name!(MemberName);

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Position {
    pub line: u32,
    pub column: u32,
    pub offset: u32,
}

impl Position {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(line: u32, column: u32, offset: u32) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Span {
    pub file_id: FileId,
    pub start: Position,
    pub end: Position,
}

impl Span {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(file_id: FileId, start: Position, end: Position) -> Self {
        Self {
            file_id,
            start,
            end,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn len(self) -> u32 {
        self.end.offset.saturating_sub(self.start.offset)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(self) -> bool {
        self.start.offset >= self.end.offset
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn source_span(self) -> SourceSpan {
        SourceSpan::from((
            usize::try_from(self.start.offset).unwrap_or(usize::MAX),
            usize::try_from(self.len()).unwrap_or(usize::MAX),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpanLabel {
    pub label: String,
    pub span: Span,
}

impl SpanLabel {
    #[must_use]
    #[instrument(level = "trace", skip(label))]
    pub fn new(label: impl Into<String>, span: Span) -> Self {
        Self {
            label: label.into(),
            span,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum Severity {
    #[default]
    Info,
    Warn,
    Error,
    Fatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum UnknownReason {
    DynamicSqlOpaque,
    DbLinkRemoteObject,
    WrappedSource,
    MissingCatalogObject,
    MissingPackageBody,
    ConditionalCompilationBranch,
    EditionedObject,
    InvokerRightsRuntimeResolution,
    RuntimeGrantOrRole,
    UnsupportedDialectFeature,
    ParserRecoveryRegion,
    /// A bounded-depth analysis walk (call-site / table-access
    /// extraction over re-lowered control-flow bodies) hit its
    /// recursion-depth cap before the body provably shrank — almost
    /// always a malformed / parser-recovered unit whose `IF`/`LOOP`
    /// text slice fails to strictly shrink across re-lowering passes.
    /// The remainder of that nested body is degraded honestly rather
    /// than walked unbounded (which would stack-overflow / abort).
    AnalysisRecursionLimit,
    /// Response had MCP / tool-call markers scrubbed before being returned
    /// to the agent. Tracks `PLSQL-MCP-LIVE-004`'s K18 prompt-injection
    /// sanitization step so downstream consumers know the row text was
    /// rewritten.
    ResponseSanitized,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
    #[default]
    Opaque,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Confidence {
    pub level: ConfidenceLevel,
    pub explanation: Option<String>,
}

impl Confidence {
    #[must_use]
    #[instrument(level = "trace", skip(explanation))]
    pub fn new(level: ConfidenceLevel, explanation: impl Into<Option<String>>) -> Self {
        Self {
            level,
            explanation: explanation.into(),
        }
    }

    #[must_use]
    #[instrument(level = "trace")]
    pub fn opaque() -> Self {
        Self {
            level: ConfidenceLevel::Opaque,
            explanation: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub code: String,
    pub summary: String,
    pub spans: Vec<SpanLabel>,
    pub notes: Vec<String>,
    pub attributes: BTreeMap<String, Value>,
    pub confidence: Option<Confidence>,
}

impl Evidence {
    #[must_use]
    #[instrument(level = "trace", skip(code, summary))]
    pub fn new(code: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            summary: summary.into(),
            ..Self::default()
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_span(mut self, span: SpanLabel) -> Self {
        self.spans.push(span);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, note))]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, key, value))]
    pub fn with_attribute(mut self, key: impl Into<String>, value: Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Option<Span>,
    pub related_spans: Vec<SpanLabel>,
    pub help: Option<String>,
    pub unknown_reasons: Vec<UnknownReason>,
    pub evidence: Vec<Evidence>,
}

impl Diagnostic {
    #[must_use]
    #[instrument(level = "trace", skip(code, message))]
    pub fn new(code: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            ..Self::default()
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_primary_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_related_span(mut self, span: SpanLabel) -> Self {
        self.related_spans.push(span);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, help))]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_unknown_reason(mut self, reason: UnknownReason) -> Self {
        self.unknown_reasons.push(reason);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_evidence(mut self, evidence: Evidence) -> Self {
        self.evidence.push(evidence);
        self
    }
}

pub trait JsonExportable: Serialize + DeserializeOwned {
    #[instrument(level = "trace", skip(self))]
    fn to_json_value(&self) -> serde_json::Result<Value> {
        serde_json::to_value(self)
    }

    fn from_json_value(value: Value) -> serde_json::Result<Self>
    where
        Self: Sized,
    {
        serde_json::from_value(value)
    }
}

impl<T> JsonExportable for T where T: Serialize + DeserializeOwned {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RobotJson<T> {
    pub payload: T,
}

impl<T> RobotJson<T> {
    #[must_use]
    #[instrument(level = "trace", skip(payload))]
    pub fn new(payload: T) -> Self {
        Self { payload }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LiteralValue {
    String(String),
    Integer(i64),
    Decimal(String),
    Boolean(bool),
    Null,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum NlsLengthSemantics {
    #[default]
    Byte,
    Char,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NlsSettings {
    pub language: Option<String>,
    pub territory: Option<String>,
    pub date_format: Option<String>,
    pub timestamp_format: Option<String>,
    pub timestamp_tz_format: Option<String>,
    pub length_semantics: NlsLengthSemantics,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum DbLinkPolicy {
    AllowRemoteMetadata,
    #[default]
    RecordOpaqueRemoteObjects,
    RejectRemoteObjects,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum UnknownFeatureBehavior {
    #[default]
    RecordUnknown,
    TreatAsUnsupported,
    FailAnalysis,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum OracleVersion {
    Oracle11g,
    Oracle12c,
    #[default]
    Oracle19c,
    Oracle21c,
    Oracle23ai,
    Oracle26ai,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum OracleFeature {
    SqlBoolean23ai,
    PlsqlVector23ai,
    BinaryVector26ai,
    SparseVector26ai,
    VectorArithmetic26ai,
    PackageResettable26ai,
    JsonRelationalDuality23ai,
    SqlMacros,
    PolymorphicTableFunctions,
    MultilingualEngineCallSpecs,
}

impl OracleVersion {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn default_features(self) -> BTreeSet<OracleFeature> {
        let features = match self {
            Self::Oracle11g | Self::Oracle12c | Self::Oracle19c => Vec::new(),
            Self::Oracle21c => vec![
                OracleFeature::SqlMacros,
                OracleFeature::PolymorphicTableFunctions,
            ],
            Self::Oracle23ai => vec![
                OracleFeature::SqlBoolean23ai,
                OracleFeature::PlsqlVector23ai,
                OracleFeature::JsonRelationalDuality23ai,
                OracleFeature::SqlMacros,
                OracleFeature::PolymorphicTableFunctions,
            ],
            Self::Oracle26ai => vec![
                OracleFeature::SqlBoolean23ai,
                OracleFeature::PlsqlVector23ai,
                OracleFeature::BinaryVector26ai,
                OracleFeature::SparseVector26ai,
                OracleFeature::VectorArithmetic26ai,
                OracleFeature::PackageResettable26ai,
                OracleFeature::JsonRelationalDuality23ai,
                OracleFeature::SqlMacros,
                OracleFeature::PolymorphicTableFunctions,
                OracleFeature::MultilingualEngineCallSpecs,
            ],
        };

        features.into_iter().collect()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FeaturePolicy {
    pub enabled: BTreeSet<OracleFeature>,
    pub disabled: BTreeSet<OracleFeature>,
    pub unknown_feature_behavior: UnknownFeatureBehavior,
}

impl FeaturePolicy {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn from_version(version: OracleVersion) -> Self {
        Self {
            enabled: version.default_features(),
            disabled: BTreeSet::new(),
            unknown_feature_behavior: UnknownFeatureBehavior::RecordUnknown,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_enabled(&self, feature: OracleFeature) -> bool {
        !self.disabled.contains(&feature) && self.enabled.contains(&feature)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_enabled(mut self, feature: OracleFeature) -> Self {
        self.disabled.remove(&feature);
        self.enabled.insert(feature);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_disabled(mut self, feature: OracleFeature) -> Self {
        self.enabled.remove(&feature);
        self.disabled.insert(feature);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisProfile {
    pub oracle_version: OracleVersion,
    pub compatibility: Option<OracleVersion>,
    pub feature_policy: FeaturePolicy,
    pub current_schema: Option<SchemaName>,
    pub current_user: Option<UserName>,
    pub current_edition: Option<EditionName>,
    pub plsql_ccflags: HashMap<String, LiteralValue>,
    pub nls: NlsSettings,
    pub enabled_roles: Vec<RoleName>,
    pub db_link_policy: DbLinkPolicy,
}

impl Default for AnalysisProfile {
    fn default() -> Self {
        Self::for_version(OracleVersion::Oracle19c)
    }
}

impl AnalysisProfile {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn for_version(oracle_version: OracleVersion) -> Self {
        Self {
            oracle_version,
            compatibility: None,
            feature_policy: FeaturePolicy::from_version(oracle_version),
            current_schema: None,
            current_user: None,
            current_edition: None,
            plsql_ccflags: HashMap::new(),
            nls: NlsSettings::default(),
            enabled_roles: Vec::new(),
            db_link_policy: DbLinkPolicy::default(),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn supports_feature(&self, feature: OracleFeature) -> bool {
        self.feature_policy.is_enabled(feature)
    }
}

/// An honest count that distinguishes "measured and found zero" from
/// "never measured" (§1.5 Evidence-UX honesty).
///
/// A bare `0` on a metric whose pipeline stage is not yet wired is a
/// false-clean lie: a reader cannot tell "we looked and found none"
/// from "we never looked". Every gap metric that is structurally
/// not-yet-computed serialises as `{ "unmeasured": true }` instead of
/// `0`, so a consumer can never mistake an un-run analysis for a
/// clean one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Measured<T> {
    /// The producing analysis stage ran and established this value.
    Measured(T),
    /// The producing analysis stage is not yet wired; the true value
    /// is unknown. NEVER treat this as zero.
    #[default]
    Unmeasured,
}

impl<T> Measured<T> {
    /// `Some` only when the value was actually measured.
    pub fn measured(self) -> Option<T> {
        match self {
            Self::Measured(v) => Some(v),
            Self::Unmeasured => None,
        }
    }

    #[must_use]
    pub fn is_measured(&self) -> bool {
        matches!(self, Self::Measured(_))
    }
}

// `untagged` needs `Unmeasured` to round-trip; model it as the JSON
// object `{ "unmeasured": true }` rather than `null` (which would be
// ambiguous with a measured `Option`-shaped value).
mod measured_serde {
    use super::Measured;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct UnmeasuredMarker {
        unmeasured: bool,
    }

    impl<T: Serialize> Serialize for Measured<T> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            match self {
                Measured::Measured(v) => v.serialize(s),
                Measured::Unmeasured => UnmeasuredMarker { unmeasured: true }.serialize(s),
            }
        }
    }

    impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Measured<T> {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let value = serde_json::Value::deserialize(d)?;
            if let Ok(m) = serde_json::from_value::<UnmeasuredMarker>(value.clone()) {
                if m.unmeasured {
                    return Ok(Measured::Unmeasured);
                }
            }
            let v = serde_json::from_value::<T>(value).map_err(serde::de::Error::custom)?;
            Ok(Measured::Measured(v))
        }
    }
}

/// The overall epistemic posture of an analysis run (§1.5, §22).
///
/// This is the headline a consumer reads first. It is derived from
/// the honest signals — it is NEVER `Clean` when the run understood
/// little (high unrecognised-object ratio or large diagnostic
/// volume). Honesty cuts both ways: a genuinely clean run over clean
/// input still reads `Clean`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompletenessPosture {
    /// Parsed and semantically lowered with no material gaps.
    Clean,
    /// Some recovery / catalog gaps, but the bulk was understood.
    Partial,
    /// A large share of objects were not understood, or the
    /// diagnostic volume is high. Downstream results are NOT
    /// trustworthy as a complete picture.
    LowConfidence,
    /// The run could not establish a meaningful model at all.
    #[default]
    Degraded,
}

impl std::fmt::Display for CompletenessPosture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Clean => "Clean",
            Self::Partial => "Partial",
            Self::LowConfidence => "LowConfidence",
            Self::Degraded => "Degraded",
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompletenessReport {
    pub files_total: usize,
    pub files_parsed_cleanly: usize,
    pub files_recovered: usize,
    pub skipped_token_ratio: f32,
    pub objects_total: usize,
    pub objects_with_source: usize,
    pub objects_catalog_only: usize,
    // --- structurally not-yet-wired gap metrics (honest Unmeasured) ---
    // These read as `{ "unmeasured": true }` until their analysis
    // stage is wired, so a reader can never mistake "not computed"
    // for "none found".
    pub wrapped_units: Measured<usize>,
    pub missing_package_bodies: Measured<usize>,
    pub dynamic_sql_sites: Measured<usize>,
    pub opaque_dynamic_sql_sites: Measured<usize>,
    pub db_link_edges: Measured<usize>,
    pub unresolved_references: Measured<usize>,
    // --- honest extraction signals (always populated) ---
    /// Total diagnostics emitted across the whole run. A large
    /// number here means the run is NOT clean even if the file
    /// counts look healthy.
    pub diagnostics_total: usize,
    /// Top-level objects the AST classifier could not lower
    /// (`IR_UNCLASSIFIED_DECL`). These contributed NOTHING to the
    /// semantic model — they are unknown, not clean.
    pub objects_unrecognized: usize,
    /// Objects for which real semantics were extracted (lowered).
    pub objects_with_extracted_semantics: usize,
    /// `objects_with_extracted_semantics / (lowered + unrecognized)`,
    /// in `[0.0, 1.0]`. Low ratio ⇒ the run understood little.
    pub extracted_semantics_ratio: f32,
    /// Derived headline. NEVER `Clean` on a low-extraction run.
    pub posture: CompletenessPosture,
    pub catalog_available: bool,
    pub plscope_available: bool,
}

impl CompletenessReport {
    /// Populate the derived honest signals (`extracted_semantics_ratio`
    /// and `posture`) from the raw counts. Call this after setting
    /// `objects_with_extracted_semantics`, `objects_unrecognized` and
    /// `diagnostics_total`.
    ///
    /// Posture rules (anti-spin — a low-extraction run MUST look
    /// exactly as uncertain as it truly is):
    /// * `Degraded`   — nothing meaningful established (no objects, or
    ///   ratio ≈ 0 with work attempted).
    /// * `LowConfidence` — a material share of objects unrecognised
    ///   (ratio < 0.85) OR the diagnostic volume rivals the object
    ///   count (a "we barely understood this" signal).
    /// * `Partial`    — mostly understood but with recovery/gap noise.
    /// * `Clean`      — fully lowered, no unrecognised objects, low
    ///   diagnostic noise, every file parsed cleanly.
    pub fn finalize_posture(&mut self) {
        let denom = self
            .objects_with_extracted_semantics
            .saturating_add(self.objects_unrecognized);
        self.extracted_semantics_ratio = if denom == 0 {
            // No top-level objects at all: a genuinely empty tree is
            // not "clean extraction"; treat ratio as 1.0 only when
            // there were truly no objects AND no diagnostics.
            if self.diagnostics_total == 0 {
                1.0
            } else {
                0.0
            }
        } else {
            self.objects_with_extracted_semantics as f32 / denom as f32
        };

        // Diagnostic pressure relative to attempted objects: when the
        // run emitted roughly as many (or more) diagnostics as it has
        // objects, it did not "cleanly parse" anything in a meaningful
        // sense regardless of the file tallies.
        let attempted = denom.max(self.objects_total);
        let high_diag_pressure = attempted > 0 && self.diagnostics_total * 2 >= attempted;

        self.posture = if denom == 0 && self.objects_total == 0 {
            // No top-level objects at all. An empty tree, or a tree
            // with files but zero diagnostics, is genuinely Clean;
            // files present *with* diagnostics is Degraded (nothing
            // understood, noise emitted).
            if self.files_total == 0 || self.diagnostics_total == 0 {
                CompletenessPosture::Clean
            } else {
                CompletenessPosture::Degraded
            }
        } else if self.extracted_semantics_ratio < 0.10 {
            CompletenessPosture::Degraded
        } else if self.objects_unrecognized > 0
            || self.extracted_semantics_ratio < 0.85
            || high_diag_pressure
        {
            CompletenessPosture::LowConfidence
        } else if self.files_recovered > 0
            || self.diagnostics_total > 0
            || self.files_parsed_cleanly < self.files_total
        {
            CompletenessPosture::Partial
        } else {
            CompletenessPosture::Clean
        };
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolInterner {
    symbols: Vec<String>,
    index: HashMap<String, SymbolId>,
}

impl SymbolInterner {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern(&mut self, text: impl Into<String>) -> Option<SymbolId> {
        let text = text.into();
        if let Some(&symbol_id) = self.index.get(text.as_str()) {
            return Some(symbol_id);
        }

        let next_index = u64::try_from(self.symbols.len()).ok()?;
        let symbol_id = SymbolId::new(next_index);
        self.symbols.push(text.clone());
        self.index.insert(text, symbol_id);
        Some(symbol_id)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn resolve(&self, symbol_id: SymbolId) -> Option<&str> {
        let index = usize::try_from(symbol_id.get()).ok()?;
        self.symbols.get(index).map(String::as_str)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn contains(&self, text: impl AsRef<str>) -> bool {
        self.index.contains_key(text.as_ref())
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_schema_name(&mut self, text: impl Into<String>) -> Option<SchemaName> {
        self.intern(text).map(SchemaName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_user_name(&mut self, text: impl Into<String>) -> Option<UserName> {
        self.intern(text).map(UserName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_role_name(&mut self, text: impl Into<String>) -> Option<RoleName> {
        self.intern(text).map(RoleName::from)
    }
}

impl Serialize for SymbolInterner {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.symbols.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SymbolInterner {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let symbols = Vec::<String>::deserialize(deserializer)?;
        let mut interner = SymbolInterner::default();
        for symbol in symbols {
            interner
                .intern(symbol)
                .ok_or_else(|| serde::de::Error::custom("symbol table overflow"))?;
        }
        Ok(interner)
    }
}

impl std::fmt::Display for NlsLengthSemantics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Byte => f.write_str("Byte"),
            Self::Char => f.write_str("Char"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnalysisProfile, ColumnName, CompletenessPosture, CompletenessReport, Confidence,
        ConfidenceLevel, DbLinkPolicy, Diagnostic, EditionName, Evidence, FeaturePolicy, FileId,
        JsonExportable, LiteralValue, Measured, NlsSettings, ObjectName, OracleFeature,
        OracleVersion, Position, RobotJson, RoleName, SchemaName, Severity, SourceSpan, Span,
        SymbolId, SymbolInterner, UnknownFeatureBehavior, UnknownReason, UserName,
    };
    use serde_json::Value;

    #[test]
    fn span_len_uses_offsets() {
        let span = Span::new(
            FileId::new(7),
            Position::new(2, 4, 10),
            Position::new(2, 9, 21),
        );

        assert_eq!(span.len(), 11);
        assert!(!span.is_empty());
        assert_eq!(span.source_span(), SourceSpan::from((10usize, 11usize)));
    }

    #[test]
    fn evidence_builder_retains_attributes() {
        let evidence = Evidence::new("SYM001", "resolved via same-schema lookup")
            .with_note("package body available")
            .with_attribute("strategy", Value::String(String::from("same-schema")))
            .with_confidence(Confidence::new(
                ConfidenceLevel::High,
                Some(String::from("catalog snapshot and source agree")),
            ));

        assert_eq!(evidence.code, "SYM001");
        assert_eq!(evidence.notes, [String::from("package body available")]);
        assert_eq!(
            evidence.attributes.get("strategy"),
            Some(&Value::String(String::from("same-schema")))
        );
        assert_eq!(
            evidence.confidence,
            Some(Confidence::new(
                ConfidenceLevel::High,
                Some(String::from("catalog snapshot and source agree")),
            ))
        );
    }

    #[test]
    fn diagnostic_builder_captures_unknowns() {
        let span = Span::new(
            FileId::new(1),
            Position::new(4, 1, 20),
            Position::new(4, 14, 33),
        );
        let diagnostic = Diagnostic::new(
            "PARSE001",
            Severity::Warn,
            "parser recovered after unsupported token",
        )
        .with_primary_span(span)
        .with_unknown_reason(UnknownReason::ParserRecoveryRegion)
        .with_help("review the recovered region before trusting downstream analysis");

        assert_eq!(diagnostic.primary_span, Some(span));
        assert_eq!(
            diagnostic.unknown_reasons,
            vec![UnknownReason::ParserRecoveryRegion]
        );
        assert_eq!(
            diagnostic.help,
            Some(String::from(
                "review the recovered region before trusting downstream analysis"
            ))
        );
    }

    #[test]
    fn symbol_interner_deduplicates_and_resolves_names() {
        let mut interner = SymbolInterner::new();
        let first = interner.intern("claims_pkg");
        let second = interner.intern("claims_pkg");
        let schema = interner.intern_schema_name("billing");
        let role = interner.intern_role_name("app_reader");

        assert_eq!(first, second);
        assert_eq!(interner.len(), 3);
        assert_eq!(
            first.and_then(|symbol_id| interner.resolve(symbol_id)),
            Some("claims_pkg")
        );
        assert_eq!(schema.map(SchemaName::symbol), interner.intern("billing"));
        assert_eq!(role.map(RoleName::symbol), interner.intern("app_reader"));
    }

    #[test]
    fn analysis_profile_uses_version_feature_defaults() {
        let base = AnalysisProfile::default();
        let modern = AnalysisProfile::for_version(OracleVersion::Oracle26ai);

        assert_eq!(base.oracle_version, OracleVersion::Oracle19c);
        assert!(!base.supports_feature(OracleFeature::SqlBoolean23ai));
        assert!(modern.supports_feature(OracleFeature::PackageResettable26ai));
        assert!(modern.supports_feature(OracleFeature::MultilingualEngineCallSpecs));
    }

    #[test]
    fn feature_policy_supports_explicit_overrides() {
        let policy = FeaturePolicy::from_version(OracleVersion::Oracle19c)
            .with_enabled(OracleFeature::SqlBoolean23ai)
            .with_disabled(OracleFeature::SqlBoolean23ai);

        assert!(!policy.is_enabled(OracleFeature::SqlBoolean23ai));
        assert_eq!(
            policy.unknown_feature_behavior,
            UnknownFeatureBehavior::RecordUnknown
        );
    }

    #[test]
    fn robot_json_round_trips_json_exportable_payloads() {
        let report = CompletenessReport {
            files_total: 8,
            files_parsed_cleanly: 7,
            files_recovered: 1,
            ..CompletenessReport::default()
        };
        let wrapped = RobotJson::new(report);
        let value = wrapped.to_json_value().expect("wrapper should serialize");
        let parsed = RobotJson::<CompletenessReport>::from_json_value(value)
            .expect("wrapper should deserialize");

        assert_eq!(parsed.payload.files_total, 8);
        assert_eq!(parsed.payload.files_recovered, 1);
    }

    #[test]
    fn names_and_policy_types_have_stable_defaults() {
        let schema = SchemaName::from(SymbolId::new(3));
        let user = UserName::from(SymbolId::new(4));
        let edition = EditionName::from(SymbolId::new(5));
        let object = ObjectName::from(SymbolId::new(6));
        let column = ColumnName::from(SymbolId::new(7));

        assert_eq!(schema.symbol().get(), 3);
        assert_eq!(user.symbol().get(), 4);
        assert_eq!(edition.symbol().get(), 5);
        assert_eq!(object.symbol().get(), 6);
        assert_eq!(column.symbol().get(), 7);
        assert_eq!(
            DbLinkPolicy::default(),
            DbLinkPolicy::RecordOpaqueRemoteObjects
        );
        assert_eq!(NlsSettings::default().length_semantics.to_string(), "Byte");
        assert_eq!(LiteralValue::Boolean(true), LiteralValue::Boolean(true));
    }

    #[test]
    fn default_features_are_monotonic_across_versions() {
        // Oracle does not remove PL/SQL language features in newer
        // releases: every feature available at version N must remain
        // available at all later versions. The per-version lists in
        // `default_features` are hand-maintained duplicated vecs, so
        // a "added to 23ai, forgot 26ai" edit would silently DISABLE
        // a feature for newer Oracle (wrong dialect gating → false
        // parse errors / missed SAST). This locks the invariant for
        // every current and future feature addition.
        let ordered = [
            OracleVersion::Oracle11g,
            OracleVersion::Oracle12c,
            OracleVersion::Oracle19c,
            OracleVersion::Oracle21c,
            OracleVersion::Oracle23ai,
            OracleVersion::Oracle26ai,
        ];
        for pair in ordered.windows(2) {
            let [older, newer] = [pair[0], pair[1]];
            let older_f = older.default_features();
            let newer_f = newer.default_features();
            assert!(
                older_f.is_subset(&newer_f),
                "{older:?} features must remain available in {newer:?}; missing: {:?}",
                older_f.difference(&newer_f).collect::<Vec<_>>()
            );
        }
    }

    // --- oracle-bh4p / Phase 2: honest CompletenessReport ----------

    #[test]
    fn low_extraction_run_is_never_clean() {
        // Mirrors a real private-estate shape: file tallies look
        // pristine, but thousands of objects were never lowered and
        // the diagnostic volume is huge. This MUST NOT read as clean.
        let mut r = CompletenessReport {
            files_total: 4251,
            files_parsed_cleanly: 4224,
            files_recovered: 27,
            objects_total: 4123,
            objects_with_source: 4123,
            objects_with_extracted_semantics: 4123,
            objects_unrecognized: 6609,
            diagnostics_total: 6784,
            ..CompletenessReport::default()
        };
        r.finalize_posture();

        assert_ne!(
            r.posture,
            CompletenessPosture::Clean,
            "a run that failed to recognise 6609 objects must not present as Clean"
        );
        assert_eq!(r.objects_unrecognized, 6609);
        assert_eq!(r.diagnostics_total, 6784);
        assert!(
            r.extracted_semantics_ratio < 0.85,
            "ratio {} should reflect heavy non-extraction",
            r.extracted_semantics_ratio
        );
        // The structurally-unwired gap metrics must NOT read as 0.
        assert_eq!(r.dynamic_sql_sites, Measured::Unmeasured);
        assert_eq!(r.unresolved_references, Measured::Unmeasured);
        assert!(!r.dynamic_sql_sites.is_measured());
    }

    #[test]
    fn clean_input_still_reads_clean() {
        // Honesty cuts both ways: a genuinely clean run over clean
        // input still reads healthy.
        let mut r = CompletenessReport {
            files_total: 12,
            files_parsed_cleanly: 12,
            files_recovered: 0,
            objects_total: 30,
            objects_with_source: 30,
            objects_with_extracted_semantics: 30,
            objects_unrecognized: 0,
            diagnostics_total: 0,
            ..CompletenessReport::default()
        };
        r.finalize_posture();
        assert_eq!(r.posture, CompletenessPosture::Clean);
        assert!((r.extracted_semantics_ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unmeasured_gap_metric_serializes_honestly_not_as_zero() {
        let r = CompletenessReport::default();
        let v = serde_json::to_value(&r).expect("serializes");
        // A not-yet-wired metric must NOT serialize as a misleading 0.
        assert_eq!(
            v["dynamic_sql_sites"],
            serde_json::json!({"unmeasured": true})
        );
        assert_ne!(v["dynamic_sql_sites"], serde_json::json!(0));
        // Round-trips back to Unmeasured.
        let back: CompletenessReport = serde_json::from_value(v).expect("round-trips");
        assert_eq!(back.dynamic_sql_sites, Measured::Unmeasured);

        // A measured value serializes as the bare number.
        let r2 = CompletenessReport {
            dynamic_sql_sites: Measured::Measured(7),
            ..CompletenessReport::default()
        };
        let v2 = serde_json::to_value(&r2).expect("serializes");
        assert_eq!(v2["dynamic_sql_sites"], serde_json::json!(7));
        let back2: CompletenessReport = serde_json::from_value(v2).expect("round-trips");
        assert_eq!(back2.dynamic_sql_sites, Measured::Measured(7));
    }

    #[test]
    fn degraded_when_nothing_understood() {
        let mut r = CompletenessReport {
            files_total: 100,
            files_parsed_cleanly: 0,
            objects_total: 500,
            objects_unrecognized: 500,
            objects_with_extracted_semantics: 0,
            diagnostics_total: 500,
            ..CompletenessReport::default()
        };
        r.finalize_posture();
        assert_eq!(r.posture, CompletenessPosture::Degraded);
    }
}
