//! `predict <changeset>` implementation.
//!
//! Combines a `ChangeSet` (from) with Oracle-specific
//! invalidation rules to emit an `InvalidationPrediction`. The rule
//! engine is intentionally text-table — every rule names the
//! `ChangedObjectKind` it triggers on, the kind of invalidation it
//! emits, and the confidence band — so adding a new Oracle 23ai rule
//! is one row, not a code re-architecture.
//!
//! When a lineage `DepGraph` is supplied (`predict_with_lineage`), the
//! function walks dependents via the lineage crate's `impact()` and
//! attaches each transitive dependent as a `PredictedInvalidation`.

use plsql_core::{Confidence, ConfidenceLevel, UnknownReason};

use crate::{
    ChangeSet, ChangedObject, ChangedObjectKind, InvalidationPrediction, InvalidationReason,
    PredictMode, PredictedInvalidation, UncertaintyRecord,
};

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

    // Sort by `(distance, owner, name)` so reports diff cleanly across runs.
    prediction
        .predicted_invalidations
        .sort_by_key(|p| (p.distance, p.owner, p.name));
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
    // invalidations; transitive ones land via `predict_with_lineage`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{ObjectName, SchemaName, SymbolId};

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

    #[test]
    fn predict_empty_changeset_returns_empty_prediction() {
        let prediction = predict(&ChangeSet::empty(), PredictMode::CatalogAware);
        assert!(prediction.predicted_invalidations.is_empty());
        assert!(prediction.uncertainties.is_empty());
    }

    #[test]
    fn package_spec_change_emits_invalidation() {
        let changeset = ChangeSet {
            objects: vec![changed(ChangedObjectKind::PackageSpec, 100)],
            ..ChangeSet::empty()
        };
        let prediction = predict(&changeset, PredictMode::CatalogAware);
        assert_eq!(prediction.predicted_invalidations.len(), 1);
        let row = &prediction.predicted_invalidations[0];
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
            prediction.predicted_invalidations[0].confidence.level,
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
        assert!(matches!(
            prediction.predicted_invalidations[0].reason,
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
