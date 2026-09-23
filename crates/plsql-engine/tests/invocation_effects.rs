//! Source-backed effect cases. Each case parses synthetic PL/SQL through the
//! production ANTLR pipeline and emits one JSONL expected/actual record.

use std::collections::BTreeSet;

use plsql_engine::{
    AnalysisRequest, AnalysisRun, InvocationArgType, InvocationClosureV1, InvocationShapeV1,
    OperatorStatementClass, RoutineEffect, RoutineEffectsV1, analyze_project,
};

fn analyze(source: &str) -> AnalysisRun {
    analyze_files(&[("fixture.sql", source)])
}

fn analyze_files(files: &[(&str, &str)]) -> AnalysisRun {
    let root = std::env::temp_dir().join(format!(
        "plsql-effects-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in files {
        std::fs::write(root.join(name), source).unwrap();
    }
    analyze_project(AnalysisRequest {
        project_root: root,
        ..AnalysisRequest::default()
    })
    .unwrap()
}

fn closure(run: &AnalysisRun, id: &str, positional_count: usize) -> InvocationClosureV1 {
    run.invocation_index.closure(&InvocationShapeV1 {
        routine_id: id.to_string(),
        positional_count,
        ..InvocationShapeV1::default()
    })
}

fn assert_case(id: &str, actual: &InvocationClosureV1, expected: &[RoutineEffect], clean: bool) {
    let expected: BTreeSet<_> = expected.iter().copied().collect();
    println!(
        "{}",
        serde_json::json!({
            "case_id": id,
            "expected_effects": expected,
            "actual_effects": actual.effects.effects,
            "expected_clean": clean,
            "actual_clean": actual.clean,
            "units": actual.units,
            "unknown_reasons": actual.unknown_reasons,
        })
    );
    assert_eq!(actual.effects.effects, expected, "{id}");
    assert_eq!(actual.clean, clean, "{id}");
}

fn procedure(statement: &str) -> AnalysisRun {
    analyze(&format!(
        "CREATE OR REPLACE PROCEDURE fx IS BEGIN {statement} END fx; /"
    ))
}

#[test]
fn gap1_dml_ddl_commit_are_distinct_effects() {
    let cases = [
        ("UPDATE omcp_t SET n = 1;", RoutineEffect::Dml),
        (
            "EXECUTE IMMEDIATE 'CREATE TABLE omcp_t (n NUMBER)';",
            RoutineEffect::Ddl,
        ),
        ("COMMIT;", RoutineEffect::TxnControl),
    ];
    for (source, effect) in cases {
        let run = procedure(source);
        assert_case(
            "gap1_dml_ddl_commit_are_distinct_effects",
            &closure(&run, "FX", 0),
            &[effect],
            true,
        );
    }
}

#[test]
fn gap2_commit_rollback_and_autonomous_emit_effects() {
    for statement in [
        "COMMIT;",
        "ROLLBACK;",
        "SAVEPOINT before_work;",
        "SET TRANSACTION READ ONLY;",
    ] {
        let run = procedure(statement);
        assert_case(
            "gap2_commit_rollback_and_autonomous_emit_effects",
            &closure(&run, "FX", 0),
            &[RoutineEffect::TxnControl],
            true,
        );
        assert!(
            run.dep_graph
                .edges
                .iter()
                .any(|edge| { edge.kind == plsql_depgraph::EdgeKind::TransactionControl })
        );
    }
    let run = analyze(
        "CREATE OR REPLACE PROCEDURE fx IS PRAGMA AUTONOMOUS_TRANSACTION; BEGIN NULL; END fx; /",
    );
    assert_case(
        "gap2_commit_rollback_and_autonomous_emit_effects",
        &closure(&run, "FX", 0),
        &[RoutineEffect::Autonomous],
        true,
    );
    assert!(
        run.dep_graph
            .edges
            .iter()
            .any(|edge| edge.kind == plsql_depgraph::EdgeKind::Autonomous)
    );
}

#[test]
fn gap3_routine_without_edges_is_not_proven() {
    let run = procedure("UTL_HTTP.REQUEST('http://example.invalid/');");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "gap3_routine_without_edges_is_not_proven",
        &actual,
        &[RoutineEffect::ExternalIo, RoutineEffect::Unknown],
        false,
    );
    assert!(
        run.dep_graph
            .edges
            .iter()
            .any(|edge| edge.kind == plsql_depgraph::EdgeKind::UnresolvedCallee)
    );
}

#[test]
fn gap4_exact_identity_no_case_fold() {
    let run = analyze(
        r#"CREATE OR REPLACE PACKAGE fx AS PROCEDURE "MixedCase"; PROCEDURE mixedcase; PROCEDURE caller; END fx;
/
CREATE OR REPLACE PACKAGE BODY fx AS
  PROCEDURE "MixedCase" IS BEGIN COMMIT; END;
  PROCEDURE mixedcase IS BEGIN NULL; END;
  PROCEDURE caller IS BEGIN "MixedCase"(); END;
END fx; /"#,
    );
    let quoted = closure(&run, "FX.MixedCase", 0);
    let unquoted = closure(&run, "FX.MIXEDCASE", 0);
    assert_case(
        "gap4_exact_identity_no_case_fold",
        &quoted,
        &[RoutineEffect::TxnControl],
        true,
    );
    assert_case("gap4_exact_identity_no_case_fold", &unquoted, &[], true);
    assert_case(
        "gap4_quoted_call_resolves_exact_member",
        &closure(&run, "FX.CALLER", 0),
        &[RoutineEffect::TxnControl],
        true,
    );
}

#[test]
fn quoted_separator_identity_is_unknown() {
    let run = analyze(
        "CREATE OR REPLACE PACKAGE fx AS PROCEDURE \"A.B\"; END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS PROCEDURE \"A.B\" IS BEGIN NULL; END; END fx; /",
    );
    assert_case(
        "quoted_separator_identity_is_unknown",
        &closure(&run, "FX.A.B", 0),
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn overload_invocation_shape_selects_member() {
    let run = analyze(
        "CREATE OR REPLACE PACKAGE fx AS PROCEDURE p(x NUMBER); PROCEDURE p(x VARCHAR2); END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS PROCEDURE p(x NUMBER) IS BEGIN COMMIT; END p; PROCEDURE p(x VARCHAR2) IS BEGIN UPDATE omcp_t SET n = 1; END p; END fx; /",
    );
    let number = run.invocation_index.closure(&InvocationShapeV1 {
        routine_id: "FX.P".into(),
        positional_count: 1,
        argument_types: vec![InvocationArgType::Number],
        overload: Some(1),
        ..InvocationShapeV1::default()
    });
    let text = run.invocation_index.closure(&InvocationShapeV1 {
        routine_id: "FX.P".into(),
        positional_count: 1,
        argument_types: vec![InvocationArgType::Text],
        overload: Some(2),
        ..InvocationShapeV1::default()
    });
    let bind = closure(&run, "FX.P", 1);
    let typed_without_overload = run.invocation_index.closure(&InvocationShapeV1 {
        routine_id: "FX.P".into(),
        positional_count: 1,
        argument_types: vec![InvocationArgType::Number],
        ..InvocationShapeV1::default()
    });
    assert_case(
        "overload_invocation_shape_selects_member_number",
        &number,
        &[RoutineEffect::TxnControl],
        true,
    );
    assert_case(
        "overload_invocation_shape_selects_member_text",
        &text,
        &[RoutineEffect::Dml],
        true,
    );
    assert_case(
        "overload_invocation_shape_selects_member_bind",
        &bind,
        &[RoutineEffect::Unknown],
        false,
    );
    assert_case(
        "overload_invocation_shape_requires_exact_ordinal",
        &typed_without_overload,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn gap5_partial_completeness_not_clean() {
    let run = analyze("CREATE OR REPLACE PACKAGE fx AS PROCEDURE p; END fx; /");
    let actual = closure(&run, "FX.P", 0);
    assert_case(
        "gap5_partial_completeness_not_clean",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn per_closure_clean_survives_unrelated_bad_file() {
    let run = analyze_files(&[
        (
            "fx.sql",
            "CREATE OR REPLACE PROCEDURE fx IS BEGIN NULL; END fx; /",
        ),
        (
            "broken.sql",
            "CREATE OR REPLACE PROCEDURE broken WRAPPED a000000 /",
        ),
    ]);
    assert_case(
        "per_closure_clean_survives_unrelated_bad_file",
        &closure(&run, "FX", 0),
        &[],
        true,
    );
}

#[test]
fn declared_member_without_body_is_unknown() {
    let run = analyze(
        "CREATE OR REPLACE PACKAGE fx AS PROCEDURE p; END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS PROCEDURE q IS BEGIN NULL; END q; END fx; /",
    );
    assert_case(
        "declared_member_without_body_is_unknown",
        &closure(&run, "FX.P", 0),
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn select_calling_nextval_routine_has_sequence_advance() {
    let run = analyze(
        "CREATE OR REPLACE FUNCTION seq_fn RETURN NUMBER IS BEGIN RETURN omcp_seq.NEXTVAL; END seq_fn; /\nCREATE OR REPLACE PROCEDURE fx IS n NUMBER; BEGIN SELECT seq_fn() INTO n FROM dual; END fx; /",
    );
    let actual = closure(&run, "FX", 0);
    assert_case(
        "select_calling_nextval_routine_has_sequence_advance",
        &actual,
        &[RoutineEffect::ReadDb, RoutineEffect::SequenceAdvance],
        true,
    );
    assert!(actual.units.contains("SEQ_FN"));
}

#[test]
fn select_for_update_routine_has_row_lock() {
    let run = procedure("SELECT n INTO v_n FROM omcp_t FOR UPDATE;");
    assert_case(
        "select_for_update_routine_has_row_lock",
        &closure(&run, "FX", 0),
        &[RoutineEffect::ReadDb, RoutineEffect::RowLock],
        true,
    );
    assert!(
        run.dep_graph
            .edges
            .iter()
            .any(|edge| edge.kind == plsql_depgraph::EdgeKind::RowLock)
    );
    let multiline = procedure("SELECT n INTO v_n FROM omcp_t FOR\nUPDATE;");
    assert_case(
        "select_for_update_multiline_has_row_lock",
        &closure(&multiline, "FX", 0),
        &[RoutineEffect::ReadDb, RoutineEffect::RowLock],
        true,
    );
}

#[test]
fn spaced_sequence_nextval_in_sql_has_sequence_advance() {
    let run = procedure("SELECT omcp_seq . NEXTVAL INTO v_n FROM dual;");
    assert_case(
        "spaced_sequence_nextval_in_sql_has_sequence_advance",
        &closure(&run, "FX", 0),
        &[RoutineEffect::ReadDb, RoutineEffect::SequenceAdvance],
        true,
    );
}

#[test]
fn literal_alter_database_default_edition_is_operator_only() {
    let run = procedure("EXECUTE IMMEDIATE 'ALTER DATABASE DEFAULT EDITION = e2';");
    assert_case(
        "literal_alter_database_default_edition_is_operator_only",
        &closure(&run, "FX", 0),
        &[RoutineEffect::OperatorOnly(
            OperatorStatementClass::DefaultEditionFlip,
        )],
        true,
    );
}

#[test]
fn literal_execute_immediate_create_table_folds_to_ddl() {
    let run = procedure("EXECUTE IMMEDIATE 'CREATE TABLE omcp_t (n NUMBER)';");
    assert_case(
        "literal_execute_immediate_create_table_folds_to_ddl",
        &closure(&run, "FX", 0),
        &[RoutineEffect::Ddl],
        true,
    );
    assert!(
        run.dep_graph
            .edges
            .iter()
            .any(|edge| edge.kind == plsql_depgraph::EdgeKind::Ddl)
    );
}

#[test]
fn malformed_literal_ddl_is_unknown() {
    let run = procedure("EXECUTE IMMEDIATE 'CREATE TABLE';");
    assert_case(
        "malformed_literal_ddl_is_unknown",
        &closure(&run, "FX", 0),
        &[RoutineEffect::DynamicSql, RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn concatenated_execute_immediate_is_dynamic_sql() {
    let run = procedure("EXECUTE IMMEDIATE 'CREATE TABLE ' || p_name;");
    assert_case(
        "concatenated_execute_immediate_is_dynamic_sql",
        &closure(&run, "FX", 0),
        &[RoutineEffect::DynamicSql, RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn variable_execute_immediate_is_dynamic_sql() {
    let run = procedure("EXECUTE IMMEDIATE v_sql;");
    assert_case(
        "variable_execute_immediate_is_dynamic_sql",
        &closure(&run, "FX", 0),
        &[RoutineEffect::DynamicSql, RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn execute_immediate_runtime_clause_is_unknown() {
    let run = procedure("EXECUTE IMMEDIATE 'UPDATE omcp_t SET n = :1' USING v_n;");
    assert_case(
        "execute_immediate_runtime_clause_is_unknown",
        &closure(&run, "FX", 0),
        &[RoutineEffect::Dml, RoutineEffect::Unknown],
        false,
    );
}

fn package(init: &str, declaration: &str, member: &str) -> AnalysisRun {
    analyze(&format!(
        "CREATE OR REPLACE PACKAGE fx AS PROCEDURE p(p_n NUMBER DEFAULT 1); END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS {declaration} PROCEDURE p(p_n NUMBER DEFAULT 1) IS BEGIN {member} END p; BEGIN {init} END fx; /"
    ))
}

#[test]
fn package_init_with_dml_in_closure() {
    let run = package("INSERT INTO omcp_log(n) VALUES (1);", "", "NULL;");
    assert_case(
        "package_init_with_dml_in_closure",
        &closure(&run, "FX.P", 1),
        &[RoutineEffect::Dml],
        true,
    );
}

#[test]
fn package_init_with_commit_in_closure() {
    let run = package("COMMIT;", "", "NULL;");
    assert_case(
        "package_init_with_commit_in_closure",
        &closure(&run, "FX.P", 1),
        &[RoutineEffect::TxnControl],
        true,
    );
}

#[test]
fn package_init_with_autonomous_in_closure() {
    let run = package("NULL;", "PRAGMA AUTONOMOUS_TRANSACTION;", "NULL;");
    let actual = closure(&run, "FX.P", 1);
    assert_case(
        "package_init_with_autonomous_in_closure",
        &actual,
        &[RoutineEffect::Autonomous],
        true,
    );
}

#[test]
fn package_init_with_nextval_in_closure() {
    let run = package("g_n := omcp_seq.NEXTVAL;", "g_n NUMBER;", "NULL;");
    let actual = closure(&run, "FX.P", 1);
    assert_case(
        "package_init_with_nextval_in_closure",
        &actual,
        &[RoutineEffect::SessionState, RoutineEffect::SequenceAdvance],
        true,
    );
    assert!(
        run.dep_graph
            .edges
            .iter()
            .any(|edge| edge.kind == plsql_depgraph::EdgeKind::SequenceAdvance)
    );
}

#[test]
fn uninitialized_package_variable_write_is_session_state() {
    let run = package("NULL;", "g_n NUMBER;", "g_n := 1;");
    assert_case(
        "uninitialized_package_variable_write_is_session_state",
        &closure(&run, "FX.P", 1),
        &[RoutineEffect::SessionState],
        true,
    );
}

#[test]
fn qualified_assignment_target_is_unknown() {
    let run = package("NULL;", "", "other_pkg.g_n := 1;");
    assert_case(
        "qualified_assignment_target_is_unknown",
        &closure(&run, "FX.P", 1),
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn package_init_with_utl_call_in_closure() {
    let run = package(
        "g_s := UTL_HTTP.REQUEST('http://example.invalid/');",
        "g_s VARCHAR2(100);",
        "NULL;",
    );
    let actual = closure(&run, "FX.P", 1);
    assert_case(
        "package_init_with_utl_call_in_closure",
        &actual,
        &[
            RoutineEffect::SessionState,
            RoutineEffect::ExternalIo,
            RoutineEffect::Unknown,
        ],
        false,
    );
}

fn package_with_effectful_helper(initializer: &str, default: &str) -> AnalysisRun {
    analyze(&format!(
        "CREATE OR REPLACE FUNCTION effectful RETURN NUMBER IS BEGIN UPDATE omcp_t SET n = 1; RETURN 1; END effectful; /\nCREATE OR REPLACE PACKAGE fx AS PROCEDURE p(p_n NUMBER DEFAULT {default}); END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS {initializer} PROCEDURE p(p_n NUMBER DEFAULT {default}) IS BEGIN NULL; END p; END fx; /"
    ))
}

#[test]
fn declaration_initializer_effectful_function_in_closure() {
    let run = package_with_effectful_helper("g_n NUMBER := effectful();", "1");
    let actual = closure(&run, "FX.P", 1);
    assert_case(
        "declaration_initializer_effectful_function_in_closure",
        &actual,
        &[RoutineEffect::SessionState, RoutineEffect::Dml],
        true,
    );
    assert!(actual.units.contains("EFFECTFUL"));
}

#[test]
fn spec_and_body_initializers_in_closure() {
    let run = analyze(
        "CREATE OR REPLACE FUNCTION effectful RETURN NUMBER IS BEGIN UPDATE omcp_t SET n = 1; RETURN 1; END effectful; /\nCREATE OR REPLACE PACKAGE fx AS g_spec NUMBER := effectful(); PROCEDURE p; END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS g_body NUMBER := effectful(); PROCEDURE p IS BEGIN NULL; END p; END fx; /",
    );
    let actual = closure(&run, "FX.P", 0);
    assert_case(
        "spec_and_body_initializers_in_closure",
        &actual,
        &[RoutineEffect::SessionState, RoutineEffect::Dml],
        true,
    );
    assert!(actual.units.contains("FX#spec_initializers"));
    assert!(actual.units.contains("FX#body_initializers"));
}

#[test]
fn omitted_default_expression_effectful_function_in_closure() {
    let run = package_with_effectful_helper("", "effectful()");
    let actual = closure(&run, "FX.P", 0);
    assert_case(
        "omitted_default_expression_effectful_function_in_closure",
        &actual,
        &[RoutineEffect::Dml],
        true,
    );
    assert!(actual.units.iter().any(|id| id.contains("#default")));
}

#[test]
fn explicit_argument_does_not_evaluate_default() {
    let run = package_with_effectful_helper("", "effectful()");
    let actual = closure(&run, "FX.P", 1);
    assert_case(
        "explicit_argument_does_not_evaluate_default",
        &actual,
        &[],
        true,
    );
    assert!(!actual.units.iter().any(|id| id.contains("#default")));
}

#[test]
fn repeated_member_calls_preserve_omitted_default_effect() {
    let run = analyze(
        "CREATE OR REPLACE FUNCTION effectful RETURN NUMBER IS BEGIN UPDATE omcp_t SET n = 1; RETURN 1; END effectful; /\nCREATE OR REPLACE PACKAGE fx AS PROCEDURE p(x NUMBER DEFAULT effectful()); PROCEDURE caller; END fx; /\nCREATE OR REPLACE PACKAGE BODY fx AS PROCEDURE p(x NUMBER DEFAULT effectful()) IS BEGIN NULL; END p; PROCEDURE caller IS BEGIN p(1); p(); END caller; END fx; /",
    );
    let actual = closure(&run, "FX.CALLER", 0);
    assert_case(
        "repeated_member_calls_preserve_omitted_default_effect",
        &actual,
        &[RoutineEffect::Dml],
        true,
    );
    assert!(actual.units.iter().any(|id| id.contains("#default")));
}

macro_rules! opaque_call_case {
    ($name:ident, $statement:expr, $effect:expr) => {
        #[test]
        fn $name() {
            let run = procedure($statement);
            let actual = closure(&run, "FX", 0);
            assert_case(
                stringify!($name),
                &actual,
                &[$effect, RoutineEffect::Unknown],
                false,
            );
        }
    };
}

opaque_call_case!(
    dbms_sql_is_unknown,
    "DBMS_SQL.OPEN_CURSOR();",
    RoutineEffect::DynamicSql
);
opaque_call_case!(
    dbms_scheduler_is_unknown,
    "DBMS_SCHEDULER.CREATE_JOB('j');",
    RoutineEffect::ExternalIo
);
opaque_call_case!(
    dbms_pipe_is_unknown,
    "DBMS_PIPE.RECEIVE_MESSAGE('p');",
    RoutineEffect::ExternalIo
);
opaque_call_case!(
    dbms_aq_is_unknown,
    "DBMS_AQ.ENQUEUE('q');",
    RoutineEffect::ExternalIo
);
opaque_call_case!(
    utl_call_is_unknown,
    "UTL_HTTP.REQUEST('http://example.invalid/');",
    RoutineEffect::ExternalIo
);

#[test]
fn wrapped_body_is_unknown() {
    let run = analyze("CREATE OR REPLACE PROCEDURE fx WRAPPED\na000000\n/");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "wrapped_body_is_unknown",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn java_call_spec_is_unknown() {
    let run = analyze("CREATE OR REPLACE PROCEDURE fx AS LANGUAGE JAVA NAME 'X.f()'; /");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "java_call_spec_is_unknown",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn external_procedure_is_unknown() {
    let run = analyze("CREATE OR REPLACE PROCEDURE fx AS EXTERNAL NAME 'f' LIBRARY l; /");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "external_procedure_is_unknown",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn authid_current_user_is_unknown() {
    let run =
        analyze("CREATE OR REPLACE PROCEDURE fx AUTHID CURRENT_USER IS BEGIN NULL; END fx; /");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "authid_current_user_is_unknown",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn construct_to_effect_mapping_is_exhaustive() {
    // The production walker has a wildcard-free match over every IR
    // Statement and Expr variant. This fixture also catches a new parser
    // construct that arrives as an unclassified statement: it must become
    // Unknown rather than disappearing.
    let run = procedure("PIPE ROW(1);");
    let actual = closure(&run, "FX", 0);
    assert_case(
        "construct_to_effect_mapping_is_exhaustive",
        &actual,
        &[RoutineEffect::Unknown],
        false,
    );
}

#[test]
fn routine_effects_contract_rejects_wrong_version() {
    let effect_set = RoutineEffectsV1::new([RoutineEffect::ReadDb]);
    let json = serde_json::to_string(&effect_set).unwrap();
    assert!(json.contains("\"contract\":\"RoutineEffectsV1\""));
    let wrong = json.replace("RoutineEffectsV1", "RoutineEffectsV2");
    assert!(serde_json::from_str::<RoutineEffectsV1>(&wrong).is_err());
    let run = procedure("NULL;");
    let closure_json = serde_json::to_string(&closure(&run, "FX", 0)).unwrap();
    assert!(closure_json.contains("\"contract\":\"InvocationClosureV1\""));
    let wrong_closure = closure_json.replace("InvocationClosureV1", "InvocationClosureV2");
    assert!(serde_json::from_str::<InvocationClosureV1>(&wrong_closure).is_err());
}
