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

/// Regression (oracle-qbqf.5): the serial accept loop applies a per-connection
/// read timeout, so an idle client that never sends a line is dropped instead
/// of stalling the daemon forever. We verify this end-to-end: launch plsqld
/// with a short `PLSQLD_READ_TIMEOUT_MS`, open a connection and send nothing,
/// then open a *second* connection that gets a framed response — proving the
/// accept loop kept running after timing out the idle connection.
#[cfg(unix)]
#[test]
fn idle_connection_is_dropped_and_loop_keeps_serving() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    // RAII short-path tempdir. Unix-domain socket paths are bounded by SUN_LEN
    // (~108 bytes), so a long worktree-scoped TMPDIR can be unusable for the
    // socket. Prefer TMPDIR when it is short enough; this keeps the test off
    // quota-limited /tmp mounts on build hosts.
    struct ShortTempDir(std::path::PathBuf);
    impl Drop for ShortTempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // RAII guard for the daemon: the accept loop runs forever, so it must be
    // killed when the test ends. Doing so on Drop (not just at the tail of the
    // function) means a panicking assertion below still reaps the child rather
    // than leaking a parked daemon that would keep the test harness pipe open.
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir_name = format!("plsqld-qbqf5-{}-{nanos}", std::process::id());
    let base_root = [
        std::env::var_os("TMPDIR"),
        std::env::var_os("XDG_RUNTIME_DIR"),
    ]
    .into_iter()
    .flatten()
    .map(std::path::PathBuf::from)
    .find(|root| root.join(&dir_name).join("plsqld.sock").as_os_str().len() < 104)
    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let base = base_root.join(dir_name);
    std::fs::create_dir_all(&base).expect("create short tempdir");
    let dir = ShortTempDir(base);
    let sock_path = dir.0.join("plsqld.sock");

    let child = ChildGuard(
        Command::new(bin_path())
            .arg(&dir.0)
            // Short read timeout so the idle connection is reaped quickly and
            // the test does not have to wait the 30s production default.
            .env("PLSQLD_READ_TIMEOUT_MS", "300")
            .spawn()
            .expect("spawn plsqld daemon"),
    );

    // Wait for the daemon to bind the socket (it is created lazily after the
    // store opens). Poll rather than sleep a fixed interval.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !sock_path.exists() {
        assert!(
            Instant::now() < deadline,
            "plsqld did not bind {} within 10s",
            sock_path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Connection #1: connect and send NOTHING, and crucially KEEP IT OPEN
    // (no EOF). Because the accept loop is serial, the daemon is now parked in
    // `reader.lines()` on this connection. Without the read timeout it would
    // block here forever (silent peer, socket never closed), starving every
    // later client. With the 300ms read timeout, `reader.lines()` errors out
    // and the `else { break }` arm ends this connection so the loop advances.
    let _idle = UnixStream::connect(&sock_path).expect("connect idle client");

    // Connection #2: connect() succeeds via the listen backlog, but it is only
    // *serviced* once the daemon finishes (i.e. reaps) connection #1. We bound
    // the client read with a 5s timeout: if the daemon never reaped the idle
    // connection (no fix), no response arrives and `read_line` errors below —
    // the regression fails loudly instead of hanging the suite.
    let mut active = UnixStream::connect(&sock_path).expect("connect active client");
    active
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set client read timeout");
    // An unframed line yields a framed error response; we only need to prove a
    // response comes back, i.e. the loop is alive and dispatching.
    active
        .write_all(b"not-a-valid-frame\n")
        .expect("write request");
    active.flush().expect("flush request");

    let mut reader = BufReader::new(active);
    let mut response = String::new();
    let n = reader
        .read_line(&mut response)
        .expect("daemon must answer the second client after reaping the idle one (read timed out)");
    assert!(
        n > 0 && !response.trim().is_empty(),
        "daemon must answer the second client after reaping the idle one; got empty response"
    );
    // The framed response is JSON; confirm it parses, proving the wire is intact.
    let v: serde_json::Value =
        serde_json::from_str(response.trim()).expect("daemon response must be framed JSON");
    assert!(
        v.get("payload").is_some(),
        "framed daemon response must carry a payload, got {response}"
    );

    // `child` (ChildGuard) and `dir` (ShortTempDir) reap the daemon and remove
    // the socket dir on drop; keep them alive until here so the daemon stays up
    // for the assertions above.
    drop(child);
    drop(dir);
}
