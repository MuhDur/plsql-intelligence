#![forbid(unsafe_code)]
//! `plsql-accretion` — the USR (Uncertainty-Sourced Repair) loop
//! library (Layer 5).
//!
//! Turns the engine's honest-uncertainty exhaust (parse errors,
//! typed `UnknownReason`s, un-lowered DDL) into a self-healing
//! coverage flywheel. **Phase P1** ships here: stage [A] —
//! `GapRecord` capture from a live `AnalysisRun` — plus its
//! versioned robot-JSON envelope.
//!
//! Layering (R20): this crate depends *only* on `plsql-core`,
//! `plsql-engine`, and `plsql-output`. It is **never** a dependency
//! of any other crate — the USR loop is strictly one-directional so
//! the loop can never perturb the analysis pipeline it observes.
//!
//! The two hard gates, enforced from line one (a violation is a P1
//! FAIL):
//!
//! * **I-PRIVACY** — no estate byte in any persisted field.
//! * **I-DETERMINISM** — same run + commit → byte-identical output.
//!
//! All phases ship here: P1 capture, P2 MinFixture, P3
//! cluster/ledger, P4 gate, P5 proposer, **P6 land + §4 monotonic
//! accretion tripwire + the `[F']` provenanced quarantine** —
//! behind stable module boundaries, one-directional, R20-safe.

pub mod capture;
pub mod cluster;
pub mod fixture;
pub mod gap;
pub mod gate;
pub mod land;
pub mod ledger;
pub mod proposer;
pub mod tokscrub;

pub use capture::{
    capture_gaps, capture_gaps_with_commit, git_head_short, is_repairable, is_repairable_code,
};
pub use cluster::{
    DEFAULT_MAX_REPRESENTATIVES, GAP_CLUSTER_SCHEMA, GapCluster, GapClusterEnvelope, MinFixtureId,
    cluster_gaps, cluster_gaps_with, fixture_sizes_from_store,
};
pub use fixture::{
    DEFAULT_MAX_BYTES, MinFixture, build_min_fixture, minimize_estate_gaps, persist_min_fixture,
};
pub use gap::{
    GAP_RECORD_SCHEMA, GapIndex, GapRecord, GapRecordEnvelope, REPAIRABLE_CODES, RepairClass,
    estate_run_id, sha256_hex,
};
pub use gate::{
    GATE_SCRIPT_REL, GATE_SHA256_PATH, GATE_STAGES, GateError, GateOutcome, GateStageVerdict,
    PRIVACY_ABORT_EXIT, run_gate, verify_gate_sha,
};
pub use land::{
    LANDED_CORPUS_REL, LandError, LandFixture, LandReceipt, QuarantineRecord, StageEvidence,
    land_candidate, land_candidate_in, landed_commit_anchor, persist_quarantine,
};
pub use ledger::{
    ACCRETION_LEDGER_FILENAME, AccretionIndex, AccretionLedger, AccretionLedgerEntry,
    BenchmarkRecord, EntryId, GENESIS_PARENT, LEDGER_FILENAME, Ledger, LedgerBody, LedgerEntry,
    LedgerError, compute_accretion_index,
};
pub use proposer::{
    CANDIDATE_DIFF_SCHEMA, CandidateDiff, CannedBackend, CompletionBackend,
    DeterministicStubProposer, HonestyManifest, LlmProposer, PatchProposer, ProposerError,
    R20_PATH_PREFIXES, RegressionTest, SubprocessBackend, path_is_r20_safe,
};

use thiserror::Error;

/// Errors surfaced by the USR loop library. Kept minimal in P1
/// (capture is infallible by construction — it only reads
/// in-memory diagnostics); serialization failures are surfaced as a
/// typed variant so callers never `unwrap` a robot-JSON write.
#[derive(Debug, Error)]
pub enum AccretionError {
    /// Serializing a [`GapRecordEnvelope`] failed.
    #[error("gap-record serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),

    /// The original span source does not reproduce the target gap
    /// signature — there is nothing to minimise (P2 `fixture.rs`).
    #[error("min-fixture: original source does not reproduce the target signature")]
    NotReproducible,

    /// **I-PRIVACY fail-safe (absolute).** No scrubbed candidate
    /// could be proven privacy-clean while still reproducing the
    /// gap. The fixture is discarded and nothing is persisted —
    /// privacy beats coverage, always (spec §1 I-PRIVACY, §2.2).
    #[error(
        "min-fixture: privacy could not be proven for any reproducing candidate; fixture discarded (I-PRIVACY)"
    )]
    PrivacyUnprovable,

    /// Persisting a privacy-proven [`MinFixture`] to the `.usr/`
    /// store failed. The caller treats this as "not stored" and
    /// leaves `min_fixture_id = None` (honest, R13) — it never
    /// downgrades privacy.
    #[error("min-fixture: persist failed: {0}")]
    FixtureIo(String),
}
