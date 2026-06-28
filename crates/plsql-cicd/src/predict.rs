//! `predict <changeset>` implementation.
//!
//! Combines a `ChangeSet` (from) with Oracle-specific
//! invalidation rules to emit an `InvalidationPrediction`. The rule
//! engine is intentionally text-table — every rule names the
//! `ChangedObjectKind` it triggers on, the kind of invalidation it
//! emits, and the confidence band — so adding a new Oracle 23ai rule
//! is one row, not a code re-architecture.
//!
//! `predict` itself remains the source-only/direct rule engine: every
//! row it emits is `distance: 1`. `predict_with_lineage` composes that
//! direct output with one or more `plsql_lineage::impact()` results and
//! adds downstream dependents from `LineageResult::affected_nodes` as
//! transitive `PredictedInvalidation` rows (`distance > 1` when the
//! dependent is downstream-of-downstream).

use std::collections::{BTreeMap, BTreeSet};

use plsql_core::{
    CompletenessReport, Confidence, ConfidenceLevel, ObjectName, SchemaName, UnknownReason,
};
use plsql_lineage::{AffectedNode, Confidence as LineageConfidence, LineageResult, UnknownEdge};
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};

use crate::{
    ChangeSet, ChangedObject, ChangedObjectKind, InvalidationPrediction, InvalidationReason,
    PredictMode, PredictedInvalidation, RecompileItem, UncertaintyRecord,
};

pub const CHANGE_IMPACT_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.cicd.change_impact",
    version: SchemaVersion::new(1, 0, 0),
    description: "Stable change-impact payload emitted by plsql cicd predict --robot-json",
};

pub type ChangeImpactEnvelope = RobotJsonEnvelope<ChangeImpactPayload>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactPayload {
    pub summary: ChangeImpactSummary,
    pub invalidated_objects_by_kind: Vec<ChangeImpactKindCount>,
    pub invalidations: Vec<ChangeImpactInvalidation>,
    pub recompile_plan: Vec<ChangeImpactRecompileItem>,
    pub compile_error_flags: Vec<ChangeImpactCompileErrorFlag>,
    pub lineage_notes: Vec<ChangeImpactLineageNote>,
    pub uncertainties: Vec<ChangeImpactUncertainty>,
    pub completeness: CompletenessReport,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactSummary {
    pub mode: PredictMode,
    pub invalidation_count: usize,
    pub recompile_count: usize,
    pub uncertainty_count: usize,
    pub compile_error_flag_count: usize,
    pub max_distance: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactKindCount {
    pub object_type: String,
    pub count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactInvalidation {
    pub owner_symbol: u64,
    pub name_symbol: u64,
    pub object_type: String,
    pub reason_code: String,
    pub reason_detail: String,
    pub confidence: String,
    pub confidence_explanation: Option<String>,
    pub distance: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactRecompileItem {
    pub owner_symbol: u64,
    pub name_symbol: u64,
    pub object_type: String,
    pub force_compile: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactCompileErrorFlag {
    pub owner_symbol: u64,
    pub name_symbol: u64,
    pub object_type: String,
    pub flag: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactLineageNote {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeImpactUncertainty {
    pub reason: String,
    pub affected_owner_symbol: Option<u64>,
    pub affected_name_symbol: Option<u64>,
    pub description: String,
}

#[must_use]
pub fn change_impact_payload(
    prediction: &InvalidationPrediction,
    compile_error_flags: Vec<ChangeImpactCompileErrorFlag>,
) -> ChangeImpactPayload {
    ChangeImpactPayload {
        summary: ChangeImpactSummary {
            mode: prediction.mode,
            invalidation_count: prediction.predicted_invalidations.len(),
            recompile_count: prediction.recompile_order.len(),
            uncertainty_count: prediction.uncertainties.len(),
            compile_error_flag_count: compile_error_flags.len(),
            max_distance: prediction
                .predicted_invalidations
                .iter()
                .map(|row| row.distance)
                .max()
                .unwrap_or(0),
        },
        invalidated_objects_by_kind: invalidated_objects_by_kind(prediction),
        invalidations: prediction
            .predicted_invalidations
            .iter()
            .map(change_impact_invalidation)
            .collect(),
        recompile_plan: prediction
            .recompile_order
            .iter()
            .map(change_impact_recompile_item)
            .collect(),
        compile_error_flags,
        lineage_notes: lineage_notes(prediction),
        uncertainties: prediction
            .uncertainties
            .iter()
            .map(change_impact_uncertainty)
            .collect(),
        completeness: prediction.completeness.clone(),
        attributes: prediction.attributes.clone(),
    }
}

#[must_use]
pub fn change_impact_envelope(
    prediction: &InvalidationPrediction,
    compile_error_flags: Vec<ChangeImpactCompileErrorFlag>,
) -> RobotJsonEnvelope<ChangeImpactPayload> {
    RobotJsonEnvelope::new(
        CHANGE_IMPACT_SCHEMA,
        change_impact_payload(prediction, compile_error_flags),
    )
}

fn invalidated_objects_by_kind(prediction: &InvalidationPrediction) -> Vec<ChangeImpactKindCount> {
    let mut counts = BTreeMap::<String, u32>::new();
    for row in &prediction.predicted_invalidations {
        counts
            .entry(row.object_type.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
    counts
        .into_iter()
        .map(|(object_type, count)| ChangeImpactKindCount { object_type, count })
        .collect()
}

fn change_impact_invalidation(row: &PredictedInvalidation) -> ChangeImpactInvalidation {
    let (reason_code, reason_detail) = invalidation_reason_parts(&row.reason);
    ChangeImpactInvalidation {
        owner_symbol: row.owner.symbol().get(),
        name_symbol: row.name.symbol().get(),
        object_type: row.object_type.clone(),
        reason_code: reason_code.into(),
        reason_detail,
        confidence: confidence_level_code(row.confidence.level).into(),
        confidence_explanation: row.confidence.explanation.clone(),
        distance: row.distance,
    }
}

fn change_impact_recompile_item(row: &RecompileItem) -> ChangeImpactRecompileItem {
    ChangeImpactRecompileItem {
        owner_symbol: row.owner.symbol().get(),
        name_symbol: row.name.symbol().get(),
        object_type: row.object_type.clone(),
        force_compile: row.force_compile,
    }
}

fn change_impact_uncertainty(row: &UncertaintyRecord) -> ChangeImpactUncertainty {
    ChangeImpactUncertainty {
        reason: unknown_reason_code(row.reason).into(),
        affected_owner_symbol: row.affected_owner.map(|owner| owner.symbol().get()),
        affected_name_symbol: row.affected_name.map(|name| name.symbol().get()),
        description: row.description.clone(),
    }
}

fn lineage_notes(prediction: &InvalidationPrediction) -> Vec<ChangeImpactLineageNote> {
    prediction
        .attributes
        .iter()
        .filter(|(key, _value)| key.starts_with("lineage."))
        .map(|(key, value)| ChangeImpactLineageNote {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

fn confidence_level_code(level: ConfidenceLevel) -> &'static str {
    match level {
        ConfidenceLevel::High => "high",
        ConfidenceLevel::Medium => "medium",
        ConfidenceLevel::Low => "low",
        ConfidenceLevel::Opaque => "opaque",
    }
}

fn invalidation_reason_parts(reason: &InvalidationReason) -> (&'static str, String) {
    match reason {
        InvalidationReason::PackageSpecChanged { .. } => (
            "package_spec_changed",
            "dependent of changed package specification".into(),
        ),
        InvalidationReason::RoutineSignatureChanged { .. } => (
            "routine_signature_changed",
            "dependent of changed standalone routine signature".into(),
        ),
        InvalidationReason::TableAdditive { .. } => (
            "table_additive_ddl",
            "table additive DDL may require dependent checks".into(),
        ),
        InvalidationReason::TableDestructive { .. } => (
            "table_destructive_ddl",
            "table destructive DDL can invalidate dependents".into(),
        ),
        InvalidationReason::TypeEvolution { .. } => (
            "type_evolution",
            "object type evolution can invalidate structural dependents".into(),
        ),
        InvalidationReason::SynonymRetargeted { .. } => (
            "synonym_retargeted",
            "synonym now resolves to a different target".into(),
        ),
        InvalidationReason::PrivilegeChange => (
            "privilege_change",
            "grant or revoke can affect dependent object authorization".into(),
        ),
        InvalidationReason::MaterializedViewRefreshAffected { .. } => (
            "materialized_view_refresh_affected",
            "materialized view refresh semantics may be affected".into(),
        ),
        InvalidationReason::EditionedObjectChange => (
            "editioned_object_change",
            "editioned object change affects the active edition".into(),
        ),
        InvalidationReason::SourceOnlyHeuristic => (
            "source_only_heuristic",
            "source-only prediction without catalog confirmation".into(),
        ),
        InvalidationReason::Other { description } => ("other", description.clone()),
    }
}

fn unknown_reason_code(reason: UnknownReason) -> &'static str {
    match reason {
        UnknownReason::DynamicSqlOpaque => "DynamicSqlOpaque",
        UnknownReason::DbLinkRemoteObject => "DbLinkRemoteObject",
        UnknownReason::WrappedSource => "WrappedSource",
        UnknownReason::MissingCatalogObject => "MissingCatalogObject",
        UnknownReason::MissingPackageBody => "MissingPackageBody",
        UnknownReason::ConditionalCompilationBranch => "ConditionalCompilationBranch",
        UnknownReason::EditionedObject => "EditionedObject",
        UnknownReason::InvokerRightsRuntimeResolution => "InvokerRightsRuntimeResolution",
        UnknownReason::RuntimeGrantOrRole => "RuntimeGrantOrRole",
        UnknownReason::UnsupportedDialectFeature => "UnsupportedDialectFeature",
        UnknownReason::ParserRecoveryRegion => "ParserRecoveryRegion",
        UnknownReason::AnalysisRecursionLimit => "AnalysisRecursionLimit",
        UnknownReason::ResponseSanitized => "ResponseSanitized",
    }
}

/// Metadata needed to lower a lineage logical id into the CI/CD prediction
/// surface.
///
/// `plsql-lineage` reports impact in graph-native string IDs
/// (`schema.object`, `schema.package.member`, ...). `plsql-cicd`
/// predictions use the workspace's interned [`SchemaName`] /
/// [`ObjectName`] identifiers instead, so the caller supplies this
/// metadata from the same symbol table/catalog/depgraph that produced
/// the lineage result. The predictor never guesses symbols from strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineageObjectMetadata {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: String,
    pub force_compile: bool,
}

impl LineageObjectMetadata {
    #[must_use]
    pub fn new(
        owner: SchemaName,
        name: ObjectName,
        object_type: impl Into<String>,
        force_compile: bool,
    ) -> Self {
        Self {
            owner,
            name,
            object_type: object_type.into(),
            force_compile,
        }
    }
}

/// Run the predict pipeline over `changeset`. `mode` decides the
/// completeness profile (plan §15.2): `SourceOnly` records a
/// `SourceOnlyHeuristic` reason on every emitted row; `CatalogAware`
/// emits the canonical reasons; `LiveSnapshot` is identical to
/// `CatalogAware` from the rule engine's point of view (the live
/// snapshot is the input on its way in, not the rule decision here).
#[must_use]
pub fn predict(changeset: &ChangeSet, mode: PredictMode) -> InvalidationPrediction {
    let mut prediction = InvalidationPrediction {
        mode,
        completeness: completeness_profile_for_mode(mode),
        ..InvalidationPrediction::default()
    };

    for object in &changeset.objects {
        apply_oracle_invalidation_rules(object, mode, &mut prediction);
        for reason in &object.uncertainties {
            prediction.uncertainties.push(UncertaintyRecord {
                reason: *reason,
                affected_owner: Some(object.owner),
                affected_name: Some(object.name),
                description: format!(
                    "{:?} change for {:?}.{:?} carries opacity",
                    object.kind, object.owner, object.name
                ),
            });
        }
    }

    // Derive the completeness posture from the changeset's actual understanding
    // and FINALIZE it (oracle-687a.2). Previously the profile shipped with the
    // `#[default]` posture (`Degraded`), so an empty / fully-understood changeset
    // serialized a false-pessimistic `Degraded` over the wire — the exact inverse
    // of the false-clean failure the design forbids. We tally objects as
    // understood (no opacity) vs unrecognized (carries an `UnknownReason`), count
    // the opacity records as diagnostics, then let `finalize_posture` apply the
    // anti-spin rules: 0 objects / 0 diagnostics ⇒ `Clean`; any opacity ⇒ never
    // `Clean`.
    let unrecognized = changeset
        .objects
        .iter()
        .filter(|o| !o.uncertainties.is_empty())
        .count();
    prediction.completeness.objects_total = changeset.objects.len();
    prediction.completeness.objects_unrecognized = unrecognized;
    prediction.completeness.objects_with_extracted_semantics =
        changeset.objects.len().saturating_sub(unrecognized);
    prediction.completeness.diagnostics_total = prediction.uncertainties.len();
    prediction.completeness.finalize_posture();

    // Sort by `(distance, owner, name)` so reports diff cleanly across runs.
    sort_prediction(&mut prediction);
    prediction
}

/// Run direct Oracle invalidation rules and append transitive downstream
/// dependents from lineage `impact()` results.
///
/// Each `LineageResult` should be produced by
/// [`plsql_lineage::impact`] for one changed object in `changeset`.
/// The resolver maps the result's graph-native `logical_id` strings
/// back to the interned names used by the CI/CD prediction structs.
/// Unresolvable affected nodes are not dropped: they become typed
/// `UnknownReason::MissingCatalogObject` uncertainty records.
#[must_use]
pub fn predict_with_lineage<F>(
    changeset: &ChangeSet,
    mode: PredictMode,
    impact_results: &[LineageResult],
    mut resolve_object: F,
) -> InvalidationPrediction
where
    F: FnMut(&str) -> Option<LineageObjectMetadata>,
{
    let mut prediction = predict(changeset, mode);
    let direct_keys: BTreeSet<(SchemaName, ObjectName, String)> = prediction
        .predicted_invalidations
        .iter()
        .map(|row| (row.owner, row.name, row.object_type.clone()))
        .collect();

    let mut lineage_rows: BTreeMap<(SchemaName, ObjectName, String), PredictedInvalidation> =
        BTreeMap::new();
    let mut recompile_rows: BTreeMap<(SchemaName, ObjectName, String), RecompileItem> =
        BTreeMap::new();
    let mut unresolved_nodes = 0usize;

    for impact in impact_results {
        let anchor = impact
            .query
            .as_ref()
            .map(|query| query.anchor.as_str())
            .unwrap_or("<unknown anchor>");

        for node in &impact.affected_nodes {
            if node.hops == 0 {
                continue;
            }
            let Some(metadata) = resolve_object(node.logical_id.as_str()) else {
                unresolved_nodes += 1;
                prediction.uncertainties.push(unresolved_lineage_node(node));
                continue;
            };

            let key = (metadata.owner, metadata.name, metadata.object_type.clone());
            if direct_keys.contains(&key) {
                continue;
            }

            let candidate = PredictedInvalidation {
                owner: metadata.owner,
                name: metadata.name,
                object_type: metadata.object_type.clone(),
                reason: InvalidationReason::Other {
                    description: format!(
                        "lineage impact from `{anchor}` reaches `{}`",
                        node.logical_id
                    ),
                },
                confidence: confidence_from_lineage(node.path_confidence),
                distance: node.hops,
            };

            match lineage_rows.get_mut(&key) {
                Some(existing) if prediction_row_is_stronger(&candidate, existing) => {
                    *existing = candidate;
                }
                Some(_) => {}
                None => {
                    lineage_rows.insert(key.clone(), candidate);
                }
            }

            recompile_rows.entry(key).or_insert_with(|| RecompileItem {
                owner: metadata.owner,
                name: metadata.name,
                object_type: metadata.object_type,
                force_compile: metadata.force_compile,
            });
        }

        for unknown in &impact.unknown_edges {
            prediction
                .uncertainties
                .push(uncertainty_from_unknown_edge(unknown, &mut resolve_object));
        }
    }

    let added_rows = lineage_rows.len();
    prediction
        .predicted_invalidations
        .extend(lineage_rows.into_values());
    prediction
        .recompile_order
        .extend(recompile_rows.into_values());
    prediction.attributes.insert(
        String::from("lineage.impact_results"),
        impact_results.len().to_string(),
    );
    prediction.attributes.insert(
        String::from("lineage.transitive_invalidations"),
        added_rows.to_string(),
    );
    prediction.attributes.insert(
        String::from("lineage.unresolved_logical_ids"),
        unresolved_nodes.to_string(),
    );
    prediction.completeness.diagnostics_total = prediction.uncertainties.len();
    prediction.completeness.finalize_posture();
    sort_prediction(&mut prediction);
    prediction
}

/// Build the `CompletenessReport` profile that `predict` attaches per
/// run mode. Each mode declares its starting evidence
/// surface so downstream gates know which `UnknownReason`s are
/// expected vs. surprising.
fn completeness_profile_for_mode(mode: PredictMode) -> plsql_core::CompletenessReport {
    let mut report = plsql_core::CompletenessReport::default();
    match mode {
        PredictMode::SourceOnly => {
            report.catalog_available = false;
            report.plscope_available = false;
        }
        PredictMode::CatalogAware => {
            report.catalog_available = true;
            report.plscope_available = false;
        }
        PredictMode::LiveSnapshot => {
            report.catalog_available = true;
            report.plscope_available = true;
        }
    }
    report
}

fn apply_oracle_invalidation_rules(
    changed: &ChangedObject,
    mode: PredictMode,
    out: &mut InvalidationPrediction,
) {
    let confidence = confidence_for_mode(mode);
    // Each match arm is one Oracle-specific invalidation rule. The
    // `distance: 1` annotation marks them as direct (single-hop)
    // invalidations. Transitive (`distance > 1`) rows are out of scope
    // for this module — see the module-level docs; no lineage walk runs
    // here.
    match changed.kind {
        ChangedObjectKind::PackageSpec => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("PACKAGE"),
                reason: InvalidationReason::PackageSpecChanged {
                    spec_owner: changed.owner,
                    spec_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::PackageBody => {
            // Body-only changes do not invalidate dependents per Oracle's
            // fine-grained dependency tracking (since 11gR2). We emit no
            // direct invalidation but record uncertainty for the
            // `Unknown` confidence band so the report still surfaces it.
            out.uncertainties.push(UncertaintyRecord {
                reason: UnknownReason::ConditionalCompilationBranch,
                affected_owner: Some(changed.owner),
                affected_name: Some(changed.name),
                description: String::from(
                    "package body change — dependents not invalidated under Oracle 11gR2+ fine-grained dependencies",
                ),
            });
        }
        ChangedObjectKind::StandaloneRoutineSignature => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("ROUTINE"),
                reason: InvalidationReason::RoutineSignatureChanged {
                    routine_owner: changed.owner,
                    routine_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::StandaloneRoutineBody => {
            // Body-only change to a standalone routine — same fine-grained
            // dependency policy as package body.
        }
        ChangedObjectKind::TableAdditiveDdl => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("TABLE"),
                reason: InvalidationReason::TableAdditive {
                    table_owner: changed.owner,
                    table_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::TableDestructiveDdl => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("TABLE"),
                reason: InvalidationReason::TableDestructive {
                    table_owner: changed.owner,
                    table_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::TypeEvolution => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("TYPE"),
                reason: InvalidationReason::TypeEvolution {
                    type_owner: changed.owner,
                    type_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::SynonymRetargeting => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("SYNONYM"),
                reason: InvalidationReason::SynonymRetargeted {
                    synonym_owner: changed.owner,
                    synonym_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::GrantOrRevoke => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("PRIVILEGE"),
                reason: InvalidationReason::PrivilegeChange,
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::EditionedObjectChange => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("EDITIONED"),
                reason: InvalidationReason::EditionedObjectChange,
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::MaterializedViewRefreshAffecting => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("MATERIALIZED_VIEW"),
                reason: InvalidationReason::MaterializedViewRefreshAffected {
                    mview_owner: changed.owner,
                    mview_name: changed.name,
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::ViewDefinitionChange => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("VIEW"),
                reason: InvalidationReason::Other {
                    description: String::from(
                        "view definition changed — dependents on column-set or projection must be revalidated",
                    ),
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::TriggerChange => {
            // Triggers themselves invalidate but Oracle re-compiles them
            // lazily on next DML; emit a low-confidence row for the
            // trigger itself.
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: String::from("TRIGGER"),
                reason: InvalidationReason::Other {
                    description: String::from("trigger body changed"),
                },
                confidence: confidence.clone(),
                distance: 1,
            });
        }
        ChangedObjectKind::IndexChange | ChangedObjectKind::SequenceChange => {
            // Indexes and sequences do not invalidate dependents per
            // Oracle's dependency model; recorded as informational uncertainty
            // so the report still surfaces the change.
            out.uncertainties.push(UncertaintyRecord {
                reason: UnknownReason::MissingCatalogObject,
                affected_owner: Some(changed.owner),
                affected_name: Some(changed.name),
                description: format!(
                    "{:?} change is informational only — Oracle does not invalidate dependents",
                    changed.kind
                ),
            });
        }
        ChangedObjectKind::OtherKnownKind { ref object_type } => {
            out.predicted_invalidations.push(PredictedInvalidation {
                owner: changed.owner,
                name: changed.name,
                object_type: object_type.clone(),
                reason: InvalidationReason::Other {
                    description: format!(
                        "{object_type} change — invalidation rule not yet codified"
                    ),
                },
                confidence: low_confidence("rule not yet codified"),
                distance: 1,
            });
        }
        ChangedObjectKind::Unclassified => {
            // R13: emit a typed uncertainty rather than silently
            // dropping.
            out.uncertainties.push(UncertaintyRecord {
                reason: UnknownReason::ParserRecoveryRegion,
                affected_owner: Some(changed.owner),
                affected_name: Some(changed.name),
                description: String::from("unclassified change — predict cannot reason about it"),
            });
        }
    }
    // SourceOnly mode adds a follow-up SourceOnlyHeuristic row to every
    // emitted invalidation so the agent reads "this is a best-effort
    // pre-catalog hint" out of the response shape.
    if matches!(mode, PredictMode::SourceOnly) && !out.predicted_invalidations.is_empty() {
        out.uncertainties.push(UncertaintyRecord {
            reason: UnknownReason::MissingCatalogObject,
            affected_owner: Some(changed.owner),
            affected_name: Some(changed.name),
            description: String::from(
                "source-only mode — catalog-confirmed dependents not consulted",
            ),
        });
    }
}

fn confidence_for_mode(mode: PredictMode) -> Confidence {
    match mode {
        PredictMode::SourceOnly => Confidence::new(
            ConfidenceLevel::Low,
            Some(String::from(
                "source-only predict — no catalog confirmation",
            )),
        ),
        PredictMode::CatalogAware => Confidence::new(
            ConfidenceLevel::High,
            Some(String::from("catalog-aware predict")),
        ),
        PredictMode::LiveSnapshot => Confidence::new(
            ConfidenceLevel::High,
            Some(String::from(
                "live-snapshot predict — catalog extracted at run time",
            )),
        ),
    }
}

fn low_confidence(reason: &str) -> Confidence {
    Confidence::new(ConfidenceLevel::Low, Some(String::from(reason)))
}

fn confidence_from_lineage(confidence: LineageConfidence) -> Confidence {
    match confidence {
        LineageConfidence::Exact => Confidence::new(
            ConfidenceLevel::High,
            Some(String::from("lineage impact path exact")),
        ),
        LineageConfidence::Heuristic => Confidence::new(
            ConfidenceLevel::Medium,
            Some(String::from("lineage impact path heuristic")),
        ),
        LineageConfidence::Unknown => Confidence::new(
            ConfidenceLevel::Opaque,
            Some(String::from("lineage impact path unknown")),
        ),
    }
}

fn prediction_row_is_stronger(
    candidate: &PredictedInvalidation,
    existing: &PredictedInvalidation,
) -> bool {
    match candidate.distance.cmp(&existing.distance) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => {
            confidence_rank(candidate.confidence.level) > confidence_rank(existing.confidence.level)
        }
        std::cmp::Ordering::Greater => false,
    }
}

fn confidence_rank(level: ConfidenceLevel) -> u8 {
    match level {
        ConfidenceLevel::High => 3,
        ConfidenceLevel::Medium => 2,
        ConfidenceLevel::Low => 1,
        ConfidenceLevel::Opaque => 0,
    }
}

fn unresolved_lineage_node(node: &AffectedNode) -> UncertaintyRecord {
    UncertaintyRecord {
        reason: UnknownReason::MissingCatalogObject,
        affected_owner: None,
        affected_name: None,
        description: format!(
            "lineage impact node `{}` could not be resolved to CI/CD object metadata",
            node.logical_id
        ),
    }
}

fn uncertainty_from_unknown_edge<F>(edge: &UnknownEdge, resolve_object: &mut F) -> UncertaintyRecord
where
    F: FnMut(&str) -> Option<LineageObjectMetadata>,
{
    let resolved = resolve_object(edge.source.as_str());
    UncertaintyRecord {
        reason: unknown_reason_from_lineage(edge.unknown_reason.as_str()),
        affected_owner: resolved.as_ref().map(|meta| meta.owner),
        affected_name: resolved.as_ref().map(|meta| meta.name),
        description: match &edge.detail {
            Some(detail) => format!(
                "lineage edge from `{}` is unresolved: {} ({detail})",
                edge.source, edge.unknown_reason
            ),
            None => format!(
                "lineage edge from `{}` is unresolved: {}",
                edge.source, edge.unknown_reason
            ),
        },
    }
}

fn unknown_reason_from_lineage(reason: &str) -> UnknownReason {
    match reason {
        "DynamicSqlOpaque" => UnknownReason::DynamicSqlOpaque,
        "DbLinkRemoteObject" => UnknownReason::DbLinkRemoteObject,
        "WrappedSource" => UnknownReason::WrappedSource,
        "MissingPackageBody" => UnknownReason::MissingPackageBody,
        _ => UnknownReason::MissingCatalogObject,
    }
}

fn sort_prediction(prediction: &mut InvalidationPrediction) {
    prediction
        .predicted_invalidations
        .sort_by_key(|p| (p.distance, p.owner, p.name, p.object_type.clone()));
    prediction
        .recompile_order
        .sort_by_key(|r| (r.owner, r.name, r.object_type.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position, Span, SymbolInterner};
    use plsql_core::{ObjectName, SchemaName, SymbolId};
    use plsql_depgraph::{
        DepGraph, Edge, EdgeId, EdgeKind, LogicalObjectId, Node, NodeId, NodeIdentityKind,
        ObjectRevisionId, Provenance, QualifiedName, ResolutionStrategy,
    };
    use plsql_lineage::impact;

    fn billing_owner() -> SchemaName {
        SchemaName::new(SymbolId::new(1))
    }
    fn obj(symbol: u64) -> ObjectName {
        ObjectName::new(SymbolId::new(symbol))
    }

    fn changed(kind: ChangedObjectKind, name: u64) -> ChangedObject {
        ChangedObject {
            owner: billing_owner(),
            name: obj(name),
            kind,
            new_hash: None,
            previous_hash: None,
            file_paths: vec![],
            uncertainties: vec![],
        }
    }

    fn test_span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 1, 0),
        )
    }

    fn test_provenance() -> Provenance {
        Provenance::new(
            FileId::new(1),
            test_span(),
            ResolutionStrategy::CatalogLookup,
        )
    }

    fn graph_edge(id: u64, from: u64, to: u64) -> Edge {
        Edge::new(
            EdgeId::new(id),
            NodeId::new(from),
            NodeId::new(to),
            EdgeKind::Reads,
            Confidence::new(ConfidenceLevel::High, None),
        )
    }

    fn object_type_for_kind(kind: NodeIdentityKind) -> &'static str {
        match kind {
            NodeIdentityKind::PackageSpecification
            | NodeIdentityKind::PackageBody
            | NodeIdentityKind::PackageProcedure
            | NodeIdentityKind::PackageFunction => "PACKAGE",
            NodeIdentityKind::StandaloneProcedure => "PROCEDURE",
            NodeIdentityKind::StandaloneFunction => "FUNCTION",
            NodeIdentityKind::Table => "TABLE",
            NodeIdentityKind::View | NodeIdentityKind::EditioningView => "VIEW",
            NodeIdentityKind::MaterializedView => "MATERIALIZED_VIEW",
            NodeIdentityKind::Trigger => "TRIGGER",
            NodeIdentityKind::Type
            | NodeIdentityKind::TypeMethod
            | NodeIdentityKind::TypeAttribute => "TYPE",
            NodeIdentityKind::Synonym => "SYNONYM",
            NodeIdentityKind::SchedulerJob => "JOB",
            _ => "OBJECT",
        }
    }

    fn insert_fixture_node(
        graph: &mut DepGraph,
        metadata: &mut BTreeMap<String, LineageObjectMetadata>,
        interner: &mut SymbolInterner,
        id: u64,
        schema: SchemaName,
        object: &str,
        kind: NodeIdentityKind,
    ) -> ObjectName {
        let object_name = ObjectName::from(interner.intern(object).expect("object name interns"));
        let logical_id = format!("BILLING.{object}");
        graph.insert_node(Node::new(
            NodeId::new(id),
            LogicalObjectId::new(logical_id.clone()),
            ObjectRevisionId::new(format!("sha256:{logical_id}")),
            QualifiedName::new(Some(schema), object_name),
            kind,
        ));
        metadata.insert(
            logical_id,
            LineageObjectMetadata::new(schema, object_name, object_type_for_kind(kind), true),
        );
        object_name
    }

    fn lineage_fixture() -> (DepGraph, BTreeMap<String, LineageObjectMetadata>, ChangeSet) {
        let mut interner = SymbolInterner::new();
        let schema = interner
            .intern_schema_name("BILLING")
            .expect("schema name interns");
        let mut graph = DepGraph::new();
        let mut metadata = BTreeMap::new();

        let customers = insert_fixture_node(
            &mut graph,
            &mut metadata,
            &mut interner,
            1,
            schema,
            "CUSTOMERS",
            NodeIdentityKind::Table,
        );
        insert_fixture_node(
            &mut graph,
            &mut metadata,
            &mut interner,
            2,
            schema,
            "REPORT_PKG",
            NodeIdentityKind::PackageBody,
        );
        insert_fixture_node(
            &mut graph,
            &mut metadata,
            &mut interner,
            3,
            schema,
            "REPORT_VIEW",
            NodeIdentityKind::View,
        );
        insert_fixture_node(
            &mut graph,
            &mut metadata,
            &mut interner,
            4,
            schema,
            "SUMMARY_JOB",
            NodeIdentityKind::SchedulerJob,
        );

        // Engine convention: from=dependent -> to=dependency.
        // Impact(CUSTOMERS) therefore walks incoming edges to
        // REPORT_PKG -> REPORT_VIEW -> SUMMARY_JOB.
        graph.insert_edge(graph_edge(1, 2, 1), test_provenance(), None);
        graph.insert_edge(graph_edge(2, 3, 2), test_provenance(), None);
        graph.insert_edge(graph_edge(3, 4, 3), test_provenance(), None);

        let changeset = ChangeSet {
            objects: vec![ChangedObject {
                owner: schema,
                name: customers,
                kind: ChangedObjectKind::TableDestructiveDdl,
                new_hash: None,
                previous_hash: None,
                file_paths: vec![],
                uncertainties: vec![],
            }],
            ..ChangeSet::empty()
        };

        (graph, metadata, changeset)
    }

    #[test]
    fn predict_empty_changeset_returns_empty_prediction() {
        let prediction = predict(&ChangeSet::empty(), PredictMode::CatalogAware);
        assert!(prediction.predicted_invalidations.is_empty());
        assert!(prediction.uncertainties.is_empty());
    }

    #[test]
    fn completeness_posture_is_finalized_not_default_degraded() {
        use plsql_core::CompletenessPosture;
        // oracle-687a.2: an empty changeset (0 objects, 0 diagnostics) must
        // serialize posture=Clean, NOT the false-pessimistic #[default] Degraded.
        for mode in [
            PredictMode::SourceOnly,
            PredictMode::CatalogAware,
            PredictMode::LiveSnapshot,
        ] {
            let p = predict(&ChangeSet::empty(), mode);
            assert_eq!(
                p.completeness.posture,
                CompletenessPosture::Clean,
                "empty changeset must be Clean in {mode:?}, not default-Degraded"
            );
        }
        // A fully-understood changeset (objects, zero opacity) is also Clean.
        let clean = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageSpec, 100)],
            ..ChangeSet::empty()
        };
        assert_eq!(
            predict(&clean, PredictMode::CatalogAware)
                .completeness
                .posture,
            CompletenessPosture::Clean
        );
        // A changeset carrying opacity (an UnknownReason) must NEVER be Clean.
        let mut opaque_obj = changed(ChangedObjectKind::PackageSpec, 101);
        opaque_obj.uncertainties = vec![UnknownReason::DynamicSqlOpaque];
        let opaque = ChangeSet {
            objects: vec![opaque_obj],
            ..ChangeSet::empty()
        };
        let pp = predict(&opaque, PredictMode::CatalogAware);
        assert_ne!(
            pp.completeness.posture,
            CompletenessPosture::Clean,
            "a changeset with opacity must not be Clean: {:?}",
            pp.completeness
        );
        assert_eq!(pp.completeness.objects_unrecognized, 1);
    }

    #[test]
    fn package_spec_change_emits_invalidation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageSpec, 100)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert_eq!(prediction.predicted_invalidations.len(), 1);
        let row = prediction
            .predicted_invalidations
            .first()
            .expect("package spec fixture emits one invalidation");
        assert!(matches!(
            row.reason,
            InvalidationReason::PackageSpecChanged { .. }
        ));
        assert_eq!(row.confidence.level, ConfidenceLevel::High);
        assert_eq!(row.distance, 1);
    }

    #[test]
    fn package_body_change_records_uncertainty_no_invalidation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageBody, 101)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert!(prediction.predicted_invalidations.is_empty());
        assert_eq!(prediction.uncertainties.len(), 1);
    }

    #[test]
    fn source_only_mode_downgrades_confidence_and_records_uncertainty() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageSpec, 102)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::SourceOnly);
        assert_eq!(prediction.predicted_invalidations.len(), 1);
        assert_eq!(
            prediction
                .predicted_invalidations
                .first()
                .expect("source-only fixture emits one invalidation")
                .confidence
                .level,
            ConfidenceLevel::Low
        );
        assert!(
            prediction
                .uncertainties
                .iter()
                .any(|u| u.description.contains("source-only"))
        );
    }

    #[test]
    fn destructive_table_ddl_emits_destructive_reason() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::TableDestructiveDdl, 103)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let first = prediction
            .predicted_invalidations
            .first()
            .expect("destructive table fixture emits one invalidation");
        assert!(matches!(
            first.reason,
            InvalidationReason::TableDestructive { .. }
        ));
    }

    #[test]
    fn index_and_sequence_changes_are_informational() {
        let changeset = ChangeSet {
            objects: vec![
                changed(ChangedObjectKind::IndexChange, 200),
                changed(ChangedObjectKind::SequenceChange, 201),
            ],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert!(prediction.predicted_invalidations.is_empty());
        assert_eq!(prediction.uncertainties.len(), 2);
    }

    #[test]
    fn unclassified_kind_emits_parser_recovery_uncertainty() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::Unclassified, 300)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert!(prediction.predicted_invalidations.is_empty());
        assert!(
            prediction
                .uncertainties
                .iter()
                .any(|u| matches!(u.reason, UnknownReason::ParserRecoveryRegion))
        );
    }

    #[test]
    fn invalidations_sorted_stable() {
        let changeset = ChangeSet {
            objects: vec![
                changed(ChangedObjectKind::PackageSpec, 200),
                changed(ChangedObjectKind::PackageSpec, 100),
                changed(ChangedObjectKind::PackageSpec, 150),
            ],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let symbols: Vec<u64> = prediction
            .predicted_invalidations
            .iter()
            .map(|r| r.name.symbol().get())
            .collect();
        assert_eq!(symbols, vec![100, 150, 200]);
    }

    /// **oracle-qm3q.17 regression — single-hop scope is honest.**
    /// The module docs previously advertised a `predict_with_lineage`
    /// transitive walker that does not exist; in reality every emitted
    /// row is direct (`distance == 1`). Exercise every emitting
    /// `ChangedObjectKind` and assert no row claims a transitive
    /// distance, so the docs and behaviour cannot drift: if a real
    /// lineage walk is wired into `predict` it will trip this test and
    /// force the module docs / gate scope note to be updated
    /// deliberately.
    #[test]
    fn predict_emits_only_direct_distance_one_rows() {
        let emitting_kinds = vec![
            ChangedObjectKind::PackageSpec,
            ChangedObjectKind::StandaloneRoutineSignature,
            ChangedObjectKind::TableAdditiveDdl,
            ChangedObjectKind::TableDestructiveDdl,
            ChangedObjectKind::TypeEvolution,
            ChangedObjectKind::SynonymRetargeting,
            ChangedObjectKind::GrantOrRevoke,
            ChangedObjectKind::EditionedObjectChange,
            ChangedObjectKind::MaterializedViewRefreshAffecting,
            ChangedObjectKind::ViewDefinitionChange,
            ChangedObjectKind::TriggerChange,
            ChangedObjectKind::OtherKnownKind {
                object_type: String::from("CONTEXT"),
            },
        ];
        let objects: Vec<ChangedObject> = emitting_kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| {
                let offset = u64::try_from(i).unwrap_or(u64::MAX);
                changed(kind, 500_u64.saturating_add(offset))
            })
            .collect();
        let changeset = ChangeSet {
            objects,
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert!(
            !prediction.predicted_invalidations.is_empty(),
            "every emitting kind should produce at least one row"
        );
        for row in &prediction.predicted_invalidations {
            assert_eq!(
                row.distance, 1,
                "predict only emits direct (single-hop) invalidations; \
                 transitive rows belong in predict_with_lineage (oracle-qm3q.17): {row:?}"
            );
        }
    }

    #[test]
    fn predict_with_lineage_adds_full_transitive_impact_closure() {
        let (graph, metadata, changeset) = lineage_fixture();
        let impact_result = impact(&graph, &NodeId::new(1), None);
        assert_eq!(impact_result.affected_nodes.len(), 3);

        let prediction = predict_with_lineage(
            &changeset,
            PredictMode::CatalogAware,
            &[impact_result],
            |logical_id| metadata.get(logical_id).cloned(),
        );

        let mut distances = BTreeMap::new();
        for (logical_id, meta) in &metadata {
            if logical_id == "BILLING.CUSTOMERS" {
                continue;
            }
            let row = prediction
                .predicted_invalidations
                .iter()
                .find(|row| row.owner == meta.owner && row.name == meta.name)
                .expect("transitive impact row emitted");
            distances.insert(logical_id.as_str(), row.distance);
        }

        assert_eq!(distances.get("BILLING.REPORT_PKG"), Some(&1));
        assert_eq!(distances.get("BILLING.REPORT_VIEW"), Some(&2));
        assert_eq!(distances.get("BILLING.SUMMARY_JOB"), Some(&3));
        assert_eq!(
            prediction
                .attributes
                .get("lineage.transitive_invalidations"),
            Some(&String::from("3"))
        );
        assert_eq!(prediction.recompile_order.len(), 3);
    }

    #[test]
    fn predict_with_lineage_records_unresolved_impact_nodes_as_uncertainty() {
        let impact_result = LineageResult {
            affected_nodes: vec![AffectedNode {
                logical_id: String::from("BILLING.MISSING_DEPENDENT"),
                hops: 1,
                path_confidence: LineageConfidence::Exact,
            }],
            ..LineageResult::default()
        };

        let prediction = predict_with_lineage(
            &ChangeSet::empty(),
            PredictMode::CatalogAware,
            &[impact_result],
            |_| None,
        );

        assert!(prediction.predicted_invalidations.is_empty());
        assert_eq!(
            prediction.attributes.get("lineage.unresolved_logical_ids"),
            Some(&String::from("1"))
        );
        assert!(prediction.uncertainties.iter().any(|u| matches!(
            u.reason,
            UnknownReason::MissingCatalogObject
        )
            && u.description.contains("BILLING.MISSING_DEPENDENT")));
    }

    fn change_impact_fixture_prediction() -> InvalidationPrediction {
        let (graph, metadata, changeset) = lineage_fixture();
        let impact_result = impact(&graph, &NodeId::new(1), None);
        predict_with_lineage(
            &changeset,
            PredictMode::CatalogAware,
            &[impact_result],
            |logical_id| metadata.get(logical_id).cloned(),
        )
    }

    fn compile_error_flag() -> ChangeImpactCompileErrorFlag {
        ChangeImpactCompileErrorFlag {
            owner_symbol: 0,
            name_symbol: 2,
            object_type: String::from("PACKAGE"),
            flag: String::from("compile_error_detected"),
            message: String::from("ORA-04063: package body has errors"),
        }
    }

    #[test]
    fn change_impact_payload_sections_are_stable() {
        let prediction = change_impact_fixture_prediction();
        let envelope = change_impact_envelope(&prediction, vec![compile_error_flag()]);

        assert!(envelope.matches_schema(CHANGE_IMPACT_SCHEMA));
        let payload = &envelope.payload;
        assert_eq!(payload.summary.invalidation_count, 4);
        assert_eq!(payload.summary.recompile_count, 3);
        assert_eq!(payload.summary.compile_error_flag_count, 1);
        assert_eq!(payload.summary.max_distance, 3);
        assert_eq!(
            payload
                .invalidated_objects_by_kind
                .iter()
                .map(|row| (row.object_type.as_str(), row.count))
                .collect::<Vec<_>>(),
            vec![("JOB", 1), ("PACKAGE", 1), ("TABLE", 1), ("VIEW", 1)]
        );
        assert_eq!(payload.lineage_notes.len(), 3);
        assert_eq!(
            payload
                .compile_error_flags
                .first()
                .expect("fixture carries one compile-error flag")
                .flag,
            "compile_error_detected"
        );
    }

    #[test]
    fn change_impact_payload_matches_golden_snapshot() {
        let prediction = change_impact_fixture_prediction();
        let envelope = change_impact_envelope(&prediction, vec![compile_error_flag()]);
        let actual = serde_json::to_string_pretty(&envelope).expect("serialize golden payload");
        let expected = include_str!("../tests/golden/change_impact_payload.json").trim_end();

        assert_eq!(actual, expected);
    }

    #[test]
    fn per_object_uncertainties_propagate_into_prediction() {
        let mut obj = changed(ChangedObjectKind::PackageSpec, 400);
        obj.uncertainties.push(UnknownReason::DynamicSqlOpaque);
        let changeset = ChangeSet {
            objects: vec![obj],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert!(
            prediction
                .uncertainties
                .iter()
                .any(|u| matches!(u.reason, UnknownReason::DynamicSqlOpaque))
        );
    }
}
