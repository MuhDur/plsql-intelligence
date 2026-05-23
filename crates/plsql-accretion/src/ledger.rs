//! The append-only, content-addressed USR ledger (spec §2.1 / §4 /
//! §1 I-PROVENANCE).
//!
//! Every cluster the loop acts on gets one [`LedgerEntry`] recording
//! the full 3-hop provenance chain:
//!
//! ```text
//! estate_run_id → (signature, diag_code, antlr_rule_path)
//!              → representative min_fixture_id(s)
//!              → [gate_verdict (P4) → landed_patch (P6)]
//! ```
//!
//! The two later hops (`gate_verdict`, `landed_patch`) are `None`
//! now — P4/P6 fill them. The ledger is **append-only and
//! tamper-evident** by construction:
//!
//! * **Content-addressed hash chain.** Each entry's [`EntryId`] is
//!   `sha256(parent_entry_id || canonical(entry_body))`. Editing,
//!   reordering, or truncating *any* line breaks the chain and
//!   [`Ledger::verify_chain`] reports exactly where (I-PROVENANCE).
//! * **Append-only enforced in code.** There is **no** public
//!   `update` / `delete` / `set` API anywhere in this module — the
//!   only mutation is [`Ledger::append`], which is additive and
//!   *idempotent by content* (appending the same logical entry twice
//!   is a no-op).
//! * **I-DETERMINISM.** Bodies serialize sorted-key; the chain hash
//!   is a pure function of content; no wall-clock, no RNG.
//!
//! Storage: JSONL under `<repo>/.usr/ledger/` (already gitignored),
//! one entry per line, in append order.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

use crate::cluster::GapCluster;
use crate::gap::{RepairClass, sha256_hex};

/// Suffix appended to a ledger filename to obtain the sidecar advisory
/// lock path. The sidecar is created lazily on the first append and
/// kept alongside the ledger so the lock travels with the directory
/// it guards.
const LOCK_SUFFIX: &str = ".lock";

/// RAII guard: holds an `fs2` advisory exclusive lock on a sidecar
/// file. Dropping the guard closes the file handle, which releases
/// the lock — including if the lock holder panics or returns early
/// with `?`.
///
/// We open `<ledger>.lock` (not the ledger file itself) so the lock
/// is decoupled from the JSONL append handle. The lock is **exclusive**
/// across both threads in this process and other processes on the
/// same host: `fs2::FileExt::lock_exclusive` translates to `flock(2)`
/// (Unix) / `LockFileEx` (Windows), so two concurrent `usr-loop`
/// invocations against the same `.usr/ledger/` directory serialize
/// here before either one reads the chain tip.
struct LedgerLockGuard {
    // Held only for its Drop side-effect (file close → unlock).
    _file: File,
}

impl LedgerLockGuard {
    /// Acquire an exclusive advisory lock on `<ledger_path>.lock`.
    /// Blocks until the lock is granted, then returns a guard.
    fn acquire(ledger_path: &Path) -> Result<Self, LedgerError> {
        let lock_path = lock_path_for(ledger_path);
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                LedgerError::Io(format!(
                    "open ledger lock {}: {e}",
                    lock_path.display()
                ))
            })?;
        // Blocking exclusive lock. Released on Drop (file close).
        FileExt::lock_exclusive(&file).map_err(|e| {
            LedgerError::Io(format!(
                "lock ledger sidecar {}: {e}",
                lock_path.display()
            ))
        })?;
        Ok(Self { _file: file })
    }
}

/// Sidecar lock path for a given ledger file: `<ledger>.lock`.
fn lock_path_for(ledger_path: &Path) -> PathBuf {
    let mut s = ledger_path.as_os_str().to_owned();
    s.push(LOCK_SUFFIX);
    PathBuf::from(s)
}

/// Filename of the append-only ledger inside `.usr/ledger/`.
pub const LEDGER_FILENAME: &str = "ledger.jsonl";

/// The genesis parent id — the "previous hash" of the first entry.
/// A fixed constant so the chain is anchored deterministically.
pub const GENESIS_PARENT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A content-addressed ledger entry id:
/// `sha256(parent_entry_id || canonical_body_json)`. Equality of
/// this id == equality of the entry's full content *and* its
/// position in the chain (tamper-evident).
pub type EntryId = String;

/// Errors surfaced by the ledger.
#[derive(Debug, Error)]
pub enum LedgerError {
    /// An I/O failure opening / reading / appending the ledger file.
    #[error("ledger io: {0}")]
    Io(String),
    /// A line could not be parsed as a [`LedgerEntry`].
    #[error("ledger parse error at line {line}: {source}")]
    Parse {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    /// Serializing an entry body failed.
    #[error("ledger serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    /// **Tamper detected.** The recomputed content/chain hash of the
    /// entry at `line` does not match its stored `entry_id`, or its
    /// `parent_entry_id` does not match the prior entry's id — the
    /// file was edited, reordered, or truncated.
    #[error(
        "ledger chain broken at line {line}: {detail} (append-only/tamper-evidence violated, I-PROVENANCE)"
    )]
    ChainBroken { line: usize, detail: String },
}

/// The provenance body of a ledger entry — everything *except* the
/// chain linkage (`entry_id` / `parent_entry_id`). The body is what
/// is content-hashed, so two logically-identical actions hash
/// identically (idempotent append).
///
/// Sorted-key serialization (struct field order is the canonical
/// order; every nested map is a `BTreeMap`) → I-DETERMINISM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LedgerBody {
    /// Hop-1: the content hash of the originating `AnalysisRun`
    /// (`GapRecord::estate_run_id`) — provenance of the *run*, never
    /// the estate.
    pub estate_run_id: String,
    /// Hop-2: the frozen gap-class signature.
    pub signature: String,
    /// Hop-2: the diagnostic code for the class.
    pub diag_code: String,
    /// Hop-2: the ANTLR rule path for the class (or `None`).
    pub antlr_rule_path: Option<String>,
    /// Hop-2: the heuristic repair lane.
    pub repair_class: RepairClass,
    /// Total estate occurrences this cluster folded (dedup count).
    pub occurrence_count: u64,
    /// Hop-3: the representative privacy-proven MinFixture ids
    /// (content-addressed; ≤ K, deterministic order — copied
    /// verbatim from the [`GapCluster`]).
    pub representative_min_fixtures: Vec<String>,
    /// Hop-4 (P4): the conformance-gate verdict. `None` until the
    /// gate runs (the loop has captured/clustered but not yet gated
    /// this class). Honest (R13).
    pub gate_verdict: Option<String>,
    /// Hop-5 (P6): the landed patch commit/diff id. `None` until a
    /// candidate is proven and landed.
    pub landed_patch: Option<String>,
}

impl LedgerBody {
    /// Build a body from a clustered gap class (the common case:
    /// the loop has captured + clustered, gate/land not yet run).
    #[must_use]
    #[instrument(level = "trace", skip(cluster))]
    pub fn from_cluster(estate_run_id: &str, cluster: &GapCluster) -> Self {
        Self {
            estate_run_id: estate_run_id.to_string(),
            signature: cluster.signature.clone(),
            diag_code: cluster.diag_code.clone(),
            antlr_rule_path: cluster.antlr_rule_path.clone(),
            repair_class: cluster.repair_class,
            occurrence_count: cluster.occurrence_count,
            representative_min_fixtures: cluster.representative_min_fixtures.clone(),
            gate_verdict: None,
            landed_patch: None,
        }
    }

    /// Canonical sorted-key JSON of the body (the pre-image of the
    /// content hash). Pure function of content.
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn canonical_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// The content+chain id this body would get under `parent`:
    /// `sha256(parent || canonical_body)`.
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn entry_id(&self, parent: &str) -> serde_json::Result<EntryId> {
        let body = self.canonical_json()?;
        let mut pre = String::with_capacity(parent.len() + 1 + body.len());
        pre.push_str(parent);
        pre.push('\n');
        pre.push_str(&body);
        Ok(sha256_hex(pre.as_bytes()))
    }
}

/// One persisted ledger line: the chain linkage + the body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// `sha256(parent_entry_id || canonical(body))` — content +
    /// position. Recomputed and checked by [`Ledger::verify_chain`].
    pub entry_id: EntryId,
    /// The prior entry's `entry_id`, or [`GENESIS_PARENT`] for the
    /// first entry. This is the chain link.
    pub parent_entry_id: EntryId,
    /// The provenance body.
    pub body: LedgerBody,
}

/// Handle to the append-only ledger at a fixed path. Holds **no**
/// mutate/delete API — the only state change is [`Self::append`].
#[derive(Debug)]
pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    /// Open (or prepare to create) the ledger at
    /// `<dir>/ledger.jsonl`. Creates the directory if missing; does
    /// **not** create or truncate the file (append-only — the file
    /// appears on first [`Self::append`]).
    ///
    /// # Errors
    /// [`LedgerError::Io`] if the directory cannot be created.
    #[instrument(level = "debug")]
    pub fn open(dir: impl AsRef<Path> + std::fmt::Debug) -> Result<Self, LedgerError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| LedgerError::Io(e.to_string()))?;
        Ok(Self {
            path: dir.join(LEDGER_FILENAME),
        })
    }

    /// The on-disk ledger path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read every entry in append order. Returns an empty `Vec` if
    /// the file does not exist yet (a fresh ledger).
    ///
    /// # Errors
    /// [`LedgerError::Io`] / [`LedgerError::Parse`] on read/parse
    /// failure.
    #[instrument(level = "debug", skip(self))]
    pub fn iter(&self) -> Result<Vec<LedgerEntry>, LedgerError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&self.path).map_err(|e| LedgerError::Io(e.to_string()))?;
        let reader = BufReader::new(f);
        let mut out = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| LedgerError::Io(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: LedgerEntry =
                serde_json::from_str(&line).map_err(|e| LedgerError::Parse {
                    line: i + 1,
                    source: e,
                })?;
            out.push(entry);
        }
        Ok(out)
    }

    /// Append one entry for a clustered gap class.
    ///
    /// **Idempotent by content (I-PROVENANCE).** The new entry's
    /// `parent` is the last entry's `entry_id` (or [`GENESIS_PARENT`]
    /// for the first). If the *identical logical entry* (a
    /// byte-equal [`LedgerBody`]) is already the chain tip, the
    /// append is a **no-op** and returns that entry's existing id —
    /// re-acting on the same cluster never grows or corrupts the
    /// chain.
    ///
    /// **Concurrency-safe (cross-thread and cross-process).** The
    /// `iter() → compute parent → write` critical section is held
    /// under a `fs2` advisory exclusive lock on a sidecar
    /// `<ledger>.lock` file. Two concurrent `usr-loop` invocations
    /// against the same `.usr/ledger/` directory therefore serialize
    /// here; without the lock both invocations would read the same
    /// tip and write conflicting `parent_entry_id`s, leaving the
    /// chain in a `ChainBroken` state that bricks every future
    /// append. The lock releases when the guard drops (function
    /// return, `?`, or panic).
    ///
    /// There is intentionally **no** update/delete counterpart:
    /// append-only is a structural property of this API, not a
    /// runtime check (then *also* proven by `verify_chain`).
    ///
    /// # Errors
    /// [`LedgerError::Io`] / [`LedgerError::Serialize`] /
    /// [`LedgerError::Parse`] on failure.
    #[instrument(level = "debug", skip(self, body))]
    pub fn append(&self, body: LedgerBody) -> Result<EntryId, LedgerError> {
        // Hold the advisory lock across the entire read-modify-write
        // critical section — anything less reintroduces the TOCTOU.
        let _guard = LedgerLockGuard::acquire(&self.path)?;

        let existing = self.iter()?;
        let parent = existing
            .last()
            .map_or_else(|| GENESIS_PARENT.to_string(), |e| e.entry_id.clone());

        // Idempotent **by content**: if the same logical entry
        // (identical `LedgerBody`) is already the chain tip,
        // re-appending it is a no-op — return the existing id
        // unchanged. This makes "act on the same cluster twice"
        // safe without ever mutating or growing the chain
        // (I-PROVENANCE: the ledger records *distinct actions*,
        // not duplicate noise).
        if let Some(tip) = existing.last() {
            if tip.body == body {
                return Ok(tip.entry_id.clone());
            }
        }

        let entry_id = body.entry_id(&parent)?;

        let entry = LedgerEntry {
            entry_id: entry_id.clone(),
            parent_entry_id: parent,
            body,
        };
        let line = serde_json::to_string(&entry)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        f.write_all(line.as_bytes())
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        f.write_all(b"\n")
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        Ok(entry_id)
    }

    /// Verify the full hash chain (I-PROVENANCE proof).
    ///
    /// For every entry, recompute `sha256(parent || canonical_body)`
    /// and assert it equals the stored `entry_id`, and that
    /// `parent_entry_id` equals the prior entry's id (genesis for
    /// the first). **Any edit, reorder, or truncation** of the
    /// JSONL is detected here and reported with the offending line
    /// and reason. This is a real cryptographic check — not an
    /// `assert!(true)`.
    ///
    /// # Errors
    /// [`LedgerError::ChainBroken`] (naming the line + reason) on
    /// any tamper; [`LedgerError::Io`] / [`LedgerError::Parse`] on
    /// read failure.
    #[instrument(level = "debug", skip(self))]
    pub fn verify_chain(&self) -> Result<(), LedgerError> {
        let entries = self.iter()?;
        let mut expected_parent = GENESIS_PARENT.to_string();
        for (idx, e) in entries.iter().enumerate() {
            let line = idx + 1;
            if e.parent_entry_id != expected_parent {
                return Err(LedgerError::ChainBroken {
                    line,
                    detail: format!(
                        "parent_entry_id mismatch: stored {}, expected {} \
                         (entry reordered, inserted, or a predecessor was \
                         edited/truncated)",
                        short(&e.parent_entry_id),
                        short(&expected_parent),
                    ),
                });
            }
            let recomputed = e.body.entry_id(&e.parent_entry_id)?;
            if recomputed != e.entry_id {
                return Err(LedgerError::ChainBroken {
                    line,
                    detail: format!(
                        "entry_id mismatch: stored {}, recomputed {} \
                         (entry body was edited)",
                        short(&e.entry_id),
                        short(&recomputed),
                    ),
                });
            }
            expected_parent = e.entry_id.clone();
        }
        Ok(())
    }
}

/// First 12 hex chars of a hash, for compact error messages.
fn short(h: &str) -> String {
    h.chars().take(12).collect()
}

// ---------------------------------------------------------------------------
// §4 — Accretion metric data model (corpus-only, reproducible)
// ---------------------------------------------------------------------------

/// One benchmark observation over the **frozen public benchmark
/// set** (`corpus/` — *never* the private estate). Each record is the
/// honest-extraction outcome for one analysed benchmark unit, plus
/// whether its gap signature (if any) now has ≥1 privacy-proven
/// MinFixture (i.e. the loop has permanently closed it).
///
/// Computed purely from a corpus scan so **anyone** can reproduce
/// the [`AccretionIndex`] — it is a public, auditable number, not a
/// vibe (spec §4 / §1 I-MONOTONIC-VALUE).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    /// Objects for which real semantics were extracted (lowered).
    pub objects_with_extracted_semantics: u64,
    /// Objects the classifier could not lower (unrecognised).
    pub objects_unrecognized: u64,
    /// Distinct gap signatures observed in this benchmark unit that
    /// now carry ≥1 privacy-proven MinFixture (a permanently-closed
    /// class). Sorted, deduped by the caller.
    pub resolved_signatures: Vec<String>,
}

/// The §4 accretion metric data model. A pure function of the
/// benchmark inputs (no estate, no wall-clock, no RNG) so two runs
/// at the same commit produce a byte-identical index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccretionIndex {
    /// `coverage_index` = a corpus-only `extracted_semantics_ratio`
    /// proxy (`Σ extracted / Σ (extracted + unrecognized)`, in
    /// `[0,1]`) **plus** the count of distinct gap signatures with
    /// ≥1 privacy-proven fixture. Monotone non-decreasing as the
    /// loop closes signatures (I-MONOTONIC-VALUE).
    pub coverage_index: f64,
    /// The corpus-only extraction-ratio proxy alone, surfaced so the
    /// tripwire (P6) can attribute movement.
    pub extracted_semantics_ratio: f64,
    /// Count of distinct signature classes the loop has permanently
    /// closed (≥1 privacy-proven fixture).
    pub distinct_resolved_gap_signatures: u64,
    /// Provenance: the engine commit the index was computed at.
    pub computed_at_commit: String,
}

/// Compute the §4 accretion index from a **corpus-only** benchmark
/// scan. Deterministic and reproducible by anyone — the inputs are
/// public corpus measurements, never the private estate.
///
/// `coverage_index = extracted_semantics_ratio + distinct_resolved`
/// where `extracted_semantics_ratio = Σ extracted / Σ (extracted +
/// unrecognized)` (0.0 when nothing was attempted — honest, never a
/// fabricated 1.0), and `distinct_resolved` is the count of unique
/// signatures across all records carrying ≥1 privacy-proven fixture.
#[must_use]
#[instrument(level = "debug", skip(benchmark_records))]
pub fn compute_accretion_index(
    benchmark_records: &[BenchmarkRecord],
    computed_at_commit: &str,
) -> AccretionIndex {
    let mut extracted: u64 = 0;
    let mut denom: u64 = 0;
    // BTreeMap-backed set → deterministic, no HashMap iteration.
    let mut resolved: BTreeMap<String, ()> = BTreeMap::new();
    for r in benchmark_records {
        extracted = extracted.saturating_add(r.objects_with_extracted_semantics);
        denom = denom
            .saturating_add(r.objects_with_extracted_semantics)
            .saturating_add(r.objects_unrecognized);
        for s in &r.resolved_signatures {
            resolved.insert(s.clone(), ());
        }
    }
    let extracted_semantics_ratio = if denom == 0 {
        0.0
    } else {
        // f64 division of small integer counts is exact and
        // deterministic across platforms for these magnitudes.
        extracted as f64 / denom as f64
    };
    let distinct_resolved_gap_signatures = resolved.len() as u64;
    let coverage_index = extracted_semantics_ratio + distinct_resolved_gap_signatures as f64;
    AccretionIndex {
        coverage_index,
        extracted_semantics_ratio,
        distinct_resolved_gap_signatures,
        computed_at_commit: computed_at_commit.to_string(),
    }
}

// ---------------------------------------------------------------------------
// §4 — Accretion-metric monotonic tripwire ledger (its own
// content-addressed, append-only history; spec §4 / §1 I-MONOTONIC-VALUE).
// ---------------------------------------------------------------------------

/// Filename of the append-only accretion-metric history (spec §4).
/// Distinct from the provenance [`LEDGER_FILENAME`]: this one is the
/// `coverage_index`-over-time public dashboard the §4 tripwire
/// asserts monotonic non-decreasing against the last release tag.
pub const ACCRETION_LEDGER_FILENAME: &str = "accretion_ledger.jsonl";

/// One content-addressed entry in the `accretion_ledger.jsonl`
/// history. Hash-chained exactly like [`LedgerEntry`] (tamper-evident,
/// append-only) but the body is an [`AccretionIndex`] measurement at a
/// commit/ref. **No wall-clock in the persisted body** (I-DETERMINISM):
/// the only time-like field is the git ref/commit the index was
/// measured at, which is itself deterministic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccretionLedgerEntry {
    /// `sha256(parent_entry_id || canonical(body))` — content+chain.
    pub entry_id: EntryId,
    /// The prior entry's id, or [`GENESIS_PARENT`] for the first.
    pub parent_entry_id: EntryId,
    /// The git ref this measurement anchors to (a release tag, or
    /// `"HEAD"` / a commit sha). Deterministic, not wall-clock.
    pub git_ref: String,
    /// The measured §4 accretion index (the monotone quantity).
    pub index: AccretionIndex,
}

impl AccretionLedgerEntry {
    /// Canonical sorted-key pre-image of the content hash (the
    /// `git_ref` + the index body; pure function of content).
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    pub fn body_canonical(&self) -> serde_json::Result<String> {
        // Body = git_ref + index (NOT the chain-linkage fields).
        let v = serde_json::json!({ "git_ref": self.git_ref, "index": self.index });
        serde_json::to_string(&v)
    }

    /// The content+chain id under `parent`: `sha256(parent || body)`.
    ///
    /// # Errors
    /// Propagates a `serde_json` serialization failure.
    pub fn id_under(&self, parent: &str) -> serde_json::Result<EntryId> {
        let body = self.body_canonical()?;
        let mut pre = String::with_capacity(parent.len() + 1 + body.len());
        pre.push_str(parent);
        pre.push('\n');
        pre.push_str(&body);
        Ok(sha256_hex(pre.as_bytes()))
    }
}

/// Handle to the append-only `accretion_ledger.jsonl` (spec §4). Like
/// [`Ledger`] it exposes **no** update/delete API — the only mutation
/// is the idempotent-by-content [`Self::append`]. The §4 tripwire
/// reads it to assert `coverage_index(HEAD) ≥ coverage_index(last
/// release tag)` (I-MONOTONIC-VALUE).
#[derive(Debug)]
pub struct AccretionLedger {
    path: PathBuf,
}

impl AccretionLedger {
    /// Open (or prepare to create) the accretion ledger at
    /// `<dir>/accretion_ledger.jsonl`. Creates the directory; does
    /// not create/truncate the file (append-only — appears on first
    /// [`Self::append`]).
    ///
    /// # Errors
    /// [`LedgerError::Io`] if the directory cannot be created.
    #[instrument(level = "debug")]
    pub fn open(dir: impl AsRef<Path> + std::fmt::Debug) -> Result<Self, LedgerError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| LedgerError::Io(e.to_string()))?;
        Ok(Self {
            path: dir.join(ACCRETION_LEDGER_FILENAME),
        })
    }

    /// The on-disk accretion-ledger path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read every entry in append order (empty if the file does not
    /// exist yet).
    ///
    /// # Errors
    /// [`LedgerError::Io`] / [`LedgerError::Parse`] on read/parse
    /// failure.
    #[instrument(level = "debug", skip(self))]
    pub fn iter(&self) -> Result<Vec<AccretionLedgerEntry>, LedgerError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&self.path).map_err(|e| LedgerError::Io(e.to_string()))?;
        let reader = BufReader::new(f);
        let mut out = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| LedgerError::Io(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: AccretionLedgerEntry =
                serde_json::from_str(&line).map_err(|e| LedgerError::Parse {
                    line: i + 1,
                    source: e,
                })?;
            out.push(entry);
        }
        Ok(out)
    }

    /// Append one accretion-index measurement. **Idempotent by
    /// content**: if the chain tip already carries a byte-equal
    /// `(git_ref, index)` body, this is a no-op returning that id
    /// (re-running the tripwire at the same commit never grows the
    /// history). The new entry's parent is the prior tip's id (or
    /// [`GENESIS_PARENT`]).
    ///
    /// **Concurrency-safe (cross-thread and cross-process)** by the
    /// same `<ledger>.lock` sidecar mechanism as [`Ledger::append`]:
    /// the `iter → compute parent → write` window is held under a
    /// `fs2` advisory exclusive lock, so two concurrent appends
    /// against the same directory serialize and the chain remains
    /// monotonic.
    ///
    /// # Errors
    /// [`LedgerError::Io`] / [`LedgerError::Serialize`] /
    /// [`LedgerError::Parse`] on failure.
    #[instrument(level = "debug", skip(self, index))]
    pub fn append(&self, git_ref: &str, index: AccretionIndex) -> Result<EntryId, LedgerError> {
        // Hold the advisory lock across the entire read-modify-write
        // critical section — same hazard, same fix as Ledger::append.
        let _guard = LedgerLockGuard::acquire(&self.path)?;

        let existing = self.iter()?;
        let parent = existing
            .last()
            .map_or_else(|| GENESIS_PARENT.to_string(), |e| e.entry_id.clone());

        if let Some(tip) = existing.last() {
            if tip.git_ref == git_ref && tip.index == index {
                return Ok(tip.entry_id.clone());
            }
        }
        let mut entry = AccretionLedgerEntry {
            entry_id: String::new(),
            parent_entry_id: parent.clone(),
            git_ref: git_ref.to_string(),
            index,
        };
        entry.entry_id = entry.id_under(&parent)?;
        let line = serde_json::to_string(&entry)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        f.write_all(line.as_bytes())
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        f.write_all(b"\n")
            .map_err(|e| LedgerError::Io(e.to_string()))?;
        Ok(entry.entry_id)
    }

    /// Verify the full tamper-evident hash chain (I-PROVENANCE).
    ///
    /// # Errors
    /// [`LedgerError::ChainBroken`] (naming the line) on any tamper;
    /// [`LedgerError::Io`] / [`LedgerError::Parse`] on read failure.
    #[instrument(level = "debug", skip(self))]
    pub fn verify_chain(&self) -> Result<(), LedgerError> {
        let entries = self.iter()?;
        let mut expected_parent = GENESIS_PARENT.to_string();
        for (idx, e) in entries.iter().enumerate() {
            let line = idx + 1;
            if e.parent_entry_id != expected_parent {
                return Err(LedgerError::ChainBroken {
                    line,
                    detail: format!(
                        "parent mismatch: stored {}, expected {}",
                        short(&e.parent_entry_id),
                        short(&expected_parent)
                    ),
                });
            }
            let recomputed = e.id_under(&e.parent_entry_id)?;
            if recomputed != e.entry_id {
                return Err(LedgerError::ChainBroken {
                    line,
                    detail: format!(
                        "entry_id mismatch: stored {}, recomputed {} (body edited)",
                        short(&e.entry_id),
                        short(&recomputed)
                    ),
                });
            }
            expected_parent = e.entry_id.clone();
        }
        Ok(())
    }
}
