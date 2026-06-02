// Regression test (hunt round oracle-y54x.4): `lower::scan_declarations` must
// NOT mis-promote a DDL verb (CREATE/ALTER/DROP/GRANT/REVOKE/COMMENT) embedded
// in a STRING LITERAL — e.g. dynamic SQL `EXECUTE IMMEDIATE 'DROP TABLE …'`
// inside a multi-subprogram PACKAGE/TYPE BODY — into a phantom top-level
// `AstDecl::Ddl`.
//
// Root cause (now fixed): the top-level scan skipped whitespace + comments but
// NOT string/q-quote literals, while the depth-0 `advance_past_statement_end`
// truncated inside a 2+ routine body, re-exposing the body interior to that
// string-literal-blind keyword scan. The scan now routes through
// `recover::skip_opaque_span`, so quoted DDL verbs are inert.

use plsql_core::FileId;
use plsql_parser::ast::AstDecl;
use plsql_parser_antlr::lower::lower_source;

fn ddl_kinds(src: &str) -> Vec<String> {
    lower_source(src, FileId::new(0))
        .root
        .declarations
        .iter()
        .filter_map(|d| match d {
            AstDecl::Ddl { kind, .. } => Some(kind.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn no_phantom_ddl_from_body_string_literal() {
    // p1's `END;` truncates the depth-0 advance, so p2's body string literal
    // `'DROP TABLE secret'` is re-scanned by the top-level keyword loop. The
    // quoted DROP must be inert — no phantom DDL decl.
    let src = "CREATE OR REPLACE PACKAGE BODY pkg AS \
               PROCEDURE p1 IS BEGIN NULL; END; \
               PROCEDURE p2 IS BEGIN EXECUTE IMMEDIATE 'DROP TABLE secret'; END; \
               END pkg;";
    let kinds = ddl_kinds(src);
    assert!(
        !kinds.iter().any(|k| k.contains("DROP")),
        "quoted DROP inside a routine body must not mint a phantom DDL decl; got {kinds:?}"
    );
}

#[test]
fn no_phantom_ddl_from_qquote_body_literal() {
    // Same shape but the dynamic SQL uses an Oracle q-quote literal whose body
    // contains both a `GRANT` verb and an apostrophe — must stay inert.
    let src = "CREATE OR REPLACE PACKAGE BODY pkg AS \
               PROCEDURE p1 IS BEGIN NULL; END; \
               PROCEDURE p2 IS BEGIN EXECUTE IMMEDIATE q'[GRANT DBA TO o'brien]'; END; \
               END pkg;";
    let kinds = ddl_kinds(src);
    assert!(
        !kinds.iter().any(|k| k.contains("GRANT")),
        "q-quoted GRANT inside a routine body must not mint a phantom DDL decl; got {kinds:?}"
    );
}

#[test]
fn real_top_level_ddl_still_detected() {
    // Positive control: a genuine, unquoted top-level DDL statement must still
    // be lowered to an `AstDecl::Ddl` — the string-skip must not blind the scan
    // to real DDL that merely follows a string literal earlier in the file.
    let src = "COMMENT ON TABLE hr.emp IS 'employee records'; \
               DROP TABLE hr.old_emp; \
               GRANT SELECT ON hr.emp TO analyst;";
    let kinds = ddl_kinds(src);
    assert!(
        kinds.iter().any(|k| k.contains("DROP")),
        "a real top-level DROP must still be detected; got {kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| k.contains("GRANT")),
        "a real top-level GRANT after a string literal must still be detected; got {kinds:?}"
    );
}
