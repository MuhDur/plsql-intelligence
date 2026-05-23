//! Stage [C] — CLUSTER / DEDUP (spec §2 step [C]).
//!
//! `N` estate occurrences of the *same gap class* collapse to
//! exactly **one** [`GapCluster`]. The clustering key is the
//! **frozen, fine-grained, spec-conformant `signature`** computed in
//! [`crate::gap`] (`sha256(diag_code, antlr_rule_path,
//! token-kind-shape)`). This module **must not** re-derive, weaken,
//! or coarsen that signature — doing so would be the exact
//! anti-gaming failure the spec forbids. It only *groups by* the
//! value `gap.rs` already produced.
//!
//! Two hard invariants govern every line here:
//!
//! * **I-DETERMINISM** — same `&[GapRecord]` input → byte-identical
//!   `Vec<GapCluster>`. No `HashMap` iteration, no RNG, no
//!   wall-clock. Records are folded through a `BTreeMap` keyed by
//!   signature and every representative list is sorted by a total
//!   order (`min_fixture_id` then source size) before truncation.
//! * **I-PROVENANCE** — a cluster keeps enough provenance
//!   (`signature`, `diag_code`, `antlr_rule_path`, `repair_class`,
//!   `first_seen_commit`, and ≤ `K` representative MinFixture ids)
//!   to start hop-3 of the §1 I-PROVENANCE chain. A cluster with no
//!   privacy-proven fixture is still a *valid* cluster — its
//!   `representative_min_fixtures` is simply empty. That is honest
//!   (R13): the gap is real and counted, it just is not yet
//!   repairable.

use std::collections::BTreeMap;
use std::path::Path;

use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::gap::{GapRecord, RepairClass};

/// Default cap on the number of representative MinFixtures kept per
/// cluster (`K` in the spec). Small, deterministic, smallest-first.
pub const DEFAULT_MAX_REPRESENTATIVES: usize = 3;

/// Versioned robot-JSON schema for a batch of [`GapCluster`]s
/// (`plsql.usr.gap_cluster` v1). Mirrors the
/// [`plsql_output::SchemaDescriptor`] pattern used by every other
/// envelope in the workspace.
pub const GAP_CLUSTER_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.usr.gap_cluster",
    version: SchemaVersion::new(1, 0, 0),
    description: "USR-loop GapCluster batch — deduped gap classes, provenance only (PLSQL-USR-001)",
};

/// A content-addressed MinFixture id (the `sha256` of the synthetic
/// fixture source — exactly `GapRecord::min_fixture_id`).
pub type MinFixtureId = String;

/// One deduped gap *class* (spec §2 step [C]). All [`GapRecord`]s
/// sharing the frozen `signature` collapse to exactly one of these.
///
/// `Ord` so a batch is sorted deterministically (by `signature`)
/// before serialization (I-DETERMINISM).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GapCluster {
    /// The frozen, fine-grained gap-class signature (the dedup key).
    /// Byte-equal to every folded `GapRecord::signature`.
    pub signature: String,
    /// The diagnostic code for the class (constant across the
    /// cluster — it is an input to the signature).
    pub diag_code: String,
    /// The ANTLR rule path for the class (constant across the
    /// cluster — it is an input to the signature). `None` for the
    /// honest no-parse-tree case.
    pub antlr_rule_path: Option<String>,
    /// The heuristic repair lane for the class.
    pub repair_class: RepairClass,
    /// Total estate occurrences folded into this cluster — the sum
    /// of every member `GapRecord::occurrence_count`. This is the
    /// dedup proof: `sum(occurrence_count) == records.len()` when
    /// every input record had `occurrence_count == 1`.
    pub occurrence_count: u64,
    /// Up to `K` representative privacy-proven MinFixture ids,
    /// smallest-source-first, deterministic (sorted by
    /// `min_fixture_id` then source size). May be **empty** — a
    /// cluster with no privacy-proven fixture is still valid and
    /// honest; it just is not yet repairable.
    pub representative_min_fixtures: Vec<MinFixtureId>,
    /// The earliest `first_seen_commit` across folded records, by
    /// byte order (deterministic; provenance hop-1 anchor).
    pub first_seen_commit: String,
}

/// Internal accumulator: one per distinct signature. Holds the
/// representative candidates as `(min_fixture_id, size)` so the
/// final sort is "smallest-source-first, ties broken by id" with a
/// total, RNG-free order.
struct Acc {
    diag_code: String,
    antlr_rule_path: Option<String>,
    repair_class: RepairClass,
    occurrence_count: u64,
    first_seen_commit: String,
    /// `(min_fixture_id, source_size)` — deduped by id.
    fixtures: BTreeMap<MinFixtureId, u64>,
}

/// Cluster + dedup a slice of [`GapRecord`]s (spec §2 step [C]).
///
/// Records with the **same frozen `signature`** collapse to **one**
/// [`GapCluster`]:
///
/// * `occurrence_count` = sum of member `occurrence_count`s.
/// * `representative_min_fixtures` = the ≤ `max_representatives`
///   smallest distinct privacy-proven MinFixtures for that
///   signature, deterministic order (`min_fixture_id`, then source
///   size). A member contributes a representative only if it carries
///   **both** `min_fixture_id` *and* `privacy_proof_id` (privacy was
///   proven — I-PRIVACY). Members without a proven fixture still
///   count toward `occurrence_count`.
/// * `first_seen_commit` = the byte-minimum across members.
///
/// Distinct signatures stay distinct clusters (the fine-grained
/// signature is preserved — never re-derived or weakened here).
///
/// I-DETERMINISM: folding is through a `BTreeMap` keyed by
/// signature; the output `Vec` is signature-sorted; representative
/// ordering is a total order. Same input → byte-identical output.
///
/// `min_fixture_size` maps a MinFixture id to its scrubbed source
/// length in bytes (used only to order representatives
/// smallest-first). An id absent from the map is treated as size
/// `u64::MAX` (sorts last) — honest, never a panic.
#[must_use]
#[instrument(level = "debug", skip(records, min_fixture_size))]
pub fn cluster_gaps_with(
    records: &[GapRecord],
    max_representatives: usize,
    min_fixture_size: &BTreeMap<MinFixtureId, u64>,
) -> Vec<GapCluster> {
    let mut by_sig: BTreeMap<String, Acc> = BTreeMap::new();

    for r in records {
        let acc = by_sig.entry(r.signature.clone()).or_insert_with(|| Acc {
            diag_code: r.diag_code.clone(),
            antlr_rule_path: r.antlr_rule_path.clone(),
            repair_class: r.repair_class,
            occurrence_count: 0,
            first_seen_commit: r.first_seen_commit.clone(),
            fixtures: BTreeMap::new(),
        });
        acc.occurrence_count = acc.occurrence_count.saturating_add(r.occurrence_count);
        if r.first_seen_commit < acc.first_seen_commit {
            acc.first_seen_commit = r.first_seen_commit.clone();
        }
        // `repair_class` is NOT an input to the frozen `signature`
        // (the signature is `sha256(diag_code, antlr_rule_path,
        // token-kind-shape)` — see `gap.rs`). A single signature can
        // therefore fold records of *mixed* `repair_class` (e.g. the
        // same `IR_DDL_NOT_LOWERED` / `text_scan>create` shape, some
        // occurrences carrying a typed `UnknownReason` ⇒
        // `TypedDegradation`, some not ⇒ `Lowering`). Taking the
        // class from whichever record `or_insert_with` happened to
        // see *first* makes the cluster a function of
        // `run.diagnostics` iteration order — which is NOT stable
        // across engine recompiles (the §3 gate's
        // `cargo build --workspace` rebuilds the engine between the
        // two acceptance runs; codegen/inlining can reorder
        // diagnostic emission), so the persisted `target_cluster.json`
        // flipped `l`↔`d` run-to-run. Fold it with the same total
        // order used everywhere else (byte/`Ord`-minimum), making the
        // cluster a pure function of the *set* of folded records,
        // independent of their order (I-DETERMINISM, spec §1.4).
        if r.repair_class < acc.repair_class {
            acc.repair_class = r.repair_class;
        }
        // `diag_code` and `antlr_rule_path` *are* signature inputs so
        // they are constant across a signature's records — but fold
        // them order-independently too (byte-minimum) so the cluster
        // is provably a pure function of the record *set*, never the
        // iteration order, even if an upstream change ever loosened
        // that coupling (defense-in-depth for I-DETERMINISM).
        if r.diag_code < acc.diag_code {
            acc.diag_code = r.diag_code.clone();
        }
        if r.antlr_rule_path < acc.antlr_rule_path {
            acc.antlr_rule_path = r.antlr_rule_path.clone();
        }
        // A representative requires a *privacy-proven* fixture:
        // both the fixture id AND the redaction-delta proof id must
        // be present (I-PRIVACY — never surface an unproven id).
        if let (Some(fid), Some(_proof)) = (&r.min_fixture_id, &r.privacy_proof_id) {
            let size = min_fixture_size.get(fid).copied().unwrap_or(u64::MAX);
            acc.fixtures.insert(fid.clone(), size);
        }
    }

    by_sig
        .into_iter()
        .map(|(signature, acc)| {
            // Deterministic representative order: smallest source
            // first, ties broken by content id. `BTreeMap` already
            // dedups + orders by id; re-sort by (size, id) and cap.
            let mut reps: Vec<(MinFixtureId, u64)> = acc.fixtures.into_iter().collect();
            reps.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            reps.truncate(max_representatives);
            let representative_min_fixtures =
                reps.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
            GapCluster {
                signature,
                diag_code: acc.diag_code,
                antlr_rule_path: acc.antlr_rule_path,
                repair_class: acc.repair_class,
                occurrence_count: acc.occurrence_count,
                representative_min_fixtures,
                first_seen_commit: acc.first_seen_commit,
            }
        })
        .collect()
}

/// Cluster + dedup with the default `K` and no external size map
/// (representatives ordered by content id alone — still a total,
/// deterministic order). Convenience wrapper over
/// [`cluster_gaps_with`].
#[must_use]
#[instrument(level = "debug", skip(records))]
pub fn cluster_gaps(records: &[GapRecord]) -> Vec<GapCluster> {
    cluster_gaps_with(records, DEFAULT_MAX_REPRESENTATIVES, &BTreeMap::new())
}

/// Build the `min_fixture_id → source-byte-size` map by reading the
/// content-addressed fixture store at `<repo_root>/.usr/fixtures/`
/// (the same store [`crate::persist_min_fixture`] writes:
/// `<id>.sql`). Used only to order cluster representatives
/// smallest-source-first deterministically. Missing/unreadable
/// entries are simply absent (they sort last via the `u64::MAX`
/// fallback in [`cluster_gaps_with`]) — honest, never a panic.
#[must_use]
#[instrument(level = "debug")]
pub fn fixture_sizes_from_store(repo_root: &Path) -> BTreeMap<MinFixtureId, u64> {
    let dir = repo_root.join(".usr").join("fixtures");
    let mut sizes = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return sizes;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("sql") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(meta) = e.metadata() {
            sizes.insert(id.to_string(), meta.len());
        }
    }
    sizes
}

/// A versioned, sorted-key envelope wrapping a batch of
/// [`GapCluster`]s. The payload `Vec` is sorted (by `signature`)
/// before wrapping so two clusterings of the same input serialize
/// byte-identically (I-DETERMINISM).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GapClusterEnvelope {
    #[serde(flatten)]
    pub envelope: RobotJsonEnvelope<Vec<GapCluster>>,
}

impl GapClusterEnvelope {
    /// Wrap a batch, sorting it canonically first (I-DETERMINISM).
    #[must_use]
    #[instrument(level = "trace", skip(clusters))]
    pub fn new(mut clusters: Vec<GapCluster>) -> Self {
        clusters.sort();
        Self {
            envelope: RobotJsonEnvelope::new(GAP_CLUSTER_SCHEMA, clusters),
        }
    }

    /// `true` iff this envelope carries the `plsql.usr.gap_cluster`
    /// v1 schema.
    #[must_use]
    #[instrument(level = "trace", skip(self))]
    pub fn is_gap_cluster_schema(&self) -> bool {
        self.envelope.matches_schema(GAP_CLUSTER_SCHEMA)
    }

    /// Canonical single-line robot-JSON.
    ///
    /// # Errors
    /// Propagates any `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_robot_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Pretty multi-line robot-JSON (human mode).
    ///
    /// # Errors
    /// Propagates any `serde_json` serialization failure.
    #[instrument(level = "trace", skip(self))]
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}
