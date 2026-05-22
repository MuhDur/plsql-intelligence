//! Local HTTP preview server (`PLSQL-DOC-010`).
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

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;

use crate::{
    DocSet, ObjectDoc, render_full_html_bundle, render_full_markdown_bundle, render_object_html,
    render_object_markdown,
};

/// Run the preview server on `addr`, blocking the caller forever.
/// Returns only on a fatal `TcpListener::bind` error.
///
/// `set` is cloned into an `Arc` so the response handler can borrow it
/// without lifetimes leaking into the server signature. `project_label`
/// shows up in the index `<title>` and `<h1>`.
pub fn serve_preview_blocking<A: ToSocketAddrs>(
    addr: A,
    set: DocSet,
    project_label: impl Into<String>,
) -> std::io::Result<()> {
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
/// to terminate.
#[doc(hidden)]
pub fn serve_preview_for_n<A: ToSocketAddrs>(
    addr: A,
    set: DocSet,
    project_label: impl Into<String>,
    n: usize,
) -> std::io::Result<SocketAddr> {
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
}
