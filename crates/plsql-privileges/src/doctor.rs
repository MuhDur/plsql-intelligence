//! Doctor surface for [`PrivilegeModel`].
//!
//! Aggregates per-model completeness signals so an agent reading the
//! engine output has a single first-stop for "how complete is my
//! privilege analysis?". Follows the project-wide
//! `/world-class-doctor-mode` convention: one stable shape per layer,
//! always serde-able, always derivable in O(n) from the model.

use serde::{Deserialize, Serialize};

use plsql_core::UnknownReason;

use crate::model::{AuthorizationMode, PrivilegeModel};

/// Aggregated diagnostic counts for a single [`PrivilegeModel`]. The
/// shape is stable across versions — new fields are added behind
/// `#[serde(default)]` so older snapshots keep deserializing.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PrivilegeDoctorReport {
    /// Total resolved privileges.
    pub privileges_total: usize,
    /// Resolved privileges granted to `PUBLIC`.
    pub public_grants_total: usize,
    /// Synonym-mediated privilege paths.
    pub synonym_paths_total: usize,
    /// Public-synonym paths (`is_public == true`).
    pub public_synonym_paths: usize,
    /// `ACCESSIBLE BY` entries observed.
    pub access_control_entries_total: usize,
    /// Cross-schema writes observed.
    pub cross_schema_writes_total: usize,
    /// Cross-schema writes whose authorization could not be resolved
    /// statically (carries `runtime_ambiguity = Some(_)`).
    pub cross_schema_writes_ambiguous: usize,
    /// `AuthorizationAmbiguity` entries — authorizations that flip on
    /// runtime role state.
    pub authorization_ambiguities_total: usize,
    /// Distinct `UnknownReason` codes seen in the ambiguity list,
    /// sorted by reason code for stable output.
    pub ambiguity_reasons: Vec<DoctorReasonRow>,
    /// Distinct schemas mentioned across the model. Useful to
    /// cross-check against [`plsql_catalog::CatalogDoctorReport`] — if
    /// the catalog has N schemas but the privilege model only mentions
    /// M, the gap is surfaced by callers.
    pub schemas_observed_total: usize,
    /// Distinct `definer` and `invoker` AuthorizationMode counts seen
    /// in evidence. We do not currently track AuthorizationMode on
    /// every ResolvedPrivilege — the surface is reserved for the
    /// engine layer to populate when the orchestration ships.
    pub authid_distribution: AuthidDistribution,
    /// Diagnostics recorded on the model itself.
    pub diagnostics_total: usize,
    /// Overall posture — `Clean` / `Caution` / `Unknown`. Set by
    /// [`classify_posture`].
    pub posture: PrivilegePosture,
    /// One-line operator hints derived from the counts above.
    pub remediation_hints: Vec<String>,
}

/// Per-reason count row. Sorted by `reason` for stable serialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorReasonRow {
    pub reason: UnknownReason,
    pub count: usize,
}

/// Bucketed `AUTHID` distribution. Pre-populated with zeros so consumers
/// can rely on every bucket existing even when the count is 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthidDistribution {
    pub definer: usize,
    pub invoker: usize,
}

impl AuthidDistribution {
    /// Record one observation of `mode`.
    pub fn record(&mut self, mode: AuthorizationMode) {
        match mode {
            AuthorizationMode::Definer => self.definer = self.definer.saturating_add(1),
            AuthorizationMode::Invoker => self.invoker = self.invoker.saturating_add(1),
        }
    }
}

/// Overall posture for the privilege model. Three-state by design —
/// `Caution` is for anything that an agent should investigate; `Unknown`
/// is reserved for cases where the model itself is suspect (e.g.
/// `runtime_ambiguities` outnumber `privileges`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum PrivilegePosture {
    /// No ambiguity / no cross-schema-write uncertainty.
    Clean,
    /// At least one ambiguity or one cross-schema-write requires
    /// follow-up but the model is otherwise consistent.
    #[default]
    Caution,
    /// The model contains more uncertainty than resolved data — the
    /// privilege resolver itself likely has gaps in catalog input.
    Unknown,
}

/// Build a [`PrivilegeDoctorReport`] from a [`PrivilegeModel`].
#[must_use]
pub fn doctor_report(model: &PrivilegeModel) -> PrivilegeDoctorReport {
    let mut ambiguity_counts: std::collections::BTreeMap<UnknownReason, usize> =
        std::collections::BTreeMap::new();
    for entry in &model.runtime_ambiguities {
        *ambiguity_counts.entry(entry.reason).or_insert(0) += 1;
    }
    let cross_schema_writes_ambiguous = model
        .cross_schema_writes
        .iter()
        .filter(|w| w.runtime_ambiguity.is_some())
        .count();
    let public_synonym_paths = model.synonym_paths.iter().filter(|p| p.is_public).count();
    let schemas_observed_total = distinct_schema_count(model);

    let ambiguity_reasons = ambiguity_counts
        .into_iter()
        .map(|(reason, count)| DoctorReasonRow { reason, count })
        .collect::<Vec<_>>();

    let posture = classify_posture(
        model.privileges.len(),
        model.runtime_ambiguities.len(),
        cross_schema_writes_ambiguous,
    );

    let remediation_hints = build_remediation_hints(
        model.privileges.len(),
        model.runtime_ambiguities.len(),
        cross_schema_writes_ambiguous,
        public_synonym_paths,
        model.diagnostics.len(),
    );

    PrivilegeDoctorReport {
        privileges_total: model.privileges.len(),
        public_grants_total: model.public_grants.len(),
        synonym_paths_total: model.synonym_paths.len(),
        public_synonym_paths,
        access_control_entries_total: model.access_control.len(),
        cross_schema_writes_total: model.cross_schema_writes.len(),
        cross_schema_writes_ambiguous,
        authorization_ambiguities_total: model.runtime_ambiguities.len(),
        ambiguity_reasons,
        schemas_observed_total,
        authid_distribution: AuthidDistribution::default(),
        diagnostics_total: model.diagnostics.len(),
        posture,
        remediation_hints,
    }
}

fn distinct_schema_count(model: &PrivilegeModel) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    let record = |seen: &mut std::collections::BTreeSet<plsql_core::SchemaName>,
                  s: plsql_core::SchemaName| {
        seen.insert(s);
    };
    for r in &model.privileges {
        record(&mut seen, r.object_owner);
    }
    for r in &model.public_grants {
        record(&mut seen, r.object_owner);
    }
    for entry in &model.access_control {
        record(&mut seen, entry.declaring_schema);
    }
    for w in &model.cross_schema_writes {
        record(&mut seen, w.caller_schema);
        record(&mut seen, w.target_schema);
    }
    for p in &model.synonym_paths {
        record(&mut seen, p.synonym_schema);
        record(&mut seen, p.target_schema);
    }
    seen.len()
}

fn classify_posture(
    privileges_total: usize,
    ambiguities_total: usize,
    cross_schema_ambiguous: usize,
) -> PrivilegePosture {
    if ambiguities_total > privileges_total && ambiguities_total > 0 {
        return PrivilegePosture::Unknown;
    }
    if ambiguities_total > 0 || cross_schema_ambiguous > 0 {
        return PrivilegePosture::Caution;
    }
    PrivilegePosture::Clean
}

fn build_remediation_hints(
    privileges_total: usize,
    ambiguities_total: usize,
    cross_schema_ambiguous: usize,
    public_synonym_paths: usize,
    diagnostics_total: usize,
) -> Vec<String> {
    let mut hints = Vec::new();
    if ambiguities_total > 0 {
        hints.push(format!(
            "Review {ambiguities_total} authorization ambiguity record(s) — \
             role-state-dependent authorizations need explicit role configuration."
        ));
    }
    if cross_schema_ambiguous > 0 {
        hints.push(format!(
            "{cross_schema_ambiguous} cross-schema write(s) carry a runtime ambiguity — \
             verify the calling unit has the expected grant chain at deploy time."
        ));
    }
    if public_synonym_paths > 0 {
        hints.push(format!(
            "{public_synonym_paths} public synonym path(s) observed — public synonyms can \
             be retargeted by anyone with CREATE PUBLIC SYNONYM, so they are an audit hotspot."
        ));
    }
    if diagnostics_total > 0 {
        hints.push(format!(
            "{diagnostics_total} diagnostic(s) emitted during privilege resolution — \
             read the model's diagnostics list for typed UnknownReason payloads."
        ));
    }
    if privileges_total == 0 && ambiguities_total == 0 {
        hints.push(String::from(
            "Privilege model is empty — confirm the catalog snapshot includes ALL_TAB_PRIVS \
             rows (capability probe should detect this in plsql-catalog).",
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    use plsql_catalog::{GrantPrivilege, Grantee};
    use plsql_core::{
        Confidence, ConfidenceLevel, Evidence, ObjectName, RoleName, SchemaName, SymbolId,
        UnknownReason,
    };

    use crate::model::{
        AccessControlEntry, AuthorizationAmbiguity, CrossSchemaWrite, PrivilegeModel,
        ResolvedPrivilege, SynonymPrivilegePath,
    };

    fn schema(id: u64) -> SchemaName {
        SchemaName::from(SymbolId::new(id))
    }

    fn object(id: u64) -> ObjectName {
        ObjectName::from(SymbolId::new(id))
    }

    fn role(id: u64) -> RoleName {
        RoleName::from(SymbolId::new(id))
    }

    fn priv_grant(owner: SchemaName, target: ObjectName) -> ResolvedPrivilege {
        ResolvedPrivilege {
            object_owner: owner,
            object_name: target,
            privilege: GrantPrivilege::Select,
            grantee: Grantee::Public,
            grant_option: crate::model::GrantOption::None,
            via_role: None,
            confidence: Confidence::new(ConfidenceLevel::High, None),
            evidence: Evidence::default(),
        }
    }

    #[test]
    fn empty_model_yields_clean_posture_with_setup_hint() {
        let model = PrivilegeModel::default();
        let report = doctor_report(&model);
        assert_eq!(report.posture, PrivilegePosture::Clean);
        assert_eq!(report.privileges_total, 0);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("Privilege model is empty"))
        );
    }

    #[test]
    fn ambiguities_drive_caution_posture_and_per_reason_counts() {
        let mut model = PrivilegeModel::default();
        model.privileges.push(priv_grant(schema(1), object(2)));
        model.privileges.push(priv_grant(schema(1), object(3)));
        model.runtime_ambiguities.push(AuthorizationAmbiguity {
            schema: schema(1),
            object: object(2),
            reason: UnknownReason::RuntimeGrantOrRole,
            dependent_roles: vec![role(7)],
            evidence: Evidence::default(),
        });
        let report = doctor_report(&model);
        assert_eq!(report.posture, PrivilegePosture::Caution);
        assert_eq!(report.authorization_ambiguities_total, 1);
        assert_eq!(report.ambiguity_reasons.len(), 1);
        assert_eq!(report.ambiguity_reasons[0].count, 1);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("authorization ambiguity record"))
        );
    }

    #[test]
    fn cross_schema_write_with_runtime_ambiguity_counts_separately() {
        let mut model = PrivilegeModel::default();
        model.cross_schema_writes.push(CrossSchemaWrite {
            caller_schema: schema(1),
            caller_object: object(2),
            target_schema: schema(4),
            target_object: object(5),
            privilege: GrantPrivilege::Update,
            confidence: Confidence::new(ConfidenceLevel::Medium, None),
            evidence: Evidence::default(),
            runtime_ambiguity: Some(UnknownReason::RuntimeGrantOrRole),
        });
        let report = doctor_report(&model);
        assert_eq!(report.cross_schema_writes_total, 1);
        assert_eq!(report.cross_schema_writes_ambiguous, 1);
        assert_eq!(report.posture, PrivilegePosture::Caution);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("cross-schema write"))
        );
    }

    #[test]
    fn ambiguity_outnumbering_privileges_yields_unknown_posture() {
        let mut model = PrivilegeModel::default();
        model.privileges.push(priv_grant(schema(1), object(2)));
        for object_id in 100..105 {
            model.runtime_ambiguities.push(AuthorizationAmbiguity {
                schema: schema(1),
                object: object(object_id),
                reason: UnknownReason::RuntimeGrantOrRole,
                dependent_roles: Vec::new(),
                evidence: Evidence::default(),
            });
        }
        let report = doctor_report(&model);
        assert_eq!(report.posture, PrivilegePosture::Unknown);
        assert_eq!(report.authorization_ambiguities_total, 5);
    }

    #[test]
    fn public_synonym_paths_are_counted_and_surfaced_as_hint() {
        let mut model = PrivilegeModel::default();
        for (idx, is_public) in [true, true, false].into_iter().enumerate() {
            model.synonym_paths.push(SynonymPrivilegePath {
                synonym_schema: schema((idx + 1) as u64),
                synonym_name: object((idx + 10) as u64),
                target_schema: schema((idx + 20) as u64),
                target_object: object((idx + 30) as u64),
                is_public,
                confidence: Confidence::new(ConfidenceLevel::High, None),
            });
        }
        let report = doctor_report(&model);
        assert_eq!(report.synonym_paths_total, 3);
        assert_eq!(report.public_synonym_paths, 2);
        assert!(
            report
                .remediation_hints
                .iter()
                .any(|h| h.contains("public synonym path"))
        );
    }

    #[test]
    fn distinct_schema_count_unions_all_record_kinds() {
        let mut model = PrivilegeModel::default();
        model.privileges.push(priv_grant(schema(1), object(10)));
        model.public_grants.push(priv_grant(schema(2), object(11)));
        model.access_control.push(AccessControlEntry {
            declaring_schema: schema(3),
            declaring_object: object(12),
            allowed_callers: Vec::new(),
        });
        model.cross_schema_writes.push(CrossSchemaWrite {
            caller_schema: schema(4),
            caller_object: object(13),
            target_schema: schema(5),
            target_object: object(14),
            privilege: GrantPrivilege::Update,
            confidence: Confidence::new(ConfidenceLevel::High, None),
            evidence: Evidence::default(),
            runtime_ambiguity: None,
        });
        let report = doctor_report(&model);
        assert_eq!(report.schemas_observed_total, 5);
    }

    #[test]
    fn authid_distribution_records_each_mode_once() {
        let mut dist = AuthidDistribution::default();
        dist.record(AuthorizationMode::Definer);
        dist.record(AuthorizationMode::Definer);
        dist.record(AuthorizationMode::Invoker);
        assert_eq!(dist.definer, 2);
        assert_eq!(dist.invoker, 1);
    }
}
