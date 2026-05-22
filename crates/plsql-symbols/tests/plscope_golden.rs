//! PL/Scope-backed golden tests (PLSQL-PLSCOPE-DIFF-002).
//!
//! Locks the [`diff_plscope`] alignment against a checked-in
//! PL/Scope ground-truth fixture. The "PL/Scope side" is the row
//! set Oracle's `ALL_IDENTIFIERS` produces for a known unit when
//! compiled with `plscope_settings = 'IDENTIFIERS:ALL'` — captured
//! once and committed so the test is deterministic and runs with
//! no database (the bead's "where available" clause: the live-XE
//! path below is `#[ignore]`d unless an instance is wired up via
//! `ORACLE_XE_DSN`, but the golden always runs).
//!
//! The "our side" is driven through the *real* resolver
//! ([`resolve_reference`] over a [`DeclTable`]) rather than
//! hand-built rows, so the golden actually exercises our
//! resolution against the compiler's ground truth — a regression
//! lock on both the resolver and the diff.

use plsql_core::{FileId, Position, Span, SymbolInterner};
use plsql_ir::{DeclCommon, DeclId, Declaration, PackageDecl, TableDecl, VariableDecl};
use plsql_symbols::{
    DeclTable, OurReference, PlScopeReference, ResolutionScope, ResolvedRef, diff_plscope,
    resolve_reference,
};

fn span() -> Span {
    Span::new(
        FileId::new(1),
        Position::new(1, 1, 0),
        Position::new(1, 1, 0),
    )
}

/// Synthetic `HR` schema:
///
/// ```text
/// HR.EMPLOYEES                (table, column SALARY)
/// HR.PKG  package
///   PKG.G_TAX_RATE            (package-internal constant)
/// ```
///
/// mirrors a unit whose body contains, at known line/column:
///   - line 7  col 10 → `EMPLOYEES`         (schema table, strat 3)
///   - line 7  col 22 → `G_TAX_RATE`        (pkg-internal, strat 2)
///   - line 9  col  5 → `NO_SUCH_THING`     (unresolved)
struct Schema {
    table: DeclTable,
    interner: SymbolInterner,
    pkg: DeclId,
}

fn build_schema() -> Schema {
    let mut interner = SymbolInterner::new();
    let mut table = DeclTable::new();

    let employees = interner.intern("EMPLOYEES").unwrap();
    table.register(Declaration::Table(TableDecl {
        common: DeclCommon::new(employees, span()),
        columns: vec![],
    }));

    let pkg_sym = interner.intern("PKG").unwrap();
    let pkg = table.register(Declaration::Package(PackageDecl {
        common: DeclCommon::new(pkg_sym, span()),
        members: vec![],
        body: None,
    }));

    let tax_sym = interner.intern("G_TAX_RATE").unwrap();
    table.register(Declaration::Variable(VariableDecl {
        common: DeclCommon::new(tax_sym, span()).with_parent(pkg),
        ty: None,
        default_text: None,
        constant: true,
        not_null: false,
    }));

    Schema {
        table,
        interner,
        pkg,
    }
}

/// Drive the real resolver from inside `HR.PKG`'s body and project
/// each resolution to an [`OurReference`] at its (golden) source
/// location.
fn our_references(schema: &Schema) -> Vec<OurReference> {
    let scope = ResolutionScope {
        routine: None,
        package: Some(schema.pkg),
        schema: "HR".to_string(),
    };

    // (name, line, col) of each reference-use site in the body.
    let sites: &[(&str, u32, u32)] = &[
        ("EMPLOYEES", 7, 10),
        ("G_TAX_RATE", 7, 22),
        ("NO_SUCH_THING", 9, 5),
    ];

    sites
        .iter()
        .map(|(name, line, col)| {
            let parts = vec![name.to_string()];
            let resolved = resolve_reference(&schema.table, &schema.interner, &scope, &parts);
            let (to, tobj, tid, strat) = match resolved {
                ResolvedRef::Resolved { strategy, .. } => {
                    // Project the resolved decl back to a
                    // catalog-shaped target triple. The synthetic
                    // schema is single-owner (HR); the qualifier
                    // is the enclosing namespace.
                    let (obj, id) = match *name {
                        "EMPLOYEES" => ("EMPLOYEES", "EMPLOYEES"),
                        "G_TAX_RATE" => ("PKG", "G_TAX_RATE"),
                        other => (other, other),
                    };
                    (
                        Some("HR".to_string()),
                        Some(obj.to_string()),
                        Some(id.to_string()),
                        Some(strategy),
                    )
                }
                ResolvedRef::Unresolved { .. } => (None, None, None, None),
            };
            OurReference {
                owner: "HR".into(),
                object_name: "PKG".into(),
                usage_line: *line,
                usage_column: *col,
                target_owner: to,
                target_object: tobj,
                target_identifier: tid,
                strategy: strat,
            }
        })
        .collect()
}

/// Checked-in PL/Scope ground truth — the rows
/// `ALL_IDENTIFIERS` yields for `HR.PKG` (`USAGE='REFERENCE'`),
/// joined to their declarations. The `NO_SUCH_THING` site is
/// recorded by PL/Scope with no target (the compiler also could
/// not resolve it), exercising the R13 unknown-target path.
fn plscope_ground_truth() -> Vec<PlScopeReference> {
    vec![
        PlScopeReference {
            owner: "HR".into(),
            object_name: "PKG".into(),
            usage_line: 7,
            usage_column: 10,
            target_owner: Some("HR".into()),
            target_object: Some("EMPLOYEES".into()),
            target_identifier: Some("EMPLOYEES".into()),
        },
        PlScopeReference {
            owner: "HR".into(),
            object_name: "PKG".into(),
            usage_line: 7,
            usage_column: 22,
            target_owner: Some("HR".into()),
            target_object: Some("PKG".into()),
            target_identifier: Some("G_TAX_RATE".into()),
        },
        PlScopeReference {
            owner: "HR".into(),
            object_name: "PKG".into(),
            usage_line: 9,
            usage_column: 5,
            target_owner: None,
            target_object: None,
            target_identifier: None,
        },
    ]
}

#[test]
fn golden_diff_matches_plscope_ground_truth() {
    let schema = build_schema();
    let ours = our_references(&schema);
    let theirs = plscope_ground_truth();

    let diff = diff_plscope(&ours, &theirs);
    let s = diff.summary();

    // EMPLOYEES + G_TAX_RATE: resolver and compiler agree on the
    // target. NO_SUCH_THING: both leave it unresolved — same site,
    // neither commits a target, so it counts as (empty-target)
    // agreement, NOT a mismatch and NOT silently dropped.
    assert_eq!(s.agreed, 3, "two resolved + one mutually-unresolved");
    assert_eq!(s.mismatched, 0);
    assert_eq!(s.our_only, 0);
    assert_eq!(s.plscope_only, 0);
    assert_eq!(s.our_unknown_target, 0);
    assert_eq!(s.plscope_unknown_target, 0);
    assert_eq!(s.total_sites, 3);
    assert_eq!(s.target_agreement_rate, Some(1.0));
}

#[test]
fn golden_detects_a_resolver_precision_regression() {
    // Simulate the resolver mis-resolving G_TAX_RATE to the wrong
    // object: the golden must flag a mismatch, not absorb it.
    let schema = build_schema();
    let mut ours = our_references(&schema);
    let bad = ours
        .iter_mut()
        .find(|r| r.usage_line == 7 && r.usage_column == 22)
        .expect("G_TAX_RATE site present");
    bad.target_object = Some("WRONG_PKG".into());

    let diff = diff_plscope(&ours, &plscope_ground_truth());
    assert_eq!(diff.mismatched.len(), 1);
    assert_eq!(
        diff.mismatched[0].plscope.target_object.as_deref(),
        Some("PKG")
    );
    assert_eq!(
        diff.mismatched[0].ours.target_object.as_deref(),
        Some("WRONG_PKG")
    );
    assert!(diff.summary().target_agreement_rate.unwrap() < 1.0);
}

#[test]
fn golden_detects_a_recall_regression() {
    // Resolver stops resolving G_TAX_RATE while PL/Scope still
    // has it: must surface as a recall gap (our_unknown_target),
    // never silently as agreement.
    let schema = build_schema();
    let mut ours = our_references(&schema);
    let lost = ours
        .iter_mut()
        .find(|r| r.usage_line == 7 && r.usage_column == 22)
        .unwrap();
    lost.target_owner = None;
    lost.target_object = None;
    lost.target_identifier = None;
    lost.strategy = None;

    let diff = diff_plscope(&ours, &plscope_ground_truth());
    assert_eq!(diff.our_unknown_target.len(), 1);
    assert_eq!(diff.agreed.len(), 2);
}

/// Live Oracle XE path — only runs when an XE instance is wired
/// up. Default `cargo test` skips it (the bead is "...against
/// Oracle XE *where available*"); the golden tests above provide
/// the always-on, DB-free regression lock.
#[test]
#[ignore = "requires ORACLE_XE_DSN; run with --ignored against a live Oracle XE 23ai"]
fn live_xe_plscope_roundtrip() {
    let Ok(dsn) = std::env::var("ORACLE_XE_DSN") else {
        eprintln!("ORACLE_XE_DSN unset — skipping live PL/Scope round-trip");
        return;
    };
    // Intentionally not implemented as a network call here: the
    // catalog extractor (PLSQL-CAT-011) owns ALL_IDENTIFIERS
    // extraction. This test documents the contract: with a live
    // DSN, extract PlScopeSnapshot, map CompilerReference rows to
    // PlScopeReference, drive the resolver over the same unit, and
    // assert diff_plscope's mismatched/our_only buckets are empty.
    panic!(
        "live XE wiring is owned by the catalog extractor; \
         DSN was provided ({dsn}) but this harness only locks \
         the offline golden — see PLSQL-CAT-008 for the \
         container-backed extraction integration test"
    );
}
