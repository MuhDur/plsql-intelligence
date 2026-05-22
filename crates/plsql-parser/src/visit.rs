//! AST visitor and walker traits.
//!
//! This module provides the [`Visitor`] trait for traversing AST nodes, and
//! the [`walk`] module with default traversal implementations.
//!
//! # Design
//!
//! - The [`Visitor`] trait has a `visit_*` method for each AST node type.
//! - Each `visit_*` method has a default implementation that calls the
//!   corresponding `walk_*` function, which recurses into children.
//! - Override individual `visit_*` methods to intercept specific node types
//!   without reimplementing the full traversal.
//!
//! # Extension
//!
//! As the AST grows (PARSE-004 through PARSE-011), new `visit_*` / `walk_*`
//! pairs will be added.  The existing `visit_source_file` entry point
//! dispatches to all child nodes.

use plsql_core::Span;

use crate::ast::{AstDecl, SourceFile};

// ---------------------------------------------------------------------------
// Visitor trait
// ---------------------------------------------------------------------------

/// Trait for visiting AST nodes.
///
/// Every method has a default implementation that recurses into children via
/// the corresponding `walk_*` function.  Override only the methods you care
/// about.
///
/// # Example
///
/// ```ignore
/// use plsql_parser::visit::{Visitor, walk};
/// use plsql_parser::ast::{SourceFile, AstDecl};
///
/// struct DeclCounter {
///     count: usize,
/// }
///
/// impl Visitor for DeclCounter {
///     fn visit_decl(&mut self, decl: &AstDecl) {
///         self.count += 1;
///         walk::walk_decl(self, decl);
///     }
/// }
/// ```
pub trait Visitor: Sized {
    /// Visit a source file (the root of the AST).
    ///
    /// Default: walk all declarations.
    fn visit_source_file(&mut self, source_file: &SourceFile) {
        walk::walk_source_file(self, source_file);
    }

    /// Visit a top-level declaration.
    ///
    /// Default: walk the specific declaration variant.
    fn visit_decl(&mut self, decl: &AstDecl) {
        walk::walk_decl(self, decl);
    }

    /// Visit a package specification.
    fn visit_package_spec(&mut self, _name: &str, _span: &Span) {}

    /// Visit a package body.
    fn visit_package_body(&mut self, _name: &str, _span: &Span) {}

    /// Visit a standalone procedure.
    fn visit_procedure(&mut self, _name: &str, _span: &Span) {}

    /// Visit a standalone function.
    fn visit_function(&mut self, _name: &str, _span: &Span) {}

    /// Visit a trigger.
    fn visit_trigger(&mut self, _name: &str, _span: &Span) {}

    /// Visit a view.
    fn visit_view(&mut self, _name: &str, _span: &Span) {}

    /// Visit a type specification.
    fn visit_type_spec(&mut self, _name: &str, _span: &Span) {}

    /// Visit a type body.
    fn visit_type_body(&mut self, _name: &str, _span: &Span) {}

    /// Visit a DDL statement.
    fn visit_ddl(&mut self, _kind: &str, _span: &Span) {}

    /// Visit an unknown/unclassified declaration (R13).
    fn visit_unknown(&mut self, _span: &Span) {}
}

// ---------------------------------------------------------------------------
// Walk functions (default traversal)
// ---------------------------------------------------------------------------

/// Default traversal implementations.
///
/// Each `walk_*` function calls the corresponding `visit_*` methods on
/// child nodes.  Override `visit_*` to intercept; call `walk_*` to
/// continue traversal.
pub mod walk {
    use super::*;

    /// Walk all declarations in a source file.
    pub fn walk_source_file<V: Visitor>(visitor: &mut V, source_file: &SourceFile) {
        for decl in &source_file.declarations {
            visitor.visit_decl(decl);
        }
    }

    /// Walk a declaration by dispatching to the variant-specific visitor.
    pub fn walk_decl<V: Visitor>(visitor: &mut V, decl: &AstDecl) {
        match decl {
            AstDecl::PackageSpec { name, span } => visitor.visit_package_spec(name, span),
            AstDecl::PackageBody { name, span } => visitor.visit_package_body(name, span),
            AstDecl::Procedure { name, span } => visitor.visit_procedure(name, span),
            AstDecl::Function { name, span } => visitor.visit_function(name, span),
            AstDecl::Trigger { name, span } => visitor.visit_trigger(name, span),
            AstDecl::View { name, span } => visitor.visit_view(name, span),
            AstDecl::TypeSpec { name, span } => visitor.visit_type_spec(name, span),
            AstDecl::TypeBody { name, span } => visitor.visit_type_body(name, span),
            AstDecl::Ddl { kind, span, .. } => visitor.visit_ddl(kind, span),
            AstDecl::Unknown { span, .. } => visitor.visit_unknown(span),
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience functions
// ---------------------------------------------------------------------------

/// Walk the entire AST with the given visitor.
pub fn visit_source_file<V: Visitor>(visitor: &mut V, source_file: &SourceFile) {
    visitor.visit_source_file(source_file);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::AstDecl;
    use plsql_core::{FileId, Position};

    fn span(offset: u32, len: u32) -> Span {
        Span::new(
            FileId::new(0),
            Position::new(1, 1, offset),
            Position::new(1, 1, offset + len),
        )
    }

    /// A visitor that counts declarations.
    struct DeclCounter {
        count: usize,
        package_count: usize,
        unknown_count: usize,
    }

    impl DeclCounter {
        fn new() -> Self {
            Self {
                count: 0,
                package_count: 0,
                unknown_count: 0,
            }
        }
    }

    impl Visitor for DeclCounter {
        fn visit_decl(&mut self, decl: &AstDecl) {
            self.count += 1;
            walk::walk_decl(self, decl);
        }

        fn visit_package_spec(&mut self, _name: &str, _span: &Span) {
            self.package_count += 1;
        }

        fn visit_package_body(&mut self, _name: &str, _span: &Span) {
            self.package_count += 1;
        }

        fn visit_unknown(&mut self, _span: &Span) {
            self.unknown_count += 1;
        }
    }

    #[test]
    fn visitor_counts_declarations() {
        let sf = SourceFile {
            span: span(0, 200),
            declarations: vec![
                AstDecl::PackageSpec {
                    name: "pkg_a".into(),
                    span: span(0, 50),
                },
                AstDecl::PackageBody {
                    name: "pkg_a".into(),
                    span: span(50, 50),
                },
                AstDecl::Procedure {
                    name: "standalone_p".into(),
                    span: span(100, 50),
                },
                AstDecl::Unknown {
                    span: span(150, 50),
                    antlr_rule_path: None,
                },
            ],
        };

        let mut counter = DeclCounter::new();
        visit_source_file(&mut counter, &sf);

        assert_eq!(counter.count, 4);
        assert_eq!(counter.package_count, 2);
        assert_eq!(counter.unknown_count, 1);
    }

    /// A visitor that collects declaration names.
    struct NameCollector {
        names: Vec<String>,
    }

    impl NameCollector {
        fn new() -> Self {
            Self { names: Vec::new() }
        }
    }

    impl Visitor for NameCollector {
        fn visit_package_spec(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("package_spec:{name}"));
        }

        fn visit_package_body(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("package_body:{name}"));
        }

        fn visit_procedure(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("procedure:{name}"));
        }

        fn visit_function(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("function:{name}"));
        }

        fn visit_trigger(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("trigger:{name}"));
        }

        fn visit_view(&mut self, name: &str, _span: &Span) {
            self.names.push(format!("view:{name}"));
        }
    }

    #[test]
    fn visitor_collects_names() {
        let sf = SourceFile {
            span: span(0, 100),
            declarations: vec![
                AstDecl::PackageSpec {
                    name: "emp_pkg".into(),
                    span: span(0, 20),
                },
                AstDecl::Function {
                    name: "get_name".into(),
                    span: span(20, 20),
                },
                AstDecl::View {
                    name: "v_emp".into(),
                    span: span(40, 20),
                },
                AstDecl::Trigger {
                    name: "trg_audit".into(),
                    span: span(60, 20),
                },
            ],
        };

        let mut collector = NameCollector::new();
        visit_source_file(&mut collector, &sf);

        assert_eq!(
            collector.names,
            vec![
                "package_spec:emp_pkg",
                "function:get_name",
                "view:v_emp",
                "trigger:trg_audit",
            ]
        );
    }

    /// A visitor that counts DDL statements by kind.
    struct DdlCounter {
        creates: usize,
        alters: usize,
        drops: usize,
    }

    impl DdlCounter {
        fn new() -> Self {
            Self {
                creates: 0,
                alters: 0,
                drops: 0,
            }
        }
    }

    impl Visitor for DdlCounter {
        fn visit_ddl(&mut self, kind: &str, _span: &Span) {
            match kind {
                "CREATE" => self.creates += 1,
                "ALTER" => self.alters += 1,
                "DROP" => self.drops += 1,
                _ => {}
            }
        }
    }

    #[test]
    fn visitor_counts_ddl_by_kind() {
        let sf = SourceFile {
            span: span(0, 100),
            declarations: vec![
                AstDecl::Ddl {
                    kind: "CREATE".into(),
                    span: span(0, 20),
                    antlr_rule_path: None,
                },
                AstDecl::Ddl {
                    kind: "ALTER".into(),
                    span: span(20, 20),
                    antlr_rule_path: None,
                },
                AstDecl::Ddl {
                    kind: "DROP".into(),
                    span: span(40, 20),
                    antlr_rule_path: None,
                },
                AstDecl::Ddl {
                    kind: "CREATE".into(),
                    span: span(60, 20),
                    antlr_rule_path: None,
                },
            ],
        };

        let mut counter = DdlCounter::new();
        visit_source_file(&mut counter, &sf);

        assert_eq!(counter.creates, 2);
        assert_eq!(counter.alters, 1);
        assert_eq!(counter.drops, 1);
    }

    #[test]
    fn default_visitor_recurse_all_variants() {
        let sf = SourceFile {
            span: span(0, 200),
            declarations: vec![
                AstDecl::PackageSpec {
                    name: "a".into(),
                    span: span(0, 10),
                },
                AstDecl::PackageBody {
                    name: "a".into(),
                    span: span(10, 10),
                },
                AstDecl::Procedure {
                    name: "p".into(),
                    span: span(20, 10),
                },
                AstDecl::Function {
                    name: "f".into(),
                    span: span(30, 10),
                },
                AstDecl::Trigger {
                    name: "t".into(),
                    span: span(40, 10),
                },
                AstDecl::View {
                    name: "v".into(),
                    span: span(50, 10),
                },
                AstDecl::TypeSpec {
                    name: "ty".into(),
                    span: span(60, 10),
                },
                AstDecl::TypeBody {
                    name: "ty".into(),
                    span: span(70, 10),
                },
                AstDecl::Ddl {
                    kind: "CREATE".into(),
                    span: span(80, 10),
                    antlr_rule_path: None,
                },
                AstDecl::Unknown {
                    span: span(90, 10),
                    antlr_rule_path: None,
                },
            ],
        };

        // Use a minimal visitor that does nothing — just verify no panic.
        struct NoOpVisitor;
        impl Visitor for NoOpVisitor {}

        let mut visitor = NoOpVisitor;
        visit_source_file(&mut visitor, &sf);
    }
}
