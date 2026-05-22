//! Tool registry for the `plsql-mcp` server.
//!
//! Every tool the server advertises over MCP is a [`ToolDescriptor`]
//! in a [`ToolRegistry`]. [`crate::default_tool_registry`] assembles
//! the canonical surface; the per-module `register_*` helpers each
//! contribute their slice of tools.

use serde::{Deserialize, Serialize};

/// Tier of a registered tool — informs the safety-profile gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    /// Static-analysis tool — operates on source, dependency graphs,
    /// and catalog snapshots only. Available regardless of safety
    /// profile; never touches a live database.
    FoundationStatic,
    /// Live-DB tool — gated by the `live-db` feature and a safety
    /// profile that allows the operation.
    FoundationLiveDb,
}

/// Stable, machine-readable identifier for a registered tool.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub tier: ToolTier,
    pub summary: String,
}

/// Minimal registry that per-tool beads populate.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolRegistry {
    pub tools: Vec<ToolDescriptor>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ToolDescriptor) {
        if !self.tools.iter().any(|t| t.name == descriptor.name) {
            self.tools.push(descriptor);
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_starts_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_deduplicates_by_name() {
        let mut registry = ToolRegistry::new();
        let tool = ToolDescriptor {
            name: String::from("describe_table"),
            tier: ToolTier::FoundationLiveDb,
            summary: String::from("Describe a table's columns and constraints"),
        };
        registry.register(tool.clone());
        registry.register(tool);
        assert_eq!(registry.len(), 1);
    }
}
