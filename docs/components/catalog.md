# `plsql-catalog` — Oracle Catalog Snapshot

Layer 1.5 of the `plsql-intelligence` workspace. Provides an offline-first model of Oracle
dictionary metadata that downstream crates (symbols, privileges, SQL semantics, dependency
graph, lineage, SAST) consume to resolve names, classify grants, and cross-check edges.

**Crate path:** `crates/plsql-catalog/`

---

## 1. Design philosophy

### 1.1 Offline-first

The parser never requires a database connection. Semantic analysis may optionally consume a
catalog. When a catalog is absent, downstream analysis correctly degrades and records
`UnknownReason::MissingCatalogObject` on every inference that would have benefited from
dictionary metadata.

This is not a compromise — it is a product feature. Many customers will run the engine
against exported source without granting database access. The tool must still produce
useful output in that mode.

### 1.2 Snapshot-based, not live-polled

The primary data model is `CatalogSnapshot` — a serializable, self-describing JSON document
that captures the structural shape of one or more Oracle schemas at a point in time. The
snapshot carries its own `SymbolInterner` so exported JSON remains reloadable without ambient
process state.

Three ingestion paths produce the same `CatalogSnapshot` type:

| Path | Function | Source |
|------|----------|--------|
| JSON snapshot | `load_snapshot_from_json()` | Pre-exported `.json` file |
| Live connection | `load_snapshot_from_connection()` | Oracle DB via `OracleConnection` trait |
| DBMS_METADATA files | `load_from_dbms_metadata_dir()` | Directory of `.sql` DDL exports |

All three paths produce identical downstream behavior — the rest of the engine does not
know or care which path was used.

### 1.3 Structural, not row-level

The model captures object shape (columns, arguments, signatures, grants, synonyms,
dependencies) — not row data. This is intentional: the engine analyzes code structure,
not query results.

---

## 2. Snapshot schema

### 2.1 `CatalogSnapshot`

The top-level container:

```
CatalogSnapshot
├── schemas: HashMap<SchemaName, SchemaCatalog>
├── profile: AnalysisProfile
├── capabilities: CatalogCapabilities
├── generated_at: DateTime<Utc>
├── source: CatalogSource
└── interner: SymbolInterner
```

The `interner` field is critical — all identifier names (`SchemaName`, `ObjectName`,
`ColumnName`, etc.) are interned `SymbolId` wrappers. The serialized snapshot must carry
its symbol table so it round-trips correctly.

### 2.2 `SchemaCatalog`

Per-schema metadata:

```
SchemaCatalog
├── objects: HashMap<ObjectName, CatalogObject>
├── synonyms: HashMap<SynonymName, SynonymTarget>
├── grants: Vec<Grant>
├── indexes: HashMap<IndexName, IndexMetadata>
├── constraints: HashMap<ConstraintName, ConstraintMetadata>
├── triggers: HashMap<TriggerName, TriggerMetadata>
├── dependencies: Vec<CatalogDependency>
└── plscope: Option<PlScopeSnapshot>
```

### 2.3 `CatalogObject` variants

Every schema object is one of:

| Variant | Key fields |
|---------|-----------|
| `Table` | columns, temporary, index-organized |
| `View` | columns, query_hash, read_only |
| `MaterializedView` | columns, refresh mode |
| `Sequence` | min/max/increment/cache |
| `Type` | attributes, finality, instantiable |
| `Package` | procedures, functions, authid, accessible_by, stateful |
| `Procedure` | signature (routine_name, arguments, return_type, authid) |
| `Function` | signature, deterministic, pipelined |
| `Trigger` | target table, timing, level, events |
| `SchedulerJob` | job_type, schedule, enabled |
| `EditioningView` | columns, edition |

All variants share `ObjectCommon`:

```rust
pub struct ObjectCommon {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub object_type: ObjectType,
    pub status: ObjectStatus,        // VALID / INVALID / N/A
    pub edition_name: Option<EditionName>,
    pub editionable: Option<bool>,
    pub last_ddl_time: Option<DateTime<Utc>>,
    pub source_hash: Option<Hash>,
    pub ddl: Option<DbmsMetadataDdl>,
}
```

### 2.4 Columns and data types

```rust
pub struct ColumnMetadata {
    pub name: ColumnName,
    pub position: u32,
    pub data_type: DataTypeRef,
    pub nullable: bool,
    pub default_expression: Option<String>,
    pub generated_expression: Option<String>,
    pub hidden: bool,
}

pub struct DataTypeRef {
    pub owner: Option<SchemaName>,
    pub name: String,            // "NUMBER", "VARCHAR2", etc.
    pub length: Option<u32>,
    pub precision: Option<u32>,
    pub scale: Option<i32>,
    pub char_semantics: Option<String>,
}
```

### 2.5 Routine signatures

Packages, procedures, and functions use `RoutineSignature`:

```rust
pub struct RoutineSignature {
    pub routine_name: ObjectName,
    pub overload: Option<u32>,
    pub arguments: Vec<ArgumentMetadata>,
    pub return_type: Option<DataTypeRef>,
    pub authid_current_user: Option<bool>,  // true = AUTHID CURRENT_USER
    pub accessible_by: Vec<AccessibleByTarget>,
}
```

Arguments carry mode (`IN`, `OUT`, `IN OUT`, `RETURN`), data type, default presence,
and position.

### 2.6 Synonyms

```rust
pub struct SynonymTarget {
    pub target_owner: Option<SchemaName>,
    pub target_name: ObjectName,
    pub target_type: Option<ObjectType>,
    pub db_link: Option<String>,
    pub public_synonym: bool,
}
```

Chained synonyms are resolved at symbol-resolution time (Layer 2), not at catalog load time.

### 2.7 Grants

```rust
pub struct Grant {
    pub object_owner: SchemaName,
    pub object_name: ObjectName,
    pub privilege: GrantPrivilege,    // Select, Insert, Update, Delete, Execute, ...
    pub grantee: Grantee,            // User(name), Role(name), Public
    pub grantable: bool,
    pub via_role: Option<RoleName>,
    pub with_hierarchy: bool,
}
```

### 2.8 Dependencies

```rust
pub struct CatalogDependency {
    pub owner: SchemaName,
    pub name: ObjectName,
    pub referenced_owner: Option<SchemaName>,
    pub referenced_name: Option<ObjectName>,
    pub dependency_type: CatalogDependencyKind,  // Hard, Soft, External
}
```

These come from `ALL_DEPENDENCIES` / `USER_DEPENDENCIES` and serve as a comparison source
for the dependency graph — not as ground truth.

### 2.9 Indexes and constraints

Indexes carry table owner/name, uniqueness, column list, and index type. Constraints carry
type (PK, FK, UK, Check, FK), search condition, and referenced constraint for FKs.

---

## 3. Capability negotiation

Not every customer environment has the same permissions or Oracle features enabled.
`CatalogCapabilities` records what the extraction was able to query:

```rust
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
```

Each warning has a `code`, `message`, and optional `remediation` hint (e.g., "Grant
SELECT_CATALOG_ROLE to improve extraction coverage").

The `doctor` subcommand reports the capability matrix and suggests the minimum grants
needed to improve analysis completeness. This is how the tool tells a DBA "here is what
you can give me to get better results" without silently degrading.

---

## 4. UnknownReason types relevant to catalog

The `plsql-core` crate defines `UnknownReason` — every gap in analysis becomes a typed
record with provenance. Catalog-relevant variants:

| Variant | When emitted |
|---------|-------------|
| `MissingCatalogObject` | A name references an object not in the snapshot |
| `MissingPackageBody` | Spec exists but body was not provided |
| `WrappedSource` | Object source is wrapped (DBMS_DDL.WRAP) |
| `DbLinkRemoteObject` | Object is on a remote DB link |
| `EditionedObject` | Object participates in edition-based redefinition |
| `RuntimeGrantOrRole` | Authorization depends on runtime role state |
| `InvokerRightsRuntimeResolution` | AUTHID CURRENT_USER dispatch is ambiguous |
| `ConditionalCompilationBranch` | Object compiled under different CC flags |

None of these are silently dropped. Every downstream crate that encounters one of these
conditions records it in its output (dependency edges, privilege model, symbol table,
fact store).

---

## 5. PL/Scope integration

### 5.1 What PL/Scope provides

PL/Scope is Oracle's compile-time identifier and statement metadata, exposed through
`ALL_IDENTIFIERS` / `USER_IDENTIFIERS` / `DBA_IDENTIFIERS`. Available since Oracle 11g.
Controlled by `PLSCOPE_SETTINGS`.

When enabled, PL/Scope provides:
- Every identifier declaration and reference with line/column
- Identifier types (variable, constant, label, subprogram, etc.)
- Usage kinds (declaration, definition, reference, call)
- SQL statement classification

### 5.2 Availability model

PL/Scope is **never assumed to be available**. It requires `PLSCOPE_SETTINGS` to be set
before recompilation. The catalog records availability as:

```rust
pub enum PlScopeAvailability {
    NotAvailable,           // PL/Scope not enabled or no data
    AvailableButStale,      // Data exists but compile timestamps don't match
    IdentifiersOnly,        // PLSCOPE_SETTINGS = 'IDENTIFIERS:ALL'
    IdentifiersAndStatements, // PLSCOPE_SETTINGS = 'IDENTIFIERS:ALL, STATEMENTS:ALL'
}
```

### 5.3 Snapshot structure

```rust
pub struct PlScopeSnapshot {
    pub availability: PlScopeAvailability,
    pub identifiers: Vec<CompilerIdentifier>,
    pub references: Vec<CompilerReference>,
    pub statements: Vec<CompilerStatementUsage>,
    pub collected_at: Option<DateTime<Utc>>,
    pub source_hash: Option<Hash>,
    pub warnings: Vec<CapabilityWarning>,
}
```

### 5.4 Strategic value

PL/Scope is Oracle's own compiler emitting "where this identifier was used" data.
Using it as a differential test source means we can prove our symbol resolver against
Oracle's own compiler output. This is the most credible validation strategy available
to a third-party PL/SQL analyzer.

The PL/Scope diff (Layer 2, `PLSCOPE-DIFF-001/002`) compares our `plsql-symbols`
resolution against PL/Scope references and surfaces missed references, spurious
references, and kind mismatches.

---

## 6. Loading paths

### 6.1 JSON snapshot

```rust
pub fn load_snapshot_from_json(path: &Path) -> Result<CatalogSnapshot, CatalogError>;
pub fn export_snapshot_to_json(snapshot: &CatalogSnapshot, path: &Path) -> Result<(), CatalogError>;
```

Validates schema ID (`plsql.catalog.snapshot`) and schema version (currently 1.1.0)
before accepting. This is the path used by CI, tests, and the no-database demo workflow.

### 6.2 Live connection

```rust
pub fn load_snapshot_from_connection<C: OracleConnection>(
    conn: &C,
    request: &CatalogLoadRequest,
) -> Result<CatalogSnapshot, CatalogError>;
```

Queries `ALL_*` / `DBA_*` dictionary views, `DBMS_METADATA`, and PL/Scope tables.
The `CatalogLoadRequest` specifies which schemas to extract:

```rust
pub struct CatalogLoadRequest {
    pub schema_filters: Vec<CatalogSchemaFilter>,
}

pub enum CatalogSchemaFilter {
    CurrentSchema,
    Named(String),      // text-backed, safe across CLI/JSON boundaries
}
```

The `OracleConnection` trait abstracts the database driver. The first implementation
uses the `oracle` crate (Rust Oracle driver) behind the `oracle-driver` Cargo feature.

### 6.3 DBMS_METADATA files

```rust
pub fn load_from_dbms_metadata_dir(dir: &Path) -> Result<CatalogSnapshot, CatalogError>;
```

Scans a directory for `.sql` files and classifies DDL statements using keyword matching.
Handles `CREATE TABLE`, `VIEW`, `PACKAGE`, `PROCEDURE`, `FUNCTION`, `SEQUENCE`,
`TRIGGER`, and `TYPE`. Unclassifiable files produce info-level diagnostics, not errors.

This path exists for customers who export DDL via `DBMS_METADATA.GET_DDL` and want
offline analysis without a live connection.

### 6.4 Synthetic test catalog

The `synthetic` module provides `SyntheticCatalogBuilder` for constructing test fixtures:

```rust
let mut builder = SyntheticCatalogBuilder::new("BILLING");
builder.add_table("CUSTOMERS", vec![("ID", "NUMBER", false), ("NAME", "VARCHAR2", false)]);
builder.add_package("BILLING_API", false, vec![]);
builder.add_grant(obj, GrantPrivilege::Select, Grantee::Role(role), false);
let snapshot = builder.build();
```

The `billing_schema()` function produces a canonical hero-demo estate used across
the test suite.

---

## 7. Error handling

All catalog operations return `Result<T, CatalogError>`:

| Variant | Meaning |
|---------|---------|
| `Io` | File I/O failure |
| `Json` | Serialization/deserialization failure |
| `OracleBackendNotCompiled` | Requested backend not enabled via Cargo feature |
| `OracleBackendError` | Runtime Oracle driver error |
| `UnexpectedRowCount` | Query returned wrong cardinality |
| `MissingColumn` / `NullColumnValue` | Expected column absent or null |
| `InvalidColumnValue` | Column value failed type conversion |
| `UnsupportedSchemaVersion` | Snapshot version mismatch |
| `UnexpectedSchemaId` | Snapshot schema ID mismatch |
| `CurrentSchemaUnavailable` | Connection couldn't determine current schema |
| `InvalidSchemaFilter` | Blank schema name in filter |

---

## 8. Versioned envelope

The JSON snapshot uses a versioned envelope (`CatalogSnapshotDocument`) with:

- `schema_id`: `"plsql.catalog.snapshot"`
- `schema_version`: `SchemaVersion(1, 1, 0)`
- `snapshot`: the actual `CatalogSnapshot`

Consumers validate both fields before accepting a snapshot. This prevents silent
mismatches when a snapshot format evolves across releases.

---

## 9. Downstream consumers

| Consumer | What it uses |
|----------|-------------|
| `plsql-symbols` | `%TYPE`/`%ROWTYPE` resolution, overload signatures, synonym chains |
| `plsql-privileges` | Grants, roles, AUTHID, ACCESSIBLE BY |
| `plsql-sqlsem` | Table/column metadata for embedded SQL analysis |
| `plsql-depgraph` | `ALL_DEPENDENCIES` cross-check, trigger/constraint edges |
| `plsql-lineage` | Column-level lineage precision, dynamic-SQL evidence |
| `plsql-scan` | SAST rules that need catalog context (SEC004, PERF001) |
| `plsql-engine` | Capability report in `CompletenessReport` |

---

## 10. Testing

The catalog crate has 15 unit tests covering:

- JSON round-trip with schema version validation
- Structural lookup maps (objects, columns, arguments, synonyms, grants)
- PL/Scope snapshot model
- Package state and AUTHID flags
- Oracle connection trait behavior (mock-based)
- DBMS_METADATA directory loading with DDL classification
- Synthetic catalog builder

All tests run without a live Oracle database — fixtures use in-memory mock connections
and synthetic data.

---

## 11. Future work

| Bead | Description |
|------|-------------|
| `CAT-010` | PL/Scope capability detection via `PLSCOPE_SETTINGS` |
| `CAT-011` | Extract `ALL_IDENTIFIERS` into `PlScopeSnapshot` |
| `CAT-014` | Extract object status, edition, dependency rows |
| `CAT-015` | `DBMS_METADATA.GET_DDL` XML form extraction |
| `CAT-017` | Capability negotiation + grant-suggestion diagnostics in `doctor` |
