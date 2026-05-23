//! Catalog-fact feed into symbol resolution.
//!
//! The source-only resolvers ([`crate::resolve_anchor`],
//! [`crate::resolve_reference`], [`crate::resolve_overload`]) only
//! see what the local `DeclTable` holds. Real schemas constantly
//! reference objects that exist *only* in the Oracle catalog — a
//! `%TYPE` anchored to a table the engagement never shipped source
//! for, a public synonym, a packaged overload set declared in a
//! spec we did not parse, an index that proves a column exists.
//!
//! This module defines the **interface** through which those
//! catalog facts reach resolution, plus the fallback combinators
//! that try source first and the catalog second. It deliberately
//! does *not* depend on `plsql-catalog`: the resolver layer must
//! not import the catalog layer's concrete types (it would also
//! invert the dependency for the `RoutineSignature` adapter). The
//! engine layer — which already owns both a `CatalogSnapshot` and
//! the symbol tables — implements [`CatalogResolutionSource`] over
//! the snapshot. This mirrors the `DeclLike` / `StoredFact`
//! decoupling used elsewhere in the workspace.
//!
//! Fact families fed (one method per family):
//!
//! * `%TYPE`  — [`CatalogResolutionSource::column_type`]
//! * `%ROWTYPE` — [`CatalogResolutionSource::rowtype_columns`]
//! * synonyms — [`CatalogResolutionSource::synonym_target`]
//! * overloads — [`CatalogResolutionSource::overloads`]
//!   (yields [`crate::RoutineSignature`] so the SYM-009 resolver
//!   consumes catalog and source overloads through one code path)
//! * indexed columns — [`CatalogResolutionSource::indexed_columns`]
//!   (an index naming a column is positive evidence the column
//!   exists even when no column source was parsed)
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL Language Reference — `%TYPE` /
//!   `%ROWTYPE` resolve against the catalog when no source
//!   declaration is visible.
//! * `LOW-LEVEL-CATALOGS.md` — `ALL_TAB_COLUMNS`, `ALL_SYNONYMS`,
//!   `ALL_ARGUMENTS`, `ALL_IND_COLUMNS` are the dictionary views the
//!   engine adapter reads to satisfy these methods.

use serde::{Deserialize, Serialize};

use plsql_core::SymbolInterner;
use plsql_ir::AnchoredType;

use crate::overload::{CallArg, OverloadResolution, RoutineSignature, resolve_overload};
use crate::resolve_anchor::{AnchorResolutionFailure, ResolvedAnchor, resolve_anchor};
use crate::table::DeclTable;

/// One catalog column fact (`ALL_TAB_COLUMNS` row, normalized).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogColumnFact {
    /// Column name, upper-cased.
    pub column: String,
    /// Rendered data type (e.g. `NUMBER`, `VARCHAR2`, `HR.ADDR_T`).
    pub type_name: String,
    /// 1-based column position, for stable `%ROWTYPE` field order.
    pub position: u32,
}

/// One catalog synonym fact (`ALL_SYNONYMS` row, normalized).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogSynonymFact {
    /// Owning schema of the target, if known.
    pub target_schema: Option<String>,
    /// Target object name, upper-cased.
    pub target_object: String,
    /// `true` for a `PUBLIC` synonym.
    pub public_synonym: bool,
    /// Remote database link, when the synonym points across one.
    pub db_link: Option<String>,
}

/// One catalog index fact (`ALL_IND_COLUMNS`, grouped per index).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogIndexedColumnFact {
    /// Index name, upper-cased.
    pub index: String,
    pub unique: bool,
    /// Indexed columns in key order, upper-cased.
    pub columns: Vec<String>,
}

/// The catalog-fact lookups the resolver needs. Implemented by the
/// engine layer over a `CatalogSnapshot`. All names are passed
/// upper-cased (Oracle dictionary convention); implementors must
/// compare case-insensitively if their backing store differs.
pub trait CatalogResolutionSource {
    /// `<schema>.<object>.<column>%TYPE` — the column's data type.
    fn column_type(&self, schema: &str, object: &str, column: &str) -> Option<String>;

    /// `<schema>.<object>%ROWTYPE` — the object's columns in
    /// position order. `None` when the object is not a
    /// table/view/mview in the catalog.
    fn rowtype_columns(&self, schema: &str, object: &str) -> Option<Vec<CatalogColumnFact>>;

    /// Resolve a synonym (private in `schema`, or `PUBLIC`).
    fn synonym_target(&self, schema: &str, synonym: &str) -> Option<CatalogSynonymFact>;

    /// Indexes on `<schema>.<table>`. Empty when none / unknown.
    fn indexed_columns(&self, schema: &str, table: &str) -> Vec<CatalogIndexedColumnFact>;

    /// Catalog-declared overload set for a routine. `package` is
    /// `Some` for a packaged subprogram, `None` for a standalone
    /// one. Returned [`RoutineSignature`]s plug straight into
    /// [`resolve_overload`].
    fn overloads(
        &self,
        schema: &str,
        package: Option<&str>,
        routine: &str,
    ) -> Vec<RoutineSignature>;
}

/// Outcome of anchor resolution that may have fallen back to the
/// catalog. Kept distinct from [`ResolvedAnchor`] because a
/// catalog-only object has no `DeclId` — faking one would corrupt
/// downstream decl lookups.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum CatalogBackedAnchor {
    /// Resolved entirely from parsed source — carries the original
    /// [`ResolvedAnchor`].
    Source(ResolvedAnchor),
    /// `%TYPE` resolved against a catalog column.
    CatalogType { type_name: String },
    /// `%ROWTYPE` resolved against a catalog object's columns.
    CatalogRowtype { columns: Vec<CatalogColumnFact> },
    /// Neither source nor catalog could resolve it. Carries the
    /// source-side failure for diagnostics.
    Unresolved(AnchorResolutionFailure),
}

/// Resolve `anchor` against parsed source first; on a
/// catalog-shaped miss (column/object/name not found in the
/// `DeclTable`), retry against `catalog`.
///
/// Source resolution always wins when it succeeds — parsed source
/// is higher-confidence than a catalog snapshot that may be a
/// different edition. Only genuine source misses fall through.
#[must_use]
pub fn resolve_anchor_with_catalog(
    table: &DeclTable,
    interner: &SymbolInterner,
    anchor: &AnchoredType,
    catalog: &dyn CatalogResolutionSource,
) -> CatalogBackedAnchor {
    let source = resolve_anchor(table, interner, anchor);
    let failure = match source {
        ResolvedAnchor::Unresolved(f) => f,
        resolved => return CatalogBackedAnchor::Source(resolved),
    };

    // Parse the anchor shape ourselves only for the fallback — the
    // source resolver already validated it, so we trust the suffix
    // split here.
    let raw = anchor.raw.trim();
    let upper = raw.to_ascii_uppercase();
    let (name_part, is_rowtype) = if let Some(r) = upper.strip_suffix("%ROWTYPE") {
        (r.trim(), true)
    } else if let Some(r) = upper.strip_suffix("%TYPE") {
        (r.trim(), false)
    } else {
        return CatalogBackedAnchor::Unresolved(failure);
    };

    let parts: Vec<&str> = name_part.split('.').map(str::trim).collect();
    if is_rowtype {
        // <schema>.<object> or <object> (object's owning schema is
        // unknown to a bare reference — only the qualified form can
        // hit the catalog deterministically).
        if let [schema, object] = parts.as_slice()
            && let Some(cols) = catalog.rowtype_columns(schema, object)
        {
            return CatalogBackedAnchor::CatalogRowtype { columns: cols };
        }
    } else if let [schema, object, column] = parts.as_slice()
        && let Some(ty) = catalog.column_type(schema, object, column)
    {
        return CatalogBackedAnchor::CatalogType { type_name: ty };
    }

    CatalogBackedAnchor::Unresolved(failure)
}

/// Resolve a call's overload set using catalog-declared signatures.
/// Thin bridge so callers don't have to hand-wire SYM-009 to the
/// catalog feed: fetch the overloads, then delegate to
/// [`resolve_overload`]. Returns `NoMatch` with no reasons when the
/// catalog knows no such routine (distinct from "knows it, nothing
/// bound" — that carries per-candidate reasons).
#[must_use]
pub fn resolve_catalog_overload(
    catalog: &dyn CatalogResolutionSource,
    schema: &str,
    package: Option<&str>,
    routine: &str,
    args: &[CallArg],
) -> OverloadResolution {
    let candidates = catalog.overloads(schema, package, routine);
    resolve_overload(&candidates, args)
}

/// Follow a synonym through the catalog, capped to bound hostile or
/// accidental cycles (mirrors `resolve_refs::MAX_SYNONYM_HOPS`).
/// Returns the terminal `(schema, object)` or `None` if the chain
/// dead-ends, loops, or crosses a db-link (remote objects are
/// opaque to static analysis).
#[must_use]
pub fn follow_catalog_synonym(
    catalog: &dyn CatalogResolutionSource,
    schema: &str,
    synonym: &str,
) -> Option<(Option<String>, String)> {
    const MAX_HOPS: u8 = 8;
    let mut cur_schema = schema.to_ascii_uppercase();
    let mut cur_name = synonym.to_ascii_uppercase();
    let mut seen: Vec<(String, String)> = Vec::new();

    // Exactly one catalog lookup per iteration, so the loop's
    // exclusive `0..MAX_HOPS` bound is the *total* number of catalog
    // `synonym_target` calls — genuinely matching the documented cap
    // and `resolve_refs::MAX_SYNONYM_HOPS`. (The previous form did a
    // second "is the target itself a synonym?" probe per hop, which
    // doubled the catalog calls a hostile chain could induce.)
    let mut last_terminal: Option<(Option<String>, String)> = None;
    for _ in 0..MAX_HOPS {
        if seen.iter().any(|(s, n)| s == &cur_schema && n == &cur_name) {
            return None; // cycle
        }
        seen.push((cur_schema.clone(), cur_name.clone()));

        let fact = match catalog.synonym_target(&cur_schema, &cur_name) {
            Some(f) => f,
            // The current name is not a synonym: whatever synonym
            // resolved to it on the previous hop was the terminal
            // object. `None` here for the first iteration means the
            // starting name was not a synonym at all.
            None => return last_terminal,
        };
        if fact.db_link.is_some() {
            return None; // remote object: opaque
        }
        let target_object = fact.target_object.clone();

        match fact.target_schema.clone() {
            // Target schema known: record it as the best terminal so
            // far and keep walking (the next iteration's single
            // lookup decides whether it is itself a synonym).
            Some(ts) => {
                last_terminal = Some((Some(ts.clone()), target_object.clone()));
                cur_schema = ts;
                cur_name = target_object;
            }
            // Bare, unqualified target: the owning schema is not
            // recorded, so we cannot deterministically follow further
            // (Oracle would apply public-synonym/current-schema
            // resolution at runtime). Treat it as the terminal object
            // rather than guessing the wrong schema.
            None => return Some((None, target_object)),
        }
    }
    None // exceeded hop cap
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::SymbolInterner;
    use plsql_ir::{AnchoredType, ParamMode};
    use std::collections::HashMap;

    use crate::overload::ParamSig;

    /// Hand-built catalog stand-in. The engine's real impl wraps a
    /// `CatalogSnapshot`; this one proves the combinators.
    #[derive(Default)]
    struct FakeCatalog {
        columns: HashMap<(String, String, String), String>,
        rowtypes: HashMap<(String, String), Vec<CatalogColumnFact>>,
        synonyms: HashMap<(String, String), CatalogSynonymFact>,
        indexes: HashMap<(String, String), Vec<CatalogIndexedColumnFact>>,
        overloads: HashMap<(String, String, String), Vec<RoutineSignature>>,
    }

    impl CatalogResolutionSource for FakeCatalog {
        fn column_type(&self, s: &str, o: &str, c: &str) -> Option<String> {
            self.columns.get(&(s.into(), o.into(), c.into())).cloned()
        }
        fn rowtype_columns(&self, s: &str, o: &str) -> Option<Vec<CatalogColumnFact>> {
            self.rowtypes.get(&(s.into(), o.into())).cloned()
        }
        fn synonym_target(&self, s: &str, n: &str) -> Option<CatalogSynonymFact> {
            self.synonyms.get(&(s.into(), n.into())).cloned()
        }
        fn indexed_columns(&self, s: &str, t: &str) -> Vec<CatalogIndexedColumnFact> {
            self.indexes
                .get(&(s.into(), t.into()))
                .cloned()
                .unwrap_or_default()
        }
        fn overloads(&self, s: &str, p: Option<&str>, r: &str) -> Vec<RoutineSignature> {
            self.overloads
                .get(&(s.into(), p.unwrap_or("").into(), r.into()))
                .cloned()
                .unwrap_or_default()
        }
    }

    fn anchor(raw: &str) -> AnchoredType {
        AnchoredType {
            raw: raw.to_string(),
        }
    }

    #[test]
    fn type_anchor_falls_back_to_catalog_when_source_misses() {
        let table = DeclTable::new();
        let interner = SymbolInterner::new();
        let mut cat = FakeCatalog::default();
        cat.columns.insert(
            ("HR".into(), "EMPLOYEES".into(), "SALARY".into()),
            "NUMBER".into(),
        );
        let r = resolve_anchor_with_catalog(
            &table,
            &interner,
            &anchor("HR.EMPLOYEES.SALARY%TYPE"),
            &cat,
        );
        assert_eq!(
            r,
            CatalogBackedAnchor::CatalogType {
                type_name: "NUMBER".into()
            }
        );
    }

    #[test]
    fn rowtype_anchor_falls_back_to_catalog() {
        let table = DeclTable::new();
        let interner = SymbolInterner::new();
        let mut cat = FakeCatalog::default();
        cat.rowtypes.insert(
            ("HR".into(), "DEPARTMENTS".into()),
            vec![
                CatalogColumnFact {
                    column: "DEPARTMENT_ID".into(),
                    type_name: "NUMBER".into(),
                    position: 1,
                },
                CatalogColumnFact {
                    column: "DEPARTMENT_NAME".into(),
                    type_name: "VARCHAR2".into(),
                    position: 2,
                },
            ],
        );
        let r =
            resolve_anchor_with_catalog(&table, &interner, &anchor("HR.DEPARTMENTS%ROWTYPE"), &cat);
        match r {
            CatalogBackedAnchor::CatalogRowtype { columns } => {
                assert_eq!(columns.len(), 2);
                assert_eq!(columns[0].column, "DEPARTMENT_ID");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unresolved_when_neither_source_nor_catalog_has_it() {
        let table = DeclTable::new();
        let interner = SymbolInterner::new();
        let cat = FakeCatalog::default();
        let r = resolve_anchor_with_catalog(&table, &interner, &anchor("HR.GHOST.COL%TYPE"), &cat);
        assert!(matches!(r, CatalogBackedAnchor::Unresolved(_)));
    }

    #[test]
    fn unsupported_anchor_shape_stays_unresolved() {
        let table = DeclTable::new();
        let interner = SymbolInterner::new();
        let cat = FakeCatalog::default();
        let r = resolve_anchor_with_catalog(&table, &interner, &anchor("NOT_AN_ANCHOR"), &cat);
        assert!(matches!(r, CatalogBackedAnchor::Unresolved(_)));
    }

    #[test]
    fn catalog_overloads_resolve_through_sym009() {
        let mut cat = FakeCatalog::default();
        cat.overloads.insert(
            ("HR".into(), "PAY_PKG".into(), "POST".into()),
            vec![
                RoutineSignature {
                    decl: plsql_ir::DeclId::new(0),
                    name: "POST".into(),
                    params: vec![ParamSig {
                        name: "AMOUNT".into(),
                        mode: ParamMode::In,
                        type_name: Some("NUMBER".into()),
                        has_default: false,
                    }],
                    is_function: false,
                },
                RoutineSignature {
                    decl: plsql_ir::DeclId::new(1),
                    name: "POST".into(),
                    params: vec![ParamSig {
                        name: "AMOUNT".into(),
                        mode: ParamMode::In,
                        type_name: Some("VARCHAR2".into()),
                        has_default: false,
                    }],
                    is_function: false,
                },
            ],
        );
        let r = resolve_catalog_overload(
            &cat,
            "HR",
            Some("PAY_PKG"),
            "POST",
            &[CallArg::positional(Some("NUMBER"))],
        );
        assert!(matches!(
            r,
            OverloadResolution::Resolved { decl, .. } if decl == plsql_ir::DeclId::new(0)
        ));
    }

    #[test]
    fn unknown_catalog_routine_is_no_match_without_reasons() {
        let cat = FakeCatalog::default();
        let r = resolve_catalog_overload(&cat, "HR", None, "MISSING", &[]);
        assert_eq!(r, OverloadResolution::NoMatch { reasons: vec![] });
    }

    #[test]
    fn synonym_chain_followed_to_terminal_object() {
        let mut cat = FakeCatalog::default();
        cat.synonyms.insert(
            ("APP".into(), "EMP".into()),
            CatalogSynonymFact {
                target_schema: Some("APP".into()),
                target_object: "EMP_SYN2".into(),
                public_synonym: false,
                db_link: None,
            },
        );
        cat.synonyms.insert(
            ("APP".into(), "EMP_SYN2".into()),
            CatalogSynonymFact {
                target_schema: Some("HR".into()),
                target_object: "EMPLOYEES".into(),
                public_synonym: false,
                db_link: None,
            },
        );
        let t = follow_catalog_synonym(&cat, "APP", "EMP");
        assert_eq!(t, Some((Some("HR".into()), "EMPLOYEES".into())));
    }

    #[test]
    fn synonym_cycle_returns_none() {
        let mut cat = FakeCatalog::default();
        cat.synonyms.insert(
            ("S".into(), "A".into()),
            CatalogSynonymFact {
                target_schema: Some("S".into()),
                target_object: "B".into(),
                public_synonym: false,
                db_link: None,
            },
        );
        cat.synonyms.insert(
            ("S".into(), "B".into()),
            CatalogSynonymFact {
                target_schema: Some("S".into()),
                target_object: "A".into(),
                public_synonym: false,
                db_link: None,
            },
        );
        assert_eq!(follow_catalog_synonym(&cat, "S", "A"), None);
    }

    #[test]
    fn bare_unqualified_synonym_target_is_terminal_not_probed() {
        // Target schema is None: the resolver must NOT probe the
        // current schema (which could walk the wrong object); it
        // returns the bare name as terminal.
        let mut cat = FakeCatalog::default();
        cat.synonyms.insert(
            ("APP".into(), "BARE".into()),
            CatalogSynonymFact {
                target_schema: None,
                target_object: "WIDGETS".into(),
                public_synonym: true,
                db_link: None,
            },
        );
        // A same-named synonym in APP must NOT be followed, because
        // the target schema was unqualified.
        cat.synonyms.insert(
            ("APP".into(), "WIDGETS".into()),
            CatalogSynonymFact {
                target_schema: Some("WRONG".into()),
                target_object: "DECOY".into(),
                public_synonym: false,
                db_link: None,
            },
        );
        assert_eq!(
            follow_catalog_synonym(&cat, "APP", "BARE"),
            Some((None, "WIDGETS".into()))
        );
    }

    #[test]
    fn synonym_chain_exceeding_hop_cap_returns_none() {
        // 10 chained synonyms S.N0 -> S.N1 -> ... -> S.N10. With an
        // 8-hop cap the walk must give up (None) rather than resolve
        // or loop.
        let mut cat = FakeCatalog::default();
        for i in 0..10 {
            cat.synonyms.insert(
                ("S".into(), format!("N{i}")),
                CatalogSynonymFact {
                    target_schema: Some("S".into()),
                    target_object: format!("N{}", i + 1),
                    public_synonym: false,
                    db_link: None,
                },
            );
        }
        assert_eq!(follow_catalog_synonym(&cat, "S", "N0"), None);
    }

    #[test]
    fn synonym_across_db_link_is_opaque() {
        let mut cat = FakeCatalog::default();
        cat.synonyms.insert(
            ("APP".into(), "REMOTE_T".into()),
            CatalogSynonymFact {
                target_schema: Some("REM".into()),
                target_object: "T".into(),
                public_synonym: false,
                db_link: Some("ORCL_REMOTE".into()),
            },
        );
        assert_eq!(follow_catalog_synonym(&cat, "APP", "REMOTE_T"), None);
    }

    #[test]
    fn indexed_columns_surface_through_feed() {
        let mut cat = FakeCatalog::default();
        cat.indexes.insert(
            ("HR".into(), "EMPLOYEES".into()),
            vec![CatalogIndexedColumnFact {
                index: "EMP_PK".into(),
                unique: true,
                columns: vec!["EMPLOYEE_ID".into()],
            }],
        );
        let ix = cat.indexed_columns("HR", "EMPLOYEES");
        assert_eq!(ix.len(), 1);
        assert!(ix[0].unique);
        assert_eq!(ix[0].columns, vec!["EMPLOYEE_ID".to_string()]);
    }
}
