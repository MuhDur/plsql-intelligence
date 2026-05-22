use serde::{Deserialize, Serialize};

use plsql_catalog::{AccessibleByTarget, GrantPrivilege, Grantee};
use plsql_core::{Confidence, Evidence, ObjectName, RoleName, SchemaName, UnknownReason, UserName};

/// Authorization mode for a PL/SQL compilation unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuthorizationMode {
    /// `AUTHID DEFINER` — runs under the owner's privileges.
    #[default]
    Definer,
    /// `AUTHID CURRENT_USER` — runs under the caller's privileges.
    Invoker,
}

/// Whether a privilege grant can be further delegated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum GrantOption {
    /// `WITH GRANT OPTION` — grantee can re-grant.
    Grantable,
    /// `WITH HIERARCHY OPTION` — grantee can grant to sub-objects.
    Hierarchy,
    #[default]
    /// No delegation rights.
    None,
}

/// Resolved privilege for a specific principal (user/role/public) on a specific object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPrivilege {
    /// The target schema.
    pub object_owner: SchemaName,
    /// The target object.
    pub object_name: ObjectName,
    /// The privilege granted.
    pub privilege: GrantPrivilege,
    /// The grantee (user, role, or PUBLIC).
    pub grantee: Grantee,
    /// Whether this grant is delegatable.
    pub grant_option: GrantOption,
    /// The role through which this grant was inherited, if any.
    pub via_role: Option<RoleName>,
    /// Confidence in this resolution.
    pub confidence: Confidence,
    /// Evidence for this resolution.
    pub evidence: Evidence,
}

/// Whether a unit is accessible by specific callers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccessControlEntry {
    /// The unit that declares the `ACCESSIBLE BY` clause.
    pub declaring_schema: SchemaName,
    pub declaring_object: ObjectName,
    /// The allowed callers.
    pub allowed_callers: Vec<AccessibleByTarget>,
}

/// Cross-schema write — a unit writes to an object in a different schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossSchemaWrite {
    /// The calling unit's schema.
    pub caller_schema: SchemaName,
    pub caller_object: ObjectName,
    /// The target object's schema (different from caller_schema).
    pub target_schema: SchemaName,
    pub target_object: ObjectName,
    /// The privilege required.
    pub privilege: GrantPrivilege,
    /// Confidence in this write being authorized.
    pub confidence: Confidence,
    /// Evidence.
    pub evidence: Evidence,
    /// If resolution is uncertain due to runtime role state.
    pub runtime_ambiguity: Option<UnknownReason>,
}

/// A resolved privilege that was inferred through a synonym chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynonymPrivilegePath {
    /// The original synonym name.
    pub synonym_schema: SchemaName,
    pub synonym_name: ObjectName,
    /// The resolved target.
    pub target_schema: SchemaName,
    pub target_object: ObjectName,
    /// Whether the synonym is public.
    pub is_public: bool,
    /// Confidence (synonym targets can change).
    pub confidence: Confidence,
}

/// Aggregated privilege model for an analysis run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrivilegeModel {
    /// All resolved privileges.
    pub privileges: Vec<ResolvedPrivilege>,
    /// All `ACCESSIBLE BY` entries.
    pub access_control: Vec<AccessControlEntry>,
    /// Cross-schema write surface.
    pub cross_schema_writes: Vec<CrossSchemaWrite>,
    /// Synonym-mediated privilege paths.
    pub synonym_paths: Vec<SynonymPrivilegePath>,
    /// Grants to `PUBLIC`.
    pub public_grants: Vec<ResolvedPrivilege>,
    /// Ambiguous authorizations due to runtime role state.
    pub runtime_ambiguities: Vec<AuthorizationAmbiguity>,
    /// Diagnostics generated during privilege resolution.
    pub diagnostics: Vec<plsql_core::Diagnostic>,
}

/// An authorization that cannot be resolved statically because it depends
/// on runtime role state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationAmbiguity {
    /// The unit that has the ambiguous authorization.
    pub schema: SchemaName,
    pub object: ObjectName,
    /// The reason the authorization is ambiguous.
    pub reason: UnknownReason,
    /// The roles that could change the outcome.
    pub dependent_roles: Vec<RoleName>,
    /// Evidence.
    pub evidence: Evidence,
}

/// Configuration for privilege resolution.
#[derive(Clone, Debug, Default)]
pub struct PrivilegeConfig {
    /// The current schema for resolution context.
    pub current_schema: Option<SchemaName>,
    /// The current user for resolution context.
    pub current_user: Option<UserName>,
    /// Roles to assume as enabled.
    pub enabled_roles: Vec<RoleName>,
}
