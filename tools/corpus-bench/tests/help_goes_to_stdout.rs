//! POSIX compliance: `--help` is documentation requested by the
//! user. It MUST go to stdout (so `tool --help | less`,
//! `tool --help > help.txt`, and `tool --help | grep flag` all
//! work), exit 0, and not write to stderr. clap CLIs already do
//! this; hand-rolled tools must match.
//!
//! The error path (unknown flag, missing arg) keeps writing the
//! usage block to stderr, which is also covered here so we never
//! drift back.

use std::process::Command;

#[test]
fn help_flag_writes_to_stdout_not_stderr() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("--help")
        .output()
        .expect("spawn corpus-bench --help");
    assert_eq!(
        out.status.code(),
        Some(0),
        "--help must exit 0; got {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.trim().is_empty(),
        "--help must write a usage block to stdout; stdout was empty. stderr={stderr:?}"
    );
    assert!(
        stderr.is_empty(),
        "--help must NOT write anything to stderr; got: {stderr:?}"
    );
    assert!(
        stdout.contains("usage:"),
        "stdout must contain a usage block; got: {stdout:?}"
    );
}

#[test]
fn short_help_flag_writes_to_stdout_not_stderr() {
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("-h")
        .output()
        .expect("spawn corpus-bench -h");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.trim().is_empty(), "-h must write to stdout");
    assert!(
        stderr.is_empty(),
        "-h must NOT write to stderr; got: {stderr:?}"
    );
}

#[test]
fn unknown_flag_keeps_usage_block_on_stderr() {
    // The error-path diagnostic is still a diagnostic and stays
    // on stderr per POSIX. We assert it explicitly so a future
    // refactor cannot accidentally move the usage block off
    // stderr in the error branch.
    let bin = env!("CARGO_BIN_EXE_corpus-bench");
    let out = Command::new(bin)
        .arg("--definitely-not-a-flag")
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown flag must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("usage:") || stderr.contains("unknown"),
        "error path must explain via stderr; got: {stderr:?}"
    );
}
