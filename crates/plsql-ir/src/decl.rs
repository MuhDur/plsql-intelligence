//! Declaration variants populated by AST→IR lowering.
//!
//! This module introduces the [`Declaration`] enum with one variant per
//! kind of named entity the engine reasons about. Each variant carries a
//! shared [`DeclCommon`] payload (name, span, owning schema, optional
//! parent declaration) plus a small number of variant-specific fields.
//! The actual lowering from parser AST to these declarations, together
//! with the registration pass (`DeclTable` + scope chain), lives in the
//! sibling lowering modules.
//!
//! Placeholder [`TypeRef`] payloads are narrowed into structured
//! representations once type resolution cross-checks against the catalog.

use plsql_core::{SchemaName, Span, SymbolId};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::DeclId;
use crate::expr::Expr;
use crate::stmt::Statement;

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
/// later passes resolve `%TYPE` / `%ROWTYPE` anchors against the catalog
/// and narrow `Unresolved` into a structured representation. Keeping this
/// an enum from day one means downstream crates do not have to be
/// re-shaped when richer resolution lands.
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
    /// The part's separately addressable units (members, parameter
    /// defaults, initializers, init section, cursors).
    pub units: PackageUnits,
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
    /// Object the synonym resolves to once runs.
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
/// New variants are additive; reshape decisions are deferred to
/// top-level lowering and the symbol resolution layer.
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

// ---------------------------------------------------------------------------
// Package units (InvocationClosureV1 identity)
// ---------------------------------------------------------------------------

/// Which part of a package a [`PackageDecl`] lowers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PackagePart {
    #[default]
    Spec,
    Body,
}

impl PackagePart {
    fn tag(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Body => "body",
        }
    }
}

/// Procedure or function.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum RoutineKind {
    Procedure,
    Function,
}

/// Identity of a package subprogram among same-named siblings, joinable to
/// Oracle's `ALL_ARGUMENTS.OVERLOAD`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum OverloadIdentity {
    /// The name occurs once (`OVERLOAD` is `NULL`).
    NotOverloaded,
    /// The 1-based `OVERLOAD` ordinal, established from the specification.
    Ordinal(u32),
    /// Overloaded, but the ordinal could not be established from source.
    /// Downstream this identity is `Unknown`; `position` only keeps the
    /// unit id unique within its part.
    Ambiguous { position: u32 },
}

/// One formal parameter with its default expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineParam {
    /// Exact identity (quoted names keep their case).
    pub name: String,
    pub mode: ParamMode,
    pub ty: Option<TypeRef>,
    /// The default expression, evaluated only when a call omits the argument.
    pub default: Option<Expr>,
    pub default_text: Option<String>,
    pub span: Span,
}

/// One package subprogram in one part, with everything it executes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMember {
    /// Exact identity (quoted names keep their case).
    pub name: String,
    pub kind: RoutineKind,
    pub overload: OverloadIdentity,
    pub params: Vec<RoutineParam>,
    /// Local initializers, local cursor queries, nested subprograms,
    /// statements and exception handlers, in source order. Empty in a spec.
    pub body: Vec<Statement>,
    pub span: Span,
}

/// A package-level declaration initializer (runs at instantiation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInitializer {
    pub name: String,
    pub initializer: Expr,
    pub initializer_text: String,
    pub span: Span,
}

/// A package-level cursor; its query runs whenever a unit opens it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageCursor {
    pub name: String,
    /// The query as a statement (empty for a cursor spec without a query).
    pub query: Vec<Statement>,
    pub span: Span,
}

/// The package body's initialization section, with its exception handlers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInitSection {
    pub body: Vec<Statement>,
    pub span: Span,
}

/// A package-level construct that could not be attributed to any unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnattributedConstruct {
    pub span: Span,
    /// `conditional_compilation`, `call_spec`, `unrecognized` or
    /// `not_lowered`.
    pub reason: String,
}

/// The separately addressable units of one package part.
///
/// The `Default` is `lowered == false`: nothing attributed, so consumers
/// must treat the package as `Unknown` — never as "no members".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageUnits {
    pub part: PackagePart,
    pub lowered: bool,
    pub members: Vec<PackageMember>,
    pub state_variables: Vec<String>,
    pub initializers: Vec<PackageInitializer>,
    pub cursors: Vec<PackageCursor>,
    pub init_section: Option<PackageInitSection>,
    pub unattributed: Vec<UnattributedConstruct>,
}

/// What an addressable unit is.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PackageUnitKind {
    /// A subprogram (its body, in a package body; no statements in a spec).
    Member { kind: RoutineKind },
    /// One parameter's default expression, in this part.
    ParamDefault { member_id: String, param: String },
    /// All declaration initializers of this part, in source order.
    Initializers,
    /// The body's initialization section.
    InitSection,
    /// One package-level cursor query.
    Cursor { name: String },
}

/// One addressable unit: a stable id, what it is, and what it executes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageUnit {
    pub id: String,
    pub kind: PackageUnitKind,
    pub statements: Vec<Statement>,
    pub span: Span,
}

impl PackageMember {
    /// `<package>.<member>` with the overload suffix: none when not
    /// overloaded, `#<ordinal>`, or `#?<position>` when ambiguous. Spec and
    /// body headers of one subprogram share this id.
    #[must_use]
    pub fn unit_id(&self, package: &str) -> String {
        match self.overload {
            OverloadIdentity::NotOverloaded => format!("{package}.{}", self.name),
            OverloadIdentity::Ordinal(n) => format!("{package}.{}#{n}", self.name),
            OverloadIdentity::Ambiguous { position } => {
                format!("{package}.{}#?{position}", self.name)
            }
        }
    }
}

impl PackageUnits {
    /// Every construct was attributed and every overload identity is
    /// exact. `false` means an invocation closure over this package is
    /// `Unknown`.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.lowered
            && self.unattributed.is_empty()
            && self
                .members
                .iter()
                .all(|m| !matches!(m.overload, OverloadIdentity::Ambiguous { .. }))
    }

    /// Every unit of this part with its stable id and executable
    /// statements: members; one unit per parameter default
    /// (`<member id>(<param>)@<part>#default`, so a consumer includes only
    /// the defaults a call shape evaluates); the declaration initializers
    /// (`<package>#<part>_initializers`); the init section
    /// (`<package>#init`); and each cursor (`<package>#cursor:<name>`).
    #[must_use]
    pub fn addressable_units(&self, package: &str) -> Vec<PackageUnit> {
        let mut out = Vec::new();
        let part = self.part.tag();
        for m in &self.members {
            let member_id = m.unit_id(package);
            out.push(PackageUnit {
                id: member_id.clone(),
                kind: PackageUnitKind::Member { kind: m.kind },
                statements: m.body.clone(),
                span: m.span,
            });
            for p in &m.params {
                if let Some(text) = &p.default_text {
                    out.push(PackageUnit {
                        id: format!("{member_id}({})@{part}#default", p.name),
                        kind: PackageUnitKind::ParamDefault {
                            member_id: member_id.clone(),
                            param: p.name.clone(),
                        },
                        statements: vec![Statement::Assignment {
                            target: p.name.clone(),
                            rhs_text: text.clone(),
                        }],
                        span: p.span,
                    });
                }
            }
        }
        if let Some(first) = self.initializers.first() {
            out.push(PackageUnit {
                id: format!("{package}#{part}_initializers"),
                kind: PackageUnitKind::Initializers,
                statements: self
                    .initializers
                    .iter()
                    .map(|i| Statement::Assignment {
                        target: i.name.clone(),
                        rhs_text: i.initializer_text.clone(),
                    })
                    .collect(),
                span: first.span,
            });
        }
        if let Some(section) = &self.init_section {
            out.push(PackageUnit {
                id: format!("{package}#init"),
                kind: PackageUnitKind::InitSection,
                statements: section.body.clone(),
                span: section.span,
            });
        }
        for c in &self.cursors {
            out.push(PackageUnit {
                id: format!("{package}#cursor:{}", c.name),
                kind: PackageUnitKind::Cursor {
                    name: c.name.clone(),
                },
                statements: c.query.clone(),
                span: c.span,
            });
        }
        out
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
                    units: PackageUnits::default(),
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
            units: PackageUnits::default(),
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
