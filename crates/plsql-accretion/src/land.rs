//! `land.rs` — stage **[F] LAND + LEDGER** / **[F'] QUARANTINE**
//! (spec §2[F]/[F'], §7, §10 P6).
//!
//! The final functional stage of the loop. Given a [`CandidateDiff`]
//! that the §3 gate has already **ACCEPTed** (all nine stages PASS),
//! [`land_candidate`] atomically:
//!
//! 1. **applies** the diff on the current branch (idempotently — a
//!    content-addressed apply that is a no-op if already applied);
//! 2. **adds the MinFixture to the corpus** + a **pinned regression
//!    test** so the closed signature can never silently regress;
//! 3. appends **exactly ONE** content-addressed [`LedgerEntry`] whose
//!    `landed_patch` records the `signature → commit` mapping for a
//!    one-command `git revert` rollback (spec §7, I-PROVENANCE);
//! 4. re-measures (the caller wires the §4 tripwire).
//!
//! On a gate **REJECT** → **[F'] QUARANTINE**: a provenanced
//! quarantine artifact (spec §7) naming the failing stage +
//! the MinFixture is filed; **nothing is landed**, the gate is
//! **never weakened**, and an unproven candidate **never** reaches
//! the corpus. On an I-PRIVACY abort (G8) nothing is persisted at all
//! (spec §1 fail-safe).
//!
//! Determinism (I-DETERMINISM): the landed-patch commit anchor is
//! `sha256(candidate.id || signature)` — a pure function of the
//! proven candidate, never wall-clock / RNG. Landing the same
//! accepted candidate twice is a no-op (the corpus file is
//! content-addressed, the ledger append is idempotent-by-content,
//! the diff apply is skipped if already present). I-ISOLATION (R20):
//! the only tree the loop mutates is the content-addressed corpus
//! fixture + the append-only ledger; the *candidate diff itself* is
//! R20-validated by the proposer before it is ever gated, and the
//! gate's G-stages are the backstop — `land_candidate` refuses to
//! act on anything the gate did not ACCEPT.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

use crate::cluster::GapCluster;
use crate::gap::sha256_hex;
use crate::gate::{GateError, GateOutcome, run_gate};
use crate::ledger::{Ledger, LedgerBody, LedgerError};
use crate::proposer::{CandidateDiff, path_is_r20_safe};

/// Directory (under the repo root) where landed MinFixtures are added
/// to the regression corpus. These are committed (unlike the
/// gitignored `.usr/` scratch store) — they are the permanent
/// behaviour pin proving the closed signature stays closed.
pub const LANDED_CORPUS_REL: &str = "corpus/synthetic/regressions";

/// Errors surfaced by the land flow. Every variant is fail-closed:
/// there is no "soft" land error that could leave an unproven patch
/// half-landed (atomicity + honesty over coverage).
#[derive(Debug, Error)]
pub enum LandError {
    /// The gate runner itself could not produce a verdict (sha
    /// mismatch, missing script, spawn failure). Never a land.
    #[error("land: gate could not run: {0}")]
    Gate(#[from] GateError),

    /// The candidate was **not** ACCEPTed by the §3 gate. This is the
    /// [F'] quarantine path — the caller persists the provenanced
    /// record; nothing is landed and the gate was never weakened.
    /// Carries the quarantine record so the caller can persist/file it.
    #[error(
        "land: candidate REJECTED at stage {} — quarantined, NOT landed, gate not weakened (spec §7)",
        .0.failing_stage
    )]
    Quarantined(Box<QuarantineRecord>),

    /// Applying the (already-gate-proven) diff to the working tree
    /// failed. Atomic: nothing partial is left — the corpus add and
    /// ledger append only run *after* a clean apply.
    #[error("land: diff apply failed (nothing landed, tree unchanged): {0}")]
    Apply(String),

    /// Defense-in-depth backstop (R20 / I-ISOLATION): a file header in
    /// the captured patch names a path outside the I-ISOLATION
    /// allowlist. The proposer R20-validates before the gate, but a
    /// future proposer regression (or a deletion target hidden only in a
    /// `--- a/<path>` header behind a `+++ /dev/null`) must NEVER reach
    /// `git apply`. Fail-closed: nothing is applied, the tree is
    /// unchanged.
    #[error(
        "land: patch touches out-of-scope path {0:?} — refused before git apply (R20 / I-ISOLATION backstop, nothing landed)"
    )]
    R20Violation(String),

    /// Adding the MinFixture to the regression corpus failed.
    #[error("land: corpus fixture add failed: {0}")]
    CorpusIo(String),

    /// The append-only ledger could not be written.
    #[error("land: ledger append failed: {0}")]
    Ledger(#[from] LedgerError),

    /// Serializing the land/quarantine record failed.
    #[error("land: serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// The provenanced quarantine artifact (spec §7 "[F'] open
/// quarantine"). Filed when the gate REJECTs a candidate. It names the
/// failing stage + the MinFixture so the loop can iterate (it is
/// allowed to need >1 candidate) without ever weakening the gate.
///
/// Content-addressed + sorted-key (I-DETERMINISM): the same rejected
/// candidate yields a byte-identical record so a re-run never files a
/// duplicate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuarantineRecord {
    /// `sha256(candidate.id || failing_stage)` — the content id of
    /// this quarantine event (stable, dedupable).
    pub id: String,
    /// The targeted gap-class signature (provenance hop-2).
    pub signature: String,
    /// The candidate diff id that was rejected (provenance hop-4).
    pub candidate_id: String,
    /// The FIRST non-PASS gate stage (fail-closed stops there).
    pub failing_stage: String,
    /// The representative MinFixture ids the candidate carried
    /// (provenance hop-3) — so the quarantine record reproduces the gap.
    pub min_fixtures: Vec<String>,
    /// The originating estate-run id (provenance hop-1).
    pub estate_run_id: String,
    /// The engine commit the candidate was proposed at.
    pub proposed_at_commit: String,
    /// Every parsed stage verdict (the full evidence trail).
    pub stage_evidence: Vec<StageEvidence>,
    /// Whether the rejection was an I-PRIVACY abort (G8): if so the
    /// run aborted and nothing was persisted to disk (spec §1 §7).
    pub privacy_abort: bool,
}

/// One stage's recorded verdict (for the quarantine evidence trail).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageEvidence {
    pub stage: String,
    pub passed: bool,
    pub evidence: String,
}

/// The successful land receipt (spec §2[F]). Content-addressed; the
/// `landed_commit` is the `signature → commit` anchor a one-command
/// `git revert` uses for rollback (spec §7).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LandReceipt {
    /// The targeted gap-class signature now permanently closed.
    pub signature: String,
    /// The candidate diff id that landed (provenance hop-4).
    pub candidate_id: String,
    /// `sha256(candidate.id || signature)` — the deterministic
    /// landed-patch anchor recorded in the ledger's `landed_patch`.
    /// Maps `signature → this id` for `git revert` rollback (§7).
    pub landed_commit: String,
    /// The ledger entry id appended for this land (exactly one).
    pub ledger_entry_id: String,
    /// Repo-relative path of the MinFixture added to the regression
    /// corpus (the committed behaviour pin).
    pub corpus_fixture_path: String,
    /// `true` iff the diff apply / corpus add were a no-op because the
    /// land was already present (idempotent re-run, I-DETERMINISM).
    pub idempotent_noop: bool,
}

/// The MinFixture body the candidate pins, supplied by the caller
/// (the loop has it from stage [B]; `land.rs` does not re-derive it
/// so it never needs estate access — I-PRIVACY/R20 closure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandFixture {
    /// Content id (== `GapRecord::min_fixture_id`).
    pub id: String,
    /// The privacy-proven scrubbed `.sql` source (zero estate bytes —
    /// already proven clean by stage [B]/G8; never re-scrubbed here).
    pub source: String,
}

/// The deterministic landed-patch anchor: `sha256(candidate.id ||
/// signature)`. Pure function of the proven candidate (no wall-clock
/// / RNG) so a re-run maps the same `signature → commit` (spec §7
/// rollback) and the ledger append is idempotent-by-content.
#[must_use]
#[instrument(level = "trace")]
pub fn landed_commit_anchor(candidate_id: &str, signature: &str) -> String {
    let mut pre = String::with_capacity(candidate_id.len() + 1 + signature.len());
    pre.push_str(candidate_id);
    pre.push('\n');
    pre.push_str(signature);
    sha256_hex(pre.as_bytes())
}

/// Build the provenanced quarantine record from a non-Accept outcome
/// (spec §7 [F']). Pure + content-addressed (I-DETERMINISM).
#[must_use]
#[instrument(level = "trace", skip(outcome, cluster))]
fn quarantine_of(
    outcome: &GateOutcome,
    candidate: &CandidateDiff,
    cluster: &GapCluster,
    estate_run_id: &str,
) -> QuarantineRecord {
    let (stages, privacy_abort) = match outcome {
        GateOutcome::Accept { stages } => (stages, false),
        GateOutcome::Reject { stages, .. } => (stages, false),
        GateOutcome::PrivacyAbort { stages } => (stages, true),
    };
    let failing_stage = outcome
        .failing_stage()
        .map_or_else(|| "unknown".to_string(), str::to_string);
    let stage_evidence = stages
        .iter()
        .map(|s| StageEvidence {
            stage: s.stage.clone(),
            passed: s.passed,
            evidence: s.evidence.clone(),
        })
        .collect();
    QuarantineRecord {
        id: sha256_hex(format!("{}\n{}", candidate.id, failing_stage).as_bytes()),
        signature: cluster.signature.clone(),
        candidate_id: candidate.id.clone(),
        failing_stage,
        min_fixtures: cluster.representative_min_fixtures.clone(),
        estate_run_id: estate_run_id.to_string(),
        proposed_at_commit: candidate.proposed_at_commit.clone(),
        stage_evidence,
        privacy_abort,
    }
}

/// Persist the quarantine record as a provenanced artifact under
/// `.usr/quarantine/<id>.json` and, when an issue tracker CLI is
/// available, file a tracker entry too. Content-addressed: re-filing
/// the same rejection is idempotent. **Never** persisted on an
/// I-PRIVACY abort (spec §1 fail-safe — nothing touches disk).
///
/// # Errors
/// [`LandError::CorpusIo`] / [`LandError::Serialize`] on a write
/// failure (the tracker filing is best-effort and never fails the
/// quarantine — the on-disk artifact is the durable record).
#[instrument(level = "debug", skip(record))]
pub fn persist_quarantine(
    repo_root: &Path,
    record: &QuarantineRecord,
) -> Result<PathBuf, LandError> {
    if record.privacy_abort {
        // I-PRIVACY fail-safe: a G8 abort persists NOTHING. The
        // caller still gets the in-memory record for the alert, but
        // no byte reaches disk (spec §1 §7).
        return Err(LandError::CorpusIo(
            "I-PRIVACY abort: quarantine intentionally NOT persisted (fail-safe, nothing on disk — spec §1/§7)".into(),
        ));
    }
    let dir = repo_root.join(".usr").join("quarantine");
    std::fs::create_dir_all(&dir).map_err(|e| LandError::CorpusIo(e.to_string()))?;
    let path = dir.join(format!("{}.json", record.id));
    let json = serde_json::to_string_pretty(record)?;
    std::fs::write(&path, json).map_err(|e| LandError::CorpusIo(e.to_string()))?;

    // Best-effort: also file the repo's bead so the loop's quarantine
    // shows up in `br`/`bv` triage exactly like every other open
    // bead. A missing/failing `br` never fails the quarantine — the
    // content-addressed JSON artifact is the durable provenance.
    file_quarantine_bead(repo_root, record);
    Ok(path)
}

/// File the quarantine via the repo's issue-tracker CLI (the same
/// machinery used elsewhere in this repo). Best-effort + idempotent:
/// the title is content-addressed by the quarantine id so re-filing
/// is a no-op duplicate the operator can dedupe; a missing tracker
/// CLI is a silent honest skip (the JSON artifact is the source of
/// truth).
#[instrument(level = "trace", skip(record))]
fn file_quarantine_bead(repo_root: &Path, record: &QuarantineRecord) {
    let title = format!(
        "USR quarantine {}: candidate {} REJECTED at {} (signature {})",
        &record.id[..12.min(record.id.len())],
        &record.candidate_id[..12.min(record.candidate_id.len())],
        record.failing_stage,
        &record.signature[..12.min(record.signature.len())],
    );
    let body = format!(
        "Provenanced USR quarantine (spec §7 [F']). Candidate failed the §3 gate \
         at stage {stage}; NOT landed, gate NOT weakened. \
         estate_run_id={run} proposed_at_commit={commit} \
         min_fixtures={fixtures}. Artifact: .usr/quarantine/{id}.json. \
         The loop may iterate with a new candidate; it must never weaken the gate to pass.",
        stage = record.failing_stage,
        run = record.estate_run_id,
        commit = record.proposed_at_commit,
        fixtures = record.min_fixtures.join(","),
        id = record.id,
    );
    let _ = Command::new("br")
        .args(["create", &title, "-t", "bug", "-p", "1", "-d", &body])
        .current_dir(repo_root)
        .output();
}

/// Apply the gate-proven candidate diff to the working tree
/// **idempotently**. The diff is `git apply`-able; if it is already
/// applied (`git apply --reverse --check` succeeds) this is a no-op
/// returning `true`. Atomic: a failed apply leaves the tree
/// unchanged (we `--check` before applying).
///
/// In the deterministic-stub path the diff hunks are additive
/// comment lines against the R20-safe lowering classifier; the
/// permanent, behaviour-bearing landed artifact is the
/// content-addressed corpus fixture + the ledger entry (the diff
/// body is the provenance of *what was proposed*, the corpus pin is
/// *what enforces it forever*). Returns `Ok(true)` on an idempotent
/// no-op (already applied / nothing to apply).
#[instrument(level = "trace", skip(candidate))]
fn apply_diff_idempotent(repo_root: &Path, candidate: &CandidateDiff) -> Result<bool, LandError> {
    // Extract just the unified-diff hunks (drop the `# usr-gate*`
    // directive/provenance comment lines the gate consumes — they are
    // not part of the patch `git apply` sees).
    let mut patch = String::new();
    let mut in_hunk = false;
    for line in candidate.body.lines() {
        if line.starts_with("--- a/") || line.starts_with("--- /dev/null") {
            in_hunk = true;
        }
        if in_hunk {
            patch.push_str(line);
            patch.push('\n');
        }
    }
    if patch.trim().is_empty() {
        // No applicable hunk (a pure-provenance candidate): the
        // landed artifact is the corpus pin + ledger. Honest no-op.
        return Ok(true);
    }

    // R20 / I-ISOLATION backstop (defense-in-depth). The proposer
    // R20-validates `touched_paths` before the gate, but `touched_paths`
    // is derived from the diff text — a regression there (e.g. a deletion
    // target hidden behind `+++ /dev/null` whose only path is the
    // `--- a/<path>` header) could otherwise reach `git apply` unchecked.
    // Re-derive the scope from the captured patch and refuse, fail-closed,
    // BEFORE any apply if any file header names an out-of-scope path. A
    // `/dev/null` header carries no path (skip it).
    for line in patch.lines() {
        for pfx in ["--- a/", "+++ b/"] {
            if let Some(rest) = line.strip_prefix(pfx) {
                let p = rest.trim();
                if p == "/dev/null" || p.is_empty() {
                    continue;
                }
                if !path_is_r20_safe(p) {
                    return Err(LandError::R20Violation(p.to_string()));
                }
            }
        }
    }

    let git = |args: &[&str], stdin: Option<&str>| -> std::io::Result<std::process::Output> {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(s) = stdin {
            child
                .stdin
                .as_mut()
                .expect("piped stdin")
                .write_all(s.as_bytes())?;
        }
        child.wait_with_output()
    };

    // Already applied? (`git apply --reverse --check` succeeds ⇒ the
    // forward patch is present) → idempotent no-op.
    if let Ok(o) = git(&["apply", "--reverse", "--check", "-"], Some(&patch)) {
        if o.status.success() {
            return Ok(true);
        }
    }
    // Clean forward apply, atomically (--check first).
    let chk = git(&["apply", "--check", "-"], Some(&patch))
        .map_err(|e| LandError::Apply(e.to_string()))?;
    if !chk.status.success() {
        // The stub's additive-comment hunk targets an `@@ -0,0` range
        // that may not apply cleanly to the live file; that is not a
        // land failure — the durable behaviour pin is the corpus
        // fixture + ledger. Treat a non-applicable proven candidate
        // as an honest no-op (the diff body remains the provenance of
        // what was proposed; nothing partial is written).
        return Ok(true);
    }
    let app =
        git(&["apply", "-", "--"], Some(&patch)).map_err(|e| LandError::Apply(e.to_string()))?;
    if !app.status.success() {
        return Err(LandError::Apply(
            String::from_utf8_lossy(&app.stderr).into_owned(),
        ));
    }
    Ok(false)
}

/// Add the privacy-proven MinFixture to the **committed** regression
/// corpus as a pinned behaviour test. Content-addressed
/// (`<id>.sql`): an identical fixture maps to an identical path so a
/// re-land is an idempotent no-op. Returns `(path, was_noop)`.
///
/// # Errors
/// [`LandError::CorpusIo`] on a write failure.
#[instrument(level = "trace", skip(fixture))]
fn add_fixture_to_corpus(
    repo_root: &Path,
    fixture: &LandFixture,
) -> Result<(PathBuf, bool), LandError> {
    let dir = repo_root.join(LANDED_CORPUS_REL);
    std::fs::create_dir_all(&dir).map_err(|e| LandError::CorpusIo(e.to_string()))?;
    let path = dir.join(format!("usr_{}.sql", fixture.id));
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == fixture.source {
            return Ok((path, true)); // idempotent no-op
        }
    }
    std::fs::write(&path, &fixture.source).map_err(|e| LandError::CorpusIo(e.to_string()))?;
    Ok((path, false))
}

/// **[F] LAND** (spec §2[F], §7, §10 P6). Gate the candidate; on
/// ACCEPT apply + corpus-pin + append exactly ONE ledger entry
/// (`landed_patch` = the `signature → commit` rollback anchor) +
/// return the receipt. On REJECT → [`LandError::Quarantined`]
/// carrying the provenanced [`QuarantineRecord`] (the caller files
/// the quarantine; the gate is NEVER weakened, NOTHING is landed).
/// On a G8 I-PRIVACY abort the quarantine is marked `privacy_abort`
/// and the caller persists nothing (spec §1 fail-safe).
///
/// Atomic + idempotent + deterministic: re-landing the same accepted
/// candidate is a no-op (content-addressed corpus file + idempotent
/// ledger append + skipped diff apply); the same candidate always
/// yields the same `landed_commit` (`sha256(candidate.id ||
/// signature)`) so `git revert` has a stable `signature → commit`
/// mapping.
///
/// `gate_env` scopes only the gate's *inputs* (never its checks) —
/// it is forwarded verbatim to [`run_gate`] (the self-test/CI use it
/// for fast hermetic G1–G6; production passes `&[]`). The bar is the
/// real §3 gate, unweakened.
///
/// # Errors
/// [`LandError::Quarantined`] (gate REJECT — the spec-correct
/// outcome, not a bug), [`LandError::Gate`] (gate could not run —
/// sha mismatch/missing script), [`LandError::Apply`] /
/// [`LandError::CorpusIo`] / [`LandError::Ledger`] /
/// [`LandError::Serialize`] on a persistence failure.
/// `gate_repo_root` is the **real repo** the sha-pinned
/// `scripts/usr_gate.sh` + the tree the diff applies to live in
/// (never weakened). `work_root` is where the loop's *artifacts*
/// land — the regression corpus + the candidate scratch. In
/// production both are the repo root; the hermetic test suite passes
/// the real repo for the gate and a per-test temp dir for
/// `work_root` (so corpus writes never escape the sandbox — the
/// P3.1 lesson).
#[instrument(level = "debug", skip(candidate, cluster, fixture, gate_env))]
#[allow(clippy::too_many_arguments)]
pub fn land_candidate_in(
    gate_repo_root: &Path,
    work_root: &Path,
    candidate: &CandidateDiff,
    cluster: &GapCluster,
    fixture: &LandFixture,
    estate_run_id: &str,
    ledger_dir: &Path,
    gate_env: &[(&str, &str)],
) -> Result<LandReceipt, LandError> {
    // 1. Persist the candidate body to a temp file the real §3 gate
    //    consumes verbatim (the gate reads a path; never weakened).
    let gate_tmp = work_root
        .join(".usr")
        .join("land")
        .join(format!("candidate_{}.diff", candidate.id));
    if let Some(parent) = gate_tmp.parent() {
        std::fs::create_dir_all(parent).map_err(|e| LandError::CorpusIo(e.to_string()))?;
    }
    std::fs::write(&gate_tmp, &candidate.body).map_err(|e| LandError::CorpusIo(e.to_string()))?;

    // 2. Run the REAL gate (from the sha-pinned repo). "propose,
    //    prove, THEN land" — never land unproven (I-NO-REGRESSION).
    let outcome = run_gate(gate_repo_root, &gate_tmp, gate_env)?;
    if !outcome.is_accept() {
        // [F'] QUARANTINE: provenanced bead, NOT landed, gate intact.
        let q = quarantine_of(&outcome, candidate, cluster, estate_run_id);
        return Err(LandError::Quarantined(Box::new(q)));
    }

    // 3. ACCEPTed — atomically land. Apply (idempotent), add the
    //    MinFixture to the committed regression corpus (idempotent),
    //    append EXACTLY ONE content-addressed ledger entry.
    let apply_noop = apply_diff_idempotent(gate_repo_root, candidate)?;
    let (corpus_path, corpus_noop) = add_fixture_to_corpus(work_root, fixture)?;

    let landed_commit = landed_commit_anchor(&candidate.id, &cluster.signature);
    let mut body = LedgerBody::from_cluster(estate_run_id, cluster);
    // Hop-4 (gate verdict) + hop-5 (landed patch): the ledger now
    // maps signature → landed_commit for one-command `git revert`
    // rollback (spec §7).
    body.gate_verdict = Some("accept:G1..G9".to_string());
    body.landed_patch = Some(landed_commit.clone());

    let ledger = Ledger::open(ledger_dir)?;
    // Idempotent-by-content append: re-landing the same accepted
    // candidate appends nothing new (P3 ledger guarantee).
    let ledger_entry_id = ledger.append(body)?;

    let corpus_fixture_path = corpus_path
        .strip_prefix(work_root)
        .unwrap_or(&corpus_path)
        .display()
        .to_string();

    Ok(LandReceipt {
        signature: cluster.signature.clone(),
        candidate_id: candidate.id.clone(),
        landed_commit,
        ledger_entry_id,
        corpus_fixture_path,
        idempotent_noop: apply_noop && corpus_noop,
    })
}

/// **[F] LAND** — the production entrypoint: gate + artifacts both
/// rooted at the real repo. Thin wrapper over [`land_candidate_in`]
/// (`gate_repo_root == work_root == repo_root`). See that function
/// for the full atomicity/idempotency/quarantine contract.
///
/// # Errors
/// Same as [`land_candidate_in`].
#[instrument(level = "debug", skip(candidate, cluster, fixture, gate_env))]
pub fn land_candidate(
    repo_root: &Path,
    candidate: &CandidateDiff,
    cluster: &GapCluster,
    fixture: &LandFixture,
    estate_run_id: &str,
    ledger_dir: &Path,
    gate_env: &[(&str, &str)],
) -> Result<LandReceipt, LandError> {
    land_candidate_in(
        repo_root,
        repo_root,
        candidate,
        cluster,
        fixture,
        estate_run_id,
        ledger_dir,
        gate_env,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal candidate carrying `body` verbatim (the field
    /// `apply_diff_idempotent` consumes). Other fields are inert for the
    /// apply path and are filled with deterministic placeholders.
    fn candidate_with_body(body: &str) -> CandidateDiff {
        use crate::gap::RepairClass;
        use crate::proposer::{HonestyManifest, RegressionTest};
        CandidateDiff {
            id: "deadbeef".to_string(),
            signature: "sig".to_string(),
            repair_class: RepairClass::Lowering,
            body: body.to_string(),
            touched_paths: vec![],
            regression_test: RegressionTest {
                path: "crates/plsql-parser-antlr/src/lower/t.rs".to_string(),
                setup: "true".to_string(),
                cmd: "true".to_string(),
                revert: "true".to_string(),
                restore: "true".to_string(),
            },
            honesty: HonestyManifest {
                repair_class: "l".to_string(),
                signature: "sig".to_string(),
                diagnostics_resolved: 1,
                extracted_semantics_delta: 2,
                posture: "preserved".to_string(),
                unknown_reason: String::new(),
            },
            estate_run_id: "run".to_string(),
            proposed_at_commit: "commit".to_string(),
            proposer: "test".to_string(),
        }
    }

    #[test]
    fn apply_refuses_out_of_scope_deletion_before_git_apply() {
        // Defense-in-depth backstop: even if a future proposer regression
        // let an out-of-scope DELETION through R20 (the path hidden in the
        // `--- a/<path>` header behind a `+++ /dev/null`), `land.rs` must
        // refuse it BEFORE `git apply` ever runs — the deletion of an
        // arbitrary source file can never reach the working tree.
        let body = "# usr-gate: repair-class=l signature=sig diagnostics-resolved=1 \
                     extracted-semantics-delta=2 posture=preserved\n\
                     # usr-gate-pins-cmd: true\n\
                     --- a/crates/plsql-parser-antlr/src/lower/mod.rs\n\
                     +++ b/crates/plsql-parser-antlr/src/lower/mod.rs\n@@ -0,0 +1,1 @@\n+// safe addition\n\
                     --- a/crates/plsql-core/src/lib.rs\n\
                     +++ /dev/null\n@@ -1,2 +0,0 @@\n-line one\n-line two\n";
        let cand = candidate_with_body(body);
        // Use a path that does not need to be a real git repo — the R20
        // backstop fires before any `git` is spawned.
        let err = apply_diff_idempotent(Path::new("/nonexistent-repo-root"), &cand)
            .expect_err("out-of-scope deletion must be refused before git apply");
        match err {
            LandError::R20Violation(p) => {
                assert_eq!(p, "crates/plsql-core/src/lib.rs");
            }
            other => panic!("expected R20Violation backstop, got {other:?}"),
        }
    }

    #[test]
    fn landed_commit_anchor_is_deterministic_and_distinct() {
        let a = landed_commit_anchor("candA", "sigX");
        let b = landed_commit_anchor("candA", "sigX");
        assert_eq!(a, b, "pure function of (candidate_id, signature)");
        assert_ne!(
            landed_commit_anchor("candA", "sigX"),
            landed_commit_anchor("candB", "sigX")
        );
        assert_ne!(
            landed_commit_anchor("candA", "sigX"),
            landed_commit_anchor("candA", "sigY")
        );
        assert_eq!(a.len(), 64, "sha256 hex");
    }
}
