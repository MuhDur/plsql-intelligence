//! Tool registry for the `plsql-mcp` server.
//!
//! The generic registry contract (`ToolTier`, `ToolDescriptor`, `ToolRegistry`)
//! was relocated to the engine-free `oraclemcp-core` crate (bead P0-0). This
//! module re-exports it so existing `crate::tools::*` paths resolve unchanged
//! during the in-place extraction — `plsql-mcp` is the engine-side product
//! binary that depends on the `oraclemcp-*` core.

pub use oraclemcp_core::tools::{ToolDescriptor, ToolRegistry, ToolTier};
