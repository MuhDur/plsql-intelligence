//! MCP transport taxonomy.
//!
//! `plsql-mcp` speaks the Model Context Protocol over **stdio** by default,
//! matching every current MCP client (Cursor, Claude Desktop, Codex CLI,
//! Windsurf). An optional **TCP** transport (`serve --listen <host:port>`)
//! serves remote agent sessions; its accept loop lives in [`crate::tcp`].
//!
//! This module defines the transport kinds the serve path selects between.

use serde::{Deserialize, Serialize};

/// Which transport an MCP server instance is bound to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransportKind {
    /// Standard input / standard output — the default MCP transport.
    #[default]
    Stdio,
    /// A TCP socket bind — used by `serve --listen <host:port>` for remote
    /// agent sessions. The accept loop is [`crate::tcp::serve`].
    Tcp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_is_the_default_transport_kind() {
        assert_eq!(TransportKind::default(), TransportKind::Stdio);
    }

    #[test]
    fn transport_kind_round_trips_through_json() {
        for kind in [TransportKind::Stdio, TransportKind::Tcp] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let back: TransportKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, kind);
        }
    }
}
