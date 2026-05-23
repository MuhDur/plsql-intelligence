//! Canonical store of [`Declaration`] values keyed by stable [`DeclId`].
//!
//! Provides a data structure with append-only registration,
//! random-access lookup, ordered iteration, and per-kind / per-name
//! indices that downstream resolution passes can query. The
//! DeclTable owns `DeclId` allocation; callers must register
//! a declaration to obtain its identity rather than minting `DeclId`s
//! themselves. Parents must be registered before their children so the
//! `parent` reference inside each child's [`DeclCommon`] is already
//! valid when inserted.

use std::collections::{BTreeMap, HashMap};

use plsql_core::SymbolId;
use plsql_ir::{DeclCommon, DeclId, DeclKind, Declaration};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Stored declaration paired with its allocated identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclEntry {
    pub id: DeclId,
    pub declaration: Declaration,
}

/// Append-only registry of declarations.
///
/// Insertion allocates a fresh [`DeclId`]; deletion is intentionally not
/// supported. Indices are kept consistent on every `register` call so
/// downstream resolution can run lookups by name or kind in `O(1)` /
/// `O(matching)` without scanning the table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclTable {
    decls: BTreeMap<DeclId, Declaration>,
    next_id: u64,
    by_name: HashMap<SymbolId, Vec<DeclId>>,
    by_kind: HashMap<DeclKind, Vec<DeclId>>,
}

impl DeclTable {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a `DeclId` and insert a declaration.
    ///
    /// The returned id is the declaration's permanent identity for this
    /// analysis run.
    #[instrument(level = "trace", skip(self, declaration))]
    pub fn register(&mut self, declaration: Declaration) -> DeclId {
        self.next_id += 1;
        let id = DeclId::new(self.next_id);
        self.index(id, &declaration);
        self.decls.insert(id, declaration);
        id
    }

    /// Register a batch of declarations, returning ids in input order.
    ///
    /// Useful for IR lowering passes that have already produced a slice
    /// of declarations (e.g. package members).
    #[instrument(level = "trace", skip(self, declarations))]
    pub fn register_all<I>(&mut self, declarations: I) -> Vec<DeclId>
    where
        I: IntoIterator<Item = Declaration>,
    {
        declarations.into_iter().map(|d| self.register(d)).collect()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn get(&self, id: DeclId) -> Option<&Declaration> {
        self.decls.get(&id)
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn entry(&self, id: DeclId) -> Option<DeclEntry> {
        self.decls
            .get(&id)
            .cloned()
            .map(|declaration| DeclEntry { id, declaration })
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn len(&self) -> usize {
        self.decls.len()
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    /// Ordered iterator over `(DeclId, Declaration)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (DeclId, &Declaration)> {
        self.decls.iter().map(|(id, d)| (*id, d))
    }

    /// Declarations sharing a given source name (case-folded `SymbolId`).
    ///
    /// Overload resolution and shadowing rules are handled by the
    /// resolution-strategy passes; this lookup is the raw input.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn by_name(&self, name: SymbolId) -> Vec<DeclId> {
        self.by_name.get(&name).cloned().unwrap_or_default()
    }

    /// Declarations of a given [`DeclKind`].
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn by_kind(&self, kind: DeclKind) -> Vec<DeclId> {
        self.by_kind.get(&kind).cloned().unwrap_or_default()
    }

    /// Direct children of a declaration (members of a package, columns
    /// of a table, parameters of a routine).
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn children(&self, parent: DeclId) -> Vec<DeclId> {
        self.decls
            .iter()
            .filter_map(|(id, d)| {
                if d.common().parent == Some(parent) {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Shorthand for `decls.get(id).map(|d| d.common())`.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn common(&self, id: DeclId) -> Option<&DeclCommon> {
        self.decls.get(&id).map(Declaration::common)
    }

    fn index(&mut self, id: DeclId, declaration: &Declaration) {
        self.by_name
            .entry(declaration.common().name)
            .or_default()
            .push(id);
        self.by_kind.entry(declaration.kind()).or_default().push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position, SchemaName, Span};
    use plsql_ir::{
        ColumnDecl, FunctionDecl, PackageDecl, ProcedureDecl, TableDecl, TypeRef, VariableDecl,
    };

    fn span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 1, 0),
        )
    }

    fn variable_decl(name: u64) -> Declaration {
        Declaration::Variable(VariableDecl {
            common: DeclCommon::new(SymbolId::new(name), span()),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            default_text: None,
            constant: false,
            not_null: false,
        })
    }

    fn package_decl(name: u64) -> Declaration {
        Declaration::Package(PackageDecl {
            common: DeclCommon::new(SymbolId::new(name), span()),
            members: vec![],
            body: None,
        })
    }

    fn procedure_under(parent: DeclId, name: u64) -> Declaration {
        Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(SymbolId::new(name), span()).with_parent(parent),
            params: vec![],
        })
    }

    #[test]
    fn register_allocates_unique_sequential_ids() {
        let mut t = DeclTable::new();
        let a = t.register(variable_decl(1));
        let b = t.register(variable_decl(2));
        let c = t.register(variable_decl(3));
        assert_eq!(a.get(), 1);
        assert_eq!(b.get(), 2);
        assert_eq!(c.get(), 3);
        assert_eq!(t.len(), 3);
        assert!(!t.is_empty());
    }

    #[test]
    fn get_returns_inserted_declaration() {
        let mut t = DeclTable::new();
        let id = t.register(variable_decl(7));
        let got = t.get(id).expect("declaration present");
        assert_eq!(got.common().name, SymbolId::new(7));
    }

    #[test]
    fn by_name_groups_declarations_with_matching_name() {
        let mut t = DeclTable::new();
        let _a = t.register(variable_decl(42));
        let _b = t.register(variable_decl(42));
        let _c = t.register(variable_decl(99));
        let matches = t.by_name(SymbolId::new(42));
        assert_eq!(matches.len(), 2);
        let other = t.by_name(SymbolId::new(99));
        assert_eq!(other.len(), 1);
        let absent = t.by_name(SymbolId::new(123));
        assert!(absent.is_empty());
    }

    #[test]
    fn by_kind_partitions_declarations() {
        let mut t = DeclTable::new();
        let _v = t.register(variable_decl(1));
        let _p = t.register(package_decl(2));
        let _q = t.register(package_decl(3));
        assert_eq!(t.by_kind(DeclKind::Variable).len(), 1);
        assert_eq!(t.by_kind(DeclKind::Package).len(), 2);
        assert!(t.by_kind(DeclKind::Trigger).is_empty());
    }

    #[test]
    fn children_returns_decls_with_matching_parent() {
        let mut t = DeclTable::new();
        let pkg = t.register(package_decl(10));
        let proc_a = t.register(procedure_under(pkg, 11));
        let proc_b = t.register(procedure_under(pkg, 12));
        let _other = t.register(variable_decl(13));
        let kids = t.children(pkg);
        assert!(kids.contains(&proc_a));
        assert!(kids.contains(&proc_b));
        assert_eq!(kids.len(), 2);
    }

    #[test]
    fn register_all_returns_ids_in_input_order() {
        let mut t = DeclTable::new();
        let ids = t.register_all([variable_decl(1), variable_decl(2), variable_decl(3)]);
        assert_eq!(ids.len(), 3);
        for (i, id) in ids.iter().enumerate() {
            let got = t.get(*id).expect("present");
            assert_eq!(got.common().name, SymbolId::new(i as u64 + 1));
        }
    }

    #[test]
    fn entry_round_trips_declaration() {
        let mut t = DeclTable::new();
        let id = t.register(variable_decl(5));
        let entry = t.entry(id).expect("entry");
        assert_eq!(entry.id, id);
        assert_eq!(entry.declaration.common().name, SymbolId::new(5));
    }

    #[test]
    fn iter_yields_in_id_order() {
        let mut t = DeclTable::new();
        let a = t.register(variable_decl(1));
        let b = t.register(variable_decl(2));
        let collected: Vec<DeclId> = t.iter().map(|(id, _)| id).collect();
        assert_eq!(collected, vec![a, b]);
    }

    #[test]
    fn package_with_columns_table_and_function_register_correctly() {
        let mut t = DeclTable::new();
        let table = t.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(SymbolId::new(100), span())
                .with_schema(SchemaName::from(SymbolId::new(99))),
            columns: vec![],
        }));
        let col = t.register(Declaration::Column(ColumnDecl {
            common: DeclCommon::new(SymbolId::new(101), span()).with_parent(table),
            ty: None,
            not_null: false,
        }));
        let pkg = t.register(package_decl(200));
        let func = t.register(Declaration::Function(FunctionDecl {
            common: DeclCommon::new(SymbolId::new(201), span()).with_parent(pkg),
            params: vec![],
            return_type: None,
        }));
        assert_eq!(
            t.common(table)
                .and_then(|c| c.schema)
                .map(SchemaName::symbol),
            Some(SymbolId::new(99))
        );
        assert_eq!(t.common(col).and_then(|c| c.parent), Some(table));
        assert_eq!(t.common(func).and_then(|c| c.parent), Some(pkg));
        assert_eq!(t.by_kind(DeclKind::Column), vec![col]);
        assert_eq!(t.by_kind(DeclKind::Table), vec![table]);
    }
}
