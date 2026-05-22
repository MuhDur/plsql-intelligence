//! Privilege-ambiguity feed (PLSQL-PRIV-002).
//!
//! The privilege resolver ([`crate::resolve_privileges`]) records two
//! kinds of "can't decide statically" outcome:
//!
//! * [`AuthorizationAmbiguity`] — a unit's authorization depends on
//!   runtime role state (`RuntimeGrantOrRole`) or invoker-rights
//!   resolution (`InvokerRightsRuntimeResolution`).
//! * [`CrossSchemaWrite`] with a non-`None` `runtime_ambiguity` — a
//!   cross-schema write whose grant can't be confirmed statically.
//!
//! Those signals are useless if they stay trapped in the privilege
//! model. This module turns them into a flat feed two downstream
//! layers consume:
//!
//! 1. **Symbol resolution** — when a reference resolves to an object
//!    whose authorization is ambiguous, the resolver should not claim
//!    `High` confidence. [`downgrade_confidence`] computes the capped
//!    confidence for a given prior + reason.
//! 2. **SAST evidence** — each ambiguity becomes an [`Evidence`]
//!    record (stable `code`, dependent-role notes) a security rule
//!    can attach to its finding so an operator sees *why* the
//!    analyser hedged.
//!
//! The feed is pure data: it neither imports the symbol crate nor the
//! SAST crate (both are same-layer / downstream), so there is no
//! dependency cycle — the engine orchestration layer wires the feed
//! into whichever consumer needs it. Object/schema/role names are
//! interned [`plsql_core`] ids; the feed keeps them typed and renders
//! them via `Debug` for evidence text (the engine resolves ids back
//! to source identifiers through its interner when presenting).
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Invoker's
//!   vs. Definer's Rights: `AUTHID CURRENT_USER` defers privilege
//!   resolution to call time, which is exactly the static-analysis
//!   gap this feed records.
//! * `LOW-LEVEL-CATALOGS.md` — `SESSION_ROLES` / `DBA_ROLE_PRIVS`:
//!   the runtime role set that makes `RuntimeGrantOrRole` undecidable
//!   without a live session.

use serde::{Deserialize, Serialize};

use plsql_core::{
    Confidence, ConfidenceLevel, Evidence, ObjectName, RoleName, SchemaName, UnknownReason,
};

use crate::model::PrivilegeModel;

/// Stable evidence code so SAST rules / golden tests can match on it.
pub const AMBIGUITY_EVIDENCE_CODE: &str = "PRIV-AMBIGUITY";

/// One downstream-consumable ambiguity record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AmbiguityFeedEntry {
    /// Owning schema of the affected object.
    pub schema: SchemaName,
    /// The affected object.
    pub object: ObjectName,
    /// Why the authorization is undecidable statically.
    pub reason: UnknownReason,
    /// Roles whose runtime state would change the outcome (may be
    /// empty when the ambiguity is not role-driven).
    pub dependent_roles: Vec<RoleName>,
    /// The confidence ceiling a symbol-resolution result that lands
    /// on this object must not exceed.
    pub confidence_ceiling: Confidence,
    /// Ready-to-attach SAST evidence record.
    pub sast_evidence: Evidence,
}

/// The strongest confidence a result may claim when its authorization
/// hinges on `reason`. Runtime role/grant state and invoker-rights
/// resolution are genuinely undecidable without a live session, so
/// they cap at `Low`; anything else we treat as `Opaque` (we don't
/// even know enough to call it `Low`).
#[must_use]
pub fn confidence_ceiling_for(reason: UnknownReason) -> ConfidenceLevel {
    match reason {
        UnknownReason::RuntimeGrantOrRole | UnknownReason::InvokerRightsRuntimeResolution => {
            ConfidenceLevel::Low
        }
        _ => ConfidenceLevel::Opaque,
    }
}

/// Cap `prior` at the ceiling implied by `reason`. Never *raises*
/// confidence — if the prior is already at/under the ceiling it is
/// returned unchanged (only the explanation is appended). Ordering
/// uses `ConfidenceLevel`'s derived `Ord` where `High < Medium <
/// Low < Opaque` (a larger discriminant == less confident), so the
/// capped level is `max(prior, ceiling)`.
#[must_use]
pub fn downgrade_confidence(prior: &Confidence, reason: UnknownReason) -> Confidence {
    let ceiling = confidence_ceiling_for(reason);
    let level = prior.level.max(ceiling);
    let note = format!(
        "privilege authorization is ambiguous ({reason:?}); confidence capped at {ceiling:?}"
    );
    let explanation = match &prior.explanation {
        Some(prev) if !prev.is_empty() => format!("{prev}; {note}"),
        _ => note,
    };
    Confidence::new(level, explanation)
}

fn ceiling_confidence(reason: UnknownReason, context: &str) -> Confidence {
    Confidence::new(confidence_ceiling_for(reason), context.to_string())
}

/// Build the flat ambiguity feed from a resolved [`PrivilegeModel`].
///
/// Deterministic: entries appear in the model's own order
/// (`runtime_ambiguities` first, then ambiguous `cross_schema_writes`).
/// `O(n)` over the two source vectors.
#[must_use]
pub fn ambiguity_feed(model: &PrivilegeModel) -> Vec<AmbiguityFeedEntry> {
    let mut feed = Vec::new();

    for amb in &model.runtime_ambiguities {
        let summary = format!(
            "{:?}.{:?} authorization depends on runtime role state ({:?})",
            amb.schema, amb.object, amb.reason
        );
        let mut ev = Evidence::new(AMBIGUITY_EVIDENCE_CODE, summary);
        if !amb.dependent_roles.is_empty() {
            ev = ev.with_note(format!("dependent roles: {:?}", amb.dependent_roles));
        }
        ev.confidence = Some(ceiling_confidence(
            amb.reason,
            "static analysis cannot confirm the grant without a live session",
        ));
        feed.push(AmbiguityFeedEntry {
            schema: amb.schema,
            object: amb.object,
            reason: amb.reason,
            dependent_roles: amb.dependent_roles.clone(),
            confidence_ceiling: ceiling_confidence(
                amb.reason,
                "authorization ambiguity from privilege model",
            ),
            sast_evidence: ev,
        });
    }

    for csw in &model.cross_schema_writes {
        let Some(reason) = csw.runtime_ambiguity else {
            continue;
        };
        let summary = format!(
            "cross-schema write {:?}.{:?} -> {:?}.{:?} ({:?}) cannot be confirmed statically ({:?})",
            csw.caller_schema,
            csw.caller_object,
            csw.target_schema,
            csw.target_object,
            csw.privilege,
            reason
        );
        let mut ev = Evidence::new(AMBIGUITY_EVIDENCE_CODE, summary);
        ev = ev.with_note("cross-schema write authorization is runtime-dependent");
        ev.confidence = Some(ceiling_confidence(
            reason,
            "grant for the cross-schema write is runtime-resolved",
        ));
        feed.push(AmbiguityFeedEntry {
            schema: csw.target_schema,
            object: csw.target_object,
            reason,
            dependent_roles: Vec::new(),
            confidence_ceiling: ceiling_confidence(reason, "cross-schema write ambiguity"),
            sast_evidence: ev,
        });
    }

    feed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthorizationAmbiguity, CrossSchemaWrite};
    use plsql_catalog::GrantPrivilege;
    use plsql_core::SymbolId;

    fn sn(id: u64) -> SchemaName {
        SchemaName::from(SymbolId::new(id))
    }
    fn on(id: u64) -> ObjectName {
        ObjectName::from(SymbolId::new(id))
    }
    fn rn(id: u64) -> RoleName {
        RoleName::from(SymbolId::new(id))
    }

    #[test]
    fn confidence_ceiling_runtime_role_is_low() {
        assert_eq!(
            confidence_ceiling_for(UnknownReason::RuntimeGrantOrRole),
            ConfidenceLevel::Low
        );
        assert_eq!(
            confidence_ceiling_for(UnknownReason::InvokerRightsRuntimeResolution),
            ConfidenceLevel::Low
        );
    }

    #[test]
    fn confidence_ceiling_other_reasons_are_opaque() {
        assert_eq!(
            confidence_ceiling_for(UnknownReason::DynamicSqlOpaque),
            ConfidenceLevel::Opaque
        );
    }

    #[test]
    fn downgrade_never_raises_confidence() {
        let prior = Confidence::new(ConfidenceLevel::Opaque, None);
        let out = downgrade_confidence(&prior, UnknownReason::RuntimeGrantOrRole);
        assert_eq!(out.level, ConfidenceLevel::Opaque);
    }

    #[test]
    fn downgrade_caps_high_to_low_for_runtime_role() {
        let prior = Confidence::new(ConfidenceLevel::High, Some("resolved in catalog".into()));
        let out = downgrade_confidence(&prior, UnknownReason::RuntimeGrantOrRole);
        assert_eq!(out.level, ConfidenceLevel::Low);
        let expl = out.explanation.unwrap();
        assert!(expl.contains("resolved in catalog"));
        assert!(expl.contains("capped at Low"));
    }

    #[test]
    fn empty_model_yields_empty_feed() {
        assert!(ambiguity_feed(&PrivilegeModel::default()).is_empty());
    }

    #[test]
    fn runtime_ambiguity_becomes_feed_entry_with_evidence() {
        let model = PrivilegeModel {
            runtime_ambiguities: vec![AuthorizationAmbiguity {
                schema: sn(1),
                object: on(2),
                reason: UnknownReason::RuntimeGrantOrRole,
                dependent_roles: vec![rn(3)],
                evidence: Evidence::new("X", "x"),
            }],
            ..PrivilegeModel::default()
        };
        let feed = ambiguity_feed(&model);
        assert_eq!(feed.len(), 1);
        let e = &feed[0];
        assert_eq!(e.schema, sn(1));
        assert_eq!(e.reason, UnknownReason::RuntimeGrantOrRole);
        assert_eq!(e.confidence_ceiling.level, ConfidenceLevel::Low);
        assert_eq!(e.sast_evidence.code, AMBIGUITY_EVIDENCE_CODE);
        assert_eq!(e.dependent_roles, vec![rn(3)]);
        assert!(
            e.sast_evidence
                .notes
                .iter()
                .any(|n| n.contains("dependent roles"))
        );
    }

    #[test]
    fn cross_schema_write_without_ambiguity_is_skipped() {
        let model = PrivilegeModel {
            cross_schema_writes: vec![CrossSchemaWrite {
                caller_schema: sn(1),
                caller_object: on(2),
                target_schema: sn(3),
                target_object: on(4),
                privilege: GrantPrivilege::Update,
                confidence: Confidence::new(ConfidenceLevel::High, None),
                evidence: Evidence::new("X", "x"),
                runtime_ambiguity: None,
            }],
            ..PrivilegeModel::default()
        };
        assert!(ambiguity_feed(&model).is_empty());
    }

    #[test]
    fn cross_schema_write_with_ambiguity_targets_written_object() {
        let model = PrivilegeModel {
            cross_schema_writes: vec![CrossSchemaWrite {
                caller_schema: sn(1),
                caller_object: on(2),
                target_schema: sn(3),
                target_object: on(4),
                privilege: GrantPrivilege::Update,
                confidence: Confidence::new(ConfidenceLevel::Low, None),
                evidence: Evidence::new("X", "x"),
                runtime_ambiguity: Some(UnknownReason::RuntimeGrantOrRole),
            }],
            ..PrivilegeModel::default()
        };
        let feed = ambiguity_feed(&model);
        assert_eq!(feed.len(), 1);
        // The feed entry keys on the *written* object so the symbol
        // layer downgrades references that resolve to it.
        assert_eq!(feed[0].schema, sn(3));
        assert_eq!(feed[0].object, on(4));
    }
}
