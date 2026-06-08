#![forbid(unsafe_code)]

//! Typed semantic intermediate representation (IR) for `plsql-intelligence`.
//!
//! The IR is one step removed from the raw AST emitted by `plsql-parser`:
//! the AST is syntactic, the IR is semantic. Downstream product surfaces
//! (lineage, SAST, docs, bindgen, CI/CD) consume the IR rather than
//! re-walking ASTs, so name resolution, overload selection, and Oracle
//! catalog cross-checking happen in one place.
//!
//! introduces the top-level container types:
//! [`SemanticModel`], [`FileModel`], and [`SchemaModel`].
//! adds the [`Declaration`] enum and its variant payloads in the
//! [`decl`] module. Statement lowering arrives in.

pub mod calls;
pub mod canonical;
pub mod column_edges;
pub mod decl;
pub mod dml_edges;
pub mod expr;
pub mod fact;
pub mod fact_emit;
pub mod flow;
pub mod flow_inter;
pub mod flow_intra;
pub mod flow_query;
pub mod lower;
pub mod recursion_guard;
pub mod sql_columns;
pub mod sql_fact_emit;
pub mod sql_resolve;
pub mod sql_sem;
pub mod stmt;
pub mod table_stub;

/// Whether `b` is a PL/SQL identifier byte (`[A-Za-z0-9_$#]`). Shared by the
/// lexical extractors (`dml_edges`, `sql_resolve`) which tokenize embedded SQL
/// at the byte level; previously copied byte-for-byte in each (oracle-687a.7).
#[must_use]
pub(crate) fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#'
}

pub use calls::{CallContext, CallSite, extract_call_sites, extract_call_sites_bounded};
pub use canonical::{
    CanonicalisationContext, CanonicalisationStats, canonicalize_expr, canonicalize_statements,
};
pub use column_edges::{
    ColumnEdge, ColumnEdgeKind, extract_column_edges, extract_column_edges_for_model,
};
pub use dml_edges::{
    AccessKind, TableAccess, extract_table_accesses, extract_table_accesses_bounded,
};
pub use expr::{Expr, NameRef, UnknownExprReason, lower_expression};
pub use fact::{Fact, FactId, FactKind, FactPayload, FactProvenance, FactStore, mint_fact};
pub use fact_emit::{
    emit_call_facts, emit_declaration_facts, emit_declarations_from, emit_dynamic_sql_facts,
    emit_privilege_facts, emit_reference_facts, emit_unknown_facts,
};
pub use flow::{ConstantValue, StringShape, Taint, TaintCleanser, TaintKind, ValueFlow, ValueSet};
pub use flow_inter::{
    CallEdgeFlow, FlowUnknownFact, InterFlowResult, RoutineFlowSummary, propagate_inter,
};
pub use flow_intra::{FlowEnv, TaintSources, analyze_flow, analyze_flow_bounded};
pub use flow_query::{FlowQuery, TaintAnswer};
pub use lower::{LoweredFile, lower_top_level};
pub use recursion_guard::{MAX_RELOWER_DEPTH, RecursionOutcome};
pub use sql_columns::{extract_columns, extract_columns_for_model};
pub use sql_fact_emit::{emit_sql_use_facts, emit_sql_use_facts_for_model};
pub use sql_resolve::resolve_sql;
pub use sql_sem::{
    AliasBinding, AliasScope, ColumnResolution, ColumnUse, ProjectionItem, SqlSemanticModel,
    SqlSemanticVerb, SqlStatementModel, TableUsageKind, TableUse,
};
pub use stmt::{IfArm, SqlVerb, Statement, UnknownStatementReason, lower_statement_body};
pub use table_stub::DeclLike;

pub use decl::{
    AnchoredType, ColumnDecl, CursorDecl, DeclCommon, DeclKind, Declaration, FunctionDecl,
    IndexDecl, PackageDecl, ParamDecl, ParamMode, ProcedureDecl, SequenceDecl, SynonymDecl,
    TableDecl, TriggerDecl, TypeDecl, TypeRef, VariableDecl, ViewDecl,
};

use std::collections::{BTreeMap, HashMap};

use plsql_catalog::{CatalogSnapshot, SynonymName};
use plsql_core::{Diagnostic, FileId, ObjectId, ObjectName, SchemaName};
use plsql_privileges::PrivilegeModel;
use serde::{Deserialize, Serialize};
use tracing::instrument;

macro_rules! numeric_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
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
        pub struct $name(u64);

        impl $name {
            #[must_use]
            #[instrument(level = "trace")]
            pub fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[must_use]
            #[instrument(level = "trace", skip(self))]
            pub fn get(self) -> u64 {
                self.0
            }
        }
    };
}

numeric_id!(
    DeclId,
    "Stable identity for a semantic declaration (procedure, function, package, type, variable, parameter, cursor, table, view, column, sequence, synonym, index, trigger). The concrete [`Declaration`] enum lands in `PLSQL-IR-002`."
);
numeric_id!(
    StatementId,
    "Stable identity for an IR statement node. The statement enum lands in `PLSQL-IR-004`; the embedded-SQL view in `PLSQL-SQLSEM-001`."
);

/// Top-level container produced by Layer 2 semantic analysis.
///
/// One `SemanticModel` summarizes a complete analysis run over a project:
/// every file's top-level declarations and statements, every schema's
/// objects and synonyms, an optional catalog snapshot, the privilege
/// model, and any diagnostics raised while constructing the IR. Layer 2.5
/// orchestration (`plsql-engine`) embeds this inside `AnalysisRun`, and
/// every product surface consumes it from there.
///
/// Spec: plan.md §9.2.1.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticModel {
    pub files: Vec<FileModel>,
    pub schemas: BTreeMap<SchemaName, SchemaModel>,
    pub catalog: Option<CatalogSnapshot>,
    pub privileges: PrivilegeModel,
    pub diagnostics: Vec<Diagnostic>,
}

impl SemanticModel {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn file(&self, file_id: FileId) -> Option<&FileModel> {
        self.files.iter().find(|f| f.file_id == file_id)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn schema(&self, name: SchemaName) -> Option<&SchemaModel> {
        self.schemas.get(&name)
    }
}

/// Per-source-file IR view.
///
/// `top_level` holds the declarations parsed from this file (package
/// specs, package bodies, standalone routines, types, tables, views,
/// triggers, ...). `statements` holds the file-scoped statements
/// (anonymous blocks, DDL, SQL\*Plus-significant directives). The
/// declarations themselves live in a future `DeclTable`; this struct
/// only carries the IDs into it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileModel {
    pub file_id: FileId,
    pub top_level: Vec<DeclId>,
    pub statements: Vec<StatementId>,
}

impl FileModel {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            top_level: Vec::new(),
            statements: Vec::new(),
        }
    }
}

/// Per-schema IR view.
///
/// `objects` maps an object name (table, view, package, type, sequence,
/// ...) inside this schema to its catalog/IR identity. `synonyms` maps a
/// synonym name to the object it resolves to; private synonyms live with
/// the owning schema, public synonyms live in the synthetic `PUBLIC`
/// schema in [`SemanticModel::schemas`]. The exact semantics of an
/// `ObjectId` (catalog-only vs source-only vs both) is recorded in
/// [](catalog) and [](self).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaModel {
    pub name: SchemaName,
    pub objects: HashMap<ObjectName, ObjectId>,
    pub synonyms: HashMap<SynonymName, ObjectId>,
}

impl SchemaModel {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(name: SchemaName) -> Self {
        Self {
            name,
            objects: HashMap::new(),
            synonyms: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::SymbolId;

    #[test]
    fn semantic_model_defaults_are_empty() {
        let model = SemanticModel::new();
        assert!(model.files.is_empty());
        assert!(model.schemas.is_empty());
        assert!(model.catalog.is_none());
        assert!(model.diagnostics.is_empty());
    }

    #[test]
    fn file_model_tracks_decls_and_statements() {
        let mut file = FileModel::new(FileId::new(7));
        file.top_level.push(DeclId::new(1));
        file.top_level.push(DeclId::new(2));
        file.statements.push(StatementId::new(10));
        assert_eq!(file.file_id, FileId::new(7));
        assert_eq!(file.top_level.len(), 2);
        assert_eq!(file.statements, vec![StatementId::new(10)]);
    }

    #[test]
    fn schema_model_indexes_objects_and_synonyms() {
        let schema_name = SchemaName::from(SymbolId::new(1));
        let mut schema = SchemaModel::new(schema_name);
        let object_name = ObjectName::from(SymbolId::new(2));
        schema.objects.insert(object_name, ObjectId::new(42));
        let synonym_name = SynonymName::from(SymbolId::new(3));
        schema.synonyms.insert(synonym_name, ObjectId::new(42));
        assert_eq!(schema.name, schema_name);
        assert_eq!(schema.objects.get(&object_name), Some(&ObjectId::new(42)));
        assert_eq!(schema.synonyms.get(&synonym_name), Some(&ObjectId::new(42)));
    }

    #[test]
    fn semantic_model_lookups_round_trip() {
        let mut model = SemanticModel::new();
        let schema_name = SchemaName::from(SymbolId::new(1));
        let file = FileModel::new(FileId::new(11));
        model.files.push(file);
        model
            .schemas
            .insert(schema_name, SchemaModel::new(schema_name));
        assert!(model.file(FileId::new(11)).is_some());
        assert!(model.schema(schema_name).is_some());
        assert!(model.file(FileId::new(12)).is_none());
    }

    #[test]
    fn ids_are_numeric_and_serialize_transparently() {
        let serialized = serde_json::to_string(&DeclId::new(99)).unwrap();
        assert_eq!(serialized, "99");
        let serialized = serde_json::to_string(&StatementId::new(7)).unwrap();
        assert_eq!(serialized, "7");
    }
}
