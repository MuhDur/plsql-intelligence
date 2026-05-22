use plsql_catalog::{CatalogObject, CatalogSnapshot, GrantPrivilege, Grantee, SchemaCatalog};
use plsql_core::{
    Confidence, ConfidenceLevel, Evidence, ObjectName, RoleName, SchemaName, UnknownReason,
};
use tracing::instrument;

use crate::{
    AccessControlEntry, AuthorizationAmbiguity, AuthorizationMode, CrossSchemaWrite, GrantOption,
    PrivilegeConfig, PrivilegeModel, ResolvedPrivilege, SynonymPrivilegePath,
};

/// Resolve a privilege model from a catalog snapshot and configuration.
#[instrument(level = "debug", skip_all)]
pub fn resolve_privileges(snapshot: &CatalogSnapshot, config: &PrivilegeConfig) -> PrivilegeModel {
    let mut model = PrivilegeModel::default();

    for (schema_name, schema_catalog) in &snapshot.schemas {
        resolve_grants_for_schema(schema_name, schema_catalog, config, &mut model);
        resolve_access_control_for_schema(schema_name, schema_catalog, &mut model);
        resolve_cross_schema_writes(schema_catalog, config, &mut model);
        resolve_synonym_paths(schema_name, schema_catalog, &mut model);
    }

    model.public_grants = model
        .privileges
        .iter()
        .filter(|p| matches!(p.grantee, Grantee::Public))
        .cloned()
        .collect();

    model
}

/// Determine the authorization mode for a PL/SQL unit from its catalog metadata.
pub fn authorization_mode_for_object(
    schema_catalog: &SchemaCatalog,
    object_name: &ObjectName,
) -> Option<AuthorizationMode> {
    match schema_catalog.objects.get(object_name)? {
        CatalogObject::Package(pkg) => pkg.authid_current_user.map(|invoker| {
            if invoker {
                AuthorizationMode::Invoker
            } else {
                AuthorizationMode::Definer
            }
        }),
        CatalogObject::Procedure(proc) => proc.signature.authid_current_user.map(|invoker| {
            if invoker {
                AuthorizationMode::Invoker
            } else {
                AuthorizationMode::Definer
            }
        }),
        CatalogObject::Function(func) => func.signature.authid_current_user.map(|invoker| {
            if invoker {
                AuthorizationMode::Invoker
            } else {
                AuthorizationMode::Definer
            }
        }),
        _ => Some(AuthorizationMode::Definer),
    }
}

fn resolve_grants_for_schema(
    schema_name: &SchemaName,
    schema_catalog: &SchemaCatalog,
    config: &PrivilegeConfig,
    model: &mut PrivilegeModel,
) {
    for grant in &schema_catalog.grants {
        let (confidence, via_role) = grant_confidence(&grant.grantee, config);

        model.privileges.push(ResolvedPrivilege {
            object_owner: grant.object_owner,
            object_name: grant.object_name,
            privilege: grant.privilege,
            grantee: grant.grantee.clone(),
            grant_option: if grant.grantable {
                GrantOption::Grantable
            } else if grant.with_hierarchy {
                GrantOption::Hierarchy
            } else {
                GrantOption::None
            },
            via_role,
            confidence,
            evidence: Evidence::new(
                "privilege-grant",
                format!(
                    "Grant {:?} on {:?}.{:?} to {:?}",
                    grant.privilege, schema_name, grant.object_name, grant.grantee
                ),
            ),
        });

        if let Grantee::Role(role) = &grant.grantee {
            if !config.enabled_roles.contains(role) {
                model.runtime_ambiguities.push(AuthorizationAmbiguity {
                    schema: grant.object_owner,
                    object: grant.object_name,
                    reason: UnknownReason::RuntimeGrantOrRole,
                    dependent_roles: vec![*role],
                    evidence: Evidence::new(
                        "runtime-role-ambiguity",
                        format!("Grant to role {:?} may not be enabled at runtime", role),
                    ),
                });
            }
        }
    }
}

fn resolve_access_control_for_schema(
    schema_name: &SchemaName,
    schema_catalog: &SchemaCatalog,
    model: &mut PrivilegeModel,
) {
    for obj in schema_catalog.objects.values() {
        let accessible_by = match obj {
            CatalogObject::Package(pkg) => pkg.accessible_by.clone(),
            CatalogObject::Procedure(proc) => proc.signature.accessible_by.clone(),
            CatalogObject::Function(func) => func.signature.accessible_by.clone(),
            _ => vec![],
        };

        if !accessible_by.is_empty() {
            model.access_control.push(AccessControlEntry {
                declaring_schema: *schema_name,
                declaring_object: object_name_for(obj),
                allowed_callers: accessible_by,
            });
        }
    }
}

fn resolve_cross_schema_writes(
    schema_catalog: &SchemaCatalog,
    config: &PrivilegeConfig,
    model: &mut PrivilegeModel,
) {
    for grant in &schema_catalog.grants {
        if !matches!(
            grant.privilege,
            GrantPrivilege::Insert | GrantPrivilege::Update | GrantPrivilege::Delete
        ) {
            continue;
        }

        if let Grantee::User(user) = &grant.grantee {
            let gs = SchemaName::new(user.symbol());
            if gs != grant.object_owner {
                let (confidence, runtime_ambiguity) = write_ambiguity(&grant.grantee, config);

                model.cross_schema_writes.push(CrossSchemaWrite {
                    caller_schema: gs,
                    caller_object: ObjectName::new(user.symbol()),
                    target_schema: grant.object_owner,
                    target_object: grant.object_name,
                    privilege: grant.privilege,
                    confidence,
                    evidence: Evidence::new(
                        "cross-schema-write",
                        format!(
                            "Write grant {:?} on {:?}.{:?} to {:?}",
                            grant.privilege, grant.object_owner, grant.object_name, user
                        ),
                    ),
                    runtime_ambiguity,
                });
            }
        }
    }
}

fn resolve_synonym_paths(
    schema_name: &SchemaName,
    schema_catalog: &SchemaCatalog,
    model: &mut PrivilegeModel,
) {
    for (syn_name, syn_target) in &schema_catalog.synonyms {
        let target_schema = syn_target.target_owner.unwrap_or(*schema_name);

        model.synonym_paths.push(SynonymPrivilegePath {
            synonym_schema: *schema_name,
            synonym_name: ObjectName::new(syn_name.symbol()),
            target_schema,
            target_object: syn_target.target_name,
            is_public: syn_target.public_synonym,
            confidence: Confidence::new(
                ConfidenceLevel::Medium,
                Some("Synonym target can change at runtime".to_string()),
            ),
        });
    }
}

fn grant_confidence(grantee: &Grantee, config: &PrivilegeConfig) -> (Confidence, Option<RoleName>) {
    match grantee {
        Grantee::User(_) => (Confidence::new(ConfidenceLevel::High, None), None),
        Grantee::Role(role) => {
            if config.enabled_roles.contains(role) {
                (
                    Confidence::new(
                        ConfidenceLevel::High,
                        Some(format!("Role {:?} is enabled in profile", role)),
                    ),
                    Some(*role),
                )
            } else {
                (
                    Confidence::new(
                        ConfidenceLevel::Low,
                        Some(format!("Role {:?} may not be enabled at runtime", role)),
                    ),
                    Some(*role),
                )
            }
        }
        Grantee::Public => (
            Confidence::new(ConfidenceLevel::High, Some("PUBLIC grant".to_string())),
            None,
        ),
    }
}

fn write_ambiguity(
    grantee: &Grantee,
    config: &PrivilegeConfig,
) -> (Confidence, Option<UnknownReason>) {
    match grantee {
        Grantee::Role(role) => {
            if config.enabled_roles.contains(role) {
                (Confidence::new(ConfidenceLevel::High, None), None)
            } else {
                (
                    Confidence::new(
                        ConfidenceLevel::Low,
                        Some(format!("Role {:?} may not be active", role)),
                    ),
                    Some(UnknownReason::RuntimeGrantOrRole),
                )
            }
        }
        _ => (Confidence::new(ConfidenceLevel::High, None), None),
    }
}

fn object_name_for(obj: &CatalogObject) -> ObjectName {
    match obj {
        CatalogObject::Table(t) => t.common.name,
        CatalogObject::View(v) => v.common.name,
        CatalogObject::MaterializedView(m) => m.common.name,
        CatalogObject::Sequence(s) => s.common.name,
        CatalogObject::Type(t) => t.common.name,
        CatalogObject::Package(p) => p.common.name,
        CatalogObject::Procedure(p) => p.common.name,
        CatalogObject::Function(f) => f.common.name,
        CatalogObject::Trigger(t) => t.common.name,
        CatalogObject::SchedulerJob(j) => j.common.name,
        CatalogObject::EditioningView(e) => e.common.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_catalog::{ObjectCommon, ObjectType, PackageMetadata, SchemaCatalog};
    use plsql_core::SymbolId;
    use std::collections::HashMap;

    fn make_schema_name(s: &str) -> SchemaName {
        SchemaName::new(SymbolId::new(s.len() as u64))
    }

    fn make_object_name(s: &str) -> ObjectName {
        ObjectName::new(SymbolId::new(s.len() as u64 + 100))
    }

    #[test]
    fn test_empty_catalog_produces_empty_model() {
        let snapshot = CatalogSnapshot {
            schemas: HashMap::new(),
            ..CatalogSnapshot::default()
        };
        let config = PrivilegeConfig::default();
        let model = resolve_privileges(&snapshot, &config);
        assert!(model.privileges.is_empty());
        assert!(model.public_grants.is_empty());
        assert!(model.access_control.is_empty());
        assert!(model.cross_schema_writes.is_empty());
        assert!(model.synonym_paths.is_empty());
        assert!(model.runtime_ambiguities.is_empty());
    }

    #[test]
    fn test_definer_vs_invoker_authorization() {
        use plsql_catalog::CatalogObject;

        let owner = make_schema_name("OWNER");
        let pkg_name = make_object_name("MY_PKG");

        // Definer-rights package.
        let mut schema = SchemaCatalog::default();
        schema.objects.insert(
            pkg_name,
            CatalogObject::Package(PackageMetadata {
                common: ObjectCommon {
                    owner,
                    name: pkg_name,
                    object_type: ObjectType::Package,
                    ..ObjectCommon::default()
                },
                authid_current_user: Some(false),
                ..PackageMetadata::default()
            }),
        );

        let mode = authorization_mode_for_object(&schema, &pkg_name);
        assert_eq!(mode, Some(AuthorizationMode::Definer));

        // Now make it invoker-rights.
        if let Some(CatalogObject::Package(pkg)) = schema.objects.get_mut(&pkg_name) {
            pkg.authid_current_user = Some(true);
        }
        let mode = authorization_mode_for_object(&schema, &pkg_name);
        assert_eq!(mode, Some(AuthorizationMode::Invoker));
    }

    #[test]
    fn authorization_mode_unknown_authid_nonroutine_and_absent() {
        use plsql_catalog::{CatalogObject, TableMetadata};

        let owner = make_schema_name("OWNER");
        let pkg_name = make_object_name("UNK_PKG");
        let tbl_name = make_object_name("SOME_TBL");
        let absent = make_object_name("NOPE");

        let mut schema = SchemaCatalog::default();

        // Package whose AUTHID could not be determined (None). R13:
        // surface the uncertainty as None — must NOT be silently
        // downgraded to Definer (claiming definer-rights when the
        // object may actually be invoker-rights would mask a
        // privilege-escalation surface).
        schema.objects.insert(
            pkg_name,
            CatalogObject::Package(PackageMetadata {
                common: ObjectCommon {
                    owner,
                    name: pkg_name,
                    object_type: ObjectType::Package,
                    ..ObjectCommon::default()
                },
                authid_current_user: None,
                ..PackageMetadata::default()
            }),
        );
        // Non-routine object: AUTHID is not a concept for tables;
        // the resolver treats them as definer-scoped.
        schema
            .objects
            .insert(tbl_name, CatalogObject::Table(TableMetadata::default()));

        assert_eq!(
            authorization_mode_for_object(&schema, &pkg_name),
            None,
            "unknown AUTHID must surface as None (R13), never silently Definer"
        );
        assert_eq!(
            authorization_mode_for_object(&schema, &tbl_name),
            Some(AuthorizationMode::Definer),
            "non-routine objects resolve to Definer"
        );
        assert_eq!(
            authorization_mode_for_object(&schema, &absent),
            None,
            "object absent from the catalog resolves to None"
        );
    }
}
