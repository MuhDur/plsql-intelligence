//! Local HTTP preview server.
//!
//! Minimal single-threaded synchronous HTTP/1.1 server using only
//! `std::net`. No tokio, no third-party HTTP crate — the goal is a
//! zero-dep preview that ships inside `plsql-doc` itself. Trust model:
//! local-only (binds 127.0.0.1 by default); the server SHOULD NOT be
//! exposed to a network.
//!
//! Routes:
//!
//! | Path | Body |
//! |------|------|
//! | `/` or `/index.html` | `render_full_html_bundle(set, label)` |
//! | `/index.md` | `render_full_markdown_bundle(set, label)` |
//! | `/object/<object_id>.html` | `render_object_html(obj)` |
//! | `/object/<object_id>.md` | `render_object_markdown(obj)` |
//! | anything else | 404 |
//!
//! The server is intentionally tiny: it parses the request line,
//! ignores headers, never reads request bodies, and returns
//! `Connection: close` so the connection ends after one response.
//!
//! ## Bind safety
//!
//! The preview server has **no authentication** and serves
//! rendered PL/SQL source, schema docs, and dependency graphs
//! over plain HTTP. Binding it to anything other than loopback
//! would expose that content to every host that can reach the
//! `ip:port`. To make the "local-only" trust model a *control*
//! rather than a comment, [`serve_preview_blocking`] **refuses**
//! any non-loopback target — including `0.0.0.0` / `::`, RFC 1918
//! private space (`10/8`, `172.16/12`, `192.168/16`), IPv4
//! link-local (`169.254/16`), IPv6 unique-local (`fc00::/7`) and
//! link-local (`fe80::/10`), and every routable public address —
//! unless the caller passes `allow_public_bind = true`. This
//! mirrors the deliberate public-bind refusal in
//! `plsql-mcp::tcp::parse_listen_target` so the two TCP-serving
//! crates in this workspace share one bind-safety posture; the
//! `is_safe_bind` predicates in both crates are kept byte-aligned
//! (loopback-only).

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;

use crate::{
    DocSet, ObjectDoc, render_full_html_bundle, render_full_markdown_bundle, render_object_html,
    render_object_markdown,
};

/// Why a requested preview bind address was refused.
#[derive(Debug)]
pub enum BindGuardError {
    /// `addr` did not resolve to any socket address.
    NoAddress,
    /// The resolved target is a public-internet address and the
    /// caller did not pass `allow_public_bind = true`. The
    /// preview server has no authentication; binding it publicly
    /// would expose unauthenticated source/schema content.
    PublicBindRefused(SocketAddr),
}

impl std::fmt::Display for BindGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindGuardError::NoAddress => {
                write!(f, "the requested address did not resolve to any socket")
            }
            BindGuardError::PublicBindRefused(s) => write!(
                f,
                "refusing to bind preview server to public-internet address {s}: \
                 it serves unauthenticated source/schema content — pass \
                 allow_public_bind=true to override"
            ),
        }
    }
}

impl std::error::Error for BindGuardError {}

impl From<BindGuardError> for std::io::Error {
    fn from(e: BindGuardError) -> Self {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, e)
    }
}

/// Treat **only loopback** (`127.0.0.0/8`, `::1`) as safe by
/// default. Every non-loopback target — including
/// RFC 1918 private space (`10/8`, `172.16/12`, `192.168/16`),
/// IPv4 link-local (`169.254/16`), IPv6 unique-local (`fc00::/7`)
/// and link-local (`fe80::/10`), and any public address — requires
/// the explicit `allow_public_bind` opt-in.
///
/// This is the same predicate `plsql-mcp::tcp::is_safe_bind`
/// applies to the MCP TCP transport, kept deliberately identical
/// so the two TCP-serving crates never diverge on what counts as
/// a "local" bind. Rationale: this preview server is *unauthenticated*
/// (no token, no TLS, no peer check) and serves rendered PL/SQL
/// source and schema docs. On a shared LAN, corporate VPN subnet,
/// cloud VPC, or multi-tenant container network, "private" is not
/// "trusted" — any co-resident host that can reach the `ip:port`
/// can pull the rendered source. Reachability must not equal
/// authorization, so the safe default is the loopback model.
#[must_use]
pub fn is_safe_bind(ip: IpAddr) -> bool {
    ip.is_loopback()
}

/// Resolve `addr` to a single socket address and enforce the
/// public-bind guard. Returns the address to bind, or a
/// [`BindGuardError`] if it does not resolve or is a public
/// target the caller did not opt into.
///
/// When `allow_public_bind` is `true` the guard is skipped: the
/// caller has explicitly accepted exposing the unauthenticated
/// preview surface on a routable interface.
pub fn guard_bind<A: ToSocketAddrs>(
    addr: A,
    allow_public_bind: bool,
) -> Result<SocketAddr, BindGuardError> {
    let resolved = addr
        .to_socket_addrs()
        .map_err(|_| BindGuardError::NoAddress)?
        .next()
        .ok_or(BindGuardError::NoAddress)?;
    if !allow_public_bind && !is_safe_bind(resolved.ip()) {
        return Err(BindGuardError::PublicBindRefused(resolved));
    }
    Ok(resolved)
}

/// Run the preview server on `addr`, blocking the caller forever.
/// Returns only on a fatal bind error.
///
/// `set` is cloned into an `Arc` so the response handler can borrow it
/// without lifetimes leaking into the server signature. `project_label`
/// shows up in the index `<title>` and `<h1>`.
///
/// `allow_public_bind` controls the bind-safety guard: when
/// `false` (the default posture), a non-loopback / non-private
/// `addr` is refused with [`BindGuardError::PublicBindRefused`]
/// — surfaced as a `PermissionDenied` I/O error — because the
/// preview server has no authentication. Pass `true` only when
/// you have deliberately decided to expose the rendered
/// source/schema content on a routable interface.
pub fn serve_preview_blocking<A: ToSocketAddrs>(
    addr: A,
    set: DocSet,
    project_label: impl Into<String>,
    allow_public_bind: bool,
) -> std::io::Result<()> {
    let addr = guard_bind(addr, allow_public_bind)?;
    let listener = TcpListener::bind(addr)?;
    let actual = listener.local_addr()?;
    eprintln!("[plsql-doc --serve] listening on http://{actual} — Ctrl+C to stop");
    let state = Arc::new(ServerState {
        set,
        project_label: project_label.into(),
    });
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                // Single-threaded by design: one request at a time. A
                // future bead (or PLSQL-DOC-013, if filed) can move to
                // a thread-per-connection model. For local preview the
                // serial loop is fine.
                if let Err(err) = handle_connection(stream, &state) {
                    eprintln!("[plsql-doc --serve] handler error: {err}");
                }
            }
            Err(err) => {
                eprintln!("[plsql-doc --serve] accept error: {err}");
            }
        }
    }
    Ok(())
}

/// Test-friendly variant: bind, accept exactly `n` connections, then
/// return. Used by the unit tests below so they don't need a Ctrl+C
/// to terminate. Applies the same bind guard as
/// [`serve_preview_blocking`] (`allow_public_bind = false`); the
/// tests only ever bind `127.0.0.1`, so the guard is transparent.
#[doc(hidden)]
pub fn serve_preview_for_n<A: ToSocketAddrs>(
    addr: A,
    set: DocSet,
    project_label: impl Into<String>,
    n: usize,
) -> std::io::Result<SocketAddr> {
    let addr = guard_bind(addr, false)?;
    let listener = TcpListener::bind(addr)?;
    let actual = listener.local_addr()?;
    let state = Arc::new(ServerState {
        set,
        project_label: project_label.into(),
    });
    for _ in 0..n {
        let (stream, _) = listener.accept()?;
        if let Err(err) = handle_connection(stream, &state) {
            eprintln!("[plsql-doc --serve test] handler error: {err}");
        }
    }
    Ok(actual)
}

struct ServerState {
    set: DocSet,
    project_label: String,
}

fn handle_connection(stream: TcpStream, state: &ServerState) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    // Drain headers until empty line — we don't act on them.
    let mut header = String::new();
    loop {
        header.clear();
        let n = reader.read_line(&mut header)?;
        if n == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }

    let mut stream = stream;
    let (method, path) = parse_request_line(&request_line);
    if method != "GET" && method != "HEAD" {
        write_response(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            "method not allowed",
        )?;
        return Ok(());
    }

    let (status, content_type, body) = route(path, state);
    if method == "HEAD" {
        write_response(&mut stream, status, content_type, "")?;
    } else {
        write_response(&mut stream, status, content_type, body.as_str())?;
    }
    Ok(())
}

fn parse_request_line(line: &str) -> (&str, &str) {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let mut parts = trimmed.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    (method, path)
}

fn route(path: &str, state: &ServerState) -> (u16, &'static str, String) {
    match path {
        "/" | "/index.html" => (
            200,
            "text/html; charset=utf-8",
            render_full_html_bundle(&state.set, &state.project_label),
        ),
        "/index.md" => (
            200,
            "text/markdown; charset=utf-8",
            render_full_markdown_bundle(&state.set, &state.project_label),
        ),
        p if p.starts_with("/object/") => {
            let tail = &p[8..];
            let (id, kind) = match tail.rsplit_once('.') {
                Some((id, ext @ ("html" | "md"))) => (id, ext),
                _ => {
                    return (
                        404,
                        "text/plain; charset=utf-8",
                        String::from("404 not found"),
                    );
                }
            };
            let Some(obj) = find_object(&state.set, id) else {
                return (
                    404,
                    "text/plain; charset=utf-8",
                    format!("404 not found: object_id `{id}`"),
                );
            };
            match kind {
                "html" => (200, "text/html; charset=utf-8", render_object_html(obj)),
                _ => (
                    200,
                    "text/markdown; charset=utf-8",
                    render_object_markdown(obj),
                ),
            }
        }
        _ => (
            404,
            "text/plain; charset=utf-8",
            String::from("404 not found"),
        ),
    }
}

fn find_object<'a>(set: &'a DocSet, object_id: &str) -> Option<&'a ObjectDoc> {
    set.objects.iter().find(|o| o.object_id == object_id)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let phrase = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let body_bytes = body.as_bytes();
    let header = format!(
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        len = body_bytes.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body_bytes)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::thread;

    fn fixture_set() -> DocSet {
        DocSet {
            objects: vec![
                ObjectDoc {
                    object_id: "billing.invoices".into(),
                    name: "INVOICES".into(),
                    kind: "table".into(),
                    summary: Some("Invoice rows".into()),
                    comments: vec![],
                    source_span: None,
                },
                ObjectDoc {
                    object_id: "billing.invoices_pkg".into(),
                    name: "INVOICES_PKG".into(),
                    kind: "package".into(),
                    summary: Some("Invoice API".into()),
                    comments: vec![],
                    source_span: None,
                },
            ],
        }
    }

    fn fetch(addr: SocketAddr, path: &str) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        s.write_all(request.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn route_index_returns_full_html_bundle() {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let body = fetch(addr, "/");
        assert!(body.contains("200 OK"));
        assert!(body.contains("text/html"));
        assert!(body.contains("billing object index"));
    }

    #[test]
    fn route_index_md_returns_markdown_bundle() {
        // Spawn the server in a worker thread, return the bound addr
        // synchronously to the test via a channel.
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let body = fetch(addr, "/index.md");
        assert!(body.starts_with("HTTP/1.1 200 OK"));
        assert!(body.contains("text/markdown"));
        assert!(body.contains("# billing object index"));
    }

    #[test]
    fn route_object_html_returns_per_object_page() {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let body = fetch(addr, "/object/billing.invoices_pkg.html");
        assert!(body.contains("200 OK"));
        assert!(body.contains("<h1>INVOICES_PKG</h1>"));
        assert!(body.contains("Connection: close"));
    }

    #[test]
    fn route_object_md_returns_per_object_markdown() {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let body = fetch(addr, "/object/billing.invoices.md");
        assert!(body.contains("200 OK"));
        assert!(body.contains("text/markdown"));
        assert!(body.contains("# INVOICES"));
    }

    #[test]
    fn route_unknown_path_returns_404() {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let body = fetch(addr, "/does-not-exist");
        assert!(body.contains("404"));
    }

    #[test]
    fn post_method_returns_405() {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();
            let state = ServerState {
                set: fixture_set(),
                project_label: "billing".into(),
            };
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &state).unwrap();
        });
        let addr = addr_rx.recv().unwrap();
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut body = String::new();
        s.read_to_string(&mut body).unwrap();
        assert!(body.contains("405"));
        assert!(body.contains("method not allowed"));
    }

    #[test]
    fn parse_request_line_handles_common_shapes() {
        assert_eq!(parse_request_line("GET /foo HTTP/1.1\r\n"), ("GET", "/foo"));
        assert_eq!(parse_request_line("HEAD /\r\n"), ("HEAD", "/"));
        assert_eq!(parse_request_line(""), ("", "/"));
    }

    // ── Bind-safety guard ─────────────────────────────────────────────────
    //
    // The preview server has no authentication; the guard mirrors
    // `plsql-mcp::tcp::parse_listen_target` so the two TCP-serving crates
    // in this workspace agree on what counts as a "local" bind. The
    // adversarial address set below is kept identical to the set the
    // plsql-mcp tcp tests run against, so neither crate can quietly
    // drift looser than the other.

    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn is_safe_bind_accepts_loopback_v4() {
        assert!(is_safe_bind(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn is_safe_bind_accepts_loopback_v6() {
        assert!(is_safe_bind(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn is_safe_bind_rejects_rfc1918_private() {
        // Mirrors plsql-mcp::tcp::private_rfc1918_refused_without_override:
        // shared LAN / VPC / container network co-residents must not
        // reach the unauthenticated preview surface by default.
        for ip in [
            IpAddr::V4(Ipv4Addr::new(10, 0, 1, 5)),
            IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)),
        ] {
            assert!(
                !is_safe_bind(ip),
                "RFC1918 {ip} must NOT be safe by default"
            );
        }
    }

    #[test]
    fn is_safe_bind_rejects_link_local_v4() {
        assert!(!is_safe_bind(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn is_safe_bind_rejects_unique_local_v6() {
        assert!(!is_safe_bind(IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn is_safe_bind_rejects_link_local_v6() {
        assert!(!is_safe_bind(IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn is_safe_bind_rejects_unspecified_v4() {
        assert!(!is_safe_bind(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }

    #[test]
    fn is_safe_bind_rejects_unspecified_v6() {
        assert!(!is_safe_bind(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[test]
    fn is_safe_bind_rejects_routable_public_v4() {
        assert!(!is_safe_bind(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn guard_bind_refuses_rfc1918_without_override() {
        for raw in [
            ("10.0.1.5", 9000u16),
            ("172.16.0.1", 9000),
            ("192.168.1.10", 9000),
        ] {
            let err = guard_bind(raw, false)
                .unwrap_err_or_else_msg(|| format!("{raw:?} must be refused without override"));
            assert!(
                matches!(err, BindGuardError::PublicBindRefused(_)),
                "{raw:?} must be PublicBindRefused, got {err:?}"
            );
        }
    }

    // Tiny in-test helper: clearer panic than `.unwrap_err()` when
    // the inner Result was unexpectedly Ok. Kept local to the test
    // module so it never leaks into the public API.
    trait UnwrapErrOrElse<T, E> {
        fn unwrap_err_or_else_msg(self, msg: impl FnOnce() -> String) -> E;
    }
    impl<T: std::fmt::Debug, E> UnwrapErrOrElse<T, E> for Result<T, E> {
        fn unwrap_err_or_else_msg(self, msg: impl FnOnce() -> String) -> E {
            match self {
                Err(e) => e,
                Ok(v) => panic!("{} (got Ok({v:?}))", msg()),
            }
        }
    }

    #[test]
    fn guard_bind_refuses_link_local_v4_without_override() {
        let err = guard_bind(("169.254.1.1", 9000u16), false).unwrap_err();
        assert!(matches!(err, BindGuardError::PublicBindRefused(_)));
    }

    #[test]
    fn guard_bind_refuses_unique_local_v6_without_override() {
        let err = guard_bind(("fc00::1", 9000u16), false).unwrap_err();
        assert!(matches!(err, BindGuardError::PublicBindRefused(_)));
    }

    #[test]
    fn guard_bind_allows_rfc1918_and_link_local_with_override() {
        // Symmetric with plsql-mcp::tcp::rfc1918_and_link_local_bind_with_override.
        // We resolve only; we do not bind, so the address need not be
        // assigned to this host.
        for raw in [
            ("10.0.1.5", 9000u16),
            ("172.16.0.1", 9000),
            ("192.168.1.10", 9000),
            ("169.254.1.1", 9000),
            ("fc00::1", 9000),
        ] {
            let bound = guard_bind(raw, true)
                .unwrap_or_else(|e| panic!("{raw:?} should resolve with override: {e}"));
            assert!(!bound.ip().is_loopback());
        }
    }

    #[test]
    fn guard_bind_allows_loopback_without_override() {
        let bound = guard_bind(("127.0.0.1", 0u16), false).unwrap();
        assert!(bound.ip().is_loopback());
    }

    #[test]
    fn guard_bind_refuses_unspecified_address_without_override() {
        let err = guard_bind(("0.0.0.0", 8080u16), false).unwrap_err();
        assert!(
            matches!(err, BindGuardError::PublicBindRefused(_)),
            "0.0.0.0 must be refused by default, got {err:?}"
        );
    }

    #[test]
    fn guard_bind_refuses_routable_public_address_without_override() {
        let err = guard_bind(("8.8.8.8", 8080u16), false).unwrap_err();
        assert!(matches!(err, BindGuardError::PublicBindRefused(_)));
    }

    #[test]
    fn guard_bind_allows_public_address_with_override() {
        let bound = guard_bind(("0.0.0.0", 8080u16), true).unwrap();
        assert!(bound.ip().is_unspecified());
    }

    #[test]
    fn bind_guard_error_converts_to_permission_denied_io_error() {
        let io: std::io::Error =
            BindGuardError::PublicBindRefused(SocketAddr::from(([0, 0, 0, 0], 8080))).into();
        assert_eq!(io.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn serve_preview_blocking_refuses_public_bind_by_default() {
        // The behavioural contract: a public bind through the real
        // entry point fails with a PermissionDenied error *before*
        // any socket is opened — never silently exposed.
        let err =
            serve_preview_blocking(("0.0.0.0", 0u16), fixture_set(), "billing", false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            err.to_string().contains("public-internet"),
            "error should explain the refusal: {err}"
        );
    }
}
