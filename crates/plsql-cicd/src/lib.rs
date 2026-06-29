#![forbid(unsafe_code)]

//! Foundational types for the CI/CD recompilation cascade (Layer 5).
//!
//! See `plan.md` §15 (CI/CD Recompilation Cascade). This file
//! intentionally defines types only — `predict`, `plan`, `gate`, and
//! `verify` live in their own modules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use plsql_catalog::Hash;
use plsql_core::{
    CompletenessReport, Confidence, ObjectName, SchemaName, SymbolInterner, UnknownReason,
};
use plsql_lineage::{
    BodyChange, ChangeRecord, ColumnChangeDetail, DdlChange, SemanticChangeSet, TypeChangeDetail,
    classify_dir_diff, classify_git_diff, parse_change_file, parse_unified_diff,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

pub mod doctor;
pub mod explain;
pub mod gate;
pub mod inspector;
pub mod plan;
pub mod post_pr_comment;
pub mod predict;
#[cfg(feature = "live-xe")]
pub mod verify;

// Re-export the post-pr-comment library (PLSQL-CICD-015 / oracle-0ean)
// + idempotent find-existing helper (PLSQL-CICD-016 / oracle-usq7)
// + PR-integration doctor (PLSQL-CICD-022 / oracle-lcxu).
pub use doctor::{ChangesetDoctorReport, DoctorKindRow, DoctorReasonRow, doctor_report};
pub use explain::{
    EXPLAIN_LIFECYCLE_SCHEMA, ExplainLifecycleReport, ExplainLifecycleRow, explain_lifecycle,
    explain_lifecycle_envelope,
};
pub use gate::{
    GateDecision, GateError, GateFailure, GatePolicy, GatePolicySummary, MinConfidence, PrComment,
    PrCommentEnvelope, parse_policy, render_pr_comment, run_gate,
};
#[cfg(feature = "live-xe")]
pub use inspector::CicdOracleInspector;
pub use inspector::is_read_only_sql;
pub use plan::plan_changeset;
pub use post_pr_comment::{
    Platform, PostPrCommentRequest, PrCommentCheck, PrCoordinates, PrIntegrationDoctorInputs,
    PrIntegrationDoctorReport, PrPosture, build_request as build_post_pr_comment_request,
    find_existing_comment, pr_integration_doctor,
};
pub use predict::{
    CHANGE_IMPACT_SCHEMA, ChangeImpactCompileErrorFlag, ChangeImpactEnvelope,
    ChangeImpactInvalidation, ChangeImpactKindCount, ChangeImpactLineageNote, ChangeImpactPayload,
    ChangeImpactRecompileItem, ChangeImpactSummary, ChangeImpactUncertainty, LineageObjectMetadata,
    change_impact_envelope, change_impact_payload, predict, predict_with_lineage,
};
#[cfg(feature = "live-xe")]
pub use verify::{
    ScratchSchemaGuard, StatementOutcome, VerifyChangeset, VerifyError, VerifyOptions,
    VerifyReport, VerifyReportRow, VerifyStatement, create_scratch_schema, is_scratch_schema,
    scratch_schema_name, scratch_schema_name_for_pid, verify,
};

/// Where a `ChangeSet` was derived from.
///
/// `predict` and `plan` accept any of these forms (see plan.md §15.2). The
/// origin is preserved for evidence and reproducibility.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChangeSetOrigin {
    /// A staged directory of DDL / PL/SQL files representing what is about to
    /// be deployed.
    Directory { path: PathBuf },
    /// A pair of before/after directories.
    BeforeAfterDirectories { before: PathBuf, after: PathBuf },
    /// A Git diff range (`<base>..<head>`).
    GitDiff { range: String },
    /// A standalone DDL change-script file.
    DdlScript { path: PathBuf },
    /// A pair of catalog snapshots to diff (`before.json`, `after.json`).
    CatalogSnapshotDiff { before: PathBuf, after: PathBuf },
}

/// Semantic classification of a single changed object inside a `ChangeSet`.
///
/// This mirrors the prediction-distinguishing kinds enumerated in plan.md
/// §15.2: "Prediction must distinguish: package spec change, package body-only
/// change, standalone procedure/function signature change, table additive DDL,
/// table destructive DDL, type evolution, synonym retargeting, grant/revoke,
/// editioned object change, materialized view refresh-affecting change."
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChangedObjectKind {
    PackageSpec,
    PackageBody,
    StandaloneRoutineSignature,
    StandaloneRoutineBody,
    TableAdditiveDdl,
    TableDestructiveDdl,
    ViewDefinitionChange,
    TypeEvolution,
    SynonymRetargeting,
    GrantOrRevoke,
    EditionedObjectChange,
    MaterializedViewRefreshAffecting,
    TriggerChange,
    IndexChange,
    SequenceChange,
    /// Catch-all for an object kind we recognize but do not yet classify into
    /// one of the above buckets. Distinct from `Unclassified` to preserve the
    /// dictionary-derived object type when known.
    OtherKnownKind {
        object_type: String,
    },
    /// Change classifier could not assign a kind — paired with an
    /// `UnknownReason` in `ChangedObject.uncertainties` (R13).
    Unclassified,
}

/// A single object whose definition is changing in a `ChangeSet`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangedObject {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub kind: ChangedObjectKind,
    /// SHA-256 of the new normalized DDL / source body where available.
    pub new_hash: Option<Hash>,
    /// SHA-256 of the previous normalized DDL / source body, if a before view
    /// was available (Git diff, before-after directories, catalog diff).
    pub previous_hash: Option<Hash>,
    /// Files contributing to this object change, project-relative.
    pub file_paths: Vec<PathBuf>,
    /// Per-object opacity reasons that downstream prediction must honor.
    pub uncertainties: Vec<UnknownReason>,
}

/// A proposed Oracle deployment change set.
///
/// Inputs flow into `predict`, `plan`,
/// `gate`, and `verify`. The `ChangeSet`
/// itself is a pure data structure — building a `ChangeSet` from a
/// `ChangeSetOrigin` is a separate Layer 4 lineage classifier
/// responsibility.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub origin: Option<ChangeSetOrigin>,
    pub objects: Vec<ChangedObject>,
    /// Project-relative DDL / change-script files that did not classify into
    /// a `ChangedObject`. Listed so `plan` can still order them in the
    /// resulting script.
    pub unclassified_files: Vec<PathBuf>,
}

impl ChangeSet {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.unclassified_files.is_empty()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    #[instrument(level = "trace", skip(repo))]
    pub fn from_git_diff(repo: &Path, from: &str, to: &str) -> Result<Self, CicdError> {
        let semantic = classify_git_diff(repo, from, to)?;
        Self::from_semantic_changes(
            Some(ChangeSetOrigin::GitDiff {
                range: format!("{from}..{to}"),
            }),
            semantic,
        )
    }

    #[instrument(level = "trace", skip(range, diff))]
    pub fn from_unified_diff(range: impl Into<String>, diff: &str) -> Result<Self, CicdError> {
        let semantic = parse_unified_diff(diff)?;
        Self::from_semantic_changes(
            Some(ChangeSetOrigin::GitDiff {
                range: range.into(),
            }),
            semantic,
        )
    }

    #[instrument(level = "trace")]
    pub fn from_before_after_dirs(before: &Path, after: &Path) -> Result<Self, CicdError> {
        let semantic = classify_dir_diff(before, after)?;
        Self::from_semantic_changes(
            Some(ChangeSetOrigin::BeforeAfterDirectories {
                before: before.to_path_buf(),
                after: after.to_path_buf(),
            }),
            semantic,
        )
    }

    #[instrument(level = "trace")]
    pub fn from_directory(path: &Path) -> Result<Self, CicdError> {
        let mut files = Vec::new();
        collect_plsql_paths(path, path, &mut files)?;
        let mut semantic = SemanticChangeSet::new();
        for rel in files {
            semantic.changes.push(ChangeRecord::Created {
                object_id: path_to_object_id(&rel),
            });
        }
        Self::from_semantic_changes(
            Some(ChangeSetOrigin::Directory {
                path: path.to_path_buf(),
            }),
            semantic,
        )
    }

    #[instrument(level = "trace")]
    pub fn from_ddl_script(path: &Path) -> Result<Self, CicdError> {
        if !path.exists() {
            return Err(CicdError::MissingChangeSetFile {
                path: path.to_path_buf(),
            });
        }
        Ok(Self {
            origin: Some(ChangeSetOrigin::DdlScript {
                path: path.to_path_buf(),
            }),
            objects: vec![],
            unclassified_files: vec![path.to_path_buf()],
        })
    }

    #[instrument(level = "trace")]
    pub fn from_change_file(path: &Path) -> Result<Self, CicdError> {
        let semantic = parse_change_file(path)?;
        Self::from_semantic_changes(
            Some(ChangeSetOrigin::DdlScript {
                path: path.to_path_buf(),
            }),
            semantic,
        )
    }

    #[instrument(level = "trace", skip(semantic))]
    pub fn from_semantic_changes(
        origin: Option<ChangeSetOrigin>,
        semantic: SemanticChangeSet,
    ) -> Result<Self, CicdError> {
        let mut interner = SymbolInterner::new();
        let mut objects = Vec::new();
        for record in semantic.changes {
            objects.push(changed_object_from_record(&mut interner, record)?);
        }
        objects.sort_by(|a, b| {
            (
                a.owner.symbol().get(),
                a.name.symbol().get(),
                format!("{:?}", a.kind),
            )
                .cmp(&(
                    b.owner.symbol().get(),
                    b.name.symbol().get(),
                    format!("{:?}", b.kind),
                ))
        });
        Ok(Self {
            origin,
            objects,
            unclassified_files: vec![],
        })
    }
}

fn changed_object_from_record(
    interner: &mut SymbolInterner,
    record: ChangeRecord,
) -> Result<ChangedObject, CicdError> {
    match record {
        ChangeRecord::Created { object_id } | ChangeRecord::Dropped { object_id } => {
            changed_object(
                interner,
                &object_id,
                ChangedObjectKind::Unclassified,
                None,
                None,
                vec![UnknownReason::MissingCatalogObject],
            )
        }
        ChangeRecord::Body(BodyChange {
            object_id,
            hash_before,
            hash_after,
        }) => changed_object(
            interner,
            &object_id,
            ChangedObjectKind::Unclassified,
            hash_after.map(Hash::new),
            hash_before.map(Hash::new),
            vec![UnknownReason::MissingCatalogObject],
        ),
        ChangeRecord::Signature(change) => {
            let target = signature_change_target(&change.object_id);
            changed_object(interner, &target.object_id, target.kind, None, None, vec![])
        }
        ChangeRecord::Privilege(change) => changed_object(
            interner,
            &change.object_id,
            ChangedObjectKind::GrantOrRevoke,
            None,
            None,
            vec![],
        ),
        ChangeRecord::Synonym(change) => changed_object(
            interner,
            &change.synonym_id,
            ChangedObjectKind::SynonymRetargeting,
            None,
            None,
            vec![],
        ),
        ChangeRecord::Column(change) => {
            let kind = match change.change {
                ColumnChangeDetail::Added => ChangedObjectKind::TableAdditiveDdl,
                ColumnChangeDetail::Dropped
                | ColumnChangeDetail::TypeChanged { .. }
                | ColumnChangeDetail::NullabilityChanged { .. } => {
                    ChangedObjectKind::TableDestructiveDdl
                }
            };
            changed_object(interner, &change.object_id, kind, None, None, vec![])
        }
        ChangeRecord::Type(change) => {
            let uncertainty = match change.detail {
                TypeChangeDetail::FinalityChanged | TypeChangeDetail::InstantiabilityChanged => {
                    vec![UnknownReason::MissingCatalogObject]
                }
                TypeChangeDetail::AttributeAdded { .. }
                | TypeChangeDetail::AttributeRemoved { .. } => {
                    vec![]
                }
            };
            changed_object(
                interner,
                &change.type_id,
                ChangedObjectKind::TypeEvolution,
                None,
                None,
                uncertainty,
            )
        }
        ChangeRecord::Grant(change) => changed_object(
            interner,
            &change.object_id,
            ChangedObjectKind::GrantOrRevoke,
            None,
            None,
            vec![],
        ),
        ChangeRecord::Ddl(change) => changed_object(
            interner,
            &change.object_id,
            kind_from_ddl(&change),
            None,
            None,
            vec![],
        ),
    }
}

fn changed_object(
    interner: &mut SymbolInterner,
    object_id: &str,
    kind: ChangedObjectKind,
    new_hash: Option<Hash>,
    previous_hash: Option<Hash>,
    mut uncertainties: Vec<UnknownReason>,
) -> Result<ChangedObject, CicdError> {
    let (owner_text, name_text, unqualified) = split_logical_object_id(object_id);
    if unqualified && !uncertainties.contains(&UnknownReason::MissingCatalogObject) {
        uncertainties.push(UnknownReason::MissingCatalogObject);
    }
    let owner = intern_schema(interner, &owner_text)?;
    let name = intern_object(interner, &name_text)?;
    Ok(ChangedObject {
        owner,
        name,
        kind,
        new_hash,
        previous_hash,
        file_paths: vec![],
        uncertainties,
    })
}

struct SignatureChangeTarget {
    object_id: String,
    kind: ChangedObjectKind,
}

fn signature_change_target(object_id: &str) -> SignatureChangeTarget {
    let mut parts = object_id
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    if let (Some(owner), Some(package), Some(_member)) = (parts.next(), parts.next(), parts.next())
    {
        return SignatureChangeTarget {
            object_id: format!("{owner}.{package}"),
            kind: ChangedObjectKind::PackageSpec,
        };
    }
    SignatureChangeTarget {
        object_id: object_id.to_string(),
        kind: ChangedObjectKind::StandaloneRoutineSignature,
    }
}

fn split_logical_object_id(object_id: &str) -> (String, String, bool) {
    let trimmed = object_id.trim().trim_matches('.');
    let normalized = if trimmed.is_empty() {
        "unknown"
    } else {
        trimmed
    };
    if let Some((owner, rest)) = normalized.split_once('.') {
        if !owner.trim().is_empty() && !rest.trim().is_empty() {
            return (
                owner.trim().to_ascii_uppercase(),
                rest.trim().to_ascii_uppercase(),
                false,
            );
        }
    }
    (
        "UNQUALIFIED".to_string(),
        normalized.to_ascii_uppercase(),
        true,
    )
}

fn intern_schema(interner: &mut SymbolInterner, value: &str) -> Result<SchemaName, CicdError> {
    interner
        .intern_schema_name(value)
        .ok_or_else(|| CicdError::SymbolInterningFailed {
            name: value.to_string(),
        })
}

fn intern_object(interner: &mut SymbolInterner, value: &str) -> Result<ObjectName, CicdError> {
    interner
        .intern(value)
        .map(ObjectName::from)
        .ok_or_else(|| CicdError::SymbolInterningFailed {
            name: value.to_string(),
        })
}

fn kind_from_ddl(change: &DdlChange) -> ChangedObjectKind {
    let object_type = change.object_type.trim().to_ascii_uppercase();
    let normalized = object_type.replace(' ', "_");
    match normalized.as_str() {
        "INDEX" => ChangedObjectKind::IndexChange,
        "TRIGGER" => ChangedObjectKind::TriggerChange,
        "SEQUENCE" => ChangedObjectKind::SequenceChange,
        "VIEW" => ChangedObjectKind::ViewDefinitionChange,
        "MATERIALIZED_VIEW" => ChangedObjectKind::MaterializedViewRefreshAffecting,
        "SYNONYM" => ChangedObjectKind::SynonymRetargeting,
        "TYPE" | "OBJECT_TYPE" => ChangedObjectKind::TypeEvolution,
        "PACKAGE" | "PACKAGE_SPEC" => ChangedObjectKind::PackageSpec,
        "PACKAGE_BODY" => ChangedObjectKind::PackageBody,
        "TABLE" => {
            let detail = change.detail.to_ascii_uppercase();
            if detail.contains("DROP") || detail.contains("TRUNCATE") || detail.contains("RENAME") {
                ChangedObjectKind::TableDestructiveDdl
            } else {
                ChangedObjectKind::TableAdditiveDdl
            }
        }
        _ => ChangedObjectKind::OtherKnownKind { object_type },
    }
}

fn collect_plsql_paths(
    root: &Path,
    current: &Path,
    out: &mut Vec<String>,
) -> Result<(), CicdError> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_plsql_paths(root, &path, out)?;
        } else if is_plsql_path(&path) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    out.sort();
    Ok(())
}

fn is_plsql_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "sql" | "pls" | "plsql" | "pkb" | "pks"
            )
        })
}

fn path_to_object_id(path: &str) -> String {
    let stripped = path.rsplit_once('.').map(|(base, _)| base).unwrap_or(path);
    stripped.replace('/', ".")
}

/// How a prediction was computed (plan.md §15.2). The mode determines the
/// completeness profile and which `UnknownReason`s can appear.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum PredictMode {
    /// No catalog — best-effort source-only invalidation reasoning.
    SourceOnly,
    /// A pre-existing `CatalogSnapshot` is consulted (recommended).
    #[default]
    CatalogAware,
    /// A fresh catalog snapshot is extracted before predicting.
    LiveSnapshot,
}

/// Oracle invalidation reasons attached to a predicted invalidation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InvalidationReason {
    /// Dependent of a changed package spec — invalidates on next reference.
    PackageSpecChanged {
        spec_owner: SchemaName,
        spec_name: ObjectName,
    },
    /// Dependent of a changed standalone routine signature.
    RoutineSignatureChanged {
        routine_owner: SchemaName,
        routine_name: ObjectName,
    },
    /// Table additive DDL — view / packaged routine using `%ROWTYPE` may need
    /// recompile, dependent triggers may need check.
    TableAdditive {
        table_owner: SchemaName,
        table_name: ObjectName,
    },
    /// Table destructive DDL — column drop / type narrowing / NOT NULL add.
    TableDestructive {
        table_owner: SchemaName,
        table_name: ObjectName,
    },
    /// Object type altered — requires recompile of types/packages that embed
    /// the type structurally.
    TypeEvolution {
        type_owner: SchemaName,
        type_name: ObjectName,
    },
    /// Synonym retargeted — dependents now resolve to a different object.
    SynonymRetargeted {
        synonym_owner: SchemaName,
        synonym_name: ObjectName,
    },
    /// Grant or revoke — dependents requiring the privilege may fail at
    /// runtime; objects may also be marked invalid by Oracle.
    PrivilegeChange,
    /// Materialized view refresh-affecting change.
    MaterializedViewRefreshAffected {
        mview_owner: SchemaName,
        mview_name: ObjectName,
    },
    /// Editioned object change — affects the active edition.
    EditionedObjectChange,
    /// Reason recorded by `predict --mode source-only` when the catalog would
    /// have been the authoritative answer.
    SourceOnlyHeuristic,
    /// Conservative fallback — invalidation listed but cause is not narrowed.
    Other { description: String },
}

/// A single invalidation row inside an `InvalidationPrediction`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PredictedInvalidation {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: String,
    pub reason: InvalidationReason,
    pub confidence: Confidence,
    /// Hop distance from the originating changed object — 1 is direct, >1 is
    /// transitive. `predict` orders the output stably by `(distance, owner,
    /// name)` so reports diff cleanly.
    pub distance: u32,
}

/// Object-level recompile guidance — feeds `DeploymentPlan.recompile_order`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecompileItem {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: String,
    /// `true` when the object also needs an explicit `ALTER ... COMPILE`
    /// statement rather than relying on lazy validation.
    pub force_compile: bool,
}

/// One opacity / blind-spot record in a prediction (R13). Each blind spot is
/// typed (via `UnknownReason`) and never silently dropped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UncertaintyRecord {
    pub reason: UnknownReason,
    pub affected_owner: Option<SchemaName>,
    pub affected_name: Option<ObjectName>,
    pub description: String,
}

/// The prediction artifact for a `ChangeSet`.
///
/// Consumed by `plsql cicd plan`, `plsql cicd gate`, and
/// `plsql cicd verify`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InvalidationPrediction {
    pub mode: PredictMode,
    pub predicted_invalidations: Vec<PredictedInvalidation>,
    pub recompile_order: Vec<RecompileItem>,
    pub uncertainties: Vec<UncertaintyRecord>,
    /// `CompletenessReport` snapshot for the prediction run. Per plan.md
    /// §15.2 every prediction mode "emits its completeness profile".
    pub completeness: CompletenessReport,
    pub attributes: BTreeMap<String, String>,
}

impl InvalidationPrediction {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn empty(mode: PredictMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn invalidation_count(&self) -> usize {
        self.predicted_invalidations.len()
    }
}

/// One DDL / DML statement in a `DeploymentPlan.statements` list.
///
/// Statements are emitted in topological order respecting both the
/// changeset's own DDL dependencies and Oracle's invalidation cascade.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeploymentStatement {
    /// Stable monotonically-increasing ordinal in the plan. The first
    /// statement is `1`.
    pub ordinal: u32,
    pub kind: DeploymentStatementKind,
    /// SQL text as it will appear in the generated deployment script (no
    /// trailing semicolon — `plan` adds the terminator appropriate for the
    /// target dialect).
    pub sql: String,
    /// Project-relative source file the statement was lowered from. `None`
    /// for synthesized statements (recompiles, sanity checks).
    pub source_file: Option<PathBuf>,
    /// Owner / name when the statement targets a single object.
    pub target_owner: Option<SchemaName>,
    pub target_name: Option<ObjectName>,
}

/// What kind of operation a `DeploymentStatement` represents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeploymentStatementKind {
    /// `CREATE OR REPLACE PACKAGE` / `CREATE TABLE` / etc. The semantic kind
    /// of DDL is captured in the matching `ChangedObject`.
    Ddl,
    /// `ALTER ... COMPILE` issued because invalidation prediction said so.
    Recompile,
    /// `GRANT` / `REVOKE` reordered into the plan.
    GrantOrRevoke,
    /// A sanity-check select / call that `verify` may inject (e.g. checking
    /// row counts after a destructive DDL).
    SanityCheck,
    /// Any other statement type (PL/SQL anonymous block, MERGE, etc.).
    Other,
}

/// Risk classification for the overall deployment plan — informs `gate`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeploymentRisk {
    /// No destructive DDL, no opaque invalidations, no missing catalog.
    Safe,
    /// Some uncertainty present (e.g. opaque dynamic SQL touches a changed
    /// object), but no destructive DDL.
    Caution,
    /// Destructive DDL or `permanently_read_only` violations — gate fails by
    /// default.
    Destructive,
    /// Could not classify (e.g. classifier missing inputs); `gate` treats as
    /// blocker until explicitly cleared.
    #[default]
    Unknown,
}

/// The deliverable artifact for a deployment.
///
/// `plan <changeset>` emits this; `verify` consumes it; `gate` summarizes its
/// risk + uncertainty for the PR comment surface (`plan.md` §15.7).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub changeset: ChangeSet,
    pub prediction: InvalidationPrediction,
    pub statements: Vec<DeploymentStatement>,
    pub overall_risk: DeploymentRisk,
    /// Free-form per-deployment notes (e.g. "requires session restart",
    /// "depends on patched-in privilege"). Each note is short, single-line.
    pub notes: Vec<String>,
}

impl DeploymentPlan {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn statement_count(&self) -> usize {
        self.statements.len()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_destructive(&self) -> bool {
        matches!(self.overall_risk, DeploymentRisk::Destructive)
    }
}

/// Errors that the future Layer 5 operations (`predict`, `plan`, `gate`,
/// `verify`) will raise. Kept here so downstream crates can build on a
/// stable error surface without re-introducing one.
#[derive(Debug, Error)]
pub enum CicdError {
    #[error("changeset has no inputs")]
    EmptyChangeSet,
    #[error("changeset references missing source file `{path}`")]
    MissingChangeSetFile { path: PathBuf },
    #[error("predict requested mode `{requested:?}` but the run is gated to `{allowed:?}`")]
    DisallowedPredictMode {
        requested: PredictMode,
        allowed: PredictMode,
    },
    #[error("deployment plan has unresolved statement ordinals")]
    UnorderedDeploymentPlan,
    #[error("io error while loading changeset: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error while loading changeset: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("change classifier error while loading changeset: {0}")]
    Classify(#[from] plsql_lineage::ClassifyError),
    #[error("symbol table overflow while interning `{name}`")]
    SymbolInterningFailed { name: String },
    #[error("CicdOracleInspector refuses non-read-only SQL (preview: `{preview}`)")]
    DisallowedWriteSqlInInspector { preview: String },
    #[error("oracle backend error: {message}")]
    OracleBackendError { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::SymbolInterner;

    fn billing_owner() -> (SymbolInterner, SchemaName, ObjectName) {
        let mut interner = SymbolInterner::new();
        let owner = interner.intern_schema_name("BILLING").expect("schema name");
        let name = interner
            .intern("BILLING_API")
            .map(ObjectName::from)
            .expect("object name");
        (interner, owner, name)
    }

    #[test]
    fn changeset_starts_empty() {
        let changeset = ChangeSet::empty();
        assert!(changeset.is_empty());
        assert_eq!(changeset.object_count(), 0);
        assert!(changeset.origin.is_none());
    }

    #[test]
    fn changeset_classifies_objects() {
        let (_interner, owner, name) = billing_owner();
        let object = ChangedObject {
            owner,
            name,
            kind: ChangedObjectKind::PackageSpec,
            new_hash: None,
            previous_hash: None,
            file_paths: vec![PathBuf::from("billing/api_spec.sql")],
            uncertainties: vec![],
        };
        let changeset = ChangeSet {
            origin: Some(ChangeSetOrigin::Directory {
                path: PathBuf::from("staging"),
            }),
            objects: vec![object],
            unclassified_files: vec![],
        };

        assert!(!changeset.is_empty());
        assert_eq!(changeset.object_count(), 1);
        assert!(matches!(
            changeset.objects[0].kind,
            ChangedObjectKind::PackageSpec
        ));
    }

    #[test]
    fn unclassified_kind_carries_object_type_label() {
        let kind = ChangedObjectKind::OtherKnownKind {
            object_type: String::from("DIRECTORY"),
        };
        let object_type = match &kind {
            ChangedObjectKind::OtherKnownKind { object_type } => Some(object_type.as_str()),
            _ => None,
        };
        assert_eq!(object_type, Some("DIRECTORY"));
    }

    #[test]
    fn invalidation_prediction_default_mode_is_catalog_aware() {
        let prediction = InvalidationPrediction::default();
        assert!(matches!(prediction.mode, PredictMode::CatalogAware));
        assert_eq!(prediction.invalidation_count(), 0);
    }

    #[test]
    fn deployment_plan_marks_destructive_correctly() {
        let mut plan = DeploymentPlan::empty();
        assert!(!plan.is_destructive());
        plan.overall_risk = DeploymentRisk::Destructive;
        assert!(plan.is_destructive());
    }

    #[test]
    fn deployment_plan_statements_have_stable_ordinals() {
        let (_interner, owner, name) = billing_owner();
        let plan = DeploymentPlan {
            statements: vec![
                DeploymentStatement {
                    ordinal: 1,
                    kind: DeploymentStatementKind::Ddl,
                    sql: String::from("CREATE OR REPLACE PACKAGE BILLING_API AS END"),
                    source_file: Some(PathBuf::from("billing/api_spec.sql")),
                    target_owner: Some(owner),
                    target_name: Some(name),
                },
                DeploymentStatement {
                    ordinal: 2,
                    kind: DeploymentStatementKind::Recompile,
                    sql: String::from("ALTER PACKAGE BILLING.BILLING_API COMPILE BODY"),
                    source_file: None,
                    target_owner: Some(owner),
                    target_name: Some(name),
                },
            ],
            ..DeploymentPlan::empty()
        };

        assert_eq!(plan.statement_count(), 2);
        assert_eq!(plan.statements[0].ordinal, 1);
        assert_eq!(plan.statements[1].ordinal, 2);
        let ddl_first = matches!(plan.statements[0].kind, DeploymentStatementKind::Ddl);
        let recompile_second =
            matches!(plan.statements[1].kind, DeploymentStatementKind::Recompile);
        assert!(ddl_first);
        assert!(recompile_second);
    }

    #[test]
    fn prediction_records_uncertainty_with_typed_reason() {
        let prediction = InvalidationPrediction {
            mode: PredictMode::SourceOnly,
            uncertainties: vec![UncertaintyRecord {
                reason: UnknownReason::DynamicSqlOpaque,
                affected_owner: None,
                affected_name: None,
                description: String::from("EXECUTE IMMEDIATE on a procedure arg"),
            }],
            ..InvalidationPrediction::default()
        };

        assert!(matches!(prediction.mode, PredictMode::SourceOnly));
        assert_eq!(prediction.uncertainties.len(), 1);
        assert!(matches!(
            prediction.uncertainties[0].reason,
            UnknownReason::DynamicSqlOpaque
        ));
    }

    #[test]
    fn changeset_serializes_round_trip() {
        let (_interner, owner, name) = billing_owner();
        let changeset = ChangeSet {
            origin: Some(ChangeSetOrigin::GitDiff {
                range: String::from("main..feature-branch"),
            }),
            objects: vec![ChangedObject {
                owner,
                name,
                kind: ChangedObjectKind::TableDestructiveDdl,
                new_hash: Some(Hash::new("sha256:deadbeef")),
                previous_hash: None,
                file_paths: vec![PathBuf::from("billing/customers.sql")],
                uncertainties: vec![],
            }],
            unclassified_files: vec![],
        };

        let serialized = serde_json::to_string(&changeset).expect("serialize");
        let deserialized: ChangeSet = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(changeset, deserialized);
    }

    fn fixture_root(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        PathBuf::from("target")
            .join("tmp")
            .join(format!("plsql-cicd-{label}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn package_member_signature_targets_package_spec() {
        let package_member = signature_change_target("billing.billing_api.process_payment");
        assert_eq!(package_member.object_id, "billing.billing_api");
        assert!(matches!(
            package_member.kind,
            ChangedObjectKind::PackageSpec
        ));

        let standalone = signature_change_target("billing.reprice_invoice");
        assert_eq!(standalone.object_id, "billing.reprice_invoice");
        assert!(matches!(
            standalone.kind,
            ChangedObjectKind::StandaloneRoutineSignature
        ));
    }

    #[test]
    fn changeset_from_unified_diff_builds_structural_objects() {
        let diff = "diff --git a/billing/pkg_api.pkb b/billing/pkg_api.pkb\n--- a/billing/pkg_api.pkb\n+++ b/billing/pkg_api.pkb\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/billing/new_pkg.pks b/billing/new_pkg.pks\n--- /dev/null\n+++ b/billing/new_pkg.pks\n@@ -0,0 +1 @@\n+CREATE PACKAGE new_pkg AS END;\ndiff --git a/billing/old_pkg.sql b/billing/old_pkg.sql\n--- a/billing/old_pkg.sql\n+++ /dev/null\n@@ -1 +0,0 @@\n-DROP ME\ndiff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-a\n+b\n";

        let changeset = ChangeSet::from_unified_diff("main..feature", diff)
            .expect("fixture diff should classify");

        assert!(matches!(
            &changeset.origin,
            Some(ChangeSetOrigin::GitDiff { range }) if range == "main..feature"
        ));
        assert_eq!(changeset.objects.len(), 3);
        assert!(changeset.unclassified_files.is_empty());
        assert!(
            changeset
                .objects
                .iter()
                .all(|object| matches!(object.kind, ChangedObjectKind::Unclassified))
        );
        let body = changeset
            .objects
            .iter()
            .find(|object| object.previous_hash == Some(Hash::new("diff:-1")))
            .expect("modified package body should preserve diff hash");
        assert_eq!(body.new_hash, Some(Hash::new("diff:+1")));
        assert!(
            body.uncertainties
                .contains(&UnknownReason::MissingCatalogObject)
        );
    }

    #[test]
    fn changeset_from_before_after_dirs_uses_lineage_directory_diff() {
        let root = fixture_root("dirs");
        let before = root.join("before");
        let after = root.join("after");
        std::fs::create_dir_all(before.join("billing")).expect("before dir");
        std::fs::create_dir_all(after.join("billing")).expect("after dir");
        std::fs::write(before.join("billing/pkg_api.pkb"), "old").expect("old body");
        std::fs::write(after.join("billing/pkg_api.pkb"), "new").expect("new body");
        std::fs::write(before.join("billing/old_pkg.pks"), "old").expect("old spec");
        std::fs::write(after.join("billing/new_pkg.pks"), "new").expect("new spec");
        std::fs::write(after.join("billing/readme.md"), "ignored").expect("ignored");

        let changeset =
            ChangeSet::from_before_after_dirs(&before, &after).expect("dir diff changeset");

        assert!(matches!(
            &changeset.origin,
            Some(ChangeSetOrigin::BeforeAfterDirectories { before: b, after: a })
                if b == &before && a == &after
        ));
        assert_eq!(changeset.objects.len(), 3);
        assert_eq!(
            changeset
                .objects
                .iter()
                .filter(|object| object.previous_hash.is_some() && object.new_hash.is_some())
                .count(),
            1
        );
    }

    #[test]
    fn changeset_from_directory_and_script_preserve_origins() {
        let root = fixture_root("inputs");
        let staged = root.join("staged");
        let scripts = root.join("scripts");
        std::fs::create_dir_all(staged.join("billing")).expect("staged dir");
        std::fs::create_dir_all(&scripts).expect("scripts dir");
        std::fs::write(
            staged.join("billing/pkg_api.pks"),
            "CREATE PACKAGE p AS END;",
        )
        .expect("staged package");
        std::fs::write(staged.join("billing/readme.md"), "ignored").expect("ignored file");
        let script = scripts.join("deploy.sql");
        std::fs::write(&script, "ALTER TABLE billing.customers ADD x NUMBER;").expect("script");

        let directory_changeset = ChangeSet::from_directory(&staged).expect("directory changeset");
        assert!(matches!(
            &directory_changeset.origin,
            Some(ChangeSetOrigin::Directory { path }) if path == &staged
        ));
        assert_eq!(directory_changeset.objects.len(), 1);

        let script_changeset = ChangeSet::from_ddl_script(&script).expect("script changeset");
        assert!(matches!(
            &script_changeset.origin,
            Some(ChangeSetOrigin::DdlScript { path }) if path == &script
        ));
        assert!(script_changeset.objects.is_empty());
        assert_eq!(script_changeset.unclassified_files, vec![script]);
    }

    #[test]
    fn semantic_changes_map_precise_records_to_cicd_kinds() {
        let mut semantic = SemanticChangeSet::new();
        semantic.push(ChangeRecord::Column(plsql_lineage::ColumnChange {
            object_id: "billing.customers".into(),
            column_name: "legacy_segment".into(),
            change: ColumnChangeDetail::Dropped,
        }));
        semantic.push(ChangeRecord::Synonym(plsql_lineage::SynonymChange {
            synonym_id: "billing.syn_customers".into(),
            target_before: Some("old.customers".into()),
            target_after: Some("billing.customers".into()),
        }));
        semantic.push(ChangeRecord::Grant(plsql_lineage::GrantChange {
            object_id: "billing.customers".into(),
            grantee: "app".into(),
            privilege: "select".into(),
            action: plsql_lineage::GrantAction::Granted,
        }));
        semantic.push(ChangeRecord::Ddl(plsql_lineage::DdlChange {
            object_id: "billing.ix_customers".into(),
            object_type: "INDEX".into(),
            detail: "rebuilt".into(),
        }));

        let changeset = ChangeSet::from_semantic_changes(None, semantic)
            .expect("semantic changeset should lower");
        let kinds = changeset
            .objects
            .iter()
            .map(|object| &object.kind)
            .collect::<Vec<_>>();
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, ChangedObjectKind::TableDestructiveDdl))
        );
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, ChangedObjectKind::SynonymRetargeting))
        );
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, ChangedObjectKind::GrantOrRevoke))
        );
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, ChangedObjectKind::IndexChange))
        );
    }

    struct SourceEditFixture {
        label: &'static str,
        before_source: &'static str,
        after_source: &'static str,
        semantic: SemanticChangeSet,
        expected_kind: ChangedObjectKind,
    }

    fn package_spec_source_edit_fixture() -> SourceEditFixture {
        let before_source = r#"
CREATE OR REPLACE PACKAGE billing.billing_api AS
  PROCEDURE process_payment(p_id NUMBER);
END billing_api;
"#;
        let after_source = r#"
CREATE OR REPLACE PACKAGE billing.billing_api AS
  PROCEDURE process_payment(p_id NUMBER, p_amount NUMBER);
END billing_api;
"#;
        let mut semantic = SemanticChangeSet::new();
        semantic.push(ChangeRecord::Signature(plsql_lineage::SignatureChange {
            object_id: "billing.billing_api.process_payment".into(),
            old_signature: Some("process_payment(p_id NUMBER)".into()),
            new_signature: Some("process_payment(p_id NUMBER, p_amount NUMBER)".into()),
        }));
        SourceEditFixture {
            label: "package-spec-signature",
            before_source,
            after_source,
            semantic,
            expected_kind: ChangedObjectKind::PackageSpec,
        }
    }

    fn column_type_source_edit_fixture() -> SourceEditFixture {
        let before_source = r#"
CREATE TABLE billing.customers (
  customer_id NUMBER PRIMARY KEY,
  credit_limit NUMBER(9,2)
);
"#;
        let after_source = r#"
CREATE TABLE billing.customers (
  customer_id NUMBER PRIMARY KEY,
  credit_limit VARCHAR2(20)
);
"#;
        let mut semantic = SemanticChangeSet::new();
        semantic.push(ChangeRecord::Column(plsql_lineage::ColumnChange {
            object_id: "billing.customers".into(),
            column_name: "credit_limit".into(),
            change: ColumnChangeDetail::TypeChanged {
                old_type: Some("NUMBER(9,2)".into()),
                new_type: Some("VARCHAR2(20)".into()),
            },
        }));
        SourceEditFixture {
            label: "column-type-change",
            before_source,
            after_source,
            semantic,
            expected_kind: ChangedObjectKind::TableDestructiveDdl,
        }
    }

    #[test]
    fn source_edit_semantic_fixtures_map_to_expected_changesets() {
        assert_source_edit_fixture_maps(package_spec_source_edit_fixture());
        assert_source_edit_fixture_maps(column_type_source_edit_fixture());
    }

    fn assert_source_edit_fixture_maps(fixture: SourceEditFixture) {
        assert_ne!(fixture.before_source, fixture.after_source);
        assert!(fixture.before_source.contains("CREATE"));
        assert!(fixture.after_source.contains("CREATE"));

        let before_path = PathBuf::from(format!("fixtures/{}/before", fixture.label));
        let after_path = PathBuf::from(format!("fixtures/{}/after", fixture.label));
        let changeset = ChangeSet::from_semantic_changes(
            Some(ChangeSetOrigin::BeforeAfterDirectories {
                before: before_path.clone(),
                after: after_path.clone(),
            }),
            fixture.semantic,
        )
        .expect("source-edit semantic fixture should lower to a ChangeSet");

        assert!(matches!(
            &changeset.origin,
            Some(ChangeSetOrigin::BeforeAfterDirectories { before, after })
                if before == &before_path && after == &after_path
        ));
        assert_eq!(changeset.objects.len(), 1, "{}", fixture.label);
        let object = changeset
            .objects
            .first()
            .expect("fixture should produce one changed object");
        assert_eq!(object.kind, fixture.expected_kind, "{}", fixture.label);
        assert_eq!(object.owner.symbol().get(), 0, "{}", fixture.label);
        assert_eq!(object.name.symbol().get(), 1, "{}", fixture.label);
        assert!(object.uncertainties.is_empty(), "{}", fixture.label);
        assert!(changeset.unclassified_files.is_empty(), "{}", fixture.label);
    }

    #[test]
    fn cicd_error_displays_descriptive_message() {
        let error = CicdError::EmptyChangeSet;
        assert_eq!(format!("{error}"), "changeset has no inputs");
    }
}
