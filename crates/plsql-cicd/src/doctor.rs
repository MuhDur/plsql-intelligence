//! Doctor subcommand for the CI/CD cascade.
//!
//! Given a `ChangeSet` (optionally plus its `InvalidationPrediction`),
//! the doctor emits a structured "customer changeset health" report:
//! object counts by kind, predicted invalidations by
//! reason, uncertainty inventory, deployment-risk classification, and a
//! short list of remediation hints. The shape is designed so an MCP /
//! CLI / HTML renderer can render the same data without re-deriving any
//! of it.

use std::collections::BTreeMap;

use plsql_core::UnknownReason;
use serde::{Deserialize, Serialize};

use crate::{ChangeSet, ChangedObjectKind, DeploymentRisk, InvalidationPrediction};

/// Top-level doctor report for a changeset.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangesetDoctorReport {
    pub origin: Option<String>,
    pub object_count: usize,
    pub unclassified_file_count: usize,
    pub object_kind_counts: Vec<DoctorKindRow>,
    pub uncertainty_counts: Vec<DoctorReasonRow>,
    pub invalidation_total: usize,
    pub invalidation_by_reason: Vec<DoctorReasonRow>,
    pub overall_risk: DeploymentRisk,
    pub remediation_hints: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorKindRow {
    pub kind: String,
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorReasonRow {
    pub label: String,
    pub count: usize,
}

/// Build the doctor report. `prediction` is optional — when omitted the
/// invalidation columns are empty and the overall risk is inferred from
/// the changeset alone (presence of any destructive DDL → `Destructive`,
/// otherwise `Unknown`).
#[must_use]
pub fn doctor_report(
    changeset: &ChangeSet,
    prediction: Option<&InvalidationPrediction>,
) -> ChangesetDoctorReport {
    let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();
    for object in &changeset.objects {
        let label = object_kind_label(&object.kind);
        *kind_counts.entry(label).or_insert(0) += 1;
    }

    let object_kind_counts: Vec<DoctorKindRow> = kind_counts
        .into_iter()
        .map(|(kind, count)| DoctorKindRow { kind, count })
        .collect();

    let (invalidation_total, invalidation_by_reason, uncertainty_counts) = match prediction {
        Some(prediction) => {
            let mut reason_counts: BTreeMap<String, usize> = BTreeMap::new();
            for row in &prediction.predicted_invalidations {
                let label = reason_label(&row.reason);
                *reason_counts.entry(label).or_insert(0) += 1;
            }
            let invalidation_by_reason: Vec<DoctorReasonRow> = reason_counts
                .into_iter()
                .map(|(label, count)| DoctorReasonRow { label, count })
                .collect();
            let mut uncertainty_counts: BTreeMap<String, usize> = BTreeMap::new();
            for record in &prediction.uncertainties {
                let label = unknown_label(record.reason);
                *uncertainty_counts.entry(label).or_insert(0) += 1;
            }
            let uncertainty_rows: Vec<DoctorReasonRow> = uncertainty_counts
                .into_iter()
                .map(|(label, count)| DoctorReasonRow { label, count })
                .collect();
            (
                prediction.predicted_invalidations.len(),
                invalidation_by_reason,
                uncertainty_rows,
            )
        }
        None => (0usize, Vec::new(), Vec::new()),
    };

    let overall_risk = classify_risk(changeset, prediction);
    let remediation_hints = build_remediation_hints(changeset, prediction, overall_risk);

    let origin = changeset.origin.as_ref().map(|o| format!("{o:?}"));
    ChangesetDoctorReport {
        origin,
        object_count: changeset.objects.len(),
        unclassified_file_count: changeset.unclassified_files.len(),
        object_kind_counts,
        uncertainty_counts,
        invalidation_total,
        invalidation_by_reason,
        overall_risk,
        remediation_hints,
    }
}

fn object_kind_label(kind: &ChangedObjectKind) -> String {
    match kind {
        ChangedObjectKind::PackageSpec => String::from("package-spec"),
        ChangedObjectKind::PackageBody => String::from("package-body"),
        ChangedObjectKind::StandaloneRoutineSignature => String::from("routine-signature"),
        ChangedObjectKind::StandaloneRoutineBody => String::from("routine-body"),
        ChangedObjectKind::TableAdditiveDdl => String::from("table-additive"),
        ChangedObjectKind::TableDestructiveDdl => String::from("table-destructive"),
        ChangedObjectKind::ViewDefinitionChange => String::from("view-definition"),
        ChangedObjectKind::TypeEvolution => String::from("type-evolution"),
        ChangedObjectKind::SynonymRetargeting => String::from("synonym-retarget"),
        ChangedObjectKind::GrantOrRevoke => String::from("grant-revoke"),
        ChangedObjectKind::EditionedObjectChange => String::from("editioned-object"),
        ChangedObjectKind::MaterializedViewRefreshAffecting => String::from("mview-refresh"),
        ChangedObjectKind::TriggerChange => String::from("trigger"),
        ChangedObjectKind::IndexChange => String::from("index"),
        ChangedObjectKind::SequenceChange => String::from("sequence"),
        ChangedObjectKind::OtherKnownKind { object_type } => format!("other:{object_type}"),
        ChangedObjectKind::Unclassified => String::from("unclassified"),
    }
}

fn reason_label(reason: &crate::InvalidationReason) -> String {
    use crate::InvalidationReason::*;
    match reason {
        PackageSpecChanged { .. } => String::from("package-spec-changed"),
        RoutineSignatureChanged { .. } => String::from("routine-signature-changed"),
        TableAdditive { .. } => String::from("table-additive"),
        TableDestructive { .. } => String::from("table-destructive"),
        TypeEvolution { .. } => String::from("type-evolution"),
        SynonymRetargeted { .. } => String::from("synonym-retarget"),
        PrivilegeChange => String::from("privilege-change"),
        MaterializedViewRefreshAffected { .. } => String::from("mview-refresh"),
        EditionedObjectChange => String::from("editioned-object-change"),
        SourceOnlyHeuristic => String::from("source-only-heuristic"),
        Other { description } => format!("other:{description}"),
    }
}

fn unknown_label(reason: UnknownReason) -> String {
    format!("{reason:?}")
}

fn classify_risk(
    changeset: &ChangeSet,
    prediction: Option<&InvalidationPrediction>,
) -> DeploymentRisk {
    let has_destructive = changeset
        .objects
        .iter()
        .any(|o| matches!(o.kind, ChangedObjectKind::TableDestructiveDdl));
    if has_destructive {
        return DeploymentRisk::Destructive;
    }
    let has_unclassified = changeset
        .objects
        .iter()
        .any(|o| matches!(o.kind, ChangedObjectKind::Unclassified));
    if has_unclassified {
        return DeploymentRisk::Unknown;
    }
    if let Some(prediction) = prediction {
        if !prediction.uncertainties.is_empty() {
            return DeploymentRisk::Caution;
        }
        if !prediction.predicted_invalidations.is_empty() {
            return DeploymentRisk::Safe;
        }
    }
    if !changeset.objects.is_empty() {
        return DeploymentRisk::Safe;
    }
    DeploymentRisk::Unknown
}

fn build_remediation_hints(
    changeset: &ChangeSet,
    prediction: Option<&InvalidationPrediction>,
    risk: DeploymentRisk,
) -> Vec<String> {
    let mut hints = Vec::new();
    match risk {
        DeploymentRisk::Destructive => {
            hints.push(String::from(
                "Destructive DDL detected. Pair every drop / type-narrowing / NOT NULL add with a rollback plan and rehearse against an isolated target (PLSQL-CICD-005 verify).",
            ));
        }
        DeploymentRisk::Unknown => {
            hints.push(String::from(
                "Unclassified changes present — re-run `predict --mode catalog-aware` (or `live-snapshot`) before gating.",
            ));
        }
        DeploymentRisk::Caution => {
            hints.push(String::from(
                "Predicted uncertainties present — review the uncertainty inventory and raise predict mode if catalog data is available.",
            ));
        }
        DeploymentRisk::Safe => {}
    }
    if changeset
        .objects
        .iter()
        .any(|o| !o.uncertainties.is_empty())
    {
        hints.push(String::from(
            "One or more ChangedObjects carry per-object UnknownReason tags (R13). Inspect `uncertainty_counts` and address the typed reasons before deploy.",
        ));
    }
    if let Some(prediction) = prediction {
        if matches!(prediction.mode, crate::PredictMode::SourceOnly) {
            hints.push(String::from(
                "Prediction ran in `source-only` mode — confidence is Low. Re-run with `catalog-aware` or `live-snapshot` for High-confidence reasoning before gating production.",
            ));
        }
    }
    if !changeset.unclassified_files.is_empty() {
        hints.push(String::from(
            "Some files in the changeset did not classify into a ChangedObject. The lineage Layer 4 classifier (PLSQL-LIN-007A) will help; run it before invoking predict.",
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangedObject, PredictMode, predict};
    use plsql_core::{ObjectName, SchemaName, SymbolId};

    fn billing() -> SchemaName {
        SchemaName::new(SymbolId::new(1))
    }

    fn obj(id: u64) -> ObjectName {
        ObjectName::new(SymbolId::new(id))
    }

    fn changed(kind: ChangedObjectKind, name: u64) -> ChangedObject {
        ChangedObject {
            owner: billing(),
            name: obj(name),
            kind,
            new_hash: None,
            previous_hash: None,
            file_paths: vec![],
            uncertainties: vec![],
        }
    }

    #[test]
    fn empty_changeset_reports_unknown_risk() {
        let report = doctor_report(&ChangeSet::empty(), None);
        assert_eq!(report.object_count, 0);
        assert!(matches!(report.overall_risk, DeploymentRisk::Unknown));
    }

    #[test]
    fn destructive_ddl_drives_destructive_risk_and_remediation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::TableDestructiveDdl, 100)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let report = doctor_report(&changeset, Some(&prediction));
        assert!(matches!(report.overall_risk, DeploymentRisk::Destructive));
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("rollback plan"))
        );
    }

    #[test]
    fn unclassified_changeset_emits_remediation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::Unclassified, 200)],
            ..ChangeSet::empty()
        };
        let report = doctor_report(&changeset, None);
        assert!(matches!(report.overall_risk, DeploymentRisk::Unknown));
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("Unclassified"))
        );
    }

    #[test]
    fn source_only_mode_appends_low_confidence_remediation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageSpec, 300)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::SourceOnly);
        let report = doctor_report(&changeset, Some(&prediction));
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("source-only"))
        );
    }

    #[test]
    fn report_aggregates_kind_and_reason_counts() {
        let changeset = ChangeSet {
            objects: vec![
                changed(ChangedObjectKind::PackageSpec, 1),
                changed(ChangedObjectKind::PackageSpec, 2),
                changed(ChangedObjectKind::TableAdditiveDdl, 3),
            ],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let report = doctor_report(&changeset, Some(&prediction));
        assert_eq!(report.object_count, 3);
        let pkg_row = report
            .object_kind_counts
            .iter()
            .find(|r| r.kind == "package-spec")
            .unwrap();
        assert_eq!(pkg_row.count, 2);
        let add_row = report
            .object_kind_counts
            .iter()
            .find(|r| r.kind == "table-additive")
            .unwrap();
        assert_eq!(add_row.count, 1);
        // 3 invalidations expected (one per object).
        assert_eq!(report.invalidation_total, 3);
    }

    #[test]
    fn report_propagates_object_uncertainty_into_remediation() {
        let mut obj_with_unknown = changed(ChangedObjectKind::PackageSpec, 100);
        obj_with_unknown
            .uncertainties
            .push(UnknownReason::DynamicSqlOpaque);
        let changeset = ChangeSet {
            objects: vec![obj_with_unknown],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        let report = doctor_report(&changeset, Some(&prediction));
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("per-object UnknownReason"))
        );
    }
}
