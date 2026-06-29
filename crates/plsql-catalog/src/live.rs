use std::collections::{HashMap, HashSet};

use asupersync::Cx;
use tracing::instrument;

use super::*;

#[cfg(test)]
pub(crate) use asupersync::runtime::RuntimeBuilder;

#[cfg(test)]
pub(crate) type LiveContext = Cx;

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
pub struct OraclemcpDbConnection {
    inner: oraclemcp_db::RustOracleConnection,
    connect_string: String,
}

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

fn map_oraclemcp_rows(rows: Vec<oraclemcp_db::OracleRow>) -> Vec<OracleRow> {
    rows.into_iter().map(map_oraclemcp_row).collect()
}

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

fn map_oraclemcp_db_error(err: oraclemcp_db::DbError) -> CatalogError {
    CatalogError::OracleBackendError {
        backend: OracleBackend::OracleRs,
        message: err.to_string(),
    }
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
pub(crate) async fn load_catalog_users<C: OracleConnection>(
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
