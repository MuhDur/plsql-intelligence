//! Synthetic test catalog builder for creating realistic test fixtures.

use std::collections::HashMap;

use chrono::Utc;
use plsql_core::{ColumnName, ObjectName, RoleName, SchemaName, SymbolId, SymbolInterner};

use crate::{
    AccessibleByTarget, ArgumentMetadata, CatalogCapabilities, CatalogObject, CatalogSnapshot,
    CatalogSource, CatalogSourceKind, ColumnMetadata, DataTypeRef, Grant, GrantPrivilege, Grantee,
    ObjectCommon, ObjectStatus, ObjectType, PackageMetadata, ProcedureMetadata, RoutineSignature,
    SchemaCatalog, SequenceMetadata, SynonymTarget, TableMetadata, TriggerEvent, TriggerLevel,
    TriggerMetadata, TriggerTiming, ViewMetadata,
};

/// Builder for constructing a synthetic `CatalogSnapshot` for testing.
#[derive(Debug)]
pub struct SyntheticCatalogBuilder {
    interner: SymbolInterner,
    schemas: HashMap<SchemaName, SchemaCatalog>,
    current_schema: SchemaName,
}

impl SyntheticCatalogBuilder {
    /// Create a new builder with a default schema.
    pub fn new(schema_name: &str) -> Self {
        let mut interner = SymbolInterner::default();
        let schema = SchemaName::new(interner.intern(schema_name).unwrap());
        let mut schemas = HashMap::new();
        schemas.insert(schema, SchemaCatalog::default());

        Self {
            interner,
            schemas,
            current_schema: schema,
        }
    }

    /// Get the current schema name.
    pub fn current_schema(&self) -> SchemaName {
        self.current_schema
    }

    /// Get a reference to the symbol interner.
    pub fn interner(&self) -> &SymbolInterner {
        &self.interner
    }

    /// Intern a string and return the symbol ID.
    fn intern(&mut self, s: &str) -> SymbolId {
        self.interner.intern(s).unwrap()
    }

    /// Add a second schema to the snapshot.
    pub fn add_schema(&mut self, name: &str) -> SchemaName {
        let schema = SchemaName::new(self.intern(name));
        self.schemas.insert(schema, SchemaCatalog::default());
        schema
    }

    /// Add a table to the current schema.
    pub fn add_table(&mut self, name: &str, columns: Vec<(&str, &str, bool)>) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));
        let col_map: HashMap<ColumnName, ColumnMetadata> = columns
            .into_iter()
            .enumerate()
            .map(|(i, (col_name, data_type, nullable))| {
                let cn = ColumnName::new(self.intern(col_name));
                (
                    cn,
                    ColumnMetadata {
                        name: cn,
                        position: i as u32 + 1,
                        data_type: DataTypeRef {
                            name: data_type.to_string(),
                            ..DataTypeRef::default()
                        },
                        nullable,
                        ..ColumnMetadata::default()
                    },
                )
            })
            .collect();

        let table = TableMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::Table,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            columns: col_map,
            ..TableMetadata::default()
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::Table(table));
        obj_name
    }

    /// Add a view to the current schema.
    pub fn add_view(&mut self, name: &str, columns: Vec<(&str, &str, bool)>) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));
        let col_map: HashMap<ColumnName, ColumnMetadata> = columns
            .into_iter()
            .enumerate()
            .map(|(i, (col_name, data_type, nullable))| {
                let cn = ColumnName::new(self.intern(col_name));
                (
                    cn,
                    ColumnMetadata {
                        name: cn,
                        position: i as u32 + 1,
                        data_type: DataTypeRef {
                            name: data_type.to_string(),
                            ..DataTypeRef::default()
                        },
                        nullable,
                        ..ColumnMetadata::default()
                    },
                )
            })
            .collect();

        let view = ViewMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::View,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            columns: col_map,
            ..ViewMetadata::default()
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::View(view));
        obj_name
    }

    /// Add a package to the current schema with optional procedures and functions.
    pub fn add_package(
        &mut self,
        name: &str,
        invoker_rights: bool,
        accessible_by: Vec<(&str, &str)>,
    ) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));

        let access_list: Vec<AccessibleByTarget> = accessible_by
            .into_iter()
            .map(|(owner, obj)| AccessibleByTarget {
                owner: Some(SchemaName::new(self.intern(owner))),
                object_name: ObjectName::new(self.intern(obj)),
            })
            .collect();

        let pkg = PackageMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::Package,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            authid_current_user: Some(invoker_rights),
            accessible_by: access_list,
            ..PackageMetadata::default()
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::Package(pkg));
        obj_name
    }

    /// Add a standalone procedure to the current schema.
    pub fn add_procedure(
        &mut self,
        name: &str,
        invoker_rights: bool,
        args: Vec<ArgumentMetadata>,
    ) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));

        let proc = ProcedureMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::Procedure,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            signature: RoutineSignature {
                routine_name: obj_name,
                authid_current_user: Some(invoker_rights),
                arguments: args,
                ..RoutineSignature::default()
            },
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::Procedure(proc));
        obj_name
    }

    /// Add a sequence to the current schema.
    pub fn add_sequence(&mut self, name: &str) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));

        let seq = SequenceMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::Sequence,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            ..SequenceMetadata::default()
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::Sequence(seq));
        obj_name
    }

    /// Add a trigger to the current schema.
    pub fn add_trigger(
        &mut self,
        name: &str,
        table_name: &str,
        event: TriggerEvent,
        timing: TriggerTiming,
        level: TriggerLevel,
    ) -> ObjectName {
        let obj_name = ObjectName::new(self.intern(name));
        let tbl = ObjectName::new(self.intern(table_name));

        let trigger = TriggerMetadata {
            common: ObjectCommon {
                owner: self.current_schema,
                name: obj_name,
                object_type: ObjectType::Trigger,
                status: ObjectStatus::Valid,
                ..ObjectCommon::default()
            },
            target_owner: self.current_schema,
            target_name: tbl,
            events: vec![event],
            timing,
            level,
            ..TriggerMetadata::default()
        };

        self.current_schema_catalog_mut()
            .objects
            .insert(obj_name, CatalogObject::Trigger(trigger));
        obj_name
    }

    /// Add a grant to the current schema.
    pub fn add_grant(
        &mut self,
        object_name: ObjectName,
        privilege: GrantPrivilege,
        grantee: Grantee,
        grantable: bool,
    ) {
        let owner = self.current_schema;
        self.current_schema_catalog_mut().grants.push(Grant {
            object_owner: owner,
            object_name,
            privilege,
            grantee,
            grantable,
            via_role: None,
            with_hierarchy: false,
        });
    }

    /// Add a synonym in the current schema.
    pub fn add_synonym(
        &mut self,
        name: &str,
        target_schema: SchemaName,
        target_name: &str,
        public: bool,
    ) {
        let syn_id = self.intern(name);
        let _syn_name = ObjectName::new(syn_id);
        let target_obj = ObjectName::new(self.intern(target_name));
        let _owner = self.current_schema;

        self.current_schema_catalog_mut().synonyms.insert(
            crate::SynonymName::new(syn_id),
            SynonymTarget {
                target_owner: Some(target_schema),
                target_name: target_obj,
                target_type: None,
                db_link: None,
                public_synonym: public,
            },
        );
    }

    /// Build the final `CatalogSnapshot`.
    pub fn build(self) -> CatalogSnapshot {
        CatalogSnapshot {
            schemas: self.schemas,
            profile: plsql_core::AnalysisProfile::default(),
            capabilities: CatalogCapabilities {
                can_query_all_views: true,
                can_query_dba_views: false,
                can_use_dbms_metadata: false,
                can_read_source: true,
                plscope_enabled: false,
                can_query_scheduler: false,
                can_query_roles_and_grants: true,
                warnings: vec![],
            },
            generated_at: Utc::now(),
            source: CatalogSource {
                kind: CatalogSourceKind::SyntheticTestCatalog,
                description: Some("Synthetic test catalog".to_string()),
                ..CatalogSource::default()
            },
            interner: self.interner,
            editions: Vec::new(),
        }
    }

    fn current_schema_catalog_mut(&mut self) -> &mut SchemaCatalog {
        self.schemas.get_mut(&self.current_schema).unwrap()
    }
}

/// Create a minimal billing schema for testing — the canonical hero demo estate.
///
/// Tables: customers, invoices, invoice_lines, payments
/// Packages: billing_api, payment_processor
/// Views: v_customer_balance
/// Grants: reader/reader roles
pub fn billing_schema() -> CatalogSnapshot {
    let mut builder = SyntheticCatalogBuilder::new("BILLING");

    // Tables
    let customers = builder.add_table(
        "CUSTOMERS",
        vec![
            ("CUSTOMER_ID", "NUMBER", false),
            ("NAME", "VARCHAR2", false),
            ("EMAIL", "VARCHAR2", true),
            ("LEGACY_SEGMENT", "VARCHAR2", true),
            ("STATUS", "VARCHAR2", false),
        ],
    );

    let invoices = builder.add_table(
        "INVOICES",
        vec![
            ("INVOICE_ID", "NUMBER", false),
            ("CUSTOMER_ID", "NUMBER", false),
            ("AMOUNT", "NUMBER", false),
            ("STATUS", "VARCHAR2", false),
            ("CREATED_DATE", "DATE", false),
        ],
    );

    let invoice_lines = builder.add_table(
        "INVOICE_LINES",
        vec![
            ("LINE_ID", "NUMBER", false),
            ("INVOICE_ID", "NUMBER", false),
            ("DESCRIPTION", "VARCHAR2", false),
            ("QTY", "NUMBER", false),
            ("UNIT_PRICE", "NUMBER", false),
        ],
    );

    let _payments = builder.add_table(
        "PAYMENTS",
        vec![
            ("PAYMENT_ID", "NUMBER", false),
            ("INVOICE_ID", "NUMBER", false),
            ("AMOUNT", "NUMBER", false),
            ("PAYMENT_DATE", "DATE", false),
        ],
    );

    // Views
    let _balance_view = builder.add_view(
        "V_CUSTOMER_BALANCE",
        vec![
            ("CUSTOMER_ID", "NUMBER", false),
            ("NAME", "VARCHAR2", false),
            ("TOTAL_INVOICED", "NUMBER", true),
            ("TOTAL_PAID", "NUMBER", true),
            ("BALANCE", "NUMBER", true),
        ],
    );

    // Packages
    let billing_api = builder.add_package("BILLING_API", false, vec![]);
    let _payment_proc =
        builder.add_package("PAYMENT_PROCESSOR", true, vec![("BILLING", "BILLING_API")]);

    // Standalone procedure
    builder.add_procedure("GENERATE_INVOICE", false, vec![]);

    // Sequence
    builder.add_sequence("INVOICE_SEQ");

    // Grants
    let reader_role = RoleName::new(builder.intern("reader"));
    builder.add_grant(
        customers,
        GrantPrivilege::Select,
        Grantee::Role(reader_role),
        false,
    );
    builder.add_grant(
        invoices,
        GrantPrivilege::Select,
        Grantee::Role(reader_role),
        false,
    );
    builder.add_grant(
        invoice_lines,
        GrantPrivilege::Select,
        Grantee::Role(reader_role),
        false,
    );
    builder.add_grant(billing_api, GrantPrivilege::Execute, Grantee::Public, false);

    builder.build()
}
