//! Layering shim for fact emission (support).
//!
//! `plsql-symbols::DeclTable` is the real declaration source,
//! but `plsql-symbols` depends on `plsql-ir` — not the reverse.
//! To let `fact_emit` pull declarations without inverting the
//! crate layer order, the engine wiring layer implements this
//! tiny [`DeclLike`] trait over whatever declaration container it
//! holds. Keeps the dependency arrow pointing the right way.

use crate::DeclId;

/// Anything that can yield `(DeclId, logical_id)` pairs.
pub trait DeclLike {
    fn iter_decls(&self) -> Vec<(DeclId, String)>;
}
