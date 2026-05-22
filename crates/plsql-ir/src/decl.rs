//! Declaration variants populated by AST→IR lowering.
//!
//! `PLSQL-IR-002` introduces the [`Declaration`] enum with one variant per
//! kind of named entity the engine reasons about. Each variant carries a
//! shared [`DeclCommon`] payload (name, span, owning schema, optional
//! parent declaration) plus a small number of variant-specific fields.
//! The actual lowering from parser AST to these declarations lands in
//! `PLSQL-IR-003` (top-level) and later beads; the registration pass
//! (`DeclTable` + scope chain) lands in `PLSQL-SYM-001`.
//!
//! Subsequent beads will refine the placeholder [`TypeRef`] payloads once
//! type resolution (`PLSQL-SYM-010`) cross-checks against the catalog.

use plsql_core::{SchemaName, Span, SymbolId};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::DeclId;

/// Shared metadata carried by every declaration variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclCommon {
    /// Interned source name of the declared entity (case folded per
    /// Oracle quoting rules at intern time).
    pub name: SymbolId,
    /// Span of the declaration site in the originating source file.
    pub span: Span,
    /// Owning schema. `None` for local-scope declarations (block-scoped
    /// variables, parameters, cursors, exception handlers) — those are
    /// resolved against the enclosing routine's scope, not a schema.
    pub schema: Option<SchemaName>,
    /// Enclosing declaration: package for package-member routines,
    /// table for columns/triggers/indexes, type for type-body methods.
    /// `None` for top-level objects.
    pub parent: Option<DeclId>,
}

impl DeclCommon {
    #[must_use]
    #[instrument(level = "trace")]
    pub fn new(name: SymbolId, span: Span) -> Self {
        Self {
            name,
            span,
            schema: None,
            parent: None,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_schema(mut self, schema: SchemaName) -> Self {
        self.schema = Some(schema);
        self
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn with_parent(mut self, parent: DeclId) -> Self {
        self.parent = Some(parent);
        self
    }
}

/// Direction-of-flow marker on procedure/function parameters.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub enum ParamMode {
    #[default]
    In,
    Out,
    InOut,
}

/// Type reference attached to typed declarations.
///
/// Lowering produces [`TypeRef::Unresolved`] holding the raw source text;
/// `PLSQL-SYM-010` resolves `%TYPE` / `%ROWTYPE` anchors against the
/// catalog and later beads narrow `Unresolved` into a structured
/// representation. Keeping this an enum from day one means downstream
/// crates do not have to be re-shaped when richer resolution lands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeRef {
    /// Raw type expression from source, awaiting resolution.
    Unresolved(String),
    /// `%TYPE` or `%ROWTYPE` anchor; resolution target captured for
    /// later cross-check against catalog metadata.
    Anchored(AnchoredType),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchoredType {
    pub raw: String,
}

/// Discriminator counterpart to [`Declaration`] for fast dispatch and
/// fact tagging without pattern-matching the full enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum DeclKind {
    Variable,
    Param,
    Cursor,
    Procedure,
    Function,
    Package,
    Type,
    Table,
    View,
    Column,
    Sequence,
    Synonym,
    Index,
    Trigger,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableDecl {
    pub common: DeclCommon,
    pub ty: Option<TypeRef>,
    pub default_text: Option<String>,
    pub constant: bool,
    pub not_null: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamDecl {
    pub common: DeclCommon,
    pub mode: ParamMode,
    pub ty: Option<TypeRef>,
    pub default_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorDecl {
    pub common: DeclCommon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureDecl {
    pub common: DeclCommon,
    pub params: Vec<DeclId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub common: DeclCommon,
    pub params: Vec<DeclId>,
    pub return_type: Option<TypeRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDecl {
    pub common: DeclCommon,
    pub members: Vec<DeclId>,
    pub body: Option<DeclId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub common: DeclCommon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableDecl {
    pub common: DeclCommon,
    pub columns: Vec<DeclId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDecl {
    pub common: DeclCommon,
    pub columns: Vec<DeclId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDecl {
    pub common: DeclCommon,
    pub ty: Option<TypeRef>,
    pub not_null: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceDecl {
    pub common: DeclCommon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynonymDecl {
    pub common: DeclCommon,
    /// Object the synonym resolves to once `PLSQL-SYM-003` runs.
    pub target: Option<DeclId>,
    pub public_synonym: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDecl {
    pub common: DeclCommon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerDecl {
    pub common: DeclCommon,
}

/// Discriminated union of every kind of declaration the IR recognizes.
///
/// New variants are additive within this bead's scope; reshape decisions
/// are deferred to `PLSQL-IR-003` (top-level lowering) and the symbol
/// resolution beads (`PLSQL-SYM-*`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Declaration {
    Variable(VariableDecl),
    Param(ParamDecl),
    Cursor(CursorDecl),
    Procedure(ProcedureDecl),
    Function(FunctionDecl),
    Package(PackageDecl),
    Type(TypeDecl),
    Table(TableDecl),
    View(ViewDecl),
    Column(ColumnDecl),
    Sequence(SequenceDecl),
    Synonym(SynonymDecl),
    Index(IndexDecl),
    Trigger(TriggerDecl),
}

impl Declaration {
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn common(&self) -> &DeclCommon {
        match self {
            Self::Variable(d) => &d.common,
            Self::Param(d) => &d.common,
            Self::Cursor(d) => &d.common,
            Self::Procedure(d) => &d.common,
            Self::Function(d) => &d.common,
            Self::Package(d) => &d.common,
            Self::Type(d) => &d.common,
            Self::Table(d) => &d.common,
            Self::View(d) => &d.common,
            Self::Column(d) => &d.common,
            Self::Sequence(d) => &d.common,
            Self::Synonym(d) => &d.common,
            Self::Index(d) => &d.common,
            Self::Trigger(d) => &d.common,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> DeclKind {
        match self {
            Self::Variable(_) => DeclKind::Variable,
            Self::Param(_) => DeclKind::Param,
            Self::Cursor(_) => DeclKind::Cursor,
            Self::Procedure(_) => DeclKind::Procedure,
            Self::Function(_) => DeclKind::Function,
            Self::Package(_) => DeclKind::Package,
            Self::Type(_) => DeclKind::Type,
            Self::Table(_) => DeclKind::Table,
            Self::View(_) => DeclKind::View,
            Self::Column(_) => DeclKind::Column,
            Self::Sequence(_) => DeclKind::Sequence,
            Self::Synonym(_) => DeclKind::Synonym,
            Self::Index(_) => DeclKind::Index,
            Self::Trigger(_) => DeclKind::Trigger,
        }
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn name(&self) -> SymbolId {
        self.common().name
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn span(&self) -> Span {
        self.common().span
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_callable(&self) -> bool {
        matches!(self, Self::Procedure(_) | Self::Function(_))
    }

    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_schema_object(&self) -> bool {
        matches!(
            self,
            Self::Package(_)
                | Self::Type(_)
                | Self::Table(_)
                | Self::View(_)
                | Self::Sequence(_)
                | Self::Synonym(_)
                | Self::Index(_)
                | Self::Trigger(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position};

    fn dummy_span() -> Span {
        Span::new(
            FileId::new(1),
            Position::new(1, 1, 0),
            Position::new(1, 5, 4),
        )
    }

    fn common_with_name(raw: u64) -> DeclCommon {
        DeclCommon::new(SymbolId::new(raw), dummy_span())
    }

    #[test]
    fn decl_common_builders_set_optional_fields() {
        let schema = SchemaName::from(SymbolId::new(10));
        let common = common_with_name(1)
            .with_schema(schema)
            .with_parent(DeclId::new(99));
        assert_eq!(common.schema, Some(schema));
        assert_eq!(common.parent, Some(DeclId::new(99)));
    }

    #[test]
    fn declaration_kind_matches_variant() {
        let cases: Vec<(Declaration, DeclKind)> = vec![
            (
                Declaration::Variable(VariableDecl {
                    common: common_with_name(1),
                    ty: Some(TypeRef::Unresolved("NUMBER".into())),
                    default_text: None,
                    constant: false,
                    not_null: false,
                }),
                DeclKind::Variable,
            ),
            (
                Declaration::Param(ParamDecl {
                    common: common_with_name(2),
                    mode: ParamMode::Out,
                    ty: None,
                    default_text: None,
                }),
                DeclKind::Param,
            ),
            (
                Declaration::Cursor(CursorDecl {
                    common: common_with_name(3),
                }),
                DeclKind::Cursor,
            ),
            (
                Declaration::Procedure(ProcedureDecl {
                    common: common_with_name(4),
                    params: vec![DeclId::new(2)],
                }),
                DeclKind::Procedure,
            ),
            (
                Declaration::Function(FunctionDecl {
                    common: common_with_name(5),
                    params: vec![],
                    return_type: Some(TypeRef::Unresolved("VARCHAR2".into())),
                }),
                DeclKind::Function,
            ),
            (
                Declaration::Package(PackageDecl {
                    common: common_with_name(6),
                    members: vec![],
                    body: None,
                }),
                DeclKind::Package,
            ),
            (
                Declaration::Type(TypeDecl {
                    common: common_with_name(7),
                }),
                DeclKind::Type,
            ),
            (
                Declaration::Table(TableDecl {
                    common: common_with_name(8),
                    columns: vec![],
                }),
                DeclKind::Table,
            ),
            (
                Declaration::View(ViewDecl {
                    common: common_with_name(9),
                    columns: vec![],
                }),
                DeclKind::View,
            ),
            (
                Declaration::Column(ColumnDecl {
                    common: common_with_name(10),
                    ty: None,
                    not_null: true,
                }),
                DeclKind::Column,
            ),
            (
                Declaration::Sequence(SequenceDecl {
                    common: common_with_name(11),
                }),
                DeclKind::Sequence,
            ),
            (
                Declaration::Synonym(SynonymDecl {
                    common: common_with_name(12),
                    target: None,
                    public_synonym: true,
                }),
                DeclKind::Synonym,
            ),
            (
                Declaration::Index(IndexDecl {
                    common: common_with_name(13),
                }),
                DeclKind::Index,
            ),
            (
                Declaration::Trigger(TriggerDecl {
                    common: common_with_name(14),
                }),
                DeclKind::Trigger,
            ),
        ];

        for (decl, expected_kind) in cases {
            assert_eq!(decl.kind(), expected_kind);
            assert_eq!(decl.name(), decl.common().name);
            assert_eq!(decl.span(), decl.common().span);
        }
    }

    #[test]
    fn is_callable_and_schema_object_partitions_match_intent() {
        let proc = Declaration::Procedure(ProcedureDecl {
            common: common_with_name(1),
            params: vec![],
        });
        let func = Declaration::Function(FunctionDecl {
            common: common_with_name(2),
            params: vec![],
            return_type: None,
        });
        let var = Declaration::Variable(VariableDecl {
            common: common_with_name(3),
            ty: None,
            default_text: None,
            constant: false,
            not_null: false,
        });
        let pkg = Declaration::Package(PackageDecl {
            common: common_with_name(4),
            members: vec![],
            body: None,
        });

        assert!(proc.is_callable());
        assert!(func.is_callable());
        assert!(!var.is_callable());
        assert!(!pkg.is_callable());

        assert!(pkg.is_schema_object());
        assert!(!proc.is_schema_object());
        assert!(!var.is_schema_object());
    }

    #[test]
    fn synonym_resolution_target_is_optional() {
        let mut syn = SynonymDecl {
            common: common_with_name(1),
            target: None,
            public_synonym: false,
        };
        assert!(syn.target.is_none());
        syn.target = Some(DeclId::new(42));
        assert_eq!(syn.target, Some(DeclId::new(42)));
    }
}
