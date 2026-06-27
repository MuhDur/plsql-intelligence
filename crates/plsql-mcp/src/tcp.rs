//! Optional TCP transport for remote MCP agents.
//!
//! By default `plsql-mcp` speaks JSON-RPC 2.0 over stdio. For
//! remote agent sessions an operator may want to bind a TCP socket
//! and accept one client at a time.
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
//! * **Loopback-only by default**. Only `127.0.0.0/8`
//!   and `::1` bind without a flag. *Every* non-loopback target —
//!   `0.0.0.0`/`::`, RFC 1918 private ranges (`10/8`, `172.16/12`,
//!   `192.168/16`), IPv4 link-local (`169.254/16`), IPv6 ULA
//!   (`fc00::/7`) and link-local (`fe80::/10`), and public IPs —
//!   is **refused** unless the operator passes an explicit
//!   `--allow-public-bind`. This transport carries no
//!   authentication (no token, no TLS, no peer check), so on a
//!   shared LAN/VPC/container network "private" is not "trusted":
//!   any co-resident host that can reach the socket drives the
//!   full tool surface. The loopback default matches the module's
//!   "one agent per process / local dev transport" model.
//!
//! ## /oracle evidence
//!
//! * `DATABASE-REFERENCE.md` PL/SQL routing — the protocol layer
//!   atop this transport is `mcp_protocol::handle_request_line`,
//!   which defers per-tool dispatch to the `ToolRegistry`
//!   populated by the foundation live-DB modules. This module
//!   doesn't change Oracle behaviour; it changes how the agent
//!   reaches the engine.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mcp_protocol::PlsqlMcpServer;

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
        "--listen target {raw:?} is not a loopback address; the MCP transport is unauthenticated, so any non-loopback bind (including RFC1918/link-local private ranges) exposes the full tool surface to every host that can reach it — pass --allow-public-bind to override"
    )]
    PublicBindRefused { raw: String },
}

/// Parse a `--listen <host:port>` value. By default only loopback
/// (`127.0.0.0/8`, `::1`) is accepted; `allow_public_bind` lifts
/// the refusal for **any** non-loopback target — RFC 1918 /
/// link-local private ranges included.
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

/// Treat **only loopback** (`127.0.0.0/8`, `::1`) as safe by
/// default. Every non-loopback target — including
/// RFC 1918 private space (`10/8`, `172.16/12`, `192.168/16`),
/// IPv4 link-local (`169.254/16`), IPv6 unique-local (`fc00::/7`)
/// and link-local (`fe80::/10`), and any public address — requires
/// the explicit `--allow-public-bind` opt-in.
///
/// Rationale: this is an *unauthenticated* JSON-RPC transport
/// (no token, no TLS, no peer check). On a shared LAN, corporate
/// VPN subnet, cloud VPC, or multi-tenant container network,
/// "private" is not "trusted" — any co-resident host that can
/// reach the `ip:port` drives the full tool surface. Reachability
/// must not equal authorization, so the safe default is the
/// loopback model the module's "one agent per process" posture
/// already assumes.
fn is_safe_bind(ip: IpAddr) -> bool {
    ip.is_loopback()
}

/// Pretty-print the listen target for the doctor / startup log.
#[must_use]
pub fn describe(target: &ListenTarget) -> String {
    let safety = if target.allow_public_bind {
        "public-bind-allowed"
    } else {
        "loopback-only"
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
    server: &PlsqlMcpServer,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = server.handle_request_line(&line) {
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
fn handle_connection(stream: TcpStream, server: &PlsqlMcpServer) -> std::io::Result<()> {
    let read_half = stream.try_clone()?;
    let mut write_half = stream;
    process_stream(BufReader::new(read_half), &mut write_half, server)
}

/// Accept loop over an already-bound listener. `max_conns` bounds how many
/// connections to serve before returning (`None` = serve forever); the
/// bound exists so integration tests can run the real loop to completion.
fn serve_with_listener(
    listener: &TcpListener,
    server: &PlsqlMcpServer,
    max_conns: Option<usize>,
) -> std::io::Result<()> {
    for (idx, incoming) in listener.incoming().enumerate() {
        let stream = incoming?;
        if let Err(e) = handle_connection(stream, server) {
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
pub fn serve(target: &ListenTarget, server: &PlsqlMcpServer) -> std::io::Result<()> {
    let listener = TcpListener::bind(target.socket)?;
    tracing::info!(target = %describe(target), "plsql-mcp TCP transport listening");
    serve_with_listener(&listener, server, None)
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
    server: &PlsqlMcpServer,
    max_conns: usize,
) -> std::io::Result<()> {
    serve_with_listener(listener, server, Some(max_conns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolRegistry;

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
    fn private_rfc1918_refused_without_override() {
        // oracle-e1ro: RFC 1918 (10/8, 172.16/12, 192.168/16) is
        // "private" but not "trusted" — a co-resident host on a
        // shared LAN/VPC/container network can drive the whole
        // unauthenticated tool surface. The default is now
        // loopback-only; any RFC 1918 bind requires the explicit
        // `--allow-public-bind` opt-in.
        for raw in ["10.0.1.5:9000", "172.16.0.1:9000", "192.168.1.10:9000"] {
            let err = parse_listen_target(raw, false).unwrap_err();
            assert!(
                matches!(err, TcpConfigError::PublicBindRefused { .. }),
                "{raw} must be refused without --allow-public-bind"
            );
        }
    }

    #[test]
    fn link_local_refused_without_override() {
        // oracle-e1ro: IPv4 link-local (169.254/16) is reachable
        // by any co-resident host and must not bind by default.
        let err = parse_listen_target("169.254.1.1:9000", false).unwrap_err();
        assert!(matches!(err, TcpConfigError::PublicBindRefused { .. }));
    }

    #[test]
    fn unique_local_v6_refused_without_override() {
        // oracle-e1ro: IPv6 ULA (fc00::/7) is not loopback;
        // default-deny applies.
        let err = parse_listen_target("[fc00::1]:9000", false).unwrap_err();
        assert!(matches!(err, TcpConfigError::PublicBindRefused { .. }));
    }

    #[test]
    fn rfc1918_and_link_local_bind_with_override() {
        // oracle-e1ro: the opt-in flag still unlocks the wider
        // bind for operators who genuinely need it.
        for raw in [
            "10.0.1.5:9000",
            "172.16.0.1:9000",
            "192.168.1.10:9000",
            "169.254.1.1:9000",
            "[fc00::1]:9000",
        ] {
            let t = parse_listen_target(raw, true).expect("target should bind with override");
            assert!(t.allow_public_bind);
        }
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
        assert!(s.contains("loopback-only"));
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
        let server =
            PlsqlMcpServer::new(ToolRegistry::new()).expect("test MCP server runtime builds");
        let mut out: Vec<u8> = Vec::new();
        process_stream(Cursor::new(input.as_bytes()), &mut out, &server).unwrap();
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
        let server =
            PlsqlMcpServer::new(ToolRegistry::new()).expect("test MCP server runtime builds");
        let mut out: Vec<u8> = Vec::new();
        process_stream(Cursor::new(&b"not json\n"[..]), &mut out, &server).unwrap();
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
            let server =
                PlsqlMcpServer::new(ToolRegistry::new()).expect("test MCP server runtime builds");
            serve_with_listener(&listener, &server, Some(1)).unwrap();
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
