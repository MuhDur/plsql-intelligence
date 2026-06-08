//! Behavioral CLI tests for the `usr-gate-rs` binary (Axiom 0 — the
//! first thing any agent reaches for is `--help`, and it MUST never
//! be an error). Mirrors the analogous `plsql-bindgen` CLI surface.

use std::process::Command;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_usr-gate-rs")
}

#[test]
fn help_long_exits_zero_and_prints_usage() {
    let out = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "`usr-gate-rs --help` must exit 0; got {:?}; stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("usr-gate-rs"),
        "--help stdout must name the binary, got {stdout}"
    );
    for sub in [
        "roundtrip",
        "honesty",
        "residue",
        "baseline-cmp",
        "metrics",
        "pins",
    ] {
        assert!(
            stdout.contains(sub),
            "--help must list subcommand `{sub}`, got {stdout}"
        );
    }
}

#[test]
fn help_short_exits_zero() {
    let out = Command::new(bin_path()).arg("-h").output().expect("spawn");
    assert!(
        out.status.success(),
        "`usr-gate-rs -h` must exit 0; got {:?}",
        out.status
    );
}

#[test]
fn version_long_exits_zero_and_prints_version() {
    let out = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "`usr-gate-rs --version` must exit 0; got {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("usr-gate-rs") && stdout.contains(env!("CARGO_PKG_VERSION")),
        "--version stdout must include binary name and version, got {stdout}"
    );
}

#[test]
fn version_short_exits_zero() {
    let out = Command::new(bin_path()).arg("-V").output().expect("spawn");
    assert!(out.status.success(), "`usr-gate-rs -V` must exit 0");
}

#[test]
fn capabilities_emits_parseable_json_with_required_keys() {
    let out = Command::new(bin_path())
        .arg("--capabilities")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "`usr-gate-rs --capabilities` must exit 0; got {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("capabilities stdout must be valid JSON");
    assert_eq!(v["binary"], "usr-gate-rs");
    assert!(v["subcommands"].is_object());
    assert!(v["info_flags"].is_object());
    assert!(v["exit_codes"].is_object());
    assert!(v["env"]["USR_GATE_TRUST_PINS"].is_string());
}

#[test]
fn robot_docs_exits_zero_and_mentions_subcommands() {
    let out = Command::new(bin_path())
        .arg("--robot-docs")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "--robot-docs must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in [
        "roundtrip",
        "honesty",
        "residue",
        "baseline-cmp",
        "metrics",
        "pins",
    ] {
        assert!(stdout.contains(sub), "robot-docs must mention `{sub}`");
    }
    assert!(stdout.contains("USR_GATE_TRUST_PINS"));
}

#[test]
fn bare_invocation_is_not_zero_but_points_at_help() {
    let out = Command::new(bin_path()).output().expect("spawn");
    assert!(
        !out.status.success(),
        "bare `usr-gate-rs` must NOT silently exit 0 (there is nothing to do)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--help"),
        "bare-invocation stderr must point at --help so the operator can recover, got {stderr}"
    );
}

#[test]
fn unknown_subcommand_points_at_help() {
    let out = Command::new(bin_path())
        .arg("bogus-xyzzy")
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "unknown subcommand must not exit 0; got {:?}",
        out.status
    );
    // The legacy contract printed the unknown-sub message to stdout
    // (so the gate script captures it as evidence); we preserve that.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--help") && stdout.contains("bogus-xyzzy"),
        "unknown-subcommand evidence must name the bad token and point at --help, got {stdout}"
    );
}
