//! Stage [A] — GAP CAPTURE (spec §2).
//!
//! Filters an [`AnalysisRun`]'s diagnostics for the repairable
//! classes and projects each into a provenance-only [`GapRecord`].
//! Cluster-free in P1: one record per qualifying diagnostic,
//! `occurrence_count = 1` (dedup/clustering is P3).
//!
//! This module *really* reads `run.diagnostics` — there is no stub.
//! It never reads source: `AnalysisRun` carries none, which is
//! precisely why I-PRIVACY holds structurally here (see
//! [`crate::gap`] docs).

use plsql_core::Diagnostic;
use plsql_engine::AnalysisRun;
use tracing::instrument;

use crate::gap::{GapRecord, REPAIRABLE_CODES, estate_run_id};

/// `true` iff this diagnostic is one the USR loop can repair:
/// a known structural code, **or** any diagnostic carrying a typed
/// `UnknownReason` (the semantic-gap signal, spec §2 line "[A]").
#[must_use]
#[instrument(level = "trace", skip(diag))]
pub fn is_repairable(diag: &Diagnostic) -> bool {
    REPAIRABLE_CODES.contains(&diag.code.as_str()) || !diag.unknown_reasons.is_empty()
}

/// `true` iff `code` is one of the known repairable structural
/// codes (the code-only half of [`is_repairable`]; used by the P2
/// estate-minimisation wiring to decide which records get a
/// MinFixture without re-holding the originating `Diagnostic`).
#[must_use]
#[instrument(level = "trace")]
pub fn is_repairable_code(code: &str) -> bool {
    REPAIRABLE_CODES.contains(&code)
}

/// Git HEAD short sha for capture provenance.
///
/// Read once, *outside* any persisted field's derivation, then
/// passed in by value so the captured records stay a pure function
/// of (run, commit). Falls back to `"unknown"` if git is
/// unavailable — never panics, never blocks determinism (the same
/// checkout always yields the same value).
#[must_use]
#[instrument(level = "trace")]
pub fn git_head_short() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Capture all repairable gaps from a run (spec stage [A]).
///
/// Deterministic: the `estate_run_id` is content-derived, the
/// commit is read once and threaded through, and the result is
/// returned in `run.diagnostics` order (the caller — or the
/// envelope — performs the canonical sort, so this stays a faithful
/// 1:1 projection for testing).
#[must_use]
#[instrument(level = "debug", skip(run))]
pub fn capture_gaps(run: &AnalysisRun) -> Vec<GapRecord> {
    capture_gaps_with_commit(run, &git_head_short())
}

/// Capture with an explicit commit string (pure; used by tests for
/// byte-determinism without shelling out to git).
#[must_use]
#[instrument(level = "debug", skip(run))]
pub fn capture_gaps_with_commit(run: &AnalysisRun, commit: &str) -> Vec<GapRecord> {
    let run_id = estate_run_id(run);
    run.diagnostics
        .iter()
        .filter(|d| is_repairable(d))
        .map(|d| GapRecord::from_diagnostic(d, &run_id, commit))
        .collect()
}
