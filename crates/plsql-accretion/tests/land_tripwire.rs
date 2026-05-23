//! `land_tripwire.rs` — P6 hermetic tests (spec §8):
//!
//! * **land idempotency/determinism** — re-landing the same accepted
//!   candidate is a no-op; the `landed_commit` anchor is a pure
//!   function of (candidate id, signature);
//! * **Ledger single-append** — a land appends EXACTLY one entry
//!   whose `landed_patch` is set; a second land of the same candidate
//!   appends nothing;
//! * **signature → commit mapping** — the ledger maps the landed
//!   signature to a stable `landed_commit` for one-command
//!   `git revert` rollback (spec §7);
//! * **quarantine path** — a gate REJECT yields a provenanced
//!   [`QuarantineRecord`] naming the failing stage; NOTHING is
//!   landed; the gate is NOT weakened;
//! * **tripwire monotonicity** — a simulated index drop ⇒ the
//!   monotone check FAILs; an increase ⇒ PASS; the accretion ledger
//!   is hash-chained + idempotent-by-content.
//!
//! All hermetic (per-test unique temp dirs — NEVER `.usr/`, the
//! P3.1/P4 cross-binary-race lesson), deterministic, green
//! back-to-back and under `--test-threads=16`. The gate's *inputs*
//! are scoped via the documented `USR_GATE_*` env; the gate's BAR is
//! the real §3 gate, unweakened.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plsql_accretion::{
    AccretionLedger, CandidateDiff, DeterministicStubProposer, GapCluster, LandError, LandFixture,
    Ledger, PatchProposer, RepairClass, compute_accretion_index, land::landed_commit_anchor,
    land_candidate_in, ledger::BenchmarkRecord,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn unique_tmp(tag: &str) -> PathBuf {
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("usr_land_{tag}_{pid}_{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("corpus")).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();
    std::fs::create_dir_all(dir.join("ledger")).unwrap();
    // The DeterministicStubProposer pins under `.usr/usr_pin_<sig16>`
    // via single-program shell hooks (oracle-k30w shell-allowlist).
    // Pre-create the directory the gate's cwd-rooted `touch` needs.
    std::fs::create_dir_all(dir.join(".usr")).unwrap();
    dir
}

const GOOD_SQL: &str = "create or replace package body p is\n\
                         begin\n\
                         null;\n\
                         end;\n";

fn baseline_path() -> String {
    repo_root()
        .join("crates/plsql-accretion/gate_baseline.json")
        .display()
        .to_string()
}

fn absent_estate() -> String {
    std::env::temp_dir()
        .join("usr_land_no_estate_xyzzy")
        .display()
        .to_string()
}

fn scoped_env<'a>(
    corpus: &'a str,
    fixtures: &'a str,
    baseline: &'a str,
    estate_absent: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("USR_GATE_SKIP_BUILD", "1"),
        ("USR_GATE_FAST", "1"),
        ("USR_GATE_CORPUS", corpus),
        ("USR_GATE_FIXTURES_DIR", fixtures),
        ("USR_GATE_BASELINE", baseline),
        ("USR_GATE_ESTATE", estate_absent),
        // Hermetic land-tripwire fixtures use deterministic
        // `true` / `touch` / `rm` shell hooks (all on the G9
        // allowlist); opt in to the trusted-pin path so the gate
        // actually executes the mutation-kill cycle instead of
        // failing closed on the new shell-injection guard
        // (oracle-k30w).
        ("USR_GATE_TRUST_PINS", "1"),
    ]
}

/// A realistic synthetic gap cluster (re-synthesised from grammar +
/// the spec's description of a top private-estate class — zero private-estate bytes).
/// `text_scan>comment` ⇒ the stub picks class `l` (COMMENT is a
/// derivable-lowering verb), producing a gate-runnable honest
/// candidate.
fn synthetic_cluster_sig(sig: &str) -> GapCluster {
    GapCluster {
        signature: sig.to_string(),
        diag_code: "IR_DDL_NOT_LOWERED".to_string(),
        antlr_rule_path: Some("text_scan>comment".to_string()),
        repair_class: RepairClass::Lowering,
        occurrence_count: 42,
        representative_min_fixtures: vec![
            "fx00112233445566778899aabbccddeeff00112233445566778899aabbccddee".to_string(),
        ],
        first_seen_commit: "deadbee".to_string(),
    }
}

/// Distinct 64-hex signature per test so the stub's
/// content-addressed `.usr/usr_pin_<sig16>` G9 marker never collides
/// across parallel tests (the P3.1 cross-binary `.usr` race lesson;
/// hermetic under `--test-threads=16`).
fn sig_for(tag: &str) -> String {
    plsql_accretion::sha256_hex(format!("usr_land_tripwire::{tag}").as_bytes())
}

fn honest_candidate(cluster: &GapCluster) -> CandidateDiff {
    DeterministicStubProposer
        .propose(cluster, "estate_run_synth", "commitsynth")
        .expect("stub produces an honest candidate for a derivable class")
}

/// A candidate whose body is the stub's honest output mutated into a
/// SUPPRESSION (resolved>0, extraction-delta=0). It must be REJECTed
/// at G7 — proving the quarantine path with a real gate failure.
fn suppression_candidate(cluster: &GapCluster) -> CandidateDiff {
    let mut c = honest_candidate(cluster);
    let honest_line = format!(
        "extracted-semantics-delta={}",
        c.honesty.extracted_semantics_delta
    );
    c.body = c.body.replace(&honest_line, "extracted-semantics-delta=0");
    c
}

fn land_fixture() -> LandFixture {
    // A privacy-clean synthetic .sql (grammar keywords + synthetic
    // aliases only — zero estate bytes; mirrors stage [B]'s output).
    LandFixture {
        id: "fx00112233445566778899aabbccddeeff00112233445566778899aabbccddee".to_string(),
        source: "COMMENT ON TABLE id_a IS 'sx';\n".to_string(),
    }
}

// ====================================================================
// land idempotency / determinism + Ledger single-append + sig→commit
// ====================================================================
#[test]
fn land_is_idempotent_single_append_and_maps_signature_to_commit() {
    let tmp = unique_tmp("land_idem");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let cluster = synthetic_cluster_sig(&sig_for("land_idem"));
    let cand = honest_candidate(&cluster);
    let fx = land_fixture();
    let (c, f, b, e) = (
        tmp.join("corpus").display().to_string(),
        tmp.join("fixtures").display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let ledger_dir = tmp.join("ledger");

    // First land: the honest stub candidate clears all 9 stages
    // (degraded-but-real G1/G5/G9 in the hermetic env). If the gate
    // does not ACCEPT here it is a real, honest signal — surface it.
    let r1 = match land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    ) {
        Ok(r) => r,
        Err(LandError::Quarantined(q)) => panic!(
            "honest stub candidate was REJECTED at {} — it should ACCEPT in the hermetic gate: {:?}",
            q.failing_stage, q.stage_evidence
        ),
        Err(e2) => panic!("land failed unexpectedly: {e2}"),
    };
    assert_eq!(r1.signature, cluster.signature);
    assert_eq!(r1.landed_commit.len(), 64, "sha256 hex anchor");
    // sig → commit mapping is the deterministic anchor (spec §7).
    assert_eq!(
        r1.landed_commit,
        landed_commit_anchor(&cand.id, &cluster.signature),
        "landed_commit must be sha256(candidate.id || signature) for git revert rollback"
    );

    // The ledger appended EXACTLY one entry whose landed_patch is set
    // and equals the rollback anchor (signature → commit).
    let ledger = Ledger::open(&ledger_dir).unwrap();
    let entries = ledger.iter().unwrap();
    assert_eq!(entries.len(), 1, "exactly one ledger entry after one land");
    let body = &entries[0].body;
    assert_eq!(body.signature, cluster.signature);
    assert_eq!(
        body.landed_patch.as_deref(),
        Some(r1.landed_commit.as_str())
    );
    assert!(body.gate_verdict.is_some(), "gate verdict recorded (hop-4)");
    ledger.verify_chain().expect("ledger chain intact");

    // Re-land the SAME accepted candidate: idempotent — NO new
    // ledger entry, identical anchor (determinism).
    let r2 = land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    )
    .expect("re-land of the same accepted candidate succeeds");
    assert_eq!(r1.landed_commit, r2.landed_commit, "deterministic anchor");
    assert_eq!(
        ledger.iter().unwrap().len(),
        1,
        "re-landing the same candidate appends NOTHING (idempotent-by-content, spec §1 I-PROVENANCE)"
    );
    assert!(
        r2.idempotent_noop,
        "second land must report an idempotent no-op"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// quarantine path — gate REJECT ⇒ provenanced bead, NOT landed,
// gate NOT weakened
// ====================================================================
#[test]
fn quarantine_on_gate_reject_lands_nothing_and_keeps_gate_intact() {
    let tmp = unique_tmp("quarantine");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let cluster = synthetic_cluster_sig(&sig_for("quarantine"));
    let cand = suppression_candidate(&cluster); // dies at G7
    let fx = land_fixture();
    let (c, f, b, e) = (
        tmp.join("corpus").display().to_string(),
        tmp.join("fixtures").display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let ledger_dir = tmp.join("ledger");

    let err = land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    )
    .expect_err("a suppression candidate MUST be quarantined, never landed");
    match err {
        LandError::Quarantined(q) => {
            assert_eq!(
                q.failing_stage, "G7",
                "suppression must be named as failing at G7 (anti-gaming)"
            );
            assert_eq!(q.signature, cluster.signature);
            assert_eq!(q.candidate_id, cand.id);
            assert!(!q.privacy_abort);
            // Content-addressed quarantine id (dedupable).
            assert_eq!(q.id.len(), 64);
        }
        other => panic!("expected Quarantined, got {other}"),
    }

    // NOTHING landed: no ledger entry, no regression-corpus file.
    let ledger = Ledger::open(&ledger_dir).unwrap();
    assert_eq!(
        ledger.iter().unwrap().len(),
        0,
        "a quarantined candidate must NOT append a ledger entry (nothing landed unproven)"
    );
    assert!(
        !tmp.join("corpus/synthetic/regressions").exists()
            || std::fs::read_dir(tmp.join("corpus/synthetic/regressions"))
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
        "a quarantined candidate must NOT add a regression-corpus fixture"
    );

    // Re-running the SAME rejected candidate is still a quarantine
    // (the gate was NOT weakened to admit it on retry).
    let again = land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    );
    assert!(
        matches!(again, Err(LandError::Quarantined(_))),
        "the gate must reject the same bad candidate every time (never weakened): {again:?}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// tripwire monotonicity — a simulated index DROP fails the monotone
// check; an INCREASE passes; the accretion ledger is hash-chained +
// idempotent-by-content.
// ====================================================================
#[test]
fn accretion_tripwire_is_monotone_and_hash_chained() {
    let tmp = unique_tmp("tripwire");
    let acc = AccretionLedger::open(&tmp).unwrap();

    // Seed the floor at a release ref.
    let base = compute_accretion_index(
        &[BenchmarkRecord {
            objects_with_extracted_semantics: 5,
            objects_unrecognized: 5,
            resolved_signatures: vec!["sigA".into()],
        }],
        "v0.1.0",
    );
    let id0 = acc.append("v0.1.0", base.clone()).unwrap();
    // Idempotent-by-content: appending the same (ref,index) is a
    // no-op returning the same id.
    let id0b = acc.append("v0.1.0", base.clone()).unwrap();
    assert_eq!(id0, id0b, "idempotent-by-content append");
    assert_eq!(acc.iter().unwrap().len(), 1);

    // An INCREASE (a newly-closed signature) ⇒ coverage_index rose ⇒
    // monotone check PASSes.
    let up = compute_accretion_index(
        &[BenchmarkRecord {
            objects_with_extracted_semantics: 5,
            objects_unrecognized: 5,
            resolved_signatures: vec!["sigA".into(), "sigB".into()],
        }],
        "HEAD",
    );
    assert!(
        up.coverage_index > base.coverage_index,
        "closing a new signature must strictly raise coverage_index ({} → {})",
        base.coverage_index,
        up.coverage_index
    );
    acc.append("HEAD", up.clone()).unwrap();
    assert_eq!(acc.iter().unwrap().len(), 2);
    acc.verify_chain().expect("accretion ledger chain intact");

    // A simulated DROP (a closed signature regressed) ⇒ the monotone
    // assertion must FAIL: coverage_index(HEAD) < coverage_index(base).
    let down = compute_accretion_index(
        &[BenchmarkRecord {
            objects_with_extracted_semantics: 1,
            objects_unrecognized: 9,
            resolved_signatures: vec![], // lost the closed signature
        }],
        "HEAD",
    );
    assert!(
        down.coverage_index + f64::EPSILON < base.coverage_index,
        "a regressed/lost signature must drop coverage_index below the floor ({} < {})",
        down.coverage_index,
        base.coverage_index
    );
    // The monotone rule (mirrors the tripwire's assertion).
    let monotone_ok = down.coverage_index + f64::EPSILON >= base.coverage_index;
    assert!(
        !monotone_ok,
        "I-MONOTONIC-VALUE: a coverage_index drop MUST fail the tripwire"
    );

    // Tamper-evidence: editing a persisted body field (the
    // measured commit, which IS part of the content-hash pre-image)
    // breaks the chain.
    let path = acc.path().to_path_buf();
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("v0.1.0"), "commit field is persisted");
    let tampered = content.replacen("v0.1.0", "vTAMPER", 1);
    assert_ne!(tampered, content, "tamper must change a hashed field");
    std::fs::write(&path, tampered).unwrap();
    assert!(
        acc.verify_chain().is_err(),
        "an edited accretion-ledger line must break the hash chain (I-PROVENANCE)"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// the §5 acceptance flow's unit-testable wiring on a SYNTHETIC estate
// (re-synthesised, zero private-estate bytes) — proves the loop wiring without
// the private estate so CI can verify it.
// ====================================================================
#[test]
fn synthetic_estate_close_one_gap_end_to_end_wiring() {
    let tmp = unique_tmp("synthetic_e2e");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let cluster = synthetic_cluster_sig(&sig_for("synthetic_e2e"));
    let cand = honest_candidate(&cluster);
    let fx = land_fixture();
    let (c, f, b, e) = (
        tmp.join("corpus").display().to_string(),
        tmp.join("fixtures").display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let ledger_dir = tmp.join("ledger");

    // [E]→[F]: gate-prove + land.
    let receipt = land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    )
    .expect("synthetic e2e: honest candidate lands");

    // §5.6 analogue: exactly one landed ledger entry, content-addressed.
    let ledger = Ledger::open(&ledger_dir).unwrap();
    let entries = ledger.iter().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].body.landed_patch.as_deref(),
        Some(receipt.landed_commit.as_str())
    );

    // §5.6 analogue: the MinFixture is pinned in the committed
    // regression corpus (the permanent, accretive behaviour pin).
    let pinned = tmp
        .join("corpus/synthetic/regressions")
        .join(format!("usr_{}.sql", fx.id));
    assert!(
        pinned.is_file(),
        "the closed signature must be pinned in the regression corpus: {}",
        pinned.display()
    );
    assert_eq!(std::fs::read_to_string(&pinned).unwrap(), fx.source);

    // §5.5 analogue: the loop's landed entry contributes a distinct
    // resolved signature ⇒ coverage_index strictly rises vs no-land.
    let before = compute_accretion_index(
        &[BenchmarkRecord {
            objects_with_extracted_semantics: 3,
            objects_unrecognized: 1,
            resolved_signatures: vec![],
        }],
        "before",
    );
    let after = compute_accretion_index(
        &[BenchmarkRecord {
            objects_with_extracted_semantics: 3,
            objects_unrecognized: 1,
            resolved_signatures: vec![cluster.signature.clone()],
        }],
        "after",
    );
    assert!(
        after.coverage_index > before.coverage_index,
        "closing the gap must strictly raise coverage_index (accretive): {} → {}",
        before.coverage_index,
        after.coverage_index
    );

    // §5.8 analogue: re-running the whole [E]→[F] is byte-identical
    // (idempotent no-op; deterministic anchor).
    let r2 = land_candidate_in(
        &repo_root(),
        &tmp,
        &cand,
        &cluster,
        &fx,
        "estate_run_synth",
        &ledger_dir,
        &env,
    )
    .unwrap();
    assert_eq!(receipt.landed_commit, r2.landed_commit);
    assert_eq!(ledger.iter().unwrap().len(), 1, "still exactly one entry");

    let _ = std::fs::remove_dir_all(&tmp);
}
