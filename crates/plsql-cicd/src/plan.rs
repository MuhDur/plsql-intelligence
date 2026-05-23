//! `plan <changeset>`.
//!
//! Consumes a `(ChangeSet, InvalidationPrediction)` pair and emits
//! a topologically-sorted [`DeploymentPlan`]: the ordered DDL +
//! `ALTER … COMPILE` recompilations the operator should apply,
//! plus the overall risk classification used by the gate.
//!
//! The plan is deterministic and pure — no Oracle round trip, no
//! filesystem I/O. It runs the same Layer 4 / Layer 5 inputs the
//! `predict` step produced, applies the recompile ordering rules
//! from `plsql_lineage::recompile_order`, and stitches everything
//! into the [`DeploymentPlan`] shape defined in `lib.rs`.
//!
//! ## Ordering algorithm
//!
//! 1. Sort `changeset.objects` by an object-kind weight so tables
//!    land before packages before triggers (since triggers reference
//!    table columns).
//! 2. Within each kind, sort by `(owner, name)` so the plan diffs
//!    cleanly across re-runs.
//! 3. Append every `recompile_order` entry as a synthesised
//!    `ALTER … COMPILE` statement, deduplicating any object that
//!    already shipped a real DDL statement in step 2.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` — PL/SQL Language Reference routing
//!   for DDL invalidation cascade semantics (`ALL_DEPENDENCIES`
//!   STATUS column drives the recompile order).
//! * `LOW-LEVEL-CATALOGS.md` — Data Dictionary View Families:
//!   `ALL_OBJECTS.OBJECT_TYPE` strings (`TABLE`, `PACKAGE`,
//!   `PACKAGE BODY`, `VIEW`, `TRIGGER`, …) are the same kind labels
//!   the change-set carries. Supplied package bucket: the
//!   `ALTER … COMPILE` form is the canonical Oracle recompile
//!   verb (see PL/SQL Packages and Types Reference, ALTER PROCEDURE
//!   / ALTER PACKAGE chapters).

use std::collections::BTreeSet;

use plsql_core::{ObjectName, SchemaName};

use crate::{
    ChangeSet, ChangedObject, ChangedObjectKind, DeploymentPlan, DeploymentRisk,
    DeploymentStatement, DeploymentStatementKind, InvalidationPrediction, RecompileItem,
};

/// Render `(owner, name)` as the `("schema_<id>", "object_<id>")` pair used
/// both as the dedupe key and as substitution tokens in the synthesised SQL.
fn qualified_names(owner: SchemaName, name: ObjectName) -> (String, String) {
    (
        format!("schema_{}", owner.symbol().get()),
        format!("object_{}", name.symbol().get()),
    )
}

/// Build a `DeploymentPlan` from a changeset + prediction.
///
/// The function is total — every legal input produces a plan. The
/// `overall_risk` field reflects the highest-severity object in the
/// changeset (Destructive > Risky > Safe).
#[must_use]
pub fn plan_changeset(
    changeset: &ChangeSet,
    prediction: &InvalidationPrediction,
) -> DeploymentPlan {
    let mut statements: Vec<DeploymentStatement> = Vec::new();
    let mut ordinal: u32 = 1;
    let mut already_emitted: BTreeSet<(String, String)> = BTreeSet::new();

    // Step 1+2: emit one DDL statement per ChangedObject in stable
    // order. `kind_weight` puts schema-foundational objects (tables,
    // sequences) before dependents (packages, views, triggers).
    let mut sorted_objects: Vec<&ChangedObject> = changeset.objects.iter().collect();
    sorted_objects.sort_by(|a, b| {
        kind_weight(&a.kind)
            .cmp(&kind_weight(&b.kind))
            .then(a.owner.symbol().get().cmp(&b.owner.symbol().get()))
            .then(a.name.symbol().get().cmp(&b.name.symbol().get()))
    });

    for obj in sorted_objects {
        let (owner_owned, name_owned) = qualified_names(obj.owner, obj.name);
        already_emitted.insert((owner_owned.clone(), name_owned.clone()));
        statements.push(DeploymentStatement {
            ordinal,
            kind: DeploymentStatementKind::Ddl,
            sql: synthesise_ddl_for_kind(&obj.kind, &owner_owned, &name_owned),
            source_file: obj.file_paths.first().cloned(),
            target_owner: Some(obj.owner),
            target_name: Some(obj.name),
        });
        ordinal += 1;
    }

    // Step 3: append ALTER … COMPILE statements for every recompile
    // entry not already covered. Using `RecompileItem.force_compile`
    // to decide between a real ALTER vs a no-op note line.
    for recompile in &prediction.recompile_order {
        let key = qualified_names(recompile.owner, recompile.name);
        if already_emitted.contains(&key) {
            continue;
        }
        statements.push(DeploymentStatement {
            ordinal,
            kind: DeploymentStatementKind::Recompile,
            sql: synthesise_recompile_for_kind(recompile),
            source_file: None,
            target_owner: Some(recompile.owner),
            target_name: Some(recompile.name),
        });
        ordinal += 1;
    }

    DeploymentPlan {
        changeset: changeset.clone(),
        prediction: prediction.clone(),
        statements,
        overall_risk: compute_overall_risk(changeset),
        notes: build_notes(changeset, prediction),
    }
}

/// Kind weight: lower weights deploy first. Mirrors Oracle's
/// implicit object-dependency layering (foundations → containers →
/// hangers-on).
fn kind_weight(kind: &ChangedObjectKind) -> u8 {
    use ChangedObjectKind as K;
    match kind {
        K::SequenceChange => 0,
        K::TypeEvolution => 1,
        K::TableAdditiveDdl | K::TableDestructiveDdl => 2,
        K::ViewDefinitionChange | K::MaterializedViewRefreshAffecting => 3,
        K::PackageSpec => 4,
        K::PackageBody => 5,
        K::StandaloneRoutineSignature | K::StandaloneRoutineBody => 6,
        K::TriggerChange => 7,
        K::SynonymRetargeting => 8,
        K::IndexChange => 9,
        K::GrantOrRevoke => 10,
        K::EditionedObjectChange => 11,
        K::OtherKnownKind { .. } => 12,
        K::Unclassified => 13,
    }
}

fn synthesise_ddl_for_kind(kind: &ChangedObjectKind, owner: &str, name: &str) -> String {
    use ChangedObjectKind as K;
    match kind {
        K::PackageSpec => {
            format!("-- apply CREATE OR REPLACE PACKAGE {owner}.{name} from source file")
        }
        K::PackageBody => {
            format!("-- apply CREATE OR REPLACE PACKAGE BODY {owner}.{name} from source file")
        }
        K::StandaloneRoutineSignature | K::StandaloneRoutineBody => {
            format!("-- apply CREATE OR REPLACE PROCEDURE/FUNCTION {owner}.{name} from source file")
        }
        K::ViewDefinitionChange => {
            format!("-- apply CREATE OR REPLACE VIEW {owner}.{name} from source file")
        }
        K::MaterializedViewRefreshAffecting => {
            format!("-- inspect MATERIALIZED VIEW {owner}.{name} refresh chain after deploy")
        }
        K::TriggerChange => {
            format!("-- apply CREATE OR REPLACE TRIGGER {owner}.{name} from source file")
        }
        K::TypeEvolution => {
            format!("-- apply ALTER TYPE {owner}.{name} (evolution — review dependents)")
        }
        K::SynonymRetargeting => {
            format!("-- apply CREATE OR REPLACE SYNONYM {owner}.{name} from source file")
        }
        K::TableAdditiveDdl => {
            format!("-- apply additive ALTER TABLE {owner}.{name} (safe forward-compatible change)")
        }
        K::TableDestructiveDdl => format!(
            "-- apply DESTRUCTIVE ALTER/DROP on TABLE {owner}.{name} — verify with operator"
        ),
        K::SequenceChange => format!("-- apply DDL for SEQUENCE {owner}.{name}"),
        K::IndexChange => format!("-- apply DDL for INDEX {owner}.{name}"),
        K::GrantOrRevoke => format!("-- apply GRANT / REVOKE on {owner}.{name}"),
        K::EditionedObjectChange => {
            format!("-- apply edition-aware DDL on {owner}.{name} (carries editioning concerns)")
        }
        K::OtherKnownKind { object_type } => {
            format!("-- apply DDL for {object_type} {owner}.{name}")
        }
        K::Unclassified => {
            format!("-- apply unclassified change for {owner}.{name} (see file_paths)")
        }
    }
}

fn synthesise_recompile_for_kind(recompile: &RecompileItem) -> String {
    let (owner, name) = qualified_names(recompile.owner, recompile.name);
    let verb = recompile_alter_verb(&recompile.object_type.to_ascii_uppercase());
    if recompile.force_compile {
        format!("ALTER {verb} {owner}.{name} COMPILE")
    } else {
        format!("-- {verb} {owner}.{name} marked for lazy recompile (no explicit ALTER required)")
    }
}

fn recompile_alter_verb(kind: &str) -> &'static str {
    match kind {
        "PACKAGE" | "PACKAGE BODY" => "PACKAGE",
        "VIEW" => "VIEW",
        "PROCEDURE" => "PROCEDURE",
        "FUNCTION" => "FUNCTION",
        "TRIGGER" => "TRIGGER",
        "TYPE" | "TYPE BODY" => "TYPE",
        _ => "OBJECT",
    }
}

fn compute_overall_risk(changeset: &ChangeSet) -> DeploymentRisk {
    use ChangedObjectKind as K;
    let mut worst = DeploymentRisk::Safe;
    for obj in &changeset.objects {
        match obj.kind {
            K::TableDestructiveDdl => return DeploymentRisk::Destructive,
            K::TableAdditiveDdl
            | K::TriggerChange
            | K::TypeEvolution
            | K::EditionedObjectChange
            | K::MaterializedViewRefreshAffecting => worst = DeploymentRisk::Caution,
            _ => {}
        }
    }
    worst
}

fn build_notes(changeset: &ChangeSet, prediction: &InvalidationPrediction) -> Vec<String> {
    let mut notes = Vec::new();
    if !changeset.unclassified_files.is_empty() {
        notes.push(format!(
            "{} unclassified file(s) present — review before deploy",
            changeset.unclassified_files.len()
        ));
    }
    if !prediction.uncertainties.is_empty() {
        notes.push(format!(
            "{} uncertainty record(s) (R13) — gate before deploy",
            prediction.uncertainties.len()
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PredictMode, RecompileItem};
    use plsql_core::{ObjectName, SchemaName, SymbolId};

    fn obj(kind: ChangedObjectKind, owner: u64, name: u64) -> ChangedObject {
        ChangedObject {
            owner: SchemaName::from(SymbolId::new(owner)),
            name: ObjectName::from(SymbolId::new(name)),
            kind,
            new_hash: None,
            previous_hash: None,
            file_paths: vec![],
            uncertainties: vec![],
        }
    }

    fn recompile(kind: &str, owner: u64, name: u64, force: bool) -> RecompileItem {
        RecompileItem {
            owner: SchemaName::from(SymbolId::new(owner)),
            name: ObjectName::from(SymbolId::new(name)),
            object_type: kind.into(),
            force_compile: force,
        }
    }

    fn empty_prediction() -> InvalidationPrediction {
        InvalidationPrediction {
            mode: PredictMode::SourceOnly,
            ..Default::default()
        }
    }

    #[test]
    fn empty_changeset_yields_empty_plan() {
        let plan = plan_changeset(&ChangeSet::default(), &empty_prediction());
        assert!(plan.statements.is_empty());
        assert!(plan.notes.is_empty());
        assert!(matches!(plan.overall_risk, DeploymentRisk::Safe));
    }

    #[test]
    fn objects_emit_in_kind_weight_order() {
        let cs = ChangeSet {
            origin: None,
            objects: vec![
                obj(ChangedObjectKind::TriggerChange, 1, 4),
                obj(ChangedObjectKind::PackageBody, 1, 3),
                obj(ChangedObjectKind::TableAdditiveDdl, 1, 1),
                obj(ChangedObjectKind::ViewDefinitionChange, 1, 2),
            ],
            unclassified_files: vec![],
        };
        let plan = plan_changeset(&cs, &empty_prediction());
        let kinds: Vec<_> = plan
            .statements
            .iter()
            .map(|s| s.target_name.unwrap().symbol().get())
            .collect();
        assert_eq!(kinds, vec![1, 2, 3, 4], "{:?}", plan.statements);
    }

    #[test]
    fn ordinals_are_monotonic_starting_at_one() {
        let cs = ChangeSet {
            origin: None,
            objects: vec![
                obj(ChangedObjectKind::TableAdditiveDdl, 1, 10),
                obj(ChangedObjectKind::ViewDefinitionChange, 1, 11),
            ],
            unclassified_files: vec![],
        };
        let plan = plan_changeset(&cs, &empty_prediction());
        for (i, st) in plan.statements.iter().enumerate() {
            assert_eq!(st.ordinal, i as u32 + 1);
        }
    }

    #[test]
    fn recompile_entries_get_appended_unless_duplicate() {
        let cs = ChangeSet {
            origin: None,
            objects: vec![obj(ChangedObjectKind::PackageSpec, 1, 100)],
            unclassified_files: vec![],
        };
        let mut pred = empty_prediction();
        pred.recompile_order = vec![
            // Same object as the DDL — should be deduped.
            recompile("PACKAGE", 1, 100, true),
            // New object — should emit ALTER VIEW … COMPILE.
            recompile("VIEW", 1, 200, true),
        ];
        let plan = plan_changeset(&cs, &pred);
        assert_eq!(plan.statements.len(), 2);
        assert!(matches!(
            plan.statements[0].kind,
            DeploymentStatementKind::Ddl
        ));
        assert!(matches!(
            plan.statements[1].kind,
            DeploymentStatementKind::Recompile
        ));
        assert!(plan.statements[1].sql.contains("ALTER VIEW"));
    }

    #[test]
    fn risky_kinds_lift_overall_risk_to_risky() {
        let cs = ChangeSet {
            origin: None,
            objects: vec![
                obj(ChangedObjectKind::PackageBody, 1, 1),
                obj(ChangedObjectKind::TableAdditiveDdl, 1, 2),
            ],
            unclassified_files: vec![],
        };
        let plan = plan_changeset(&cs, &empty_prediction());
        assert!(matches!(plan.overall_risk, DeploymentRisk::Caution));
    }

    #[test]
    fn destructive_table_lifts_overall_risk_to_destructive() {
        let cs = ChangeSet {
            origin: None,
            objects: vec![
                obj(ChangedObjectKind::PackageBody, 1, 1),
                obj(ChangedObjectKind::TableDestructiveDdl, 1, 2),
            ],
            unclassified_files: vec![],
        };
        let plan = plan_changeset(&cs, &empty_prediction());
        assert!(matches!(plan.overall_risk, DeploymentRisk::Destructive));
    }

    #[test]
    fn recompile_force_false_emits_note_not_alter() {
        let pred = InvalidationPrediction {
            mode: PredictMode::SourceOnly,
            recompile_order: vec![recompile("VIEW", 1, 300, false)],
            ..Default::default()
        };
        let plan = plan_changeset(&ChangeSet::default(), &pred);
        assert_eq!(plan.statements.len(), 1);
        assert!(!plan.statements[0].sql.starts_with("ALTER"));
        assert!(plan.statements[0].sql.contains("lazy recompile"));
    }

    #[test]
    fn unclassified_files_produce_review_note() {
        use std::path::PathBuf;
        let cs = ChangeSet {
            origin: None,
            objects: vec![],
            unclassified_files: vec![PathBuf::from("scripts/cleanup.sql")],
        };
        let plan = plan_changeset(&cs, &empty_prediction());
        assert!(plan.notes.iter().any(|n| n.contains("unclassified")));
    }

    #[test]
    fn uncertainties_produce_gate_note() {
        use crate::UncertaintyRecord;
        use plsql_core::UnknownReason;
        let mut pred = empty_prediction();
        pred.uncertainties = vec![UncertaintyRecord {
            reason: UnknownReason::DynamicSqlOpaque,
            affected_owner: None,
            affected_name: None,
            description: "test".into(),
        }];
        let plan = plan_changeset(&ChangeSet::default(), &pred);
        assert!(plan.notes.iter().any(|n| n.contains("uncertainty")));
    }
}
