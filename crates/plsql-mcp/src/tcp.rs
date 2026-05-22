//! Optional TCP transport for remote MCP agents (`PLSQL-MCP-008`).
//!
//! By default `plsql-mcp` speaks JSON-RPC 2.0 over stdio
//! (`PLSQL-MCP-002`). For remote agent sessions an operator may
//! want to bind a TCP socket and accept one client at a time.
//! This module ships the parsed `--listen` flag configuration,
//! validates the bind target, and exposes a thin runtime hook
//! the binary can call to enter the accept-loop.
//!
//! The accept loop is intentionally minimal:
//!
//! * Bind once to the supplied `host:port`.
//! * Accept connections sequentially (single-threaded), matching
//!   the stdio path's "one agent per process" posture so the
//!   per-tool state machine never sees concurrent requests.
//! * For each connection: framing is line-delimited JSON (same
//!   as stdio) — read a line, hand it to
//!   [`crate::handle_request_line`], write the response line.
//! * Close on EOF or first I/O error.
//!
//! Refusals (in `parse_listen_target`):
//! * Public-internet binds (`0.0.0.0:*`, `::`) are **refused by
//!   default**. The operator must pass an explicit
//!   `--allow-public-bind` to override — the goal is to prevent
//!   an accidental copy-paste from exposing the MCP surface to
//!   the open net.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL routing — the protocol layer
//!   atop this transport is `mcp_protocol::handle_request_line`,
//!   which defers per-tool dispatch to the `ToolRegistry`
//!   populated by the foundation live-DB beads. This module
//!   doesn't change Oracle behaviour; it changes how the agent
//!   reaches the engine.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mcp_protocol::handle_request_line;
use crate::tools::ToolRegistry;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenTarget {
    pub socket: SocketAddr,
    pub allow_public_bind: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TcpConfigError {
    #[error("--listen target is empty")]
    Empty,
    #[error("--listen target {raw:?} is not a valid <host>:<port> pair: {detail}")]
    InvalidSocket { raw: String, detail: String },
    #[error(
        "--listen target {raw:?} binds to a public-internet address; pass --allow-public-bind to override"
    )]
    PublicBindRefused { raw: String },
}

/// Parse a `--listen <host:port>` value. `allow_public_bind`
/// disables the default refusal of `0.0.0.0` / `::` /
/// non-loopback non-private targets.
pub fn parse_listen_target(
    raw: &str,
    allow_public_bind: bool,
) -> Result<ListenTarget, TcpConfigError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TcpConfigError::Empty);
    }
    let socket: SocketAddr =
        trimmed.parse().map_err(
            |e: std::net::AddrParseError| TcpConfigError::InvalidSocket {
                raw: trimmed.into(),
                detail: e.to_string(),
            },
        )?;
    if !allow_public_bind && !is_safe_bind(socket.ip()) {
        return Err(TcpConfigError::PublicBindRefused {
            raw: trimmed.into(),
        });
    }
    Ok(ListenTarget {
        socket,
        allow_public_bind,
    })
}

/// Treat loopback and RFC 1918 / unique-local addresses as
/// "safe by default". Everything else (including `0.0.0.0` /
/// `::`) requires the explicit opt-in flag.
fn is_safe_bind(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            // Loopback or unique-local (fc00::/7) or link-local.
            v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || is_v6_link_local(v6)
        }
    }
}

fn is_v6_link_local(v6: std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

/// Pretty-print the listen target for the doctor / startup log.
#[must_use]
pub fn describe(target: &ListenTarget) -> String {
    let safety = if target.allow_public_bind {
        "public-bind-allowed"
    } else {
        "loopback-or-private-only"
    };
    format!("tcp://{} ({safety})", target.socket)
}

// ── TCP accept loop (PLSQL-MCP-008B / oracle-k8ef) ────────────────────────────
//
// `parse_listen_target` (above) validates a `--listen <host:port>` value;
// this section is the transport itself. It mirrors the stdio model: one
// line-delimited JSON-RPC request per line, dispatched through the
// transport-agnostic `mcp_protocol::handle_request_line`, one response line
// back. Connections are served sequentially (a local dev/agent transport,
// like stdio, is single-stream); multiple requests per connection are
// supported via the line loop. Concurrent *connection* fan-out is
// intentionally out of scope (would require `ToolRegistry: Send + Sync` and
// belongs to the broader serve-loop bead, oracle-vnlk / PLSQL-MCP-002).

/// Pure request/response pump over any line reader + writer. Factored out
/// of the socket path so it is unit-testable without binding a port: read
/// newline-delimited JSON-RPC requests, dispatch each, write the response
/// (if any) followed by `\n`. Blank lines are skipped. Returns when the
/// reader reaches EOF.
/// Pure request/response pump, reused verbatim by the stdio transport in
/// the `plsql-mcp` binary (a separate crate, so this must be `pub`). Keeping
/// stdio and TCP on the exact same dispatch loop means a request can never
/// be answered differently depending on the transport it arrived on.
pub fn process_stream<R: BufRead, W: Write>(
    reader: R,
    writer: &mut W,
    registry: &ToolRegistry,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle_request_line(&line, registry) {
            let mut bytes = serde_json::to_vec(&resp)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            bytes.push(b'\n');
            writer.write_all(&bytes)?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Serve one accepted TCP connection: many line-delimited requests until
/// the peer closes the stream.
fn handle_connection(stream: TcpStream, registry: &ToolRegistry) -> std::io::Result<()> {
    let read_half = stream.try_clone()?;
    let mut write_half = stream;
    process_stream(BufReader::new(read_half), &mut write_half, registry)
}

/// Accept loop over an already-bound listener. `max_conns` bounds how many
/// connections to serve before returning (`None` = serve forever); the
/// bound exists so integration tests can run the real loop to completion.
fn serve_with_listener(
    listener: &TcpListener,
    registry: &ToolRegistry,
    max_conns: Option<usize>,
) -> std::io::Result<()> {
    for (idx, incoming) in listener.incoming().enumerate() {
        let stream = incoming?;
        if let Err(e) = handle_connection(stream, registry) {
            tracing::warn!(error = %e, "tcp connection terminated with an error");
        }
        if max_conns.is_some_and(|max| idx + 1 >= max) {
            break;
        }
    }
    Ok(())
}

/// Bind the validated `--listen` target and serve MCP over TCP forever.
///
/// The target is assumed to have passed [`parse_listen_target`] (which
/// enforces the public-bind refusal); `serve` does not re-validate, it
/// honours the decision already encoded in `ListenTarget`.
pub fn serve(target: &ListenTarget, registry: &ToolRegistry) -> std::io::Result<()> {
    let listener = TcpListener::bind(target.socket)?;
    tracing::info!(target = %describe(target), "plsql-mcp TCP transport listening");
    serve_with_listener(&listener, registry, None)
}

/// Run the **real** accept loop over a caller-supplied, already-bound
/// listener for a bounded number of connections, then return.
///
/// This is the same code path `serve` drives (`serve_with_listener`),
/// only with the listener injected and a connection bound so a hermetic
/// integration test can bind an ephemeral `127.0.0.1:0` port *once*,
/// read back `local_addr()`, and hand the **live** listener straight in
/// — eliminating the bind→drop→rebind window that was racing the OS
/// ephemeral-port pool under test parallelism. Production `serve` is
/// unchanged; this only adds a test-facing entry point and does not
/// touch [`parse_listen_target`] semantics.
pub fn serve_bounded_on_listener(
    listener: &TcpListener,
    registry: &ToolRegistry,
    max_conns: usize,
) -> std::io::Result<()> {
    serve_with_listener(listener, registry, Some(max_conns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_v4_accepted_without_override() {
        let t = parse_listen_target("127.0.0.1:9000", false).unwrap();
        assert_eq!(t.socket.port(), 9000);
        assert!(!t.allow_public_bind);
    }

    #[test]
    fn loopback_v6_accepted_without_override() {
        let t = parse_listen_target("[::1]:9001", false).unwrap();
        assert_eq!(t.socket.port(), 9001);
    }

    #[test]
    fn private_rfc1918_accepted_without_override() {
        // 10.0.0.0/8 is RFC 1918 private space.
        let t = parse_listen_target("10.0.1.5:9000", false).unwrap();
        assert_eq!(t.socket.port(), 9000);
    }

    #[test]
    fn link_local_accepted_without_override() {
        let t = parse_listen_target("169.254.1.1:9000", false).unwrap();
        assert_eq!(t.socket.port(), 9000);
    }

    #[test]
    fn unique_local_v6_accepted_without_override() {
        let t = parse_listen_target("[fc00::1]:9000", false).unwrap();
        assert_eq!(t.socket.port(), 9000);
    }

    #[test]
    fn zero_address_refused_without_override() {
        let err = parse_listen_target("0.0.0.0:9000", false).unwrap_err();
        assert!(matches!(err, TcpConfigError::PublicBindRefused { .. }));
    }

    #[test]
    fn public_v4_refused_without_override() {
        let err = parse_listen_target("8.8.8.8:9000", false).unwrap_err();
        assert!(matches!(err, TcpConfigError::PublicBindRefused { .. }));
    }

    #[test]
    fn override_flag_unlocks_public_bind() {
        let t = parse_listen_target("0.0.0.0:9000", true).unwrap();
        assert!(t.allow_public_bind);
    }

    #[test]
    fn empty_input_rejected() {
        assert_eq!(
            parse_listen_target("   ", false).unwrap_err(),
            TcpConfigError::Empty
        );
    }

    #[test]
    fn malformed_socket_rejected() {
        let err = parse_listen_target("not-a-socket", false).unwrap_err();
        assert!(matches!(err, TcpConfigError::InvalidSocket { .. }));
    }

    #[test]
    fn describe_includes_safety_tag() {
        let t = parse_listen_target("127.0.0.1:9000", false).unwrap();
        let s = describe(&t);
        assert!(s.contains("tcp://"));
        assert!(s.contains("loopback-or-private-only"));
    }

    #[test]
    fn describe_marks_public_bind_when_overridden() {
        let t = parse_listen_target("0.0.0.0:9000", true).unwrap();
        assert!(describe(&t).contains("public-bind-allowed"));
    }

    // ── TCP accept loop (PLSQL-MCP-008B) ──────────────────────────────────

    use std::io::{Cursor, Read};

    #[test]
    fn process_stream_dispatches_each_line_and_skips_blanks() {
        // Pure pump test — no socket. Two real JSON-RPC requests separated
        // by a blank line; expect exactly two response lines back.
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n",
            "\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        );
        let reg = ToolRegistry::new();
        let mut out: Vec<u8> = Vec::new();
        process_stream(Cursor::new(input.as_bytes()), &mut out, &reg).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2, "two requests → two responses: {text:?}");
        for (want_id, line) in [1, 2].iter().zip(&lines) {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["jsonrpc"], "2.0");
            assert_eq!(v["id"], *want_id);
            assert!(
                v.get("result").is_some() || v.get("error").is_some(),
                "well-formed JSON-RPC response: {line}"
            );
        }
    }

    #[test]
    fn process_stream_returns_parse_error_for_garbage_line() {
        let reg = ToolRegistry::new();
        let mut out: Vec<u8> = Vec::new();
        process_stream(Cursor::new(&b"not json\n"[..]), &mut out, &reg).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        assert_eq!(v["error"]["code"], -32700, "JSON parse error code");
    }

    #[test]
    fn serve_with_listener_round_trips_a_real_loopback_connection() {
        // Bind an ephemeral loopback port, serve exactly one connection on a
        // thread, then drive it with a real TcpStream client.
        let target = parse_listen_target("127.0.0.1:0", false).unwrap();
        let listener = TcpListener::bind(target.socket).unwrap();
        let bound = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let reg = ToolRegistry::new();
            serve_with_listener(&listener, &reg, Some(1)).unwrap();
        });

        let mut client = TcpStream::connect(bound).unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/list\"}\n")
            .unwrap();
        client.flush().unwrap();
        // Half-close the write side so the server's line loop sees EOF and
        // the connection (and the max_conns=1 accept loop) completes.
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().unwrap();

        let v: serde_json::Value =
            serde_json::from_str(resp.lines().next().expect("a response line")).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["jsonrpc"], "2.0");
        assert!(v.get("result").is_some() || v.get("error").is_some());
    }
}
