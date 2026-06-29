//! Adapter seam from the shared `oraclemcp-db` Oracle foundation into
//! `plsql-catalog`'s catalog-shaped rows.
//!
//! This module deliberately lives in `plsql-mcp`: the offline catalog crate
//! must not depend on `oraclemcp-db`, `oraclemcp-guard`, or the MCP runtime.
//! The adapter implements the catalog trait directly without adding a
//! per-round-trip `block_on` bridge.

use asupersync::Cx;
use oraclemcp_db::SerializeOptions;
use oraclemcp_guard::{ObjectRef, Purity, SideEffectOracle};
use plsql_catalog::{
    CatalogError, OracleBackend as CatalogBackend, OracleBind as CatalogBind,
    OracleCell as CatalogCell, OracleConnection as CatalogOracleConnection,
    OracleConnectionInfo as CatalogConnectionInfo, OracleRow as CatalogRow,
};
use std::collections::HashSet;

use crate::identifier::normalize_identifier;

/// MCP-side adapter over an `oraclemcp-db` connection.
#[derive(Debug)]
pub struct OraclemcpCatalogConnection<C> {
    inner: C,
    serialize_options: SerializeOptions,
}

impl<C> OraclemcpCatalogConnection<C> {
    /// Wrap an existing upstream connection with catalog-extraction
    /// serialization options.
    #[must_use]
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            serialize_options: catalog_extraction_serialize_options(),
        }
    }

    /// Wrap an existing upstream connection with explicit serialization caps.
    #[must_use]
    pub fn with_serialize_options(inner: C, serialize_options: SerializeOptions) -> Self {
        Self {
            inner,
            serialize_options,
        }
    }

    /// Borrow the upstream `oraclemcp-db` connection.
    #[must_use]
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Borrow the serialization caps used for query delegation.
    #[must_use]
    pub fn serialize_options(&self) -> &SerializeOptions {
        &self.serialize_options
    }
}

/// Serialization options for catalog extraction queries.
///
/// `oraclemcp-db`'s defaults cap CLOBs and BLOBs for agent-facing query
/// responses. Catalog extraction consumes `DBMS_METADATA.GET_DDL`,
/// `ALL_SOURCE`, and similar metadata text where truncation corrupts the
/// resulting snapshot, so the MCP-side catalog adapter asks upstream to read
/// complete LOB locator values.
#[must_use]
pub fn catalog_extraction_serialize_options() -> SerializeOptions {
    SerializeOptions {
        max_lob_chars: usize::MAX,
        max_blob_bytes: usize::MAX,
        ..SerializeOptions::default()
    }
}

impl OraclemcpCatalogConnection<oraclemcp_db::RustOracleConnection> {
    /// Open a real pure-Rust thin Oracle connection via `oraclemcp-db`.
    pub async fn connect(
        cx: &Cx,
        options: oraclemcp_db::OracleConnectOptions,
    ) -> Result<Self, CatalogError> {
        let connection = oraclemcp_db::RustOracleConnection::connect(cx, options)
            .await
            .map_err(map_db_error)?;
        Ok(Self::new(connection))
    }
}

impl<C> OraclemcpCatalogConnection<C>
where
    C: oraclemcp_db::OracleConnection,
{
    /// The catalog-facing backend identifier for this adapter.
    #[must_use]
    pub fn backend(&self) -> CatalogBackend {
        map_backend(self.inner.backend())
    }

    /// Round-trip the upstream connection.
    pub async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
        self.inner.ping(cx).await.map_err(map_db_error)
    }

    /// Return catalog-shaped connection metadata.
    pub async fn describe(&self, cx: &Cx) -> Result<CatalogConnectionInfo, CatalogError> {
        self.inner
            .describe(cx)
            .await
            .map(map_connection_info)
            .map_err(map_db_error)
    }

    /// Run a positional-bind query and return catalog-shaped rows.
    pub async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<Vec<CatalogRow>, CatalogError> {
        let upstream_binds = map_binds(params)?;
        self.inner
            .query_rows_with_serialize_options(cx, sql, &upstream_binds, &self.serialize_options)
            .await
            .map(map_rows)
            .map_err(map_db_error)
    }

    /// Run a positional-bind statement through the upstream connection.
    pub async fn execute(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<u64, CatalogError> {
        let upstream_binds = map_binds(params)?;
        self.inner
            .execute(cx, sql, &upstream_binds)
            .await
            .map_err(map_db_error)
    }

    /// Build a synchronous guard oracle from live dictionary facts for the
    /// objects the upstream classifier resolved from the statement.
    pub async fn side_effect_oracle(
        &self,
        cx: &Cx,
        base_objects: &[ObjectRef],
    ) -> Result<CatalogSideEffectOracle, CatalogError> {
        let current_schema = self.describe(cx).await?.current_schema;
        let mut oracle = CatalogSideEffectOracle::default();
        oracle.set_default_schema(current_schema.clone());
        for key in resolved_object_keys(base_objects, current_schema.as_deref()) {
            oracle.mark_checked(key.clone());
            if self.object_has_live_side_effect_fact(cx, &key).await? {
                oracle.mark_side_effecting(key);
            }
        }
        Ok(oracle)
    }

    async fn object_has_live_side_effect_fact(
        &self,
        cx: &Cx,
        key: &ObjectKey,
    ) -> Result<bool, CatalogError> {
        // oraclemcp-db 0.4.1 currently decides query prefetch from the first
        // SQL keyword. Keep the metadata probe SELECT-led while preserving the
        // CTE shape Oracle accepts.
        let sql = "
select *
from (
with resolved_objects as (
  select :1 as owner, :2 as name
  from dual
  union
  select table_owner as owner, table_name as name
  from all_synonyms
  where synonym_name = :2
    and owner in (:1, 'PUBLIC')
    and table_owner is not null
    and table_name is not null
)
select
  case
    when exists (
      select 1
      from all_policies
      where (object_owner, object_name) in (
        select owner, name from resolved_objects
      )
        and upper(enable) in ('Y', 'YES', 'TRUE', '1')
        and upper(sel) in ('Y', 'YES', 'TRUE', '1')
    )
    or exists (
      select 1
      from all_triggers
      where (table_owner, table_name) in (
        select owner, name from resolved_objects
      )
        and upper(status) = 'ENABLED'
        and base_object_type in ('TABLE', 'VIEW')
    )
    then 1
    else 0
  end as side_effecting
from dual
)
";
        let rows = self
            .query_rows(
                cx,
                sql,
                &[
                    CatalogBind::from(key.owner.clone()),
                    CatalogBind::from(key.name.clone()),
                ],
            )
            .await?;
        let Some(row) = rows.first() else {
            return Ok(false);
        };
        row.parse_u64("SIDE_EFFECTING").map(|count| count > 0)
    }
}

#[async_trait::async_trait(?Send)]
impl<C> CatalogOracleConnection for OraclemcpCatalogConnection<C>
where
    C: oraclemcp_db::OracleConnection,
{
    fn backend(&self) -> CatalogBackend {
        OraclemcpCatalogConnection::backend(self)
    }

    async fn ping(&self, cx: &Cx) -> Result<(), CatalogError> {
        OraclemcpCatalogConnection::ping(self, cx).await
    }

    async fn describe(&self, cx: &Cx) -> Result<CatalogConnectionInfo, CatalogError> {
        OraclemcpCatalogConnection::describe(self, cx).await
    }

    async fn query_rows(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<Vec<CatalogRow>, CatalogError> {
        OraclemcpCatalogConnection::query_rows(self, cx, sql, params).await
    }

    async fn execute(
        &self,
        cx: &Cx,
        sql: &str,
        params: &[CatalogBind],
    ) -> Result<u64, CatalogError> {
        OraclemcpCatalogConnection::execute(self, cx, sql, params).await
    }
}

fn map_backend(backend: oraclemcp_db::OracleBackend) -> CatalogBackend {
    match backend {
        oraclemcp_db::OracleBackend::RustOracle => CatalogBackend::OracleRs,
        _ => CatalogBackend::OracleRs,
    }
}

fn map_connection_info(info: oraclemcp_db::OracleConnectionInfo) -> CatalogConnectionInfo {
    CatalogConnectionInfo {
        backend: info
            .backend
            .map(map_backend)
            .unwrap_or(CatalogBackend::OracleRs),
        // `oraclemcp-db` deliberately keeps connect material out of
        // `OracleConnectionInfo`; Phase D's live runtime owns named profile
        // state and can fill this from the profile registry when needed.
        connect_string: String::new(),
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

fn map_binds(params: &[CatalogBind]) -> Result<Vec<oraclemcp_db::OracleBind>, CatalogError> {
    params
        .iter()
        .map(|param| match param {
            CatalogBind::String(value) => Ok(oraclemcp_db::OracleBind::String(value.clone())),
            CatalogBind::I64(value) => Ok(oraclemcp_db::OracleBind::I64(*value)),
            CatalogBind::U64(value) => {
                let signed =
                    i64::try_from(*value).map_err(|_| CatalogError::InvalidColumnValue {
                        column: String::from("bind"),
                        expected: "u64 <= i64::MAX for oraclemcp-db positional bind",
                        value: value.to_string(),
                    })?;
                Ok(oraclemcp_db::OracleBind::I64(signed))
            }
            CatalogBind::Bool(value) => Ok(oraclemcp_db::OracleBind::Bool(*value)),
        })
        .collect()
}

fn map_rows(rows: Vec<oraclemcp_db::OracleRow>) -> Vec<CatalogRow> {
    rows.into_iter().map(map_row).collect()
}

fn map_row(row: oraclemcp_db::OracleRow) -> CatalogRow {
    let mut mapped = CatalogRow::default();
    for (name, cell) in row.columns {
        mapped.columns.insert(
            name.to_ascii_uppercase(),
            CatalogCell::new(cell.oracle_type, cell.value),
        );
    }
    mapped
}

fn map_db_error(err: oraclemcp_db::DbError) -> CatalogError {
    CatalogError::OracleBackendError {
        backend: CatalogBackend::OracleRs,
        message: err.to_string(),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogSideEffectOracle {
    default_schema: Option<String>,
    checked_objects: HashSet<ObjectKey>,
    side_effecting_objects: HashSet<ObjectKey>,
}

impl CatalogSideEffectOracle {
    fn set_default_schema(&mut self, schema: Option<String>) {
        self.default_schema = schema;
    }

    fn mark_checked(&mut self, key: ObjectKey) {
        self.checked_objects.insert(key);
    }

    fn mark_side_effecting(&mut self, key: ObjectKey) {
        self.side_effecting_objects.insert(key);
    }

    #[must_use]
    pub fn has_side_effecting_object(&self) -> bool {
        !self.side_effecting_objects.is_empty()
    }
}

impl SideEffectOracle for CatalogSideEffectOracle {
    fn statement_purity(&self, base_objects: &[ObjectRef]) -> Purity {
        let mut all_checked = !base_objects.is_empty();
        for object in base_objects {
            let Some(key) = ObjectKey::from_ref(object, self.default_schema.as_deref()) else {
                all_checked = false;
                continue;
            };
            if self.side_effecting_objects.contains(&key) {
                return Purity::ProvenSideEffecting;
            }
            if !self.checked_objects.contains(&key) {
                all_checked = false;
            }
        }
        if all_checked {
            Purity::ProvenReadOnly
        } else {
            Purity::Unknown
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ObjectKey {
    owner: String,
    name: String,
}

impl ObjectKey {
    fn from_ref(object: &ObjectRef, default_schema: Option<&str>) -> Option<Self> {
        let owner = match object.schema.as_deref() {
            Some(schema) => normalize_identifier(schema),
            None => default_schema?.to_string(),
        };
        let name = normalize_identifier(&object.name);
        (!owner.is_empty() && !name.is_empty()).then_some(Self { owner, name })
    }
}

fn resolved_object_keys(
    base_objects: &[ObjectRef],
    current_schema: Option<&str>,
) -> Vec<ObjectKey> {
    let mut seen = HashSet::new();
    let mut keys = Vec::new();
    for object in base_objects {
        let owner = match object.schema.as_deref() {
            Some(schema) => Some(normalize_identifier(schema)),
            None => current_schema.map(ToString::to_string),
        };
        let name = normalize_identifier(&object.name);
        let Some(owner) = owner else {
            continue;
        };
        if owner.is_empty() || name.is_empty() {
            continue;
        }
        let key = ObjectKey { owner, name };
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::RuntimeBuilder;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeDbConnection {
        observed_serialize_options: Mutex<Option<SerializeOptions>>,
        observed_sql: Mutex<Vec<String>>,
        side_effecting_objects: Mutex<HashSet<(String, String)>>,
    }

    impl FakeDbConnection {
        fn with_side_effecting_object(owner: &str, name: &str) -> Self {
            let connection = Self::default();
            connection
                .side_effecting_objects
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert((normalize_identifier(owner), normalize_identifier(name)));
            connection
        }

        fn record_serialize_options(&self, serialize_options: &SerializeOptions) {
            let mut observed = self
                .observed_serialize_options
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *observed = Some(*serialize_options);
        }

        fn observed_serialize_options(&self) -> Option<SerializeOptions> {
            *self
                .observed_serialize_options
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        fn observed_sql(&self) -> Vec<String> {
            self.observed_sql
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    fn run_async<F, T>(f: F) -> T
    where
        F: Future<Output = T>,
    {
        let runtime = RuntimeBuilder::current_thread()
            .build()
            .expect("current-thread runtime");
        runtime.block_on(async {
            let _ = Cx::current().expect("block_on installs a request Cx");
            f.await
        })
    }

    #[async_trait::async_trait(?Send)]
    impl oraclemcp_db::OracleConnection for FakeDbConnection {
        fn backend(&self) -> oraclemcp_db::OracleBackend {
            oraclemcp_db::OracleBackend::RustOracle
        }

        async fn ping(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn describe(
            &self,
            _cx: &Cx,
        ) -> Result<oraclemcp_db::OracleConnectionInfo, oraclemcp_db::DbError> {
            Ok(oraclemcp_db::OracleConnectionInfo {
                backend: Some(oraclemcp_db::OracleBackend::RustOracle),
                connection_strategy: Some(String::from("single_session")),
                server_version: Some(String::from("23ai")),
                current_schema: Some(String::from("BILLING")),
                database_role: Some(String::from("PRIMARY")),
                ..oraclemcp_db::OracleConnectionInfo::default()
            })
        }

        async fn query_rows(
            &self,
            _cx: &Cx,
            sql: &str,
            binds: &[oraclemcp_db::OracleBind],
        ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
            self.observed_sql
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(String::from(sql));
            if sql.contains("from all_policies") {
                let (owner, name) = match binds {
                    [
                        oraclemcp_db::OracleBind::String(owner),
                        oraclemcp_db::OracleBind::String(name),
                    ] => (owner.clone(), name.clone()),
                    _ => {
                        return Err(oraclemcp_db::DbError::Query(String::from(
                            "side-effect query expected owner/name string binds",
                        )));
                    }
                };
                let side_effecting = self
                    .side_effecting_objects
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .contains(&(owner, name));
                return Ok(vec![oraclemcp_db::OracleRow {
                    columns: vec![(
                        String::from("SIDE_EFFECTING"),
                        oraclemcp_db::OracleCell::new(
                            "NUMBER",
                            Some(if side_effecting { "1" } else { "0" }.to_string()),
                        ),
                    )],
                }]);
            }
            assert_eq!(sql, "select :1, :2, :3 from dual");
            assert_eq!(
                binds,
                &[
                    oraclemcp_db::OracleBind::String(String::from("BILLING")),
                    oraclemcp_db::OracleBind::I64(42),
                    oraclemcp_db::OracleBind::Bool(true),
                ]
            );
            Ok(vec![oraclemcp_db::OracleRow {
                columns: vec![
                    (
                        String::from("owner"),
                        oraclemcp_db::OracleCell::new("VARCHAR2", Some(String::from("BILLING"))),
                    ),
                    (
                        String::from("object_count"),
                        oraclemcp_db::OracleCell::new("NUMBER", Some(String::from("42"))),
                    ),
                    (
                        String::from("source_text"),
                        oraclemcp_db::OracleCell::new("CLOB", Some(String::from("body"))),
                    ),
                ],
            }])
        }

        async fn query_rows_with_serialize_options(
            &self,
            cx: &Cx,
            sql: &str,
            binds: &[oraclemcp_db::OracleBind],
            serialize_opts: &SerializeOptions,
        ) -> Result<Vec<oraclemcp_db::OracleRow>, oraclemcp_db::DbError> {
            self.record_serialize_options(serialize_opts);
            self.query_rows(cx, sql, binds).await
        }

        async fn execute(
            &self,
            _cx: &Cx,
            _sql: &str,
            _binds: &[oraclemcp_db::OracleBind],
        ) -> Result<u64, oraclemcp_db::DbError> {
            Ok(1)
        }

        async fn commit(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }

        async fn rollback(&self, _cx: &Cx) -> Result<(), oraclemcp_db::DbError> {
            Ok(())
        }
    }

    #[test]
    fn adapter_maps_oraclemcp_rows_to_catalog_rows() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection::default());
            let rows = adapter
                .query_rows(
                    &cx,
                    "select :1, :2, :3 from dual",
                    &[
                        CatalogBind::from("BILLING"),
                        CatalogBind::from(42_u64),
                        CatalogBind::from(true),
                    ],
                )
                .await
                .expect("query rows");

            assert_eq!(rows.len(), 1);
            let row = rows.first().expect("adapter should return one row");
            assert_eq!(row.text("OWNER"), Some("BILLING"));
            assert_eq!(row.text("owner"), Some("BILLING"));
            assert_eq!(row.parse_u64("object_count").expect("count"), 42);
            assert_eq!(
                row.cell("source_text").expect("source cell").oracle_type,
                "CLOB"
            );

            let observed_options = adapter
                .inner()
                .observed_serialize_options()
                .expect("adapter should pass serialize options to upstream query");
            assert_eq!(observed_options.max_lob_chars, usize::MAX);
            assert_eq!(observed_options.max_blob_bytes, usize::MAX);
            assert!(observed_options.max_text_chars.is_none());
        });
    }

    #[test]
    fn adapter_maps_connection_metadata_to_catalog_shape() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection::default());
            let info = adapter.describe(&cx).await.expect("describe");

            assert_eq!(info.backend, CatalogBackend::OracleRs);
            assert_eq!(info.current_schema.as_deref(), Some("BILLING"));
            assert_eq!(info.server_version, "23ai");
            assert_eq!(info.server_type, "PRIMARY");
        });
    }

    #[test]
    fn adapter_rejects_u64_binds_that_oraclemcp_db_cannot_represent() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection::default());
            let err = adapter
                .query_rows(&cx, "select :1 from dual", &[CatalogBind::U64(u64::MAX)])
                .await
                .expect_err("out-of-range u64 bind should be rejected");

            assert!(err.to_string().contains("u64 <= i64::MAX"));
        });
    }

    #[test]
    fn catalog_extraction_options_remove_oraclemcp_default_lob_caps() {
        let upstream_defaults = SerializeOptions::default();
        let catalog_options = catalog_extraction_serialize_options();

        assert_eq!(catalog_options.max_lob_chars, usize::MAX);
        assert_eq!(catalog_options.max_blob_bytes, usize::MAX);
        assert!(catalog_options.max_lob_chars > upstream_defaults.max_lob_chars);
        assert!(catalog_options.max_blob_bytes > upstream_defaults.max_blob_bytes);
    }

    #[test]
    fn explicit_serialize_options_override_catalog_extraction_defaults() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let explicit_options = SerializeOptions {
                max_lob_chars: 8,
                max_blob_bytes: 16,
                ..SerializeOptions::default()
            };
            let adapter = OraclemcpCatalogConnection::with_serialize_options(
                FakeDbConnection::default(),
                explicit_options,
            );

            let _ = adapter
                .query_rows(
                    &cx,
                    "select :1, :2, :3 from dual",
                    &[
                        CatalogBind::from("BILLING"),
                        CatalogBind::from(42_u64),
                        CatalogBind::from(true),
                    ],
                )
                .await
                .expect("query rows");

            let observed_options = adapter
                .inner()
                .observed_serialize_options()
                .expect("adapter should pass explicit serialize options upstream");
            assert_eq!(observed_options.max_lob_chars, 8);
            assert_eq!(observed_options.max_blob_bytes, 16);
        });
    }

    #[test]
    fn side_effect_oracle_marks_live_dictionary_hit_side_effecting() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let connection = FakeDbConnection::with_side_effecting_object("billing", "invoices");
            let adapter = OraclemcpCatalogConnection::new(connection);
            let oracle = adapter
                .side_effect_oracle(&cx, &[ObjectRef::new(None, "invoices")])
                .await
                .expect("side-effect oracle");

            assert!(oracle.has_side_effecting_object());
            assert_eq!(
                oracle.statement_purity(&[ObjectRef::new(None, "invoices")]),
                Purity::ProvenSideEffecting
            );
            assert!(
                adapter
                    .inner()
                    .observed_sql()
                    .iter()
                    .any(|sql| sql.contains("from all_policies")),
                "live dictionary query must consult VPD policy facts"
            );
            assert!(
                adapter
                    .inner()
                    .observed_sql()
                    .iter()
                    .any(|sql| sql.contains("from all_synonyms")),
                "live dictionary query must account for synonym targets"
            );
        });
    }

    #[test]
    fn side_effect_oracle_preserves_quoted_identifier_case() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let connection = FakeDbConnection::with_side_effecting_object(
                "\"MixedOwner\"",
                "\"Billing\"\"Pkg\"",
            );
            let adapter = OraclemcpCatalogConnection::new(connection);
            let object = ObjectRef::new(Some(String::from("\"MixedOwner\"")), "\"Billing\"\"Pkg\"");
            let oracle = adapter
                .side_effect_oracle(&cx, std::slice::from_ref(&object))
                .await
                .expect("side-effect oracle");

            assert!(oracle.has_side_effecting_object());
            assert_eq!(
                oracle.statement_purity(&[object]),
                Purity::ProvenSideEffecting
            );
            let observed_sql = adapter.inner().observed_sql().join("\n");
            assert!(!observed_sql.contains("upper(object_owner)"));
            assert!(!observed_sql.contains("upper(object_name)"));
            assert!(!observed_sql.contains("upper(table_owner)"));
            assert!(!observed_sql.contains("upper(table_name)"));
            assert!(!observed_sql.contains("upper(synonym_name)"));
        });
    }

    #[test]
    fn side_effect_oracle_marks_checked_clean_object_read_only() {
        run_async(async {
            let cx = Cx::current().expect("test runtime installs Cx");
            let adapter = OraclemcpCatalogConnection::new(FakeDbConnection::default());
            let oracle = adapter
                .side_effect_oracle(&cx, &[ObjectRef::new(None, "invoices")])
                .await
                .expect("side-effect oracle");

            assert!(!oracle.has_side_effecting_object());
            assert_eq!(
                oracle.statement_purity(&[ObjectRef::new(None, "invoices")]),
                Purity::ProvenReadOnly
            );
        });
    }
}
