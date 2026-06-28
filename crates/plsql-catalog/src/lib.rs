#![forbid(unsafe_code)]
pub mod synthetic;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use asupersync::Cx;
use chrono::{DateTime, Utc};
use plsql_core::{
    AnalysisProfile, ColumnName, EditionName, MemberName, ObjectName, OracleVersion, RoleName,
    SchemaName, SymbolId, SymbolInterner, UserName,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

use plsql_output::SchemaVersion;

macro_rules! catalog_name {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(SymbolId);

        impl $name {
            #[must_use]
            #[instrument(level = "trace")]
            pub fn new(symbol: SymbolId) -> Self {
                Self(symbol)
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn symbol(self) -> SymbolId {
                self.0
            }
        }

        impl From<SymbolId> for $name {
            fn from(value: SymbolId) -> Self {
                Self::new(value)
            }
        }
    };
}

catalog_name!(SynonymName);
catalog_name!(IndexName);
catalog_name!(ConstraintName);
catalog_name!(TriggerName);

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hash(String);

impl Hash {
    #[must_use]
    #[instrument(level = "trace", skip(value))]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DbmsMetadataDdl {
    pub ddl_text: String,
    pub normalized_ddl: Option<String>,
    pub xml_text: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogSourceKind {
    #[default]
    JsonSnapshot,
    LiveConnection,
    DbmsMetadataFiles,
    SyntheticTestCatalog,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogSource {
    pub kind: CatalogSourceKind,
    pub path: Option<PathBuf>,
    pub description: Option<String>,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum ObjectType {
    Table,
    View,
    MaterializedView,
    Sequence,
    Type,
    Package,
    Procedure,
    Function,
    Trigger,
    SchedulerJob,
    EditioningView,
    Synonym,
    Index,
    Constraint,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ObjectStatus {
    Valid,
    Invalid,
    #[default]
    NotApplicable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObjectCommon {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: ObjectType,
    pub status: ObjectStatus,
    pub edition_name: Option<EditionName>,
    pub editionable: Option<bool>,
    pub last_ddl_time: Option<DateTime<Utc>>,
    pub source_hash: Option<Hash>,
    pub ddl: Option<DbmsMetadataDdl>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogCapabilities {
    pub can_query_dba_views: bool,
    pub can_query_all_views: bool,
    pub can_use_dbms_metadata: bool,
    pub can_read_source: bool,
    pub plscope_enabled: bool,
    pub can_query_scheduler: bool,
    pub can_query_roles_and_grants: bool,
    pub warnings: Vec<CapabilityWarning>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityWarning {
    pub code: String,
    pub message: String,
    pub remediation: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub schemas: HashMap<SchemaName, SchemaCatalog>,
    pub profile: AnalysisProfile,
    pub capabilities: CatalogCapabilities,
    pub generated_at: DateTime<Utc>,
    pub source: CatalogSource,
    pub interner: SymbolInterner,
    /// Database-wide edition tree from `ALL_EDITIONS`. Empty when EBR
    /// is not in use.
    #[serde(default)]
    pub editions: Vec<Edition>,
    /// Set of database usernames observed from `ALL_USERS`, used during
    /// live extraction to discriminate an object-privilege grantee
    /// (`ALL_TAB_PRIVS.GRANTEE`) between a real user and a database role —
    /// Oracle's `ALL_TAB_PRIVS` carries no user/role discriminator column.
    ///
    /// `None` means the username set was never loaded (the `ALL_USERS`
    /// probe was not run or failed); in that state grantee classification
    /// is *undetermined* and, honoring R13, the extractor must NOT
    /// silently assume a direct (high-confidence) user grant. `Some(_)`
    /// (even when empty) means the set was loaded and is authoritative for
    /// the schemas under analysis.
    ///
    /// This is transient extraction state: it is never serialized, because
    /// the resulting `Grantee` discrimination is already baked into each
    /// persisted `Grant`. JSON snapshots therefore round-trip unchanged.
    #[serde(default, skip)]
    pub known_users: Option<HashSet<UserName>>,
}

impl CatalogSnapshot {
    #[must_use]
    #[instrument(level = "trace", skip(profile, capabilities, source))]
    pub fn new(
        profile: AnalysisProfile,
        capabilities: CatalogCapabilities,
        source: CatalogSource,
        generated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schemas: HashMap::new(),
            profile,
            capabilities,
            generated_at,
            source,
            interner: SymbolInterner::new(),
            editions: Vec::new(),
            known_users: None,
        }
    }

    /// Intern `text` as a [`UserName`] without changing classification
    /// state. Mirrors [`SymbolInterner::intern_user_name`] but routes
    /// through this snapshot's interner.
    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_user_name(&mut self, text: impl Into<String>) -> Option<UserName> {
        self.interner.intern_user_name(text)
    }

    /// Intern `text` as a [`RoleName`] through this snapshot's interner.
    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_role_name(&mut self, text: impl Into<String>) -> Option<RoleName> {
        self.interner.intern_role_name(text)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_schema_name(&mut self, text: impl Into<String>) -> Option<SchemaName> {
        self.interner.intern_schema_name(text)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_object_name(&mut self, text: impl Into<String>) -> Option<ObjectName> {
        self.interner.intern(text).map(ObjectName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_column_name(&mut self, text: impl Into<String>) -> Option<ColumnName> {
        self.interner.intern(text).map(ColumnName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_member_name(&mut self, text: impl Into<String>) -> Option<MemberName> {
        self.interner.intern(text).map(MemberName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_synonym_name(&mut self, text: impl Into<String>) -> Option<SynonymName> {
        self.interner.intern(text).map(SynonymName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_index_name(&mut self, text: impl Into<String>) -> Option<IndexName> {
        self.interner.intern(text).map(IndexName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_constraint_name(&mut self, text: impl Into<String>) -> Option<ConstraintName> {
        self.interner.intern(text).map(ConstraintName::from)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self, text))]
    pub fn intern_trigger_name(&mut self, text: impl Into<String>) -> Option<TriggerName> {
        self.interner.intern(text).map(TriggerName::from)
    }
}

pub const CATALOG_SNAPSHOT_SCHEMA_ID: &str = "plsql.catalog.snapshot";
pub const CATALOG_SNAPSHOT_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 1, 0);

pub const CATALOG_DOCTOR_SCHEMA_ID: &str = "plsql.catalog.doctor";
pub const CATALOG_DOCTOR_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);

/// Per-`ObjectType` count tile shown in the doctor report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorObjectCount {
    pub object_type: ObjectType,
    pub total: usize,
    pub valid: usize,
    pub invalid: usize,
    pub other: usize,
}

/// Summary of how many catalog rows landed per family and how many
/// schema-scoped buckets are populated.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorExtractionTotals {
    pub schemas_observed: usize,
    pub objects_total: usize,
    pub columns_total: usize,
    pub indexes_total: usize,
    pub constraints_total: usize,
    pub triggers_total: usize,
    pub synonyms_total: usize,
    pub grants_total: usize,
    pub dependencies_total: usize,
}

/// Doctor-flagged missing privilege: the `plsql-catalog` driver could not
/// observe an Oracle dictionary view that some upstream features require.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MissingPermissionReport {
    pub view_name: String,
    pub required_for: Vec<String>,
    pub suggested_grant: String,
}

/// Structured doctor report for a `CatalogSnapshot`.
///
/// Consumers (`plsql catalog doctor --robot-json`, `plsql-mcp` foundation
/// tools, and the planned `plsql doctor` umbrella surface) can render the
/// report directly or wrap it in a `RobotJsonEnvelope` for stable,
/// versioned output.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogDoctorReport {
    /// Identifier of the snapshot's origin (`live extraction via ...` or the
    /// JSON snapshot path).
    pub source_description: String,
    pub source_kind: CatalogSourceKind,
    pub generated_at: Option<DateTime<Utc>>,
    pub totals: DoctorExtractionTotals,
    pub object_counts: Vec<DoctorObjectCount>,
    pub capability_warnings: Vec<CapabilityWarning>,
    pub missing_permissions: Vec<MissingPermissionReport>,
    /// Per-schema PL/Scope availability. Empty when the snapshot has no
    /// PL/Scope detection wired.
    pub plscope_availability_per_schema: Vec<PlScopeAvailabilityRow>,
    /// Capability-bit copy for downstream consumers that don't want to read
    /// the full `CatalogSnapshot` to learn whether a query family worked.
    pub can_query_dba_views: bool,
    pub can_query_all_views: bool,
    pub can_use_dbms_metadata: bool,
    pub can_read_source: bool,
    pub plscope_enabled: bool,
    pub can_query_scheduler: bool,
    pub can_query_roles_and_grants: bool,
}

/// One row of the doctor report's per-schema PL/Scope availability summary.
/// The `schema_name` is rendered through the snapshot's `SymbolInterner` so
/// the report is stable across JSON snapshots and live extractions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlScopeAvailabilityRow {
    pub schema_name: String,
    pub availability: PlScopeAvailability,
}

impl CatalogSnapshot {
    /// Build the doctor report directly from this snapshot.
    ///
    /// The doctor is read-only — it never queries the DB itself; it
    /// summarizes what was already extracted into the snapshot plus any
    /// `CapabilityWarning`s the loader recorded. Missing-permission diagnoses
    /// are inferred from per-family capability bits so the report is
    /// equally useful for live-extracted and JSON-loaded snapshots.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn doctor_report(&self) -> CatalogDoctorReport {
        let mut counts: BTreeMap<ObjectType, DoctorObjectCount> = BTreeMap::new();
        let mut columns_total = 0usize;
        let mut indexes_total = 0usize;
        let mut constraints_total = 0usize;
        let mut triggers_total = 0usize;
        let mut synonyms_total = 0usize;
        let mut grants_total = 0usize;
        let mut dependencies_total = 0usize;
        let mut objects_total = 0usize;

        for schema_catalog in self.schemas.values() {
            for object in schema_catalog.objects.values() {
                let common = catalog_object_common(object);
                let tile = counts
                    .entry(common.object_type)
                    .or_insert(DoctorObjectCount {
                        object_type: common.object_type,
                        ..DoctorObjectCount::default()
                    });
                tile.total = tile.total.saturating_add(1);
                match common.status {
                    ObjectStatus::Valid => {
                        tile.valid = tile.valid.saturating_add(1);
                    }
                    ObjectStatus::Invalid => {
                        tile.invalid = tile.invalid.saturating_add(1);
                    }
                    ObjectStatus::NotApplicable => {
                        tile.other = tile.other.saturating_add(1);
                    }
                }
                objects_total = objects_total.saturating_add(1);

                columns_total = columns_total.saturating_add(catalog_object_column_count(object));
            }
            indexes_total = indexes_total.saturating_add(schema_catalog.indexes.len());
            constraints_total = constraints_total.saturating_add(schema_catalog.constraints.len());
            triggers_total = triggers_total.saturating_add(schema_catalog.triggers.len());
            synonyms_total = synonyms_total.saturating_add(schema_catalog.synonyms.len());
            grants_total = grants_total.saturating_add(schema_catalog.grants.len());
            dependencies_total =
                dependencies_total.saturating_add(schema_catalog.dependencies.len());
        }

        let totals = DoctorExtractionTotals {
            schemas_observed: self.schemas.len(),
            objects_total,
            columns_total,
            indexes_total,
            constraints_total,
            triggers_total,
            synonyms_total,
            grants_total,
            dependencies_total,
        };

        let mut object_counts: Vec<DoctorObjectCount> = counts.into_values().collect();
        object_counts.sort_by_key(|tile| std::cmp::Reverse(tile.total));

        let missing_permissions =
            derive_missing_permission_reports(&self.capabilities, &self.source);

        let mut plscope_availability_per_schema: Vec<PlScopeAvailabilityRow> = self
            .schemas
            .iter()
            .filter_map(|(owner, schema_catalog)| {
                let availability = schema_catalog.plscope.as_ref()?.availability;
                let schema_name = self.interner.resolve(owner.symbol())?.to_string();
                Some(PlScopeAvailabilityRow {
                    schema_name,
                    availability,
                })
            })
            .collect();
        plscope_availability_per_schema.sort_by(|a, b| a.schema_name.cmp(&b.schema_name));

        CatalogDoctorReport {
            source_description: self.source.description.clone().unwrap_or_default(),
            source_kind: self.source.kind,
            generated_at: Some(self.generated_at),
            totals,
            object_counts,
            capability_warnings: self.capabilities.warnings.clone(),
            missing_permissions,
            plscope_availability_per_schema,
            can_query_dba_views: self.capabilities.can_query_dba_views,
            can_query_all_views: self.capabilities.can_query_all_views,
            can_use_dbms_metadata: self.capabilities.can_use_dbms_metadata,
            can_read_source: self.capabilities.can_read_source,
            plscope_enabled: self.capabilities.plscope_enabled,
            can_query_scheduler: self.capabilities.can_query_scheduler,
            can_query_roles_and_grants: self.capabilities.can_query_roles_and_grants,
        }
    }
}

fn catalog_object_common(object: &CatalogObject) -> &ObjectCommon {
    match object {
        CatalogObject::Table(metadata) => &metadata.common,
        CatalogObject::View(metadata) => &metadata.common,
        CatalogObject::MaterializedView(metadata) => &metadata.common,
        CatalogObject::Sequence(metadata) => &metadata.common,
        CatalogObject::Type(metadata) => &metadata.common,
        CatalogObject::Package(metadata) => &metadata.common,
        CatalogObject::Procedure(metadata) => &metadata.common,
        CatalogObject::Function(metadata) => &metadata.common,
        CatalogObject::Trigger(metadata) => &metadata.common,
        CatalogObject::SchedulerJob(metadata) => &metadata.common,
        CatalogObject::EditioningView(metadata) => &metadata.common,
    }
}

fn catalog_object_column_count(object: &CatalogObject) -> usize {
    match object {
        CatalogObject::Table(metadata) => metadata.columns.len(),
        CatalogObject::View(metadata) => metadata.columns.len(),
        CatalogObject::MaterializedView(metadata) => metadata.columns.len(),
        CatalogObject::EditioningView(metadata) => metadata.columns.len(),
        CatalogObject::Sequence(_)
        | CatalogObject::Type(_)
        | CatalogObject::Package(_)
        | CatalogObject::Procedure(_)
        | CatalogObject::Function(_)
        | CatalogObject::Trigger(_)
        | CatalogObject::SchedulerJob(_) => 0,
    }
}

fn derive_missing_permission_reports(
    capabilities: &CatalogCapabilities,
    source: &CatalogSource,
) -> Vec<MissingPermissionReport> {
    // Missing-permission diagnoses only make sense for live extractions. A
    // JSON snapshot was already produced once — its capability bits reflect
    // the original extraction; we surface them verbatim instead of inventing
    // new grant suggestions.
    if !matches!(source.kind, CatalogSourceKind::LiveConnection) {
        return Vec::new();
    }

    let mut reports = Vec::new();
    if !capabilities.can_query_dba_views {
        reports.push(MissingPermissionReport {
            view_name: String::from("DBA_OBJECTS / DBA_TAB_COLUMNS / DBA_DEPENDENCIES"),
            required_for: vec![
                String::from("cross-schema extraction beyond ALL_*"),
                String::from("PLSQL-CAT-014 dependency reachability over schemas"),
            ],
            suggested_grant: String::from(
                "grant select_catalog_role to <user>; -- or individual grants on DBA_* views",
            ),
        });
    }
    if !capabilities.can_use_dbms_metadata {
        reports.push(MissingPermissionReport {
            view_name: String::from("DBMS_METADATA"),
            required_for: vec![
                String::from("PLSQL-CAT-015 DBMS_METADATA.GET_DDL extraction"),
                String::from("normalized DDL hashes for `what-breaks`"),
            ],
            suggested_grant: String::from("grant execute on DBMS_METADATA to <user>;"),
        });
    }
    if !capabilities.can_read_source {
        reports.push(MissingPermissionReport {
            view_name: String::from("ALL_SOURCE / DBA_SOURCE"),
            required_for: vec![
                String::from("packaged routine body inspection"),
                String::from("get_object_source MCP tool"),
            ],
            suggested_grant: String::from(
                "grant select on ALL_SOURCE to <user>; -- ALL_SOURCE itself is normally readable; ensure no DROP/REVOKE narrowed it",
            ),
        });
    }
    if !capabilities.plscope_enabled {
        reports.push(MissingPermissionReport {
            view_name: String::from("PLSCOPE_SETTINGS / ALL_IDENTIFIERS"),
            required_for: vec![
                String::from("PLSQL-CAT-010 PL/Scope availability detection"),
                String::from("PLSQL-CAT-011 identifier extraction"),
            ],
            suggested_grant: String::from(
                "alter session set plscope_settings = 'identifiers:all'; -- and recompile target objects",
            ),
        });
    }
    if !capabilities.can_query_scheduler {
        reports.push(MissingPermissionReport {
            view_name: String::from("ALL_SCHEDULER_JOBS / ALL_SCHEDULER_PROGRAMS"),
            required_for: vec![String::from("scheduler job lineage edges")],
            suggested_grant: String::from(
                "grant select on ALL_SCHEDULER_JOBS to <user>; grant select on ALL_SCHEDULER_PROGRAMS to <user>;",
            ),
        });
    }
    if !capabilities.can_query_roles_and_grants {
        reports.push(MissingPermissionReport {
            view_name: String::from("DBA_ROLE_PRIVS / DBA_SYS_PRIVS / DBA_TAB_PRIVS"),
            required_for: vec![
                String::from("definer-rights privilege chain analysis"),
                String::from("role-mediated execution evidence (PRIVILEGES-* beads)"),
            ],
            suggested_grant: String::from(
                "grant select_catalog_role to <user>; -- enables DBA_*_PRIVS reads",
            ),
        });
    }
    reports
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogSnapshotDocument {
    pub schema_id: String,
    pub schema_version: SchemaVersion,
    pub snapshot: CatalogSnapshot,
}

impl CatalogSnapshotDocument {
    #[must_use]
    #[instrument(level = "trace", skip(snapshot))]
    pub fn new(snapshot: CatalogSnapshot) -> Self {
        Self {
            schema_id: String::from(CATALOG_SNAPSHOT_SCHEMA_ID),
            schema_version: CATALOG_SNAPSHOT_SCHEMA_VERSION,
            snapshot,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogLoadRequest {
    pub schema_filters: Vec<CatalogSchemaFilter>,
}

impl CatalogLoadRequest {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn for_current_schema() -> Self {
        Self {
            schema_filters: vec![CatalogSchemaFilter::CurrentSchema],
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(schema_names))]
    pub fn for_named_schemas<I, S>(schema_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            schema_filters: schema_names
                .into_iter()
                .map(CatalogSchemaFilter::named)
                .collect(),
        }
    }
}

impl Default for CatalogLoadRequest {
    fn default() -> Self {
        Self::for_current_schema()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogSchemaFilter {
    CurrentSchema,
    Named(String),
}

impl CatalogSchemaFilter {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn current_schema() -> Self {
        Self::CurrentSchema
    }

    #[must_use]
    #[instrument(level = "trace", skip(schema_name))]
    pub fn named(schema_name: impl Into<String>) -> Self {
        Self::Named(schema_name.into())
    }
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("oracle backend `{backend}` is unavailable in this build; use `{feature}`")]
    OracleBackendNotCompiled {
        backend: OracleBackend,
        feature: &'static str,
    },
    #[error("oracle backend `{backend}` error: {message}")]
    OracleBackendError {
        backend: OracleBackend,
        message: String,
    },
    #[error("expected {expected} row(s) but received {actual}")]
    UnexpectedRowCount { expected: String, actual: usize },
    #[error("required column `{column}` was missing from the query result")]
    MissingColumn { column: String },
    #[error("column `{column}` was null")]
    NullColumnValue { column: String },
    #[error("column `{column}` could not be parsed as {expected}: `{value}`")]
    InvalidColumnValue {
        column: String,
        expected: &'static str,
        value: String,
    },
    #[error("unsupported catalog snapshot schema {found} for {schema_id}; expected {expected}")]
    UnsupportedSchemaVersion {
        schema_id: String,
        found: SchemaVersion,
        expected: SchemaVersion,
    },
    #[error("unexpected catalog snapshot schema id `{0}`")]
    UnexpectedSchemaId(String),
    #[error("catalog load request could not resolve the current schema from the Oracle connection")]
    CurrentSchemaUnavailable,
    #[error("schema filter `{schema_name}` is invalid: schema names must not be blank")]
    InvalidSchemaFilter { schema_name: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OracleBackend {
    RustOracle,
    OracleRs,
}

impl OracleBackend {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustOracle => "oracle",
            Self::OracleRs => "oracle-rs",
        }
    }
}

impl std::fmt::Display for OracleBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleConnectOptions {
    pub username: String,
    pub password: String,
    pub connect_string: String,
    pub current_schema: Option<String>,
    pub module: Option<String>,
    pub action: Option<String>,
    pub client_info: Option<String>,
    pub client_identifier: Option<String>,
}

impl OracleConnectOptions {
    #[must_use]
    pub fn new(
        username: impl Into<String>,
        password: impl Into<String>,
        connect_string: impl Into<String>,
    ) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
            connect_string: connect_string.into(),
            current_schema: None,
            module: None,
            action: None,
            client_info: None,
            client_identifier: None,
        }
    }

    #[must_use]
    pub fn with_current_schema(mut self, current_schema: impl Into<String>) -> Self {
        self.current_schema = Some(current_schema.into());
        self
    }

    #[must_use]
    pub fn with_module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }

    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    #[must_use]
    pub fn with_client_info(mut self, client_info: impl Into<String>) -> Self {
        self.client_info = Some(client_info.into());
        self
    }

    #[must_use]
    pub fn with_client_identifier(mut self, client_identifier: impl Into<String>) -> Self {
        self.client_identifier = Some(client_identifier.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OracleBind {
    String(String),
    I64(i64),
    U64(u64),
    Bool(bool),
}

impl From<&str> for OracleBind {
    fn from(value: &str) -> Self {
        Self::String(String::from(value))
    }
}

impl From<String> for OracleBind {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i32> for OracleBind {
    fn from(value: i32) -> Self {
        Self::I64(i64::from(value))
    }
}

impl From<i64> for OracleBind {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl From<u32> for OracleBind {
    fn from(value: u32) -> Self {
        Self::U64(u64::from(value))
    }
}

impl From<u64> for OracleBind {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<bool> for OracleBind {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleCell {
    pub oracle_type: String,
    pub value: Option<String>,
}

impl OracleCell {
    #[must_use]
    #[instrument(level = "trace", skip(oracle_type, value))]
    pub fn new(oracle_type: impl Into<String>, value: Option<String>) -> Self {
        Self {
            oracle_type: oracle_type.into(),
            value,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleRow {
    pub columns: BTreeMap<String, OracleCell>,
}

impl OracleRow {
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        oracle_type: impl Into<String>,
        value: Option<String>,
    ) {
        self.columns.insert(
            name.into().to_ascii_uppercase(),
            OracleCell::new(oracle_type, value),
        );
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn cell(&self, name: &str) -> Option<&OracleCell> {
        self.columns.get(&name.to_ascii_uppercase())
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn text(&self, name: &str) -> Option<&str> {
        self.cell(name).and_then(|cell| cell.value.as_deref())
    }

    #[instrument(level = "trace", skip(self))]
    pub fn require_text(&self, name: &str) -> Result<&str, CatalogError> {
        let Some(cell) = self.cell(name) else {
            return Err(CatalogError::MissingColumn {
                column: name.to_ascii_uppercase(),
            });
        };
        cell.value
            .as_deref()
            .ok_or_else(|| CatalogError::NullColumnValue {
                column: name.to_ascii_uppercase(),
            })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn parse_i64(&self, name: &str) -> Result<i64, CatalogError> {
        let text = self.require_text(name)?;
        text.parse::<i64>()
            .map_err(|_| CatalogError::InvalidColumnValue {
                column: name.to_ascii_uppercase(),
                expected: "i64",
                value: String::from(text),
            })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn parse_u64(&self, name: &str) -> Result<u64, CatalogError> {
        let text = self.require_text(name)?;
        text.parse::<u64>()
            .map_err(|_| CatalogError::InvalidColumnValue {
                column: name.to_ascii_uppercase(),
                expected: "u64",
                value: String::from(text),
            })
    }

    #[instrument(level = "trace", skip(self))]
    pub fn parse_bool(&self, name: &str) -> Result<bool, CatalogError> {
        let text = self.require_text(name)?;
        let normalized = text.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "Y" | "YES" | "TRUE" | "1" => Ok(true),
            "N" | "NO" | "FALSE" | "0" => Ok(false),
            _ => Err(CatalogError::InvalidColumnValue {
                column: name.to_ascii_uppercase(),
                expected: "bool",
                value: String::from(text),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleConnectionInfo {
    pub backend: OracleBackend,
    pub connect_string: String,
    pub current_schema: Option<String>,
    pub server_version: String,
    pub db_name: String,
    pub db_domain: String,
    pub service_name: String,
    pub instance_name: String,
    pub server_type: String,
    pub max_identifier_length: u32,
    pub max_open_cursors: u32,
}

#[async_trait::async_trait(?Send)]
pub trait OracleConnection: Send + Sync {
    fn backend(&self) -> OracleBackend;
    async fn ping(&self, cx: &Cx) -> Result<(), CatalogError>;
    async fn describe(&self, cx: &Cx) -> Result<OracleConnectionInfo, CatalogError>;
    async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<Vec<OracleRow>, CatalogError>;
    async fn execute(&self, cx: &Cx, sql: &str, params: &[OracleBind])
    -> Result<u64, CatalogError>;

    #[instrument(level = "trace", skip(self, sql, params))]
    async fn query_optional_row(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<Option<OracleRow>, CatalogError> {
        let mut rows = self.query_rows(cx, sql, params).await?;
        match rows.len() {
            0 => Ok(None),
            1 => Ok(rows.pop()),
            actual => Err(CatalogError::UnexpectedRowCount {
                expected: String::from("0 or 1"),
                actual,
            }),
        }
    }

    #[instrument(level = "trace", skip(self, sql, params))]
    async fn query_one_row(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<OracleRow, CatalogError> {
        let mut rows = self.query_rows(cx, sql, params).await?;
        match rows.len() {
            1 => rows.pop().ok_or(CatalogError::UnexpectedRowCount {
                expected: String::from("exactly 1"),
                actual: 0,
            }),
            actual => Err(CatalogError::UnexpectedRowCount {
                expected: String::from("exactly 1"),
                actual,
            }),
        }
    }
}

/// Pure-Rust thin Oracle adapter over the shared `oraclemcp-db` connection
/// layer.
///
/// This adapter is optional and exists for live-XE tests and lower-layer
/// callers that need a concrete implementation of [`OracleConnection`] without
/// depending on the MCP crate. The default catalog crate remains offline-first.
#[cfg(feature = "oraclemcp-db")]
pub struct OraclemcpDbConnection {
    inner: oraclemcp_db::RustOracleConnection,
    connect_string: String,
}

#[cfg(feature = "oraclemcp-db")]
impl OraclemcpDbConnection {
    /// Open a pure-Rust thin Oracle connection.
    pub async fn connect(
        cx: &Cx,
        options: oraclemcp_db::OracleConnectOptions,
    ) -> Result<Self, CatalogError> {
        let connect_string = options.connect_string.clone();
        let inner = oraclemcp_db::RustOracleConnection::connect(cx, options)
            .await
            .map_err(map_oraclemcp_db_error)?;
        Ok(Self {
            inner,
            connect_string,
        })
    }

    /// Open a password-authenticated pure-Rust thin Oracle connection with a
    /// module/action identity.
    pub async fn connect_with_password(
        cx: &Cx,
        username: impl Into<String>,
        password: impl Into<String>,
        connect_string: impl Into<String>,
        module: impl Into<String>,
        action: impl Into<String>,
    ) -> Result<Self, CatalogError> {
        let options = oraclemcp_db::OracleConnectOptions {
            connect_string: connect_string.into(),
            username: Some(username.into()),
            password: Some(password.into()),
            session_identity: Some(oraclemcp_db::OracleSessionIdentity {
                module: Some(module.into()),
                action: Some(action.into()),
                ..oraclemcp_db::OracleSessionIdentity::default()
            }),
            ..oraclemcp_db::OracleConnectOptions::default()
        };
        Self::connect(cx, options).await
    }

    /// Borrow the underlying shared driver connection.
    #[must_use]
    pub fn inner(&self) -> &oraclemcp_db::RustOracleConnection {
        &self.inner
    }
}

#[cfg(feature = "oraclemcp-db")]
#[async_trait::async_trait(?Send)]
impl OracleConnection for OraclemcpDbConnection {
    #[instrument(level = "trace", skip(self))]
    fn backend(&self) -> OracleBackend {
        OracleBackend::OracleRs
    }

    #[instrument(level = "trace", skip(self))]
    async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
        oraclemcp_db::OracleConnection::ping(&self.inner, cx)
            .await
            .map_err(map_oraclemcp_db_error)
    }

    #[instrument(level = "trace", skip(self))]
    async fn describe(&self, cx: &Cx) -> Result<OracleConnectionInfo, CatalogError> {
        oraclemcp_db::OracleConnection::describe(&self.inner, cx)
            .await
            .map(|info| map_oraclemcp_connection_info(info, &self.connect_string))
            .map_err(map_oraclemcp_db_error)
    }

    #[instrument(level = "trace", skip(self, sql, params))]
    async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<Vec<OracleRow>, CatalogError> {
        let binds = map_oraclemcp_binds(params)?;
        oraclemcp_db::OracleConnection::query_rows(&self.inner, cx, sql, &binds)
            .await
            .map(map_oraclemcp_rows)
            .map_err(map_oraclemcp_db_error)
    }

    #[instrument(level = "trace", skip(self, sql, params))]
    async fn execute(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[OracleBind],
    ) -> Result<u64, CatalogError> {
        let binds = map_oraclemcp_binds(params)?;
        oraclemcp_db::OracleConnection::execute(&self.inner, cx, sql, &binds)
            .await
            .map_err(map_oraclemcp_db_error)
    }
}

#[cfg(feature = "oraclemcp-db")]
fn map_oraclemcp_connection_info(
    info: oraclemcp_db::OracleConnectionInfo,
    connect_string: &str,
) -> OracleConnectionInfo {
    OracleConnectionInfo {
        backend: OracleBackend::OracleRs,
        connect_string: connect_string.to_owned(),
        current_schema: info.current_schema,
        server_version: info.server_version.unwrap_or_default(),
        db_name: String::new(),
        db_domain: String::new(),
        service_name: String::new(),
        instance_name: String::new(),
        server_type: info.database_role.unwrap_or_default(),
        max_identifier_length: 128,
        max_open_cursors: 0,
    }
}

#[cfg(feature = "oraclemcp-db")]
fn map_oraclemcp_binds(
    params: &[OracleBind],
) -> Result<Vec<oraclemcp_db::OracleBind>, CatalogError> {
    params
        .iter()
        .map(|param| match param {
            OracleBind::String(value) => Ok(oraclemcp_db::OracleBind::String(value.clone())),
            OracleBind::I64(value) => Ok(oraclemcp_db::OracleBind::I64(*value)),
            OracleBind::U64(value) => {
                let signed =
                    i64::try_from(*value).map_err(|_| CatalogError::InvalidColumnValue {
                        column: String::from("bind"),
                        expected: "u64 <= i64::MAX for oraclemcp-db positional bind",
                        value: value.to_string(),
                    })?;
                Ok(oraclemcp_db::OracleBind::I64(signed))
            }
            OracleBind::Bool(value) => Ok(oraclemcp_db::OracleBind::Bool(*value)),
        })
        .collect()
}

#[cfg(feature = "oraclemcp-db")]
fn map_oraclemcp_rows(rows: Vec<oraclemcp_db::OracleRow>) -> Vec<OracleRow> {
    rows.into_iter().map(map_oraclemcp_row).collect()
}

#[cfg(feature = "oraclemcp-db")]
fn map_oraclemcp_row(row: oraclemcp_db::OracleRow) -> OracleRow {
    let mut mapped = OracleRow::default();
    for (name, cell) in row.columns {
        mapped.columns.insert(
            name.to_ascii_uppercase(),
            OracleCell::new(cell.oracle_type, cell.value),
        );
    }
    mapped
}

#[cfg(feature = "oraclemcp-db")]
fn map_oraclemcp_db_error(err: oraclemcp_db::DbError) -> CatalogError {
    CatalogError::OracleBackendError {
        backend: OracleBackend::OracleRs,
        message: err.to_string(),
    }
}

#[instrument(level = "trace")]
pub fn load_snapshot_from_json(path: &std::path::Path) -> Result<CatalogSnapshot, CatalogError> {
    let raw = fs::read_to_string(path)?;
    let document: CatalogSnapshotDocument = serde_json::from_str(&raw)?;

    if !document.schema_id.as_str().eq(CATALOG_SNAPSHOT_SCHEMA_ID) {
        return Err(CatalogError::UnexpectedSchemaId(document.schema_id));
    }

    if !matches!(
        document
            .schema_version
            .cmp(&CATALOG_SNAPSHOT_SCHEMA_VERSION),
        std::cmp::Ordering::Equal
    ) {
        return Err(CatalogError::UnsupportedSchemaVersion {
            schema_id: String::from(CATALOG_SNAPSHOT_SCHEMA_ID),
            found: document.schema_version,
            expected: CATALOG_SNAPSHOT_SCHEMA_VERSION,
        });
    }

    Ok(document.snapshot)
}

#[instrument(level = "trace", skip(snapshot))]
pub fn export_snapshot_to_json(
    snapshot: &CatalogSnapshot,
    path: &std::path::Path,
) -> Result<(), CatalogError> {
    let document = CatalogSnapshotDocument::new(snapshot.clone());
    let rendered = serde_json::to_string_pretty(&document)?;
    fs::write(path, rendered)?;
    Ok(())
}

/// Load a catalog snapshot from a directory of DBMS_METADATA-exported .sql files.
///
/// Load a `CatalogSnapshot` by classifying every `.sql` file under `dir` as a
/// single top-level CREATE DDL statement (the shape `DBMS_METADATA.GET_DDL`
/// emits when written per-object to disk).
///
/// For each file:
///
/// * The object kind is read from the leading `CREATE …` keyword
///   (`TABLE` / `VIEW` / `PACKAGE` / `PROCEDURE` / `FUNCTION` /
///   `SEQUENCE` / `TRIGGER` / `TYPE`); statements whose keyword does
///   not match a known kind are skipped (graceful degradation per
///   R13).
/// * The owner schema is read from the optional `OWNER.OBJECT` prefix
///   on the CREATE target. Unqualified statements (no `OWNER.`
///   prefix) are filed under a stable `PUBLIC` schema interned through
///   the regular interner — never `SymbolId::new(0)`, which would
///   collide with whatever the first object name happens to be.
/// * The raw file bytes are stored verbatim on
///   [`ObjectCommon::ddl`] as a [`DbmsMetadataDdl`] so downstream
///   consumers (doc generation, lineage, the doctor's
///   ddl-extraction ratio) can inspect the exact source the catalog
///   was derived from.
///
/// This classifier is keyword-shaped and does not parse arbitrary
/// PL/SQL bodies — column definitions, parameter signatures, view
/// projections and constraint details are *not* populated. When the
/// full parser (Layer 1) lands, callers that need column- or
/// signature-level fidelity should switch to that path; the
/// `DbmsMetadataDdl` stored here is sufficient seed for re-parsing
/// on demand without re-reading the disk.
#[instrument(level = "info", skip_all, fields(dir = %dir.display()))]
pub fn load_from_dbms_metadata_dir(dir: &std::path::Path) -> Result<CatalogSnapshot, CatalogError> {
    if !dir.is_dir() {
        return Err(CatalogError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("not a directory: {}", dir.display()),
        )));
    }

    let mut interner = SymbolInterner::default();
    let mut schemas: HashMap<SchemaName, SchemaCatalog> = HashMap::new();
    let mut file_count = 0usize;
    let mut classified_count = 0usize;

    // Collect + sort entries so the resulting snapshot (and its
    // interner symbol ids) are deterministic across runs and
    // platforms — `read_dir` ordering is unspecified.
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq("sql"))
        })
        .collect();
    paths.sort();

    for path in paths {
        file_count += 1;
        let ddl_text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };

        if let Some((schema, obj_name, obj)) = classify_dbms_metadata_ddl(&ddl_text, &mut interner)
        {
            let schema_catalog = schemas.entry(schema).or_default();
            schema_catalog.objects.insert(obj_name, obj);
            classified_count += 1;
        }
    }

    tracing::info!(
        files = file_count,
        classified = classified_count,
        "loaded DBMS_METADATA directory"
    );

    Ok(CatalogSnapshot {
        schemas,
        profile: AnalysisProfile::default(),
        capabilities: CatalogCapabilities {
            can_query_all_views: false,
            can_query_dba_views: false,
            can_use_dbms_metadata: true,
            can_read_source: true,
            plscope_enabled: false,
            can_query_scheduler: false,
            can_query_roles_and_grants: false,
            warnings: vec![],
        },
        generated_at: Utc::now(),
        source: CatalogSource {
            kind: CatalogSourceKind::DbmsMetadataFiles,
            description: Some(format!("loaded from {}", dir.display())),
            ..CatalogSource::default()
        },
        interner,
        editions: Vec::new(),
        // DBMS_METADATA directory loads do not query ALL_USERS; grantee
        // classification is not exercised on this path.
        known_users: None,
    })
}

/// Default schema name used when a CREATE statement has no `OWNER.`
/// prefix. Interned through the regular interner so the resulting
/// `SchemaName` has a real, resolvable text — never a collision with
/// `SymbolId::new(0)`.
const UNQUALIFIED_DDL_SCHEMA: &str = "PUBLIC";

/// Classify a single per-file DDL statement into a `CatalogObject`.
///
/// Returns `None` for whitespace-only / comment-only files and for
/// CREATE statements whose object kind keyword is not in the
/// known set. The DDL bytes are preserved verbatim on
/// [`ObjectCommon::ddl`] so downstream code can re-parse them as
/// fidelity improves.
fn classify_dbms_metadata_ddl(
    ddl_text: &str,
    interner: &mut SymbolInterner,
) -> Option<(SchemaName, ObjectName, CatalogObject)> {
    // Parse the DDL HEADER as a real token stream — never substring-match
    // the whole DDL. Body / comment text that mentions `TABLE` etc. used
    // to silently re-classify VIEWs and PROCEDUREs as tables.
    let header = parse_create_header(ddl_text)?;

    // `PACKAGE BODY` / `TYPE BODY` are bodies — the spec's catalog row
    // is the source of truth. Honest uncertainty: return None.
    if matches!(
        header.kind,
        DdlKind::PackageBody | DdlKind::TypeBody | DdlKind::Unknown
    ) {
        return None;
    }

    let (owner_text, object_text) = extract_owner_and_name(&header.after_kind)?;

    let owner_text = owner_text.unwrap_or_else(|| UNQUALIFIED_DDL_SCHEMA.to_string());
    let owner = interner.intern_schema_name(owner_text)?;
    let name_sid = interner.intern(&object_text)?;
    let obj_name = ObjectName::new(name_sid);

    let ddl = DbmsMetadataDdl {
        ddl_text: ddl_text.to_string(),
        normalized_ddl: Some(normalize_dbms_metadata_ddl(ddl_text)),
        xml_text: None,
    };

    let common = ObjectCommon {
        owner,
        name: obj_name,
        object_type: header.kind.object_type(),
        ddl: Some(ddl),
        ..ObjectCommon::default()
    };

    let object = match header.kind {
        DdlKind::Table => CatalogObject::Table(TableMetadata {
            common,
            ..TableMetadata::default()
        }),
        DdlKind::View => CatalogObject::View(ViewMetadata {
            common,
            ..ViewMetadata::default()
        }),
        DdlKind::MaterializedView => CatalogObject::MaterializedView(MViewMetadata {
            common,
            ..MViewMetadata::default()
        }),
        DdlKind::Package => CatalogObject::Package(PackageMetadata {
            common,
            ..PackageMetadata::default()
        }),
        DdlKind::Procedure => CatalogObject::Procedure(ProcedureMetadata {
            common,
            signature: RoutineSignature {
                routine_name: obj_name,
                ..RoutineSignature::default()
            },
        }),
        DdlKind::Function => CatalogObject::Function(FunctionMetadata {
            common,
            signature: RoutineSignature {
                routine_name: obj_name,
                ..RoutineSignature::default()
            },
            ..FunctionMetadata::default()
        }),
        DdlKind::Sequence => CatalogObject::Sequence(SequenceMetadata {
            common,
            ..SequenceMetadata::default()
        }),
        DdlKind::Trigger => CatalogObject::Trigger(TriggerMetadata {
            common,
            ..TriggerMetadata::default()
        }),
        DdlKind::Type => CatalogObject::Type(TypeMetadata {
            common,
            ..TypeMetadata::default()
        }),
        // Filtered above — the match is exhaustive only because we
        // handle every concrete kind.
        DdlKind::PackageBody | DdlKind::TypeBody | DdlKind::Unknown => return None,
    };

    Some((owner, obj_name, object))
}

/// Object kinds the per-file DDL classifier recognizes. `Unknown`
/// represents honest uncertainty (R13) — the header didn't tokenize
/// into a kind we model. `PackageBody` / `TypeBody` are recognized
/// separately so the classifier can skip them without confusing them
/// with their specs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DdlKind {
    Table,
    View,
    MaterializedView,
    Package,
    PackageBody,
    Procedure,
    Function,
    Sequence,
    Trigger,
    Type,
    TypeBody,
    Unknown,
}

impl DdlKind {
    fn object_type(self) -> ObjectType {
        match self {
            DdlKind::Table => ObjectType::Table,
            DdlKind::View => ObjectType::View,
            DdlKind::MaterializedView => ObjectType::MaterializedView,
            DdlKind::Package | DdlKind::PackageBody => ObjectType::Package,
            DdlKind::Procedure => ObjectType::Procedure,
            DdlKind::Function => ObjectType::Function,
            DdlKind::Sequence => ObjectType::Sequence,
            DdlKind::Trigger => ObjectType::Trigger,
            DdlKind::Type | DdlKind::TypeBody => ObjectType::Type,
            DdlKind::Unknown => ObjectType::Unknown,
        }
    }
}

/// Parsed CREATE header: the typed `DdlKind` plus the upper-cased
/// remainder of the DDL starting immediately after the kind tokens.
/// Callers use `after_kind` to locate the `[OWNER.]NAME` — it has
/// already been stripped of leading comments / whitespace / `CREATE`
/// modifiers / kind tokens so a substring match in there cannot be
/// fooled by body content.
#[derive(Clone, Debug)]
struct ParsedCreateHeader {
    kind: DdlKind,
    after_kind: String,
}

/// Parse the CREATE header of a raw DDL string.
///
/// Skips leading whitespace, `--` line comments, and `/* … */` block
/// comments. Consumes `CREATE` then optional `OR REPLACE`, optional
/// `FORCE` / `EDITIONABLE` / `NONEDITIONABLE` (in any order), then
/// reads one or two tokens to form a [`DdlKind`] (multi-word kinds
/// `MATERIALIZED VIEW`, `PACKAGE BODY`, `TYPE BODY` handled). Returns
/// `None` only when the input has no `CREATE` token at all; an
/// unrecognized kind word produces `DdlKind::Unknown` so callers can
/// represent honest uncertainty (R13).
fn parse_create_header(ddl: &str) -> Option<ParsedCreateHeader> {
    let mut cursor = Cursor::new(ddl);
    cursor.skip_ws_and_comments();

    // Must start with `CREATE`.
    if !cursor.consume_keyword("CREATE") {
        return None;
    }
    cursor.skip_ws_and_comments();

    // Optional `OR REPLACE`.
    if cursor.consume_keyword("OR") {
        cursor.skip_ws_and_comments();
        // `OR` without `REPLACE` is malformed; let it fall through to
        // kind parsing — the kind word won't match and we'll honestly
        // return `Unknown`.
        let _ = cursor.consume_keyword("REPLACE");
        cursor.skip_ws_and_comments();
    }

    // Optional `FORCE` / `EDITIONABLE` / `NONEDITIONABLE` modifiers,
    // any order, any subset.
    loop {
        if cursor.consume_keyword("FORCE")
            || cursor.consume_keyword("NONEDITIONABLE")
            || cursor.consume_keyword("EDITIONABLE")
            || cursor.consume_keyword("NO")
        {
            cursor.skip_ws_and_comments();
            continue;
        }
        break;
    }

    // Read the kind word (one token, possibly extended to two for
    // `MATERIALIZED VIEW` / `PACKAGE BODY` / `TYPE BODY`).
    let first = match cursor.consume_identifier() {
        Some(tok) => tok,
        None => {
            return Some(ParsedCreateHeader {
                kind: DdlKind::Unknown,
                after_kind: cursor.upper_remainder(),
            });
        }
    };
    cursor.skip_ws_and_comments();

    // Speculatively look at the second token without committing — only
    // commit if it forms a known two-word kind.
    let kind = match first.as_str() {
        "MATERIALIZED" => {
            if cursor.peek_keyword("VIEW") {
                cursor.consume_keyword("VIEW");
                cursor.skip_ws_and_comments();
                DdlKind::MaterializedView
            } else {
                DdlKind::Unknown
            }
        }
        "PACKAGE" => {
            if cursor.peek_keyword("BODY") {
                cursor.consume_keyword("BODY");
                cursor.skip_ws_and_comments();
                DdlKind::PackageBody
            } else {
                DdlKind::Package
            }
        }
        "TYPE" => {
            if cursor.peek_keyword("BODY") {
                cursor.consume_keyword("BODY");
                cursor.skip_ws_and_comments();
                DdlKind::TypeBody
            } else {
                DdlKind::Type
            }
        }
        "TABLE" => DdlKind::Table,
        "VIEW" => DdlKind::View,
        "PROCEDURE" => DdlKind::Procedure,
        "FUNCTION" => DdlKind::Function,
        "SEQUENCE" => DdlKind::Sequence,
        "TRIGGER" => DdlKind::Trigger,
        _ => DdlKind::Unknown,
    };

    Some(ParsedCreateHeader {
        kind,
        after_kind: cursor.upper_remainder(),
    })
}

/// Hand-rolled byte cursor for the CREATE header tokenizer.
///
/// Only knows enough about SQL to skip whitespace / `--` line
/// comments / `/* … */` block comments and to read alphabetic
/// identifier keywords case-insensitively. It deliberately does
/// **not** try to parse the whole DDL — every operation past the
/// kind word is delegated to [`extract_owner_and_name`] working on
/// the upper-cased remainder.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            // Skip ASCII whitespace.
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            // `--` line comment.
            if self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos].eq(&b'-')
                && self.bytes[self.pos + 1].eq(&b'-')
            {
                self.pos += 2;
                while self.pos < self.bytes.len() && self.bytes[self.pos].ne(&b'\n') {
                    self.pos += 1;
                }
                continue;
            }
            // `/* … */` block comment.
            if self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos].eq(&b'/')
                && self.bytes[self.pos + 1].eq(&b'*')
            {
                self.pos += 2;
                while self.pos + 1 < self.bytes.len()
                    && !(self.bytes[self.pos].eq(&b'*') && self.bytes[self.pos + 1].eq(&b'/'))
                {
                    self.pos += 1;
                }
                if self.pos + 1 < self.bytes.len() {
                    self.pos += 2; // consume the closing `*/`
                } else {
                    self.pos = self.bytes.len(); // unterminated — end-of-input
                }
                continue;
            }
            break;
        }
    }

    /// Returns true if the next identifier token matches `kw`
    /// case-insensitively (and is followed by a non-identifier
    /// character or end-of-input). Does not advance the cursor.
    fn peek_keyword(&self, kw: &str) -> bool {
        let end = self.pos + kw.len();
        if end > self.bytes.len() {
            return false;
        }
        if !self.bytes[self.pos..end].eq_ignore_ascii_case(kw.as_bytes()) {
            return false;
        }
        // Word boundary check — `CREATEDOC` must not match `CREATE`.
        if end < self.bytes.len() {
            let next = self.bytes[end];
            if next.eq(&b'_') || next.is_ascii_alphanumeric() {
                return false;
            }
        }
        true
    }

    fn consume_keyword(&mut self, kw: &str) -> bool {
        if self.peek_keyword(kw) {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }

    /// Consume the next bare ASCII identifier (letters / digits /
    /// underscore, must start with a letter) and return it
    /// upper-cased. Returns `None` if the cursor is not on an
    /// identifier start character — e.g. a quoted identifier or a
    /// punctuation token. Quoted identifiers in the header position
    /// (the kind word) are not legal Oracle DDL so we don't bother.
    fn consume_identifier(&mut self) -> Option<String> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let first = self.bytes[self.pos];
        if !first.is_ascii_alphabetic() {
            return None;
        }
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_alphanumeric() || b.eq(&b'_') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        Some(raw.to_ascii_uppercase())
    }

    /// Return the rest of the input from the current cursor position,
    /// upper-cased. Used to hand off to [`extract_owner_and_name`].
    fn upper_remainder(&self) -> String {
        std::str::from_utf8(&self.bytes[self.pos..])
            .unwrap_or("")
            .to_ascii_uppercase()
    }
}

/// Extract the optional `OWNER` and the bare `OBJECT` name from the
/// upper-cased remainder that follows the parsed `CREATE <KIND>`
/// header. Strips surrounding quotes (so `CREATE TABLE "HR"."EMP"`
/// works) and trailing punctuation / parenthesis that the column
/// list would attach. Operates on the post-header slice only — never
/// on the body — so it can't be fooled by `TABLE` appearing later.
fn extract_owner_and_name(after_kind: &str) -> Option<(Option<String>, String)> {
    let after = after_kind.trim_start();

    // Scan the `[OWNER.]NAME` token honouring double-quoted Oracle
    // identifiers. A `"..."` segment is a single token that may contain
    // whitespace and runs to its closing `"`; an unquoted segment stops
    // at whitespace, `(`, `;`, or other DDL punctuation. The owner/name
    // split is the first top-level (outside-quotes) `.`.
    let mut segments: Vec<Segment> = Vec::new();
    let bytes = after.as_bytes();
    let mut i = 0usize;
    'scan: while i < bytes.len() {
        if bytes[i].eq(&b'"') {
            // Quoted segment: consume up to (and including) the closing `"`.
            let content_start = i + 1;
            let mut j = content_start;
            while j < bytes.len() && bytes[j].ne(&b'"') {
                j += 1;
            }
            // Unterminated quote ⇒ malformed header; give up.
            if j >= bytes.len() {
                return None;
            }
            segments.push(Segment {
                text: after[content_start..j].to_string(),
                quoted: true,
            });
            i = j + 1; // skip closing quote
        } else {
            // Unquoted run: identifier chars only. Anything else (space,
            // `(`, `;`, `,`, …) terminates the `[OWNER.]NAME` token —
            // except a top-level `.` which separates owner from name.
            let start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c.eq(&'_') {
                    i += 1;
                } else {
                    break;
                }
            }
            // An empty unquoted run means we hit a non-identifier byte
            // that is not a segment separator: stop scanning the token.
            if i.eq(&start) {
                break 'scan;
            }
            segments.push(Segment {
                text: after[start..i].to_string(),
                quoted: false,
            });
        }

        // After a segment, a `.` continues into the next (NAME) segment;
        // anything else ends the `[OWNER.]NAME` token.
        if i < bytes.len() && bytes[i].eq(&b'.') {
            i += 1;
        } else {
            break 'scan;
        }
    }

    // Validate each segment: quoted segments accept any non-empty
    // content; unquoted segments must be a real identifier.
    let valid = |seg: &Segment| -> bool {
        if seg.text.is_empty() {
            return false;
        }
        seg.quoted || seg.text.chars().all(|c| c.is_alphanumeric() || c.eq(&'_'))
    };

    match segments.as_slice() {
        [name] if valid(name) => Some((None, name.text.clone())),
        [owner, name] if valid(owner) && valid(name) => {
            Some((Some(owner.text.clone()), name.text.clone()))
        }
        _ => None,
    }
}

/// One dot-delimited segment of a `[OWNER.]NAME` token, tracking whether
/// it originated from a double-quoted Oracle identifier (which may hold
/// whitespace and bypasses the unquoted identifier-char validity rule).
struct Segment {
    text: String,
    quoted: bool,
}
#[instrument(level = "trace", skip(cx, conn, request))]
pub async fn load_snapshot_from_connection<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    request: &CatalogLoadRequest,
) -> Result<CatalogSnapshot, CatalogError> {
    let connection_info = conn.describe(cx).await?;
    let resolved_schemas = resolve_schema_filters(&connection_info, request)?;
    let (oracle_version, version_warning) =
        oracle_version_from_server_version(&connection_info.server_version);

    let mut capabilities = negotiate_capabilities(cx, conn).await;
    if let Some(warning) = version_warning {
        capabilities.warnings.push(warning);
    }

    let mut snapshot = CatalogSnapshot::new(
        AnalysisProfile::for_version(oracle_version),
        capabilities,
        CatalogSource {
            kind: CatalogSourceKind::LiveConnection,
            path: None,
            description: Some(format!(
                "live extraction via {} from {}",
                connection_info.backend, connection_info.connect_string
            )),
        },
        Utc::now(),
    );

    if let Some(current_schema) = connection_info.current_schema.as_deref() {
        snapshot.profile.current_schema = snapshot.intern_schema_name(current_schema);
    }

    load_catalog_objects(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_columns(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_constraints(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_indexes(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_triggers(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_synonyms(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_routines(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_views(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_mviews(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_sequences(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_type_attrs(cx, conn, &mut snapshot, &resolved_schemas).await?;
    // Must precede grant extraction: ALL_TAB_PRIVS has no user/role
    // discriminator, so grantee classification consults `known_users`.
    load_catalog_users(cx, conn, &mut snapshot).await?;
    load_catalog_grants(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_db_links(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_table_comments(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_column_comments(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_editions(cx, conn, &mut snapshot).await?;
    load_catalog_editioning_views(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_vpd_policies(cx, conn, &mut snapshot, &resolved_schemas).await?;
    load_catalog_dependencies(cx, conn, &mut snapshot, &resolved_schemas).await?;
    if snapshot.capabilities.plscope_enabled {
        load_catalog_plscope_availability(cx, conn, &mut snapshot, &resolved_schemas).await?;
        load_catalog_plscope_identifiers(cx, conn, &mut snapshot, &resolved_schemas).await?;
    }

    Ok(snapshot)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum RoutineKind {
    Procedure,
    Function,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct RoutineLocator {
    owner: SchemaName,
    package_name: Option<ObjectName>,
    routine_name: ObjectName,
    subprogram_id: Option<u32>,
    overload: Option<u32>,
}

#[derive(Clone, Debug, Default)]
struct RoutineAccumulator {
    signature: Option<RoutineSignature>,
    kind_hint: Option<RoutineKind>,
    deterministic: bool,
    pipelined: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchemaCatalog {
    pub objects: HashMap<ObjectName, CatalogObject>,
    pub synonyms: HashMap<SynonymName, SynonymTarget>,
    pub grants: Vec<Grant>,
    pub indexes: HashMap<IndexName, IndexMetadata>,
    pub constraints: HashMap<ConstraintName, ConstraintMetadata>,
    pub triggers: HashMap<TriggerName, TriggerMetadata>,
    pub dependencies: Vec<CatalogDependency>,
    pub plscope: Option<PlScopeSnapshot>,
    /// Database links owned by this schema. Public links live in the
    /// synthetic `PUBLIC` schema. Sourced from `ALL_DB_LINKS`.
    #[serde(default)]
    pub db_links: Vec<DatabaseLink>,
    /// Per-object COMMENT ON TABLE / VIEW text. Sourced from
    /// `ALL_TAB_COMMENTS`.
    #[serde(default)]
    pub table_comments: Vec<TableComment>,
    /// Per-column COMMENT ON COLUMN text. Sourced from
    /// `ALL_COL_COMMENTS`.
    #[serde(default)]
    pub column_comments: Vec<ColumnComment>,
    /// Editioning views owned by this schema (the views that mask the
    /// underlying base table in an EBR shop). Sourced from
    /// `ALL_EDITIONING_VIEWS`.
    #[serde(default)]
    pub editioning_views: Vec<EditioningView>,
    /// VPD/RLS policies attached to objects in this schema. Sourced
    /// from `ALL_POLICIES`.
    #[serde(default)]
    pub vpd_policies: Vec<VpdPolicy>,
}

fn resolve_schema_filters(
    connection_info: &OracleConnectionInfo,
    request: &CatalogLoadRequest,
) -> Result<Vec<String>, CatalogError> {
    let mut resolved = Vec::<String>::new();

    for filter in &request.schema_filters {
        let schema_name = match filter {
            CatalogSchemaFilter::CurrentSchema => connection_info
                .current_schema
                .clone()
                .ok_or(CatalogError::CurrentSchemaUnavailable)?,
            CatalogSchemaFilter::Named(schema_name) => {
                let trimmed = schema_name.trim();
                if trimmed.is_empty() {
                    return Err(CatalogError::InvalidSchemaFilter {
                        schema_name: schema_name.clone(),
                    });
                }
                String::from(trimmed)
            }
        };

        if !resolved.iter().any(|candidate| candidate.eq(&schema_name)) {
            resolved.push(schema_name);
        }
    }

    if resolved.is_empty() {
        return Err(CatalogError::CurrentSchemaUnavailable);
    }

    Ok(resolved)
}

/// Fetch the canonical DDL + XML representation of a single object via
/// `DBMS_METADATA`.
///
/// Callers usually batch via [`populate_dbms_metadata_ddl`] which iterates
/// every object in a `CatalogSnapshot` after the structural loaders have
/// run. `object_type` must map to a DBMS_METADATA object type (see
/// [`object_type_to_dbms_metadata_value`]); unknown types return an
/// `Ok(None)` so the caller can continue without aborting the snapshot.
#[instrument(level = "trace", skip(cx, conn))]
pub async fn fetch_dbms_metadata_ddl<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    object_type: ObjectType,
    name: &str,
    owner: &str,
) -> Result<Option<DbmsMetadataDdl>, CatalogError> {
    let Some(dbms_type) = object_type_to_dbms_metadata_value(object_type) else {
        return Ok(None);
    };
    let sql = "select dbms_metadata.get_ddl(:1, :2, :3) as ddl_text, \
               dbms_metadata.get_xml(:1, :2, :3) as xml_text from dual";
    let params = vec![
        OracleBind::from(dbms_type.to_string()),
        OracleBind::from(name.to_string()),
        OracleBind::from(owner.to_string()),
    ];
    let rows = conn.query_rows(cx, sql, &params).await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };

    let ddl_text = match optional_nonblank_text(&row, "DDL_TEXT") {
        Some(value) => value.to_string(),
        None => return Ok(None),
    };
    let xml_text = optional_nonblank_text(&row, "XML_TEXT").map(String::from);
    let normalized_ddl = Some(normalize_dbms_metadata_ddl(&ddl_text));

    Ok(Some(DbmsMetadataDdl {
        ddl_text,
        normalized_ddl,
        xml_text,
    }))
}

/// Populate `ObjectCommon.ddl` for every object in the snapshot using
/// `DBMS_METADATA.GET_DDL` and `DBMS_METADATA.GET_XML`. Skips silently when
/// `capabilities.can_use_dbms_metadata` is false. Failures on individual
/// objects are recorded as `CapabilityWarning`s on the snapshot and do not
/// abort the populate pass.
#[instrument(level = "trace", skip(cx, conn, snapshot))]
pub async fn populate_dbms_metadata_ddl<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
) -> Result<(), CatalogError> {
    if !snapshot.capabilities.can_use_dbms_metadata {
        return Ok(());
    }

    let mut targets: Vec<(SchemaName, ObjectName, ObjectType, String, String)> = Vec::new();
    for (owner, schema) in &snapshot.schemas {
        let owner_name = snapshot
            .interner
            .resolve(owner.symbol())
            .unwrap_or("")
            .to_string();
        for (name, object) in &schema.objects {
            let common = catalog_object_common(object);
            let object_name = snapshot
                .interner
                .resolve(name.symbol())
                .unwrap_or("")
                .to_string();
            targets.push((
                *owner,
                *name,
                common.object_type,
                owner_name.clone(),
                object_name,
            ));
        }
    }

    let mut warnings: Vec<CapabilityWarning> = Vec::new();
    let mut writes: Vec<(SchemaName, ObjectName, DbmsMetadataDdl)> = Vec::new();
    for (owner_symbol, name_symbol, object_type, owner_text, name_text) in targets {
        if owner_text.is_empty() || name_text.is_empty() {
            continue;
        }
        match fetch_dbms_metadata_ddl(cx, conn, object_type, &name_text, &owner_text).await {
            Ok(Some(ddl)) => writes.push((owner_symbol, name_symbol, ddl)),
            Ok(None) => {}
            Err(error) => warnings.push(CapabilityWarning {
                code: String::from("dbms-metadata-fetch-failed"),
                message: format!("DBMS_METADATA.GET_DDL({owner_text}.{name_text}) failed: {error}"),
                remediation: Some(String::from(
                    "Ensure DBMS_METADATA execute privilege is granted; the object may be wrapped or in an inaccessible edition.",
                )),
            }),
        }
    }

    for (owner_symbol, name_symbol, ddl) in writes {
        if let Some(catalog_object) = snapshot
            .schemas
            .get_mut(&owner_symbol)
            .and_then(|schema| schema.objects.get_mut(&name_symbol))
        {
            set_catalog_object_ddl(catalog_object, ddl);
        }
    }

    snapshot.capabilities.warnings.extend(warnings);
    Ok(())
}

fn set_catalog_object_ddl(object: &mut CatalogObject, ddl: DbmsMetadataDdl) {
    match object {
        CatalogObject::Table(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::View(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::MaterializedView(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Sequence(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Type(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Package(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Procedure(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Function(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::Trigger(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::SchedulerJob(metadata) => metadata.common.ddl = Some(ddl),
        CatalogObject::EditioningView(metadata) => metadata.common.ddl = Some(ddl),
    }
}

/// Normalize DDL text emitted by `DBMS_METADATA.GET_DDL` so equality checks
/// across runs ignore cosmetic differences:
///
/// - Trim leading + trailing whitespace.
/// - Collapse runs of whitespace inside the body to a single space (newlines
///   are preserved as-is so the result remains readable).
/// - Strip the trailing `/` SQL*Plus terminator if present.
#[must_use]
pub fn normalize_dbms_metadata_ddl(text: &str) -> String {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_suffix('/').unwrap_or(trimmed).trim_end();
    let mut normalized = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for c in trimmed.chars() {
        if c.eq(&' ') || c.eq(&'\t') {
            if !prev_space {
                normalized.push(' ');
                prev_space = true;
            }
        } else {
            normalized.push(c);
            prev_space = false;
        }
    }
    normalized
}

/// Map an `ObjectType` to the string the `DBMS_METADATA.GET_DDL` /
/// `GET_XML` overloads expect as their first parameter. Returns `None` for
/// types that have no DBMS_METADATA representation (e.g.
/// `ObjectType::Unknown`, `ObjectType::Constraint`).
#[must_use]
pub fn object_type_to_dbms_metadata_value(object_type: ObjectType) -> Option<&'static str> {
    match object_type {
        ObjectType::Table => Some("TABLE"),
        ObjectType::View => Some("VIEW"),
        ObjectType::MaterializedView => Some("MATERIALIZED_VIEW"),
        ObjectType::Sequence => Some("SEQUENCE"),
        ObjectType::Type => Some("TYPE"),
        ObjectType::Package => Some("PACKAGE"),
        ObjectType::Procedure => Some("PROCEDURE"),
        ObjectType::Function => Some("FUNCTION"),
        ObjectType::Trigger => Some("TRIGGER"),
        ObjectType::EditioningView => Some("VIEW"),
        ObjectType::SchedulerJob => Some("PROCOBJ"),
        ObjectType::Synonym => Some("SYNONYM"),
        ObjectType::Index => Some("INDEX"),
        ObjectType::Constraint | ObjectType::Unknown => None,
    }
}

/// Probe an `OracleConnection` for the dictionary surface it can actually
/// reach. The loader records `CatalogCapabilities` from real probe outcomes
/// instead of optimistic defaults, so downstream consumers can render an
/// accurate doctor report and the right `MissingPermissionReport` rows.
///
/// The probes are intentionally cheap (`WHERE rownum = 0` / `BEGIN ... END`
/// blocks that no-op) and resilient to permission errors: each probe falls
/// back to `false` on any error, with a typed `CapabilityWarning` carrying
/// the probe name + Oracle error message + remediation hint.
#[must_use]
#[instrument(level = "trace", skip(cx, conn))]
pub async fn negotiate_capabilities<C: OracleConnection>(cx: &Cx, conn: &C) -> CatalogCapabilities {
    let mut capabilities = CatalogCapabilities {
        can_query_all_views: false,
        ..CatalogCapabilities::default()
    };

    type CapabilitySetter = fn(&mut CatalogCapabilities);
    let probes: &[(&str, &str, &str, CapabilitySetter)] = &[
        (
            "select 1 from all_objects where rownum = 0",
            "all-views-probe",
            "ALL_OBJECTS unreachable; ensure the user has SELECT privilege on the standard ALL_* views.",
            |c| c.can_query_all_views = true,
        ),
        (
            "select 1 from dba_objects where rownum = 0",
            "dba-views-probe",
            "DBA_OBJECTS unreachable; grant SELECT_CATALOG_ROLE or specific DBA_* privileges to widen cross-schema coverage.",
            |c| c.can_query_dba_views = true,
        ),
        (
            "select 1 from all_source where rownum = 0",
            "all-source-probe",
            "ALL_SOURCE unreachable; ensure the user can read package/procedure bodies for source extraction.",
            |c| c.can_read_source = true,
        ),
        (
            "select 1 from all_scheduler_jobs where rownum = 0",
            "scheduler-probe",
            "ALL_SCHEDULER_JOBS unreachable; grant SELECT on the scheduler dictionary views to enable scheduler lineage.",
            |c| c.can_query_scheduler = true,
        ),
        (
            "select 1 from all_tab_privs where rownum = 0",
            "roles-and-grants-probe",
            "ALL_TAB_PRIVS unreachable; grant SELECT_CATALOG_ROLE to enable privilege chain analysis.",
            |c| c.can_query_roles_and_grants = true,
        ),
        (
            "select 1 from all_plsql_object_settings where rownum = 0",
            "plscope-probe",
            "ALL_PLSQL_OBJECT_SETTINGS unreachable; PL/Scope identifier extraction (PLSQL-CAT-010/011) will be unavailable.",
            |c| c.plscope_enabled = true,
        ),
    ];

    for (sql, probe_code, remediation, setter) in probes {
        match conn.query_rows(cx, sql, &[]).await {
            Ok(_) => setter(&mut capabilities),
            Err(error) => capabilities.warnings.push(CapabilityWarning {
                code: String::from(*probe_code),
                message: format!("probe `{sql}` failed: {error}"),
                remediation: Some(String::from(*remediation)),
            }),
        }
    }

    // DBMS_METADATA detection is a stored-procedure probe. Use an anonymous
    // PL/SQL block that bails out cheaply — `dbms_metadata.get_ddl` against a
    // guaranteed-existing object (`DUAL`) returns a CLOB without DDL side
    // effects.
    let dbms_metadata_probe =
        "begin if dbms_metadata.get_ddl('TABLE', 'DUAL', 'SYS') is null then null; end if; end;";
    match conn.execute(cx, dbms_metadata_probe, &[]).await {
        Ok(_) => {
            capabilities.can_use_dbms_metadata = true;
        }
        Err(error) => {
            capabilities.warnings.push(CapabilityWarning {
                code: String::from("dbms-metadata-probe"),
                message: format!("DBMS_METADATA probe failed: {error}"),
                remediation: Some(String::from(
                    "grant execute on DBMS_METADATA to <user> to enable PLSQL-CAT-015 DDL extraction.",
                )),
            });
        }
    }

    capabilities
}

fn oracle_version_from_server_version(
    server_version: &str,
) -> (OracleVersion, Option<CapabilityWarning>) {
    let major_component = server_version
        .split('.')
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .ok();

    match major_component {
        Some(11) => (OracleVersion::Oracle11g, None),
        Some(12) => (OracleVersion::Oracle12c, None),
        Some(19) => (OracleVersion::Oracle19c, None),
        Some(21) => (OracleVersion::Oracle21c, None),
        Some(23) => (OracleVersion::Oracle23ai, None),
        Some(26) => (OracleVersion::Oracle26ai, None),
        _ => (
            OracleVersion::Oracle19c,
            Some(CapabilityWarning {
                code: String::from("catalog-version-parse-fallback"),
                message: format!(
                    "server version `{server_version}` did not map cleanly to a supported OracleVersion; defaulted AnalysisProfile.oracle_version to Oracle19c"
                ),
                remediation: Some(String::from(
                    "Set the workspace AnalysisProfile explicitly if this estate targets a newer or older Oracle release.",
                )),
            }),
        ),
    }
}

async fn load_catalog_objects<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  object_name,
  object_type,
  status,
  to_char(last_ddl_time, 'YYYY-MM-DD\"T\"HH24:MI:SS') as last_ddl_time_iso,
  editionable,
  edition_name
from all_objects
where owner in ({owner_clause})
  and object_type in (
    'TABLE',
    'VIEW',
    'MATERIALIZED VIEW',
    'SEQUENCE',
    'TYPE',
    'PACKAGE',
    'PROCEDURE',
    'FUNCTION',
    'TRIGGER',
    'EDITIONING VIEW'
  )
order by owner, object_type, object_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_object_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_plscope_identifiers<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  name,
  type,
  usage,
  line,
  col,
  object_name
from all_identifiers
where owner in ({owner_clause})
order by owner, object_name, line, col
"
    );
    let params = schema_filter_params(schema_names);
    let rows = match conn.query_rows(cx, &sql, &params).await {
        Ok(rows) => rows,
        Err(error) => {
            snapshot.capabilities.warnings.push(CapabilityWarning {
                code: String::from("plscope-identifiers-failed"),
                message: format!("ALL_IDENTIFIERS query failed: {error}"),
                remediation: Some(String::from(
                    "Ensure the user can read ALL_IDENTIFIERS, or recompile target objects with `alter session set plscope_settings = 'identifiers:all'`.",
                )),
            });
            return Ok(());
        }
    };

    for row in &rows {
        let Some(owner_text) = optional_nonblank_text(row, "OWNER") else {
            continue;
        };
        let Some(object_name_text) = optional_nonblank_text(row, "OBJECT_NAME") else {
            continue;
        };
        let Some(identifier_name_text) = optional_nonblank_text(row, "NAME") else {
            continue;
        };
        let Some(owner) = snapshot.intern_schema_name(owner_text) else {
            continue;
        };
        let Some(object_name) = snapshot.intern_object_name(object_name_text) else {
            continue;
        };
        let Some(identifier_name) = snapshot.intern_member_name(identifier_name_text) else {
            continue;
        };
        let identifier_type = optional_nonblank_text(row, "TYPE")
            .map(String::from)
            .unwrap_or_default();
        let usage = optional_nonblank_text(row, "USAGE")
            .map(String::from)
            .unwrap_or_default();
        let line = optional_u32(row, "LINE")?.unwrap_or(0);
        let column = optional_u32(row, "COL")?.unwrap_or(0);

        let identifier = CompilerIdentifier {
            owner,
            object_name,
            identifier_name,
            identifier_type,
            usage,
            line,
            column,
        };

        let plscope = snapshot
            .schemas
            .entry(owner)
            .or_default()
            .plscope
            .get_or_insert_with(|| PlScopeSnapshot {
                availability: PlScopeAvailability::IdentifiersOnly,
                collected_at: Some(snapshot.generated_at),
                ..PlScopeSnapshot::default()
            });
        plscope.identifiers.push(identifier);
    }

    Ok(())
}

async fn load_catalog_plscope_availability<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  plscope_settings
from all_plsql_object_settings
where owner in ({owner_clause})
"
    );
    let params = schema_filter_params(schema_names);
    let rows = match conn.query_rows(cx, &sql, &params).await {
        Ok(rows) => rows,
        Err(error) => {
            // Record the warning, leave per-schema plscope as None (the
            // default `PlScopeAvailability::NotAvailable`), and return Ok.
            snapshot.capabilities.warnings.push(CapabilityWarning {
                code: String::from("plscope-detect-failed"),
                message: format!("ALL_PLSQL_OBJECT_SETTINGS query failed: {error}"),
                remediation: Some(String::from(
                    "Grant SELECT on ALL_PLSQL_OBJECT_SETTINGS, or accept that PL/Scope detection is unavailable.",
                )),
            });
            return Ok(());
        }
    };

    // Per-schema tallies: how many PLSQL units have IDENTIFIERS:* and how
    // many also carry STATEMENTS:*. The most informative observed setting
    // wins per-schema: STATEMENTS > IDENTIFIERS > NONE.
    let mut per_schema: HashMap<SchemaName, PlScopeTally> = HashMap::new();
    for row in &rows {
        let owner_text = match row.text("OWNER") {
            Some(value) if !value.trim().is_empty() => value,
            _ => continue,
        };
        let settings = row
            .text("PLSCOPE_SETTINGS")
            .unwrap_or("")
            .to_ascii_uppercase();
        let Some(owner) = snapshot.intern_schema_name(owner_text) else {
            continue;
        };
        let tally = per_schema.entry(owner).or_default();
        tally.total = tally.total.saturating_add(1);
        if settings.contains("STATEMENTS:") && !settings.contains("STATEMENTS:NONE") {
            tally.with_statements = tally.with_statements.saturating_add(1);
        }
        if settings.contains("IDENTIFIERS:") && !settings.contains("IDENTIFIERS:NONE") {
            tally.with_identifiers = tally.with_identifiers.saturating_add(1);
        }
    }

    for (owner, tally) in per_schema {
        let availability = if tally.with_statements > 0 {
            PlScopeAvailability::IdentifiersAndStatements
        } else if tally.with_identifiers > 0 {
            PlScopeAvailability::IdentifiersOnly
        } else if tally.total > 0 {
            // PLSQL objects exist but none compiled with PL/Scope enabled.
            PlScopeAvailability::AvailableButStale
        } else {
            PlScopeAvailability::NotAvailable
        };
        let schema_catalog = snapshot.schemas.entry(owner).or_default();
        schema_catalog.plscope = Some(PlScopeSnapshot {
            availability,
            collected_at: Some(snapshot.generated_at),
            ..PlScopeSnapshot::default()
        });
    }

    Ok(())
}

#[derive(Default)]
struct PlScopeTally {
    total: usize,
    with_identifiers: usize,
    with_statements: usize,
}

async fn load_catalog_dependencies<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  name,
  type,
  referenced_owner,
  referenced_name,
  referenced_type,
  dependency_type
from all_dependencies
where owner in ({owner_clause})
order by owner, name, referenced_owner, referenced_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_dependency_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_columns<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  table_name,
  column_name,
  nvl(column_id, internal_column_id) as column_position,
  data_type_owner,
  data_type,
  data_length,
  data_precision,
  data_scale,
  char_used,
  nullable,
  data_default_vc,
  virtual_column,
  hidden_column
from all_tab_cols
where owner in ({owner_clause})
order by owner, table_name, nvl(column_id, internal_column_id)
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_column_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_constraints<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  c.owner,
  c.constraint_name,
  c.table_name,
  c.constraint_type,
  c.r_owner as referenced_table_owner,
  p.table_name as referenced_table_name,
  c.search_condition_vc,
  case when c.deferrable = 'DEFERRABLE' then 'Y' else 'N' end as is_deferrable,
  case when c.deferred = 'DEFERRED' then 'Y' else 'N' end as is_deferred,
  child.column_name,
  child.position as column_position,
  parent.column_name as referenced_column_name
from all_constraints c
left join all_constraints p
  on p.owner = c.r_owner
 and p.constraint_name = c.r_constraint_name
left join all_cons_columns child
  on child.owner = c.owner
 and child.constraint_name = c.constraint_name
left join all_cons_columns parent
  on parent.owner = p.owner
 and parent.constraint_name = p.constraint_name
 and parent.position = child.position
where c.owner in ({owner_clause})
  and c.constraint_type in ('P', 'R', 'U', 'C', 'F')
order by c.owner, c.constraint_name, child.position
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_constraint_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_indexes<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  i.owner,
  i.index_name,
  i.table_owner,
  i.table_name,
  case when i.uniqueness = 'UNIQUE' then 'Y' else 'N' end as is_unique,
  i.index_type,
  i.status,
  c.column_name,
  c.column_position
from all_indexes i
left join all_ind_columns c
  on c.index_owner = i.owner
 and c.index_name = i.index_name
 and c.table_owner = i.table_owner
 and c.table_name = i.table_name
where i.owner in ({owner_clause})
order by i.owner, i.index_name, c.column_position
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_index_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_triggers<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  trigger_name,
  table_owner,
  table_name,
  trigger_type,
  triggering_event,
  when_clause
from all_triggers
where owner in ({owner_clause})
  and base_object_type in ('TABLE', 'VIEW')
order by owner, trigger_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_trigger_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_synonyms<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  synonym_name,
  table_owner,
  table_name,
  db_link
from all_synonyms
where owner = 'PUBLIC'
   or owner in ({owner_clause})
order by owner, synonym_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_synonym_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_routines<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let procedure_sql = format!(
        "
select
  owner,
  object_name,
  procedure_name,
  subprogram_id,
  overload,
  object_type,
  deterministic,
  pipelined
from all_procedures
where owner in ({owner_clause})
  and (procedure_name is not null or object_type in ('FUNCTION', 'PROCEDURE'))
order by owner, object_name, procedure_name, subprogram_id
"
    );
    let argument_sql = format!(
        "
select
  owner,
  package_name,
  object_name,
  subprogram_id,
  overload,
  argument_name,
  position,
  sequence,
  data_type,
  type_owner,
  type_name,
  data_length,
  data_precision,
  data_scale,
  in_out,
  defaulted
from all_arguments
where owner in ({owner_clause})
  and data_level = 0
order by owner, package_name, object_name, subprogram_id, sequence
"
    );
    let params = schema_filter_params(schema_names);
    let procedure_rows = conn.query_rows(cx, &procedure_sql, &params).await?;
    let argument_rows = conn.query_rows(cx, &argument_sql, &params).await?;
    let mut routines = HashMap::<RoutineLocator, RoutineAccumulator>::new();

    for row in &procedure_rows {
        apply_routine_row(snapshot, row, &mut routines)?;
    }
    for row in &argument_rows {
        apply_argument_row(snapshot, row, &mut routines)?;
    }

    finalize_routines(snapshot, routines)
}

async fn load_catalog_views<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  view_name,
  text_vc,
  read_only
from all_views
where owner in ({owner_clause})
order by owner, view_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_view_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_mviews<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  mview_name,
  refresh_mode,
  refresh_method,
  query
from all_mviews
where owner in ({owner_clause})
order by owner, mview_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_mview_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_sequences<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  sequence_owner,
  sequence_name,
  min_value,
  max_value,
  increment_by,
  cycle_flag,
  order_flag,
  cache_size
from all_sequences
where sequence_owner in ({owner_clause})
order by sequence_owner, sequence_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_sequence_row(snapshot, &row)?;
    }

    Ok(())
}

async fn load_catalog_type_attrs<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  type_name,
  attr_name,
  attr_no,
  attr_type_owner,
  attr_type_name,
  length,
  precision,
  scale
from all_type_attrs
where owner in ({owner_clause})
order by owner, type_name, attr_no
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_type_attr_row(snapshot, &row)?;
    }

    Ok(())
}

/// Load `ALL_DB_LINKS` rows into [`SchemaCatalog::db_links`].
///
/// Both private (owned by a user schema) and public (`OWNER = PUBLIC`)
/// links are fetched in a single query. The schema filter is applied as
/// `owner in ({schemas}) or owner = 'PUBLIC'` so public links always
/// surface — a remote reference can target a public link from any
/// schema and lineage needs that resolution to succeed.
async fn load_catalog_db_links<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  db_link,
  host
from all_db_links
where owner = 'PUBLIC'
   or owner in ({owner_clause})
order by owner, db_link
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_db_link_row(snapshot, &row)?;
    }

    Ok(())
}

/// Load `ALL_POLICIES` rows into [`SchemaCatalog::vpd_policies`].
/// One row per (object, policy_group, policy_name) triple. Filters to
/// enabled and disabled policies alike because lineage needs to know
/// about disabled ones as deployment-debt.
async fn load_catalog_vpd_policies<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  object_owner,
  object_name,
  policy_group,
  policy_name,
  pf_owner,
  package,
  function,
  sel,
  ins,
  upd,
  del,
  enable
from all_policies
where object_owner in ({owner_clause})
order by object_owner, object_name, policy_group, policy_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_vpd_policy_row(snapshot, &row)?;
    }
    Ok(())
}

/// Load `ALL_EDITIONS` into [`CatalogSnapshot::editions`]. The edition
/// tree is database-wide (not per-schema) so this loader takes no schema
/// filter.
async fn load_catalog_editions<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
) -> Result<(), CatalogError> {
    let sql = "
select
  edition_name,
  parent_edition_name,
  usable
from all_editions
order by edition_name
";
    for row in conn.query_rows(cx, sql, &[]).await? {
        apply_edition_row(snapshot, &row)?;
    }
    Ok(())
}

/// Load `ALL_EDITIONING_VIEWS` rows into
/// [`SchemaCatalog::editioning_views`].
async fn load_catalog_editioning_views<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  view_name,
  table_name
from all_editioning_views
where owner in ({owner_clause})
order by owner, view_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_editioning_view_row(snapshot, &row)?;
    }
    Ok(())
}

/// Load `ALL_TAB_COMMENTS` rows into [`SchemaCatalog::table_comments`].
/// Filters NULL comments at the source to keep the snapshot compact.
async fn load_catalog_table_comments<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  table_name,
  table_type,
  comments
from all_tab_comments
where owner in ({owner_clause})
  and comments is not null
order by owner, table_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_table_comment_row(snapshot, &row)?;
    }

    Ok(())
}

/// Load `ALL_COL_COMMENTS` rows into [`SchemaCatalog::column_comments`].
/// Filters NULL comments at the source.
async fn load_catalog_column_comments<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  owner,
  table_name,
  column_name,
  comments
from all_col_comments
where owner in ({owner_clause})
  and comments is not null
order by owner, table_name, column_name
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_column_comment_row(snapshot, &row)?;
    }

    Ok(())
}

/// Populate [`CatalogSnapshot::known_users`] from `ALL_USERS`.
///
/// `ALL_USERS` is readable by `PUBLIC` on a stock Oracle instance, so this
/// needs no `SELECT_CATALOG_ROLE` / DBA grant. The resulting set lets
/// [`grantee_from_dictionary_value`] discriminate object-privilege grantees
/// (whose `ALL_TAB_PRIVS.GRANTEE` carries no user/role type column) into
/// real users versus database roles.
///
/// Failure is non-fatal: if `ALL_USERS` cannot be read, the snapshot keeps
/// `known_users` set to `None` (an explicit "undetermined" state, R13) and records
/// a [`CapabilityWarning`] rather than aborting the extraction. Callers must
/// invoke this BEFORE [`load_catalog_grants`].
async fn load_catalog_users<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
) -> Result<(), CatalogError> {
    let sql = "select username from all_users order by username";
    match conn.query_rows(cx, sql, &[]).await {
        Ok(rows) => {
            let mut users = HashSet::with_capacity(rows.len());
            for row in &rows {
                let username = row.require_text("USERNAME")?;
                let Some(user) = snapshot.intern_user_name(username) else {
                    return Err(CatalogError::InvalidColumnValue {
                        column: String::from("USERNAME"),
                        expected: "interned user name",
                        value: String::from(username),
                    });
                };
                users.insert(user);
            }
            snapshot.known_users = Some(users);
        }
        Err(error) => {
            // R13: do not fail the snapshot and do not silently pretend the
            // grantee universe is known. Leave `known_users` as `None` so
            // grantee classification stays conservative downstream.
            snapshot.known_users = None;
            snapshot.capabilities.warnings.push(CapabilityWarning {
                code: String::from("all-users-probe"),
                message: format!("ALL_USERS read failed: {error}"),
                remediation: Some(String::from(
                    "ensure the analysis user can SELECT ALL_USERS so object grants to roles are not misclassified as direct user grants.",
                )),
            });
        }
    }
    Ok(())
}

async fn load_catalog_grants<C: OracleConnection>(
    cx: &Cx,
    conn: &C,
    snapshot: &mut CatalogSnapshot,
    schema_names: &[String],
) -> Result<(), CatalogError> {
    let owner_clause = oracle_bind_placeholders(schema_names.len(), 1);
    let sql = format!(
        "
select
  table_schema,
  table_name,
  grantee,
  privilege,
  grantable,
  hierarchy
from all_tab_privs
where table_schema in ({owner_clause})
order by table_schema, table_name, grantee, privilege
"
    );
    let params = schema_filter_params(schema_names);

    for row in conn.query_rows(cx, &sql, &params).await? {
        apply_grant_row(snapshot, &row)?;
    }

    Ok(())
}

fn oracle_bind_placeholders(count: usize, start_index: usize) -> String {
    (0..count)
        .map(|offset| format!(":{}", start_index + offset))
        .collect::<Vec<_>>()
        .join(", ")
}

fn hash_text(text: &str) -> Hash {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    // sha2 0.11+ returns `Array<u8, …>` from `finalize` which no
    // longer impls `LowerHex` directly; render byte-by-byte (matches
    // the plsql-store pattern). Keeps the bump from being a breaking
    // change for callers.
    let digest = hasher.finalize();
    let mut rendered = String::with_capacity(7 + digest.len() * 2);
    rendered.push_str("sha256:");
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    Hash::new(rendered)
}

fn schema_filter_params(schema_names: &[String]) -> Vec<OracleBind> {
    schema_names
        .iter()
        .cloned()
        .map(OracleBind::from)
        .collect::<Vec<_>>()
}

fn apply_object_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let object_name_text = row.require_text("OBJECT_NAME")?;
    let object_type_text = row.require_text("OBJECT_TYPE")?;
    let Some(object_type) = object_type_from_dictionary_value(object_type_text) else {
        return Ok(());
    };

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(object_name) = snapshot.intern_object_name(object_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OBJECT_NAME"),
            expected: "interned object name",
            value: String::from(object_name_text),
        });
    };

    let last_ddl_time =
        optional_nonblank_text(row, "LAST_DDL_TIME_ISO").and_then(parse_dictionary_timestamp);
    let editionable = optional_bool(row, "EDITIONABLE")?;
    let edition_name = optional_nonblank_text(row, "EDITION_NAME")
        .map(|value| {
            snapshot
                .interner
                .intern(value)
                .map(EditionName::from)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("EDITION_NAME"),
                    expected: "interned edition name",
                    value: String::from(value),
                })
        })
        .transpose()?;

    let common = ObjectCommon {
        owner,
        name: object_name,
        object_type,
        status: row
            .text("STATUS")
            .map(object_status_from_dictionary_value)
            .unwrap_or_default(),
        edition_name,
        editionable,
        last_ddl_time,
        ..ObjectCommon::default()
    };

    let Some(catalog_object) = blank_catalog_object(common) else {
        return Ok(());
    };

    snapshot
        .schemas
        .entry(owner)
        .or_default()
        .objects
        .insert(object_name, catalog_object);

    Ok(())
}

fn apply_dependency_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let name_text = row.require_text("NAME")?;
    let referenced_owner_text = row.require_text("REFERENCED_OWNER")?;
    let referenced_name_text = row.require_text("REFERENCED_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(object_name) = snapshot.intern_object_name(name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("NAME"),
            expected: "interned object name",
            value: String::from(name_text),
        });
    };
    let Some(referenced_owner) = snapshot.intern_schema_name(referenced_owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("REFERENCED_OWNER"),
            expected: "interned schema name",
            value: String::from(referenced_owner_text),
        });
    };
    let Some(referenced_name) = snapshot.intern_object_name(referenced_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("REFERENCED_NAME"),
            expected: "interned object name",
            value: String::from(referenced_name_text),
        });
    };

    let object_type = optional_nonblank_text(row, "TYPE")
        .and_then(object_type_from_dictionary_value)
        .unwrap_or_default();
    let referenced_type =
        optional_nonblank_text(row, "REFERENCED_TYPE").and_then(object_type_from_dictionary_value);

    let dependency = CatalogDependency {
        owner,
        name: object_name,
        object_type,
        referenced_owner: Some(referenced_owner),
        referenced_name,
        referenced_type,
        dependency_kind: optional_nonblank_text(row, "DEPENDENCY_TYPE")
            .map(catalog_dependency_kind_from_dictionary_value)
            .unwrap_or_default(),
        via_db_link: None,
    };

    snapshot
        .schemas
        .entry(owner)
        .or_default()
        .dependencies
        .push(dependency);

    Ok(())
}

fn parse_dictionary_timestamp(text: &str) -> Option<DateTime<Utc>> {
    // Expected shape from the loader query: `YYYY-MM-DD"T"HH24:MI:SS`.
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn catalog_dependency_kind_from_dictionary_value(text: &str) -> CatalogDependencyKind {
    match text.to_ascii_uppercase().as_str() {
        "HARD" => CatalogDependencyKind::Hard,
        "REF" => CatalogDependencyKind::Reference,
        "EXTENDED" => CatalogDependencyKind::Extended,
        _ => CatalogDependencyKind::default(),
    }
}

fn apply_column_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let table_name_text = row.require_text("TABLE_NAME")?;
    let column_name_text = row.require_text("COLUMN_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };
    let Some(column_name) = snapshot.intern_column_name(column_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("COLUMN_NAME"),
            expected: "interned column name",
            value: String::from(column_name_text),
        });
    };
    let data_type = data_type_ref_from_row(snapshot, row)?;

    let Some(schema_catalog) = snapshot.schemas.get_mut(&owner) else {
        return Ok(());
    };
    let Some(catalog_object) = schema_catalog.objects.get_mut(&table_name) else {
        return Ok(());
    };

    let default_expression = row
        .text("DATA_DEFAULT_VC")
        .map(String::from)
        .filter(|value| !value.trim().is_empty());
    let virtual_column = optional_bool(row, "VIRTUAL_COLUMN")?.unwrap_or(false);
    let column = ColumnMetadata {
        name: column_name,
        position: required_u32(row, "COLUMN_POSITION")?,
        data_type,
        nullable: optional_bool(row, "NULLABLE")?.unwrap_or(false),
        default_expression: if virtual_column {
            None
        } else {
            default_expression.clone()
        },
        generated_expression: if virtual_column {
            default_expression
        } else {
            None
        },
        hidden: optional_bool(row, "HIDDEN_COLUMN")?.unwrap_or(false),
    };

    match catalog_object {
        CatalogObject::Table(metadata) => {
            metadata.columns.insert(column.name, column);
        }
        CatalogObject::View(metadata) => {
            metadata.columns.insert(column.name, column);
        }
        CatalogObject::MaterializedView(metadata) => {
            metadata.columns.insert(column.name, column);
        }
        CatalogObject::EditioningView(metadata) => {
            metadata.columns.insert(column.name, column);
        }
        CatalogObject::Sequence(_)
        | CatalogObject::Type(_)
        | CatalogObject::Package(_)
        | CatalogObject::Procedure(_)
        | CatalogObject::Function(_)
        | CatalogObject::Trigger(_)
        | CatalogObject::SchedulerJob(_) => {}
    }

    Ok(())
}

fn apply_constraint_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let constraint_name_text = row.require_text("CONSTRAINT_NAME")?;
    let table_name_text = row.require_text("TABLE_NAME")?;
    let search_condition = optional_nonblank_text(row, "SEARCH_CONDITION_VC").map(String::from);

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(constraint_name) = snapshot.intern_constraint_name(constraint_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("CONSTRAINT_NAME"),
            expected: "interned constraint name",
            value: String::from(constraint_name_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };
    let referenced_table_owner = optional_nonblank_text(row, "REFERENCED_TABLE_OWNER")
        .map(|value| {
            snapshot
                .intern_schema_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("REFERENCED_TABLE_OWNER"),
                    expected: "interned schema name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let referenced_table_name = optional_nonblank_text(row, "REFERENCED_TABLE_NAME")
        .map(|value| {
            snapshot
                .intern_object_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("REFERENCED_TABLE_NAME"),
                    expected: "interned object name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let child_column = optional_nonblank_text(row, "COLUMN_NAME")
        .map(|value| {
            snapshot
                .intern_column_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("COLUMN_NAME"),
                    expected: "interned column name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let referenced_column = optional_nonblank_text(row, "REFERENCED_COLUMN_NAME")
        .map(|value| {
            snapshot
                .intern_column_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("REFERENCED_COLUMN_NAME"),
                    expected: "interned column name",
                    value: String::from(value),
                })
        })
        .transpose()?;

    let constraint_type = constraint_type_from_dictionary_value(
        row.require_text("CONSTRAINT_TYPE")?,
        search_condition.as_deref(),
        child_column.is_some(),
    );

    let metadata = snapshot
        .schemas
        .entry(owner)
        .or_default()
        .constraints
        .entry(constraint_name)
        .or_insert_with(|| ConstraintMetadata {
            name: constraint_name,
            table_owner: owner,
            table_name,
            constraint_type,
            columns: Vec::new(),
            referenced_table_owner,
            referenced_table_name,
            referenced_columns: Vec::new(),
            search_condition: search_condition.clone(),
            deferrable: optional_bool(row, "IS_DEFERRABLE").ok().flatten(),
            initially_deferred: optional_bool(row, "IS_DEFERRED").ok().flatten(),
        });

    metadata.table_name = table_name;
    metadata.constraint_type = constraint_type;
    metadata.referenced_table_owner = referenced_table_owner;
    metadata.referenced_table_name = referenced_table_name;
    metadata.search_condition = search_condition;
    metadata.deferrable = optional_bool(row, "IS_DEFERRABLE")?;
    metadata.initially_deferred = optional_bool(row, "IS_DEFERRED")?;

    if let Some(column_name) = child_column {
        push_unique_column(&mut metadata.columns, column_name);
    }
    if let Some(column_name) = referenced_column {
        push_unique_column(&mut metadata.referenced_columns, column_name);
    }

    Ok(())
}

fn apply_index_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let index_name_text = row.require_text("INDEX_NAME")?;
    let table_owner_text = row.require_text("TABLE_OWNER")?;
    let table_name_text = row.require_text("TABLE_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(index_name) = snapshot.intern_index_name(index_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("INDEX_NAME"),
            expected: "interned index name",
            value: String::from(index_name_text),
        });
    };
    let Some(table_owner) = snapshot.intern_schema_name(table_owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_OWNER"),
            expected: "interned schema name",
            value: String::from(table_owner_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };
    let index_column = optional_nonblank_text(row, "COLUMN_NAME")
        .map(|value| {
            snapshot
                .intern_column_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("COLUMN_NAME"),
                    expected: "interned column name",
                    value: String::from(value),
                })
        })
        .transpose()?;

    let metadata = snapshot
        .schemas
        .entry(owner)
        .or_default()
        .indexes
        .entry(index_name)
        .or_insert_with(|| IndexMetadata {
            name: index_name,
            table_owner,
            table_name,
            unique: optional_bool(row, "IS_UNIQUE")
                .ok()
                .flatten()
                .unwrap_or(false),
            columns: Vec::new(),
            index_type: String::from(row.text("INDEX_TYPE").unwrap_or_default()),
            status: row
                .text("STATUS")
                .map(object_status_from_dictionary_value)
                .unwrap_or_default(),
        });

    metadata.table_owner = table_owner;
    metadata.table_name = table_name;
    metadata.unique = optional_bool(row, "IS_UNIQUE")?.unwrap_or(false);
    metadata.index_type = String::from(row.text("INDEX_TYPE").unwrap_or_default());
    metadata.status = row
        .text("STATUS")
        .map(object_status_from_dictionary_value)
        .unwrap_or_default();

    if let Some(column_name) = index_column {
        push_unique_column(&mut metadata.columns, column_name);
    }

    Ok(())
}

fn apply_trigger_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let trigger_name_text = row.require_text("TRIGGER_NAME")?;
    let table_owner_text = row.require_text("TABLE_OWNER")?;
    let table_name_text = row.require_text("TABLE_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(trigger_name) = snapshot.intern_trigger_name(trigger_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TRIGGER_NAME"),
            expected: "interned trigger name",
            value: String::from(trigger_name_text),
        });
    };
    let Some(object_name) = snapshot.intern_object_name(trigger_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TRIGGER_NAME"),
            expected: "interned object name",
            value: String::from(trigger_name_text),
        });
    };
    let Some(target_owner) = snapshot.intern_schema_name(table_owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_OWNER"),
            expected: "interned schema name",
            value: String::from(table_owner_text),
        });
    };
    let Some(target_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    let common = schema_catalog
        .objects
        .get(&object_name)
        .and_then(|object| {
            if let CatalogObject::Trigger(metadata) = object {
                Some(metadata.common.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| ObjectCommon {
            owner,
            name: object_name,
            object_type: ObjectType::Trigger,
            ..ObjectCommon::default()
        });

    let metadata = TriggerMetadata {
        common,
        target_owner,
        target_name,
        timing: trigger_timing_from_dictionary_value(row.text("TRIGGER_TYPE").unwrap_or_default()),
        level: trigger_level_from_dictionary_value(row.text("TRIGGER_TYPE").unwrap_or_default()),
        events: trigger_events_from_dictionary_value(
            row.text("TRIGGERING_EVENT").unwrap_or_default(),
        ),
        when_clause: optional_nonblank_text(row, "WHEN_CLAUSE").map(String::from),
        body_hash: None,
    };

    schema_catalog
        .triggers
        .insert(trigger_name, metadata.clone());
    schema_catalog
        .objects
        .insert(object_name, CatalogObject::Trigger(metadata));

    Ok(())
}

fn apply_synonym_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let synonym_name_text = row.require_text("SYNONYM_NAME")?;
    let target_name_text = row.require_text("TABLE_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(synonym_name) = snapshot.intern_synonym_name(synonym_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("SYNONYM_NAME"),
            expected: "interned synonym name",
            value: String::from(synonym_name_text),
        });
    };
    let Some(target_name) = snapshot.intern_object_name(target_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(target_name_text),
        });
    };
    let target_owner = optional_nonblank_text(row, "TABLE_OWNER")
        .map(|value| {
            snapshot
                .intern_schema_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("TABLE_OWNER"),
                    expected: "interned schema name",
                    value: String::from(value),
                })
        })
        .transpose()?;

    snapshot.schemas.entry(owner).or_default().synonyms.insert(
        synonym_name,
        SynonymTarget {
            target_owner,
            target_name,
            target_type: None,
            db_link: optional_nonblank_text(row, "DB_LINK").map(String::from),
            public_synonym: owner_text.eq_ignore_ascii_case("PUBLIC"),
        },
    );

    Ok(())
}

fn apply_view_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let view_name_text = row.require_text("VIEW_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(view_name) = snapshot.intern_object_name(view_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("VIEW_NAME"),
            expected: "interned object name",
            value: String::from(view_name_text),
        });
    };

    let query_hash = optional_nonblank_text(row, "TEXT_VC").map(hash_text);
    let read_only = optional_bool(row, "READ_ONLY")?;

    let Some(schema_catalog) = snapshot.schemas.get_mut(&owner) else {
        return Ok(());
    };
    let Some(catalog_object) = schema_catalog.objects.get_mut(&view_name) else {
        return Ok(());
    };

    if let CatalogObject::View(metadata) = catalog_object {
        metadata.query_hash = query_hash;
        metadata.read_only = read_only;
    }

    Ok(())
}

fn apply_mview_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let mview_name_text = row.require_text("MVIEW_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(mview_name) = snapshot.intern_object_name(mview_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("MVIEW_NAME"),
            expected: "interned object name",
            value: String::from(mview_name_text),
        });
    };

    let refresh_mode = optional_nonblank_text(row, "REFRESH_MODE").map(String::from);
    let refresh_method = optional_nonblank_text(row, "REFRESH_METHOD").map(String::from);
    let query_hash = optional_nonblank_text(row, "QUERY").map(hash_text);

    let Some(schema_catalog) = snapshot.schemas.get_mut(&owner) else {
        return Ok(());
    };
    let Some(catalog_object) = schema_catalog.objects.get_mut(&mview_name) else {
        return Ok(());
    };

    if let CatalogObject::MaterializedView(metadata) = catalog_object {
        metadata.refresh_mode = refresh_mode;
        metadata.refresh_method = refresh_method;
        metadata.query_hash = query_hash;
    }

    Ok(())
}

fn apply_sequence_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("SEQUENCE_OWNER")?;
    let sequence_name_text = row.require_text("SEQUENCE_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("SEQUENCE_OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(sequence_name) = snapshot.intern_object_name(sequence_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("SEQUENCE_NAME"),
            expected: "interned object name",
            value: String::from(sequence_name_text),
        });
    };

    let increment_by = row.parse_i64("INCREMENT_BY").unwrap_or(1);
    let min_value = row.parse_i64("MIN_VALUE").ok();
    let max_value = row.parse_i64("MAX_VALUE").ok();
    let cycle = row
        .text("CYCLE_FLAG")
        .map(|value| value.eq_ignore_ascii_case("Y"))
        .unwrap_or(false);
    let ordered = row
        .text("ORDER_FLAG")
        .map(|value| value.eq_ignore_ascii_case("Y"))
        .unwrap_or(false);
    let cache_size = row.parse_u64("CACHE_SIZE").ok();

    let Some(schema_catalog) = snapshot.schemas.get_mut(&owner) else {
        return Ok(());
    };
    let Some(catalog_object) = schema_catalog.objects.get_mut(&sequence_name) else {
        return Ok(());
    };

    if let CatalogObject::Sequence(metadata) = catalog_object {
        metadata.increment_by = increment_by;
        metadata.min_value = min_value;
        metadata.max_value = max_value;
        metadata.cycle = cycle;
        metadata.ordered = ordered;
        metadata.cache_size = cache_size;
    }

    Ok(())
}

fn apply_type_attr_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let type_name_text = row.require_text("TYPE_NAME")?;
    let attr_name_text = row.require_text("ATTR_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(type_name) = snapshot.intern_object_name(type_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TYPE_NAME"),
            expected: "interned object name",
            value: String::from(type_name_text),
        });
    };
    let Some(attr_name) = snapshot.intern_member_name(attr_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("ATTR_NAME"),
            expected: "interned member name",
            value: String::from(attr_name_text),
        });
    };

    let attr_type_owner = optional_nonblank_text(row, "ATTR_TYPE_OWNER")
        .map(|value| {
            snapshot
                .intern_schema_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("ATTR_TYPE_OWNER"),
                    expected: "interned schema name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let attr_type_name = row
        .text("ATTR_TYPE_NAME")
        .map(String::from)
        .unwrap_or_default();

    let attribute = TypeAttribute {
        name: attr_name,
        position: required_u32(row, "ATTR_NO")?,
        data_type: DataTypeRef {
            owner: attr_type_owner,
            name: attr_type_name,
            length: optional_u32(row, "LENGTH")?,
            precision: optional_u32(row, "PRECISION")?,
            scale: optional_i32(row, "SCALE")?,
            char_semantics: None,
        },
    };

    let Some(schema_catalog) = snapshot.schemas.get_mut(&owner) else {
        return Ok(());
    };
    let Some(catalog_object) = schema_catalog.objects.get_mut(&type_name) else {
        return Ok(());
    };

    if let CatalogObject::Type(metadata) = catalog_object {
        match metadata
            .attributes
            .iter()
            .position(|existing| existing.position.eq(&attribute.position))
        {
            Some(index) => metadata.attributes[index] = attribute,
            None => metadata.attributes.push(attribute),
        }
        metadata
            .attributes
            .sort_by_key(|attribute| attribute.position);
    }

    Ok(())
}

/// Apply a single `ALL_DB_LINKS` row into the snapshot. Ensures the
/// owning schema entry exists (lazily creates it) so a `PUBLIC` row
/// lands even when no other catalog object has been recorded for that
/// synthetic schema.
fn apply_db_link_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let link_name_text = row.require_text("DB_LINK")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };

    let host = optional_nonblank_text(row, "HOST").map(String::from);
    let public_link = owner_text.eq_ignore_ascii_case("PUBLIC");

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    schema_catalog.db_links.push(DatabaseLink {
        owner,
        name: String::from(link_name_text),
        host,
        public_link,
    });

    Ok(())
}

/// Apply a single `ALL_POLICIES` row.
fn apply_vpd_policy_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let object_owner_text = row.require_text("OBJECT_OWNER")?;
    let object_name_text = row.require_text("OBJECT_NAME")?;
    let policy_name = row.require_text("POLICY_NAME")?.to_string();
    let function_owner_text = row.require_text("PF_OWNER")?;
    let function_name = row.require_text("FUNCTION")?.to_string();

    let Some(object_owner) = snapshot.intern_schema_name(object_owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OBJECT_OWNER"),
            expected: "interned schema name",
            value: String::from(object_owner_text),
        });
    };
    let Some(object_name) = snapshot.intern_object_name(object_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OBJECT_NAME"),
            expected: "interned object name",
            value: String::from(object_name_text),
        });
    };
    let Some(function_owner) = snapshot.intern_schema_name(function_owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("PF_OWNER"),
            expected: "interned schema name",
            value: String::from(function_owner_text),
        });
    };

    let policy_group = optional_nonblank_text(row, "POLICY_GROUP").map(String::from);
    let function_package = optional_nonblank_text(row, "PACKAGE").map(String::from);

    let yn = |col: &str| {
        row.text(col)
            .map(|v| v.eq_ignore_ascii_case("Y") || v.eq_ignore_ascii_case("YES"))
            .unwrap_or(false)
    };

    let schema_catalog = snapshot.schemas.entry(object_owner).or_default();
    schema_catalog.vpd_policies.push(VpdPolicy {
        object_owner,
        object_name,
        policy_group,
        policy_name,
        function_owner,
        function_package,
        function_name,
        on_select: yn("SEL"),
        on_insert: yn("INS"),
        on_update: yn("UPD"),
        on_delete: yn("DEL"),
        enabled: yn("ENABLE"),
    });
    Ok(())
}

/// Apply a single `ALL_EDITIONS` row.
fn apply_edition_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let edition_name = row.require_text("EDITION_NAME")?.to_string();
    let parent_edition_name = optional_nonblank_text(row, "PARENT_EDITION_NAME").map(String::from);
    let usable = row
        .text("USABLE")
        .map(|v| v.eq_ignore_ascii_case("Y"))
        .unwrap_or(true);
    snapshot.editions.push(Edition {
        edition_name,
        parent_edition_name,
        usable,
    });
    Ok(())
}

/// Apply a single `ALL_EDITIONING_VIEWS` row.
fn apply_editioning_view_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let view_name_text = row.require_text("VIEW_NAME")?;
    let table_name_text = row.require_text("TABLE_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(view_name) = snapshot.intern_object_name(view_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("VIEW_NAME"),
            expected: "interned object name",
            value: String::from(view_name_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    schema_catalog.editioning_views.push(EditioningView {
        owner,
        view_name,
        table_name,
    });
    Ok(())
}

/// Apply a single `ALL_TAB_COMMENTS` row into the snapshot.
fn apply_table_comment_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let table_name_text = row.require_text("TABLE_NAME")?;
    let table_type = row.text("TABLE_TYPE").map(String::from).unwrap_or_default();
    let comments = row.require_text("COMMENTS")?.to_string();

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    schema_catalog.table_comments.push(TableComment {
        owner,
        table_name,
        table_type,
        comments,
    });
    Ok(())
}

/// Apply a single `ALL_COL_COMMENTS` row into the snapshot.
fn apply_column_comment_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let table_name_text = row.require_text("TABLE_NAME")?;
    let column_name_text = row.require_text("COLUMN_NAME")?;
    let comments = row.require_text("COMMENTS")?.to_string();

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(table_name) = snapshot.intern_object_name(table_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(table_name_text),
        });
    };
    let Some(column_name) = snapshot.intern_column_name(column_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("COLUMN_NAME"),
            expected: "interned column name",
            value: String::from(column_name_text),
        });
    };

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    schema_catalog.column_comments.push(ColumnComment {
        owner,
        table_name,
        column_name,
        comments,
    });
    Ok(())
}

fn apply_grant_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let owner_text = row.require_text("TABLE_SCHEMA")?;
    let object_name_text = row.require_text("TABLE_NAME")?;
    let grantee_text = row.require_text("GRANTEE")?;
    let privilege_text = row.require_text("PRIVILEGE")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_SCHEMA"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(object_name) = snapshot.intern_object_name(object_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("TABLE_NAME"),
            expected: "interned object name",
            value: String::from(object_name_text),
        });
    };

    let grantee = grantee_from_dictionary_value(snapshot, grantee_text)?;
    let privilege = grant_privilege_from_dictionary_value(privilege_text);
    let grantable = row
        .text("GRANTABLE")
        .map(|value| value.eq_ignore_ascii_case("YES"))
        .unwrap_or(false);
    let with_hierarchy = row
        .text("HIERARCHY")
        .map(|value| value.eq_ignore_ascii_case("YES"))
        .unwrap_or(false);

    let grant = Grant {
        object_owner: owner,
        object_name,
        privilege,
        grantee,
        grantable,
        via_role: None,
        with_hierarchy,
    };

    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    if !schema_catalog
        .grants
        .iter()
        .any(|existing| existing.eq(&grant))
    {
        schema_catalog.grants.push(grant);
    }

    Ok(())
}

/// Classify an `ALL_TAB_PRIVS.GRANTEE` value into a [`Grantee`].
///
/// `ALL_TAB_PRIVS` carries no user/role discriminator column, so the
/// grantee universe — `{ user, role, PUBLIC }` — is resolved against the
/// `ALL_USERS`-derived [`CatalogSnapshot::known_users`] set:
///
/// * `PUBLIC` -> [`Grantee::Public`].
/// * known username -> [`Grantee::User`] (a statically certain direct grant).
/// * loaded set, name absent -> [`Grantee::Role`] (the only remaining class;
///   the resolver then caps it at Low confidence and emits a
///   `RuntimeGrantOrRole` ambiguity because a role grant only applies when
///   the role is enabled in `SESSION_ROLES` at runtime).
/// * username set NOT loaded (`known_users` is `None`) -> [`Grantee::Role`]
///   as well. This is the R13 fail-toward-restrictive choice: when the
///   grantee class is genuinely undetermined we must NOT default to a
///   high-confidence direct user grant (a fail-toward-permissive result in
///   a privilege/SAST product); treating it as a role routes it through the
///   runtime-ambiguity downgrade instead of over-claiming certainty.
fn grantee_from_dictionary_value(
    snapshot: &mut CatalogSnapshot,
    text: &str,
) -> Result<Grantee, CatalogError> {
    if text.eq_ignore_ascii_case("PUBLIC") {
        return Ok(Grantee::Public);
    }
    let Some(symbol) = snapshot.interner.intern(text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("GRANTEE"),
            expected: "interned grantee name",
            value: String::from(text),
        });
    };
    let is_known_user = snapshot
        .known_users
        .as_ref()
        .is_some_and(|users| users.contains(&UserName::from(symbol)));
    if is_known_user {
        Ok(Grantee::User(UserName::from(symbol)))
    } else {
        Ok(Grantee::Role(RoleName::from(symbol)))
    }
}

fn grant_privilege_from_dictionary_value(text: &str) -> GrantPrivilege {
    match text.to_ascii_uppercase().as_str() {
        "SELECT" => GrantPrivilege::Select,
        "INSERT" => GrantPrivilege::Insert,
        "UPDATE" => GrantPrivilege::Update,
        "DELETE" => GrantPrivilege::Delete,
        "EXECUTE" => GrantPrivilege::Execute,
        "ALTER" => GrantPrivilege::Alter,
        "INDEX" => GrantPrivilege::Index,
        "REFERENCES" => GrantPrivilege::References,
        "DEBUG" => GrantPrivilege::Debug,
        _ => GrantPrivilege::Other,
    }
}

fn apply_routine_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
    routines: &mut HashMap<RoutineLocator, RoutineAccumulator>,
) -> Result<(), CatalogError> {
    let locator = routine_locator_from_procedure_row(snapshot, row)?;
    let deterministic = optional_bool(row, "DETERMINISTIC")?.unwrap_or(false);
    let pipelined = optional_bool(row, "PIPELINED")?.unwrap_or(false);
    let kind_hint = routine_kind_from_dictionary_value(optional_nonblank_text(row, "OBJECT_TYPE"));

    let accumulator = routines.entry(locator).or_default();
    accumulator
        .signature
        .get_or_insert_with(|| RoutineSignature {
            routine_name: locator.routine_name,
            overload: locator.overload,
            ..RoutineSignature::default()
        });
    accumulator.kind_hint = kind_hint.or(accumulator.kind_hint);
    accumulator.deterministic = deterministic;
    accumulator.pipelined = pipelined;

    Ok(())
}

fn apply_argument_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
    routines: &mut HashMap<RoutineLocator, RoutineAccumulator>,
) -> Result<(), CatalogError> {
    let locator = routine_locator_from_argument_row(snapshot, row)?;
    let data_type = data_type_ref_from_argument_row(snapshot, row)?;
    let accumulator = routines.entry(locator).or_default();
    let signature = accumulator
        .signature
        .get_or_insert_with(|| RoutineSignature {
            routine_name: locator.routine_name,
            overload: locator.overload,
            ..RoutineSignature::default()
        });
    let position = required_u32(row, "POSITION")?;

    if position.eq(&0) {
        signature.return_type = Some(data_type);
        accumulator.kind_hint = Some(RoutineKind::Function);
        return Ok(());
    }

    signature.arguments.push(ArgumentMetadata {
        position,
        name: optional_nonblank_text(row, "ARGUMENT_NAME")
            .map(|value| {
                snapshot
                    .intern_member_name(value)
                    .ok_or(CatalogError::InvalidColumnValue {
                        column: String::from("ARGUMENT_NAME"),
                        expected: "interned member name",
                        value: String::from(value),
                    })
            })
            .transpose()?,
        mode: parameter_mode_from_dictionary_value(row.text("IN_OUT")),
        data_type,
        defaulted: optional_bool(row, "DEFAULTED")?.unwrap_or(false),
    });

    Ok(())
}

fn finalize_routines(
    snapshot: &mut CatalogSnapshot,
    routines: HashMap<RoutineLocator, RoutineAccumulator>,
) -> Result<(), CatalogError> {
    for (locator, accumulator) in routines {
        let Some(signature) = accumulator.signature else {
            continue;
        };
        let kind = accumulator
            .kind_hint
            .or_else(|| {
                if signature.return_type.is_some() {
                    Some(RoutineKind::Function)
                } else {
                    Some(RoutineKind::Procedure)
                }
            })
            .unwrap_or(RoutineKind::Procedure);

        if let Some(package_name) = locator.package_name {
            upsert_packaged_routine(snapshot, locator.owner, package_name, kind, signature)?;
        } else {
            upsert_top_level_routine(
                snapshot,
                locator.owner,
                locator.routine_name,
                kind,
                signature,
                accumulator.deterministic,
                accumulator.pipelined,
            )?;
        }
    }

    Ok(())
}

fn object_type_from_dictionary_value(text: &str) -> Option<ObjectType> {
    match text.trim().to_ascii_uppercase().as_str() {
        "TABLE" => Some(ObjectType::Table),
        "VIEW" => Some(ObjectType::View),
        "MATERIALIZED VIEW" => Some(ObjectType::MaterializedView),
        "SEQUENCE" => Some(ObjectType::Sequence),
        "TYPE" => Some(ObjectType::Type),
        "PACKAGE" => Some(ObjectType::Package),
        "PROCEDURE" => Some(ObjectType::Procedure),
        "FUNCTION" => Some(ObjectType::Function),
        "TRIGGER" => Some(ObjectType::Trigger),
        "EDITIONING VIEW" => Some(ObjectType::EditioningView),
        _ => None,
    }
}

fn object_status_from_dictionary_value(text: &str) -> ObjectStatus {
    match text.trim().to_ascii_uppercase().as_str() {
        "VALID" => ObjectStatus::Valid,
        "ENABLED" => ObjectStatus::Valid,
        "INVALID" => ObjectStatus::Invalid,
        "UNUSABLE" | "DISABLED" => ObjectStatus::Invalid,
        _ => ObjectStatus::NotApplicable,
    }
}

fn routine_kind_from_dictionary_value(text: Option<&str>) -> Option<RoutineKind> {
    match text.map(|value| value.trim().to_ascii_uppercase()) {
        Some(value) if value.eq("FUNCTION") => Some(RoutineKind::Function),
        Some(value) if value.eq("PROCEDURE") => Some(RoutineKind::Procedure),
        _ => None,
    }
}

fn constraint_type_from_dictionary_value(
    text: &str,
    search_condition: Option<&str>,
    has_columns: bool,
) -> ConstraintType {
    match text.trim().to_ascii_uppercase().as_str() {
        "P" => ConstraintType::PrimaryKey,
        "R" => ConstraintType::ForeignKey,
        "U" => ConstraintType::Unique,
        "F" => ConstraintType::Ref,
        "C" => {
            if has_columns
                && search_condition
                    .map(|condition| {
                        condition
                            .trim()
                            .to_ascii_uppercase()
                            .contains("IS NOT NULL")
                    })
                    .unwrap_or(false)
            {
                ConstraintType::NotNull
            } else {
                ConstraintType::Check
            }
        }
        _ => ConstraintType::Other,
    }
}

fn trigger_timing_from_dictionary_value(text: &str) -> TriggerTiming {
    let normalized = text.trim().to_ascii_uppercase();
    if normalized.contains("INSTEAD OF") {
        TriggerTiming::InsteadOf
    } else if normalized.contains("BEFORE") {
        TriggerTiming::Before
    } else if normalized.contains("AFTER") {
        TriggerTiming::After
    } else {
        TriggerTiming::Unknown
    }
}

fn trigger_level_from_dictionary_value(text: &str) -> TriggerLevel {
    let normalized = text.trim().to_ascii_uppercase();
    if normalized.contains("EACH ROW") {
        TriggerLevel::Row
    } else if normalized.contains("STATEMENT") {
        TriggerLevel::Statement
    } else {
        TriggerLevel::Unknown
    }
}

fn trigger_events_from_dictionary_value(text: &str) -> Vec<TriggerEvent> {
    let normalized = text.trim().to_ascii_uppercase();
    let mut events = Vec::<TriggerEvent>::new();

    if normalized.contains("INSERT") {
        events.push(TriggerEvent::Insert);
    }
    if normalized.contains("UPDATE") {
        events.push(TriggerEvent::Update);
    }
    if normalized.contains("DELETE") {
        events.push(TriggerEvent::Delete);
    }
    if normalized.contains("LOGON") {
        events.push(TriggerEvent::Logon);
    }
    if normalized.contains("LOGOFF") {
        events.push(TriggerEvent::Logoff);
    }
    if normalized.contains("DDL") {
        events.push(TriggerEvent::Ddl);
    }

    if events.is_empty() {
        events.push(TriggerEvent::Other);
    }

    events
}

fn push_unique_column(columns: &mut Vec<ColumnName>, column_name: ColumnName) {
    if !columns.contains(&column_name) {
        columns.push(column_name);
    }
}

fn routine_locator_from_procedure_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<RoutineLocator, CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let container_name_text = row.require_text("OBJECT_NAME")?;
    let routine_name_text = row
        .text("PROCEDURE_NAME")
        .unwrap_or(container_name_text)
        .trim();

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let Some(container_name) = snapshot.intern_object_name(container_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OBJECT_NAME"),
            expected: "interned object name",
            value: String::from(container_name_text),
        });
    };
    let Some(routine_name) = snapshot.intern_object_name(routine_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("PROCEDURE_NAME"),
            expected: "interned object name",
            value: String::from(routine_name_text),
        });
    };

    Ok(RoutineLocator {
        owner,
        package_name: if optional_nonblank_text(row, "PROCEDURE_NAME").is_some() {
            Some(container_name)
        } else {
            None
        },
        routine_name,
        subprogram_id: optional_u32(row, "SUBPROGRAM_ID")?,
        overload: optional_u32(row, "OVERLOAD")?,
    })
}

fn routine_locator_from_argument_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<RoutineLocator, CatalogError> {
    let owner_text = row.require_text("OWNER")?;
    let routine_name_text = row.require_text("OBJECT_NAME")?;

    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OWNER"),
            expected: "interned schema name",
            value: String::from(owner_text),
        });
    };
    let package_name = optional_nonblank_text(row, "PACKAGE_NAME")
        .map(|value| {
            snapshot
                .intern_object_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("PACKAGE_NAME"),
                    expected: "interned object name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let Some(routine_name) = snapshot.intern_object_name(routine_name_text) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("OBJECT_NAME"),
            expected: "interned object name",
            value: String::from(routine_name_text),
        });
    };

    Ok(RoutineLocator {
        owner,
        package_name,
        routine_name,
        subprogram_id: optional_u32(row, "SUBPROGRAM_ID")?,
        overload: optional_u32(row, "OVERLOAD")?,
    })
}

fn upsert_packaged_routine(
    snapshot: &mut CatalogSnapshot,
    owner: SchemaName,
    package_name: ObjectName,
    kind: RoutineKind,
    signature: RoutineSignature,
) -> Result<(), CatalogError> {
    let schema_catalog = snapshot.schemas.entry(owner).or_default();

    schema_catalog
        .objects
        .entry(package_name)
        .or_insert_with(|| {
            CatalogObject::Package(PackageMetadata {
                common: ObjectCommon {
                    owner,
                    name: package_name,
                    object_type: ObjectType::Package,
                    ..ObjectCommon::default()
                },
                ..PackageMetadata::default()
            })
        });

    let Some(CatalogObject::Package(metadata)) = schema_catalog.objects.get_mut(&package_name)
    else {
        return Ok(());
    };

    match kind {
        RoutineKind::Procedure => upsert_signature(&mut metadata.procedures, signature),
        RoutineKind::Function => upsert_signature(&mut metadata.functions, signature),
    }

    Ok(())
}

fn upsert_top_level_routine(
    snapshot: &mut CatalogSnapshot,
    owner: SchemaName,
    routine_name: ObjectName,
    kind: RoutineKind,
    signature: RoutineSignature,
    deterministic: bool,
    pipelined: bool,
) -> Result<(), CatalogError> {
    let schema_catalog = snapshot.schemas.entry(owner).or_default();
    let common = schema_catalog
        .objects
        .get(&routine_name)
        .and_then(|object| match object {
            CatalogObject::Procedure(metadata) => Some(metadata.common.clone()),
            CatalogObject::Function(metadata) => Some(metadata.common.clone()),
            _ => None,
        })
        .unwrap_or_else(|| ObjectCommon {
            owner,
            name: routine_name,
            object_type: match kind {
                RoutineKind::Procedure => ObjectType::Procedure,
                RoutineKind::Function => ObjectType::Function,
            },
            ..ObjectCommon::default()
        });

    let catalog_object = match kind {
        RoutineKind::Procedure => CatalogObject::Procedure(ProcedureMetadata { common, signature }),
        RoutineKind::Function => CatalogObject::Function(FunctionMetadata {
            common,
            signature,
            deterministic,
            pipelined,
        }),
    };
    schema_catalog.objects.insert(routine_name, catalog_object);

    Ok(())
}

fn upsert_signature(signatures: &mut Vec<RoutineSignature>, signature: RoutineSignature) {
    if let Some(existing) = signatures.iter_mut().find(|candidate| {
        candidate.routine_name.eq(&signature.routine_name)
            && candidate.overload.eq(&signature.overload)
    }) {
        *existing = signature;
    } else {
        signatures.push(signature);
    }
}

fn data_type_ref_from_argument_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<DataTypeRef, CatalogError> {
    let owner = optional_nonblank_text(row, "TYPE_OWNER")
        .map(|value| {
            snapshot
                .intern_schema_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("TYPE_OWNER"),
                    expected: "interned schema name",
                    value: String::from(value),
                })
        })
        .transpose()?;
    let type_name = optional_nonblank_text(row, "TYPE_NAME")
        .or_else(|| optional_nonblank_text(row, "DATA_TYPE"))
        .unwrap_or_default();

    Ok(DataTypeRef {
        owner,
        name: String::from(type_name),
        length: optional_u32(row, "DATA_LENGTH")?,
        precision: optional_u32(row, "DATA_PRECISION")?,
        scale: optional_i32(row, "DATA_SCALE")?,
        char_semantics: None,
    })
}

fn parameter_mode_from_dictionary_value(text: Option<&str>) -> ParameterMode {
    match text.map(|value| value.trim().to_ascii_uppercase()) {
        Some(value) if value.eq("OUT") => ParameterMode::Out,
        Some(value) if value.eq("IN/OUT") => ParameterMode::InOut,
        _ => ParameterMode::In,
    }
}

fn blank_catalog_object(common: ObjectCommon) -> Option<CatalogObject> {
    match common.object_type {
        ObjectType::Table => Some(CatalogObject::Table(TableMetadata {
            common,
            ..TableMetadata::default()
        })),
        ObjectType::View => Some(CatalogObject::View(ViewMetadata {
            common,
            ..ViewMetadata::default()
        })),
        ObjectType::MaterializedView => Some(CatalogObject::MaterializedView(MViewMetadata {
            common,
            ..MViewMetadata::default()
        })),
        ObjectType::Sequence => Some(CatalogObject::Sequence(SequenceMetadata {
            common,
            ..SequenceMetadata::default()
        })),
        ObjectType::Type => Some(CatalogObject::Type(TypeMetadata {
            common,
            ..TypeMetadata::default()
        })),
        ObjectType::Package => Some(CatalogObject::Package(PackageMetadata {
            common,
            ..PackageMetadata::default()
        })),
        ObjectType::Procedure => Some(CatalogObject::Procedure(ProcedureMetadata {
            common,
            ..ProcedureMetadata::default()
        })),
        ObjectType::Function => Some(CatalogObject::Function(FunctionMetadata {
            common,
            ..FunctionMetadata::default()
        })),
        ObjectType::Trigger => Some(CatalogObject::Trigger(TriggerMetadata {
            common,
            ..TriggerMetadata::default()
        })),
        ObjectType::SchedulerJob => Some(CatalogObject::SchedulerJob(SchedulerJobMetadata {
            common,
            ..SchedulerJobMetadata::default()
        })),
        ObjectType::EditioningView => Some(CatalogObject::EditioningView(EditioningViewMetadata {
            common,
            ..EditioningViewMetadata::default()
        })),
        ObjectType::Synonym | ObjectType::Index | ObjectType::Constraint | ObjectType::Unknown => {
            None
        }
    }
}

fn data_type_ref_from_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<DataTypeRef, CatalogError> {
    let owner = row
        .text("DATA_TYPE_OWNER")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            snapshot
                .intern_schema_name(value)
                .ok_or(CatalogError::InvalidColumnValue {
                    column: String::from("DATA_TYPE_OWNER"),
                    expected: "interned schema name",
                    value: String::from(value),
                })
        })
        .transpose()?;

    Ok(DataTypeRef {
        owner,
        name: String::from(row.require_text("DATA_TYPE")?),
        length: optional_u32(row, "DATA_LENGTH")?,
        precision: optional_u32(row, "DATA_PRECISION")?,
        scale: optional_i32(row, "DATA_SCALE")?,
        char_semantics: row.text("CHAR_USED").map(String::from),
    })
}

fn optional_bool(row: &OracleRow, column: &str) -> Result<Option<bool>, CatalogError> {
    match row.text(column) {
        Some(_) => row.parse_bool(column).map(Some),
        None => Ok(None),
    }
}

fn optional_nonblank_text<'a>(row: &'a OracleRow, column: &str) -> Option<&'a str> {
    row.text(column)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn optional_u32(row: &OracleRow, column: &str) -> Result<Option<u32>, CatalogError> {
    match row.text(column) {
        Some(_) => {
            let parsed = row.parse_u64(column)?;
            u32::try_from(parsed)
                .map(Some)
                .map_err(|_| CatalogError::InvalidColumnValue {
                    column: column.to_ascii_uppercase(),
                    expected: "u32",
                    value: parsed.to_string(),
                })
        }
        None => Ok(None),
    }
}

fn required_u32(row: &OracleRow, column: &str) -> Result<u32, CatalogError> {
    let parsed = row.parse_u64(column)?;
    u32::try_from(parsed).map_err(|_| CatalogError::InvalidColumnValue {
        column: column.to_ascii_uppercase(),
        expected: "u32",
        value: parsed.to_string(),
    })
}

fn optional_i32(row: &OracleRow, column: &str) -> Result<Option<i32>, CatalogError> {
    match row.text(column) {
        Some(_) => {
            let parsed = row.parse_i64(column)?;
            i32::try_from(parsed)
                .map(Some)
                .map_err(|_| CatalogError::InvalidColumnValue {
                    column: column.to_ascii_uppercase(),
                    expected: "i32",
                    value: parsed.to_string(),
                })
        }
        None => Ok(None),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SynonymTarget {
    pub target_owner: Option<SchemaName>,
    pub target_name: ObjectName,
    pub target_type: Option<ObjectType>,
    pub db_link: Option<String>,
    pub public_synonym: bool,
}

/// Virtual Private Database (VPD / RLS) policy entry from
/// `ALL_POLICIES`. Each row describes one policy attached to an object;
/// the policy function (PF_OWNER.PACKAGE.FUNCTION) is the predicate
/// generator that Oracle invokes at parse time to inject a WHERE clause
/// into reads (and optional ones into INSERT/UPDATE/DELETE).
///
/// Lineage flags VPD-protected objects with
/// `UnknownReason::DbLinkRemoteObject` reused-as-marker pending a
/// dedicated `UnknownReason::VpdPolicyApplied` variant.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct VpdPolicy {
    /// Owning schema (the object's owner).
    pub object_owner: SchemaName,
    /// Protected object.
    pub object_name: ObjectName,
    /// Optional policy-group name (Oracle policy-groups; usually NULL).
    pub policy_group: Option<String>,
    /// Policy name within the group.
    pub policy_name: String,
    /// Owner of the policy function.
    pub function_owner: SchemaName,
    /// Package containing the policy function (NULL for standalone).
    pub function_package: Option<String>,
    /// Function name that produces the WHERE-clause predicate.
    pub function_name: String,
    /// Statement-type bits — true means the policy applies to that DML.
    pub on_select: bool,
    pub on_insert: bool,
    pub on_update: bool,
    pub on_delete: bool,
    /// Whether the policy is currently enabled.
    pub enabled: bool,
}

/// Edition entry from `ALL_EDITIONS` — the per-database edition tree
/// used by Oracle Edition-Based Redefinition (EBR). Linked into
/// [`CatalogSnapshot::editions`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Edition {
    /// Edition name (Oracle identifiers are case-preserving but case-
    /// insensitive when unquoted; stored as the dictionary value).
    pub edition_name: String,
    /// Parent edition, if any. `None` for the root edition (typically
    /// `ORA$BASE`).
    pub parent_edition_name: Option<String>,
    /// Whether the edition is currently usable (`USABLE = 'Y'`).
    pub usable: bool,
}

/// Editioning view from `ALL_EDITIONING_VIEWS` — a view that masks an
/// editioned table during EBR cutovers. Linked into
/// [`SchemaCatalog::editioning_views`].
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditioningView {
    /// Owning schema.
    pub owner: SchemaName,
    /// Editioning view name.
    pub view_name: ObjectName,
    /// Base table the editioning view masks.
    pub table_name: ObjectName,
}

/// Documentation comment attached to a table, view, or materialized
/// view via `COMMENT ON TABLE owner.name IS '...'`. Sourced from
/// `ALL_TAB_COMMENTS`.
///
/// `plsql-docgen` consumes these to render description text alongside
/// object docs; dependency analysis does not interact with them.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableComment {
    /// Owning schema.
    pub owner: SchemaName,
    /// Object name.
    pub table_name: ObjectName,
    /// `TABLE` / `VIEW` / `MATERIALIZED VIEW` — preserved verbatim
    /// from `ALL_TAB_COMMENTS.TABLE_TYPE` so docgen can pick a
    /// per-kind template.
    pub table_type: String,
    /// Comment text. Always present (we filter out NULL rows server-side
    /// to keep the snapshot compact).
    pub comments: String,
}

/// Documentation comment attached to a column via
/// `COMMENT ON COLUMN owner.table.column IS '...'`. Sourced from
/// `ALL_COL_COMMENTS`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ColumnComment {
    /// Owning schema.
    pub owner: SchemaName,
    /// Object name.
    pub table_name: ObjectName,
    /// Column name.
    pub column_name: ColumnName,
    /// Comment text.
    pub comments: String,
}

/// Database link metadata sourced from `ALL_DB_LINKS`.
///
/// PL/SQL code that references `remote_object@my_link` resolves through
/// one of these entries. Public links have `owner = PUBLIC`. Lineage uses
/// the `host` field to classify the remote endpoint (a TNS alias, a full
/// EZCONNECT string, or the legacy `(DESCRIPTION=...)` form).
///
/// The shape intentionally avoids `username` — `ALL_DB_LINKS.USERNAME`
/// is the *connect user*, not a privilege grant, and most consumers
/// don't need it. If a future product surface needs the connect user it
/// can be added behind `#[serde(default)]` without breaking older
/// snapshots.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatabaseLink {
    /// Owning schema. `PUBLIC` for public links.
    pub owner: SchemaName,
    /// Link name, e.g. `REPORTING.WORLD`.
    pub name: String,
    /// Connect-target host string. Can be a TNS alias, an EZCONNECT
    /// string, or a full `(DESCRIPTION=...)` block.
    pub host: Option<String>,
    /// `true` when `owner` is `PUBLIC`. Surfaced for fast filtering in
    /// downstream lineage without re-resolving the schema interner.
    pub public_link: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum GrantPrivilege {
    Select,
    Insert,
    Update,
    Delete,
    Execute,
    Alter,
    Index,
    References,
    Debug,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Grantee {
    User(UserName),
    Role(RoleName),
    #[default]
    Public,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Grant {
    pub object_owner: SchemaName,
    pub object_name: ObjectName,
    pub privilege: GrantPrivilege,
    pub grantee: Grantee,
    pub grantable: bool,
    pub via_role: Option<RoleName>,
    pub with_hierarchy: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub name: IndexName,
    pub table_owner: SchemaName,
    pub table_name: ObjectName,
    pub unique: bool,
    pub columns: Vec<ColumnName>,
    pub index_type: String,
    pub status: ObjectStatus,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    NotNull,
    Ref,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConstraintMetadata {
    pub name: ConstraintName,
    pub table_owner: SchemaName,
    pub table_name: ObjectName,
    pub constraint_type: ConstraintType,
    pub columns: Vec<ColumnName>,
    pub referenced_table_owner: Option<SchemaName>,
    pub referenced_table_name: Option<ObjectName>,
    pub referenced_columns: Vec<ColumnName>,
    pub search_condition: Option<String>,
    pub deferrable: Option<bool>,
    pub initially_deferred: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogDependencyKind {
    Hard,
    Reference,
    Extended,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogDependency {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: ObjectType,
    pub referenced_owner: Option<SchemaName>,
    pub referenced_name: ObjectName,
    pub referenced_type: Option<ObjectType>,
    pub dependency_kind: CatalogDependencyKind,
    pub via_db_link: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlScopeAvailability {
    #[default]
    NotAvailable,
    AvailableButStale,
    IdentifiersOnly,
    IdentifiersAndStatements,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilerIdentifier {
    pub owner: SchemaName,
    pub object_name: ObjectName,
    pub identifier_name: MemberName,
    pub identifier_type: String,
    pub usage: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilerReference {
    pub owner: SchemaName,
    pub object_name: ObjectName,
    pub usage_line: u32,
    pub usage_column: u32,
    pub target_owner: Option<SchemaName>,
    pub target_object_name: Option<ObjectName>,
    pub target_identifier_name: Option<MemberName>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilerStatementUsage {
    pub owner: SchemaName,
    pub object_name: ObjectName,
    pub statement_kind: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlScopeSnapshot {
    pub availability: PlScopeAvailability,
    pub identifiers: Vec<CompilerIdentifier>,
    pub references: Vec<CompilerReference>,
    pub statements: Vec<CompilerStatementUsage>,
    pub collected_at: Option<DateTime<Utc>>,
    pub source_hash: Option<Hash>,
    pub warnings: Vec<CapabilityWarning>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataTypeRef {
    pub owner: Option<SchemaName>,
    pub name: String,
    pub length: Option<u32>,
    pub precision: Option<u32>,
    pub scale: Option<i32>,
    pub char_semantics: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ColumnMetadata {
    pub name: ColumnName,
    pub position: u32,
    pub data_type: DataTypeRef,
    pub nullable: bool,
    pub default_expression: Option<String>,
    pub generated_expression: Option<String>,
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TemporaryTableDuration {
    #[default]
    Transaction,
    Session,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableMetadata {
    pub common: ObjectCommon,
    pub columns: HashMap<ColumnName, ColumnMetadata>,
    pub temporary: bool,
    pub temporary_duration: Option<TemporaryTableDuration>,
    pub index_organized: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewMetadata {
    pub common: ObjectCommon,
    pub columns: HashMap<ColumnName, ColumnMetadata>,
    pub query_hash: Option<Hash>,
    pub read_only: Option<bool>,
    pub check_option: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MViewMetadata {
    pub common: ObjectCommon,
    pub columns: HashMap<ColumnName, ColumnMetadata>,
    pub refresh_mode: Option<String>,
    pub refresh_method: Option<String>,
    pub query_hash: Option<Hash>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SequenceMetadata {
    pub common: ObjectCommon,
    pub increment_by: i64,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
    pub cycle: bool,
    pub ordered: bool,
    pub cache_size: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ParameterMode {
    #[default]
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArgumentMetadata {
    pub position: u32,
    pub name: Option<MemberName>,
    pub mode: ParameterMode,
    pub data_type: DataTypeRef,
    pub defaulted: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccessibleByTarget {
    pub owner: Option<SchemaName>,
    pub object_name: ObjectName,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutineSignature {
    pub routine_name: ObjectName,
    pub overload: Option<u32>,
    pub arguments: Vec<ArgumentMetadata>,
    pub return_type: Option<DataTypeRef>,
    pub authid_current_user: Option<bool>,
    pub accessible_by: Vec<AccessibleByTarget>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeFinality {
    Final,
    NotFinal,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeInstantiable {
    Instantiable,
    NotInstantiable,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TypeAttribute {
    pub name: MemberName,
    pub position: u32,
    pub data_type: DataTypeRef,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TypeMetadata {
    pub common: ObjectCommon,
    pub attributes: Vec<TypeAttribute>,
    pub methods: Vec<RoutineSignature>,
    pub supertype_owner: Option<SchemaName>,
    pub supertype_name: Option<ObjectName>,
    pub finality: TypeFinality,
    pub instantiable: TypeInstantiable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub common: ObjectCommon,
    pub procedures: Vec<RoutineSignature>,
    pub functions: Vec<RoutineSignature>,
    pub package_stateful: Option<bool>,
    pub authid_current_user: Option<bool>,
    pub accessible_by: Vec<AccessibleByTarget>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcedureMetadata {
    pub common: ObjectCommon,
    pub signature: RoutineSignature,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FunctionMetadata {
    pub common: ObjectCommon,
    pub signature: RoutineSignature,
    pub deterministic: bool,
    pub pipelined: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TriggerLevel {
    Statement,
    Row,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Logon,
    Logoff,
    Ddl,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TriggerMetadata {
    pub common: ObjectCommon,
    pub target_owner: SchemaName,
    pub target_name: ObjectName,
    pub timing: TriggerTiming,
    pub level: TriggerLevel,
    pub events: Vec<TriggerEvent>,
    pub when_clause: Option<String>,
    pub body_hash: Option<Hash>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchedulerJobMetadata {
    pub common: ObjectCommon,
    pub enabled: bool,
    pub job_type: String,
    pub program_name: Option<ObjectName>,
    pub schedule_name: Option<ObjectName>,
    pub job_action: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditioningViewMetadata {
    pub common: ObjectCommon,
    pub base_table_owner: SchemaName,
    pub base_table_name: ObjectName,
    pub columns: HashMap<ColumnName, ColumnMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogObject {
    Table(TableMetadata),
    View(ViewMetadata),
    MaterializedView(MViewMetadata),
    Sequence(SequenceMetadata),
    Type(TypeMetadata),
    Package(PackageMetadata),
    Procedure(ProcedureMetadata),
    Function(FunctionMetadata),
    Trigger(TriggerMetadata),
    SchedulerJob(SchedulerJobMetadata),
    EditioningView(EditioningViewMetadata),
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::future::Future;

    use asupersync::{Cx, runtime::RuntimeBuilder};
    use chrono::{DateTime, Utc};
    use plsql_core::{AnalysisProfile, ColumnName, MemberName, ObjectName, SchemaName, SymbolId};
    use tempfile::tempdir;

    use crate::{
        AccessibleByTarget, CATALOG_SNAPSHOT_SCHEMA_ID, CATALOG_SNAPSHOT_SCHEMA_VERSION,
        CapabilityWarning, CatalogCapabilities, CatalogDependencyKind, CatalogError,
        CatalogLoadRequest, CatalogObject, CatalogSchemaFilter, CatalogSnapshot,
        CatalogSnapshotDocument, CatalogSource, CatalogSourceKind, CompilerIdentifier,
        ConstraintType, DataTypeRef, Hash, ObjectCommon, ObjectStatus, ObjectType, OracleBackend,
        OracleBind, OracleConnectOptions, OracleConnection, OracleConnectionInfo, OracleRow,
        PackageMetadata, PlScopeAvailability, PlScopeSnapshot, RoutineSignature, SchemaCatalog,
        SynonymName, SynonymTarget, TableMetadata, TriggerEvent, TriggerLevel, TriggerName,
        TriggerTiming, TypeFinality, TypeInstantiable, export_snapshot_to_json,
        grantee_from_dictionary_value, load_catalog_users, load_from_dbms_metadata_dir,
        load_snapshot_from_connection, load_snapshot_from_json, negotiate_capabilities,
        populate_dbms_metadata_ddl,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct QueryExpectation {
        sql_contains: String,
        params: Vec<OracleBind>,
        rows: Vec<OracleRow>,
    }

    #[derive(Clone, Debug)]
    struct StaticConnection {
        rows: Vec<OracleRow>,
        row_count: u64,
        expected_queries: Vec<QueryExpectation>,
        connection_info: OracleConnectionInfo,
    }

    impl Default for StaticConnection {
        fn default() -> Self {
            Self {
                rows: Vec::new(),
                row_count: 0,
                expected_queries: Vec::new(),
                connection_info: OracleConnectionInfo {
                    backend: OracleBackend::RustOracle,
                    connect_string: String::from("//localhost/XE"),
                    current_schema: Some(String::from("BILLING")),
                    server_version: String::from("23.0.0.0.0"),
                    db_name: String::from("XE"),
                    db_domain: String::new(),
                    service_name: String::from("XE"),
                    instance_name: String::from("xe"),
                    server_type: String::from("Dedicated"),
                    max_identifier_length: 128,
                    max_open_cursors: 500,
                },
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl OracleConnection for StaticConnection {
        fn backend(&self) -> OracleBackend {
            OracleBackend::RustOracle
        }

        async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
            let _ = cx;
            Ok(())
        }

        async fn describe(&self, cx: &Cx) -> Result<OracleConnectionInfo, CatalogError> {
            let _ = cx;
            Ok(self.connection_info.clone())
        }

        async fn query_rows(
            &self,
            cx: &Cx,
            sql: &str,
            params: &[OracleBind],
        ) -> Result<Vec<OracleRow>, CatalogError> {
            let _ = cx;
            if !self.expected_queries.is_empty() {
                if let Some(expectation) = self.expected_queries.iter().find(|expectation| {
                    sql.contains(expectation.sql_contains.as_str())
                        && params.eq(expectation.params.as_slice())
                }) {
                    return Ok(expectation.rows.clone());
                }

                return Err(CatalogError::OracleBackendError {
                    backend: OracleBackend::RustOracle,
                    message: format!("unexpected query `{sql}` with params {params:?}"),
                });
            }

            Ok(self.rows.clone())
        }

        async fn execute(
            &self,
            cx: &Cx,
            _sql: &str,
            _params: &[OracleBind],
        ) -> Result<u64, CatalogError> {
            let _ = cx;
            Ok(self.row_count)
        }
    }

    fn run_catalog_future<F: Future>(future: F) -> F::Output {
        RuntimeBuilder::current_thread()
            .build()
            .expect("test asupersync runtime")
            .block_on(future)
    }

    fn test_cx() -> Cx {
        Cx::current().expect("test runtime installs a request Cx")
    }

    fn load_snapshot_for_test<C: OracleConnection>(
        connection: &C,
        request: &CatalogLoadRequest,
    ) -> Result<CatalogSnapshot, CatalogError> {
        run_catalog_future(async {
            let cx = test_cx();
            load_snapshot_from_connection(&cx, connection, request).await
        })
    }

    fn load_catalog_users_for_test<C: OracleConnection>(
        connection: &C,
        snapshot: &mut CatalogSnapshot,
    ) -> Result<(), CatalogError> {
        run_catalog_future(async {
            let cx = test_cx();
            load_catalog_users(&cx, connection, snapshot).await
        })
    }

    fn populate_dbms_metadata_ddl_for_test<C: OracleConnection>(
        connection: &C,
        snapshot: &mut CatalogSnapshot,
    ) -> Result<(), CatalogError> {
        run_catalog_future(async {
            let cx = test_cx();
            populate_dbms_metadata_ddl(&cx, connection, snapshot).await
        })
    }

    fn negotiate_capabilities_for_test<C: OracleConnection>(connection: &C) -> CatalogCapabilities {
        run_catalog_future(async {
            let cx = test_cx();
            negotiate_capabilities(&cx, connection).await
        })
    }

    fn oracle_row(columns: &[(&str, &str, Option<&str>)]) -> OracleRow {
        let mut row = OracleRow::default();
        for (name, oracle_type, value) in columns {
            row.insert(*name, *oracle_type, value.map(String::from));
        }
        row
    }

    /// Probe queries issued by `negotiate_capabilities`. Every mock test
    /// that wants the live-extraction loader to behave normally should
    /// prepend these to its `expected_queries` so each probe succeeds.
    fn capability_probe_expectations() -> Vec<QueryExpectation> {
        [
            "from all_objects where rownum = 0",
            "from dba_objects where rownum = 0",
            "from all_source where rownum = 0",
            "from all_scheduler_jobs where rownum = 0",
            "from all_tab_privs where rownum = 0",
            "from all_plsql_object_settings where rownum = 0",
        ]
        .into_iter()
        .map(|fragment| QueryExpectation {
            sql_contains: String::from(fragment),
            params: vec![],
            rows: vec![],
        })
        .collect()
    }

    /// `ALL_USERS` extraction expectation. The live loader queries this
    /// (database-wide, no schema bind) to learn which grantees are users so
    /// `grantee_from_dictionary_value` can tell users from roles. Tests pass
    /// the set of usernames they want to be classified as `Grantee::User`;
    /// any grantee absent from this set is classified as `Grantee::Role`.
    fn all_users_expectation(usernames: &[&str]) -> QueryExpectation {
        QueryExpectation {
            sql_contains: String::from("from all_users"),
            params: vec![],
            rows: usernames
                .iter()
                .map(|name| oracle_row(&[("USERNAME", "VARCHAR2(128)", Some(name))]))
                .collect(),
        }
    }

    #[test]
    fn oracle_row_helpers_are_case_insensitive_and_typed() {
        let mut row = OracleRow::default();
        row.insert(
            "current_schema",
            "VARCHAR2(128)",
            Some(String::from("billing")),
        );
        row.insert("object_id", "NUMBER(10)", Some(String::from("42")));
        row.insert("editionable", "VARCHAR2(1)", Some(String::from("Y")));
        row.insert("ddl_text", "CLOB", None);

        assert_eq!(row.text("CURRENT_SCHEMA"), Some("billing"));
        assert_eq!(row.parse_u64("object_id").ok(), Some(42));
        assert_eq!(row.parse_bool("EDITIONABLE").ok(), Some(true));
        assert!(matches!(
            row.require_text("ddl_text"),
            Err(CatalogError::NullColumnValue { column }) if column.eq("DDL_TEXT")
        ));
    }

    #[test]
    fn parse_bool_honors_oracle_conventions_and_errors() {
        // The whole catalog's boolean flags (status/editionable/
        // deterministic/…) flow through parse_bool. Lock the Oracle
        // convention: Y/YES/TRUE/1 -> true, N/NO/FALSE/0 -> false,
        // case-insensitive, whitespace-trimmed; anything else is an
        // explicit InvalidColumnValue (R13 — never a silent default).
        for t in ["Y", "y", "YES", " yes ", "TRUE", "true", "1"] {
            let row = oracle_row(&[("FLAG", "VARCHAR2(3)", Some(t))]);
            assert_eq!(row.parse_bool("flag").ok(), Some(true), "{t:?} -> true");
        }
        for f in ["N", "n", "NO", " no ", "FALSE", "false", "0"] {
            let row = oracle_row(&[("FLAG", "VARCHAR2(3)", Some(f))]);
            assert_eq!(row.parse_bool("flag").ok(), Some(false), "{f:?} -> false");
        }
        // Unrecognized value -> explicit error, not a silent bool.
        let bad = oracle_row(&[("FLAG", "VARCHAR2(5)", Some("MAYBE"))]);
        assert!(matches!(
            bad.parse_bool("FLAG"),
            Err(CatalogError::InvalidColumnValue {
                column,
                expected: "bool",
                ..
            }) if column.eq("FLAG")
        ));
        // Missing column and NULL value are distinct typed errors.
        let empty = oracle_row(&[]);
        assert!(matches!(
            empty.parse_bool("FLAG"),
            Err(CatalogError::MissingColumn { column }) if column.eq("FLAG")
        ));
        let nullv = oracle_row(&[("FLAG", "VARCHAR2(1)", None)]);
        assert!(matches!(
            nullv.parse_bool("FLAG"),
            Err(CatalogError::NullColumnValue { column }) if column.eq("FLAG")
        ));

        // parse_u64 must reject a negative value (Oracle NUMBER can
        // be signed) rather than wrap/panic.
        let neg = oracle_row(&[("N", "NUMBER", Some("-1"))]);
        assert!(matches!(
            neg.parse_u64("N"),
            Err(CatalogError::InvalidColumnValue {
                expected: "u64",
                ..
            })
        ));
        let i = oracle_row(&[("N", "NUMBER", Some("-42"))]);
        assert_eq!(i.parse_i64("N").ok(), Some(-42));
    }

    #[test]
    fn catalog_load_request_defaults_to_current_schema() {
        let request = CatalogLoadRequest::default();
        assert_eq!(
            request.schema_filters,
            vec![CatalogSchemaFilter::CurrentSchema]
        );
    }

    #[test]
    fn load_snapshot_from_connection_extracts_structural_metadata() {
        let object_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("PACKAGE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CALCULATE_TAX")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("FUNCTION")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CUSTOMERS")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TABLE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TABLE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICE_SUMMARY")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("VIEW")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES_BIU")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TRIGGER")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
        ];
        let column_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("CUSTOMERS")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                ("DATA_TYPE_OWNER", "VARCHAR2(128)", None),
                ("DATA_TYPE", "VARCHAR2(128)", Some("NUMBER")),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("CHAR_USED", "VARCHAR2(1)", None),
                ("NULLABLE", "VARCHAR2(1)", Some("N")),
                ("DATA_DEFAULT_VC", "VARCHAR2(4000)", None),
                ("VIRTUAL_COLUMN", "VARCHAR2(3)", Some("NO")),
                ("HIDDEN_COLUMN", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                ("DATA_TYPE_OWNER", "VARCHAR2(128)", None),
                ("DATA_TYPE", "VARCHAR2(128)", Some("NUMBER")),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("CHAR_USED", "VARCHAR2(1)", None),
                ("NULLABLE", "VARCHAR2(1)", Some("N")),
                ("DATA_DEFAULT_VC", "VARCHAR2(4000)", None),
                ("VIRTUAL_COLUMN", "VARCHAR2(3)", Some("NO")),
                ("HIDDEN_COLUMN", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("2")),
                ("DATA_TYPE_OWNER", "VARCHAR2(128)", None),
                ("DATA_TYPE", "VARCHAR2(128)", Some("NUMBER")),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("CHAR_USED", "VARCHAR2(1)", None),
                ("NULLABLE", "VARCHAR2(1)", Some("N")),
                ("DATA_DEFAULT_VC", "VARCHAR2(4000)", None),
                ("VIRTUAL_COLUMN", "VARCHAR2(3)", Some("NO")),
                ("HIDDEN_COLUMN", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICE_SUMMARY")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("TOTAL_DUE")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                ("DATA_TYPE_OWNER", "VARCHAR2(128)", None),
                ("DATA_TYPE", "VARCHAR2(128)", Some("NUMBER")),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("12")),
                ("DATA_SCALE", "NUMBER(10)", Some("2")),
                ("CHAR_USED", "VARCHAR2(1)", None),
                ("NULLABLE", "VARCHAR2(1)", Some("Y")),
                ("DATA_DEFAULT_VC", "VARCHAR2(4000)", Some("0")),
                ("VIRTUAL_COLUMN", "VARCHAR2(3)", Some("YES")),
                ("HIDDEN_COLUMN", "VARCHAR2(3)", Some("NO")),
            ]),
        ];
        let constraint_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                (
                    "CONSTRAINT_NAME",
                    "VARCHAR2(128)",
                    Some("INVOICES_CUSTOMER_FK"),
                ),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("CONSTRAINT_TYPE", "VARCHAR2(1)", Some("R")),
                ("REFERENCED_TABLE_OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("REFERENCED_TABLE_NAME", "VARCHAR2(128)", Some("CUSTOMERS")),
                ("SEARCH_CONDITION_VC", "VARCHAR2(4000)", None),
                ("IS_DEFERRABLE", "VARCHAR2(1)", Some("N")),
                ("IS_DEFERRED", "VARCHAR2(1)", Some("N")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                (
                    "REFERENCED_COLUMN_NAME",
                    "VARCHAR2(128)",
                    Some("CUSTOMER_ID"),
                ),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("CONSTRAINT_NAME", "VARCHAR2(128)", Some("INVOICES_PK")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("CONSTRAINT_TYPE", "VARCHAR2(1)", Some("P")),
                ("REFERENCED_TABLE_OWNER", "VARCHAR2(128)", None),
                ("REFERENCED_TABLE_NAME", "VARCHAR2(128)", None),
                ("SEARCH_CONDITION_VC", "VARCHAR2(4000)", None),
                ("IS_DEFERRABLE", "VARCHAR2(1)", Some("N")),
                ("IS_DEFERRED", "VARCHAR2(1)", Some("N")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                ("REFERENCED_COLUMN_NAME", "VARCHAR2(128)", None),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                (
                    "CONSTRAINT_NAME",
                    "VARCHAR2(128)",
                    Some("INVOICES_CUSTOMER_NN"),
                ),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("CONSTRAINT_TYPE", "VARCHAR2(1)", Some("C")),
                ("REFERENCED_TABLE_OWNER", "VARCHAR2(128)", None),
                ("REFERENCED_TABLE_NAME", "VARCHAR2(128)", None),
                (
                    "SEARCH_CONDITION_VC",
                    "VARCHAR2(4000)",
                    Some("\"CUSTOMER_ID\" IS NOT NULL"),
                ),
                ("IS_DEFERRABLE", "VARCHAR2(1)", Some("N")),
                ("IS_DEFERRED", "VARCHAR2(1)", Some("N")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
                ("REFERENCED_COLUMN_NAME", "VARCHAR2(128)", None),
            ]),
        ];
        let index_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("INDEX_NAME", "VARCHAR2(128)", Some("INVOICES_PK_IDX")),
            ("TABLE_OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
            ("IS_UNIQUE", "VARCHAR2(1)", Some("Y")),
            ("INDEX_TYPE", "VARCHAR2(27)", Some("NORMAL")),
            ("STATUS", "VARCHAR2(8)", Some("VALID")),
            ("COLUMN_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
            ("COLUMN_POSITION", "NUMBER(10)", Some("1")),
        ])];
        let trigger_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("TRIGGER_NAME", "VARCHAR2(128)", Some("INVOICES_BIU")),
            ("TABLE_OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
            ("TRIGGER_TYPE", "VARCHAR2(80)", Some("BEFORE EACH ROW")),
            (
                "TRIGGERING_EVENT",
                "VARCHAR2(246)",
                Some("INSERT OR UPDATE"),
            ),
            ("WHEN_CLAUSE", "VARCHAR2(4000)", Some(":new.total_due >= 0")),
        ])];
        let synonym_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("SYNONYM_NAME", "VARCHAR2(128)", Some("LATEST_INVOICE")),
                ("TABLE_OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("DB_LINK", "VARCHAR2(128)", None),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("PUBLIC")),
                ("SYNONYM_NAME", "VARCHAR2(128)", Some("INVOICE_PUBLIC")),
                ("TABLE_OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("DB_LINK", "VARCHAR2(128)", None),
            ]),
        ];
        let procedure_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("PROCEDURE_NAME", "VARCHAR2(128)", Some("CREATE_INVOICE")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("OBJECT_TYPE", "VARCHAR2(13)", Some("PROCEDURE")),
                ("DETERMINISTIC", "VARCHAR2(3)", Some("NO")),
                ("PIPELINED", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                (
                    "PROCEDURE_NAME",
                    "VARCHAR2(128)",
                    Some("TOTAL_FOR_CUSTOMER"),
                ),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("2")),
                ("OVERLOAD", "VARCHAR2(40)", Some("1")),
                ("OBJECT_TYPE", "VARCHAR2(13)", Some("FUNCTION")),
                ("DETERMINISTIC", "VARCHAR2(3)", Some("NO")),
                ("PIPELINED", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CALCULATE_TAX")),
                ("PROCEDURE_NAME", "VARCHAR2(128)", None),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("OBJECT_TYPE", "VARCHAR2(13)", Some("FUNCTION")),
                ("DETERMINISTIC", "VARCHAR2(3)", Some("YES")),
                ("PIPELINED", "VARCHAR2(3)", Some("NO")),
            ]),
        ];
        let argument_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CREATE_INVOICE")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("ARGUMENT_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("POSITION", "NUMBER(10)", Some("1")),
                ("SEQUENCE", "NUMBER(10)", Some("1")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("IN_OUT", "VARCHAR2(9)", Some("IN")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CREATE_INVOICE")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("ARGUMENT_NAME", "VARCHAR2(128)", Some("TOTAL_DUE")),
                ("POSITION", "NUMBER(10)", Some("2")),
                ("SEQUENCE", "NUMBER(10)", Some("2")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("12")),
                ("DATA_SCALE", "NUMBER(10)", Some("2")),
                ("IN_OUT", "VARCHAR2(9)", Some("IN")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("TOTAL_FOR_CUSTOMER")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("2")),
                ("OVERLOAD", "VARCHAR2(40)", Some("1")),
                ("ARGUMENT_NAME", "VARCHAR2(128)", None),
                ("POSITION", "NUMBER(10)", Some("0")),
                ("SEQUENCE", "NUMBER(10)", Some("1")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("12")),
                ("DATA_SCALE", "NUMBER(10)", Some("2")),
                ("IN_OUT", "VARCHAR2(9)", Some("OUT")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", Some("BILLING_API")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("TOTAL_FOR_CUSTOMER")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("2")),
                ("OVERLOAD", "VARCHAR2(40)", Some("1")),
                ("ARGUMENT_NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("POSITION", "NUMBER(10)", Some("1")),
                ("SEQUENCE", "NUMBER(10)", Some("2")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("IN_OUT", "VARCHAR2(9)", Some("IN")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", None),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CALCULATE_TAX")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("ARGUMENT_NAME", "VARCHAR2(128)", None),
                ("POSITION", "NUMBER(10)", Some("0")),
                ("SEQUENCE", "NUMBER(10)", Some("1")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("2")),
                ("IN_OUT", "VARCHAR2(9)", Some("OUT")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("PACKAGE_NAME", "VARCHAR2(128)", None),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CALCULATE_TAX")),
                ("SUBPROGRAM_ID", "NUMBER(10)", Some("1")),
                ("OVERLOAD", "VARCHAR2(40)", None),
                ("ARGUMENT_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
                ("POSITION", "NUMBER(10)", Some("1")),
                ("SEQUENCE", "NUMBER(10)", Some("2")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("TYPE_OWNER", "VARCHAR2(128)", None),
                ("TYPE_NAME", "VARCHAR2(128)", None),
                ("DATA_LENGTH", "NUMBER(10)", Some("22")),
                ("DATA_PRECISION", "NUMBER(10)", Some("10")),
                ("DATA_SCALE", "NUMBER(10)", Some("0")),
                ("IN_OUT", "VARCHAR2(9)", Some("IN")),
                ("DEFAULTED", "VARCHAR2(1)", Some("N")),
            ]),
        ];
        let mut expected_queries = capability_probe_expectations();
        // REPORTING is a database user, so its object grant stays a direct
        // (high-confidence) Grantee::User after the role-classification fix.
        expected_queries.push(all_users_expectation(&["BILLING", "REPORTING"]));
        expected_queries.extend(vec![
                QueryExpectation {
                    sql_contains: String::from("from all_objects"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: object_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_tab_cols"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: column_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_constraints c"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: constraint_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_indexes i"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: index_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_triggers"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: trigger_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_synonyms"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: synonym_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_procedures"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: procedure_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_arguments"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: argument_rows,
                },
                QueryExpectation {
                    sql_contains: String::from("from all_views"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![oracle_row(&[
                        ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                        ("VIEW_NAME", "VARCHAR2(128)", Some("INVOICE_SUMMARY")),
                        (
                            "TEXT_VC",
                            "VARCHAR2(4000)",
                            Some(
                                "select invoice_id, sum(amount) total_due from invoices group by invoice_id",
                            ),
                        ),
                        ("READ_ONLY", "VARCHAR2(1)", Some("N")),
                    ])],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_mviews"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_sequences"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_type_attrs"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_tab_privs"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![oracle_row(&[
                        ("TABLE_SCHEMA", "VARCHAR2(128)", Some("BILLING")),
                        ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                        ("GRANTEE", "VARCHAR2(128)", Some("REPORTING")),
                        ("PRIVILEGE", "VARCHAR2(40)", Some("SELECT")),
                        ("GRANTABLE", "VARCHAR2(3)", Some("NO")),
                        ("HIERARCHY", "VARCHAR2(3)", Some("NO")),
                    ])],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_db_links"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_tab_comments"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_col_comments"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_editions"),
                    params: vec![],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_editioning_views"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_policies"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
                QueryExpectation {
                    sql_contains: String::from("from all_dependencies"),
                    params: vec![OracleBind::from("BILLING")],
                    rows: vec![],
                },
        ]);
        let connection = StaticConnection {
            expected_queries,
            ..StaticConnection::default()
        };

        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();

        let current_schema = snapshot
            .profile
            .current_schema
            .and_then(|name| snapshot.interner.resolve(name.symbol()));
        assert_eq!(current_schema, Some("BILLING"));
        assert_eq!(
            snapshot.profile.oracle_version,
            plsql_core::OracleVersion::Oracle23ai
        );
        assert!(snapshot.capabilities.can_query_all_views);

        let billing_schema = snapshot
            .profile
            .current_schema
            .expect("current schema should be interned");
        let schema_catalog = snapshot
            .schemas
            .get(&billing_schema)
            .expect("billing schema should exist");

        let invoices_name = schema_catalog
            .objects
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("INVOICES"))
            })
            .copied()
            .expect("object name should intern");
        let invoice_summary_name = schema_catalog
            .objects
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("INVOICE_SUMMARY"))
            })
            .copied()
            .expect("object name should intern");
        let package_object_name = schema_catalog
            .objects
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("BILLING_API"))
            })
            .copied()
            .expect("package object name should intern");
        let tax_function_name = schema_catalog
            .objects
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("CALCULATE_TAX"))
            })
            .copied()
            .expect("function object name should intern");
        let trigger_object_name = schema_catalog
            .objects
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("INVOICES_BIU"))
            })
            .copied()
            .expect("trigger object name should intern");

        let invoices_table = schema_catalog
            .objects
            .get(&invoices_name)
            .and_then(|object| {
                if let CatalogObject::Table(table) = object {
                    Some(table)
                } else {
                    None
                }
            });
        assert!(invoices_table.is_some());
        let invoice_id = invoices_table
            .and_then(|table| {
                table.columns.values().find(|column| {
                    snapshot
                        .interner
                        .resolve(column.name.symbol())
                        .is_some_and(|label| label.eq("INVOICE_ID"))
                })
            })
            .expect("table column should exist");
        assert_eq!(invoice_id.position, 1);
        assert_eq!(invoice_id.data_type.name, "NUMBER");
        assert_eq!(invoice_id.data_type.precision, Some(10));
        assert!(!invoice_id.nullable);
        assert!(!invoice_id.hidden);
        assert!(invoice_id.generated_expression.is_none());

        let invoice_summary_view =
            schema_catalog
                .objects
                .get(&invoice_summary_name)
                .and_then(|object| {
                    if let CatalogObject::View(view) = object {
                        Some(view)
                    } else {
                        None
                    }
                });
        assert!(invoice_summary_view.is_some());
        let total_due = invoice_summary_view
            .and_then(|view| {
                view.columns.values().find(|column| {
                    snapshot
                        .interner
                        .resolve(column.name.symbol())
                        .is_some_and(|label| label.eq("TOTAL_DUE"))
                })
            })
            .expect("view column should exist");
        assert_eq!(total_due.position, 1);
        assert!(total_due.nullable);
        assert_eq!(total_due.generated_expression.as_deref(), Some("0"));
        assert!(total_due.default_expression.is_none());

        let invoices_pk_index = schema_catalog
            .indexes
            .values()
            .find(|index| {
                snapshot
                    .interner
                    .resolve(index.name.symbol())
                    .is_some_and(|label| label.eq("INVOICES_PK_IDX"))
            })
            .expect("index metadata should exist");
        assert!(invoices_pk_index.unique);
        assert_eq!(invoices_pk_index.index_type, "NORMAL");
        assert_eq!(invoices_pk_index.status, ObjectStatus::Valid);
        assert_eq!(
            invoices_pk_index
                .columns
                .first()
                .and_then(|column| snapshot.interner.resolve(column.symbol())),
            Some("INVOICE_ID")
        );

        let customer_fk = schema_catalog
            .constraints
            .values()
            .find(|constraint| {
                snapshot
                    .interner
                    .resolve(constraint.name.symbol())
                    .is_some_and(|label| label.eq("INVOICES_CUSTOMER_FK"))
            })
            .expect("foreign key should exist");
        assert_eq!(customer_fk.constraint_type, ConstraintType::ForeignKey);
        assert_eq!(
            customer_fk
                .columns
                .first()
                .and_then(|column| snapshot.interner.resolve(column.symbol())),
            Some("CUSTOMER_ID")
        );
        assert_eq!(
            customer_fk
                .referenced_table_name
                .and_then(|name| snapshot.interner.resolve(name.symbol())),
            Some("CUSTOMERS")
        );
        assert_eq!(
            customer_fk
                .referenced_columns
                .first()
                .and_then(|column| snapshot.interner.resolve(column.symbol())),
            Some("CUSTOMER_ID")
        );

        let customer_not_null = schema_catalog
            .constraints
            .values()
            .find(|constraint| {
                snapshot
                    .interner
                    .resolve(constraint.name.symbol())
                    .is_some_and(|label| label.eq("INVOICES_CUSTOMER_NN"))
            })
            .expect("not-null constraint should exist");
        assert_eq!(customer_not_null.constraint_type, ConstraintType::NotNull);

        let trigger_metadata = schema_catalog
            .triggers
            .values()
            .find(|trigger| {
                snapshot
                    .interner
                    .resolve(trigger.common.name.symbol())
                    .is_some_and(|label| label.eq("INVOICES_BIU"))
            })
            .expect("trigger metadata should exist");
        assert_eq!(trigger_metadata.timing, TriggerTiming::Before);
        assert_eq!(trigger_metadata.level, TriggerLevel::Row);
        assert_eq!(
            trigger_metadata.events.as_slice(),
            &[TriggerEvent::Insert, TriggerEvent::Update]
        );
        assert_eq!(
            trigger_metadata.target_name.symbol().get(),
            invoices_name.symbol().get()
        );
        assert_eq!(
            trigger_metadata.when_clause.as_deref(),
            Some(":new.total_due >= 0")
        );
        assert!(matches!(
            schema_catalog.objects.get(&trigger_object_name),
            Some(CatalogObject::Trigger(_))
        ));

        let latest_invoice_synonym = schema_catalog
            .synonyms
            .values()
            .find(|synonym| {
                snapshot
                    .interner
                    .resolve(synonym.target_name.symbol())
                    .is_some_and(|label| label.eq("INVOICES"))
            })
            .expect("private synonym should exist");
        assert!(!latest_invoice_synonym.public_synonym);

        let public_schema_name = snapshot
            .schemas
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("PUBLIC"))
            })
            .copied()
            .expect("public schema should exist");
        let public_schema = snapshot
            .schemas
            .get(&public_schema_name)
            .expect("public schema catalog should exist");
        let public_synonym = public_schema
            .synonyms
            .values()
            .find(|synonym| {
                snapshot
                    .interner
                    .resolve(synonym.target_name.symbol())
                    .is_some_and(|label| label.eq("INVOICES"))
            })
            .expect("public synonym should exist");
        assert!(public_synonym.public_synonym);

        let package_metadata = schema_catalog
            .objects
            .get(&package_object_name)
            .and_then(|object| {
                if let CatalogObject::Package(metadata) = object {
                    Some(metadata)
                } else {
                    None
                }
            })
            .expect("package metadata should exist");
        let create_invoice = package_metadata
            .procedures
            .iter()
            .find(|signature| {
                snapshot
                    .interner
                    .resolve(signature.routine_name.symbol())
                    .is_some_and(|label| label.eq("CREATE_INVOICE"))
            })
            .expect("package procedure should exist");
        assert_eq!(create_invoice.arguments.len(), 2);
        let total_for_customer = package_metadata
            .functions
            .iter()
            .find(|signature| {
                snapshot
                    .interner
                    .resolve(signature.routine_name.symbol())
                    .is_some_and(|label| label.eq("TOTAL_FOR_CUSTOMER"))
            })
            .expect("package function should exist");
        assert_eq!(total_for_customer.overload, Some(1));
        assert_eq!(
            total_for_customer
                .return_type
                .as_ref()
                .map(|data_type| data_type.name.as_str()),
            Some("NUMBER")
        );
        assert_eq!(total_for_customer.arguments.len(), 1);

        let tax_function = schema_catalog
            .objects
            .get(&tax_function_name)
            .and_then(|object| {
                if let CatalogObject::Function(metadata) = object {
                    Some(metadata)
                } else {
                    None
                }
            })
            .expect("top-level function should exist");
        assert!(tax_function.deterministic);
        assert!(!tax_function.pipelined);
        assert_eq!(tax_function.signature.arguments.len(), 1);
        assert_eq!(
            tax_function
                .signature
                .return_type
                .as_ref()
                .map(|data_type| data_type.name.as_str()),
            Some("NUMBER")
        );

        let invoice_summary_view_with_hash = schema_catalog
            .objects
            .get(&invoice_summary_name)
            .and_then(|object| {
                if let CatalogObject::View(view) = object {
                    Some(view)
                } else {
                    None
                }
            })
            .expect("invoice summary view should be present");
        assert!(invoice_summary_view_with_hash.query_hash.is_some());
        assert_eq!(invoice_summary_view_with_hash.read_only, Some(false));

        assert_eq!(schema_catalog.grants.len(), 1);
        let reporting_grant = &schema_catalog.grants[0];
        assert!(matches!(
            reporting_grant.privilege,
            crate::GrantPrivilege::Select
        ));
        // REPORTING appears in the ALL_USERS fixture, so it classifies as a
        // direct user grant (and not, conservatively, as a role).
        assert!(matches!(reporting_grant.grantee, crate::Grantee::User(_)));
        assert!(!reporting_grant.grantable);
    }

    /// oracle-qm3q.2 regression: `grantee_from_dictionary_value` must
    /// discriminate an object-privilege grantee against the loaded
    /// `ALL_USERS` set. A grantee that is NOT a known user is a database
    /// role (the only remaining grantee class besides PUBLIC), and must be
    /// recorded as `Grantee::Role` so the privilege resolver downgrades it
    /// to Low confidence with a `RuntimeGrantOrRole` ambiguity instead of
    /// the previous, fail-toward-permissive `Grantee::User` (High).
    #[test]
    fn grantee_classification_uses_loaded_user_set() {
        let mut snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities::default(),
            CatalogSource::default(),
            DateTime::<Utc>::UNIX_EPOCH,
        );

        // Before the user set is loaded, the grantee class is undetermined.
        // R13 / fail-toward-restrictive: an undetermined grantee must NOT be
        // a high-confidence direct user grant — it routes through the role
        // ambiguity path instead.
        assert!(snapshot.known_users.is_none());
        let undetermined =
            grantee_from_dictionary_value(&mut snapshot, "MYSTERY_GRANTEE").expect("grantee");
        assert!(
            matches!(undetermined, crate::Grantee::Role(_)),
            "undetermined grantee (ALL_USERS not loaded) must not be a direct user grant; got {undetermined:?}"
        );

        // Load a user set: APP_USER is a user, APP_READER_ROLE is not.
        let app_user = snapshot.intern_user_name("APP_USER").expect("user");
        let mut users = HashSet::new();
        users.insert(app_user);
        snapshot.known_users = Some(users);

        // PUBLIC is always PUBLIC.
        assert!(matches!(
            grantee_from_dictionary_value(&mut snapshot, "PUBLIC").expect("grantee"),
            crate::Grantee::Public
        ));
        // A known user classifies as a direct user grant.
        assert!(matches!(
            grantee_from_dictionary_value(&mut snapshot, "APP_USER").expect("grantee"),
            crate::Grantee::User(_)
        ));
        // A grantee absent from ALL_USERS classifies as a role — the defect
        // this bead fixes (previously always `Grantee::User`).
        let role =
            grantee_from_dictionary_value(&mut snapshot, "APP_READER_ROLE").expect("grantee");
        let role_name = match role {
            crate::Grantee::Role(role_name) => Some(role_name),
            _ => None,
        };
        assert!(
            role_name.is_some(),
            "APP_READER_ROLE must classify as a role"
        );
        assert_eq!(
            snapshot
                .interner
                .resolve(role_name.expect("role assertion above").symbol()),
            Some("APP_READER_ROLE")
        );
    }

    /// oracle-qm3q.2: if `ALL_USERS` cannot be read, `load_catalog_users`
    /// must leave `known_users` as `None` (so grantees stay conservatively
    /// classified) and record a capability warning rather than aborting the
    /// extraction or silently assuming the grantee universe.
    #[test]
    fn load_catalog_users_failure_is_nonfatal_and_marks_unknown() {
        // A strict mock with NO matching expectation for `from all_users`
        // makes the query fail.
        let connection = StaticConnection {
            expected_queries: vec![QueryExpectation {
                sql_contains: String::from("from something_else"),
                params: vec![],
                rows: vec![],
            }],
            ..StaticConnection::default()
        };
        let mut snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities::default(),
            CatalogSource::default(),
            DateTime::<Utc>::UNIX_EPOCH,
        );

        load_catalog_users_for_test(&connection, &mut snapshot).expect("non-fatal");
        assert!(
            snapshot.known_users.is_none(),
            "failed ALL_USERS read must leave grantee universe undetermined"
        );
        assert!(
            snapshot
                .capabilities
                .warnings
                .iter()
                .any(|w| w.code.eq("all-users-probe")),
            "a capability warning must record the ALL_USERS read failure"
        );
    }

    #[test]
    fn load_snapshot_from_connection_extracts_views_sequences_mviews_types_and_grants() {
        let object_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("ACTIVE_CUSTOMERS")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("VIEW")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CUSTOMER_TOTALS_MV")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("MATERIALIZED VIEW")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICE_SEQ")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("SEQUENCE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("ADDRESS_T")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TYPE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
        ];
        let view_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("VIEW_NAME", "VARCHAR2(128)", Some("ACTIVE_CUSTOMERS")),
            (
                "TEXT_VC",
                "VARCHAR2(4000)",
                Some("select customer_id from customers where active = 'Y'"),
            ),
            ("READ_ONLY", "VARCHAR2(1)", Some("Y")),
        ])];
        let mview_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("MVIEW_NAME", "VARCHAR2(128)", Some("CUSTOMER_TOTALS_MV")),
            ("REFRESH_MODE", "VARCHAR2(6)", Some("DEMAND")),
            ("REFRESH_METHOD", "VARCHAR2(8)", Some("COMPLETE")),
            (
                "QUERY",
                "LONG",
                Some("select customer_id, sum(amount) from invoices group by customer_id"),
            ),
        ])];
        let sequence_rows = vec![oracle_row(&[
            ("SEQUENCE_OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("SEQUENCE_NAME", "VARCHAR2(128)", Some("INVOICE_SEQ")),
            ("MIN_VALUE", "NUMBER(28)", Some("1")),
            ("MAX_VALUE", "NUMBER(28)", Some("9999999999")),
            ("INCREMENT_BY", "NUMBER(28)", Some("1")),
            ("CYCLE_FLAG", "VARCHAR2(1)", Some("N")),
            ("ORDER_FLAG", "VARCHAR2(1)", Some("N")),
            ("CACHE_SIZE", "NUMBER(28)", Some("20")),
        ])];
        let type_attr_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TYPE_NAME", "VARCHAR2(128)", Some("ADDRESS_T")),
                ("ATTR_NAME", "VARCHAR2(128)", Some("STREET")),
                ("ATTR_NO", "NUMBER(10)", Some("1")),
                ("ATTR_TYPE_OWNER", "VARCHAR2(128)", None),
                ("ATTR_TYPE_NAME", "VARCHAR2(128)", Some("VARCHAR2")),
                ("LENGTH", "NUMBER(10)", Some("200")),
                ("PRECISION", "NUMBER(10)", None),
                ("SCALE", "NUMBER(10)", None),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TYPE_NAME", "VARCHAR2(128)", Some("ADDRESS_T")),
                ("ATTR_NAME", "VARCHAR2(128)", Some("ZIP")),
                ("ATTR_NO", "NUMBER(10)", Some("2")),
                ("ATTR_TYPE_OWNER", "VARCHAR2(128)", None),
                ("ATTR_TYPE_NAME", "VARCHAR2(128)", Some("VARCHAR2")),
                ("LENGTH", "NUMBER(10)", Some("10")),
                ("PRECISION", "NUMBER(10)", None),
                ("SCALE", "NUMBER(10)", None),
            ]),
        ];
        let grant_rows = vec![
            oracle_row(&[
                ("TABLE_SCHEMA", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("ACTIVE_CUSTOMERS")),
                ("GRANTEE", "VARCHAR2(128)", Some("PUBLIC")),
                ("PRIVILEGE", "VARCHAR2(40)", Some("SELECT")),
                ("GRANTABLE", "VARCHAR2(3)", Some("NO")),
                ("HIERARCHY", "VARCHAR2(3)", Some("NO")),
            ]),
            oracle_row(&[
                ("TABLE_SCHEMA", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("ACTIVE_CUSTOMERS")),
                ("GRANTEE", "VARCHAR2(128)", Some("REPORTING_ROLE")),
                ("PRIVILEGE", "VARCHAR2(40)", Some("UPDATE")),
                ("GRANTABLE", "VARCHAR2(3)", Some("YES")),
                ("HIERARCHY", "VARCHAR2(3)", Some("NO")),
            ]),
        ];

        // PLSQL-CAT-NEW-3 / oracle-fmro: one root edition + one child
        // edition + one editioning view to exercise both EBR paths.
        let edition_rows = vec![
            oracle_row(&[
                ("EDITION_NAME", "VARCHAR2(128)", Some("ORA$BASE")),
                ("PARENT_EDITION_NAME", "VARCHAR2(128)", None),
                ("USABLE", "VARCHAR2(1)", Some("Y")),
            ]),
            oracle_row(&[
                ("EDITION_NAME", "VARCHAR2(128)", Some("PATCH_2026_05")),
                ("PARENT_EDITION_NAME", "VARCHAR2(128)", Some("ORA$BASE")),
                ("USABLE", "VARCHAR2(1)", Some("Y")),
            ]),
        ];
        let editioning_view_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("VIEW_NAME", "VARCHAR2(128)", Some("INVOICES_E1")),
            ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
        ])];

        // PLSQL-CAT-NEW-2 / oracle-c0gg: one VPD policy that gates
        // SELECT-only on INVOICES — exercises the yn() column reader
        // and the SchemaCatalog::vpd_policies path.
        let vpd_policy_rows = vec![oracle_row(&[
            ("OBJECT_OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES")),
            ("POLICY_GROUP", "VARCHAR2(128)", None),
            (
                "POLICY_NAME",
                "VARCHAR2(128)",
                Some("BILLING_TENANT_ISOLATION"),
            ),
            ("PF_OWNER", "VARCHAR2(128)", Some("SECURITY")),
            ("PACKAGE", "VARCHAR2(128)", Some("RLS_PKG")),
            ("FUNCTION", "VARCHAR2(128)", Some("TENANT_PREDICATE")),
            ("SEL", "VARCHAR2(3)", Some("YES")),
            ("INS", "VARCHAR2(3)", Some("NO")),
            ("UPD", "VARCHAR2(3)", Some("NO")),
            ("DEL", "VARCHAR2(3)", Some("NO")),
            ("ENABLE", "VARCHAR2(3)", Some("YES")),
        ])];

        // PLSQL-CAT-NEW-5 / oracle-grs0: one table comment + one column
        // comment exercise both apply_*_comment_row paths.
        let table_comment_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
            ("TABLE_TYPE", "VARCHAR2(11)", Some("TABLE")),
            (
                "COMMENTS",
                "VARCHAR2(4000)",
                Some("Customer invoice header rows; one per invoice"),
            ),
        ])];
        let column_comment_rows = vec![oracle_row(&[
            ("OWNER", "VARCHAR2(128)", Some("BILLING")),
            ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
            ("COLUMN_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
            (
                "COMMENTS",
                "VARCHAR2(4000)",
                Some("Primary key; surrogate, allocated from INVOICES_SEQ"),
            ),
        ])];

        // One private link (BILLING.REPORTING_LINK) and one public link
        // (PUBLIC.WORLDWIDE_LINK) — exercises both code paths in
        // `apply_db_link_row` (PLSQL-CAT-NEW-1 / oracle-rr4y).
        let db_link_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("DB_LINK", "VARCHAR2(128)", Some("REPORTING_LINK")),
                ("HOST", "VARCHAR2(2000)", Some("reporting-db.internal")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("PUBLIC")),
                ("DB_LINK", "VARCHAR2(128)", Some("WORLDWIDE_LINK")),
                ("HOST", "VARCHAR2(2000)", Some("//world.example.com/PROD")),
            ]),
        ];

        let billing_only = vec![OracleBind::from("BILLING")];
        let mut expected_queries = capability_probe_expectations();
        // Only BILLING is a real user; REPORTING_ROLE is deliberately absent
        // so the UPDATE grant to it classifies as a role, not a user.
        expected_queries.push(all_users_expectation(&["BILLING"]));
        expected_queries.extend(vec![
            QueryExpectation {
                sql_contains: String::from("from all_objects"),
                params: billing_only.clone(),
                rows: object_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_cols"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_constraints c"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_indexes i"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_triggers"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_synonyms"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_procedures"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_arguments"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_views"),
                params: billing_only.clone(),
                rows: view_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_mviews"),
                params: billing_only.clone(),
                rows: mview_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_sequences"),
                params: billing_only.clone(),
                rows: sequence_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_type_attrs"),
                params: billing_only.clone(),
                rows: type_attr_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_privs"),
                params: billing_only.clone(),
                rows: grant_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_db_links"),
                params: billing_only.clone(),
                rows: db_link_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_comments"),
                params: billing_only.clone(),
                rows: table_comment_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_col_comments"),
                params: billing_only.clone(),
                rows: column_comment_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_editions"),
                params: vec![],
                rows: edition_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_editioning_views"),
                params: billing_only.clone(),
                rows: editioning_view_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_policies"),
                params: billing_only.clone(),
                rows: vpd_policy_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_dependencies"),
                params: billing_only,
                rows: vec![],
            },
        ]);
        let connection = StaticConnection {
            expected_queries,
            ..StaticConnection::default()
        };

        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();

        let schema = snapshot
            .profile
            .current_schema
            .expect("current schema interned");
        let schema_catalog = snapshot
            .schemas
            .get(&schema)
            .expect("billing schema catalog should exist");

        let view = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::View(view) => Some(view),
                _ => None,
            })
            .expect("view metadata present");
        assert!(view.query_hash.is_some());
        assert_eq!(view.read_only, Some(true));

        let mview = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::MaterializedView(metadata) => Some(metadata),
                _ => None,
            })
            .expect("mview metadata present");
        assert_eq!(mview.refresh_mode.as_deref(), Some("DEMAND"));
        assert_eq!(mview.refresh_method.as_deref(), Some("COMPLETE"));
        assert!(mview.query_hash.is_some());

        let sequence = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::Sequence(metadata) => Some(metadata),
                _ => None,
            })
            .expect("sequence metadata present");
        assert_eq!(sequence.increment_by, 1);
        assert_eq!(sequence.min_value, Some(1));
        assert_eq!(sequence.max_value, Some(9999999999));
        assert!(!sequence.cycle);
        assert!(!sequence.ordered);
        assert_eq!(sequence.cache_size, Some(20));

        let type_metadata = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::Type(metadata) => Some(metadata),
                _ => None,
            })
            .expect("type metadata present");
        assert_eq!(type_metadata.attributes.len(), 2);
        assert_eq!(type_metadata.attributes[0].position, 1);
        assert_eq!(type_metadata.attributes[1].position, 2);
        assert_eq!(type_metadata.attributes[0].data_type.name, "VARCHAR2");
        assert_eq!(type_metadata.attributes[0].data_type.length, Some(200));

        let public_grant = schema_catalog
            .grants
            .iter()
            .find(|grant| matches!(grant.grantee, crate::Grantee::Public))
            .expect("public grant present");
        assert!(matches!(
            public_grant.privilege,
            crate::GrantPrivilege::Select
        ));
        // REPORTING_ROLE is not in the ALL_USERS fixture, so the UPDATE
        // grant to it must classify as a role grant (it was previously,
        // and incorrectly, recorded as a direct user grant — oracle-qm3q.2).
        let role_grant = schema_catalog
            .grants
            .iter()
            .find(|grant| matches!(grant.grantee, crate::Grantee::Role(_)))
            .expect("role grant present");
        let crate::Grantee::Role(role) = &role_grant.grantee else {
            unreachable!("matched Grantee::Role above");
        };
        assert_eq!(
            snapshot.interner.resolve(role.symbol()),
            Some("REPORTING_ROLE")
        );
        assert!(matches!(
            role_grant.privilege,
            crate::GrantPrivilege::Update
        ));
        assert!(role_grant.grantable);
        // And no grantee is misclassified as a direct user in this fixture.
        assert!(
            !schema_catalog
                .grants
                .iter()
                .any(|grant| matches!(grant.grantee, crate::Grantee::User(_)))
        );

        // PLSQL-CAT-NEW-1 / oracle-rr4y: ALL_DB_LINKS rows lower into
        // the owning schema's `db_links` list. The fixture seeds one
        // private link on BILLING and one public link on the synthetic
        // PUBLIC schema — both must surface with correct `public_link`
        // classification and the raw HOST string preserved.
        let private_link = schema_catalog
            .db_links
            .iter()
            .find(|link| link.name.eq("REPORTING_LINK"))
            .expect("private db link present on BILLING schema");
        assert!(!private_link.public_link);
        assert_eq!(private_link.host.as_deref(), Some("reporting-db.internal"));

        // Locate the synthetic PUBLIC schema by resolving each interned
        // schema name back through the interner — the loader materialized
        // it on demand when a `PUBLIC`-owned link was applied, but no
        // ergonomic lookup-by-text exists on SymbolInterner yet.
        let public_link = snapshot
            .schemas
            .iter()
            .find_map(|(schema_id, schema_catalog)| {
                let label = snapshot.interner.resolve(schema_id.symbol())?;
                if label.eq_ignore_ascii_case("PUBLIC") {
                    schema_catalog
                        .db_links
                        .iter()
                        .find(|link| link.name.eq("WORLDWIDE_LINK"))
                } else {
                    None
                }
            })
            .expect("public db link present on PUBLIC synthetic schema");
        assert!(public_link.public_link);
        assert_eq!(
            public_link.host.as_deref(),
            Some("//world.example.com/PROD")
        );

        // PLSQL-CAT-NEW-2 / oracle-c0gg: ALL_POLICIES row lands in
        // SchemaCatalog::vpd_policies with SEL/INS/UPD/DEL/ENABLE
        // correctly decoded from the Y/N/YES/NO mix in dictionary text.
        assert_eq!(schema_catalog.vpd_policies.len(), 1);
        let policy = &schema_catalog.vpd_policies[0];
        assert_eq!(policy.policy_name, "BILLING_TENANT_ISOLATION");
        assert_eq!(policy.function_name, "TENANT_PREDICATE");
        assert_eq!(policy.function_package.as_deref(), Some("RLS_PKG"));
        assert!(policy.policy_group.is_none());
        assert!(policy.on_select);
        assert!(!policy.on_insert);
        assert!(!policy.on_update);
        assert!(!policy.on_delete);
        assert!(policy.enabled);

        // PLSQL-CAT-NEW-3 / oracle-fmro: ALL_EDITIONS rows land in
        // CatalogSnapshot::editions; ALL_EDITIONING_VIEWS rows land in
        // SchemaCatalog::editioning_views.
        assert_eq!(snapshot.editions.len(), 2);
        let root = snapshot
            .editions
            .iter()
            .find(|e| e.edition_name.eq("ORA$BASE"))
            .expect("root edition present");
        assert!(root.parent_edition_name.is_none());
        assert!(root.usable);
        let child = snapshot
            .editions
            .iter()
            .find(|e| e.edition_name.eq("PATCH_2026_05"))
            .expect("child edition present");
        assert_eq!(child.parent_edition_name.as_deref(), Some("ORA$BASE"));
        assert_eq!(schema_catalog.editioning_views.len(), 1);
        let ev = &schema_catalog.editioning_views[0];
        let view_label = snapshot.interner.resolve(ev.view_name.symbol()).unwrap();
        let table_label = snapshot.interner.resolve(ev.table_name.symbol()).unwrap();
        assert_eq!(view_label, "INVOICES_E1");
        assert_eq!(table_label, "INVOICES");

        // PLSQL-CAT-NEW-5 / oracle-grs0: ALL_TAB_COMMENTS row reaches
        // SchemaCatalog::table_comments with TABLE_TYPE + COMMENTS
        // preserved verbatim.
        assert_eq!(schema_catalog.table_comments.len(), 1);
        let table_comment = &schema_catalog.table_comments[0];
        assert_eq!(table_comment.table_type, "TABLE");
        assert_eq!(
            table_comment.comments,
            "Customer invoice header rows; one per invoice"
        );
        // ALL_COL_COMMENTS row lands in column_comments with the
        // interned ColumnName.
        assert_eq!(schema_catalog.column_comments.len(), 1);
        assert_eq!(
            schema_catalog.column_comments[0].comments,
            "Primary key; surrogate, allocated from INVOICES_SEQ"
        );
    }

    #[test]
    fn load_snapshot_from_connection_requires_current_schema_when_requested() {
        let mut connection_info = StaticConnection::default().connection_info;
        connection_info.current_schema = None;
        let connection = StaticConnection {
            connection_info,
            ..StaticConnection::default()
        };

        let error = load_snapshot_for_test(&connection, &CatalogLoadRequest::default());

        assert!(matches!(error, Err(CatalogError::CurrentSchemaUnavailable)));
    }

    #[test]
    fn oracle_connection_default_helpers_enforce_row_cardinality() {
        let mut row = OracleRow::default();
        row.insert(
            "schema_name",
            "VARCHAR2(128)",
            Some(String::from("billing")),
        );

        let single = StaticConnection {
            rows: vec![row.clone()],
            row_count: 1,
            ..StaticConnection::default()
        };
        let multiple = StaticConnection {
            rows: vec![row.clone(), row],
            row_count: 2,
            ..StaticConnection::default()
        };

        let single_result = run_catalog_future(async {
            let cx = test_cx();
            single.query_one_row(&cx, "select * from dual", &[]).await
        });
        assert!(single_result.is_ok());
        let multiple_result = run_catalog_future(async {
            let cx = test_cx();
            multiple
                .query_optional_row(&cx, "select * from dual", &[])
                .await
        });
        assert!(matches!(
            multiple_result,
            Err(CatalogError::UnexpectedRowCount { expected, actual })
                if expected.eq("0 or 1") && actual.eq(&2)
        ));
    }

    #[test]
    fn oracle_connect_options_capture_session_overrides() {
        let options = OracleConnectOptions::new("scott", "tiger", "//localhost/XE")
            .with_current_schema("billing")
            .with_module("plsql-intelligence")
            .with_action("catalog-extract")
            .with_client_info("tests")
            .with_client_identifier("unit");

        assert_eq!(options.current_schema.as_deref(), Some("billing"));
        assert_eq!(options.module.as_deref(), Some("plsql-intelligence"));
        assert_eq!(options.action.as_deref(), Some("catalog-extract"));
        assert_eq!(options.client_info.as_deref(), Some("tests"));
        assert_eq!(options.client_identifier.as_deref(), Some("unit"));
    }

    #[test]
    fn catalog_snapshot_initializes_with_empty_schema_map() {
        let snapshot = CatalogSnapshot::new(
            AnalysisProfile::default(),
            CatalogCapabilities::default(),
            CatalogSource {
                kind: CatalogSourceKind::SyntheticTestCatalog,
                path: None,
                description: Some(String::from("fixture")),
            },
            DateTime::<Utc>::UNIX_EPOCH,
        );

        assert!(snapshot.schemas.is_empty());
        assert!(snapshot.interner.is_empty());
        assert_eq!(snapshot.generated_at, DateTime::<Utc>::UNIX_EPOCH);
        assert_eq!(
            snapshot.source.kind,
            CatalogSourceKind::SyntheticTestCatalog
        );
    }

    #[test]
    fn schema_catalog_can_hold_structural_lookup_maps() {
        let mut schema_catalog = SchemaCatalog::default();
        let object_name = ObjectName::from(SymbolId::new(2));
        let column_name = ColumnName::from(SymbolId::new(3));
        let owner = SchemaName::from(SymbolId::new(1));

        let table = TableMetadata {
            common: ObjectCommon {
                owner,
                name: object_name,
                object_type: ObjectType::Table,
                status: ObjectStatus::Valid,
                source_hash: Some(Hash::new("abc123")),
                ..ObjectCommon::default()
            },
            columns: HashMap::from([(
                column_name,
                crate::ColumnMetadata {
                    name: column_name,
                    position: 1,
                    data_type: DataTypeRef {
                        name: String::from("NUMBER"),
                        precision: Some(10),
                        ..DataTypeRef::default()
                    },
                    nullable: false,
                    ..crate::ColumnMetadata::default()
                },
            )]),
            ..TableMetadata::default()
        };

        schema_catalog
            .objects
            .insert(object_name, CatalogObject::Table(table));
        schema_catalog.synonyms.insert(
            SynonymName::from(SymbolId::new(4)),
            SynonymTarget {
                target_owner: Some(owner),
                target_name: object_name,
                public_synonym: false,
                ..SynonymTarget::default()
            },
        );

        assert_eq!(schema_catalog.objects.len(), 1);
        assert_eq!(schema_catalog.synonyms.len(), 1);
    }

    #[test]
    fn package_and_plscope_models_capture_signature_state() {
        let owner = SchemaName::from(SymbolId::new(10));
        let package_name = ObjectName::from(SymbolId::new(11));
        let procedure_name = ObjectName::from(SymbolId::new(12));
        let member_name = MemberName::from(SymbolId::new(13));

        let package = PackageMetadata {
            common: ObjectCommon {
                owner,
                name: package_name,
                object_type: ObjectType::Package,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            procedures: vec![RoutineSignature {
                routine_name: procedure_name,
                arguments: vec![crate::ArgumentMetadata {
                    position: 1,
                    name: Some(member_name),
                    data_type: DataTypeRef {
                        name: String::from("VARCHAR2"),
                        length: Some(30),
                        ..DataTypeRef::default()
                    },
                    ..crate::ArgumentMetadata::default()
                }],
                accessible_by: vec![AccessibleByTarget {
                    owner: Some(owner),
                    object_name: package_name,
                }],
                ..RoutineSignature::default()
            }],
            ..PackageMetadata::default()
        };

        let plscope = PlScopeSnapshot {
            availability: PlScopeAvailability::IdentifiersAndStatements,
            identifiers: vec![CompilerIdentifier {
                owner,
                object_name: package_name,
                identifier_name: member_name,
                identifier_type: String::from("FORMAL IN"),
                usage: String::from("DECLARATION"),
                line: 4,
                column: 12,
            }],
            ..PlScopeSnapshot::default()
        };

        assert_eq!(package.procedures.len(), 1);
        assert_eq!(package.procedures[0].accessible_by.len(), 1);
        assert_eq!(
            plscope.availability,
            PlScopeAvailability::IdentifiersAndStatements
        );
        assert_eq!(plscope.identifiers[0].line, 4);
        assert_eq!(ConstraintType::ForeignKey, ConstraintType::ForeignKey);
        assert_eq!(TypeFinality::Unknown, TypeFinality::default());
        assert_eq!(TypeInstantiable::Unknown, TypeInstantiable::default());
        assert_eq!(TriggerName::from(SymbolId::new(14)).symbol().get(), 14);
    }

    #[test]
    fn catalog_snapshot_round_trips_through_versioned_json_document() {
        let tempdir = tempdir();
        assert!(tempdir.is_ok());
        let tempdir = if let Ok(tempdir) = tempdir {
            tempdir
        } else {
            return;
        };

        let mut snapshot = CatalogSnapshot::new(
            AnalysisProfile::default(),
            CatalogCapabilities::default(),
            CatalogSource {
                kind: CatalogSourceKind::JsonSnapshot,
                path: None,
                description: Some(String::from("roundtrip")),
            },
            DateTime::<Utc>::UNIX_EPOCH,
        );
        let billing = snapshot.intern_schema_name("billing");
        let claims = snapshot.intern_object_name("claims");
        assert!(billing.is_some());
        assert!(claims.is_some());

        let path = tempdir.path().join("snapshot.json");
        let exported = export_snapshot_to_json(&snapshot, &path);
        assert!(exported.is_ok());

        let loaded = load_snapshot_from_json(&path);
        assert!(loaded.is_ok());
        assert_eq!(loaded.ok(), Some(snapshot.clone()));

        let rendered = std::fs::read_to_string(path);
        assert!(rendered.is_ok());
        let rendered = if let Ok(rendered) = rendered {
            rendered
        } else {
            return;
        };
        let document = serde_json::from_str::<CatalogSnapshotDocument>(&rendered);
        assert!(document.is_ok());
        let document = if let Ok(document) = document {
            document
        } else {
            return;
        };

        assert!(document.schema_id.as_str().eq(CATALOG_SNAPSHOT_SCHEMA_ID));
        assert!(matches!(
            document
                .schema_version
                .cmp(&CATALOG_SNAPSHOT_SCHEMA_VERSION),
            std::cmp::Ordering::Equal
        ));
        assert_eq!(
            document
                .snapshot
                .interner
                .resolve(billing.unwrap_or_default().symbol()),
            Some("billing")
        );
        assert_eq!(
            document
                .snapshot
                .interner
                .resolve(claims.unwrap_or_default().symbol()),
            Some("claims")
        );
    }

    #[test]
    fn load_from_dbms_metadata_dir_classifies_create_statements() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Write some .sql files
        std::fs::write(
            root.join("customers.sql"),
            "CREATE TABLE customers (id NUMBER, name VARCHAR2(100));",
        )
        .unwrap();
        std::fs::write(
            root.join("billing_api.sql"),
            "CREATE OR REPLACE PACKAGE billing_api AS\n  PROCEDURE charge(p_id NUMBER);\nEND;",
        )
        .unwrap();
        std::fs::write(
            root.join("invoice_seq.sql"),
            "CREATE SEQUENCE invoice_seq START WITH 1 INCREMENT BY 1;",
        )
        .unwrap();
        std::fs::write(root.join("skip.txt"), "not a sql file").unwrap();

        let snapshot = load_from_dbms_metadata_dir(root).unwrap();
        assert_eq!(snapshot.source.kind, CatalogSourceKind::DbmsMetadataFiles);

        // Should have found objects in a default schema
        let total_objects: usize = snapshot.schemas.values().map(|s| s.objects.len()).sum();
        assert!(
            total_objects >= 2,
            "expected at least 2 objects, got {}",
            total_objects
        );
    }

    #[test]
    fn load_from_dbms_metadata_dir_returns_error_for_nonexistent_dir() {
        let result = load_from_dbms_metadata_dir(std::path::Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn load_from_dbms_metadata_dir_handles_empty_dir() {
        let dir = tempdir().unwrap();
        let snapshot = load_from_dbms_metadata_dir(dir.path()).unwrap();
        assert!(
            snapshot.schemas.is_empty() || snapshot.schemas.values().all(|s| s.objects.is_empty())
        );
    }

    /// DDL with a qualified `OWNER.OBJECT` prefix must be filed under the
    /// real owner schema, never collapsed to a single shared bucket. A
    /// multi-schema DBMS_METADATA dump that lands under one schema would
    /// silently lose cross-schema topology.
    #[test]
    fn load_from_dbms_metadata_dir_records_real_schema_owner() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("hr_employees.sql"),
            "CREATE TABLE hr.employees (id NUMBER PRIMARY KEY);",
        )
        .unwrap();
        std::fs::write(
            root.join("billing_invoices.sql"),
            "CREATE TABLE billing.invoices (id NUMBER, amount NUMBER(12,2));",
        )
        .unwrap();
        let snapshot = load_from_dbms_metadata_dir(root).unwrap();

        // Two distinct owner schemas must be present — not collapsed.
        let schema_names: std::collections::HashSet<String> = snapshot
            .schemas
            .keys()
            .filter_map(|s| snapshot.interner.resolve(s.symbol()).map(str::to_string))
            .collect();
        assert!(
            schema_names.contains("HR"),
            "HR schema bucket must exist; got {schema_names:?}"
        );
        assert!(
            schema_names.contains("BILLING"),
            "BILLING schema bucket must exist; got {schema_names:?}"
        );

        // Each schema bucket holds exactly its own object — never the
        // other's.
        for (schema, bucket) in &snapshot.schemas {
            let name = snapshot.interner.resolve(schema.symbol()).unwrap();
            assert_eq!(
                bucket.objects.len(),
                1,
                "{name} bucket must hold exactly one object"
            );
        }
    }

    /// Unqualified CREATE statements (no owner prefix) must land in a
    /// stable named schema (e.g. `PUBLIC`) interned through the regular
    /// interner — never `SymbolId::new(0)` which collides with whatever
    /// the first interner entry happens to be.
    #[test]
    fn load_from_dbms_metadata_dir_uses_named_default_schema_for_unqualified_ddl() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("customers.sql"),
            "CREATE TABLE customers (id NUMBER, name VARCHAR2(100));",
        )
        .unwrap();
        let snapshot = load_from_dbms_metadata_dir(root).unwrap();

        let schema_names: std::collections::HashSet<String> = snapshot
            .schemas
            .keys()
            .filter_map(|s| snapshot.interner.resolve(s.symbol()).map(str::to_string))
            .collect();
        assert!(
            schema_names.contains("PUBLIC"),
            "default schema bucket (PUBLIC) must exist for unqualified DDL; got {schema_names:?}"
        );
    }

    /// The classifier must actually record the raw DDL text on the
    /// produced `CatalogObject` — the original docstring promised this
    /// and downstream consumers (doc generation, lineage) rely on it.
    #[test]
    fn load_from_dbms_metadata_dir_records_raw_ddl_text_on_object() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let raw = "CREATE TABLE hr.orders (id NUMBER PRIMARY KEY, total NUMBER(12,2));";
        std::fs::write(root.join("orders.sql"), raw).unwrap();
        let snapshot = load_from_dbms_metadata_dir(root).unwrap();

        let bucket = snapshot
            .schemas
            .values()
            .find(|b| !b.objects.is_empty())
            .expect("at least one schema bucket with an object");
        let obj = bucket.objects.values().next().unwrap();
        let common = match obj {
            CatalogObject::Table(t) => Some(&t.common),
            _ => None,
        }
        .expect("expected Table");
        let ddl = common
            .ddl
            .as_ref()
            .expect("CatalogObject must carry its raw DDL text");
        assert_eq!(ddl.ddl_text, raw, "ddl_text must round-trip the source DDL");
    }

    #[test]
    fn load_snapshot_populates_object_metadata_and_dependency_rows() {
        let object_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_PKG")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("PACKAGE")),
                ("STATUS", "VARCHAR2(7)", Some("INVALID")),
                (
                    "LAST_DDL_TIME_ISO",
                    "VARCHAR2(19)",
                    Some("2026-05-01T13:14:15"),
                ),
                ("EDITIONABLE", "VARCHAR2(1)", Some("Y")),
                ("EDITION_NAME", "VARCHAR2(128)", Some("ORA$BASE")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("CUSTOMERS")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TABLE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
                (
                    "LAST_DDL_TIME_ISO",
                    "VARCHAR2(19)",
                    Some("2026-04-22T08:30:00"),
                ),
                ("EDITIONABLE", "VARCHAR2(1)", None),
                ("EDITION_NAME", "VARCHAR2(128)", None),
            ]),
        ];

        let dependency_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("NAME", "VARCHAR2(128)", Some("BILLING_PKG")),
                ("TYPE", "VARCHAR2(30)", Some("PACKAGE")),
                ("REFERENCED_OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("REFERENCED_NAME", "VARCHAR2(128)", Some("CUSTOMERS")),
                ("REFERENCED_TYPE", "VARCHAR2(30)", Some("TABLE")),
                ("DEPENDENCY_TYPE", "VARCHAR2(4)", Some("HARD")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("NAME", "VARCHAR2(128)", Some("BILLING_PKG")),
                ("TYPE", "VARCHAR2(30)", Some("PACKAGE")),
                ("REFERENCED_OWNER", "VARCHAR2(128)", Some("SYS")),
                ("REFERENCED_NAME", "VARCHAR2(128)", Some("DBMS_OUTPUT")),
                ("REFERENCED_TYPE", "VARCHAR2(30)", Some("PACKAGE")),
                ("DEPENDENCY_TYPE", "VARCHAR2(4)", Some("REF")),
            ]),
        ];

        let billing_only = vec![OracleBind::from("BILLING")];
        let mut expected_queries = capability_probe_expectations();
        // No grants are asserted here, but live extraction still issues the
        // ALL_USERS probe before ALL_TAB_PRIVS.
        expected_queries.push(all_users_expectation(&["BILLING"]));
        expected_queries.extend(vec![
            QueryExpectation {
                sql_contains: String::from("from all_objects"),
                params: billing_only.clone(),
                rows: object_rows,
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_cols"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_constraints c"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_indexes i"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_triggers"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_synonyms"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_procedures"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_arguments"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_views"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_mviews"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_sequences"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_type_attrs"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_privs"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_db_links"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_tab_comments"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_col_comments"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_editions"),
                params: vec![],
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_editioning_views"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_policies"),
                params: billing_only.clone(),
                rows: vec![],
            },
            QueryExpectation {
                sql_contains: String::from("from all_dependencies"),
                params: billing_only,
                rows: dependency_rows,
            },
        ]);
        let connection = StaticConnection {
            expected_queries,
            ..StaticConnection::default()
        };

        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();
        let schema = snapshot
            .profile
            .current_schema
            .expect("current schema interned");
        let schema_catalog = snapshot.schemas.get(&schema).expect("billing schema");

        let billing_pkg = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::Package(metadata) => Some(metadata),
                _ => None,
            })
            .expect("billing pkg present");
        assert_eq!(billing_pkg.common.status, ObjectStatus::Invalid);
        assert_eq!(billing_pkg.common.editionable, Some(true));
        assert!(billing_pkg.common.edition_name.is_some());
        let last_ddl = billing_pkg
            .common
            .last_ddl_time
            .expect("last_ddl_time populated");
        // The fixture date is 2026-05-01 13:14:15 UTC.
        assert_eq!(last_ddl.timestamp(), 1_777_641_255);

        let customers_table = schema_catalog
            .objects
            .values()
            .find_map(|object| match object {
                CatalogObject::Table(metadata) => Some(metadata),
                _ => None,
            })
            .expect("customers table");
        assert!(customers_table.common.editionable.is_none());
        assert!(customers_table.common.edition_name.is_none());

        assert_eq!(schema_catalog.dependencies.len(), 2);
        let pkg_to_customers = schema_catalog
            .dependencies
            .iter()
            .find(|d| matches!(d.dependency_kind, CatalogDependencyKind::Hard))
            .expect("hard dependency");
        assert!(matches!(pkg_to_customers.object_type, ObjectType::Package));
        assert!(matches!(
            pkg_to_customers.referenced_type,
            Some(ObjectType::Table)
        ));
        let pkg_to_sys = schema_catalog
            .dependencies
            .iter()
            .find(|d| matches!(d.dependency_kind, CatalogDependencyKind::Reference))
            .expect("ref dependency");
        assert!(matches!(
            pkg_to_sys.referenced_type,
            Some(ObjectType::Package)
        ));
        assert_eq!(
            pkg_to_sys
                .referenced_owner
                .and_then(|owner| snapshot.interner.resolve(owner.symbol())),
            Some("SYS")
        );
    }

    #[test]
    fn doctor_report_counts_objects_columns_and_indexes() {
        let connection = StaticConnection::default();
        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();
        let mut snapshot = snapshot;
        let billing = snapshot.intern_schema_name("BILLING").expect("schema name");
        let invoices = snapshot
            .intern_object_name("INVOICES")
            .expect("object name");
        let invoice_view = snapshot
            .intern_object_name("INVOICE_VIEW")
            .expect("object name");
        let billing_seq = snapshot
            .intern_object_name("BILLING_SEQ")
            .expect("sequence name");

        let invoice_id = snapshot
            .intern_column_name("INVOICE_ID")
            .expect("column name");
        let table_metadata = crate::TableMetadata {
            common: ObjectCommon {
                owner: billing,
                name: invoices,
                object_type: ObjectType::Table,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            columns: HashMap::from([(
                invoice_id,
                crate::ColumnMetadata {
                    name: invoice_id,
                    position: 1,
                    ..crate::ColumnMetadata::default()
                },
            )]),
            ..crate::TableMetadata::default()
        };

        let view_metadata = crate::ViewMetadata {
            common: ObjectCommon {
                owner: billing,
                name: invoice_view,
                object_type: ObjectType::View,
                status: ObjectStatus::Invalid,
                ..ObjectCommon::default()
            },
            ..crate::ViewMetadata::default()
        };

        let sequence_metadata = crate::SequenceMetadata {
            common: ObjectCommon {
                owner: billing,
                name: billing_seq,
                object_type: ObjectType::Sequence,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            ..crate::SequenceMetadata::default()
        };

        let schema_catalog = snapshot.schemas.entry(billing).or_default();
        schema_catalog
            .objects
            .insert(invoices, CatalogObject::Table(table_metadata));
        schema_catalog
            .objects
            .insert(invoice_view, CatalogObject::View(view_metadata));
        schema_catalog
            .objects
            .insert(billing_seq, CatalogObject::Sequence(sequence_metadata));

        let report = snapshot.doctor_report();
        assert_eq!(report.totals.objects_total, 3);
        assert_eq!(report.totals.columns_total, 1);
        let table_tile = report
            .object_counts
            .iter()
            .find(|tile| matches!(tile.object_type, ObjectType::Table))
            .expect("table tile");
        assert_eq!(table_tile.total, 1);
        assert_eq!(table_tile.valid, 1);
        let view_tile = report
            .object_counts
            .iter()
            .find(|tile| matches!(tile.object_type, ObjectType::View))
            .expect("view tile");
        assert_eq!(view_tile.invalid, 1);
        // Capability negotiation (PLSQL-CAT-017) probes the connection; the
        // StaticConnection mock returns Ok([]) for unmatched queries which
        // means every probe succeeds in this test path → no missing-permission
        // suggestions should appear.
        assert!(report.can_query_all_views);
        assert!(report.can_use_dbms_metadata);
        assert!(
            report.missing_permissions.is_empty(),
            "all probes succeeded on the mock so no permissions should be flagged"
        );
    }

    #[test]
    fn doctor_report_suggests_grants_when_capabilities_are_missing() {
        let mut snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities {
                can_query_all_views: true,
                can_query_dba_views: false,
                can_use_dbms_metadata: false,
                can_read_source: false,
                plscope_enabled: false,
                can_query_scheduler: false,
                can_query_roles_and_grants: false,
                warnings: vec![CapabilityWarning {
                    code: String::from("catalog-version-parse-fallback"),
                    message: String::from("server version did not parse"),
                    remediation: None,
                }],
            },
            CatalogSource {
                kind: CatalogSourceKind::LiveConnection,
                path: None,
                description: Some(String::from("live extraction via oraclemcp-db from xe")),
            },
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        let _ = snapshot.intern_schema_name("BILLING");

        let report = snapshot.doctor_report();
        assert!(matches!(
            report.source_kind,
            CatalogSourceKind::LiveConnection
        ));
        assert_eq!(report.capability_warnings.len(), 1);
        let view_names: Vec<&str> = report
            .missing_permissions
            .iter()
            .map(|m| m.view_name.as_str())
            .collect();
        assert!(view_names.iter().any(|v| v.contains("DBA_OBJECTS")));
        assert!(view_names.iter().any(|v| v.contains("DBMS_METADATA")));
        assert!(view_names.iter().any(|v| v.contains("ALL_SOURCE")));
        assert!(view_names.iter().any(|v| v.contains("PLSCOPE_SETTINGS")));
        assert!(view_names.iter().any(|v| v.contains("SCHEDULER_JOBS")));
        assert!(view_names.iter().any(|v| v.contains("ROLE_PRIVS")));
    }

    #[test]
    fn load_snapshot_populates_plscope_availability_from_object_settings() {
        let plscope_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                (
                    "PLSCOPE_SETTINGS",
                    "VARCHAR2(255)",
                    Some("IDENTIFIERS:ALL,STATEMENTS:ALL"),
                ),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                (
                    "PLSCOPE_SETTINGS",
                    "VARCHAR2(255)",
                    Some("IDENTIFIERS:ALL,STATEMENTS:NONE"),
                ),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("REPORTING")),
                (
                    "PLSCOPE_SETTINGS",
                    "VARCHAR2(255)",
                    Some("IDENTIFIERS:NONE,STATEMENTS:NONE"),
                ),
            ]),
        ];

        let billing_only = vec![OracleBind::from("BILLING")];
        let mut expected_queries = capability_probe_expectations();
        // Live extraction issues the ALL_USERS probe before ALL_TAB_PRIVS.
        expected_queries.push(all_users_expectation(&["BILLING", "REPORTING"]));
        for fragment in [
            "from all_objects",
            "from all_tab_cols",
            "from all_constraints c",
            "from all_indexes i",
            "from all_triggers",
            "from all_synonyms",
            "from all_procedures",
            "from all_arguments",
            "from all_views",
            "from all_mviews",
            "from all_sequences",
            "from all_type_attrs",
            "from all_tab_privs",
            "from all_db_links",
            "from all_tab_comments",
            "from all_col_comments",
            "from all_editions",
            "from all_editioning_views",
            "from all_policies",
            "from all_dependencies",
        ] {
            // all_editions is database-wide — no schema-bind param.
            let params = if fragment.eq("from all_editions") {
                vec![]
            } else {
                billing_only.clone()
            };
            expected_queries.push(QueryExpectation {
                sql_contains: String::from(fragment),
                params,
                rows: vec![],
            });
        }
        expected_queries.push(QueryExpectation {
            sql_contains: String::from("from all_plsql_object_settings"),
            params: billing_only.clone(),
            rows: plscope_rows,
        });
        let connection = StaticConnection {
            expected_queries,
            ..StaticConnection::default()
        };
        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();

        let billing = snapshot
            .schemas
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("BILLING"))
            })
            .copied()
            .expect("billing schema");
        let plscope = snapshot
            .schemas
            .get(&billing)
            .and_then(|s| s.plscope.as_ref())
            .expect("billing plscope");
        assert!(matches!(
            plscope.availability,
            PlScopeAvailability::IdentifiersAndStatements
        ));

        let reporting = snapshot
            .schemas
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("REPORTING"))
            })
            .copied()
            .expect("reporting schema");
        let reporting_plscope = snapshot
            .schemas
            .get(&reporting)
            .and_then(|s| s.plscope.as_ref())
            .expect("reporting plscope");
        // All-NONE settings → PL/Scope is wired but stale.
        assert!(matches!(
            reporting_plscope.availability,
            PlScopeAvailability::AvailableButStale
        ));
    }

    #[test]
    fn load_snapshot_extracts_all_identifiers_into_plscope() {
        let identifier_rows = vec![
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("TYPE", "VARCHAR2(30)", Some("VARIABLE")),
                ("USAGE", "VARCHAR2(20)", Some("DECLARATION")),
                ("LINE", "NUMBER(10)", Some("12")),
                ("COL", "NUMBER(10)", Some("5")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_PKG")),
            ]),
            oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("NAME", "VARCHAR2(128)", Some("CUSTOMER_ID")),
                ("TYPE", "VARCHAR2(30)", Some("VARIABLE")),
                ("USAGE", "VARCHAR2(20)", Some("REFERENCE")),
                ("LINE", "NUMBER(10)", Some("18")),
                ("COL", "NUMBER(10)", Some("21")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("BILLING_PKG")),
            ]),
        ];

        let billing_only = vec![OracleBind::from("BILLING")];
        let mut expected_queries = capability_probe_expectations();
        // Live extraction issues the ALL_USERS probe before ALL_TAB_PRIVS.
        expected_queries.push(all_users_expectation(&["BILLING"]));
        for fragment in [
            "from all_objects",
            "from all_tab_cols",
            "from all_constraints c",
            "from all_indexes i",
            "from all_triggers",
            "from all_synonyms",
            "from all_procedures",
            "from all_arguments",
            "from all_views",
            "from all_mviews",
            "from all_sequences",
            "from all_type_attrs",
            "from all_tab_privs",
            "from all_db_links",
            "from all_tab_comments",
            "from all_col_comments",
            "from all_editions",
            "from all_editioning_views",
            "from all_policies",
            "from all_dependencies",
            "from all_plsql_object_settings",
        ] {
            let params = if fragment.eq("from all_editions") {
                vec![]
            } else {
                billing_only.clone()
            };
            expected_queries.push(QueryExpectation {
                sql_contains: String::from(fragment),
                params,
                rows: vec![],
            });
        }
        expected_queries.push(QueryExpectation {
            sql_contains: String::from("from all_identifiers"),
            params: billing_only.clone(),
            rows: identifier_rows,
        });
        let connection = StaticConnection {
            expected_queries,
            ..StaticConnection::default()
        };
        let snapshot = load_snapshot_for_test(&connection, &CatalogLoadRequest::default()).unwrap();

        let billing = snapshot
            .schemas
            .keys()
            .find(|name| {
                snapshot
                    .interner
                    .resolve(name.symbol())
                    .is_some_and(|label| label.eq("BILLING"))
            })
            .copied()
            .expect("billing schema");
        let plscope = snapshot
            .schemas
            .get(&billing)
            .and_then(|s| s.plscope.as_ref())
            .expect("plscope present");
        assert_eq!(plscope.identifiers.len(), 2);
        let first = &plscope.identifiers[0];
        assert_eq!(first.identifier_type, "VARIABLE");
        assert_eq!(first.usage, "DECLARATION");
        assert_eq!(first.line, 12);
        assert_eq!(first.column, 5);
    }

    #[test]
    fn normalize_dbms_metadata_ddl_collapses_whitespace_and_trims_slash() {
        use crate::normalize_dbms_metadata_ddl;
        let input = "  CREATE   TABLE   FOO ( ID   NUMBER )  \n/  ";
        let normalized = normalize_dbms_metadata_ddl(input);
        assert_eq!(normalized, "CREATE TABLE FOO ( ID NUMBER )");
    }

    #[test]
    fn object_type_to_dbms_metadata_value_covers_all_known_types() {
        use crate::object_type_to_dbms_metadata_value;
        // Every object kind that DBMS_METADATA supports must map to a name.
        for object_type in [
            ObjectType::Table,
            ObjectType::View,
            ObjectType::MaterializedView,
            ObjectType::Sequence,
            ObjectType::Type,
            ObjectType::Package,
            ObjectType::Procedure,
            ObjectType::Function,
            ObjectType::Trigger,
            ObjectType::EditioningView,
            ObjectType::SchedulerJob,
            ObjectType::Synonym,
            ObjectType::Index,
        ] {
            assert!(object_type_to_dbms_metadata_value(object_type).is_some());
        }
        // Constraint + Unknown are not directly fetchable via DBMS_METADATA.
        assert!(object_type_to_dbms_metadata_value(ObjectType::Constraint).is_none());
        assert!(object_type_to_dbms_metadata_value(ObjectType::Unknown).is_none());
    }

    #[test]
    fn populate_dbms_metadata_ddl_records_warnings_on_fetch_failure() {
        use crate::CatalogCapabilities;

        // Build a tiny snapshot with one TABLE object, then run populate
        // against a mock that returns Err on every query. We expect the
        // CapabilityWarning trail to grow by one but the snapshot to remain
        // intact.
        let mut snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities {
                can_use_dbms_metadata: true,
                ..CatalogCapabilities::default()
            },
            CatalogSource {
                kind: CatalogSourceKind::LiveConnection,
                path: None,
                description: Some(String::from("test")),
            },
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        let billing = snapshot.intern_schema_name("BILLING").expect("schema");
        let invoices = snapshot.intern_object_name("INVOICES").expect("object");
        let table = crate::TableMetadata {
            common: ObjectCommon {
                owner: billing,
                name: invoices,
                object_type: ObjectType::Table,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            ..crate::TableMetadata::default()
        };
        snapshot
            .schemas
            .entry(billing)
            .or_default()
            .objects
            .insert(invoices, CatalogObject::Table(table));

        // Force the mock to fail by requesting params that won't match.
        let failing_connection = StaticConnection {
            expected_queries: vec![QueryExpectation {
                sql_contains: String::from("dbms_metadata.get_ddl"),
                params: vec![OracleBind::from("UNREACHABLE_SENTINEL")],
                rows: vec![],
            }],
            ..StaticConnection::default()
        };

        populate_dbms_metadata_ddl_for_test(&failing_connection, &mut snapshot).unwrap();

        assert!(
            snapshot
                .capabilities
                .warnings
                .iter()
                .any(|w| w.code.eq("dbms-metadata-fetch-failed"))
        );

        // Capability disabled → populate is a no-op (no extra warning).
        snapshot.capabilities.can_use_dbms_metadata = false;
        let baseline_warnings = snapshot.capabilities.warnings.len();
        populate_dbms_metadata_ddl_for_test(&failing_connection, &mut snapshot).unwrap();
        assert_eq!(snapshot.capabilities.warnings.len(), baseline_warnings);
    }

    #[test]
    fn populate_dbms_metadata_ddl_writes_ddl_field_on_success() {
        use crate::CatalogCapabilities;

        let mut snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities {
                can_use_dbms_metadata: true,
                ..CatalogCapabilities::default()
            },
            CatalogSource {
                kind: CatalogSourceKind::LiveConnection,
                path: None,
                description: Some(String::from("test")),
            },
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        let billing = snapshot.intern_schema_name("BILLING").expect("schema");
        let invoices = snapshot.intern_object_name("INVOICES").expect("object");
        let table = crate::TableMetadata {
            common: ObjectCommon {
                owner: billing,
                name: invoices,
                object_type: ObjectType::Table,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            ..crate::TableMetadata::default()
        };
        snapshot
            .schemas
            .entry(billing)
            .or_default()
            .objects
            .insert(invoices, CatalogObject::Table(table));

        let connection = StaticConnection {
            expected_queries: vec![QueryExpectation {
                sql_contains: String::from("dbms_metadata.get_ddl"),
                params: vec![
                    OracleBind::from("TABLE"),
                    OracleBind::from("INVOICES"),
                    OracleBind::from("BILLING"),
                ],
                rows: vec![oracle_row(&[
                    (
                        "DDL_TEXT",
                        "CLOB",
                        Some("  CREATE TABLE BILLING.INVOICES (ID NUMBER)  \n  "),
                    ),
                    (
                        "XML_TEXT",
                        "CLOB",
                        Some("<TABLE_T><NAME>INVOICES</NAME></TABLE_T>"),
                    ),
                ])],
            }],
            ..StaticConnection::default()
        };

        populate_dbms_metadata_ddl_for_test(&connection, &mut snapshot).unwrap();

        let stored = snapshot
            .schemas
            .get(&billing)
            .and_then(|schema| schema.objects.get(&invoices));
        assert!(
            matches!(stored, Some(CatalogObject::Table(_))),
            "expected a Table catalog object for billing.invoices"
        );
        let Some(CatalogObject::Table(table)) = stored else {
            return;
        };
        let ddl = table.common.ddl.as_ref().expect("ddl populated");
        assert!(ddl.ddl_text.contains("CREATE TABLE BILLING.INVOICES"));
        assert_eq!(
            ddl.normalized_ddl.as_deref(),
            Some("CREATE TABLE BILLING.INVOICES (ID NUMBER)")
        );
        assert_eq!(
            ddl.xml_text.as_deref(),
            Some("<TABLE_T><NAME>INVOICES</NAME></TABLE_T>")
        );
    }

    #[test]
    fn negotiate_capabilities_reports_failures_as_warnings() {
        let connection = StaticConnection {
            expected_queries: capability_probe_expectations()
                .into_iter()
                .map(|mut q| {
                    // Force every probe to "fail" by demanding a non-empty
                    // params slice while the negotiator passes empty params.
                    q.params = vec![OracleBind::from("UNREACHABLE_SENTINEL")];
                    q
                })
                .collect(),
            ..StaticConnection::default()
        };

        let capabilities = negotiate_capabilities_for_test(&connection);

        assert!(!capabilities.can_query_all_views);
        assert!(!capabilities.can_query_dba_views);
        assert!(!capabilities.can_read_source);
        assert!(!capabilities.can_query_scheduler);
        assert!(!capabilities.can_query_roles_and_grants);
        assert!(!capabilities.plscope_enabled);
        // execute() succeeds unconditionally in the mock, so DBMS_METADATA
        // probe is the one capability that "passes" without explicit setup.
        assert!(capabilities.can_use_dbms_metadata);
        // One warning per failed probe (six probes).
        assert_eq!(capabilities.warnings.len(), 6);
        assert!(
            capabilities
                .warnings
                .iter()
                .any(|w| w.code.eq("all-views-probe"))
        );
        assert!(
            capabilities
                .warnings
                .iter()
                .any(|w| w.code.eq("plscope-probe"))
        );
    }

    #[test]
    fn negotiate_capabilities_marks_every_probe_succeeded_on_open_mock() {
        // Default StaticConnection has no expected_queries — falls through to
        // returning self.rows (empty) for every query → every probe succeeds.
        let connection = StaticConnection::default();
        let capabilities = negotiate_capabilities_for_test(&connection);
        assert!(capabilities.can_query_all_views);
        assert!(capabilities.can_query_dba_views);
        assert!(capabilities.can_read_source);
        assert!(capabilities.can_query_scheduler);
        assert!(capabilities.can_query_roles_and_grants);
        assert!(capabilities.plscope_enabled);
        assert!(capabilities.can_use_dbms_metadata);
        assert!(capabilities.warnings.is_empty());
    }

    #[test]
    fn doctor_report_skips_grant_suggestions_for_json_snapshot_source() {
        let snapshot = CatalogSnapshot::new(
            plsql_core::AnalysisProfile::default(),
            CatalogCapabilities {
                can_query_all_views: false,
                can_query_dba_views: false,
                can_use_dbms_metadata: false,
                can_read_source: false,
                plscope_enabled: false,
                can_query_scheduler: false,
                can_query_roles_and_grants: false,
                warnings: vec![],
            },
            CatalogSource {
                kind: CatalogSourceKind::JsonSnapshot,
                path: Some(std::path::PathBuf::from("/tmp/snapshot.json")),
                description: None,
            },
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        );

        let report = snapshot.doctor_report();
        assert!(matches!(
            report.source_kind,
            CatalogSourceKind::JsonSnapshot
        ));
        // Recommendations are inert for JSON snapshots — the capability bits
        // were already frozen at extraction time.
        assert!(report.missing_permissions.is_empty());
    }

    // ----------------------------------------------------------------------
    // DDL kind classifier (header tokenizer, not body substring).
    //
    // Regression bar: the prior implementation substring-matched on the full
    // upper-cased DDL, so any object whose body or comments mentioned
    // `TABLE` was silently filed as a Table. Real public surface, real
    // silent data corruption.
    // ----------------------------------------------------------------------

    /// Drive the per-file DDL through the public loader and return the
    /// single (`ObjectType`, `CatalogObject`) pair it classifies into.
    /// Panics on anything but exactly one classified object — every test
    /// below feeds exactly one DDL so the helper stays obvious.
    fn classify_single(ddl: &str) -> (ObjectType, CatalogObject) {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("obj.sql"), ddl).unwrap();
        let snapshot = load_from_dbms_metadata_dir(dir.path()).unwrap();
        let mut found: Vec<CatalogObject> = snapshot
            .schemas
            .values()
            .flat_map(|s| s.objects.values().cloned())
            .collect();
        assert_eq!(
            found.len(),
            1,
            "expected exactly one classified object for DDL: {ddl}"
        );
        let obj = found.remove(0);
        let common = match &obj {
            CatalogObject::Table(m) => &m.common,
            CatalogObject::View(m) => &m.common,
            CatalogObject::MaterializedView(m) => &m.common,
            CatalogObject::Sequence(m) => &m.common,
            CatalogObject::Type(m) => &m.common,
            CatalogObject::Package(m) => &m.common,
            CatalogObject::Procedure(m) => &m.common,
            CatalogObject::Function(m) => &m.common,
            CatalogObject::Trigger(m) => &m.common,
            CatalogObject::SchedulerJob(m) => &m.common,
            CatalogObject::EditioningView(m) => &m.common,
        };
        (common.object_type, obj)
    }

    /// A VIEW whose body merely mentions the word `TABLE` must classify as
    /// a View, never a Table. The prior substring matcher silently filed
    /// such views as tables — real catalog corruption visible to every
    /// downstream consumer.
    #[test]
    fn classify_view_with_table_in_body_is_view_not_table() {
        let ddl = "CREATE OR REPLACE VIEW hr.v_emp AS SELECT * FROM hr.emp WHERE 'TABLE'='TABLE';";
        let (kind, obj) = classify_single(ddl);
        assert_eq!(
            kind,
            ObjectType::View,
            "VIEW with 'TABLE' literal in body must classify as View, got {kind:?}",
        );
        assert!(
            matches!(obj, CatalogObject::View(_)),
            "expected CatalogObject::View, got {obj:?}",
        );
    }

    /// A TRIGGER body that touches a TABLE must classify as Trigger.
    #[test]
    fn classify_trigger_with_table_in_body_is_trigger() {
        let ddl = "CREATE OR REPLACE TRIGGER hr.t_audit \
                   AFTER INSERT ON hr.employees \
                   BEGIN INSERT INTO hr.audit_table VALUES (:NEW.id); END;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Trigger);
    }

    /// A PROCEDURE body that touches a TABLE must classify as Procedure.
    #[test]
    fn classify_procedure_with_table_in_body_is_procedure() {
        let ddl = "CREATE OR REPLACE PROCEDURE hr.p_load \
                   AS BEGIN INSERT INTO hr.staging_table SELECT * FROM hr.src; END;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Procedure);
    }

    /// A FUNCTION body that touches a TABLE must classify as Function.
    #[test]
    fn classify_function_with_table_in_body_is_function() {
        let ddl = "CREATE OR REPLACE FUNCTION hr.f_count RETURN NUMBER \
                   AS n NUMBER; BEGIN SELECT COUNT(*) INTO n FROM hr.big_table; RETURN n; END;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Function);
    }

    /// `PACKAGE BODY` is a body, not a spec — the classifier returns the
    /// spec only, so a body file must produce zero classified objects
    /// (not a Package, never a Table just because the body mentions one).
    #[test]
    fn classify_package_body_with_table_is_not_package_or_table() {
        let ddl = "CREATE OR REPLACE PACKAGE BODY hr.billing_api AS \
                   PROCEDURE charge IS BEGIN INSERT INTO hr.charges_table VALUES (1); END; END;";
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("body.sql"), ddl).unwrap();
        let snapshot = load_from_dbms_metadata_dir(dir.path()).unwrap();
        let total: usize = snapshot.schemas.values().map(|s| s.objects.len()).sum();
        assert_eq!(
            total, 0,
            "PACKAGE BODY must not produce a classified object (got {total})",
        );
    }

    /// `MATERIALIZED VIEW` must classify as `ObjectType::MaterializedView`,
    /// never as a plain View (substring match on `VIEW` would do the
    /// wrong thing).
    #[test]
    fn classify_materialized_view_is_materialized_view_not_view() {
        let ddl = "CREATE MATERIALIZED VIEW hr.mv_emp_summary AS SELECT dept, COUNT(*) FROM hr.emp GROUP BY dept;";
        let (kind, obj) = classify_single(ddl);
        assert_eq!(
            kind,
            ObjectType::MaterializedView,
            "MATERIALIZED VIEW must classify as MaterializedView, got {kind:?}",
        );
        assert!(
            matches!(obj, CatalogObject::MaterializedView(_)),
            "expected CatalogObject::MaterializedView, got {obj:?}",
        );
    }

    /// Leading block comment that contains `CREATE TABLE …` must NOT
    /// fool the classifier — the real CREATE is for a VIEW.
    #[test]
    fn classify_view_with_leading_block_comment_mentioning_create_table_is_view() {
        let ddl = "/* comment with CREATE TABLE x; */\n\
                   CREATE OR REPLACE VIEW hr.v_dept AS SELECT * FROM hr.dept;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::View);
    }

    /// Leading line comments that mention `CREATE TABLE` must NOT fool
    /// the classifier.
    #[test]
    fn classify_procedure_with_leading_line_comments_is_procedure() {
        let ddl = "-- CREATE TABLE oops (x NUMBER);\n\
                   -- another line mentioning VIEW and TABLE\n\
                   CREATE OR REPLACE PROCEDURE hr.p_noop AS BEGIN NULL; END;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Procedure);
    }

    /// `EDITIONABLE` / `NONEDITIONABLE` / `FORCE` modifiers between
    /// `CREATE [OR REPLACE]` and the KIND must be skipped — they are
    /// not the object kind.
    #[test]
    fn classify_editionable_view_is_view() {
        let ddl = "CREATE OR REPLACE EDITIONABLE VIEW hr.v_emp AS SELECT * FROM hr.emp;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::View);
    }

    /// Quoted identifiers for owner/name must not break the
    /// header-tokenizer-based classifier.
    #[test]
    fn classify_quoted_table_owner_name_is_table() {
        let ddl = "CREATE TABLE \"HR\".\"EMP\" (id NUMBER);";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Table);
    }

    /// A double-quoted Oracle identifier containing whitespace must be
    /// kept whole — never truncated at the first interior space. The
    /// prior `split_whitespace().next()` tokenizer cut `"MY TABLE"` down
    /// to `MY`, corrupting the snapshot key. `extract_owner_and_name`
    /// works on the already-upper-cased post-header remainder, so the
    /// inputs here are upper-cased the way `upper_remainder()` produces.
    #[test]
    fn extract_owner_and_name_keeps_whitespace_in_quoted_identifiers() {
        // OWNER.NAME, both quoted, name has a space.
        assert_eq!(
            crate::extract_owner_and_name("\"HR\".\"MY TABLE\" (ID NUMBER);"),
            Some((Some("HR".to_string()), "MY TABLE".to_string())),
        );
        // Quoted OWNER with a space must not be dropped (no PUBLIC misroute).
        assert_eq!(
            crate::extract_owner_and_name("\"MY OWNER\".\"EMP\" (ID NUMBER);"),
            Some((Some("MY OWNER".to_string()), "EMP".to_string())),
        );
        // Fully-quoted, unqualified, whitespace-bearing name.
        assert_eq!(
            crate::extract_owner_and_name("\"MY TABLE\" (ID NUMBER);"),
            Some((None, "MY TABLE".to_string())),
        );
        // Spaceless quoted and unquoted inputs still resolve correctly.
        assert_eq!(
            crate::extract_owner_and_name("\"HR\".\"EMP\" (ID NUMBER);"),
            Some((Some("HR".to_string()), "EMP".to_string())),
        );
        assert_eq!(
            crate::extract_owner_and_name("HR.ORDERS (ID NUMBER);"),
            Some((Some("HR".to_string()), "ORDERS".to_string())),
        );
    }

    /// Two distinct whitespace-bearing quoted tables in one schema must
    /// intern under distinct keys — the truncating tokenizer collapsed
    /// both `"MY TABLE"` and `"MY OTHER"` to `MY` (last-write-wins).
    #[test]
    fn quoted_whitespace_names_intern_as_distinct_keys() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.sql"),
            "CREATE TABLE \"HR\".\"MY TABLE\" (id NUMBER);",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.sql"),
            "CREATE TABLE \"HR\".\"MY OTHER\" (id NUMBER);",
        )
        .unwrap();
        let snapshot = load_from_dbms_metadata_dir(dir.path()).unwrap();

        // Both objects land in the HR schema (no PUBLIC misroute).
        let hr = snapshot
            .schemas
            .iter()
            .find(|(s, _)| {
                snapshot
                    .interner
                    .resolve(s.symbol())
                    .is_some_and(|label| label.eq("HR"))
            })
            .map(|(_, c)| c)
            .expect("HR schema present");
        assert_eq!(hr.objects.len(), 2, "two distinct objects, no collision");

        let mut names: Vec<&str> = hr
            .objects
            .values()
            .filter_map(|o| {
                let CatalogObject::Table(m) = o else {
                    return None;
                };
                snapshot.interner.resolve(m.common.name.symbol())
            })
            .collect();
        assert_eq!(names.len(), 2, "all HR objects must be tables");
        names.sort_unstable();
        assert_eq!(names, vec!["MY OTHER", "MY TABLE"]);
    }

    /// Plain CREATE TABLE still works (negative-of-negative regression).
    #[test]
    fn classify_plain_table_is_table() {
        let ddl = "CREATE TABLE hr.orders (id NUMBER PRIMARY KEY, total NUMBER(12,2));";
        let (kind, obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Table);
        assert!(matches!(obj, CatalogObject::Table(_)));
    }

    /// A PACKAGE spec that mentions VIEW / TABLE in a comment or
    /// procedure name must classify as Package.
    #[test]
    fn classify_package_spec_mentioning_view_and_table_is_package() {
        let ddl = "CREATE OR REPLACE PACKAGE hr.report_api AS \
                   -- builds a VIEW over a TABLE \n\
                   PROCEDURE rebuild_view_from_table; END;";
        let (kind, _obj) = classify_single(ddl);
        assert_eq!(kind, ObjectType::Package);
    }
}
