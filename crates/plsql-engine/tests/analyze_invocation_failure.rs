//! End-to-end CLI contract: `plsql-engine analyze` must surface
//! invocation failures (nonexistent / non-directory project root) as
//! a non-zero exit + a stderr message that names the offending path —
//! never as a "Clean" empty-result JSON envelope on stdout.
//!
//! This pins the Axiom 14 (never-silent-fail) hole an agent that
//! mistypes a project path would otherwise fall through.

use std::process::Command;

#[test]
fn analyze_nonexistent_path_exits_nonzero_and_names_path_on_stderr() {
    let bin = env!("CARGO_BIN_EXE_plsql-engine");
    let bogus = "/nonexistent/plsql-engine-analyze-cli-contract";

    let out = Command::new(bin)
        .args(["analyze", bogus])
        .output()
        .expect("spawn plsql-engine analyze");

    assert!(
        !out.status.success(),
        "analyze of a nonexistent path must exit non-zero; got status {:?}, stdout={}, stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stderr_flat = flatten_for_path_match(&stderr);
    assert!(
        stderr.contains("does not exist"),
        "stderr must describe the missing-path failure: {stderr}"
    );
    assert!(
        stderr_flat.contains(bogus),
        "stderr must echo the offending path: {stderr}"
    );

    // The stdout must not be a fake "Clean" posture envelope.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\"posture\""),
        "analyze of a missing path must not emit a posture envelope on stdout: {stdout}"
    );
}

/// Strip ANSI line-wrap chrome miette inserts so substring checks on
/// long paths survive its pretty printer. We collapse runs of
/// whitespace and the box-drawing continuation glyph (`│`) so a path
/// that miette split across two lines still matches as one token.
fn flatten_for_path_match(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '│' {
            continue;
        }
        out.push(ch);
    }
    out
}

#[test]
fn analyze_nonexistent_path_in_robot_json_mode_also_fails_cleanly() {
    let bin = env!("CARGO_BIN_EXE_plsql-engine");
    let bogus = "/nonexistent/plsql-engine-analyze-cli-contract-robot";

    let out = Command::new(bin)
        .args(["--robot-json", "analyze", bogus])
        .output()
        .expect("spawn plsql-engine --robot-json analyze");

    assert!(
        !out.status.success(),
        "analyze of a nonexistent path must exit non-zero even in --robot-json mode; \
         got status {:?}, stdout={}, stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // The exit code must be 2 (invocation failure) per the capabilities
    // exit-code dictionary — not 0 (success), not 1 (runtime error).
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit code 2 (invocation failure) for nonexistent path"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stderr_flat = flatten_for_path_match(&stderr);
    assert!(
        stderr_flat.contains(bogus),
        "stderr must name the offending path: {stderr}"
    );
}
