//! End-to-end resolution test over a 10-package synthetic schema
//! (PLSQL-SYM-008).
//!
//! Builds a synthetic schema `HR` containing ten top-level packages
//! `PKG_01 .. PKG_10`, each exposing one public procedure
//! `PROC_01 .. PROC_10`, plus one shared schema-level table
//! `SHARED_AUDIT`. The resolver is then driven exactly as the engine
//! would drive it from inside one package's body, and we assert that:
//!
//! * every `PKG_n.PROC_n` cross-package call resolves (strategy 5,
//!   schema-qualified — the package name acts as the qualifier),
//! * a bare schema-level table reference resolves (strategy 3),
//! * a same-package member resolves package-internally (strategy 2),
//! * a non-existent cross-package member surfaces a typed
//!   `Unresolved`, never a panic.
//!
//! Source-text *parsing* into IR `Declaration`s is gated on
//! `PLSQL-PARSE-005` (`lower.rs` for statement bodies); until that
//! lands the synthetic schema is constructed through the IR
//! `Declaration` + `SymbolInterner` API, which is the same shape the
//! lowering pass will feed the [`DeclTable`] with. The resolution
//! engine under test is exercised unchanged.

use plsql_core::{FileId, Position, Span, SymbolInterner};
use plsql_ir::{DeclCommon, DeclId, DeclKind, Declaration, PackageDecl, ProcedureDecl, TableDecl};
use plsql_symbols::{
    DeclTable, ResolutionScope, ResolutionStrategy, ResolvedRef, resolve_reference,
};

const PACKAGE_COUNT: usize = 10;

fn span() -> Span {
    Span::new(
        FileId::new(1),
        Position::new(1, 1, 0),
        Position::new(1, 1, 0),
    )
}

struct Schema {
    table: DeclTable,
    interner: SymbolInterner,
    /// `PKG_0n` package decl ids, indexed 0-based.
    packages: Vec<DeclId>,
}

/// Construct the 10-package synthetic schema. Each procedure is a
/// top-level decl whose owning "schema" is its *package* name, which
/// is exactly what makes `PKG_n.PROC_n` resolve through the
/// schema-qualified strategy (the resolver treats the leading
/// qualifier as a namespace).
fn build_schema() -> Schema {
    let mut interner = SymbolInterner::new();
    let mut table = DeclTable::new();
    let hr = interner.intern_schema_name("HR").unwrap();
    let mut packages = Vec::with_capacity(PACKAGE_COUNT);

    for n in 1..=PACKAGE_COUNT {
        let pkg_name = format!("PKG_{n:02}");
        let proc_name = format!("PROC_{n:02}");

        let pkg_sym = interner.intern(pkg_name.clone()).unwrap();
        let pkg_id = table.register(Declaration::Package(PackageDecl {
            common: DeclCommon::new(pkg_sym, span()).with_schema(hr),
            members: vec![],
            body: None,
        }));
        packages.push(pkg_id);

        // The package name doubles as the qualifier namespace for its
        // public procedure so `PKG_0n.PROC_0n` is resolvable.
        let pkg_as_ns = interner.intern_schema_name(pkg_name).unwrap();
        let proc_sym = interner.intern(proc_name).unwrap();
        table.register(Declaration::Procedure(ProcedureDecl {
            common: DeclCommon::new(proc_sym, span()).with_schema(pkg_as_ns),
            params: vec![],
        }));
    }

    // One shared schema-level table referenced by every package.
    let audit_sym = interner.intern("SHARED_AUDIT").unwrap();
    table.register(Declaration::Table(TableDecl {
        common: DeclCommon::new(audit_sym, span()).with_schema(hr),
        columns: vec![],
    }));

    // A package-internal helper under PKG_01 (parent-scoped, no
    // schema) to exercise strategy 2 alongside the cross-package path.
    let helper_sym = interner.intern("HELPER_01").unwrap();
    table.register(Declaration::Procedure(ProcedureDecl {
        common: DeclCommon::new(helper_sym, span()).with_parent(packages[0]),
        params: vec![],
    }));

    Schema {
        table,
        interner,
        packages,
    }
}

/// Scope of code executing inside `PKG_0{pkg_index+1}` in schema HR.
fn scope_in(packages: &[DeclId], pkg_index: usize) -> ResolutionScope {
    ResolutionScope {
        routine: None,
        package: Some(packages[pkg_index]),
        schema: "HR".into(),
    }
}

#[test]
fn every_cross_package_call_resolves() {
    let s = build_schema();
    // Drive from inside PKG_01 and resolve a call into every package
    // (including a self-qualified call back into PKG_01).
    let caller = scope_in(&s.packages, 0);
    for n in 1..=PACKAGE_COUNT {
        let parts = vec![format!("PKG_{n:02}"), format!("PROC_{n:02}")];
        let r = resolve_reference(&s.table, &s.interner, &caller, &parts);
        match r {
            ResolvedRef::Resolved { strategy, kind, .. } => {
                assert_eq!(
                    strategy,
                    ResolutionStrategy::SchemaQualified,
                    "PKG_{n:02}.PROC_{n:02} should resolve schema-qualified"
                );
                assert_eq!(kind, DeclKind::Procedure, "PKG_{n:02}.PROC_{n:02} kind");
            }
            other => panic!("PKG_{n:02}.PROC_{n:02} did not resolve: {other:?}"),
        }
    }
}

#[test]
fn cross_package_calls_resolve_from_every_caller() {
    let s = build_schema();
    // The caller's own package must not affect a fully-qualified
    // cross-package reference — verify from all 10 vantage points.
    for caller_idx in 0..PACKAGE_COUNT {
        let caller = scope_in(&s.packages, caller_idx);
        let target = ((caller_idx + 1) % PACKAGE_COUNT) + 1; // next pkg, 1-based
        let parts = vec![format!("PKG_{target:02}"), format!("PROC_{target:02}")];
        let r = resolve_reference(&s.table, &s.interner, &caller, &parts);
        assert!(
            matches!(
                r,
                ResolvedRef::Resolved {
                    strategy: ResolutionStrategy::SchemaQualified,
                    ..
                }
            ),
            "caller PKG_{:02} -> PKG_{target:02}.PROC_{target:02}: {r:?}",
            caller_idx + 1
        );
    }
}

#[test]
fn shared_schema_table_resolves_same_schema() {
    let s = build_schema();
    let caller = scope_in(&s.packages, 4); // inside PKG_05
    let r = resolve_reference(
        &s.table,
        &s.interner,
        &caller,
        &["SHARED_AUDIT".to_string()],
    );
    match r {
        ResolvedRef::Resolved { strategy, kind, .. } => {
            assert_eq!(strategy, ResolutionStrategy::SameSchema);
            assert_eq!(kind, DeclKind::Table);
        }
        other => panic!("SHARED_AUDIT did not resolve: {other:?}"),
    }
}

#[test]
fn same_package_member_resolves_package_internal() {
    let s = build_schema();
    let caller = scope_in(&s.packages, 0); // inside PKG_01
    let r = resolve_reference(&s.table, &s.interner, &caller, &["HELPER_01".to_string()]);
    match r {
        ResolvedRef::Resolved { strategy, kind, .. } => {
            assert_eq!(strategy, ResolutionStrategy::PackageInternal);
            assert_eq!(kind, DeclKind::Procedure);
        }
        other => panic!("HELPER_01 did not resolve package-internally: {other:?}"),
    }
}

#[test]
fn unknown_cross_package_member_is_typed_unresolved() {
    let s = build_schema();
    let caller = scope_in(&s.packages, 2);
    // A qualifier that names no package and a member that names
    // nothing — there are only 10 packages, so PKG_99 cannot shadow
    // anything. Must surface a typed Unresolved, never a panic.
    let r = resolve_reference(
        &s.table,
        &s.interner,
        &caller,
        &["PKG_99".to_string(), "DOES_NOT_EXIST".to_string()],
    );
    assert!(
        matches!(r, ResolvedRef::Unresolved { .. }),
        "missing cross-package member must surface as Unresolved, got {r:?}"
    );
}

#[test]
fn all_ten_packages_and_procedures_registered() {
    let s = build_schema();
    let packages = s
        .table
        .iter()
        .filter(|(_, d)| matches!(d, Declaration::Package(_)))
        .count();
    let procedures = s
        .table
        .iter()
        .filter(|(_, d)| matches!(d, Declaration::Procedure(_)))
        .count();
    assert_eq!(packages, PACKAGE_COUNT);
    // 10 public PROC_n + 1 HELPER_01.
    assert_eq!(procedures, PACKAGE_COUNT + 1);
}
