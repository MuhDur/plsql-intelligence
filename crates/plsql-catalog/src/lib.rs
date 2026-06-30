#![forbid(unsafe_code)]
pub mod synthetic;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use plsql_core::{
    AnalysisProfile, ColumnName, EditionName, MemberName, ObjectName, RoleName, SchemaName,
    SymbolId, SymbolInterner, UserName,
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

/// Dictionary row family accepted by [`CatalogSnapshotBuilder`].
///
/// Each variant corresponds to one Oracle dictionary query shape used by the
/// live extractor. The enum is intentionally stable and DB-free so
/// `oraclemcp` can own live querying while this crate owns row normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum CatalogRowSet {
    Objects,
    Columns,
    Constraints,
    Indexes,
    Triggers,
    Synonyms,
    Routines,
    RoutineArguments,
    Views,
    MaterializedViews,
    Sequences,
    TypeAttributes,
    Users,
    Grants,
    DatabaseLinks,
    TableComments,
    ColumnComments,
    Editions,
    EditioningViews,
    VpdPolicies,
    Dependencies,
    PlScopeAvailability,
    PlScopeIdentifiers,
}

impl CatalogRowSet {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Objects => "objects",
            Self::Columns => "columns",
            Self::Constraints => "constraints",
            Self::Indexes => "indexes",
            Self::Triggers => "triggers",
            Self::Synonyms => "synonyms",
            Self::Routines => "routines",
            Self::RoutineArguments => "routine_arguments",
            Self::Views => "views",
            Self::MaterializedViews => "materialized_views",
            Self::Sequences => "sequences",
            Self::TypeAttributes => "type_attributes",
            Self::Users => "users",
            Self::Grants => "grants",
            Self::DatabaseLinks => "database_links",
            Self::TableComments => "table_comments",
            Self::ColumnComments => "column_comments",
            Self::Editions => "editions",
            Self::EditioningViews => "editioning_views",
            Self::VpdPolicies => "vpd_policies",
            Self::Dependencies => "dependencies",
            Self::PlScopeAvailability => "plscope_availability",
            Self::PlScopeIdentifiers => "plscope_identifiers",
        }
    }
}

/// Stable, offline builder for Oracle dictionary rows.
///
/// The builder accepts already-fetched [`OracleRow`] values and applies the
/// same normalization used by the live loader. It performs no network or
/// database I/O; callers such as `oraclemcp` own extraction and feed rows into
/// this crate through [`CatalogRowSet`].
///
/// ```
/// use chrono::Utc;
/// use plsql_catalog::{
///     CatalogCapabilities, CatalogRowSet, CatalogSnapshotBuilder, CatalogSource,
///     CatalogSourceKind, ObjectType, OracleRow,
/// };
/// use plsql_core::AnalysisProfile;
///
/// fn row(columns: &[(&str, &str, Option<&str>)]) -> OracleRow {
///     let mut row = OracleRow::default();
///     for (name, oracle_type, value) in columns {
///         row.insert(*name, *oracle_type, value.map(String::from));
///     }
///     row
/// }
///
/// let mut builder = CatalogSnapshotBuilder::new(
///     AnalysisProfile::default(),
///     CatalogCapabilities::default(),
///     CatalogSource {
///         kind: CatalogSourceKind::LiveConnection,
///         description: Some("synthetic rows from an external extractor".to_string()),
///         ..CatalogSource::default()
///     },
///     Utc::now(),
/// );
///
/// builder.apply_row(
///     CatalogRowSet::Objects,
///     &row(&[
///         ("OWNER", "VARCHAR2(128)", Some("BILLING")),
///         ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES")),
///         ("OBJECT_TYPE", "VARCHAR2(30)", Some("TABLE")),
///         ("STATUS", "VARCHAR2(7)", Some("VALID")),
///     ]),
/// )?;
///
/// let snapshot = builder.finish()?;
/// let report = snapshot.doctor_report();
/// assert_eq!(report.totals.schemas_observed, 1);
/// assert_eq!(
///     report.object_counts.first().map(|count| count.object_type),
///     Some(ObjectType::Table),
/// );
/// # Ok::<(), plsql_catalog::CatalogError>(())
/// ```
pub struct CatalogSnapshotBuilder {
    snapshot: CatalogSnapshot,
    routines: HashMap<RoutineLocator, RoutineAccumulator>,
    plscope_tallies: HashMap<SchemaName, PlScopeTally>,
}

impl CatalogSnapshotBuilder {
    #[must_use]
    #[instrument(level = "trace", skip(profile, capabilities, source))]
    pub fn new(
        profile: AnalysisProfile,
        capabilities: CatalogCapabilities,
        source: CatalogSource,
        generated_at: DateTime<Utc>,
    ) -> Self {
        Self::from_snapshot(CatalogSnapshot::new(
            profile,
            capabilities,
            source,
            generated_at,
        ))
    }

    #[must_use]
    #[instrument(level = "trace", skip(snapshot))]
    pub fn from_snapshot(snapshot: CatalogSnapshot) -> Self {
        Self {
            snapshot,
            routines: HashMap::new(),
            plscope_tallies: HashMap::new(),
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn snapshot(&self) -> &CatalogSnapshot {
        &self.snapshot
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn snapshot_mut(&mut self) -> &mut CatalogSnapshot {
        &mut self.snapshot
    }

    #[instrument(level = "trace", skip(self, row), fields(row_set = row_set.as_str()))]
    pub fn apply_row(
        &mut self,
        row_set: CatalogRowSet,
        row: &OracleRow,
    ) -> Result<&mut Self, CatalogError> {
        match row_set {
            CatalogRowSet::Objects => apply_object_row(&mut self.snapshot, row)?,
            CatalogRowSet::Columns => apply_column_row(&mut self.snapshot, row)?,
            CatalogRowSet::Constraints => apply_constraint_row(&mut self.snapshot, row)?,
            CatalogRowSet::Indexes => apply_index_row(&mut self.snapshot, row)?,
            CatalogRowSet::Triggers => apply_trigger_row(&mut self.snapshot, row)?,
            CatalogRowSet::Synonyms => apply_synonym_row(&mut self.snapshot, row)?,
            CatalogRowSet::Routines => {
                apply_routine_row(&mut self.snapshot, row, &mut self.routines)?;
            }
            CatalogRowSet::RoutineArguments => {
                apply_argument_row(&mut self.snapshot, row, &mut self.routines)?;
            }
            CatalogRowSet::Views => apply_view_row(&mut self.snapshot, row)?,
            CatalogRowSet::MaterializedViews => apply_mview_row(&mut self.snapshot, row)?,
            CatalogRowSet::Sequences => apply_sequence_row(&mut self.snapshot, row)?,
            CatalogRowSet::TypeAttributes => apply_type_attr_row(&mut self.snapshot, row)?,
            CatalogRowSet::Users => apply_user_row(&mut self.snapshot, row)?,
            CatalogRowSet::Grants => apply_grant_row(&mut self.snapshot, row)?,
            CatalogRowSet::DatabaseLinks => apply_db_link_row(&mut self.snapshot, row)?,
            CatalogRowSet::TableComments => apply_table_comment_row(&mut self.snapshot, row)?,
            CatalogRowSet::ColumnComments => apply_column_comment_row(&mut self.snapshot, row)?,
            CatalogRowSet::Editions => apply_edition_row(&mut self.snapshot, row)?,
            CatalogRowSet::EditioningViews => apply_editioning_view_row(&mut self.snapshot, row)?,
            CatalogRowSet::VpdPolicies => apply_vpd_policy_row(&mut self.snapshot, row)?,
            CatalogRowSet::Dependencies => apply_dependency_row(&mut self.snapshot, row)?,
            CatalogRowSet::PlScopeAvailability => {
                apply_plscope_availability_row(&mut self.snapshot, row, &mut self.plscope_tallies)?;
            }
            CatalogRowSet::PlScopeIdentifiers => {
                apply_plscope_identifier_row(&mut self.snapshot, row)?;
            }
        }
        Ok(self)
    }

    #[instrument(level = "trace", skip(self, rows), fields(row_set = row_set.as_str()))]
    pub fn apply_rows<'a, I>(
        &mut self,
        row_set: CatalogRowSet,
        rows: I,
    ) -> Result<&mut Self, CatalogError>
    where
        I: IntoIterator<Item = &'a OracleRow>,
    {
        if row_set.eq(&CatalogRowSet::Users) {
            self.snapshot.known_users.get_or_insert_with(HashSet::new);
        }
        for row in rows {
            self.apply_row(row_set, row)?;
        }
        Ok(self)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn finish(mut self) -> Result<CatalogSnapshot, CatalogError> {
        let routines = std::mem::take(&mut self.routines);
        finalize_routines(&mut self.snapshot, routines)?;
        let plscope_tallies = std::mem::take(&mut self.plscope_tallies);
        finalize_plscope_availability(&mut self.snapshot, plscope_tallies);
        Ok(self.snapshot)
    }
}

impl Default for CatalogSnapshotBuilder {
    fn default() -> Self {
        Self::new(
            AnalysisProfile::default(),
            CatalogCapabilities::default(),
            CatalogSource::default(),
            Utc::now(),
        )
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

#[derive(Clone, Debug, Default)]
struct PlScopeTally {
    total: usize,
    with_identifiers: usize,
    with_statements: usize,
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

fn apply_user_row(snapshot: &mut CatalogSnapshot, row: &OracleRow) -> Result<(), CatalogError> {
    let username = row.require_text("USERNAME")?;
    let Some(user) = snapshot.intern_user_name(username) else {
        return Err(CatalogError::InvalidColumnValue {
            column: String::from("USERNAME"),
            expected: "interned user name",
            value: String::from(username),
        });
    };
    snapshot
        .known_users
        .get_or_insert_with(HashSet::new)
        .insert(user);
    Ok(())
}

fn apply_plscope_availability_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
    tallies: &mut HashMap<SchemaName, PlScopeTally>,
) -> Result<(), CatalogError> {
    let Some(owner_text) = optional_nonblank_text(row, "OWNER") else {
        return Ok(());
    };
    let settings = row
        .text("PLSCOPE_SETTINGS")
        .unwrap_or("")
        .to_ascii_uppercase();
    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Ok(());
    };
    let tally = tallies.entry(owner).or_default();
    tally.total = tally.total.saturating_add(1);
    if settings.contains("STATEMENTS:") && !settings.contains("STATEMENTS:NONE") {
        tally.with_statements = tally.with_statements.saturating_add(1);
    }
    if settings.contains("IDENTIFIERS:") && !settings.contains("IDENTIFIERS:NONE") {
        tally.with_identifiers = tally.with_identifiers.saturating_add(1);
    }
    Ok(())
}

fn finalize_plscope_availability(
    snapshot: &mut CatalogSnapshot,
    tallies: HashMap<SchemaName, PlScopeTally>,
) {
    for (owner, tally) in tallies {
        let availability = if tally.with_statements > 0 {
            PlScopeAvailability::IdentifiersAndStatements
        } else if tally.with_identifiers > 0 {
            PlScopeAvailability::IdentifiersOnly
        } else if tally.total > 0 {
            PlScopeAvailability::AvailableButStale
        } else {
            PlScopeAvailability::NotAvailable
        };
        let schema_catalog = snapshot.schemas.entry(owner).or_default();
        let plscope = schema_catalog
            .plscope
            .get_or_insert_with(PlScopeSnapshot::default);
        plscope.availability = availability;
        plscope.collected_at = Some(snapshot.generated_at);
    }
}

fn apply_plscope_identifier_row(
    snapshot: &mut CatalogSnapshot,
    row: &OracleRow,
) -> Result<(), CatalogError> {
    let Some(owner_text) = optional_nonblank_text(row, "OWNER") else {
        return Ok(());
    };
    let Some(object_name_text) = optional_nonblank_text(row, "OBJECT_NAME") else {
        return Ok(());
    };
    let Some(identifier_name_text) = optional_nonblank_text(row, "NAME") else {
        return Ok(());
    };
    let Some(owner) = snapshot.intern_schema_name(owner_text) else {
        return Ok(());
    };
    let Some(object_name) = snapshot.intern_object_name(object_name_text) else {
        return Ok(());
    };
    let Some(identifier_name) = snapshot.intern_member_name(identifier_name_text) else {
        return Ok(());
    };
    let identifier = CompilerIdentifier {
        owner,
        object_name,
        identifier_name,
        identifier_type: optional_nonblank_text(row, "TYPE")
            .map(String::from)
            .unwrap_or_default(),
        usage: optional_nonblank_text(row, "USAGE")
            .map(String::from)
            .unwrap_or_default(),
        line: optional_u32(row, "LINE")?.unwrap_or(0),
        column: optional_u32(row, "COL")?.unwrap_or(0),
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
mod builder_tests {
    use chrono::{DateTime, Utc};
    use plsql_core::AnalysisProfile;

    use crate::{
        CatalogCapabilities, CatalogRowSet, CatalogSnapshotBuilder, CatalogSource,
        CatalogSourceKind, CatalogSourceKind::LiveConnection, GrantPrivilege, Grantee, ObjectType,
        OracleRow, PlScopeAvailability,
    };

    fn oracle_row(columns: &[(&str, &str, Option<&str>)]) -> OracleRow {
        let mut row = OracleRow::default();
        for (name, oracle_type, value) in columns {
            row.insert(*name, *oracle_type, value.map(String::from));
        }
        row
    }

    fn fixed_generated_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-29T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&Utc)
    }

    fn builder() -> CatalogSnapshotBuilder {
        CatalogSnapshotBuilder::new(
            AnalysisProfile::default(),
            CatalogCapabilities {
                can_query_all_views: true,
                can_query_dba_views: true,
                can_use_dbms_metadata: true,
                can_read_source: true,
                plscope_enabled: true,
                can_query_scheduler: true,
                can_query_roles_and_grants: true,
                ..CatalogCapabilities::default()
            },
            CatalogSource {
                kind: LiveConnection,
                description: Some(String::from("synthetic external extractor")),
                ..CatalogSource::default()
            },
            fixed_generated_at(),
        )
    }

    fn apply_synthetic_builder_rows(
        builder: &mut CatalogSnapshotBuilder,
    ) -> Result<(), crate::CatalogError> {
        builder.apply_row(
            CatalogRowSet::Objects,
            &oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("OBJECT_TYPE", "VARCHAR2(30)", Some("TABLE")),
                ("STATUS", "VARCHAR2(7)", Some("VALID")),
            ]),
        )?;
        builder.apply_row(
            CatalogRowSet::Columns,
            &oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("COLUMN_NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
                ("COLUMN_POSITION", "NUMBER", Some("1")),
                ("DATA_TYPE", "VARCHAR2(30)", Some("NUMBER")),
                ("DATA_PRECISION", "NUMBER", Some("10")),
                ("DATA_SCALE", "NUMBER", Some("0")),
                ("NULLABLE", "VARCHAR2(1)", Some("N")),
            ]),
        )?;
        builder.apply_row(
            CatalogRowSet::Users,
            &oracle_row(&[("USERNAME", "VARCHAR2(128)", Some("APP_USER"))]),
        )?;
        builder.apply_row(
            CatalogRowSet::Grants,
            &oracle_row(&[
                ("TABLE_SCHEMA", "VARCHAR2(128)", Some("BILLING")),
                ("TABLE_NAME", "VARCHAR2(128)", Some("INVOICES")),
                ("GRANTEE", "VARCHAR2(128)", Some("APP_USER")),
                ("PRIVILEGE", "VARCHAR2(40)", Some("SELECT")),
                ("GRANTABLE", "VARCHAR2(3)", Some("NO")),
                ("HIERARCHY", "VARCHAR2(3)", Some("NO")),
            ]),
        )?;
        builder.apply_row(
            CatalogRowSet::PlScopeAvailability,
            &oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                (
                    "PLSCOPE_SETTINGS",
                    "VARCHAR2(4000)",
                    Some("IDENTIFIERS:ALL, STATEMENTS:ALL"),
                ),
            ]),
        )?;
        builder.apply_row(
            CatalogRowSet::PlScopeIdentifiers,
            &oracle_row(&[
                ("OWNER", "VARCHAR2(128)", Some("BILLING")),
                ("OBJECT_NAME", "VARCHAR2(128)", Some("INVOICES_PKG")),
                ("NAME", "VARCHAR2(128)", Some("INVOICE_ID")),
                ("TYPE", "VARCHAR2(128)", Some("VARIABLE")),
                ("USAGE", "VARCHAR2(128)", Some("DECLARATION")),
                ("LINE", "NUMBER", Some("7")),
                ("COL", "NUMBER", Some("12")),
            ]),
        )?;
        Ok(())
    }

    #[test]
    fn catalog_snapshot_builder_applies_synthetic_dictionary_rows_on_stable()
    -> Result<(), crate::CatalogError> {
        let mut builder = builder();
        apply_synthetic_builder_rows(&mut builder)?;

        let snapshot = builder.finish()?;
        let report = snapshot.doctor_report();
        assert_eq!(report.source_kind, CatalogSourceKind::LiveConnection);
        assert_eq!(report.totals.schemas_observed, 1);
        assert_eq!(report.totals.objects_total, 1);
        assert_eq!(report.totals.columns_total, 1);
        assert_eq!(report.totals.grants_total, 1);
        assert_eq!(
            report.object_counts.first().map(|count| count.object_type),
            Some(ObjectType::Table)
        );
        assert_eq!(
            report
                .plscope_availability_per_schema
                .first()
                .map(|entry| entry.availability),
            Some(PlScopeAvailability::IdentifiersAndStatements)
        );
        assert!(snapshot.schemas.values().any(|schema| {
            schema.grants.iter().any(|grant| {
                grant.privilege == GrantPrivilege::Select
                    && matches!(grant.grantee, Grantee::User(_))
            })
        }));
        assert_eq!(
            snapshot
                .schemas
                .values()
                .filter_map(|schema| schema.plscope.as_ref())
                .map(|plscope| plscope.identifiers.len())
                .sum::<usize>(),
            1
        );
        Ok(())
    }

    #[test]
    fn catalog_snapshot_builder_doctor_report_matches_golden() -> Result<(), crate::CatalogError> {
        let mut builder = builder();
        apply_synthetic_builder_rows(&mut builder)?;

        let actual = serde_json::to_value(builder.finish()?.doctor_report())?;
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/golden/catalog_snapshot_builder_doctor_report.json"
        ))?;

        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn catalog_snapshot_builder_can_mark_user_universe_known_empty()
    -> Result<(), crate::CatalogError> {
        let mut builder = builder();
        let rows: Vec<OracleRow> = Vec::new();
        builder.apply_rows(CatalogRowSet::Users, &rows)?;
        let snapshot = builder.finish()?;
        assert_eq!(snapshot.known_users, Some(Default::default()));
        Ok(())
    }
}
