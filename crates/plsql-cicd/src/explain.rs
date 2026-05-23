//! `explain-lifecycle` — customer-facing CI/CD report.
//!
//! Translates an `InvalidationPrediction` into a structured Markdown +
//! JSON report a release reviewer can attach to a change ticket. Each
//! predicted invalidation gets a category, evidence text, and a fixed
//! safety-warning string. The safety language is FIXED (not generated)
//! because it's compliance-relevant and must be reviewed by humans.

use plsql_core::ConfidenceLevel;
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};

use crate::{InvalidationPrediction, InvalidationReason};

pub const EXPLAIN_LIFECYCLE_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.cicd.explain_lifecycle",
    version: SchemaVersion::new(1, 0, 0),
    description: "Customer-facing explanation of an InvalidationPrediction with evidence + safety warnings",
};

/// A single explained invalidation row in the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainLifecycleRow {
    pub object: String,
    pub object_type: String,
    pub category: String,
    pub evidence: String,
    pub safety_warning: String,
    pub distance: u32,
    pub confidence: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainLifecycleReport {
    pub mode: String,
    pub total_invalidations: usize,
    pub rows: Vec<ExplainLifecycleRow>,
    pub uncertainty_count: usize,
}

/// Build the report from an `InvalidationPrediction`.
#[must_use]
pub fn explain_lifecycle(prediction: &InvalidationPrediction) -> ExplainLifecycleReport {
    let mut report = ExplainLifecycleReport {
        mode: format!("{:?}", prediction.mode).to_lowercase(),
        total_invalidations: prediction.predicted_invalidations.len(),
        uncertainty_count: prediction.uncertainties.len(),
        rows: Vec::new(),
    };
    for inv in &prediction.predicted_invalidations {
        let category = category_for(&inv.reason);
        let safety_warning = safety_warning_for(&inv.reason);
        let evidence = evidence_for(&inv.reason);
        report.rows.push(ExplainLifecycleRow {
            object: format!("{:?}.{:?}", inv.owner, inv.name),
            object_type: inv.object_type.clone(),
            category: category.to_owned(),
            evidence,
            safety_warning: safety_warning.to_owned(),
            distance: inv.distance,
            confidence: confidence_label(inv.confidence.level).to_owned(),
        });
    }
    report
}

/// Wrap a [`ExplainLifecycleReport`] in a versioned envelope.
#[must_use]
pub fn explain_lifecycle_envelope(
    report: ExplainLifecycleReport,
) -> RobotJsonEnvelope<ExplainLifecycleReport> {
    RobotJsonEnvelope::new(EXPLAIN_LIFECYCLE_SCHEMA, report)
}

fn category_for(reason: &InvalidationReason) -> &'static str {
    match reason {
        InvalidationReason::PackageSpecChanged { .. } => "package-spec-change",
        InvalidationReason::RoutineSignatureChanged { .. } => "routine-signature-change",
        InvalidationReason::TableAdditive { .. } => "table-additive-ddl",
        InvalidationReason::TableDestructive { .. } => "table-destructive-ddl",
        InvalidationReason::TypeEvolution { .. } => "type-evolution",
        InvalidationReason::SynonymRetargeted { .. } => "synonym-retarget",
        InvalidationReason::PrivilegeChange => "privilege-change",
        InvalidationReason::MaterializedViewRefreshAffected { .. } => "mview-refresh-affecting",
        InvalidationReason::EditionedObjectChange => "editioned-object-change",
        InvalidationReason::SourceOnlyHeuristic => "source-only-heuristic",
        InvalidationReason::Other { .. } => "other",
    }
}

fn safety_warning_for(reason: &InvalidationReason) -> &'static str {
    match reason {
        InvalidationReason::PackageSpecChanged { .. } => {
            "Every dependent of this spec invalidates on next reference. Recompile order matters; \
             use `recompile_order` to drive deployment."
        }
        InvalidationReason::RoutineSignatureChanged { .. } => {
            "Callers of this routine will fail until recompiled. Verify no live sessions hold a \
             cursor against the old signature before deploying."
        }
        InvalidationReason::TableAdditive { .. } => {
            "Additive DDL is usually safe but %ROWTYPE-dependent code may still recompile. \
             Surrounding views may need rebuild."
        }
        InvalidationReason::TableDestructive { .. } => {
            "DROP COLUMN / DROP TABLE / column-type change can break dependents. Run \
             `what-breaks` against the changeset before any production deploy."
        }
        InvalidationReason::TypeEvolution { .. } => {
            "Type evolution is allowed only via ALTER TYPE ... CASCADE INCLUDING DATA. Verify \
             every materialised type-column has converted before signing off."
        }
        InvalidationReason::SynonymRetargeted { .. } => {
            "Retargeting a synonym is silent at compile time but rewrites the dependent's \
             resolution at next parse. Force a hard-parse to validate."
        }
        InvalidationReason::PrivilegeChange => {
            "Privilege changes don't recompile but can break invoker-rights code paths. Audit \
             AUTHID CURRENT_USER routines that depend on this grant."
        }
        InvalidationReason::MaterializedViewRefreshAffected { .. } => {
            "Materialised-view refresh may need re-running or a new refresh group definition. \
             Verify ON COMMIT vs ON DEMAND policy still matches the new dependency shape."
        }
        InvalidationReason::EditionedObjectChange => {
            "Edition-based redefinition isolates the change to the active edition until cutover. \
             Confirm the cross-edition trigger covers any data the new edition will read."
        }
        InvalidationReason::SourceOnlyHeuristic => {
            "Source-only mode — confidence is LOW. Re-run with `catalog-aware` or \
             `live-snapshot` for High-confidence reasoning before gating production."
        }
        InvalidationReason::Other { .. } => {
            "Cause not narrowed — engine recorded a conservative fallback. Treat as uncertain; \
             additional manual review recommended before deploy."
        }
    }
}

fn evidence_for(reason: &InvalidationReason) -> String {
    match reason {
        InvalidationReason::PackageSpecChanged {
            spec_owner,
            spec_name,
        } => format!("package spec `{:?}.{:?}` changed", spec_owner, spec_name),
        InvalidationReason::RoutineSignatureChanged {
            routine_owner,
            routine_name,
        } => format!(
            "routine signature `{:?}.{:?}` changed",
            routine_owner, routine_name
        ),
        InvalidationReason::TableAdditive {
            table_owner,
            table_name,
        } => format!(
            "table `{:?}.{:?}` received additive DDL",
            table_owner, table_name
        ),
        InvalidationReason::TableDestructive {
            table_owner,
            table_name,
        } => format!(
            "table `{:?}.{:?}` received destructive DDL",
            table_owner, table_name
        ),
        InvalidationReason::TypeEvolution {
            type_owner,
            type_name,
        } => format!("type `{:?}.{:?}` evolved", type_owner, type_name),
        InvalidationReason::SynonymRetargeted {
            synonym_owner,
            synonym_name,
        } => format!(
            "synonym `{:?}.{:?}` retargeted",
            synonym_owner, synonym_name
        ),
        InvalidationReason::PrivilegeChange => "privilege grant/revoke".to_owned(),
        InvalidationReason::MaterializedViewRefreshAffected {
            mview_owner,
            mview_name,
        } => format!(
            "materialized view `{:?}.{:?}` refresh affected",
            mview_owner, mview_name
        ),
        InvalidationReason::EditionedObjectChange => "editioned object replaced".to_owned(),
        InvalidationReason::SourceOnlyHeuristic => {
            "source-only mode — catalog not consulted".to_owned()
        }
        InvalidationReason::Other { description } => description.clone(),
    }
}

fn confidence_label(level: ConfidenceLevel) -> &'static str {
    match level {
        ConfidenceLevel::High => "high",
        ConfidenceLevel::Medium => "medium",
        ConfidenceLevel::Low => "low",
        ConfidenceLevel::Opaque => "opaque",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChangeSet, ChangeSetOrigin, ChangedObject, ChangedObjectKind, PredictMode, predict,
    };
    use plsql_core::{ObjectName, SchemaName, SymbolInterner};
    use std::path::PathBuf;

    fn make_changeset() -> ChangeSet {
        let mut interner = SymbolInterner::new();
        let billing = interner.intern("billing").expect("intern");
        let customers = interner.intern("customers").expect("intern");
        ChangeSet {
            origin: Some(ChangeSetOrigin::GitDiff {
                range: String::from("main..feature"),
            }),
            objects: vec![ChangedObject {
                owner: SchemaName::from(billing),
                name: ObjectName::from(customers),
                kind: ChangedObjectKind::TableDestructiveDdl,
                new_hash: None,
                previous_hash: None,
                file_paths: vec![PathBuf::from("billing/customers.sql")],
                uncertainties: vec![],
            }],
            unclassified_files: vec![],
        }
    }

    #[test]
    fn explain_lifecycle_renders_rows_with_safety_text() {
        let cs = make_changeset();
        let prediction = predict(&cs, PredictMode::CatalogAware);
        let report = explain_lifecycle(&prediction);
        assert!(!report.rows.is_empty());
        for row in &report.rows {
            assert!(!row.safety_warning.is_empty());
            assert!(!row.category.is_empty());
            assert!(!row.evidence.is_empty());
        }
    }

    #[test]
    fn explain_lifecycle_envelope_pins_schema() {
        let cs = make_changeset();
        let prediction = predict(&cs, PredictMode::CatalogAware);
        let report = explain_lifecycle(&prediction);
        let envelope = explain_lifecycle_envelope(report);
        assert_eq!(envelope.schema_id, EXPLAIN_LIFECYCLE_SCHEMA.id);
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("plsql.cicd.explain_lifecycle"));
    }
}
