//! `%TYPE` / `%ROWTYPE` anchor resolution against the Layer-2 symbol
//! table (`PLSQL-BG-006`).
//!
//! PL/SQL anchored declarations resolve at compile time:
//!
//! * `<var>%TYPE` — same type as a previously declared variable / param.
//! * `<table>.<col>%TYPE` — same type as a table or view column.
//! * `<table>%ROWTYPE` — a record whose fields mirror the columns of
//!   a table, view, or explicit cursor.
//!
//! This module is the source-only resolver. The input is the raw
//! anchor expression captured in [`AnchoredType::raw`]; the output is
//! a [`ResolvedAnchor`] discriminated by what we found. Anything we
//! cannot resolve from the local DeclTable surfaces as
//! `Unresolved(...)` with a typed reason — never `panic!` — so the
//! caller (PLSQL-BG-002 type-mapping consumer + downstream bindgen)
//! can decide how to render the bindings.
//!
//! Cross-references: PL/SQL Language Reference §2.5 "Using %TYPE
//! attribute" / §2.6 "Using %ROWTYPE attribute"; routing through
//! `~/.claude/skills/oracle/DATABASE-REFERENCE.md` (PL/SQL language
//! topic) and the PL/SQL Language Reference URL anchor.

use plsql_core::SymbolInterner;
use plsql_ir::{AnchoredType, DeclId, DeclKind, Declaration, TypeRef};
use serde::{Deserialize, Serialize};

use crate::table::DeclTable;

/// Outcome of resolving an [`AnchoredType`] against a [`DeclTable`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedAnchor {
    /// `%TYPE` resolved to a variable or column. `ty` is the
    /// underlying type carried by the source declaration (may itself
    /// still be `TypeRef::Unresolved` — type-mapping happens in
    /// PLSQL-BG-002).
    Type {
        anchor_kind: AnchorKind,
        source_decl: DeclId,
        ty: Option<TypeRef>,
    },
    /// `%ROWTYPE` resolved to a table / view. `fields` holds the
    /// columns of that record, in registration order.
    Rowtype {
        source_decl: DeclId,
        fields: Vec<DeclId>,
    },
    /// Resolution failed. The reason discriminator drives R13
    /// (typed UnknownReason) when the bindgen surfaces an opaque
    /// binding.
    Unresolved(AnchorResolutionFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorKind {
    /// `<var>%TYPE` resolved to a variable declaration.
    Variable,
    /// `<table>.<col>%TYPE` resolved to a column declaration.
    Column,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum AnchorResolutionFailure {
    /// The anchor text did not end in `%TYPE` or `%ROWTYPE`.
    UnsupportedAnchor { raw: String },
    /// The anchor named more than three dotted parts (we cap at
    /// `<schema>.<table>.<column>` for `%TYPE` and
    /// `<schema>.<table>` for `%ROWTYPE`).
    TooManyParts { raw: String },
    /// `%TYPE` / `%ROWTYPE` was supplied with no name on the left.
    EmptyName { raw: String },
    /// The named symbol was not interned. Without an interned
    /// SymbolId we cannot consult `DeclTable::by_name`.
    NameNotInterned { name: String },
    /// `%TYPE` referenced `<table>.<col>` but no column with that
    /// name lives under a declaration matching `<table>`.
    ColumnNotFound { table: String, column: String },
    /// `%TYPE` resolved to nothing — neither a variable nor a column.
    NameNotFound { name: String },
    /// `%ROWTYPE` named a symbol that is not a table / view /
    /// cursor.
    NotARowtypeTarget { name: String, found_kind: DeclKind },
}

/// Resolve an [`AnchoredType`] expression against the symbol table.
///
/// Resolution is deterministic and case-insensitive on the way in
/// (we upper-case identifiers to match the Oracle dictionary
/// convention), but the returned `source_decl` keeps the caller's
/// original interner symbol intact.
///
/// The `interner` must already hold the upper-case form of each name
/// referenced by the anchor; if it does not, we surface
/// [`AnchorResolutionFailure::NameNotInterned`] rather than mutating
/// shared state.
pub fn resolve_anchor(
    table: &DeclTable,
    interner: &SymbolInterner,
    anchor: &AnchoredType,
) -> ResolvedAnchor {
    let raw = anchor.raw.trim();
    let upper = raw.to_ascii_uppercase();

    let (name_part, is_rowtype) = if let Some(rest) = upper.strip_suffix("%ROWTYPE") {
        (rest.trim().to_string(), true)
    } else if let Some(rest) = upper.strip_suffix("%TYPE") {
        (rest.trim().to_string(), false)
    } else {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::UnsupportedAnchor {
            raw: raw.to_string(),
        });
    };

    if name_part.is_empty() {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::EmptyName {
            raw: raw.to_string(),
        });
    }

    let parts: Vec<&str> = name_part.split('.').map(str::trim).collect();
    let max_parts = if is_rowtype { 2 } else { 3 };
    if parts.len() > max_parts || parts.iter().any(|p| p.is_empty()) {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::TooManyParts {
            raw: raw.to_string(),
        });
    }

    if is_rowtype {
        // Last segment is the row-source name; an optional first
        // segment is the schema (ignored here — DeclTable already
        // carries `DeclCommon::schema` per declaration, so the name
        // lookup suffices for the local view).
        let name = *parts.last().expect("non-empty after empty-check");
        let Some(sym) = interner.lookup_symbol(name) else {
            return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotInterned {
                name: name.to_string(),
            });
        };
        let candidates = table.by_name(sym);
        for id in &candidates {
            let Some(decl) = table.get(*id) else { continue };
            match decl {
                Declaration::Table(_) | Declaration::View(_) | Declaration::Cursor(_) => {
                    let fields = table.children(*id);
                    return ResolvedAnchor::Rowtype {
                        source_decl: *id,
                        fields,
                    };
                }
                _ => continue,
            }
        }
        if let Some(id) = candidates.first() {
            let kind = table
                .get(*id)
                .map(Declaration::kind)
                .unwrap_or(DeclKind::Variable);
            return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NotARowtypeTarget {
                name: name.to_string(),
                found_kind: kind,
            });
        }
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotFound {
            name: name.to_string(),
        });
    }

    // %TYPE — either bare `<name>` (variable) or `<owner>.<name>` or
    // `<schema>.<table>.<column>` (column).
    match parts.as_slice() {
        [solo] => resolve_variable_type(table, interner, solo),
        [owner, column] => resolve_column_type(table, interner, owner, column),
        [_schema, owner, column] => resolve_column_type(table, interner, owner, column),
        _ => ResolvedAnchor::Unresolved(AnchorResolutionFailure::TooManyParts {
            raw: raw.to_string(),
        }),
    }
}

fn resolve_variable_type(
    table: &DeclTable,
    interner: &SymbolInterner,
    name: &str,
) -> ResolvedAnchor {
    let Some(sym) = interner.lookup_symbol(name) else {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotInterned {
            name: name.to_string(),
        });
    };
    let candidates = table.by_name(sym);
    for id in &candidates {
        let Some(decl) = table.get(*id) else { continue };
        if let Declaration::Variable(v) = decl {
            return ResolvedAnchor::Type {
                anchor_kind: AnchorKind::Variable,
                source_decl: *id,
                ty: v.ty.clone(),
            };
        }
    }
    // Stand-alone identifier didn't match a variable — fall back to
    // searching for a column with that name (rare but legal when the
    // caller wrote `EMPLOYEES_SAL%TYPE` against a single-table view).
    for id in &candidates {
        let Some(decl) = table.get(*id) else { continue };
        if let Declaration::Column(c) = decl {
            return ResolvedAnchor::Type {
                anchor_kind: AnchorKind::Column,
                source_decl: *id,
                ty: c.ty.clone(),
            };
        }
    }
    ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotFound {
        name: name.to_string(),
    })
}

fn resolve_column_type(
    table: &DeclTable,
    interner: &SymbolInterner,
    owner: &str,
    column: &str,
) -> ResolvedAnchor {
    let Some(owner_sym) = interner.lookup_symbol(owner) else {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotInterned {
            name: owner.to_string(),
        });
    };
    let Some(col_sym) = interner.lookup_symbol(column) else {
        return ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotInterned {
            name: column.to_string(),
        });
    };
    let owner_candidates = table.by_name(owner_sym);
    for owner_id in &owner_candidates {
        let Some(decl) = table.get(*owner_id) else {
            continue;
        };
        match decl {
            Declaration::Table(_) | Declaration::View(_) | Declaration::Cursor(_) => {
                for child_id in table.children(*owner_id) {
                    let Some(child) = table.get(child_id) else {
                        continue;
                    };
                    if child.common().name == col_sym
                        && let Declaration::Column(c) = child
                    {
                        return ResolvedAnchor::Type {
                            anchor_kind: AnchorKind::Column,
                            source_decl: child_id,
                            ty: c.ty.clone(),
                        };
                    }
                }
            }
            _ => continue,
        }
    }
    ResolvedAnchor::Unresolved(AnchorResolutionFailure::ColumnNotFound {
        table: owner.to_string(),
        column: column.to_string(),
    })
}

/// Trait shim — `SymbolInterner` exposes `intern` (mutating) but no
/// `lookup` accessor. We need read-only "does this name exist?"
/// during resolution, so we add a single helper here that walks the
/// interner's vec via the public `resolve` / `contains` shape.
trait LookupSymbol {
    fn lookup_symbol(&self, text: &str) -> Option<plsql_core::SymbolId>;
}

impl LookupSymbol for SymbolInterner {
    fn lookup_symbol(&self, text: &str) -> Option<plsql_core::SymbolId> {
        if !self.contains(text) {
            return None;
        }
        // `SymbolInterner::contains` only confirms presence; we still
        // need the SymbolId. Walk the public iter-style API by
        // calling `resolve` from low to high until we hit a match.
        // The interner never reorders symbols so this lookup is
        // stable; it is O(N) but the interner is small in practice
        // (one entry per distinct identifier in the analysed corpus).
        (0..self.len())
            .find(|i| self.resolve(plsql_core::SymbolId::new(*i as u64)) == Some(text))
            .map(|i| plsql_core::SymbolId::new(i as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position, Span};
    use plsql_ir::{
        ColumnDecl, CursorDecl, DeclCommon, TableDecl, TypeRef, VariableDecl, ViewDecl,
    };

    fn span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 1, 0),
        )
    }

    fn setup() -> (DeclTable, SymbolInterner) {
        let mut interner = SymbolInterner::new();
        let mut table = DeclTable::new();

        let salary_var_name = interner.intern("V_SALARY").unwrap();
        let emp_name = interner.intern("EMPLOYEES").unwrap();
        let sal_col_name = interner.intern("SALARY").unwrap();
        let id_col_name = interner.intern("ID").unwrap();
        let view_name = interner.intern("EMP_VIEW").unwrap();
        let cur_name = interner.intern("C_BATCH").unwrap();

        // V_SALARY NUMBER;
        table.register(Declaration::Variable(VariableDecl {
            common: DeclCommon::new(salary_var_name, span()),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            default_text: None,
            constant: false,
            not_null: false,
        }));

        // EMPLOYEES (ID NUMBER, SALARY NUMBER).
        let emp = table.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(emp_name, span()),
            columns: vec![],
        }));
        table.register(Declaration::Column(ColumnDecl {
            common: DeclCommon::new(id_col_name, span()).with_parent(emp),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            not_null: true,
        }));
        table.register(Declaration::Column(ColumnDecl {
            common: DeclCommon::new(sal_col_name, span()).with_parent(emp),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            not_null: false,
        }));

        // EMP_VIEW (ID NUMBER) — view with one column.
        let v = table.register(Declaration::View(ViewDecl {
            common: DeclCommon::new(view_name, span()),
            columns: vec![],
        }));
        table.register(Declaration::Column(ColumnDecl {
            common: DeclCommon::new(id_col_name, span()).with_parent(v),
            ty: Some(TypeRef::Unresolved("NUMBER".into())),
            not_null: true,
        }));

        // C_BATCH cursor.
        table.register(Declaration::Cursor(CursorDecl {
            common: DeclCommon::new(cur_name, span()),
        }));

        (table, interner)
    }

    fn anchor(raw: &str) -> AnchoredType {
        AnchoredType { raw: raw.into() }
    }

    #[test]
    fn variable_type_resolves_to_variable_decl() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("V_SALARY%TYPE"));
        match r {
            ResolvedAnchor::Type {
                anchor_kind, ty, ..
            } => {
                assert_eq!(anchor_kind, AnchorKind::Variable);
                assert!(matches!(ty, Some(TypeRef::Unresolved(s)) if s == "NUMBER"));
            }
            other => panic!("expected Type, got {other:?}"),
        }
    }

    #[test]
    fn column_type_resolves_two_part() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("EMPLOYEES.SALARY%TYPE"));
        match r {
            ResolvedAnchor::Type {
                anchor_kind, ty, ..
            } => {
                assert_eq!(anchor_kind, AnchorKind::Column);
                assert!(matches!(ty, Some(TypeRef::Unresolved(s)) if s == "NUMBER"));
            }
            other => panic!("expected Type, got {other:?}"),
        }
    }

    #[test]
    fn column_type_resolves_three_part_qualified() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("HR.EMPLOYEES.SALARY%TYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Type {
                anchor_kind: AnchorKind::Column,
                ..
            }
        ));
    }

    #[test]
    fn rowtype_returns_table_fields() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("EMPLOYEES%ROWTYPE"));
        match r {
            ResolvedAnchor::Rowtype { fields, .. } => {
                assert_eq!(fields.len(), 2, "EMPLOYEES has 2 columns");
            }
            other => panic!("expected Rowtype, got {other:?}"),
        }
    }

    #[test]
    fn rowtype_works_on_view() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("EMP_VIEW%ROWTYPE"));
        match r {
            ResolvedAnchor::Rowtype { fields, .. } => assert_eq!(fields.len(), 1),
            other => panic!("expected Rowtype, got {other:?}"),
        }
    }

    #[test]
    fn rowtype_works_on_cursor() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("C_BATCH%ROWTYPE"));
        assert!(matches!(r, ResolvedAnchor::Rowtype { .. }));
    }

    #[test]
    fn rowtype_against_variable_is_unresolved() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("V_SALARY%ROWTYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::NotARowtypeTarget { .. })
        ));
    }

    #[test]
    fn column_not_found_under_known_table() {
        let mut interner = SymbolInterner::new();
        let _ = interner.intern("EMPLOYEES");
        let _ = interner.intern("NOPE");
        // Set up a table with no columns named NOPE.
        let mut t = DeclTable::new();
        let _emp = t.register(Declaration::Table(TableDecl {
            common: DeclCommon::new(interner.intern("EMPLOYEES").unwrap(), span()),
            columns: vec![],
        }));
        let r = resolve_anchor(&t, &interner, &anchor("EMPLOYEES.NOPE%TYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::ColumnNotFound { .. })
        ));
    }

    #[test]
    fn unsupported_anchor_text_rejected() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("EMPLOYEES"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::UnsupportedAnchor { .. })
        ));
    }

    #[test]
    fn empty_name_rejected() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("%TYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::EmptyName { .. })
        ));
    }

    #[test]
    fn too_many_parts_rejected() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("A.B.C.D%TYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::TooManyParts { .. })
        ));
    }

    #[test]
    fn name_not_interned_surfaces() {
        let table = DeclTable::new();
        let interner = SymbolInterner::new();
        let r = resolve_anchor(&table, &interner, &anchor("MISSING%TYPE"));
        assert!(matches!(
            r,
            ResolvedAnchor::Unresolved(AnchorResolutionFailure::NameNotInterned { .. })
        ));
    }

    #[test]
    fn lowercase_anchor_is_resolved() {
        let (table, interner) = setup();
        let r = resolve_anchor(&table, &interner, &anchor("employees.salary%type"));
        assert!(matches!(
            r,
            ResolvedAnchor::Type {
                anchor_kind: AnchorKind::Column,
                ..
            }
        ));
    }
}
