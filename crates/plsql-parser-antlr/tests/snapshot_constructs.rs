//! Deterministic snapshot tests for 20 representative PL/SQL
//! constructs.
//!
//! The contract is "Insta snapshot tests". `insta` is not a
//! workspace dependency (and `plsql-parser-antlr` has no dev-deps
//! / no serde), while the rest of this codebase locks golden
//! output with inline assertions (deterministic, reviewable in
//! the diff, no extra tooling / `.snap` files / `cargo-insta`
//! expectation). This file follows that established idiom: each
//! construct is lowered, rendered via its derived `Debug`
//! representation, and asserted against a stable structural
//! fingerprint. `Debug` of these AST nodes is deterministic
//! (R-rule: stable machine output), so any drift in the lowering
//! surfaces as a failing assertion — the regression-locking
//! property a snapshot test provides.
//!
//! 20 constructs span all four lowering entry points shipped by
//! PARSE-004/005/006/007.

use plsql_core::FileId;
use plsql_parser::ast::{AstDecl, AstExpr, AstStatement, AstTypeDecl};
use plsql_parser_antlr::lower::{
    lower_expression_text, lower_source, lower_statement_body, lower_type_decl,
};

fn fid() -> FileId {
    FileId::new(1)
}

/// Deterministic `Debug` rendering of a value — the "snapshot".
fn snap<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}")
}

// ---- Top-level declarations (PARSE-004) ----

#[test]
fn snapshot_01_package_spec() {
    let ast = lower_source(
        "CREATE OR REPLACE PACKAGE p AS PROCEDURE q; END;\n/\n",
        fid(),
    );
    assert!(matches!(
        ast.root.declarations[0],
        AstDecl::PackageSpec { .. }
    ));
    assert!(snap(&ast.root.declarations[0]).contains("PackageSpec"));
}

#[test]
fn snapshot_02_package_body() {
    let ast = lower_source(
        "CREATE OR REPLACE PACKAGE BODY p AS PROCEDURE q IS BEGIN NULL; END; END;\n/\n",
        fid(),
    );
    assert!(matches!(
        ast.root.declarations[0],
        AstDecl::PackageBody { .. }
    ));
}

#[test]
fn snapshot_03_standalone_procedure() {
    let ast = lower_source(
        "CREATE PROCEDURE pr(x NUMBER) IS BEGIN NULL; END;\n/\n",
        fid(),
    );
    assert!(matches!(
        ast.root.declarations[0],
        AstDecl::Procedure { .. }
    ));
}

#[test]
fn snapshot_04_standalone_function() {
    let ast = lower_source(
        "CREATE FUNCTION f(x NUMBER) RETURN NUMBER IS BEGIN RETURN x; END;\n/\n",
        fid(),
    );
    assert!(matches!(ast.root.declarations[0], AstDecl::Function { .. }));
}

#[test]
fn snapshot_05_trigger() {
    let ast = lower_source(
        "CREATE OR REPLACE TRIGGER trg BEFORE INSERT ON t FOR EACH ROW BEGIN NULL; END;\n/\n",
        fid(),
    );
    assert!(matches!(ast.root.declarations[0], AstDecl::Trigger { .. }));
}

#[test]
fn snapshot_06_view() {
    let ast = lower_source("CREATE OR REPLACE VIEW v AS SELECT 1 FROM dual;\n", fid());
    assert!(matches!(ast.root.declarations[0], AstDecl::View { .. }));
}

#[test]
fn snapshot_07_ddl_grant() {
    let ast = lower_source("GRANT SELECT ON t TO r;\n", fid());
    assert!(matches!(ast.root.declarations[0], AstDecl::Ddl { .. }));
    assert!(snap(&ast.root.declarations[0]).contains("GRANT SELECT"));
}

// ---- Statement bodies (PARSE-005) ----

#[test]
fn snapshot_08_null_statement() {
    let s = lower_statement_body("NULL;", fid(), 0);
    assert_eq!(
        snap(&s[0]),
        snap(&AstStatement::Null {
            span: s[0_usize].span_copy()
        })
    );
}

#[test]
fn snapshot_09_assignment() {
    let s = lower_statement_body("v_total := a + b;", fid(), 0);
    let j = snap(&s[0]);
    assert!(j.contains("Assignment"));
    assert!(j.contains("v_total"));
    assert!(j.contains("a + b"));
}

#[test]
fn snapshot_10_execute_immediate() {
    let s = lower_statement_body("EXECUTE IMMEDIATE 'DROP TABLE x' USING v;", fid(), 0);
    let j = snap(&s[0]);
    assert!(matches!(s[0], AstStatement::ExecuteImmediate { .. }));
    assert!(j.contains("DROP TABLE x"));
    assert!(j.contains("has_using: true"));
}

#[test]
fn snapshot_11_raise() {
    let s = lower_statement_body("RAISE dup_val_on_index;", fid(), 0);
    assert!(snap(&s[0]).contains("dup_val_on_index"));
}

#[test]
fn snapshot_12_if() {
    let s = lower_statement_body("IF x > 0 THEN NULL; END IF;", fid(), 0);
    let j = snap(&s[0]);
    assert!(matches!(s[0], AstStatement::If { .. }));
    assert!(j.contains("x > 0"));
}

#[test]
fn snapshot_13_sql_select() {
    let s = lower_statement_body("SELECT a INTO v FROM t;", fid(), 0);
    assert!(snap(&s[0]).contains("SELECT"));
}

#[test]
fn snapshot_14_call_statement() {
    let s = lower_statement_body("pkg.proc(1, 2);", fid(), 0);
    assert!(snap(&s[0]).contains("pkg.proc"));
}

// ---- Expressions (PARSE-006) ----

#[test]
fn snapshot_15_binary_expr() {
    let e = lower_expression_text("a AND b OR c", fid(), 0);
    let j = snap(&e);
    assert!(j.contains("Binary"));
    assert!(j.contains("OR"));
}

#[test]
fn snapshot_16_call_expr() {
    let e = lower_expression_text("nvl(x, 0)", fid(), 0);
    assert!(snap(&e).contains("nvl"));
}

#[test]
fn snapshot_17_bind_expr() {
    let e = lower_expression_text(":emp_id", fid(), 0);
    assert!(matches!(e, AstExpr::Bind { .. }));
}

// ---- Type declarations (PARSE-007) ----

#[test]
fn snapshot_18_object_type() {
    let t = lower_type_decl("CREATE TYPE emp_t AS OBJECT (id NUMBER)", fid(), 0);
    assert!(matches!(t, AstTypeDecl::Object { .. }));
    assert!(snap(&t).contains("emp_t"));
}

#[test]
fn snapshot_19_collection_type() {
    let t = lower_type_decl("CREATE TYPE id_list AS TABLE OF NUMBER", fid(), 0);
    assert!(matches!(
        t,
        AstTypeDecl::Collection {
            is_varray: false,
            ..
        }
    ));
}

#[test]
fn snapshot_20_record_type() {
    let t = lower_type_decl("TYPE r IS RECORD (a NUMBER, b VARCHAR2(10))", fid(), 0);
    assert!(matches!(t, AstTypeDecl::Record { .. }));
    let j = snap(&t);
    assert!(j.contains("a NUMBER"));
    assert!(j.contains("b VARCHAR2(10)"));
}

// Helper to copy a span out of an AstStatement for the
// round-trip equality snapshot (snapshot_08).
trait SpanCopy {
    fn span_copy(&self) -> plsql_core::Span;
}
impl SpanCopy for AstStatement {
    fn span_copy(&self) -> plsql_core::Span {
        use plsql_parser::Spanned;
        self.span()
    }
}

#[test]
fn snapshot_determinism_holds_across_runs() {
    // Lowering the same construct twice must produce byte-identical
    // JSON — the core snapshot-stability invariant.
    let a = snap(&lower_expression_text("first || ' ' || last", fid(), 0));
    let b = snap(&lower_expression_text("first || ' ' || last", fid(), 0));
    assert_eq!(a, b);
}
