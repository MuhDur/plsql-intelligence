#![no_main]

use std::fs;

use libfuzzer_sys::fuzz_target;
use plsql_engine::{AnalysisRequest, InvocationShapeV1, analyze_project};
use plsql_parser::{ParseOptions, parse_with_backend};
use plsql_parser_antlr::Antlr4RustBackend;

use plsql_fuzz::check_effects_invariants;

const MAX_INPUT_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }

    let Ok(body) = std::str::from_utf8(data) else {
        return;
    };
    if body
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\n' | '\r'))
    {
        return;
    }
    let source = format!(
        "CREATE OR REPLACE PROCEDURE fuzz_root IS
         BEGIN
           EXECUTE IMMEDIATE 'CREATE TABLE fuzz_effects (value NUMBER)';
           {body}
         END fuzz_root;
         /"
    );

    let parse = parse_with_backend(
        &source,
        plsql_core::FileId::new(0),
        &Antlr4RustBackend::new(),
        &ParseOptions::default(),
    );
    let has_parse_diagnostics = !parse.is_clean() || parse.recovered;

    let project = tempfile::tempdir().expect("create per-input project directory");
    fs::write(project.path().join("fuzz.sql"), &source).expect("write synthetic PL/SQL input");
    let run = analyze_project(AnalysisRequest {
        project_root: project.path().to_path_buf(),
        ..AnalysisRequest::default()
    })
    .expect("analyze synthetic fuzz project");
    let closure = run.invocation_closure(&InvocationShapeV1 {
        routine_id: "FUZZ_ROOT".to_string(),
        ..InvocationShapeV1::default()
    });

    // Cross-check parser diagnostics against the production parse/lower/
    // closure/effect path. A mismatch is a fuzz finding, not a tolerated
    // degraded result.
    check_effects_invariants(&closure, has_parse_diagnostics)
        .expect("effects extraction invariant violated");
});
