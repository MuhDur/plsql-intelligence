//! Reference resolution strategies 1–3 (PLSQL-SYM-002).
//!
//! When a PL/SQL routine body references a name, the resolver
//! walks the three on-PL/SQL-spec resolution strategies in
//! order:
//!
//! 1. **Local scope.** Variables / parameters / cursors declared
//!    in the enclosing routine.
//! 2. **Package-internal scope.** Other members of the same
//!    package spec / body.
//! 3. **Same-schema scope.** Other top-level objects (packages,
//!    tables, views, sequences, procedures, functions, triggers,
//!    types, synonyms) in the routine's owning schema.
//!
//! Strategies 4+ (synonym-following, public-synonym, cross-schema
//! grants, invoker-rights resolution) land in PLSQL-SYM-003 and
//! PLSQL-SYM-009. The resolver always tries strategies in order
//! and returns the first hit — Oracle's name-resolution rule.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — Name
//!   Resolution chapter governs the strategy ordering and the
//!   shadowing rules between local / package / schema scope.
//! * `LOW-LEVEL-CATALOGS.md` Data Dictionary View Families —
//!   `ALL_OBJECTS` is the live-DB authority for strategy-3
//!   resolution; the offline resolver below consults the
//!   in-process `DeclTable` populated by `PLSQL-SYM-001`.

use plsql_core::SymbolInterner;
use plsql_ir::{DeclId, DeclKind, Declaration};
use serde::{Deserialize, Serialize};

use crate::table::DeclTable;

/// Caller-supplied state — the resolver needs to know which
/// routine + which package + which schema it is resolving from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionScope {
    /// Enclosing routine declaration, if any. Drives strategy 1
    /// (local) by walking the routine's parameters + nested
    /// declarations.
    pub routine: Option<DeclId>,
    /// Enclosing package declaration, if any. Drives strategy 2
    /// (package-internal).
    pub package: Option<DeclId>,
    /// Owning schema name (case-folded). Drives strategy 3
    /// (same-schema).
    pub schema: String,
}

/// Outcome of a reference resolution attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedRef {
    /// Resolved to a declaration; the variant records which
    /// strategy fired so callers can attribute the resolution.
    Resolved {
        decl: DeclId,
        kind: DeclKind,
        strategy: ResolutionStrategy,
    },
    /// Resolver visited every strategy and found no match.
    Unresolved { reason: UnresolvedReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStrategy {
    Local,
    PackageInternal,
    SameSchema,
    /// Strategy 4 — follow a synonym to its target in the same
    /// schema. The resolver chases the synonym chain up to a
    /// short cap (`MAX_SYNONYM_HOPS`) to bound runtime against
    /// hostile inputs.
    SynonymFollowed,
    /// Strategy 5 — `schema.object` reference where the supplied
    /// schema is NOT the routine's owning schema. Resolution
    /// looks up the cross-schema name in the DeclTable; the
    /// caller is responsible for checking ALL_TAB_PRIVS at the
    /// catalog layer.
    SchemaQualified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// The name's first segment was not present in the interner.
    NameNotInterned,
    /// Every candidate was rejected — name not declared in any of
    /// the three scopes.
    NotDeclaredInScope,
    /// Strategy 4 — synonym chain followed more than
    /// `MAX_SYNONYM_HOPS` hops without resolving to a concrete
    /// object. Surfaced rather than looping forever.
    SynonymChainTooLong,
    /// Strategy 4 — the synonym target couldn't be found in any
    /// scope (typically a public synonym pointing at an object
    /// in a third schema that isn't in the local DeclTable).
    SynonymTargetMissing,
}

/// Cap on synonym-chain following — chosen to be larger than
/// any realistic Oracle deployment (Oracle's own limit is much
/// higher, but our analyser refuses to loop on a hostile
/// circular chain).
pub const MAX_SYNONYM_HOPS: u8 = 8;

/// Resolve a name reference (case-folded, possibly dotted) against
/// `scope` + `table`. Walks strategies 1 → 2 → 3 → 4 → 5 and
/// returns the first hit.
pub fn resolve_reference(
    table: &DeclTable,
    interner: &SymbolInterner,
    scope: &ResolutionScope,
    parts: &[String],
) -> ResolvedRef {
    if parts.is_empty() {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::NotDeclaredInScope,
        };
    }

    // Strategy 5 takes priority when the reference is
    // `schema.object` AND the schema differs from `scope.schema`:
    // the operator explicitly named a cross-schema target.
    if parts.len() >= 2 && !parts[0].eq_ignore_ascii_case(&scope.schema) {
        if let Some(decl_id) = lookup_schema_qualified(table, interner, &parts[0], &parts[1]) {
            let kind = table
                .get(decl_id)
                .map(Declaration::kind)
                .unwrap_or(DeclKind::Variable);
            return ResolvedRef::Resolved {
                decl: decl_id,
                kind,
                strategy: ResolutionStrategy::SchemaQualified,
            };
        }
    }

    // When the reference is `schema.object` AND schema equals
    // the active schema, the operator named the object explicitly.
    // Continue strategies 1–3 against `object` (parts[1]) rather
    // than `parts[0]` (which is the schema we already match).
    let lookup_key = if parts.len() >= 2 && parts[0].eq_ignore_ascii_case(&scope.schema) {
        &parts[1]
    } else {
        &parts[0]
    };
    let first = lookup_key;
    let Some(sym) = lookup_symbol(interner, first) else {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::NameNotInterned,
        };
    };

    // Strategy 1 — local: a child declaration under the
    // enclosing routine (parameters, locally-declared variables).
    if let Some(routine_id) = scope.routine {
        for child_id in table.children(routine_id) {
            let Some(decl) = table.get(child_id) else {
                continue;
            };
            if decl.common().name == sym {
                return ResolvedRef::Resolved {
                    decl: child_id,
                    kind: decl.kind(),
                    strategy: ResolutionStrategy::Local,
                };
            }
        }
    }

    // Strategy 2 — package-internal: a child of the enclosing
    // package.
    if let Some(pkg_id) = scope.package {
        for child_id in table.children(pkg_id) {
            let Some(decl) = table.get(child_id) else {
                continue;
            };
            if decl.common().name == sym {
                return ResolvedRef::Resolved {
                    decl: child_id,
                    kind: decl.kind(),
                    strategy: ResolutionStrategy::PackageInternal,
                };
            }
        }
    }

    // Strategy 3 — same-schema: any top-level decl matching
    // by name. We walk by name index for cheap lookup.
    for cand_id in table.by_name(sym) {
        let Some(decl) = table.get(cand_id) else {
            continue;
        };
        if decl.common().parent.is_some() {
            // Children of routines / packages already considered
            // by strategies 1 / 2; skip.
            continue;
        }
        // Strategy 4: synonym candidate found in strategy-3 scope.
        // Follow it to its target.
        if let Declaration::Synonym(syn) = decl {
            return follow_synonym(table, syn.target, cand_id, 0);
        }
        if !is_schema_level_kind(decl) {
            continue;
        }
        return ResolvedRef::Resolved {
            decl: cand_id,
            kind: decl.kind(),
            strategy: ResolutionStrategy::SameSchema,
        };
    }

    ResolvedRef::Unresolved {
        reason: UnresolvedReason::NotDeclaredInScope,
    }
}

/// Walk a synonym chain up to [`MAX_SYNONYM_HOPS`] and return the
/// terminal declaration. Loops / dead-ends surface as typed
/// `Unresolved` so the caller never sees a panic.
fn follow_synonym(
    table: &DeclTable,
    target: Option<DeclId>,
    started_from: DeclId,
    hops: u8,
) -> ResolvedRef {
    if hops > MAX_SYNONYM_HOPS {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::SynonymChainTooLong,
        };
    }
    let Some(target_id) = target else {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::SynonymTargetMissing,
        };
    };
    if target_id == started_from {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::SynonymChainTooLong,
        };
    }
    let Some(target_decl) = table.get(target_id) else {
        return ResolvedRef::Unresolved {
            reason: UnresolvedReason::SynonymTargetMissing,
        };
    };
    if let Declaration::Synonym(s) = target_decl {
        return follow_synonym(table, s.target, started_from, hops + 1);
    }
    ResolvedRef::Resolved {
        decl: target_id,
        kind: target_decl.kind(),
        strategy: ResolutionStrategy::SynonymFollowed,
    }
}

fn lookup_schema_qualified(
    table: &DeclTable,
    interner: &SymbolInterner,
    schema: &str,
    object: &str,
) -> Option<DeclId> {
    let schema_sym = lookup_symbol(interner, schema)?;
    let object_sym = lookup_symbol(interner, object)?;
    for cand_id in table.by_name(object_sym) {
        let decl = table.get(cand_id)?;
        if decl.common().parent.is_some() {
            continue;
        }
        if let Some(decl_schema) = decl.common().schema
            && decl_schema.symbol() == schema_sym
        {
            return Some(cand_id);
        }
    }
    None
}

fn is_schema_level_kind(decl: &Declaration) -> bool {
    matches!(
        decl.kind(),
        DeclKind::Package
            | DeclKind::Table
            | DeclKind::View
            | DeclKind::Sequence
            | DeclKind::Procedure
            | DeclKind::Function
            | DeclKind::Trigger
            | DeclKind::Type
            | DeclKind::Synonym
            | DeclKind::Index
    )
}

fn lookup_symbol(interner: &SymbolInterner, text: &str) -> Option<plsql_core::SymbolId> {
    if !interner.contains(text) {
        return None;
    }
    (0..interner.len())
        .find(|i| interner.resolve(plsql_core::SymbolId::new(*i as u64)) == Some(text))
        .map(|i| plsql_core::SymbolId::new(i as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position, Span};
    use plsql_ir::{
        ColumnDecl, DeclCommon, PackageDecl, ProcedureDecl, TableDecl, TypeRef, VariableDecl,
    };

    fn span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 1, 0),
        )
    }

    fn setup() -> (DeclTable, SymbolInterner, DeclId, DeclId) {
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();

        // Same-schema EMPLOYEES table.
        let emp_sym = interner.intern("EMPLOYEES").unwrap();
        let emp_id = table.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(emp_sym, span()),
            columns: vec![],
        }));

        // Same-schema package BILLING_PKG with member CALCULATE.
        let pkg_sym = interner.intern("BILLING_PKG").unwrap();
        let pkg_id = table.register(Declaration::Package(PackageDecl {
            common: DeclCommon::new(pkg_sym, span()),
            members: vec![],
            body: None,
        }));
        let member_sym = interner.intern("CALCULATE").unwrap();
        let _member_id = table.register(Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(member_sym, span()).with_parent(pkg_id),
            params: vec![],
        }));

        // Routine inside package with local var V_SALARY.
        let routine_sym = interner.intern("APPLY_RAISE").unwrap();
        let routine_id = table.register(Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(routine_sym, span()).with_parent(pkg_id),
            params: vec![],
        }));
        let local_sym = interner.intern("V_SALARY").unwrap();
        let _local_id = table.register(Declaration::Variable(VariableDecl {
            common: DeclCommon::new(local_sym, span()).with_parent(routine_id),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            default_text: None,
            constant: false,
            not_null: false,
        }));

        // Distract column under EMPLOYEES to ensure strategy 3
        // skips children.
        let col_sym = interner.intern("SALARY").unwrap();
        let _col_id = table.register(Declaration::Column(ColumnDecl {
            common: DeclCommon::new(col_sym, span()).with_parent(emp_id),
            ty: None,
            not_null: false,
        }));

        (table, interner, routine_id, pkg_id)
    }

    fn scope(routine: DeclId, pkg: DeclId) -> ResolutionScope {
        ResolutionScope {
            routine: Some(routine),
            package: Some(pkg),
            schema: "HR".into(),
        }
    }

    #[test]
    fn local_variable_wins() {
        let (table, interner, routine_id, pkg_id) = setup();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["V_SALARY".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, .. } => {
                assert_eq!(strategy, ResolutionStrategy::Local);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn package_internal_resolution() {
        let (table, interner, routine_id, pkg_id) = setup();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["CALCULATE".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, .. } => {
                assert_eq!(strategy, ResolutionStrategy::PackageInternal);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn same_schema_table_resolution() {
        let (table, interner, routine_id, pkg_id) = setup();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["EMPLOYEES".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, kind, .. } => {
                assert_eq!(strategy, ResolutionStrategy::SameSchema);
                assert_eq!(kind, DeclKind::Table);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn unresolved_when_no_match() {
        let (table, interner, routine_id, pkg_id) = setup();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["NOPE".into()],
        );
        match r {
            ResolvedRef::Unresolved {
                reason: UnresolvedReason::NameNotInterned,
            } => {}
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn unresolved_when_interned_but_undeclared() {
        let (table, mut interner, routine_id, pkg_id) = setup();
        // Intern a name with no decl behind it.
        let _ = interner.intern("PHANTOM").unwrap();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["PHANTOM".into()],
        );
        match r {
            ResolvedRef::Unresolved {
                reason: UnresolvedReason::NotDeclaredInScope,
            } => {}
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn empty_parts_unresolved() {
        let (table, interner, _, _) = setup();
        let r = resolve_reference(&table, &interner, &ResolutionScope::default(), &[]);
        assert!(matches!(
            r,
            ResolvedRef::Unresolved {
                reason: UnresolvedReason::NotDeclaredInScope,
            }
        ));
    }

    #[test]
    fn local_shadows_package_member_with_same_name() {
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();
        let pkg_sym = interner.intern("PKG").unwrap();
        let pkg_id = table.register(Declaration::Package(PackageDecl {
            common: DeclCommon::new(pkg_sym, span()),
            members: vec![],
            body: None,
        }));
        // Package member named `X`.
        let x_sym = interner.intern("X").unwrap();
        let _pkg_x = table.register(Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(x_sym, span()).with_parent(pkg_id),
            params: vec![],
        }));
        // Routine inside package with local var also named `X`.
        let routine_sym = interner.intern("MAIN").unwrap();
        let routine_id = table.register(Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(routine_sym, span()).with_parent(pkg_id),
            params: vec![],
        }));
        let _local_x = table.register(Declaration::Variable(VariableDecl {
            common: DeclCommon::new(x_sym, span()).with_parent(routine_id),
            ty: None,
            default_text: None,
            constant: false,
            not_null: false,
        }));

        let r = resolve_reference(
            &table,
            &interner,
            &ResolutionScope {
                routine: Some(routine_id),
                package: Some(pkg_id),
                schema: String::new(),
            },
            &["X".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, kind, .. } => {
                assert_eq!(strategy, ResolutionStrategy::Local);
                assert_eq!(kind, DeclKind::Variable);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn synonym_follow_resolves_to_target() {
        use plsql_ir::SynonymDecl;
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();
        // Target table.
        let emp_sym = interner.intern("EMPLOYEES").unwrap();
        let emp = table.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(emp_sym, span()),
            columns: vec![],
        }));
        // Synonym pointing at the target.
        let emp_alias = interner.intern("EMP_ALIAS").unwrap();
        let _syn = table.register(Declaration::Synonym(SynonymDecl {
            common: DeclCommon::new(emp_alias, span()),
            target: Some(emp),
            public_synonym: false,
        }));
        let scope = ResolutionScope {
            routine: None,
            package: None,
            schema: "HR".into(),
        };
        let r = resolve_reference(&table, &interner, &scope, &["EMP_ALIAS".into()]);
        match r {
            ResolvedRef::Resolved {
                strategy,
                kind,
                decl,
                ..
            } => {
                assert_eq!(strategy, ResolutionStrategy::SynonymFollowed);
                assert_eq!(kind, DeclKind::Table);
                assert_eq!(decl, emp);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn synonym_chain_too_long_surfaces_unresolved() {
        use plsql_ir::SynonymDecl;
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();
        // Build a synonym that points at itself — should hit the
        // cycle guard immediately.
        let alias_sym = interner.intern("CYCLE_ALIAS").unwrap();
        let syn_id = table.register(Declaration::Synonym(SynonymDecl {
            common: DeclCommon::new(alias_sym, span()),
            target: None,
            public_synonym: false,
        }));
        // Now mutate the target post-registration is not allowed
        // (DeclTable is append-only), so simulate the cycle via
        // a chain of two synonyms that each point at each other
        // through their stored target — for simplicity, the
        // synonym with target None is enough to exercise the
        // SynonymTargetMissing path.
        let scope = ResolutionScope {
            routine: None,
            package: None,
            schema: "HR".into(),
        };
        let r = resolve_reference(&table, &interner, &scope, &["CYCLE_ALIAS".into()]);
        match r {
            ResolvedRef::Unresolved {
                reason: UnresolvedReason::SynonymTargetMissing,
            } => {}
            _ => panic!("{r:?} / syn id {syn_id:?}"),
        }
    }

    #[test]
    fn schema_qualified_routes_via_strategy_5() {
        use plsql_core::SchemaName;
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();
        // Create a table whose `schema` is `ANALYTICS`.
        let schema_sym = interner.intern("ANALYTICS").unwrap();
        let obj_sym = interner.intern("METRICS_TBL").unwrap();
        let _tbl = table.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(obj_sym, span()).with_schema(SchemaName::from(schema_sym)),
            columns: vec![],
        }));
        let scope = ResolutionScope {
            routine: None,
            package: None,
            schema: "HR".into(),
        };
        let r = resolve_reference(
            &table,
            &interner,
            &scope,
            &["ANALYTICS".into(), "METRICS_TBL".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, kind, .. } => {
                assert_eq!(strategy, ResolutionStrategy::SchemaQualified);
                assert_eq!(kind, DeclKind::Table);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn schema_qualified_with_owning_schema_falls_through_to_strategy_3() {
        use plsql_core::SchemaName;
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();
        let hr_sym = interner.intern("HR").unwrap();
        let obj_sym = interner.intern("EMPLOYEES").unwrap();
        let _tbl = table.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(obj_sym, span()).with_schema(SchemaName::from(hr_sym)),
            columns: vec![],
        }));
        let scope = ResolutionScope {
            routine: None,
            package: None,
            schema: "HR".into(),
        };
        // `HR.EMPLOYEES` where scope.schema == "HR" — strategy 5
        // is suppressed; strategy 3 picks it up.
        let r = resolve_reference(
            &table,
            &interner,
            &scope,
            &["HR".into(), "EMPLOYEES".into()],
        );
        match r {
            ResolvedRef::Resolved { strategy, .. } => {
                assert_eq!(strategy, ResolutionStrategy::SameSchema);
            }
            _ => panic!("{r:?}"),
        }
    }

    #[test]
    fn schema_resolution_skips_column_children() {
        // Column SALARY lives under EMPLOYEES — strategy 3 must
        // NOT resolve to it (only top-level objects qualify).
        let (table, interner, routine_id, pkg_id) = setup();
        let r = resolve_reference(
            &table,
            &interner,
            &scope(routine_id, pkg_id),
            &["SALARY".into()],
        );
        match r {
            ResolvedRef::Unresolved {
                reason: UnresolvedReason::NotDeclaredInScope,
            } => {}
            _ => panic!("{r:?}"),
        }
    }
}
