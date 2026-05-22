//! `plsqld` — optional local artifact-cache daemon
//! (PLSQL-STORE-DAEMON-002).
//!
//! Usage: `plsqld <cache-dir>`
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
//! Exit codes: `0` clean shutdown, `2` invocation/bind error.

fn main() -> std::process::ExitCode {
    #[cfg(unix)]
    {
        run_unix()
    }
    #[cfg(not(unix))]
    {
        eprintln!("plsqld requires a Unix-domain socket and is not supported on this platform");
        std::process::ExitCode::from(2)
    }
}

#[cfg(unix)]
fn run_unix() -> std::process::ExitCode {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    let mut args = std::env::args().skip(1);
    let Some(cache_dir) = args.next() else {
        eprintln!("usage: plsqld <cache-dir>");
        return std::process::ExitCode::from(2);
    };
    let cache_dir = PathBuf::from(cache_dir);
    if !cache_dir.is_dir() {
        eprintln!(
            "plsqld: cache directory {} does not exist (create it explicitly first)",
            cache_dir.display()
        );
        return std::process::ExitCode::from(2);
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
        return std::process::ExitCode::from(2);
    }

    let store = match plsql_store::Store::open(
        &cache_dir.join("cache.db"),
        plsql_store::StoreConfig::default(),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("plsqld: cannot open cache store: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("plsqld: cannot bind {}: {e}", sock_path.display());
            return std::process::ExitCode::from(2);
        }
    };
    eprintln!("plsqld: listening on {}", sock_path.display());

    for conn in listener.incoming() {
        let stream = match conn {
            Ok(s) => s,
            Err(e) => {
                eprintln!("plsqld: accept error: {e}");
                continue;
            }
        };
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
    std::process::ExitCode::SUCCESS
}
