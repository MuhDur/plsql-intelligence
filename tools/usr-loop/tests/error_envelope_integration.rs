//! Integration test for the error-envelope contract: every error
//! path under `--robot-json` must emit a single-line
//! `plsql.usr.error_envelope` v1 JSON object on stdout (matching the
//! success-envelope contract), so `usr-loop --robot-json … | jq .`
//! pipelines never see an empty stdout.

use std::process::Command;

#[test]
fn scan_nonexistent_estate_emits_error_envelope_in_robot_json() {
    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .args(["--robot-json", "scan", "/nonexistent-path-for-usr-loop"])
        .output()
        .expect("spawn usr-loop");

    // Non-zero exit (estate_not_found → exit 1).
    assert!(!out.status.success(), "must exit non-zero");

    // Human diagnostic still on stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("estate path does not exist"),
        "stderr must carry the human diagnostic; got: {stderr}"
    );

    // Robot-JSON error envelope on stdout — a `| jq .` pipeline reading
    // stdout MUST see structured output, not an empty buffer.
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(
        !stdout.trim().is_empty(),
        "stdout must carry the error envelope in --robot-json mode"
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e}): {stdout}"));
    assert_eq!(v["format"], "plsql-robot-json");
    assert_eq!(v["schema_id"], "plsql.usr.error_envelope");
    assert_eq!(v["schema_version"]["major"], 1);
    assert_eq!(v["payload"]["kind"], "error");
    assert_eq!(v["payload"]["code"], "estate_not_found");
    assert!(v["payload"]["message"].is_string());
    assert!(v["payload"]["path"].is_string());

    // Single-line stdout (robot-mode contract).
    let trimmed = stdout.trim_end();
    assert!(
        !trimmed.contains('\n'),
        "robot-json error envelope must be single-line; got: {trimmed:?}"
    );
}

#[test]
fn scan_nonexistent_estate_human_mode_writes_no_stdout_envelope() {
    // In non-robot mode, stderr carries the diagnostic and stdout
    // stays empty — no surprise envelope.
    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .args(["scan", "/nonexistent-path-for-usr-loop"])
        .output()
        .expect("spawn usr-loop");

    assert!(!out.status.success(), "must exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("estate path does not exist"),
        "stderr must carry the human diagnostic"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "human mode must keep stdout empty for this error; got: {stdout:?}"
    );
}

#[test]
fn capabilities_subcommand_emits_pinned_contract() {
    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .args(["--robot-json", "capabilities"])
        .output()
        .expect("spawn usr-loop");

    assert!(out.status.success(), "capabilities must exit 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e})"));
    assert_eq!(v["binary"], "usr-loop");
    assert!(v["contract_version"].is_number());
    assert!(v["subcommands"]["scan"].is_string());
    assert!(v["subcommands"]["capabilities"].is_string());
    assert!(v["subcommands"]["robot-docs"].is_string());
    assert!(v["exit_codes"]["0"].is_string());
}

#[test]
fn robot_docs_subcommand_emits_handbook() {
    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .args(["robot-docs"])
        .output()
        .expect("spawn usr-loop");

    assert!(out.status.success(), "robot-docs must exit 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(stdout.contains("usr-loop agent handbook"));
    assert!(stdout.contains("capabilities"));
    assert!(stdout.contains("--robot-triage"));
}

#[test]
fn robot_triage_emits_mega_object() {
    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .args(["--robot-json", "--robot-triage"])
        .output()
        .expect("spawn usr-loop");

    // Healthy posture → exit 0.
    assert!(
        out.status.success(),
        "--robot-triage must exit 0 on healthy"
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e})"));
    for key in ["capabilities", "health", "quick_ref"] {
        assert!(v.get(key).is_some(), "triage missing `{key}`");
    }
    assert_eq!(v["capabilities"]["binary"], "usr-loop");
}
