#![forbid(unsafe_code)]

//! Foundational types for the CI/CD recompilation cascade (Layer 5).
//!
//! See `plan.md` §15 (CI/CD Recompilation Cascade) and `PLSQL-CICD-001` for
//! the bead this crate seeds. This file intentionally defines types only —
//! `predict`, `plan`, `gate`, and `verify` land in their own beads
//! (`PLSQL-CICD-002`..`PLSQL-CICD-010`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use plsql_catalog::Hash;
use plsql_core::{CompletenessReport, Confidence, ObjectName, SchemaName, UnknownReason};
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
pub use inspector::{CicdOracleInspector, is_read_only_sql};
pub use plan::plan_changeset;
pub use post_pr_comment::{
    Platform, PostPrCommentRequest, PrCommentCheck, PrCoordinates, PrIntegrationDoctorInputs,
    PrIntegrationDoctorReport, PrPosture, build_request as build_post_pr_comment_request,
    find_existing_comment, pr_integration_doctor,
};
pub use predict::predict;
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
/// Inputs flow into `predict` (PLSQL-CICD-002), `plan` (PLSQL-CICD-003),
/// `gate` (PLSQL-CICD-006), and `verify` (PLSQL-CICD-005). The `ChangeSet`
/// itself is a pure data structure — building a `ChangeSet` from a
/// `ChangeSetOrigin` is a separate Layer 4 lineage classifier responsibility
/// (`PLSQL-LIN-007A`).
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
/// Consumed by `plsql cicd plan` (PLSQL-CICD-003), `plsql cicd gate`
/// (PLSQL-CICD-006), and `plsql cicd verify` (PLSQL-CICD-005).
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
/// `verify`) will raise. Kept here so downstream beads can build on a stable
/// error surface without re-introducing one.
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
        if let ChangedObjectKind::OtherKnownKind { object_type } = &kind {
            assert_eq!(object_type, "DIRECTORY");
        } else {
            panic!("expected OtherKnownKind");
        }
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

    #[test]
    fn cicd_error_displays_descriptive_message() {
        let error = CicdError::EmptyChangeSet;
        assert_eq!(format!("{error}"), "changeset has no inputs");
    }
}
