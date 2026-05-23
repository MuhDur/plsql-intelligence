//! `GapRecord` — the provenance-only artifact emitted by stage [A]
//! of the USR loop (spec §2.1).
//!
//! A `GapRecord` records *that* the engine was honestly uncertain
//! and *enough structure to cluster and minimize the gap later* —
//! and **nothing else**. It carries no source text, no identifier,
//! no literal. Two invariants are hard gates from line one:
//!
//! * **I-PRIVACY** — no customer or private-estate byte may appear in any
//!   serialized field. `span_shape` is a sequence of token-*KIND*
//!   class markers, never source text. (`AnalysisRun` itself carries
//!   zero source — see [`crate::capture`] for why P1's shape is
//!   span-*geometry* derived; real lexer `TokenKind` shapes arrive
//!   in P2 over re-synthesized MinFixtures we own.)
//! * **I-DETERMINISM** — every persisted field is derived purely
//!   from content. No wall-clock, no RNG, no map-iteration order.
//!   Serialization is sorted-key (`BTreeMap`/sorted `Vec`).

use std::collections::BTreeMap;

use plsql_core::Diagnostic;
use plsql_engine::AnalysisRun;
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::tokscrub::token_kind_shape;

/// Versioned robot-JSON schema for a batch of [`GapRecord`]s
/// (`plsql.usr.gap_record` v1). Mirrors the
/// [`plsql_output::SchemaDescriptor`] pattern used by every other
/// envelope in the workspace (e.g. `plsql.engine.analysis_run`).
pub const GAP_RECORD_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.gap_record",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR-loop GapRecord batch — provenance only, no source bytes (PLSQL-USR-001)",
};

/// The diagnostic codes stage [A] treats as *repairable* (spec §2).
pub const REPAIRABLE_CODES: [&str; 3] = [
    "PARSE-ANTLR4RUST-001",
    "IR_UNCLASSIFIED_DECL",
    "IR_DDL_NOT_LOWERED",
];

/// Which repair lane a gap is heuristically routed to (spec §2.1).
///
/// P1 classifies purely from the diagnostic code / typed
/// `UnknownReason` — the actual repair (grammar/lowering/typed
/// degradation) is proposed and proven in later phases. The
/// serialized form uses the spec's single-letter tags
/// (`g`|`l`|`d`|`unrepairable`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RepairClass {
    /// Grammar `.g4` gap — `PARSE-ANTLR4RUST-001`.
    #[serde(rename = "g")]
    Grammar,
    /// Lowering/dispatch gap — `IR_UNCLASSIFIED_DECL` /
    /// `IR_DDL_NOT_LOWERED`.
    #[serde(rename = "l")]
    Lowering,
    /// Typed-degradation gap — a diagnostic carrying a typed
    /// `UnknownReason` (honest "recognised, not deep-parsed").
    #[serde(rename = "d")]
    TypedDegradation,
    /// No heuristic lane applies yet.
    #[serde(rename = "unrepairable")]
    Unrepairable,
}

impl RepairClass {
    /// Heuristic P1 classifier (spec §2.1). A typed `UnknownReason`
    /// dominates (it is the strongest honest-uncertainty signal),
    /// then the structural code, else `Unrepairable`.
    #[must_use]
    #[instrument(level = "trace", skip(diag))]
    pub fn classify(diag: &Diagnostic) -> Self {
        if !diag.unknown_reasons.is_empty() {
            return RepairClass::TypedDegradation;
        }
        match diag.code.as_str() {
            "PARSE-ANTLR4RUST-001" => RepairClass::Grammar,
            "IR_UNCLASSIFIED_DECL" | "IR_DDL_NOT_LOWERED" => RepairClass::Lowering,
            _ => RepairClass::Unrepairable,
        }
    }
}

/// One captured honest-uncertainty gap (spec §2.1). Every field is
/// derived; none contains source. `Ord` so a batch can be sorted
/// deterministically before serialization (I-DETERMINISM).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GapRecord {
    /// sha256 content hash of `diag_code` + `antlr_rule_path` +
    /// the **token-KIND shape** (spec §2[C]/§2.1):
    /// `sha256(diag_code, antlr_rule_path, token_kind_shape)`. The
    /// shape is the real ANTLR lexer's `TokenKind` sequence over the
    /// canonical grammar skeleton implied by `antlr_rule_path` — it
    /// carries **no span width, line count, or byte offset** (P1's
    /// span-width-bucket stopgap is removed). Stable across (a)
    /// different occurrences of the same gap class and (b) ddmin
    /// minimisation of the surrounding estate block — that stability
    /// is the entire point: a true gap-class identifier, not a
    /// block-size fingerprint.
    pub signature: String,
    /// The diagnostic code (e.g. `PARSE-ANTLR4RUST-001`).
    pub diag_code: String,
    /// The ANTLR rule the parser was in, where the diagnostic
    /// carries it. `None` until the parser stamps it (honest gap —
    /// see crate docs / capture).
    pub antlr_rule_path: Option<String>,
    /// The typed `UnknownReason` *variant name* (never its data),
    /// or `None`.
    pub unknown_reason: Option<String>,
    /// Token-KIND class markers for the diagnostic span — never
    /// source text, identifiers, or literals.
    pub span_shape: Vec<String>,
    /// sha256 of the `AnalysisRun` (provenance of the run, not the
    /// estate). Same run → same id.
    pub estate_run_id: String,
    /// Occurrences folded into this record. Always `1` in P1
    /// (clustering is P3).
    pub occurrence_count: u64,
    /// Git HEAD short sha at capture (provenance, not wall-clock).
    pub first_seen_commit: String,
    /// Content hash of the synthetic MinFixture. `None` in P1
    /// (P2 fills it).
    pub min_fixture_id: Option<String>,
    /// Heuristic repair lane.
    pub repair_class: RepairClass,
    /// Redaction-delta manifest hash. `None` in P1 (P2 fills it).
    pub privacy_proof_id: Option<String>,
}

/// Lowercase hex sha256 of `bytes`.
#[must_use]
#[instrument(level = "trace", skip(bytes))]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Content hash of an [`AnalysisRun`] — the `estate_run_id`. Folds
/// only deterministic, source-free provenance (the run id, profile,
/// backend, file count, and the sorted diagnostic codes), so the
/// same run always yields the same id and **no source** can leak in
/// (I-PRIVACY + I-DETERMINISM).
#[must_use]
#[instrument(level = "trace", skip(run))]
pub fn estate_run_id(run: &AnalysisRun) -> String {
    let mut codes: Vec<&str> = run.diagnostics.iter().map(|d| d.code.as_str()).collect();
    codes.sort_unstable();
    let mut buf = String::new();
    buf.push_str("run_id=");
    buf.push_str(&run.run_id.get().to_string());
    buf.push_str(";backend=");
    buf.push_str(&run.parser_backend);
    buf.push_str(";files=");
    buf.push_str(&run.project.file_count.to_string());
    buf.push_str(";codes=");
    buf.push_str(&codes.join(","));
    sha256_hex(buf.as_bytes())
}

/// The canonical grammar-keyword **skeleton** implied by an
/// `antlr_rule_path` leaf — the construct identity, expressed only in
/// SQL grammar-keyword *constants* (never estate data).
///
/// The parser stamps `antlr_rule_path` as a `>`-joined path of
/// grammar rule names (e.g. `text_scan>create_table`,
/// `unit_statement>create_synonym`, `text_scan>drop`,
/// `text_scan>comment`). Its **leaf** is itself a grammar constant:
/// a verb (`create`/`alter`/`drop`/`comment`/…) optionally followed
/// by an object keyword (`table`/`index`/`synonym`/…). We map each
/// underscore-joined component to its SQL keyword and join with a
/// space, yielding a parseable canonical skeleton like `CREATE TABLE`
/// / `DROP` / `COMMENT`. Every byte of the output is a fixed grammar
/// keyword — **zero estate text** (I-PRIVACY) — and the value is a
/// pure function of `antlr_rule_path` (I-DETERMINISM), so it is
/// invariant under ddmin of the surrounding estate block.
#[must_use]
#[instrument(level = "trace")]
fn rule_path_skeleton(rule_path: Option<&str>) -> Option<String> {
    let path = rule_path?;
    // The leaf carries the construct (everything before `>` is the
    // grammar nest — `text_scan`/`unit_statement` — which is itself a
    // grammar constant but not a SQL skeleton token).
    let leaf = path.rsplit('>').next().unwrap_or(path);
    if leaf.is_empty() {
        return None;
    }
    // `create_table` → ["create","table"], `materialized_view` is a
    // single allowlisted object keyword already (see
    // `plsql_parser_antlr::lower`), `drop` → ["drop"]. Each component
    // is a lowercased grammar keyword constant; upper-case it so the
    // real ANTLR lexer lexes it as a `Keyword` (its canonical form).
    let skeleton: Vec<String> = leaf
        .split('_')
        .filter(|c| !c.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    if skeleton.is_empty() {
        None
    } else {
        Some(skeleton.join(" "))
    }
}

/// Derive the spec §2.1 `span_shape` — a **token-KIND sequence,
/// never text**.
///
/// **Spec-conformance (the P1 stopgap correction).** P1's original
/// `span_shape` folded a span *width bucket* (`W512`, …) and a line
/// bucket into the value — a documented stopgap because P1 had no
/// lexer in dependency reach. That made the shape (hence the
/// signature) a *block-size fingerprint*: ddmin narrowing a
/// `text_scan>*` block flipped the width bucket, changed the
/// signature, and the (correctly unchanged) `SignatureOracle`
/// rejected the minimised form — the bulk of private-estate gaps were
/// unminimisable *by construction*.
///
/// The corrected shape is exactly §2.1: the real ANTLR lexer's
/// `TokenKind` sequence (via the same `Antlr4RustBackend` the privacy
/// scrub uses, [`crate::tokscrub::token_kind_shape`]) over the
/// **canonical construct skeleton** implied by the gap's
/// `antlr_rule_path` ([`rule_path_skeleton`]). It contains only
/// grammar-constant `TokenKind` names (`KW`, `ID`, …) — no token
/// text, **no width, no line count, no offset** — so it is
/// I-PRIVACY-safe (kinds are grammar constants), I-DETERMINISM-safe
/// (pure function of the rule path via a deterministic lexer), and
/// **invariant under ddmin** of the surrounding estate.
///
/// When the diagnostic carries no `antlr_rule_path` (the honest
/// no-parse-tree case — e.g. the raw `PARSE-ANTLR4RUST-001` class)
/// there is no construct skeleton to lex; the shape is the fixed
/// `RULE_ABSENT` kind marker so those gaps still cluster
/// deterministically by `(code, "", RULE_ABSENT)` — never a
/// fabricated or width-derived shape.
#[must_use]
#[instrument(level = "trace")]
fn span_shape_of(rule_path: Option<&str>) -> Vec<String> {
    let Some(skeleton) = rule_path_skeleton(rule_path) else {
        return vec!["RULE_ABSENT".to_string()];
    };
    match token_kind_shape(&skeleton) {
        Some(kinds) if !kinds.is_empty() => kinds,
        // The skeleton is grammar keywords by construction, so the
        // lexer always produces tokens; the empty branch is an
        // honest, deterministic fallback (never width-derived).
        _ => vec!["RULE_ABSENT".to_string()],
    }
}

/// Best-effort `antlr_rule_path` from the diagnostic's structured
/// evidence. The parser does not stamp a rule path on diagnostics
/// today (honest gap, reported in P1); when an evidence attribute
/// keyed `antlr_rule_path` appears it is surfaced, else `None`.
#[must_use]
#[instrument(level = "trace", skip(diag))]
fn antlr_rule_path_of(diag: &Diagnostic) -> Option<String> {
    for ev in &diag.evidence {
        if let Some(v) = ev.attributes.get("antlr_rule_path") {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

impl GapRecord {
    /// Build a single `GapRecord` from a qualifying diagnostic.
    ///
    /// `run_id` and `commit` are passed in (not read here) so the
    /// function stays a pure, testable, deterministic projection.
    #[must_use]
    #[instrument(level = "trace", skip(diag))]
    pub fn from_diagnostic(diag: &Diagnostic, run_id: &str, commit: &str) -> Self {
        let antlr_rule_path = antlr_rule_path_of(diag);
        // Spec §2.1 token-KIND shape — derived from the construct's
        // grammar position (`antlr_rule_path`), NOT the span geometry.
        // No width / line / offset participates (P1 stopgap removed),
        // so the shape is invariant under ddmin of the surrounding
        // estate block.
        let span_shape = span_shape_of(antlr_rule_path.as_deref());
        // First typed UnknownReason variant *name* only — never its
        // payload (the enum is fieldless, but we still take the
        // Debug discriminant, not any source-derived data).
        let unknown_reason = diag.unknown_reasons.first().map(|r| format!("{r:?}"));

        let mut sig_input = String::new();
        sig_input.push_str("code=");
        sig_input.push_str(&diag.code);
        sig_input.push_str(";rule=");
        sig_input.push_str(antlr_rule_path.as_deref().unwrap_or(""));
        sig_input.push_str(";shape=");
        sig_input.push_str(&span_shape.join(","));
        let signature = sha256_hex(sig_input.as_bytes());

        Self {
            signature,
            diag_code: diag.code.clone(),
            antlr_rule_path,
            unknown_reason,
            span_shape,
            estate_run_id: run_id.to_string(),
            occurrence_count: 1,
            first_seen_commit: commit.to_string(),
            min_fixture_id: None,
            repair_class: RepairClass::classify(diag),
            privacy_proof_id: None,
        }
    }
}

/// A versioned, sorted-key envelope wrapping a batch of
/// `GapRecord`s. `BTreeMap` is *not* needed at the top level (the
/// envelope is a fixed struct) but the payload `Vec` is sorted
/// before wrapping so two captures of the same run serialize
/// byte-identically (I-DETERMINISM).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GapRecordEnvelope {
    #[serde(flatten)]
    pub envelope: RobotJsonEnvelope<Vec<GapRecord>>,
}

impl GapRecordEnvelope {
    /// Wrap a batch, sorting it canonically first (I-DETERMINISM).
    #[must_use]
    #[instrument(level = "trace", skip(records))]
    pub fn new(mut records: Vec<GapRecord>) -> Self {
        records.sort();
        Self {
            envelope: RobotJsonEnvelope::new(GAP_RECORD_SCHEMA, records),
        }
    }

    /// `true` iff this envelope carries the `plsql.usr.gap_record`
    /// v1 schema.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_gap_record_schema(&self) -> bool {
        self.envelope.matches_schema(GAP_RECORD_SCHEMA)
    }

    /// Canonical single-line robot-JSON (sorted keys via serde's
    /// struct field order + the pre-sorted payload).
    ///
    /// # Errors
    /// Propagates any `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_robot_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Pretty multi-line robot-JSON (human mode).
    ///
    /// # Errors
    /// Propagates any `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

/// A typed map alias kept for downstream phases that need a
/// content-addressed index of records by signature without
/// introducing `HashMap` iteration order (I-DETERMINISM).
pub type GapIndex = BTreeMap<String, GapRecord>;
