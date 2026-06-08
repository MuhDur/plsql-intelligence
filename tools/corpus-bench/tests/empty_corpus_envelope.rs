//! Integration test for the empty-corpus contract: `corpus-bench
//! --corpus-root /nonexistent` must NOT write the "nothing to
//! benchmark" line to stdout (which would break `| jq` pipelines) and
//! must emit a valid empty-report envelope on stdout under
//! `--robot-json`.

use std::process::Command;

#[test]
fn empty_corpus_under_robot_json_emits_valid_envelope_on_stdout() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .args(["--robot-json", "--corpus-root", "/nonexistent-corpus-bench"])
        .output()
        .expect("spawn corpus-bench");

    // Exit 2 = invocation error (empty corpus).
    assert_eq!(out.status.code(), Some(2), "must exit 2 on empty corpus");

    // Human diagnostic on stderr (still helpful for humans).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no PL/SQL files found"),
        "stderr must carry the human diagnostic; got: {stderr}"
    );

    // Stdout must carry a valid JSON envelope — a `| jq .` pipeline
    // must never see invalid JSON on stdout.
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(
        !stdout.trim().is_empty(),
        "--robot-json mode must emit an envelope even on the empty-corpus path"
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e}): {stdout}"));
    assert_eq!(v["schema_id"], "corpus-bench.report");
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["file_count"], 0);
    assert_eq!(v["empty_corpus"], true);
    assert!(v["summary"].is_null());
    assert_eq!(v["per_file"].as_array().unwrap().len(), 0);
}

#[test]
fn empty_corpus_human_mode_keeps_stdout_silent() {
    // In human mode, the diagnostic is on stderr — stdout stays
    // empty so `jq` etc. never sees a stray line.
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .args(["--corpus-root", "/nonexistent-corpus-bench"])
        .output()
        .expect("spawn corpus-bench");

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no PL/SQL files found"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "human mode must keep stdout empty for the empty-corpus error; got: {stdout:?}"
    );
}

#[test]
fn empty_non_default_corpus_appends_hint() {
    // The default-path hint should appear in stderr only when
    // --corpus-root differs from the canonical default.
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .args(["--corpus-root", "/nonexistent-corpus-bench"])
        .output()
        .expect("spawn corpus-bench");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("corpus-bench --corpus-root ./corpus"),
        "stderr must hint at the default corpus root; got: {stderr}"
    );
}

#[test]
fn capabilities_flag_emits_pinned_contract() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("--capabilities")
        .output()
        .expect("spawn corpus-bench");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e})"));
    assert_eq!(v["binary"], "corpus-bench");
    assert!(v["contract_version"].is_number());
    assert!(v["flags"]["--robot-json"].is_string());
}

#[test]
fn version_flag_emits_version_line() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .expect("spawn corpus-bench");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(stdout.starts_with("corpus-bench "));
}

#[test]
fn unknown_flag_emits_dym_hint() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("--robotjson")
        .output()
        .expect("spawn corpus-bench");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("did you mean") && stderr.contains("--robot-json"),
        "stderr should suggest --robot-json; got: {stderr}"
    );
}
