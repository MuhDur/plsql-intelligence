#![forbid(unsafe_code)]

use thiserror::Error;
use tracing::instrument;

pub mod config {
    use std::path::PathBuf;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub enum ParserBackendChoice {
        #[default]
        Antlr4Rust,
        JavaAntlrWorker,
        TreeSitterExperimental,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub enum CatalogSourceConfig {
        #[default]
        Disabled,
        Snapshot(PathBuf),
        LiveConnection,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum CacheMode {
        #[default]
        ImmutableArtifact,
        LocalDaemon,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct CacheConfig {
        pub enabled: bool,
        pub mode: CacheMode,
        pub directory: Option<PathBuf>,
        /// When `true`, the run is persisted to the cache in its
        /// **compact** form ([`AnalysisRun::compact`]) — heavy,
        /// re-derivable payloads (catalog snapshot, dependency graph)
        /// are dropped so long-lived caches stay small. Opt-in;
        /// `false` (default) persists the full run unchanged.
        pub compact_persisted: bool,
    }

    impl Default for CacheConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                mode: CacheMode::ImmutableArtifact,
                directory: None,
                compact_persisted: false,
            }
        }
    }
}

pub mod model {
    use plsql_catalog::CatalogSnapshot;
    use plsql_core::{AnalysisProfile, AnalysisRunId, CompletenessReport, Diagnostic};
    use plsql_depgraph::DepGraph;
    use plsql_output::{RedactionPolicy, SchemaVersion};

    use super::config::{CacheConfig, CatalogSourceConfig, ParserBackendChoice};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct AnalysisRequest {
        pub project_root: PathBuf,
        pub analysis_profile: AnalysisProfile,
        pub parser_backend: ParserBackendChoice,
        pub catalog_source: CatalogSourceConfig,
        pub cache: CacheConfig,
        pub redaction_policy: RedactionPolicy,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct ProjectModel {
        pub root: PathBuf,
        pub file_count: usize,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct ParseResult {
        pub file: PathBuf,
        pub recovered: bool,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct SemanticModel {
        pub declaration_count: usize,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct SqlSemanticModel {
        pub statement_count: usize,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct FlowSummary {
        pub taint_path_count: usize,
        pub string_shape_count: usize,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct FactStoreSnapshot {
        pub fact_count: usize,
        /// The minted facts. Embedded so downstream consumers (and
        /// the acceptance gate) can inspect the actual
        /// Declaration/DependencyEdge/Reference payloads, not just a
        /// count. `#[serde(default)]` keeps older cached snapshots
        /// (count-only) deserialisable (R13).
        #[serde(default)]
        pub facts: Vec<plsql_ir::fact::Fact>,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct AnalysisArtifactDigest {
        pub name: String,
        pub digest_hex: String,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct AnalysisArtifactManifest {
        pub schema_version: SchemaVersion,
        pub artifact_digests: Vec<AnalysisArtifactDigest>,
        pub redaction_policy: RedactionPolicy,
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    pub struct AnalysisRun {
        pub run_id: AnalysisRunId,
        pub profile: AnalysisProfile,
        /// Parser backend that produced `parse_results` (provenance
        /// for the doctor's backend block). Empty for a no-op run.
        pub parser_backend: String,
        /// Cache outcome: `None` ⇒ artifact caching was not active
        /// (no cache dir / disabled); `Some(true)` ⇒ this run was
        /// served from the content+profile-keyed plsql-store cache;
        /// `Some(false)` ⇒ cache active but missed (run computed
        /// + stored).
        #[serde(default)]
        pub cache_outcome: Option<bool>,
        pub project: ProjectModel,
        pub parse_results: Vec<ParseResult>,
        pub catalog: Option<CatalogSnapshot>,
        pub semantic_model: SemanticModel,
        pub sql_semantic: SqlSemanticModel,
        pub flow_summary: FlowSummary,
        pub fact_store: FactStoreSnapshot,
        pub dep_graph: DepGraph,
        pub completeness: CompletenessReport,
        pub diagnostics: Vec<Diagnostic>,
        pub artifacts: AnalysisArtifactManifest,
    }

    impl AnalysisRun {
        /// Return the **compact** persisted form of this run.
        ///
        /// Drops the two heavy, fully re-derivable payloads — the
        /// `CatalogSnapshot` (re-extractable from the catalog
        /// source) and the `DepGraph` (rebuildable from the
        /// analysed sources) — while preserving every cheap
        /// summary, count, completeness flag, diagnostic, the
        /// run id/profile, and the cache outcome. Used for
        /// long-lived caches where storing the full graph/catalog
        /// per run is wasteful (the consumer rebuilds them on
        /// demand). This is *lossy by design but reconstructable*
        /// — it never drops information that is not derivable
        /// from the inputs (R13: the drop is explicit and
        /// reversible, never a silent truncation of unique data).
        #[must_use]
        pub fn compact(&self) -> AnalysisRun {
            AnalysisRun {
                catalog: None,
                dep_graph: DepGraph::new(),
                ..self.clone()
            }
        }
    }
}

pub use config::{CacheConfig, CacheMode, CatalogSourceConfig, ParserBackendChoice};
pub use model::{
    AnalysisArtifactDigest, AnalysisArtifactManifest, AnalysisRequest, AnalysisRun,
    FactStoreSnapshot, FlowSummary, ParseResult, ProjectModel, SemanticModel, SqlSemanticModel,
};
pub use plsql_catalog::CatalogSnapshot;
pub use plsql_depgraph::DepGraph;

// ---------------------------------------------------------------------------
// Reusable AnalysisRun artifact (PLSQL-ENG-004)
// ---------------------------------------------------------------------------

/// Robot-JSON schema for the reusable [`AnalysisRun`] artifact
/// emitted by `plsql-engine analyze`. The engine `doctor` gate
/// (`plsql-engine doctor --run`) classifies a loaded artifact's
/// `(schema_id, schema_version)` through [`schema_compatibility`]
/// before trusting the payload: a same-id artifact whose minor
/// version is `>=` this build's is accepted (the additive-minor
/// forward-compat tolerance), and only a differing `schema_id` or
/// major version is rejected. The same [`schema_compatibility`] /
/// [`AnalysisArtifactManifest::is_readable_by`] helpers back any
/// downstream consumer (the SAST scan harness, MCP foundation
/// tools) that gates on the embedded manifest version.
pub const ANALYSIS_RUN_SCHEMA: plsql_output::SchemaDescriptor = plsql_output::SchemaDescriptor {
    id: "plsql.engine.analysis_run",
    // 1.1.0: additive honest-completeness signals (oracle-bh4p) —
    // posture / objects_unrecognized / diagnostics_total /
    // extracted_semantics_ratio added; structurally-unwired gap
    // metrics now serialise as `{ "unmeasured": true }` instead of
    // a misleading `0`.
    version: plsql_output::SchemaVersion::new(1, 1, 0),
    description: "Reusable canonical AnalysisRun artifact (PLSQL-ENG-004)",
};

/// Schema for the compact [`EngineDoctorReport`] envelope. Kept
/// distinct from [`ANALYSIS_RUN_SCHEMA`] so a consumer can tell a
/// doctor report apart from a full run artifact via
/// `matches_schema` — they are different payload shapes.
pub const ENGINE_DOCTOR_SCHEMA: plsql_output::SchemaDescriptor = plsql_output::SchemaDescriptor {
    id: "plsql.engine.doctor",
    version: plsql_output::SchemaVersion::new(1, 0, 0),
    description: "Compact engine doctor report (PLSQL-ENG-004)",
};

/// Schema for the [`EngineFullDoctorReport`] envelope (the
/// backend/catalog/cache/fact/graph/completeness block). Distinct
/// from both schemas above.
pub const ENGINE_FULL_DOCTOR_SCHEMA: plsql_output::SchemaDescriptor =
    plsql_output::SchemaDescriptor {
        id: "plsql.engine.doctor_full",
        version: plsql_output::SchemaVersion::new(1, 0, 0),
        description: "Full engine doctor report (PLSQL-ENG-005)",
    };

/// Schema for the [`MemoryProfile`] envelope. Distinct from the
/// other doctor schemas so a consumer can discriminate via
/// `matches_schema`.
pub const ENGINE_MEMORY_SCHEMA: plsql_output::SchemaDescriptor = plsql_output::SchemaDescriptor {
    id: "plsql.engine.memory_profile",
    version: plsql_output::SchemaVersion::new(1, 0, 0),
    description: "Engine memory/footprint profile (PLSQL-PERF-002)",
};

/// `plsql-engine doctor --memory` payload: the serialized footprint
/// of an [`AnalysisRun`], the footprint of its
/// [`compact`](AnalysisRun::compact) form, and the per-section
/// breakdown of the two heavy evictable payloads. Sizes are the
/// byte length of the canonical JSON serialization — deterministic
/// and reproducible (R10/R11), not a live RSS sample.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemoryProfile {
    pub schema_id: String,
    pub schema_version: plsql_output::SchemaVersion,
    /// Bytes of the full run's canonical JSON.
    pub full_bytes: usize,
    /// Bytes of the compact run's canonical JSON.
    pub compact_bytes: usize,
    /// `full_bytes - compact_bytes` (what compact persistence
    /// saves per cached run).
    pub savings_bytes: usize,
    /// `savings_bytes / full_bytes` in `[0,1]`; `0.0` when the
    /// run is empty.
    pub savings_ratio: f64,
    /// Approx bytes attributable to the catalog snapshot (the
    /// JSON length of just that field).
    pub catalog_bytes: usize,
    /// Approx bytes attributable to the dependency graph.
    pub dep_graph_bytes: usize,
    /// Approx bytes attributable to the parse-result list.
    pub parse_results_bytes: usize,
}

#[must_use]
fn json_len<T: serde::Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0)
}

#[must_use]
pub fn engine_memory_profile(run: &AnalysisRun) -> MemoryProfile {
    let full_bytes = json_len(run);
    let compact_bytes = json_len(&run.compact());
    let savings_bytes = full_bytes.saturating_sub(compact_bytes);
    MemoryProfile {
        schema_id: ENGINE_MEMORY_SCHEMA.id.to_string(),
        schema_version: ENGINE_MEMORY_SCHEMA.version,
        full_bytes,
        compact_bytes,
        savings_bytes,
        savings_ratio: if full_bytes == 0 {
            0.0
        } else {
            savings_bytes as f64 / full_bytes as f64
        },
        catalog_bytes: json_len(&run.catalog),
        dep_graph_bytes: json_len(&run.dep_graph),
        parse_results_bytes: json_len(&run.parse_results),
    }
}

#[must_use]
pub fn engine_memory_profile_envelope(
    profile: MemoryProfile,
) -> plsql_output::RobotJsonEnvelope<MemoryProfile> {
    plsql_output::RobotJsonEnvelope::new(ENGINE_MEMORY_SCHEMA, profile)
}

/// Wrap an [`AnalysisRun`] in the shared versioned robot-JSON
/// envelope so it round-trips through every downstream consumer.
#[must_use]
pub fn analysis_run_envelope(run: AnalysisRun) -> plsql_output::RobotJsonEnvelope<AnalysisRun> {
    plsql_output::RobotJsonEnvelope::new(ANALYSIS_RUN_SCHEMA, run)
}

/// Compact health summary of an [`AnalysisRun`] (R10/R11 doctor
/// surface). Reports exactly what the run established — it never
/// re-derives or guesses; the unwired stages show as zero with
/// `catalog_available` / `plscope_available` making the boundary
/// explicit (R13).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EngineDoctorReport {
    pub schema_id: String,
    pub schema_version: plsql_output::SchemaVersion,
    pub files_total: usize,
    pub files_parsed_cleanly: usize,
    pub files_recovered: usize,
    pub objects_total: usize,
    pub declaration_count: usize,
    pub fact_count: usize,
    pub catalog_available: bool,
    pub plscope_available: bool,
    pub diagnostic_count: usize,
    /// Honest headline: never `Clean` on a low-extraction run even
    /// when the file tally looks pristine.
    pub posture: plsql_core::CompletenessPosture,
    /// Top-level objects the classifier could not lower.
    pub objects_unrecognized: usize,
}

#[must_use]
pub fn engine_doctor_report(run: &AnalysisRun) -> EngineDoctorReport {
    EngineDoctorReport {
        schema_id: ENGINE_DOCTOR_SCHEMA.id.to_string(),
        schema_version: ENGINE_DOCTOR_SCHEMA.version,
        files_total: run.completeness.files_total,
        files_parsed_cleanly: run.completeness.files_parsed_cleanly,
        files_recovered: run.completeness.files_recovered,
        objects_total: run.completeness.objects_total,
        declaration_count: run.semantic_model.declaration_count,
        fact_count: run.fact_store.fact_count,
        catalog_available: run.completeness.catalog_available,
        plscope_available: run.completeness.plscope_available,
        diagnostic_count: run.diagnostics.len(),
        posture: run.completeness.posture,
        objects_unrecognized: run.completeness.objects_unrecognized,
    }
}

#[must_use]
pub fn engine_doctor_envelope(
    report: EngineDoctorReport,
) -> plsql_output::RobotJsonEnvelope<EngineDoctorReport> {
    plsql_output::RobotJsonEnvelope::new(ENGINE_DOCTOR_SCHEMA, report)
}

/// Status of a doctor section whose data is not (yet) carried in
/// the artifact. R13: a missing capability is reported as a typed
/// status, never silently shown as a zero/healthy value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    /// Data is present and reported.
    Reported,
    /// The producing stage is owned by a separate, not-yet-wired
    /// component; the section is intentionally empty (not "healthy").
    NotWired,
}

/// Full `plsql-engine doctor` report: the backend / catalog /
/// cache / fact-store / graph / completeness blocks, derived
/// purely from an [`AnalysisRun`] artifact.
///
/// Sections whose inputs are not in the artifact (e.g. the cache
/// hit ratio, not yet wired) report [`SectionStatus::NotWired`]
/// rather than a fabricated `0.0` that would read as "healthy" (R13).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EngineFullDoctorReport {
    pub schema_id: String,
    pub schema_version: plsql_output::SchemaVersion,
    // --- backend ---
    pub parser_backend: String,
    // --- catalog capability ---
    pub catalog_status: SectionStatus,
    pub catalog_available: bool,
    pub plscope_available: bool,
    // --- cache ---
    pub cache_status: SectionStatus,
    /// `Some` only when `cache_status == Reported`.
    pub cache_hit_ratio: Option<f64>,
    // --- fact store ---
    pub fact_count: usize,
    // --- graph ---
    pub graph_node_count: usize,
    pub graph_edge_count: usize,
    // --- completeness block ---
    pub completeness: plsql_core::CompletenessReport,
    pub diagnostic_count: usize,
}

#[must_use]
pub fn engine_full_doctor_report(run: &AnalysisRun) -> EngineFullDoctorReport {
    EngineFullDoctorReport {
        schema_id: ENGINE_FULL_DOCTOR_SCHEMA.id.to_string(),
        schema_version: ENGINE_FULL_DOCTOR_SCHEMA.version,
        parser_backend: if run.parser_backend.is_empty() {
            "<none>".to_string()
        } else {
            run.parser_backend.clone()
        },
        catalog_status: if run.catalog.is_some() {
            SectionStatus::Reported
        } else {
            SectionStatus::NotWired
        },
        catalog_available: run.completeness.catalog_available,
        plscope_available: run.completeness.plscope_available,
        // ENG-003B: cache provenance is now carried on the run.
        // `None` ⇒ caching was not active (no cache dir) →
        // honestly NotWired, not a misleading 0.0. `Some(hit)` ⇒
        // caching ran; ratio is 1.0 on a served-from-cache run,
        // 0.0 on a miss (single-run granularity).
        cache_status: match run.cache_outcome {
            Some(_) => SectionStatus::Reported,
            None => SectionStatus::NotWired,
        },
        cache_hit_ratio: run.cache_outcome.map(|hit| if hit { 1.0 } else { 0.0 }),
        fact_count: run.fact_store.fact_count,
        graph_node_count: run.dep_graph.node_count(),
        graph_edge_count: run.dep_graph.edge_count(),
        completeness: run.completeness.clone(),
        diagnostic_count: run.diagnostics.len(),
    }
}

#[must_use]
pub fn engine_full_doctor_envelope(
    report: EngineFullDoctorReport,
) -> plsql_output::RobotJsonEnvelope<EngineFullDoctorReport> {
    plsql_output::RobotJsonEnvelope::new(ENGINE_FULL_DOCTOR_SCHEMA, report)
}

/// Schema-version compatibility verdict for an
/// [`AnalysisArtifactManifest`].
///
/// A consumer that produced its tooling against schema version
/// `consumer` reading an artifact tagged `produced` is:
///
/// * **Compatible** — same major, `produced.minor <=
///   consumer.minor`. The consumer understands every field.
/// * **ForwardCompatible** — same major, `produced.minor >
///   consumer.minor`. The artifact carries newer optional
///   fields the consumer can ignore (additive minor-version
///   policy).
/// * **Incompatible** — major versions differ. The wire shape
///   changed; the consumer must refuse the artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaCompatibility {
    Compatible,
    ForwardCompatible,
    Incompatible,
}

/// Classify whether a consumer at schema version `consumer` can
/// read an artifact tagged `produced`. Patch level never affects
/// compatibility (patch = bugfix-only by policy).
#[must_use]
pub fn schema_compatibility(
    produced: plsql_output::SchemaVersion,
    consumer: plsql_output::SchemaVersion,
) -> SchemaCompatibility {
    if produced.major != consumer.major {
        return SchemaCompatibility::Incompatible;
    }
    if produced.minor > consumer.minor {
        SchemaCompatibility::ForwardCompatible
    } else {
        SchemaCompatibility::Compatible
    }
}

impl AnalysisArtifactManifest {
    /// Check this manifest's `schema_version` against the
    /// `consumer` version the reader was built for.
    #[must_use]
    pub fn compatibility_with(&self, consumer: plsql_output::SchemaVersion) -> SchemaCompatibility {
        schema_compatibility(self.schema_version, consumer)
    }

    /// True iff the consumer can safely read this manifest
    /// (Compatible or ForwardCompatible — never Incompatible).
    #[must_use]
    pub fn is_readable_by(&self, consumer: plsql_output::SchemaVersion) -> bool {
        !matches!(
            self.compatibility_with(consumer),
            SchemaCompatibility::Incompatible
        )
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum EngineError {
    /// Project discovery failed (bad root, unreadable manifest).
    #[error("project discovery failed: {0}")]
    ProjectDiscovery(String),
    /// A discovered source file could not be read off disk. The
    /// path is kept so the caller can surface exactly which file
    /// blocked the run rather than failing opaquely (R13).
    #[error("could not read source file {path}: {reason}")]
    SourceRead { path: String, reason: String },
}

/// File extensions treated as PL/SQL source by the engine spine.
/// Anything else discovered in the project tree is carried in the
/// completeness counts but not lowered.
const PLSQL_EXTENSIONS: &[&str] = &[
    "sql", "pls", "plsql", "pks", "pkb", "prc", "fnc", "trg", "tps", "tpb", "plb", "bdy", "spec",
    "typ",
];

/// Deterministic run id derived from the sorted relative paths of
/// the discovered files (djb2). Two runs over the same project
/// tree mint the same id — the analysis pipeline must be
/// reproducible (R10/R11 stable machine output).
fn deterministic_run_id(sorted_rel_paths: &[String]) -> AnalysisRunIdInner {
    let mut hash: u64 = 5381;
    for p in sorted_rel_paths {
        for b in p.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(u64::from(*b));
        }
        hash = hash.wrapping_mul(33).wrapping_add(0x1F);
    }
    hash
}

type AnalysisRunIdInner = u64;

/// Convert a slice of [`plsql_parser::ast::AstStatement`]s (syntactic layer)
/// into [`plsql_ir::Statement`]s (semantic IR layer).
///
/// This is a thin projection — the semantic IR lowering carries the same
/// information at a slightly different representation. Statements not
/// explicitly classified become `Statement::Unrecognized` with the raw text,
/// satisfying R13 (typed uncertainty, never silent drops).
/// Build the typed honest-degradation diagnostic for a unit whose
/// re-lowering walk hit the bounded recursion cap.
///
/// This is the R13 posture: a malformed or parser-recovered unit
/// whose `IF`/`LOOP` body text fails to strictly shrink across
/// re-lowering passes would otherwise grow the stack unbounded and
/// SIGABRT the whole project analyse. We instead degrade *that
/// nested body* honestly — carrying the typed
/// [`plsql_core::UnknownReason::AnalysisRecursionLimit`] with
/// provenance (which unit, which file, which walk, how many bodies)
/// — and continue the rest of the analysis. Pushed into
/// `run.diagnostics` *before* the completeness report is finalised
/// so the posture cannot read Clean.
fn recursion_limit_diagnostic(
    unit_logical_id: &str,
    relative_path: &str,
    walk: &str,
    truncated_bodies: usize,
) -> plsql_core::Diagnostic {
    plsql_core::Diagnostic::new(
        "ENG_ANALYSIS_RECURSION_LIMIT",
        plsql_core::Severity::Warn,
        format!(
            "{walk} for unit `{unit}` in `{path}` hit the bounded \
             re-lowering depth cap ({cap}) on {n} nested control-flow \
             {body_word}; that nested body is degraded (its calls / \
             table accesses below the cap are not extracted) rather \
             than walked unbounded. This unit's source did not parse \
             cleanly (a malformed / parser-recovered IF/LOOP whose \
             body slice fails to shrink) — re-check the upstream \
             parse diagnostics for this file.",
            walk = walk,
            unit = unit_logical_id,
            path = relative_path,
            cap = plsql_ir::MAX_RELOWER_DEPTH,
            n = truncated_bodies,
            body_word = if truncated_bodies == 1 {
                "body"
            } else {
                "bodies"
            },
        ),
    )
    .with_unknown_reason(plsql_core::UnknownReason::AnalysisRecursionLimit)
    .with_help(
        "Fix the unit's parse errors so its control-flow bodies \
         lower cleanly; the depth guard exists only to keep a \
         non-shrinking malformed slice from crashing the analyser.",
    )
}

fn ast_stmts_to_ir(ast_stmts: &[plsql_parser::ast::AstStatement]) -> Vec<plsql_ir::Statement> {
    use plsql_ir::{SqlVerb, Statement, UnknownStatementReason};
    use plsql_parser::ast::AstStatement;

    ast_stmts
        .iter()
        .flat_map(|s| match s {
            AstStatement::Null { .. } => vec![Statement::Null],
            AstStatement::Assignment {
                target, rhs_text, ..
            } => vec![Statement::Assignment {
                target: target.clone(),
                rhs_text: rhs_text.clone(),
            }],
            AstStatement::Return { value_text, .. } => vec![Statement::Return {
                value_text: value_text.clone(),
            }],
            AstStatement::Raise { exception, .. } => vec![Statement::Raise {
                exception: exception.clone(),
            }],
            AstStatement::ExecuteImmediate {
                sql_text,
                has_using,
                ..
            } => vec![Statement::ExecuteImmediate {
                sql_literal: sql_text.clone(),
                has_bind_variables: *has_using,
            }],
            AstStatement::Sql { verb, raw_text, .. } => {
                let sql_verb = match verb.to_ascii_uppercase().as_str() {
                    "SELECT" => SqlVerb::Select,
                    "INSERT" => SqlVerb::Insert,
                    "UPDATE" => SqlVerb::Update,
                    "DELETE" => SqlVerb::Delete,
                    "MERGE" => SqlVerb::Merge,
                    _ => SqlVerb::Select, // fallback
                };
                vec![Statement::Sql {
                    verb: sql_verb,
                    raw_text: raw_text.clone(),
                }]
            }
            AstStatement::Call { callee, .. } => {
                // A call statement: emit as Unrecognized with raw_text of the
                // form "callee()" so that `lower_expression` recognises it as
                // an `Expr::Call` and `extract_call_sites` can resolve the
                // callee. If the callee already contains `(`, trust it as-is.
                let raw = if callee.contains('(') {
                    callee.clone()
                } else {
                    format!("{callee}()")
                };
                vec![Statement::Unrecognized {
                    raw_text: raw,
                    unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
                }]
            }
            // IF / LOOP: `cond_text` / `header_text` carry the *full*
            // `IF … END IF;` / `LOOP … END LOOP;` source slice
            // (the whole parse-tree node span, body included). Re-lower
            // it through the IR statement-body parser so the nested
            // DML becomes recursive `Statement::If`/`ForLoop`/… that
            // `extract_table_accesses` (PLSQL-DEP-003) walks — without
            // this the body's SELECT/INSERT/UPDATE/DELETE is invisible.
            AstStatement::If { cond_text, .. } => plsql_ir::lower_statement_body(cond_text),
            AstStatement::Loop { header_text, .. } => plsql_ir::lower_statement_body(header_text),
            AstStatement::Unknown { .. } => vec![Statement::Unrecognized {
                raw_text: String::new(),
                unknown_reason: UnknownStatementReason::UnrecognizedKeyword,
            }],
        })
        .collect()
}

/// Run the canonical analysis pipeline
/// (`project → parse → catalog → IR → symbols → privileges →
/// sqlsem → flow → facts → depgraph`) and emit a populated
/// [`AnalysisRun`] with an honest [`CompletenessReport`].
///
/// ## Honest partial completeness (R13)
///
/// This is the orchestration *spine*. Stages whose deep analysis
/// is owned by their own follow-up components — live-catalog
/// extraction, SQL-semantic modelling, inter-procedural flow, fact
/// minting, dependency-edge construction — are wired but currently
/// yield empty summaries. The pipeline never *fabricates* counts:
/// the emitted [`CompletenessReport`] reports exactly what was
/// established (`catalog_available`, `plscope_available`, parsed
/// vs. recovered, object totals) so a consumer can see the
/// boundary instead of mistaking "not yet wired" for "analysed
/// and clean".
#[instrument(level = "debug", skip(req), fields(root = %req.project_root.display()))]
pub fn analyze_project(req: AnalysisRequest) -> Result<AnalysisRun, EngineError> {
    use plsql_core::{AnalysisRunId, CompletenessReport, SymbolInterner};

    let mut run = AnalysisRun {
        profile: req.analysis_profile.clone(),
        ..AnalysisRun::default()
    };
    run.project.root = req.project_root.clone();
    // Record the real backend name unconditionally — even for the no-op empty-root
    // path so that doctor reports always reflect the wired backend (not "<none>").
    run.parser_backend = "antlr4rust".to_string();
    run.artifacts = AnalysisArtifactManifest {
        // Stamp the embedded manifest with the single-source-of-truth
        // schema version so the producer and ANALYSIS_RUN_SCHEMA cannot
        // drift apart when the schema advances. A hardcoded literal here
        // would mislabel a 1.1.0-field-bearing artifact as 1.0.0 and make
        // a consumer that gates on the manifest (compatibility_with /
        // is_readable_by) read it as Compatible instead of ForwardCompatible.
        schema_version: ANALYSIS_RUN_SCHEMA.version,
        artifact_digests: Vec::new(),
        redaction_policy: req.redaction_policy.clone(),
    };

    // An empty/default request (no project root) is a valid,
    // reproducible no-op run rather than an error — keeps callers
    // (and the default-construction tests) from needing a real
    // tree just to exercise the type.
    if req.project_root.as_os_str().is_empty() {
        return Ok(run);
    }

    // --- Stage 1: project discovery -----------------------------
    let manifest = plsql_project::ProjectManifest::load(&req.project_root)
        .map_err(|e| EngineError::ProjectDiscovery(format!("{e:?}")))?;
    let mut files = plsql_project::discover_files(&req.project_root, &manifest)
        .map_err(|e| EngineError::ProjectDiscovery(format!("{e:?}")))?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let sorted_rel: Vec<String> = files.iter().map(|f| f.relative_path.clone()).collect();
    run.run_id = AnalysisRunId::new(deterministic_run_id(&sorted_rel));
    run.project.file_count = files.len();

    // --- ENG-003B: content+profile-keyed cache reuse ------------
    // Best-effort: a cache directory must be explicitly
    // configured AND enabled. Any cache failure degrades to an
    // uncached run (cache is an optimisation, never correctness).
    // The key folds every source file's bytes (so a content
    // change misses) and a serialisation of BOTH the analysis
    // profile AND the redaction policy (so a profile or a
    // redaction-posture change invalidates — a stale fragment is
    // never served across profiles or across redaction policies,
    // by key construction). Folding the redaction policy is
    // forward-correctness hardening: no shipped consumer yet reads
    // run.artifacts.redaction_policy to gate disclosure, but were
    // one wired up, a fragment cached under a permissive policy
    // must never be served to a request that asked for a stricter
    // one (which would under-redact).
    const CACHE_STRATEGY: &str = "semantic_fragment";
    let cache_ctx: Option<(plsql_store::Store, String, String)> = (|| {
        if !req.cache.enabled {
            return None;
        }
        let dir = req.cache.directory.as_ref()?;
        let mut hasher_input: Vec<u8> = Vec::new();
        for f in &files {
            hasher_input.extend_from_slice(f.relative_path.as_bytes());
            hasher_input.push(0);
            let bytes = std::fs::read(req.project_root.join(&f.relative_path)).ok()?;
            hasher_input.extend_from_slice(&bytes);
            hasher_input.push(0);
        }
        let content_hash = plsql_store::hash_hex(&hasher_input);
        // Fold the analysis profile AND the redaction policy into the
        // same key component so a change to either invalidates the
        // cached fragment (a permissively-redacted artifact must never
        // satisfy a stricter-redaction request).
        let profile_bytes =
            serde_json::to_vec(&(&req.analysis_profile, &req.redaction_policy)).ok()?;
        let profile_hash = plsql_store::hash_hex(&profile_bytes);
        let store =
            plsql_store::Store::open(&dir.join("cache.db"), plsql_store::StoreConfig::default())
                .ok()?;
        Some((store, content_hash, profile_hash))
    })();

    if let Some((store, content_hash, profile_hash)) = &cache_ctx {
        let key = plsql_store::CacheKey {
            strategy_name: CACHE_STRATEGY,
            content_hash,
            profile_hash,
        };
        if let Ok(Some(blob)) = store.get_cached(key) {
            if let Ok(mut cached) = serde_json::from_slice::<AnalysisRun>(&blob.body) {
                cached.cache_outcome = Some(true);
                return Ok(cached);
            }
        }
    }

    // --- Stage 2: parse + Stage 4: IR lowering + Stage 5: symbols
    use plsql_core::{Confidence, ConfidenceLevel};
    use plsql_depgraph::{
        Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind, ObjectRevisionId,
        Provenance, QualifiedName, ResolutionStrategy,
    };
    use plsql_ir::{
        FactStore,
        fact::{FactPayload, FactProvenance, mint_fact},
        lower_statement_body,
    };
    use plsql_parser::{ParseOptions, parse_with_backend};
    use plsql_parser_antlr::Antlr4RustBackend;

    let backend = Antlr4RustBackend::new();
    let parse_opts = ParseOptions::default();

    // Provenance stamped on every fact this stage mints (identical across
    // call sites — declaration, call edge, table access).
    let engine_prov = || FactProvenance {
        component: "plsql-engine".into(),
        component_version: env!("CARGO_PKG_VERSION").into(),
        run_id: String::new(),
    };
    // The synthetic "UNKNOWN" schema used for nodes whose schema cannot
    // be resolved from source alone (no live catalog).
    let unknown_schema = |interner: &mut SymbolInterner| {
        interner
            .intern_schema_name("UNKNOWN")
            .unwrap_or_else(|| plsql_core::SchemaName::from(plsql_core::SymbolId::new(0)))
    };

    let mut interner = SymbolInterner::new();
    let mut table = plsql_symbols::DeclTable::new();
    let mut file_counter: u32 = 0;
    let mut files_parsed_cleanly = 0usize;
    let mut files_recovered = 0usize;

    // Accumulate semantic artifacts across files.
    let mut fact_store = FactStore::default();
    let mut dep_graph = DepGraph::new();
    let mut next_node_id: u64 = 1;
    let mut next_edge_id: u64 = 1;

    // Logical-id → graph NodeId for cross-declaration call resolution.
    let mut nodes_by_logical_id: std::collections::HashMap<String, NodeId> =
        std::collections::HashMap::new();

    for f in &files {
        if !PLSQL_EXTENSIONS.contains(&f.extension.as_str()) {
            continue;
        }
        let abs = req.project_root.join(&f.relative_path);
        let source = std::fs::read_to_string(&abs).map_err(|e| EngineError::SourceRead {
            path: f.relative_path.clone(),
            reason: e.to_string(),
        })?;

        let file_id = plsql_core::FileId::new(file_counter);
        file_counter += 1;

        // Parse with the real ANTLR4 backend — this is the D2 real parse path.
        let parse_result = parse_with_backend(&source, file_id, &backend, &parse_opts);
        let ast = &parse_result.ast;

        let lowered = plsql_ir::lower_top_level(ast, &mut interner);

        let had_errors = parse_result
            .diagnostics
            .iter()
            .any(|d| d.severity >= plsql_core::Severity::Error);
        if !had_errors {
            files_parsed_cleanly += 1;
        }
        if parse_result.recovered {
            files_recovered += 1;
        }

        let parse_result_path = abs.clone();
        run.parse_results.push(ParseResult {
            file: parse_result_path,
            recovered: parse_result.recovered,
        });
        run.diagnostics.extend(parse_result.diagnostics);
        run.diagnostics.extend(lowered.diagnostics);

        // --- Fact emission: one Declaration fact per top-level decl --------
        let prov = engine_prov();
        for decl in &lowered.declarations {
            let logical_id = interner
                .resolve(decl.common().name)
                .unwrap_or("")
                .to_ascii_uppercase();
            let decl_fact = mint_fact(
                prov.clone(),
                FactPayload::Declaration {
                    decl: plsql_ir::DeclId::new(next_node_id),
                    logical_id: logical_id.clone(),
                },
            );
            fact_store.push(decl_fact);

            // --- Depgraph: register a node for this declaration ------
            let schema_name = unknown_schema(&mut interner);
            let obj_name = plsql_core::ObjectName::from(decl.common().name);
            let kind = match decl {
                plsql_ir::Declaration::Package(_) => NodeIdentityKind::PackageBody,
                plsql_ir::Declaration::Procedure(_) => NodeIdentityKind::StandaloneProcedure,
                plsql_ir::Declaration::Function(_) => NodeIdentityKind::StandaloneFunction,
                plsql_ir::Declaration::Trigger(_) => NodeIdentityKind::Trigger,
                plsql_ir::Declaration::View(_) => NodeIdentityKind::View,
                _ => NodeIdentityKind::Unknown,
            };
            let node_id = NodeId::new(next_node_id);
            next_node_id += 1;
            dep_graph.insert_node(Node::new(
                node_id,
                LogicalObjectId::new(logical_id.clone()),
                ObjectRevisionId::new("source"),
                QualifiedName::new(Some(schema_name), obj_name),
                kind,
            ));
            nodes_by_logical_id.insert(logical_id, node_id);
        }

        // --- Call-site extraction from body statements ----------------
        // Use body_statements from the real ANTLR parse tree if populated
        // (non-empty), otherwise fall back to the text scanner for each
        // declaration's source span.
        //
        // `ast.body_statements` is parallel with `ast.root.declarations`
        // (the *unfiltered* parser output), NOT with `lowered.declarations`
        // (a filtered subset — DDL/Unknown decls are dropped during
        // lowering, see plsql_ir::lower_top_level). Indexing the parser-
        // parallel bodies with the *lowered* loop index therefore misattributes
        // a later object's body to an earlier slot whenever a dropped
        // (DDL/Unknown) declaration precedes a real one, silently corrupting
        // Calls/Reads/Writes edges (oracle-qm3q.9). Pair them by source span
        // instead: each lowered Declaration carries the same span the parser
        // recorded on its AstDecl (lower_top_level threads it through
        // make_common), so a span-keyed map recovers the correct body
        // regardless of how many decls were filtered out ahead of it.
        let body_by_span: std::collections::HashMap<plsql_core::Span, &Vec<plsql_parser::ast::AstStatement>> =
            ast.root
                .declarations
                .iter()
                .map(plsql_parser::ast::Spanned::span)
                .zip(ast.body_statements.iter())
                .collect();

        for decl in &lowered.declarations {
            let caller_id_str = interner
                .resolve(decl.common().name)
                .unwrap_or("")
                .to_ascii_uppercase();
            let Some(&caller_node_id) = nodes_by_logical_id.get(&caller_id_str) else {
                continue;
            };

            // Get body statements: prefer real parse-tree lowering, fall back
            // to text-scanner on the span slice. Look the parse-tree body up
            // by span (not loop position) — see the note above.
            let body_stmts: Vec<plsql_ir::Statement> = if let Some(ast_stmts) = body_by_span
                .get(&decl.common().span)
                .copied()
                .filter(|s| !s.is_empty())
            {
                // Convert AstStatement → plsql_ir::Statement.
                ast_stmts_to_ir(ast_stmts)
            } else {
                // Fallback: extract body slice from source and use the
                // text-scanner statement lowerer.
                let span = decl.common().span;
                let s = (span.start.offset as usize).min(source.len());
                let e = (span.end.offset as usize).min(source.len());
                let slice = if s < e { &source[s..e] } else { "" };
                lower_statement_body(slice)
            };

            let (call_sites, call_recursion) = plsql_ir::extract_call_sites_bounded(&body_stmts);
            if call_recursion.limit_hit {
                run.diagnostics.push(recursion_limit_diagnostic(
                    &caller_id_str,
                    &f.relative_path,
                    "call-site extraction",
                    call_recursion.truncated_bodies,
                ));
            }
            for cs in &call_sites {
                let callee_logical = cs.callee_parts.join(".").to_ascii_uppercase();
                // Emit a call fact.
                let call_fact = mint_fact(
                    engine_prov(),
                    FactPayload::DependencyEdge {
                        from_logical_id: caller_id_str.clone(),
                        to_logical_id: callee_logical.clone(),
                        edge_kind: "Calls".to_string(),
                    },
                );
                fact_store.push(call_fact);

                // Add a depgraph edge if callee node is known.
                // Resolution tries the full dotted name first (e.g. "PKG_A.DO_WORK"),
                // then falls back to the leading package name ("PKG_A") so that
                // package-qualified calls resolve to the package node when the
                // individual member isn't registered separately.
                let resolved_callee_node = nodes_by_logical_id
                    .get(&callee_logical)
                    .copied()
                    .or_else(|| {
                        cs.callee_parts
                            .first()
                            .map(|p| p.to_ascii_uppercase())
                            .and_then(|pkg| nodes_by_logical_id.get(&pkg).copied())
                    });
                if let Some(callee_node_id) = resolved_callee_node {
                    let edge = Edge::new(
                        EdgeId::new(next_edge_id),
                        caller_node_id,
                        callee_node_id,
                        EdgeKind::Calls,
                        Confidence::new(ConfidenceLevel::Medium, None),
                    );
                    next_edge_id += 1;
                    let prov3 = Provenance::new(
                        file_id,
                        decl.common().span,
                        ResolutionStrategy::LocalLexical,
                    )
                    .with_note(format!("call to {}", cs.callee_display));
                    dep_graph.insert_edge(edge, prov3, None);
                }
            }

            // --- Table-level Read/Write extraction (PLSQL-DEP-003) ----
            // Walk the embedded SQL DML in this body and emit a
            // Reads/Writes dep-graph edge + a DependencyEdge fact per
            // distinct table access. Each referenced table is
            // registered as a synthetic node (identity Unknown) so the
            // edge has a concrete endpoint even when the table is not a
            // declared object in this project.
            let (accesses, dml_recursion) =
                plsql_ir::dml_edges::extract_table_accesses_bounded(&body_stmts);
            if dml_recursion.limit_hit {
                run.diagnostics.push(recursion_limit_diagnostic(
                    &caller_id_str,
                    &f.relative_path,
                    "table-access extraction",
                    dml_recursion.truncated_bodies,
                ));
            }
            for acc in &accesses {
                let table_logical = match &acc.schema {
                    Some(s) => format!(
                        "{}.{}",
                        s.to_ascii_uppercase(),
                        acc.table.to_ascii_uppercase()
                    ),
                    None => acc.table.to_ascii_uppercase(),
                };
                if table_logical.is_empty() {
                    continue;
                }
                let (edge_kind, edge_kind_str) = match acc.access {
                    plsql_ir::dml_edges::AccessKind::Read => (EdgeKind::Reads, "Reads"),
                    plsql_ir::dml_edges::AccessKind::Write => (EdgeKind::Writes, "Writes"),
                };

                // Emit a DependencyEdge fact (Reads/Writes).
                fact_store.push(mint_fact(
                    engine_prov(),
                    FactPayload::DependencyEdge {
                        from_logical_id: caller_id_str.clone(),
                        to_logical_id: table_logical.clone(),
                        edge_kind: edge_kind_str.to_string(),
                    },
                ));

                // Resolve (or synthesise) the table node.
                let table_node_id = match nodes_by_logical_id.get(&table_logical).copied() {
                    Some(id) => id,
                    None => {
                        let schema_name = unknown_schema(&mut interner);
                        let tbl_sym = interner
                            .intern(table_logical.clone())
                            .unwrap_or_else(|| plsql_core::SymbolId::new(0));
                        let obj_name = plsql_core::ObjectName::from(tbl_sym);
                        let nid = NodeId::new(next_node_id);
                        next_node_id += 1;
                        dep_graph.insert_node(Node::new(
                            nid,
                            LogicalObjectId::new(table_logical.clone()),
                            ObjectRevisionId::new("source"),
                            QualifiedName::new(Some(schema_name), obj_name),
                            NodeIdentityKind::Unknown,
                        ));
                        nodes_by_logical_id.insert(table_logical.clone(), nid);
                        nid
                    }
                };

                let edge = Edge::new(
                    EdgeId::new(next_edge_id),
                    caller_node_id,
                    table_node_id,
                    edge_kind,
                    Confidence::new(ConfidenceLevel::Medium, None),
                );
                next_edge_id += 1;
                let provp = Provenance::new(
                    file_id,
                    decl.common().span,
                    ResolutionStrategy::LocalLexical,
                )
                .with_note(format!("{edge_kind_str} {table_logical}"));
                dep_graph.insert_edge(edge, provp, None);
            }
        }

        table.register_all(lowered.declarations);
    }

    let declaration_count = table.len();
    run.semantic_model = SemanticModel { declaration_count };
    // parser_backend already set unconditionally at top of analyze_project.

    // --- Stage 3: catalog ---------------------------------------
    // Live-connection / snapshot extraction is owned by the
    // PLSQL-CAT beads. The spine records availability honestly;
    // it does not synthesise a catalog.
    run.catalog = None;
    let catalog_available = false;

    // --- Stage 6-9: privileges / sqlsem / flow / facts ----------
    // Facts and call edges are now populated above from the real parse
    // tree. Deep SQL-semantic / flow analysis lives in its own beads.
    run.sql_semantic = SqlSemanticModel::default();
    run.flow_summary = FlowSummary::default();
    run.fact_store = FactStoreSnapshot {
        fact_count: fact_store.len(),
        facts: fact_store.facts.clone(),
    };

    // --- Stage 10: depgraph -------------------------------------
    run.dep_graph = dep_graph;

    // --- CompletenessReport -------------------------------------
    // Honest signals (oracle-bh4p / Phase 2, §1.5): a near-pristine
    // file tally MUST NOT hide that thousands of top-level objects
    // were never lowered. Count the `IR_UNCLASSIFIED_DECL`
    // diagnostics (AST classifier returned `Unknown`) — these are
    // objects that contributed nothing to the semantic model.
    let objects_unrecognized = run
        .diagnostics
        .iter()
        .filter(|d| d.code == "IR_UNCLASSIFIED_DECL")
        .count();
    let diagnostics_total = run.diagnostics.len();

    let mut completeness = CompletenessReport {
        files_total: run.parse_results.len(),
        files_parsed_cleanly,
        files_recovered,
        skipped_token_ratio: 0.0,
        objects_total: declaration_count,
        objects_with_source: declaration_count,
        objects_catalog_only: 0,
        // Pipeline stages that mint these are owned by their own
        // (open) beads and not yet wired. They serialise as
        // `{ "unmeasured": true }` — NEVER a misleading `0` that a
        // reader could confuse with "looked, found none".
        wrapped_units: plsql_core::Measured::Unmeasured,
        missing_package_bodies: plsql_core::Measured::Unmeasured,
        dynamic_sql_sites: plsql_core::Measured::Unmeasured,
        opaque_dynamic_sql_sites: plsql_core::Measured::Unmeasured,
        db_link_edges: plsql_core::Measured::Unmeasured,
        unresolved_references: plsql_core::Measured::Unmeasured,
        diagnostics_total,
        objects_unrecognized,
        // Lowered declarations are the ones with real extracted
        // semantics; unrecognized objects are NOT counted here.
        objects_with_extracted_semantics: declaration_count,
        extracted_semantics_ratio: 0.0,
        posture: plsql_core::CompletenessPosture::default(),
        catalog_available,
        plscope_available: false,
    };
    // Derive the headline posture + ratio from the honest counts.
    // A low-extraction run is now forced to read non-Clean.
    completeness.finalize_posture();
    run.completeness = completeness;

    // Cache miss path: caching was active but no hit — record
    // the freshly-computed run for next time (best-effort; a
    // store write failure never fails the analysis).
    if let Some((store, content_hash, profile_hash)) = &cache_ctx {
        run.cache_outcome = Some(false);
        // PLSQL-PERF-001: persist the compact form when opted in
        // (heavy catalog/graph dropped); default persists the
        // full run unchanged.
        let to_store = if req.cache.compact_persisted {
            run.compact()
        } else {
            run.clone()
        };
        if let Ok(body) = serde_json::to_vec(&to_store) {
            let _ = store.put_cached(
                plsql_store::CacheKey {
                    strategy_name: CACHE_STRATEGY,
                    content_hash,
                    profile_hash,
                },
                "application/json",
                &body,
            );
        }
    }

    Ok(run)
}

#[cfg(test)]
mod tests {
    use crate::{AnalysisRequest, analyze_project};

    #[test]
    fn analysis_request_default_is_constructible() {
        let request = AnalysisRequest::default();

        assert!(request.cache.enabled);
        assert_eq!(request.project_root.as_os_str(), "");
    }

    #[test]
    fn empty_request_is_a_reproducible_noop_run() {
        // No project root → a valid empty run, not an error, and
        // byte-identical across invocations (deterministic spine).
        let a = analyze_project(AnalysisRequest::default()).expect("empty run ok");
        let b = analyze_project(AnalysisRequest::default()).expect("empty run ok");
        assert_eq!(a, b);
        assert_eq!(a.project.file_count, 0);
        assert_eq!(a.completeness.objects_total, 0);
        assert!(!a.completeness.catalog_available);
    }

    #[test]
    fn pipeline_lowers_discovered_sources_and_counts_objects() {
        let dir = std::env::temp_dir().join(format!(
            "plsql-eng002-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.sql"),
            "CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("b.sql"),
            "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n",
        )
        .unwrap();

        let req = AnalysisRequest {
            project_root: dir.clone(),
            ..AnalysisRequest::default()
        };
        let run = analyze_project(req).expect("pipeline ok");

        assert_eq!(run.parse_results.len(), 2, "both .sql files lowered");
        assert!(
            run.semantic_model.declaration_count >= 2,
            "package + procedure registered, got {}",
            run.semantic_model.declaration_count
        );
        assert_eq!(
            run.completeness.objects_total,
            run.semantic_model.declaration_count
        );
        assert!(!run.completeness.catalog_available);

        // Deterministic run id over the same tree.
        let req2 = AnalysisRequest {
            project_root: dir.clone(),
            ..AnalysisRequest::default()
        };
        let run2 = analyze_project(req2).expect("ok");
        assert_eq!(run.run_id, run2.run_id);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// D2 Phase 1C acceptance gate: real ANTLR4 backend must produce
    /// non-empty dep_graph nodes, dep_graph edges, and fact_store facts
    /// on a minimal two-package corpus where one package calls the other.
    #[test]
    fn d2_phase1c_real_backend_yields_nonzero_semantics() {
        let dir = std::env::temp_dir().join(format!(
            "plsql-d2-1c-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // Package A: simple spec.
        std::fs::write(
            dir.join("pkg_a.sql"),
            "CREATE OR REPLACE PACKAGE PKG_A AS\n  PROCEDURE do_work;\nEND PKG_A;\n/\n",
        )
        .unwrap();
        // Package B body: calls PKG_A.do_work so the engine can resolve
        // a dep_graph edge from PKG_B → PKG_A.
        std::fs::write(
            dir.join("pkg_b.sql"),
            "CREATE OR REPLACE PACKAGE BODY PKG_B AS\n  PROCEDURE run IS\n  BEGIN\n    PKG_A.do_work;\n  END run;\nEND PKG_B;\n/\n",
        )
        .unwrap();

        let req = AnalysisRequest {
            project_root: dir.clone(),
            ..AnalysisRequest::default()
        };
        let run = analyze_project(req).expect("D2-1C pipeline ok");

        // Fact-store must be non-empty (Declaration facts at minimum).
        assert!(
            run.fact_store.fact_count > 0,
            "fact_store must be non-empty after real parse, got {}",
            run.fact_store.fact_count
        );

        // Dep_graph must have nodes for the parsed declarations.
        assert!(
            run.dep_graph.node_count() > 0,
            "dep_graph must have nodes, got {}",
            run.dep_graph.node_count()
        );

        // Dep_graph must have at least one resolved call edge
        // (PKG_B → PKG_A resolved because both are in the corpus).
        assert!(
            run.dep_graph.edge_count() > 0,
            "dep_graph must have >=1 edge (PKG_B calls PKG_A), got {}",
            run.dep_graph.edge_count()
        );

        // Parser backend must be labelled correctly.
        assert_eq!(run.parser_backend, "antlr4rust");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn eng003b_cache_miss_then_hit_then_profile_invalidation() {
        use crate::{SectionStatus, config::CacheConfig, engine_full_doctor_report};
        let base = std::env::temp_dir().join(format!(
            "plsql-eng003b-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        let cache = base.join("cache");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            proj.join("a.sql"),
            "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n",
        )
        .unwrap();

        let mk = || AnalysisRequest {
            project_root: proj.clone(),
            cache: CacheConfig {
                enabled: true,
                directory: Some(cache.clone()),
                ..CacheConfig::default()
            },
            ..AnalysisRequest::default()
        };

        // First run: cache active but empty -> miss, stored.
        let miss = analyze_project(mk()).expect("miss ok");
        assert_eq!(miss.cache_outcome, Some(false), "first run is a miss");
        let d_miss = engine_full_doctor_report(&miss);
        assert_eq!(d_miss.cache_status, SectionStatus::Reported);
        assert_eq!(d_miss.cache_hit_ratio, Some(0.0));

        // Second run, identical content+profile -> hit; the
        // served run equals the stored one (modulo the outcome
        // flag).
        let hit = analyze_project(mk()).expect("hit ok");
        assert_eq!(hit.cache_outcome, Some(true), "second run is a hit");
        assert_eq!(hit.run_id, miss.run_id);
        assert_eq!(hit.semantic_model, miss.semantic_model);
        assert_eq!(engine_full_doctor_report(&hit).cache_hit_ratio, Some(1.0));

        // Profile change -> different profile_hash -> key miss
        // (a stale fragment is never served across profiles).
        let mut prof_req = mk();
        prof_req.analysis_profile.current_schema = Some(
            plsql_core::SymbolInterner::new()
                .intern_schema_name("DIFFERENT")
                .unwrap(),
        );
        let after_profile_change = analyze_project(prof_req).expect("ok");
        assert_eq!(
            after_profile_change.cache_outcome,
            Some(false),
            "a changed analysis profile must invalidate the cached fragment"
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn eng003b_redaction_policy_change_invalidates_cache() {
        // oracle-hrzg.7: the cache key must fold the redaction policy
        // so a fragment cached under a permissive (non-redacting)
        // policy is never served to a request asking for a stricter
        // posture. Before the fix the key folded only the analysis
        // profile, so flipping `redact_freeform_text` was a cache hit
        // and stamped the request's strict policy verbatim onto a body
        // computed under the permissive one.
        use crate::config::CacheConfig;
        use plsql_output::RedactionPolicy;
        let base = std::env::temp_dir().join(format!(
            "plsql-eng003b-redact-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        let cache = base.join("cache");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            proj.join("a.sql"),
            "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n",
        )
        .unwrap();

        let mk = |policy: RedactionPolicy| AnalysisRequest {
            project_root: proj.clone(),
            cache: CacheConfig {
                enabled: true,
                directory: Some(cache.clone()),
                ..CacheConfig::default()
            },
            redaction_policy: policy,
            ..AnalysisRequest::default()
        };

        // First run under the permissive default policy -> miss, stored.
        let permissive = RedactionPolicy::default();
        assert!(!permissive.redact_freeform_text);
        let miss = analyze_project(mk(permissive.clone())).expect("miss ok");
        assert_eq!(miss.cache_outcome, Some(false), "first run is a miss");

        // Same content + profile + policy -> hit.
        let hit = analyze_project(mk(permissive)).expect("hit ok");
        assert_eq!(hit.cache_outcome, Some(true), "identical request is a hit");

        // Only the redaction posture changes (stricter): must MISS so
        // the permissively-cached fragment cannot under-redact.
        let strict = RedactionPolicy {
            redact_freeform_text: true,
            ..RedactionPolicy::default()
        };
        let after_policy_change = analyze_project(mk(strict.clone())).expect("ok");
        assert_eq!(
            after_policy_change.cache_outcome,
            Some(false),
            "a stricter redaction policy must invalidate the cached fragment"
        );
        // The freshly computed run carries the requested strict policy.
        assert_eq!(after_policy_change.artifacts.redaction_policy, strict);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn eng003b_no_cache_dir_stays_notwired() {
        use crate::{SectionStatus, engine_full_doctor_report};
        // Default request has no cache directory -> caching
        // inactive -> cache_outcome None -> doctor honestly
        // reports NotWired, not a fabricated 0.0.
        let run = analyze_project(AnalysisRequest::default()).unwrap();
        assert_eq!(run.cache_outcome, None);
        let d = engine_full_doctor_report(&run);
        assert_eq!(d.cache_status, SectionStatus::NotWired);
        assert_eq!(d.cache_hit_ratio, None);
    }

    #[test]
    fn perf001_compact_drops_heavy_payloads_preserves_summaries() {
        use crate::{AnalysisRun, CatalogSnapshot};
        // A run carrying a catalog snapshot + a non-empty graph.
        let mut run = AnalysisRun {
            catalog: Some(CatalogSnapshot::default()),
            ..AnalysisRun::default()
        };
        run.semantic_model.declaration_count = 7;
        run.completeness.files_total = 3;
        run.diagnostics.push(plsql_core::Diagnostic::default());
        run.cache_outcome = Some(false);

        let c = run.compact();
        // Heavy, re-derivable payloads dropped.
        assert!(c.catalog.is_none(), "catalog snapshot evicted");
        assert_eq!(c.dep_graph.node_count(), 0, "dep graph evicted");
        // Every cheap summary preserved verbatim.
        assert_eq!(c.semantic_model.declaration_count, 7);
        assert_eq!(c.completeness.files_total, 3);
        assert_eq!(c.diagnostics.len(), 1);
        assert_eq!(c.cache_outcome, Some(false));
        assert_eq!(c.run_id, run.run_id);
        // Idempotent + JSON round-trips.
        assert_eq!(c.compact(), c);
        let j = serde_json::to_string(&c).unwrap();
        let back: AnalysisRun = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn perf002_memory_profile_reports_compact_savings() {
        use crate::{AnalysisRun, CatalogSnapshot, ENGINE_MEMORY_SCHEMA, engine_memory_profile};
        let run = AnalysisRun {
            catalog: Some(CatalogSnapshot::default()),
            ..AnalysisRun::default()
        };
        let m = engine_memory_profile(&run);
        assert_eq!(m.schema_id, ENGINE_MEMORY_SCHEMA.id);
        assert!(m.full_bytes > 0);
        // Compact drops the catalog snapshot -> never larger.
        assert!(m.compact_bytes <= m.full_bytes);
        assert_eq!(m.savings_bytes, m.full_bytes - m.compact_bytes);
        assert!(m.catalog_bytes > 0, "the Some(catalog) contributes bytes");
        assert!((0.0..=1.0).contains(&m.savings_ratio));
        // Deterministic.
        assert_eq!(engine_memory_profile(&run), m);
    }

    #[test]
    fn perf002_empty_run_profile_is_zero_ratio_not_nan() {
        use crate::{AnalysisRun, engine_memory_profile};
        let m = engine_memory_profile(&AnalysisRun::default());
        assert_eq!(m.savings_ratio, 0.0);
        assert!(m.savings_ratio.is_finite());
    }

    #[test]
    fn perf001_compact_persisted_flag_keeps_cache_functional() {
        use crate::config::CacheConfig;
        let base = std::env::temp_dir().join(format!(
            "plsql-perf001-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        let cache = base.join("cache");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            proj.join("a.sql"),
            "CREATE PROCEDURE pr IS BEGIN NULL; END;\n/\n",
        )
        .unwrap();
        let mk = || AnalysisRequest {
            project_root: proj.clone(),
            cache: CacheConfig {
                enabled: true,
                directory: Some(cache.clone()),
                compact_persisted: true,
                ..CacheConfig::default()
            },
            ..AnalysisRequest::default()
        };
        let miss = analyze_project(mk()).expect("miss ok");
        assert_eq!(miss.cache_outcome, Some(false));
        // Hit serves the compact persisted form: the summary that
        // matters survives; the evicted catalog stays None.
        let hit = analyze_project(mk()).expect("hit ok");
        assert_eq!(hit.cache_outcome, Some(true));
        assert_eq!(hit.semantic_model, miss.semantic_model);
        assert!(hit.catalog.is_none());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn analysis_run_artifact_round_trips_through_robot_json() {
        use crate::{ANALYSIS_RUN_SCHEMA, analysis_run_envelope};
        let run = analyze_project(AnalysisRequest::default()).unwrap();
        let env = analysis_run_envelope(run.clone());
        assert!(env.matches_schema(ANALYSIS_RUN_SCHEMA));
        let json = serde_json::to_string(&env).unwrap();
        let back: plsql_output::RobotJsonEnvelope<crate::AnalysisRun> =
            serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload, run, "artifact must survive a JSON round-trip");
        assert!(back.matches_schema(ANALYSIS_RUN_SCHEMA));
    }

    #[test]
    fn doctor_report_summarises_run_without_fabrication() {
        use crate::{ANALYSIS_RUN_SCHEMA, ENGINE_DOCTOR_SCHEMA, engine_doctor_report};
        let run = analyze_project(AnalysisRequest::default()).unwrap();
        let d = engine_doctor_report(&run);
        // The doctor report carries its OWN schema id, distinct
        // from the run-artifact schema, so a consumer can tell
        // them apart via matches_schema.
        assert_eq!(d.schema_id, ENGINE_DOCTOR_SCHEMA.id);
        assert_ne!(d.schema_id, ANALYSIS_RUN_SCHEMA.id);
        assert_eq!(d.objects_total, 0);
        assert_eq!(d.declaration_count, 0);
        assert!(!d.catalog_available);
        assert!(!d.plscope_available);
    }

    #[test]
    fn full_doctor_reports_all_blocks_and_flags_unwired_honestly() {
        use crate::{SectionStatus, engine_full_doctor_report};
        let run = analyze_project(AnalysisRequest::default()).unwrap();
        let f = engine_full_doctor_report(&run);
        // Backend recorded from the request (default).
        assert_eq!(f.parser_backend, "antlr4rust");
        // No catalog / cache wired -> typed NotWired, not a
        // fabricated healthy zero (R13).
        assert_eq!(f.catalog_status, SectionStatus::NotWired);
        assert_eq!(f.cache_status, SectionStatus::NotWired);
        assert_eq!(f.cache_hit_ratio, None);
        // Graph + fact blocks derive straight from the artifact.
        assert_eq!(f.graph_node_count, 0);
        assert_eq!(f.graph_edge_count, 0);
        assert_eq!(f.fact_count, 0);
        assert_eq!(f.completeness, run.completeness);
    }

    #[test]
    fn full_doctor_envelope_round_trips() {
        use crate::{
            ANALYSIS_RUN_SCHEMA, ENGINE_FULL_DOCTOR_SCHEMA, EngineFullDoctorReport,
            engine_full_doctor_envelope, engine_full_doctor_report,
        };
        let run = analyze_project(AnalysisRequest::default()).unwrap();
        let env = engine_full_doctor_envelope(engine_full_doctor_report(&run));
        assert!(env.matches_schema(ENGINE_FULL_DOCTOR_SCHEMA));
        assert!(
            !env.matches_schema(ANALYSIS_RUN_SCHEMA),
            "full doctor envelope must NOT masquerade as a run artifact"
        );
        let json = serde_json::to_string(&env).unwrap();
        let back: plsql_output::RobotJsonEnvelope<EngineFullDoctorReport> =
            serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload, engine_full_doctor_report(&run));
    }

    use crate::{AnalysisArtifactManifest, SchemaCompatibility, schema_compatibility};
    use plsql_output::SchemaVersion;

    #[test]
    fn same_version_is_compatible() {
        assert_eq!(
            schema_compatibility(SchemaVersion::new(1, 2, 3), SchemaVersion::new(1, 2, 0)),
            SchemaCompatibility::Compatible
        );
    }

    #[test]
    fn older_produced_minor_is_compatible() {
        // Artifact produced at 1.1, consumer built for 1.4 — the
        // consumer understands every field the older artifact has.
        assert_eq!(
            schema_compatibility(SchemaVersion::new(1, 1, 0), SchemaVersion::new(1, 4, 0)),
            SchemaCompatibility::Compatible
        );
    }

    #[test]
    fn newer_produced_minor_is_forward_compatible() {
        // Artifact produced at 1.5, consumer only built for 1.2 —
        // newer optional fields, consumer ignores them.
        assert_eq!(
            schema_compatibility(SchemaVersion::new(1, 5, 0), SchemaVersion::new(1, 2, 0)),
            SchemaCompatibility::ForwardCompatible
        );
    }

    #[test]
    fn different_major_is_incompatible() {
        assert_eq!(
            schema_compatibility(SchemaVersion::new(2, 0, 0), SchemaVersion::new(1, 9, 0)),
            SchemaCompatibility::Incompatible
        );
        assert_eq!(
            schema_compatibility(SchemaVersion::new(1, 0, 0), SchemaVersion::new(2, 0, 0)),
            SchemaCompatibility::Incompatible
        );
    }

    #[test]
    fn patch_level_never_affects_compatibility() {
        assert_eq!(
            schema_compatibility(SchemaVersion::new(1, 2, 99), SchemaVersion::new(1, 2, 0)),
            SchemaCompatibility::Compatible
        );
    }

    #[test]
    fn manifest_helpers_route_through_schema_compatibility() {
        let m = AnalysisArtifactManifest {
            schema_version: SchemaVersion::new(1, 3, 0),
            ..AnalysisArtifactManifest::default()
        };
        assert_eq!(
            m.compatibility_with(SchemaVersion::new(1, 1, 0)),
            SchemaCompatibility::ForwardCompatible
        );
        assert!(m.is_readable_by(SchemaVersion::new(1, 1, 0)));
        assert!(!m.is_readable_by(SchemaVersion::new(2, 0, 0)));
    }

    /// The producer must stamp the embedded `AnalysisArtifactManifest`
    /// with the single-source-of-truth `ANALYSIS_RUN_SCHEMA.version`,
    /// never a hardcoded literal. A stale stamp (e.g. 1.0.0 while the
    /// canonical schema is 1.1.0) would make a consumer that gates on
    /// the manifest version (`compatibility_with` / `is_readable_by`)
    /// mis-classify a real 1.1.0-field-bearing artifact as Compatible
    /// for an older minor instead of ForwardCompatible — silently
    /// trusting fields it may not understand. Guards against drift
    /// when the schema advances (oracle-ajm2.19).
    #[test]
    fn producer_stamps_manifest_with_canonical_schema_version() {
        use crate::ANALYSIS_RUN_SCHEMA;
        let run = analyze_project(AnalysisRequest::default()).expect("empty run ok");
        assert_eq!(
            run.artifacts.schema_version,
            ANALYSIS_RUN_SCHEMA.version,
            "embedded manifest schema_version must equal ANALYSIS_RUN_SCHEMA.version, \
             not a hardcoded literal"
        );
    }

    /// D2 Phase 1: lowering routine-body SQL/DML must produce a
    /// non-empty fact store AND Reads/Writes dep-graph edges from a
    /// package body doing SELECT + INSERT. Guards against the
    /// regression where `tree_lower` only emitted Declaration shells
    /// (fact_store empty, edges Calls-only). Gated on the real ANTLR
    /// backend (always wired now).
    #[test]
    fn d2_phase1_routine_body_sql_yields_facts_and_read_write_edges() {
        use plsql_depgraph::EdgeKind;

        let base = std::env::temp_dir().join(format!(
            "plsql-d2p1-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("pkg_orders.pkb"),
            "CREATE OR REPLACE PACKAGE BODY pkg_orders IS\n\
             \x20 PROCEDURE sync_orders IS\n\
             \x20   v_total NUMBER;\n\
             \x20 BEGIN\n\
             \x20   SELECT COUNT(*) INTO v_total FROM raw_orders;\n\
             \x20   INSERT INTO orders_summary (total) VALUES (v_total);\n\
             \x20 END sync_orders;\n\
             END pkg_orders;\n/\n",
        )
        .unwrap();

        let run = analyze_project(AnalysisRequest {
            project_root: proj.clone(),
            ..AnalysisRequest::default()
        })
        .expect("analyze ok");

        // Fact store must be non-empty (Declaration + DependencyEdge).
        assert!(
            !run.fact_store.facts.is_empty(),
            "fact_store must be non-empty after routine-body lowering, got {}",
            run.fact_store.facts.len()
        );
        assert_eq!(
            run.fact_store.facts.len(),
            run.fact_store.fact_count,
            "facts list length must equal fact_count"
        );

        // The dep graph must carry a Reads edge (SELECT FROM
        // raw_orders) AND a Writes edge (INSERT INTO orders_summary).
        let kinds: Vec<EdgeKind> = run.dep_graph.edges.iter().map(|e| e.kind).collect();
        assert!(
            kinds.contains(&EdgeKind::Reads),
            "expected a Reads edge from SELECT FROM raw_orders, edges={:?}",
            run.dep_graph
                .edges
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        );
        assert!(
            kinds.contains(&EdgeKind::Writes),
            "expected a Writes edge from INSERT INTO orders_summary, edges={:?}",
            run.dep_graph
                .edges
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// oracle-qm3q.9 regression: when a dropped top-level declaration
    /// (a CREATE TABLE DDL, which `lower_top_level` filters out) precedes
    /// two real procedures, the parser-parallel `ast.body_statements`
    /// array no longer aligns positionally with the *filtered*
    /// `lowered.declarations`. The old code indexed `body_statements`
    /// with the lowered loop index, so P2 (lowered[1]) picked up P1's
    /// body (body_statements[1], because the DDL occupies body_statements[0]),
    /// and P2 was attributed a Writes edge to P1's table. Pairing by span
    /// fixes it: P2 must write its OWN table (T2), never P1's (T1).
    #[test]
    fn qm3q9_ddl_before_two_procs_pairs_bodies_by_span_not_index() {
        use plsql_depgraph::EdgeKind;

        let base = std::env::temp_dir().join(format!(
            "plsql-qm3q9-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        // A DDL (dropped by lowering) ahead of two procedures, each
        // writing a DISTINCT table. The DDL slot lands in
        // body_statements[0], shifting the later (non-empty) bodies
        // relative to the filtered lowered.declarations.
        std::fs::write(
            proj.join("deploy.sql"),
            "CREATE TABLE foo (id NUMBER);\n/\n\
             CREATE OR REPLACE PROCEDURE p1 IS BEGIN INSERT INTO t1 VALUES(1); END;\n/\n\
             CREATE OR REPLACE PROCEDURE p2 IS BEGIN INSERT INTO t2 VALUES(2); END;\n/\n",
        )
        .unwrap();

        let run = analyze_project(AnalysisRequest {
            project_root: proj.clone(),
            ..AnalysisRequest::default()
        })
        .expect("analyze ok");

        // Map every Writes edge to (caller logical-id, table logical-id).
        let writes: Vec<(String, String)> = run
            .dep_graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Writes)
            .filter_map(|e| {
                let from = run.dep_graph.nodes.get(&e.from)?;
                let to = run.dep_graph.nodes.get(&e.to)?;
                Some((
                    from.logical_id.as_str().to_string(),
                    to.logical_id.as_str().to_string(),
                ))
            })
            .collect();

        // P2 must write T2 (its own body) ...
        assert!(
            writes
                .iter()
                .any(|(caller, table)| caller == "P2" && table == "T2"),
            "P2 must Writes its own table T2, writes={writes:?}"
        );
        // ... and must NOT be credited with P1's table T1 (the bug).
        assert!(
            !writes
                .iter()
                .any(|(caller, table)| caller == "P2" && table == "T1"),
            "P2 must NOT be attributed a Writes edge to P1's table T1, writes={writes:?}"
        );
        // Symmetrically, P1 keeps its own table and never picks up T2.
        assert!(
            writes
                .iter()
                .any(|(caller, table)| caller == "P1" && table == "T1"),
            "P1 must Writes its own table T1, writes={writes:?}"
        );
        assert!(
            !writes
                .iter()
                .any(|(caller, table)| caller == "P1" && table == "T2"),
            "P1 must NOT be attributed a Writes edge to T2, writes={writes:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The CompletenessReport must tell the TRUTH. A tree full of
    /// garbage the classifier cannot lower MUST NOT present as
    /// clean — and it must surface the unrecognized-object +
    /// diagnostic counts; the not-yet-wired gap metrics must
    /// serialise honestly as `unmeasured`, never 0.
    #[test]
    fn bh4p_low_extraction_run_reports_honest_non_clean_completeness() {
        let base = std::env::temp_dir().join(format!(
            "plsql-bh4p-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        // Content the AST classifier cannot lower into a known
        // top-level object → IR_UNCLASSIFIED_DECL diagnostics.
        for i in 0..6 {
            std::fs::write(
                proj.join(format!("junk_{i}.sql")),
                "@@##  not plsql at all $$ %% \n???\n/\n",
            )
            .unwrap();
        }

        let run = analyze_project(AnalysisRequest {
            project_root: proj.clone(),
            ..AnalysisRequest::default()
        })
        .expect("analyze ok");

        let c = &run.completeness;
        // A tree the engine could not turn into any lowered object
        // MUST NOT present as Clean — it reads Degraded (nothing
        // understood), the strongest honest signal.
        assert_ne!(
            c.posture,
            plsql_core::CompletenessPosture::Clean,
            "garbage tree must NOT present as Clean (got {:?}, unrecognized={}, diags={}, objs={})",
            c.posture,
            c.objects_unrecognized,
            c.diagnostics_total,
            c.objects_total,
        );
        assert!(
            matches!(
                c.posture,
                plsql_core::CompletenessPosture::Degraded
                    | plsql_core::CompletenessPosture::LowConfidence
            ),
            "garbage tree must read Degraded/LowConfidence, got {:?}",
            c.posture
        );
        assert!(
            c.diagnostics_total > 0,
            "garbage tree must surface diagnostics, got {}",
            c.diagnostics_total
        );
        assert_eq!(
            c.diagnostics_total,
            run.diagnostics.len(),
            "diagnostics_total must equal the real diagnostic count"
        );
        // Not-yet-wired gap metrics must read honestly, never 0.
        assert_eq!(c.dynamic_sql_sites, plsql_core::Measured::Unmeasured);
        assert_eq!(c.unresolved_references, plsql_core::Measured::Unmeasured);
        let v = serde_json::to_value(c).expect("serializes");
        assert_eq!(
            v["dynamic_sql_sites"],
            serde_json::json!({"unmeasured": true})
        );
        assert_ne!(v["dynamic_sql_sites"], serde_json::json!(0));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Honesty cuts both ways: a clean synthetic corpus must still
    /// read healthy (NOT Degraded) — a fully-lowered tree with no
    /// unrecognized objects is Clean or Partial, never LowConfidence.
    #[test]
    fn bh4p_clean_corpus_stays_honest_but_healthy() {
        let base = std::env::temp_dir().join(format!(
            "plsql-bh4p-clean-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("pkg_a.sql"),
            "CREATE OR REPLACE PACKAGE PKG_A AS\n  PROCEDURE do_work;\nEND PKG_A;\n/\n",
        )
        .unwrap();
        std::fs::write(
            proj.join("pkg_b.sql"),
            "CREATE OR REPLACE PACKAGE BODY PKG_B AS\n  PROCEDURE run IS\n  BEGIN\n    PKG_A.do_work;\n  END run;\nEND PKG_B;\n/\n",
        )
        .unwrap();

        let run = analyze_project(AnalysisRequest {
            project_root: proj.clone(),
            ..AnalysisRequest::default()
        })
        .expect("analyze ok");

        let c = &run.completeness;
        assert_eq!(
            c.objects_unrecognized, 0,
            "clean corpus has no unrecognized objects, got {}",
            c.objects_unrecognized
        );
        assert!(
            matches!(
                c.posture,
                plsql_core::CompletenessPosture::Clean | plsql_core::CompletenessPosture::Partial
            ),
            "clean corpus must read healthy (Clean/Partial), got {:?}",
            c.posture
        );
        assert!(
            c.extracted_semantics_ratio >= 0.99,
            "clean corpus extraction ratio should be ~1.0, got {}",
            c.extracted_semantics_ratio
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Regression: analysing the minimized public
    /// `SELECT … FOR UPDATE` fixture must NOT abort (no
    /// stack-overflow / SIGABRT). It must complete and surface the
    /// typed `AnalysisRecursionLimit` degradation with provenance,
    /// and the completeness posture must NOT read Clean (no
    /// silently hiding the truncation).
    #[test]
    fn oracle_v4wa_for_update_degrades_instead_of_stack_overflow() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/synthetic/regressions/oracle_v4wa_for_update");
        assert!(
            root.is_dir(),
            "minimized regression fixture missing at {}",
            root.display()
        );

        let req = AnalysisRequest {
            project_root: root,
            ..AnalysisRequest::default()
        };
        // The bug was a process abort — reaching this assert at
        // all proves the recursion is now bounded.
        let run = analyze_project(req).expect("analyze must not error/abort");

        let recursion_diags: Vec<_> = run
            .diagnostics
            .iter()
            .filter(|d| d.code == "ENG_ANALYSIS_RECURSION_LIMIT")
            .collect();
        assert!(
            !recursion_diags.is_empty(),
            "the non-shrinking FOR UPDATE unit must surface a typed \
             recursion-limit diagnostic, diagnostics={:?}",
            run.diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            recursion_diags.iter().any(|d| d
                .unknown_reasons
                .contains(&plsql_core::UnknownReason::AnalysisRecursionLimit)),
            "diagnostic must carry the typed UnknownReason"
        );
        assert!(
            recursion_diags
                .iter()
                .any(|d| d.message.contains("PKG_FOR_UPDATE")),
            "diagnostic must name the offending unit (provenance)"
        );
        assert_ne!(
            run.completeness.posture,
            plsql_core::CompletenessPosture::Clean,
            "a unit degraded at the recursion cap must NOT read Clean"
        );
    }
}
