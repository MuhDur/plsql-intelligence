#![forbid(unsafe_code)]
//! `plsqld` — optional local artifact-cache daemon.
//!
//! ## Usage
//!
//! ```text
//! plsqld <cache-dir>
//! plsqld --help
//! plsqld --version
//! plsqld --capabilities
//! plsqld --robot-docs
//! ```
//!
//! Binds a **Unix-domain socket** at `<cache-dir>/plsqld.sock`
//! and serves the cache SQLite at `<cache-dir>/cache.db`. It
//! never opens a TCP port and never makes an outbound connection
//! (R17 — no network telemetry); the cache directory is supplied
//! explicitly by the operator, never inferred. All request
//! dispatch is the unit-tested
//! [`plsql_store::serve_envelope`] — this file is only the
//! accept-loop shell.
//!
//! ## Exit codes
//!
//! ```text
//! 0  clean shutdown or informational flag handled
//! 2  invocation / bind error (e.g. missing cache dir, stale socket,
//!    bad flag, refused to clobber an existing socket)
//! ```

use std::process::ExitCode;

/// Stable contract version for the `--capabilities` payload. Bump
/// only on a breaking change to the JSON shape (Axiom 17 — every
/// contract surface has a drift-guard test).
const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

/// All recognised info-flag spellings (presented in the bare-invoke
/// hint so an operator who typed nothing learns the discovery surface
/// immediately).
const INFO_FLAGS: &[&str] = &[
    "--capabilities",
    "--help",
    "--robot-docs",
    "--version",
    "-V",
    "-h",
];

fn capabilities_json() -> serde_json::Value {
    serde_json::json!({
        "binary": "plsqld",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "Unix-domain socket daemon serving plsql-store envelope requests",
        "positional": {
            "<cache-dir>": "directory that holds cache.db (SQLite) and plsqld.sock (UDS); \
                            must exist; never auto-created (R17 — operator supplies it)"
        },
        "info_flags": {
            "-h / --help":     "print human usage and exit 0",
            "-V / --version":  "print binary version and exit 0",
            "--capabilities":  "print this machine-readable contract and exit 0",
            "--robot-docs":    "print a paste-ready agent handbook and exit 0"
        },
        "exit_codes": {
            "0": "clean shutdown or informational flag handled",
            "2": "invocation error (missing dir, stale socket, unknown flag)"
        },
        "listen_socket_template": "<cache-dir>/plsqld.sock",
        "store_db_template": "<cache-dir>/cache.db",
        "network": "Unix-domain socket only; never opens a TCP port; \
                    never makes an outbound connection (R17 — no telemetry)",
        "wire_protocol": "one JSON envelope per line over the Unix socket; \
                          dispatched by plsql_store::serve_envelope"
    })
}

fn robot_docs_text() -> String {
    format!(
        r#"plsqld agent handbook
=======================

WHAT IT DOES
  Optional local artifact-cache daemon. Serves the plsql-store
  envelope protocol over a Unix-domain socket bound under the
  operator-supplied cache directory.

CANONICAL INVOCATION
  plsqld /path/to/cache-dir

  The cache directory must already exist; plsqld never auto-creates
  it (R17 — the operator supplies the storage explicitly). The
  socket is bound at <cache-dir>/plsqld.sock and the SQLite store
  at <cache-dir>/cache.db.

INFO FLAGS
  -h / --help        this usage
  -V / --version     binary version
  --capabilities     machine-readable agent contract (JSON)
  --robot-docs       this handbook

EXIT CODES
  0   clean shutdown or info-flag handled
  2   invocation / bind error (missing dir, stale socket, unknown flag)

WIRE PROTOCOL
  One JSON envelope per line over the Unix socket; dispatched by
  plsql_store::serve_envelope (unit-tested).

NETWORK
  Unix-domain socket only. Never opens a TCP port; never makes an
  outbound connection (R17 — no network telemetry).

MACHINE-READABLE CONTRACT
  Run: plsqld --capabilities
  Pinned contract_version={contract_version}; a bump signals a
  breaking change to the JSON shape.
"#,
        contract_version = CAPABILITIES_CONTRACT_VERSION
    )
}

fn print_usage() {
    println!("usage: plsqld <cache-dir>");
    println!();
    println!("Optional local artifact-cache daemon.");
    println!();
    println!("Positional:");
    println!("  <cache-dir>      directory holding cache.db (SQLite) and plsqld.sock (UDS);");
    println!("                   must exist (R17 — never auto-created)");
    println!();
    println!("Info flags:");
    println!("  -h, --help          this usage");
    println!("  -V, --version       binary version");
    println!("      --capabilities  machine-readable agent contract (JSON)");
    println!("      --robot-docs    paste-ready agent handbook");
    println!();
    println!("Network: Unix-domain socket only (no TCP, no outbound; R17).");
    println!("Exit codes: 0 clean shutdown / info-flag handled; 2 invocation error.");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let first = args.get(1).map(String::as_str).unwrap_or("");

    // Info flags FIRST (Axiom 0). Any argument starting with `-` is
    // treated as a flag, never as a literal cache directory — fixes
    // the documented oracle-g9p5 behavior where `plsqld --help`
    // tried to create a directory called `--help`.
    match first {
        "-h" | "--help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" => {
            println!("plsqld {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "--capabilities" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&capabilities_json()).unwrap_or_default()
            );
            return ExitCode::SUCCESS;
        }
        "--robot-docs" => {
            print!("{}", robot_docs_text());
            return ExitCode::SUCCESS;
        }
        "" => {
            eprintln!("plsqld: missing positional <cache-dir>");
            eprintln!("info flags: {}", INFO_FLAGS.join(", "));
            eprintln!("run `plsqld --help` for usage");
            return ExitCode::from(2);
        }
        // Reject any other flag-shaped first arg instead of silently
        // misinterpreting it as a cache-dir path. The legacy behavior
        // would have tried to `is_dir()` a name like `--bogus` and
        // told the operator to create that directory — actively
        // misleading. Now we surface the unknown flag clearly.
        s if s.starts_with('-') => {
            eprintln!("plsqld: unknown flag {s:?}");
            eprintln!("info flags: {}", INFO_FLAGS.join(", "));
            eprintln!("run `plsqld --help` for usage");
            return ExitCode::from(2);
        }
        _ => {}
    }

    #[cfg(unix)]
    {
        run_unix(&args[1])
    }
    #[cfg(not(unix))]
    {
        let _ = &args; // silence unused warning on non-unix
        eprintln!("plsqld requires a Unix-domain socket and is not supported on this platform");
        ExitCode::from(2)
    }
}

#[cfg(unix)]
fn run_unix(cache_dir_arg: &str) -> ExitCode {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    let cache_dir = PathBuf::from(cache_dir_arg);
    if !cache_dir.is_dir() {
        eprintln!(
            "plsqld: cache directory {} does not exist (create it explicitly first)",
            cache_dir.display()
        );
        return ExitCode::from(2);
    }

    let sock_path = cache_dir.join("plsqld.sock");
    if sock_path.exists() {
        // Never auto-delete the operator's filesystem. A stale
        // socket is surfaced, not silently removed.
        eprintln!(
            "plsqld: {} already exists; if no daemon is running, remove the stale socket and \
             retry",
            sock_path.display()
        );
        return ExitCode::from(2);
    }

    let store = match plsql_store::Store::open(
        &cache_dir.join("cache.db"),
        plsql_store::StoreConfig::default(),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("plsqld: cannot open cache store: {e}");
            return ExitCode::from(2);
        }
    };

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("plsqld: cannot bind {}: {e}", sock_path.display());
            return ExitCode::from(2);
        }
    };
    eprintln!("plsqld: listening on {}", sock_path.display());

    // Connections are served sequentially (single-stream by design). Like
    // the sibling local transports — `plsql_doc::serve` (serve.rs:164) and
    // `plsql_mcp::tcp` (tcp.rs:13) — this is a dev/local daemon that mirrors
    // the "one agent per process" posture, so the per-store state machine
    // never sees concurrent requests. Concurrent connection fan-out
    // (thread-per-connection + `Arc<Store>`) is intentionally out of scope
    // until a real multi-client UDS consumer exists (none does today).
    //
    // Defensive read timeout: a stalled or idle client would otherwise hold
    // the serial loop open indefinitely. A 30s read timeout turns a stalled
    // `reader.lines()` into a `WouldBlock`/`TimedOut` error, which falls into
    // the `else { break }` arm below and ends that connection so the next one
    // can be accepted. `PLSQLD_READ_TIMEOUT_MS` is a test-only knob (the
    // regression test sets a short timeout so it need not wait the full 30s);
    // operators never set it.
    let read_timeout = std::env::var("PLSQLD_READ_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or_else(|| std::time::Duration::from_secs(30));
    for conn in listener.incoming() {
        let stream = match conn {
            Ok(s) => s,
            Err(e) => {
                eprintln!("plsqld: accept error: {e}");
                continue;
            }
        };
        if let Err(e) = stream.set_read_timeout(Some(read_timeout)) {
            eprintln!("plsqld: cannot set read timeout: {e}");
            continue;
        }
        let reader = BufReader::new(match stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("plsqld: stream clone error: {e}");
                continue;
            }
        });
        let mut writer = stream;
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let response = plsql_store::serve_envelope(&store, &line);
            if writer.write_all(response.as_bytes()).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drift-guard for the `--capabilities` agent contract (Axiom 17).
    /// If the JSON shape changes, this test must be updated AND
    /// [`CAPABILITIES_CONTRACT_VERSION`] bumped.
    #[test]
    fn capabilities_contract_is_pinned() {
        let c = capabilities_json();
        assert_eq!(c["binary"], "plsqld");
        assert_eq!(
            c["contract_version"],
            u64::from(CAPABILITIES_CONTRACT_VERSION)
        );
        assert_eq!(c["version"], env!("CARGO_PKG_VERSION"));
        for key in [
            "mode",
            "positional",
            "info_flags",
            "exit_codes",
            "listen_socket_template",
            "store_db_template",
            "network",
            "wire_protocol",
        ] {
            assert!(c.get(key).is_some(), "capabilities missing key `{key}`");
        }
        // Info flags must mention each long form so agents can grep.
        let info = c["info_flags"].as_object().expect("info_flags is a map");
        let combined = serde_json::to_string(&info).unwrap();
        for f in ["--help", "--version", "--capabilities", "--robot-docs"] {
            assert!(
                combined.contains(f),
                "capabilities info_flags must mention `{f}`, got {combined}"
            );
        }
        // R17 footprint: no TCP, no outbound.
        let net = c["network"].as_str().unwrap_or("");
        assert!(net.contains("Unix-domain") && net.contains("never"));
    }

    #[test]
    fn capabilities_serializes_to_single_line_json() {
        let s = serde_json::to_string(&capabilities_json()).unwrap();
        assert!(
            !s.contains('\n'),
            "single-line JSON must not contain newlines"
        );
        let round: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(round["binary"], "plsqld");
    }

    #[test]
    fn robot_docs_mentions_socket_and_protocol() {
        let docs = robot_docs_text();
        assert!(docs.contains("plsqld"));
        assert!(docs.contains("Unix-domain"));
        assert!(docs.contains("serve_envelope"));
        for flag in ["--help", "--version", "--capabilities", "--robot-docs"] {
            assert!(docs.contains(flag), "robot-docs must mention `{flag}`");
        }
    }
}
