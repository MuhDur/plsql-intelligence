//! Bounded-depth guard for the re-lowering walks.
//!
//! Both [`crate::extract_call_sites`] and
//! [`crate::extract_table_accesses`] descend through control-flow
//! statements (`IF` / `LOOP` / nested block) by *re-lowering* the
//! captured raw body text and recursing into the result. This is
//! sound only while the captured slice **strictly shrinks** on each
//! pass. On a malformed / parser-recovered unit (e.g. an `IF` whose
//! `END IF` is missing, so the text-scanner's `rfind("END IF")`
//! falls back to `text.len()` and the arm body re-captures almost
//! the whole input) the slice fails to shrink and the mutual
//! recursion is unbounded — a real stack-overflow / SIGABRT seen on
//! the bundled public fixture `corpus/synthetic/l1`.
//!
//! The guard caps recursion depth. Hitting the cap is **not**
//! silently swallowed: the walk records that it degraded a nested
//! body, the caller surfaces a typed
//! [`plsql_core::UnknownReason::AnalysisRecursionLimit`] +
//! `Diagnostic` with provenance, and the rest of the analysis
//! continues (R13: honest degradation, never crash, never hide
//! uncertainty — the anti-pattern is *not* to cap
//! silently).

/// Maximum re-lowering recursion depth. Real well-formed PL/SQL
/// nests far below this (the deepest private-estate control-flow body
/// re-lowered is < 30 levels); the cap exists only to make a
/// non-shrinking malformed slice terminate. Chosen high enough
/// that it never clips genuine extraction on well-formed input
/// and low enough that 128 stack frames of the walk cannot
/// overflow the default 8 MiB main-thread stack.
pub const MAX_RELOWER_DEPTH: usize = 128;

/// Outcome of a bounded re-lowering walk: whether the depth cap
/// was hit (so the caller can emit a typed degradation) and how
/// many distinct nested bodies were truncated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecursionOutcome {
    /// `true` iff at least one nested body was abandoned because
    /// the depth cap was reached before it provably shrank.
    pub limit_hit: bool,
    /// Count of nested bodies degraded at the cap. Useful for the
    /// completeness report / diagnostic wording.
    pub truncated_bodies: usize,
}

impl RecursionOutcome {
    /// Fold a child walk's outcome into this one.
    pub fn absorb(&mut self, other: RecursionOutcome) {
        self.limit_hit |= other.limit_hit;
        self.truncated_bodies += other.truncated_bodies;
    }

    /// Record that a single nested body was abandoned at the cap.
    pub fn note_truncated(&mut self) {
        self.limit_hit = true;
        self.truncated_bodies += 1;
    }
}
