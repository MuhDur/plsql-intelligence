//! Behavioral CLI tests for the `plsqld` binary (Axiom 0 — the
//! first thing any agent reaches for is `--help`, and it MUST never
//! be an error). Mirrors the analogous `usr-gate-rs` and
//! `plsql-bindgen` CLI surfaces.

use std::process::Command;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_plsqld")
}

#[test]
fn help_long_exits_zero_and_prints_usage() {
    let out = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "`plsqld --help` must exit 0 (Axiom 0); got {:?}; stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("plsqld"),
        "--help stdout must name the binary"
    );
    assert!(
        stdout.contains("<cache-dir>"),
        "--help must document the positional <cache-dir>"
    );
    assert!(
        stdout.contains("Unix-domain") || stdout.contains("UDS"),
        "--help must mention the Unix-domain socket footprint"
    );
}

#[test]
fn help_short_exits_zero() {
    let out = Command::new(bin_path()).arg("-h").output().expect("spawn");
    assert!(out.status.success(), "`plsqld -h` must exit 0");
}

#[test]
fn version_long_exits_zero_and_prints_version() {
    let out = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "`plsqld --version` must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("plsqld"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_short_exits_zero() {
    let out = Command::new(bin_path()).arg("-V").output().expect("spawn");
    assert!(out.status.success(), "`plsqld -V` must exit 0");
}

#[test]
fn capabilities_emits_parseable_json_with_required_keys() {
    let out = Command::new(bin_path())
        .arg("--capabilities")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "`plsqld --capabilities` must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("capabilities stdout must be valid JSON");
    assert_eq!(v["binary"], "plsqld");
    assert!(v["positional"]["<cache-dir>"].is_string());
    assert!(v["info_flags"].is_object());
    assert!(v["listen_socket_template"].is_string());
}

#[test]
fn robot_docs_exits_zero() {
    let out = Command::new(bin_path())
        .arg("--robot-docs")
        .output()
        .expect("spawn");
    assert!(out.status.success(), "`plsqld --robot-docs` must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Unix-domain"));
}

/// Critical regression: the legacy plsqld treated `--help` as a path
/// and tried to create a directory called `--help`. This must NEVER
/// happen again.
#[test]
fn help_does_not_attempt_to_create_a_help_directory() {
    let out = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("spawn");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("cache directory --help"),
        "plsqld must not interpret `--help` as a cache-dir path (oracle-g9p5 regression): {combined}"
    );
}

#[test]
fn unknown_flag_is_reported_with_pointer_to_help() {
    let out = Command::new(bin_path())
        .arg("--no-such-flag")
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "unknown flag must not exit 0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--no-such-flag") && stderr.contains("--help"),
        "unknown-flag stderr must name the bad flag and point at --help, got {stderr}"
    );
}

#[test]
fn bare_invocation_points_at_help() {
    let out = Command::new(bin_path()).output().expect("spawn");
    assert!(
        !out.status.success(),
        "bare `plsqld` must NOT silently exit 0 (no socket to bind)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--help") && stderr.contains("cache-dir"),
        "bare-invocation stderr must point at --help and name <cache-dir>, got {stderr}"
    );
}
