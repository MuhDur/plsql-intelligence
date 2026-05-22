//! Concrete syntax tree and abstract syntax tree types.
//!
//! These types define the public AST / CST surface for the parser frontend.
//! They will be expanded with full node hierarchies by downstream beads
//! (`PLSQL-PARSE-004` through `PLSQL-PARSE-011`), but the structural
//! definitions — [`ConcreteSyntaxTree`], [`Ast`], [`TokenTape`], [`TriviaTable`]
//! — are settled here.
//!
//! # Lossless vs lossy
//!
//! - [`ConcreteSyntaxTree`] is **lossless**: every delimiter, keyword, and
//!   trivia is represented with byte-offset spans.  Round-tripping goes
//!   through the CST / token tape.
//!
//! - [`Ast`] is **lossy** (semantic): whitespace, comments, and exact
//!   delimiter positions are not preserved.  Pretty-printing from the AST
//!   produces *equivalent* but not *byte-identical* output.
//!
//! # Spanned invariant (PLSQL-PARSE-010)
//!
//! Every AST node **MUST** carry a source [`Span`].  The [`Spanned`] trait
//! formalises this requirement.  All new AST node types added by downstream
//! beads must implement [`Spanned`].  This is enforced by code review, not
//! by a compile-time lint (Rust's type system cannot express "every variant
//! of an enum has a `span` field").

use std::collections::BTreeMap;

use plsql_core::Span;
use serde::{Deserialize, Serialize};

use crate::tokens::{TokenTape, TriviaTable};

// ---------------------------------------------------------------------------
// Spanned trait (PLSQL-PARSE-010)
// ---------------------------------------------------------------------------

/// Every AST node must implement this trait.
///
/// The trait returns the node's source [`Span`] — the byte-offset range in the
/// original source file that this node corresponds to.  This is a hard
/// requirement for provenance tracking (R12) and diagnostic quality (every
/// diagnostic has a non-empty `Span` pointing to the offending source range,
/// plan §7.6).
///
/// # Contract
///
/// - `span()` MUST return the tightest bounding span that covers all tokens
///   belonging to this node.
/// - For nodes that span multiple non-contiguous ranges (e.g., a package spec
///   with a separate body), `span()` returns the *primary* span (the spec
///   keyword range).  Related spans are carried via `SpanLabel` in the
///   `Evidence` or `Diagnostic` types.
pub trait Spanned {
    /// The source span of this AST node.
    fn span(&self) -> Span;
}

// ---------------------------------------------------------------------------
// CST node identifiers
// ---------------------------------------------------------------------------

/// Opaque identifier for a node in the [`ConcreteSyntaxTree`].
///
/// These are backend-local indices; they are NOT stable across parse
/// invocations or backends.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct CstNodeId(pub u32);

// ---------------------------------------------------------------------------
// SourceMap
// ---------------------------------------------------------------------------

/// Maps [`CstNodeId`]s to their source [`Span`]s.
///
/// This is a side-table rather than embedding spans in every CST node, so
/// the node arena stays compact and span lookups are O(log n).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceMap {
    inner: BTreeMap<u32, Span>,
}

impl SourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the span for a given CST node.
    pub fn insert(&mut self, node: CstNodeId, span: Span) {
        self.inner.insert(node.0, span);
    }

    /// Look up the span for a given CST node.
    #[must_use]
    pub fn get(&self, node: CstNodeId) -> Option<&Span> {
        self.inner.get(&node.0)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ConcreteSyntaxTree
// ---------------------------------------------------------------------------

/// The lossless concrete syntax tree produced by a [`ParseBackend`].
///
/// The CST preserves every token and trivia element with source spans.
/// Combined with the [`TokenTape`] and [`TriviaTable`], it supports
/// byte-for-byte source reconstruction.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConcreteSyntaxTree {
    /// The root node of the CST.
    pub root: CstNodeId,
    /// The lossless token tape.
    pub token_tape: TokenTape,
    /// Trivia (whitespace, comments) associated with tokens.
    pub trivia: TriviaTable,
    /// Maps CST node IDs to source spans.
    pub source_map: SourceMap,
}

impl ConcreteSyntaxTree {
    /// Create a new empty CST.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstruct the original source text from the CST.
    ///
    /// This is the lossless round-trip operation.
    #[must_use]
    pub fn reconstruct(&self) -> String {
        self.token_tape.reconstruct(&self.trivia)
    }
}

// ---------------------------------------------------------------------------
// SourceFile / Ast
// ---------------------------------------------------------------------------

/// A single parsed source file (the root of the typed AST).
///
/// The `declarations` vector holds top-level PL/SQL declarations (packages,
/// procedures, functions, triggers, views, types, DDL statements) discovered
/// in the file.  Each carries a name and source span.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceFile {
    /// Byte span covering the entire file.
    pub span: Span,
    /// Top-level declarations discovered in the file.
    pub declarations: Vec<AstDecl>,
}

impl Spanned for SourceFile {
    fn span(&self) -> Span {
        self.span
    }
}

/// A top-level PL/SQL declaration.
///
/// Variants cover the full set of top-level constructs the parser must
/// recognize (plan §7.2).  The `Unknown` variant satisfies R13 — no
/// uncertainty is silently dropped.
///
/// **Every variant MUST carry a `span` field** (Spanned invariant,
/// PLSQL-PARSE-010).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AstDecl {
    /// A PL/SQL package specification.
    PackageSpec { name: String, span: Span },
    /// A PL/SQL package body.
    PackageBody { name: String, span: Span },
    /// A standalone procedure.
    Procedure { name: String, span: Span },
    /// A standalone function.
    Function { name: String, span: Span },
    /// A trigger.
    Trigger { name: String, span: Span },
    /// A view.
    View { name: String, span: Span },
    /// A type specification.
    TypeSpec { name: String, span: Span },
    /// A type body.
    TypeBody { name: String, span: Span },
    /// A DDL statement (CREATE / ALTER / DROP / GRANT).
    ///
    /// `antlr_rule_path` is a bounded, `>`-joined path of ANTLR
    /// *grammar rule names* (never source text or identifiers — see
    /// [`crate::ast`] / `PLSQL-USR-001 §2.1`) identifying the
    /// grammar position the DDL was recognised at. `None` when the
    /// declaration did not originate from a real ANTLR parse tree
    /// (e.g. the text-scanner fallback). It is a plain `String`, so
    /// no ANTLR generated type crosses the crate boundary (R20).
    Ddl {
        kind: String,
        span: Span,
        #[serde(default)]
        antlr_rule_path: Option<String>,
    },
    /// A declaration the backend could not classify (R13).
    ///
    /// `antlr_rule_path` — see [`AstDecl::Ddl`].
    Unknown {
        span: Span,
        #[serde(default)]
        antlr_rule_path: Option<String>,
    },
}

impl Spanned for AstDecl {
    fn span(&self) -> Span {
        match self {
            Self::PackageSpec { span, .. }
            | Self::PackageBody { span, .. }
            | Self::Procedure { span, .. }
            | Self::Function { span, .. }
            | Self::Trigger { span, .. }
            | Self::View { span, .. }
            | Self::TypeSpec { span, .. }
            | Self::TypeBody { span, .. }
            | Self::Ddl { span, .. }
            | Self::Unknown { span, .. } => *span,
        }
    }
}

/// A statement inside a routine / anonymous block body
/// (`PLSQL-PARSE-005`).
///
/// This is the **syntactic** projection of a statement body — one
/// step before `plsql_ir::Statement` (the semantic IR). The
/// parser frontend only recognises the shape; name resolution +
/// flow happen in Layer 2. `Unknown` satisfies R13.
///
/// Every variant carries a `span` (Spanned invariant,
/// PLSQL-PARSE-010).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstStatement {
    /// `NULL;`
    Null { span: Span },
    /// `target := <rhs>;` — RHS kept as raw text for the IR
    /// lowering (`PLSQL-IR-004`) to re-parse.
    Assignment {
        target: String,
        rhs_text: String,
        span: Span,
    },
    /// `IF … THEN … [ELSIF …] [ELSE …] END IF;` — the body
    /// slices are raw text the IR layer recurses into.
    If { cond_text: String, span: Span },
    /// Any loop form (`LOOP` / `FOR … LOOP` / `WHILE … LOOP`).
    Loop { header_text: String, span: Span },
    /// `RAISE [exception];`
    Raise {
        exception: Option<String>,
        span: Span,
    },
    /// `RETURN [expr];`
    Return {
        value_text: Option<String>,
        span: Span,
    },
    /// `EXECUTE IMMEDIATE '<sql>' [USING …];`
    ExecuteImmediate {
        sql_text: String,
        has_using: bool,
        span: Span,
    },
    /// An embedded SQL DML statement (`SELECT`/`INSERT`/`UPDATE`/
    /// `DELETE`/`MERGE`). `raw_text` is the verbatim statement source
    /// slice so the IR layer can recover table/column read/write
    /// dependencies (`PLSQL-DEP-003`). Empty when the backend could
    /// only classify the verb.
    Sql {
        verb: String,
        raw_text: String,
        span: Span,
    },
    /// A procedure / function call statement.
    Call { callee: String, span: Span },
    /// A statement the backend could not classify (R13).
    Unknown { span: Span },
}

impl Spanned for AstStatement {
    fn span(&self) -> Span {
        match self {
            Self::Null { span }
            | Self::Assignment { span, .. }
            | Self::If { span, .. }
            | Self::Loop { span, .. }
            | Self::Raise { span, .. }
            | Self::Return { span, .. }
            | Self::ExecuteImmediate { span, .. }
            | Self::Sql { span, .. }
            | Self::Call { span, .. }
            | Self::Unknown { span } => *span,
        }
    }
}

/// A PL/SQL expression node (`PLSQL-PARSE-006`).
///
/// The **syntactic** expression projection — binary ops,
/// function / procedure calls, cursor + attribute references,
/// literals, bind / substitution placeholders. One step before
/// `plsql_ir::Expr` (the semantic IR). `Unknown` satisfies R13.
///
/// Every variant carries a `span` (Spanned invariant
/// PLSQL-PARSE-010).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstExpr {
    /// A literal (number / string / `NULL` / `TRUE` / `FALSE`),
    /// kept verbatim so the IR can re-classify precisely.
    Literal { text: String, span: Span },
    /// A dotted name reference (`a`, `pkg.fn`, `t.col%TYPE`,
    /// `c%ROWTYPE`, `:new.id`).
    Name { path: String, span: Span },
    /// Bind placeholder (`:1`, `:name`).
    Bind { name: String, span: Span },
    /// Substitution variable (`&v`, `&&v`).
    Substitution {
        name: String,
        sticky: bool,
        span: Span,
    },
    /// A call `callee(<args-text>)` — args kept as raw text for
    /// the IR layer to split + recurse.
    Call {
        callee: String,
        args_text: String,
        span: Span,
    },
    /// Binary op at the top level. Operand slices are raw text.
    Binary {
        op: String,
        lhs_text: String,
        rhs_text: String,
        span: Span,
    },
    /// Unary op (`NOT` / `-` / `+`).
    Unary {
        op: String,
        operand_text: String,
        span: Span,
    },
    /// An expression the backend could not classify (R13).
    Unknown { text: String, span: Span },
}

impl Spanned for AstExpr {
    fn span(&self) -> Span {
        match self {
            Self::Literal { span, .. }
            | Self::Name { span, .. }
            | Self::Bind { span, .. }
            | Self::Substitution { span, .. }
            | Self::Call { span, .. }
            | Self::Binary { span, .. }
            | Self::Unary { span, .. }
            | Self::Unknown { span, .. } => *span,
        }
    }
}

/// A type declaration (`PLSQL-PARSE-007`).
///
/// The **syntactic** projection of `CREATE TYPE … AS OBJECT`,
/// `TABLE OF` / `VARRAY` collection types, and PL/SQL
/// `TYPE … IS RECORD` declarations. Attribute / element text is
/// kept raw for the bindgen layer (PLSQL-BG-003) to resolve.
/// `Unknown` satisfies R13; every variant Spanned
/// (PLSQL-PARSE-010).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstTypeDecl {
    /// `CREATE [OR REPLACE] TYPE <name> AS OBJECT ( … )`.
    Object {
        name: String,
        attributes_text: String,
        span: Span,
    },
    /// `… AS TABLE OF <elem>` (nested table) or
    /// `… AS VARRAY(n) OF <elem>`.
    Collection {
        name: String,
        element_text: String,
        is_varray: bool,
        span: Span,
    },
    /// `TYPE <name> IS RECORD ( … )` (PL/SQL record).
    Record {
        name: String,
        fields_text: String,
        span: Span,
    },
    /// A type declaration the backend could not classify (R13).
    Unknown { text: String, span: Span },
}

impl Spanned for AstTypeDecl {
    fn span(&self) -> Span {
        match self {
            Self::Object { span, .. }
            | Self::Collection { span, .. }
            | Self::Record { span, .. }
            | Self::Unknown { span, .. } => *span,
        }
    }
}

/// The typed abstract syntax tree.
///
/// This is a **semantic** projection — it is NOT required to preserve
/// whitespace, comments, or exact delimiter positions.  Pretty-printing
/// from the AST produces *equivalent* but not *byte-identical* output.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ast {
    /// The root source-file node.
    pub root: SourceFile,
    /// Source map for AST nodes (maps node IDs to spans).
    pub source_map: SourceMap,
    /// Body statements for each top-level declaration, in parallel
    /// with `root.declarations`.  `body_statements[i]` is the lowered
    /// body of `root.declarations[i]`.  Declarations with no body
    /// (e.g., package specs, views, DDL) carry an empty inner vec.
    /// Defaulting to empty so existing callers that produce an `Ast`
    /// without body lowering remain valid (backward-compatible; R13).
    #[serde(default)]
    pub body_statements: Vec<Vec<AstStatement>>,
}

impl Ast {
    /// Create a new empty AST.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::{FileId, Position};

    fn span(offset: u32, len: u32) -> Span {
        Span::new(
            FileId::new(0),
            Position::new(1, 1, offset),
            Position::new(1, 1, offset + len),
        )
    }

    #[test]
    fn source_map_insert_and_get() {
        let mut sm = SourceMap::new();
        let id = CstNodeId(42);
        let s = span(10, 5);
        sm.insert(id, s);
        assert_eq!(sm.get(id), Some(&s));
        assert_eq!(sm.get(CstNodeId(99)), None);
    }

    #[test]
    fn source_map_len() {
        let mut sm = SourceMap::new();
        assert!(sm.is_empty());
        sm.insert(CstNodeId(0), span(0, 1));
        sm.insert(CstNodeId(1), span(1, 1));
        assert_eq!(sm.len(), 2);
        assert!(!sm.is_empty());
    }

    #[test]
    fn cst_default_has_empty_source_map() {
        let cst = ConcreteSyntaxTree::new();
        assert!(cst.source_map.is_empty());
    }

    #[test]
    fn ast_default_has_empty_source_map() {
        let ast = Ast::new();
        assert!(ast.source_map.is_empty());
    }

    #[test]
    fn source_map_serializes_round_trip() {
        let mut sm = SourceMap::new();
        sm.insert(CstNodeId(1), span(0, 10));
        sm.insert(CstNodeId(5), span(20, 30));
        let json = serde_json::to_string(&sm).unwrap();
        let back: SourceMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.get(CstNodeId(1)), Some(&span(0, 10)));
    }

    // -----------------------------------------------------------------------
    // Spanned trait tests (PLSQL-PARSE-010)
    // -----------------------------------------------------------------------

    #[test]
    fn source_file_is_spanned() {
        let s = span(0, 100);
        let sf = SourceFile {
            span: s,
            declarations: Vec::new(),
        };
        assert_eq!(sf.span(), s);
    }

    #[test]
    fn ast_decl_all_variants_are_spanned() {
        let s = span(10, 20);
        let decls = vec![
            AstDecl::PackageSpec {
                name: "pkg".into(),
                span: s,
            },
            AstDecl::PackageBody {
                name: "pkg".into(),
                span: s,
            },
            AstDecl::Procedure {
                name: "p".into(),
                span: s,
            },
            AstDecl::Function {
                name: "f".into(),
                span: s,
            },
            AstDecl::Trigger {
                name: "t".into(),
                span: s,
            },
            AstDecl::View {
                name: "v".into(),
                span: s,
            },
            AstDecl::TypeSpec {
                name: "ty".into(),
                span: s,
            },
            AstDecl::TypeBody {
                name: "ty".into(),
                span: s,
            },
            AstDecl::Ddl {
                kind: "CREATE".into(),
                span: s,
                antlr_rule_path: None,
            },
            AstDecl::Unknown {
                span: s,
                antlr_rule_path: None,
            },
        ];

        for decl in &decls {
            assert_eq!(
                decl.span(),
                s,
                "Spanned::span() returned wrong span for variant"
            );
        }
    }

    #[test]
    fn spanned_trait_is_object_safe() {
        // Verify that Spanned can be used as a trait object
        fn take_spanned(node: &dyn Spanned) -> Span {
            node.span()
        }
        let s = span(0, 50);
        let sf = SourceFile {
            span: s,
            declarations: Vec::new(),
        };
        assert_eq!(take_spanned(&sf), s);
    }
}
