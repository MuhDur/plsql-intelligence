//! Runtime configuration for `plsql-mcp`.
//!
//! Bead adds the data shape only; loaders that read
//! `~/.plsql-mcp/connections.toml` arrive with.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::safety::SafetyProfile;

/// Top-level configuration consumed by the `plsql-mcp` binary.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpConfig {
    /// Transport-layer settings. Defaults to stdio.
    pub transport: TransportConfig,
    /// Active safety profile (defaults to `static_only` when the
    /// `live-db` feature is disabled, `inspect_only` otherwise).
    pub safety: SafetyProfile,
    /// Path to the connections file, if loaded.
    pub connections_path: Option<PathBuf>,
}

/// Transport configuration. `plsql-mcp` defaults to stdio; the optional TCP
/// transport is reserved for `--listen 127.0.0.1:<port>` (§13A.3).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportConfig {
    #[default]
    Stdio,
    Tcp {
        listen: String,
    },
}
