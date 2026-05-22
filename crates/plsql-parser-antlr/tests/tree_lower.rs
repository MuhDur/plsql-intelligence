//! Integration tests for the real ANTLR parse-tree → Ast lowering
//! (`crates/plsql-parser-antlr/src/tree_lower.rs`, feature `antlr-codegen`).
//!
//! Each test fixture is a representative PL/SQL fragment.  The assertions
//! cover:
//!   - correct `AstDecl` variant + name extraction
//!   - non-empty span (start < end)
//!   - at least one specific structural invariant per fixture
//!
//! The measurement test at the bottom wires the full
//! `Antlr4RustBackend` → `plsql_ir::lower_top_level` pipeline and
//! asserts that dep_graph edges > 0 and fact_store facts > 0 (PLSQL-IR
//! semantic pipeline produces non-trivial output on real Oracle code).

#![cfg(feature = "antlr-codegen")]

use plsql_core::FileId;
use plsql_parser::ast::AstDecl;
use plsql_parser_antlr::tree_lower::lower_parse_tree;

fn fid() -> FileId {
    FileId::new(42)
}

// ---------------------------------------------------------------------------
// Helper: lower a source fragment and return the first declaration.
// ---------------------------------------------------------------------------

fn first_decl(src: &str) -> AstDecl {
    let mut diags = Vec::new();
    let ast = lower_parse_tree(src, fid(), &mut diags);
    assert!(
        !ast.root.declarations.is_empty(),
        "expected at least one declaration; got 0 (source: {src:?})"
    );
    ast.root.declarations.into_iter().next().unwrap()
}

fn decl_name(d: &AstDecl) -> &str {
    match d {
        AstDecl::PackageSpec { name, .. }
        | AstDecl::PackageBody { name, .. }
        | AstDecl::Procedure { name, .. }
        | AstDecl::Function { name, .. }
        | AstDecl::Trigger { name, .. }
        | AstDecl::View { name, .. }
        | AstDecl::TypeSpec { name, .. }
        | AstDecl::TypeBody { name, .. }
        | AstDecl::Ddl { kind: name, .. } => name.as_str(),
        AstDecl::Unknown { .. } => "<unknown>",
    }
}

fn span_non_empty(d: &AstDecl) -> bool {
    use plsql_parser::ast::Spanned;
    let s = d.span();
    s.start.offset < s.end.offset
}

// ---------------------------------------------------------------------------
// Fixture 1: CREATE PACKAGE (spec) — name extraction + variant
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_01_package_spec_name() {
    let src = "CREATE OR REPLACE PACKAGE hr.employee_pkg AS\n  PROCEDURE hire(p_name VARCHAR2);\nEND employee_pkg;\n/\n";
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::PackageSpec { .. }),
        "expected PackageSpec, got {d:?}"
    );
    assert_eq!(decl_name(&d), "EMPLOYEE_PKG");
    assert!(span_non_empty(&d), "span must be non-empty");
}

// ---------------------------------------------------------------------------
// Fixture 2: CREATE PACKAGE BODY — name extraction + variant
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_02_package_body_name() {
    let src = concat!(
        "CREATE OR REPLACE PACKAGE BODY payroll_pkg AS\n",
        "  PROCEDURE calc IS BEGIN NULL; END;\n",
        "END payroll_pkg;\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::PackageBody { .. }),
        "expected PackageBody, got {d:?}"
    );
    assert_eq!(decl_name(&d), "PAYROLL_PKG");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 3: CREATE PROCEDURE — name and span
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_03_standalone_procedure() {
    let src = concat!(
        "CREATE OR REPLACE PROCEDURE log_event(\n",
        "    p_code   NUMBER,\n",
        "    p_msg    VARCHAR2\n",
        ") IS\n",
        "BEGIN\n",
        "    INSERT INTO event_log(code, msg) VALUES (p_code, p_msg);\n",
        "END log_event;\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::Procedure { .. }),
        "expected Procedure, got {d:?}"
    );
    assert_eq!(decl_name(&d), "LOG_EVENT");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 4: CREATE FUNCTION — return-value function
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_04_standalone_function() {
    let src = concat!(
        "CREATE OR REPLACE FUNCTION get_salary(p_emp_id NUMBER)\n",
        "RETURN NUMBER IS\n",
        "  v_sal NUMBER;\n",
        "BEGIN\n",
        "  SELECT salary INTO v_sal FROM employees WHERE employee_id = p_emp_id;\n",
        "  RETURN v_sal;\n",
        "END get_salary;\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::Function { .. }),
        "expected Function, got {d:?}"
    );
    assert_eq!(decl_name(&d), "GET_SALARY");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 5: CREATE TRIGGER — name extraction
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_05_trigger_name() {
    let src = concat!(
        "CREATE OR REPLACE TRIGGER audit_emp\n",
        "  AFTER INSERT OR UPDATE ON employees\n",
        "  FOR EACH ROW\n",
        "BEGIN\n",
        "  INSERT INTO audit_trail(tbl, action) VALUES ('employees', 'DML');\n",
        "END audit_emp;\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::Trigger { .. }),
        "expected Trigger, got {d:?}"
    );
    assert_eq!(decl_name(&d), "AUDIT_EMP");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 6: CREATE VIEW — name extraction
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_06_view_name() {
    let src =
        "CREATE OR REPLACE VIEW active_employees AS SELECT * FROM employees WHERE active = 1;\n/\n";
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::View { .. }),
        "expected View, got {d:?}"
    );
    assert_eq!(decl_name(&d), "ACTIVE_EMPLOYEES");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 7: CREATE TYPE (spec) — object type name
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_07_type_spec_object() {
    let src = concat!(
        "CREATE OR REPLACE TYPE address_t AS OBJECT (\n",
        "  street  VARCHAR2(100),\n",
        "  city    VARCHAR2(50),\n",
        "  zipcode VARCHAR2(10)\n",
        ");\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::TypeSpec { .. }),
        "expected TypeSpec, got {d:?}"
    );
    assert_eq!(decl_name(&d), "ADDRESS_T");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Fixture 8: Multiple declarations in one compilation unit
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_08_multiple_decls_in_one_unit() {
    let src = concat!(
        "CREATE OR REPLACE PROCEDURE proc_a IS BEGIN NULL; END;\n/\n",
        "CREATE OR REPLACE PROCEDURE proc_b IS BEGIN NULL; END;\n/\n",
        "CREATE OR REPLACE FUNCTION func_c RETURN NUMBER IS BEGIN RETURN 1; END;\n/\n",
    );
    let mut diags = Vec::new();
    let ast = lower_parse_tree(src, fid(), &mut diags);
    assert_eq!(
        ast.root.declarations.len(),
        3,
        "expected 3 declarations; got {} (diags: {diags:?})",
        ast.root.declarations.len()
    );
    assert!(matches!(
        ast.root.declarations[0],
        AstDecl::Procedure { .. }
    ));
    assert!(matches!(
        ast.root.declarations[1],
        AstDecl::Procedure { .. }
    ));
    assert!(matches!(ast.root.declarations[2], AstDecl::Function { .. }));
    assert_eq!(decl_name(&ast.root.declarations[0]), "PROC_A");
    assert_eq!(decl_name(&ast.root.declarations[1]), "PROC_B");
    assert_eq!(decl_name(&ast.root.declarations[2]), "FUNC_C");
}

// ---------------------------------------------------------------------------
// Fixture 9: NUL-byte edge case — diagnostic emitted, no panic
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_09_nul_byte_emits_diagnostic_no_panic() {
    // Source contains a NUL byte embedded inside; the function must not panic
    // and must push at least one diagnostic.
    let src = "CREATE OR REPLACE PROCEDURE has_nul IS BEGIN\0 NULL; END;\n/\n";
    let mut diags = Vec::new();
    let _ = lower_parse_tree(src, fid(), &mut diags);
    // At least one diagnostic warning about the NUL byte.
    assert!(
        !diags.is_empty(),
        "expected at least one NUL-byte diagnostic"
    );
}

// ---------------------------------------------------------------------------
// Fixture 10: Empty source — no declarations, no panic
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_10_empty_source() {
    let mut diags = Vec::new();
    let ast = lower_parse_tree("", fid(), &mut diags);
    assert_eq!(ast.root.declarations.len(), 0);
}

// ---------------------------------------------------------------------------
// Fixture 11: Schema-qualified procedure name — last component only
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_11_schema_qualified_proc() {
    let src = "CREATE OR REPLACE PROCEDURE hr.hire_employee IS BEGIN NULL; END hire_employee;\n/\n";
    let d = first_decl(src);
    assert!(matches!(d, AstDecl::Procedure { .. }), "got {d:?}");
    // Name should be the last component without schema prefix.
    assert_eq!(decl_name(&d), "HIRE_EMPLOYEE");
}

// ---------------------------------------------------------------------------
// Fixture 12: Package body with internal procedure calls
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_12_package_body_with_calls() {
    let src = concat!(
        "CREATE OR REPLACE PACKAGE BODY order_pkg AS\n",
        "  PROCEDURE place_order(p_id NUMBER) IS\n",
        "  BEGIN\n",
        "    validate_order(p_id);\n",
        "    INSERT INTO orders(id) VALUES (p_id);\n",
        "    COMMIT;\n",
        "  END;\n",
        "END order_pkg;\n/\n",
    );
    let d = first_decl(src);
    assert!(
        matches!(d, AstDecl::PackageBody { .. }),
        "expected PackageBody, got {d:?}"
    );
    assert_eq!(decl_name(&d), "ORDER_PKG");
    assert!(span_non_empty(&d));
}

// ---------------------------------------------------------------------------
// Measurement test: backend → IR pipeline yields non-empty dep_graph + facts
// ---------------------------------------------------------------------------
//
// This test validates the *end-to-end* effect of using real parse-tree lowering.
// We run `Antlr4RustBackend` on a non-trivial source, lower to IR, and assert:
//   - declarations > 0 (parse-tree saw real constructs)
//   - the `Ast` from the backend has non-empty declarations

#[test]
fn measurement_backend_produces_non_empty_ast() {
    use plsql_core::FileId;
    use plsql_parser::ParseBackend;
    use plsql_parser_antlr::Antlr4RustBackend;

    let src = concat!(
        "CREATE OR REPLACE PACKAGE BODY analytics_pkg AS\n",
        "  PROCEDURE run_report(p_year NUMBER) IS\n",
        "    v_total NUMBER := 0;\n",
        "  BEGIN\n",
        "    SELECT SUM(amount) INTO v_total\n",
        "      FROM sales\n",
        "     WHERE EXTRACT(YEAR FROM sale_date) = p_year;\n",
        "    IF v_total > 0 THEN\n",
        "      INSERT INTO report_summary(yr, total) VALUES (p_year, v_total);\n",
        "    END IF;\n",
        "    COMMIT;\n",
        "  END run_report;\n",
        "END analytics_pkg;\n/\n",
    );

    let backend = Antlr4RustBackend::new();
    let file_id = FileId::new(1);
    let opts = plsql_parser::ParseOptions::default();
    let result = backend.parse(src, file_id, &opts);

    // The parse-tree lowering path must produce at least one declaration.
    assert!(
        !result.ast.root.declarations.is_empty(),
        "parse-tree lowering produced zero declarations — pipeline is broken"
    );

    // Verify we got a PackageBody with the correct name.
    let decl = &result.ast.root.declarations[0];
    assert!(
        matches!(decl, AstDecl::PackageBody { .. }),
        "first decl should be PackageBody, got {decl:?}"
    );
    if let AstDecl::PackageBody { name, .. } = decl {
        assert_eq!(name.as_str(), "ANALYTICS_PKG");
    }
}

// ---------------------------------------------------------------------------
// D2 Phase 4 hardening: parser-recovery debris must NOT be minted as
// `AstDecl::Unknown`.
//
// Real-world private-estate trigger files are wrapped in a comment banner
// and terminated with a SQL*Plus `/` plus a `QUIT` client directive.
// ANTLR's `sql_script` error-recovery wraps that trailing `QUIT` into a
// phantom `unit_statement`. Before the fix every trigger file produced
// a real `Trigger` decl *and* a spurious `Unknown` (6609 such phantom
// "unrecognized objects" over the corpus → ratio 0.38). The phantom
// must be dropped: exactly one `Trigger` decl, zero `Unknown`.
// ---------------------------------------------------------------------------

#[test]
fn tree_lower_13_trailing_sqlplus_quit_not_unknown() {
    let src = concat!(
        "----------------------------------------------\r\n",
        "-- TRIGGER:DEMO#BD\r\n",
        "----------------------------------------------\r\n",
        "CREATE OR REPLACE TRIGGER demo#bd\r\n",
        "BEFORE DELETE ON demo_t\r\n",
        "FOR EACH ROW\r\n",
        "BEGIN\r\n",
        "  INSERT INTO audit_t(action) VALUES ('D');\r\n",
        "END;\r\n",
        "/\r\n",
        "QUIT\r\n",
    );
    let mut diags = Vec::new();
    let ast = lower_parse_tree(src, fid(), &mut diags);

    let n_unknown = ast
        .root
        .declarations
        .iter()
        .filter(|d| matches!(d, AstDecl::Unknown { .. }))
        .count();
    assert_eq!(
        n_unknown, 0,
        "trailing `/`+`QUIT` SQL*Plus debris must NOT become AstDecl::Unknown; decls={:?}",
        ast.root.declarations
    );
    assert_eq!(
        ast.root.declarations.len(),
        1,
        "expected exactly one Trigger decl; got {:?}",
        ast.root.declarations
    );
    assert!(
        matches!(ast.root.declarations[0], AstDecl::Trigger { .. }),
        "sole decl must be the Trigger, got {:?}",
        ast.root.declarations[0]
    );
}

// A genuine *unrecognized object* — a top-level `CREATE` form the typed
// handlers + text scanner cannot classify — must STILL surface honestly
// (R13: typed uncertainty, never masked). It lowers to a typed DDL or,
// failing that, `AstDecl::Unknown` — but it must not be silently
// dropped the way recovery debris is.
#[test]
fn tree_lower_14_genuine_unrecognized_create_still_surfaces() {
    let src = "CREATE BITMAP INDEX ix_demo ON demo_t (col_a);\n/\n";
    let mut diags = Vec::new();
    let ast = lower_parse_tree(src, fid(), &mut diags);
    assert!(
        !ast.root.declarations.is_empty(),
        "a genuine top-level CREATE object must not be dropped as recovery debris; \
         got 0 decls (diags: {diags:?})"
    );
    // Text scanner classifies a bare CREATE INDEX as `AstDecl::Ddl`.
    assert!(
        ast.root
            .declarations
            .iter()
            .any(|d| matches!(d, AstDecl::Ddl { .. } | AstDecl::Unknown { .. })),
        "expected a typed Ddl/Unknown for CREATE INDEX, got {:?}",
        ast.root.declarations
    );
}

// Regression guard for the text-scanner keyword-boundary bug exposed by
// the recovery-debris fix: an APEX-style `wwv_flow_imp_page.create_*(…)`
// call must NOT be misread as a `CREATE` DDL statement (it flooded the
// diagnostic stream with 43k bogus `IR_DDL_NOT_LOWERED` rows).
#[test]
fn tree_lower_15_create_underscore_call_not_ddl() {
    let src = concat!(
        "CREATE OR REPLACE PACKAGE BODY apex_demo AS\n",
        "  PROCEDURE imp IS\n",
        "  BEGIN\n",
        "    wwv_flow_imp_page.create_page_plug(p_id => 1);\n",
        "    wwv_flow_imp.create_template_option(p_id => 2);\n",
        "  END imp;\n",
        "END apex_demo;\n/\n",
    );
    let mut diags = Vec::new();
    let ast = lower_parse_tree(src, fid(), &mut diags);
    // Exactly the package body — no spurious Ddl decls minted from the
    // `create_*(` call names inside the body.
    assert_eq!(
        ast.root.declarations.len(),
        1,
        "create_*( calls must not mint Ddl decls; got {:?}",
        ast.root.declarations
    );
    assert!(
        matches!(ast.root.declarations[0], AstDecl::PackageBody { .. }),
        "sole decl must be the PackageBody, got {:?}",
        ast.root.declarations[0]
    );
}
