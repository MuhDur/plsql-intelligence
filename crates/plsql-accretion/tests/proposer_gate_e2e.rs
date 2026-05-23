//! `proposer_gate_e2e.rs` — the P5 keystone (spec §10 P5 + §3),
//!
//! Takes a **realistic synthetic gap cluster** (re-synthesised from
//! grammar + the spec's description of a top private-estate class — an
//! un-lowered DDL `IR_DDL_NOT_LOWERED` pattern; **never** a private-estate
//! byte), runs the [`DeterministicStubProposer`] → a
//! [`CandidateDiff`], then feeds that candidate through the **REAL P4
//! gate** (`plsql_accretion::run_gate` → `scripts/usr_gate.sh`, the
//! exact production code path — NOT stubbed or weakened) and asserts:
//!
//! 1. the candidate is well-formed, in a valid repair class, and
//!    R20-safe (proposer's own I-ISOLATION backstop holds);
//! 2. the **stub-driven gate runs DETERMINISTICALLY** — two runs on
//!    the same candidate + commit ⇒ byte-identical verdict (the spec
//!    §10 P5 bar);
//! 3. a **suppression-style mutation** of the stub's honest output is
//!    correctly REJECTED at **G7** — proving the proposer's honest
//!    candidates are *distinguishable* from gaming (G7/D3).
//!
//! Per §10 P5 the bar here is "produces valid candidate diffs" +
//! "stub-driven gate runs deterministically" — NOT a fully-green
//! G1–G9 land on a real private-estate gap (that is P6's `usr_acceptance.sh`).
//! Only the gate's *inputs* are scoped to a small hermetic fixture
//! set (per-test temp dirs + the documented `USR_GATE_*` env, the
//! P3.1/P4 cross-binary `.usr` race lesson); the gate's BAR is
//! identical. Deterministic, green back-to-back and under
//! `--test-threads=16`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plsql_accretion::{
    CandidateDiff, CannedBackend, DeterministicStubProposer, GapCluster, LlmProposer,
    PatchProposer, RepairClass, gate::GateOutcome, gate::run_gate,
};

/// Repo root = the workspace dir two levels up from this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Per-test unique scratch dir under the OS temp (NEVER `.usr/` —
/// avoids the P3.1 cross-binary race; hermetic, parallel-safe).
fn unique_tmp(tag: &str) -> PathBuf {
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("usr_proposer_e2e_{tag}_{pid}_{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("corpus")).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();
    dir
}

/// A small valid PL/SQL snippet that round-trips losslessly through
/// the antlr4rust backend (so G2 PASSes — the test exercises the
/// proposer + gate determinism, not a broken corpus).
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

/// No-estate path so G6 takes the documented honest skip-as-pass.
fn absent_estate() -> String {
    std::env::temp_dir()
        .join("usr_proposer_e2e_no_estate_xyzzy")
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
        // Proposer-e2e fixtures pin via the documented `true`
        // hooks; opt in to the trusted-pin path so the G9
        // mutation-kill cycle actually runs (oracle-k30w
        // shell-injection guard otherwise refuses by default).
        ("USR_GATE_TRUST_PINS", "1"),
    ]
}

/// Re-synthesise a realistic gap cluster mirroring a **top
/// private-estate class** (the spec §5 names 992 `IR_DDL_NOT_LOWERED`
/// classes). The signature/commit/fixture-id are synthetic content
/// hashes — every byte is grammar-derived, **zero private-estate bytes**
/// (I-PRIVACY by construction). `text_scan>create_materialized` ⇒ the
/// stub picks class `d` (CREATE MATERIALIZED is not a derivable-lowering
/// verb; last resort, typed Unknown).
fn synthetic_estate_like_cluster() -> GapCluster {
    GapCluster {
        signature: "a1b2c3d4e5f60718a1b2c3d4e5f60718a1b2c3d4e5f60718a1b2c3d4e5f60718".to_string(),
        diag_code: "IR_DDL_NOT_LOWERED".to_string(),
        antlr_rule_path: Some("unit_statement>create_materialized".to_string()),
        repair_class: RepairClass::Lowering,
        occurrence_count: 992,
        // A privacy-proven representative fixture id (synthetic
        // content hash — never a private-estate byte).
        representative_min_fixtures: vec![
            "fixturehash00112233445566778899aabbccddeeff00112233445566778899".to_string(),
        ],
        first_seen_commit: "deadbee".to_string(),
    }
}

/// Write a candidate's gate-consumed body to a temp file the gate
/// reads (mirrors how P6/the loop persists a proposed candidate).
fn write_candidate(dir: &Path, cand: &CandidateDiff) -> PathBuf {
    let p = dir.join("candidate.diff");
    std::fs::write(&p, &cand.body).unwrap();
    p
}

// ====================================================================
// THE KEYSTONE: stub → CandidateDiff → REAL P4 gate, deterministic.
// ====================================================================
#[test]
fn stub_driven_candidate_runs_real_gate_deterministically() {
    let tmp = unique_tmp("keystone");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();

    // 1. Re-synthesised realistic cluster → DeterministicStubProposer.
    let cluster = synthetic_estate_like_cluster();
    let proposer = DeterministicStubProposer;
    let cand = proposer
        .propose(&cluster, "estate_run_synth", "commitsynth")
        .expect("stub produces a valid candidate for a top estate-like class");

    // 1a. Well-formed, valid repair class, R20-safe (the proposer's
    //     own I-ISOLATION backstop — never even emit out-of-scope).
    assert_eq!(cand.repair_class, RepairClass::TypedDegradation);
    assert_eq!(cand.honesty.repair_class, "d");
    assert_eq!(cand.honesty.unknown_reason, "UnsupportedDialectFeature");
    assert_eq!(cand.signature, cluster.signature);
    cand.validate_r20().expect("stub candidate is R20-safe");
    assert!(
        cand.touched_paths
            .iter()
            .all(|p| p.starts_with("crates/plsql-parser-antlr/")),
        "stub must only touch R20-safe paths: {:?}",
        cand.touched_paths
    );
    // D3 inequality holds by construction (delta ≥ resolved), posture
    // never weakened, Unknown→typed-known still surfaced.
    assert_eq!(
        cand.honesty.extracted_semantics_delta,
        cand.honesty.diagnostics_resolved
    );
    assert_eq!(cand.honesty.posture, "preserved");

    // 1b. Determinism of the proposer artifact itself.
    let cand2 = proposer
        .propose(&cluster, "estate_run_synth", "commitsynth")
        .unwrap();
    assert_eq!(
        cand, cand2,
        "same cluster+commit ⇒ byte-identical candidate"
    );

    // 2. Feed the candidate through the REAL P4 gate twice; assert
    //    the stub-driven gate runs DETERMINISTICALLY (§10 P5 bar).
    let candidate_path = write_candidate(&tmp, &cand);
    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let v1 =
        run_gate(&repo_root(), &candidate_path, &env).expect("gate runs on stub candidate (1)");
    let v2 =
        run_gate(&repo_root(), &candidate_path, &env).expect("gate runs on stub candidate (2)");
    assert_eq!(
        v1, v2,
        "stub-driven gate must be byte-identical across two runs (I-DETERMINISM, §10 P5)"
    );
    // The honest candidate must clear the honesty stage (G7) — its
    // manifest is consistent by construction. (A full G1–G9 ACCEPT on
    // a real private-estate gap is P6's usr_acceptance.sh, NOT claimed here; we
    // assert the candidate is NOT rejected for dishonesty.)
    assert_ne!(
        v1.failing_stage(),
        Some("G7"),
        "an honest stub candidate must NOT be rejected at G7 (anti-gaming): {v1:?}"
    );
    // It is never a privacy abort (the candidate body carries zero
    // estate marker; the fixtures dir is empty/synthetic).
    assert!(
        !matches!(v1, GateOutcome::PrivacyAbort { .. }),
        "honest synthetic candidate must not trip the I-PRIVACY abort: {v1:?}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// A suppression-style MUTATION of the stub's honest output is
// correctly REJECTED at G7 — the proposer's honest candidates are
// distinguishable from gaming (spec §10 P5 + G7/D3).
// ====================================================================
#[test]
fn suppression_mutation_of_stub_candidate_rejected_at_g7() {
    let tmp = unique_tmp("suppmut");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();

    let cluster = synthetic_estate_like_cluster();
    let cand = DeterministicStubProposer
        .propose(&cluster, "run", "commit")
        .unwrap();

    // Mutate the honest manifest into a SUPPRESSION: keep the
    // resolved count but zero the extraction delta — diagnostics fall
    // with NO commensurate extraction rise (the exact oracle-bh4p
    // dishonesty G7/D3 forbid). Everything else is the stub's honest
    // body, so the ONLY reason to reject is the suppression: it MUST
    // die at G7, proving honest ≠ gaming is gate-detectable.
    let resolved = cand.honesty.diagnostics_resolved;
    let honest_line = format!(
        "extracted-semantics-delta={}",
        cand.honesty.extracted_semantics_delta
    );
    let suppressed = cand
        .body
        .replace(&honest_line, "extracted-semantics-delta=0");
    assert_ne!(suppressed, cand.body, "mutation must change the body");
    assert!(resolved > 0, "resolved must be > 0 for suppression to bite");

    let candidate_path = tmp.join("suppressed.diff");
    std::fs::write(&candidate_path, &suppressed).unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let outcome = run_gate(&repo_root(), &candidate_path, &scoped_env(&c, &f, &b, &e))
        .expect("gate runs on the suppression-mutated candidate");
    assert!(
        !outcome.is_accept(),
        "a suppression mutation of the stub's output was ACCEPTED — proposer honesty is not distinguishable from gaming: {outcome:?}"
    );
    assert_eq!(
        outcome.failing_stage(),
        Some("G7"),
        "suppression mutation must die at G7 (anti-gaming), got {outcome:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// The honest refusal path is a correct, deterministic outcome (spec
// §7/§9): a cluster with no privacy-proven fixture is `unrepairable`,
// NOT a fabricated candidate.
// ====================================================================
#[test]
fn stub_refusal_is_deterministic_and_correct() {
    let mut cluster = synthetic_estate_like_cluster();
    cluster.representative_min_fixtures.clear(); // no proven fixture
    let p = DeterministicStubProposer;
    let e1 = p.propose(&cluster, "r", "c").unwrap_err();
    let e2 = p.propose(&cluster, "r", "c").unwrap_err();
    assert!(
        matches!(e1, plsql_accretion::ProposerError::Unrepairable { .. }),
        "{e1:?}"
    );
    // Determinism: the refusal message is a pure function of the
    // cluster (same error string both times).
    assert_eq!(e1.to_string(), e2.to_string());
}

// ====================================================================
// The LlmProposer (canned deterministic backend, NO network) is held
// to the IDENTICAL R20/D3/gate path: a canned reply parses into a
// valid CandidateDiff that runs the real gate deterministically.
// ====================================================================
#[test]
fn llm_canned_candidate_runs_real_gate_deterministically() {
    let tmp = unique_tmp("llm");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();

    let cluster = synthetic_estate_like_cluster();
    // A canned model reply: an honest class-`d` candidate touching
    // only the R20-safe lowering classifier, with the gate-runnable
    // manifest + pins. NO network — the backend is deterministic.
    let reply = format!(
        "# usr-gate: repair-class=d signature={sig} diagnostics-resolved=992 \
         extracted-semantics-delta=992 posture=preserved unknown-reason=UnsupportedDialectFeature\n\
         # usr-gate-pins-cmd: true\n\
         # usr-gate-pins-revert: true\n\
         # usr-gate-pins-restore: true\n\
         # usr-gate-test-path: crates/plsql-parser-antlr/src/lower/usr_llm_d.rs\n\
         # USR candidate (class d) — PROPOSED, NOT APPLIED\n\
         --- a/crates/plsql-parser-antlr/src/lower/mod.rs\n\
         +++ b/crates/plsql-parser-antlr/src/lower/mod.rs\n\
         @@ -0,0 +1,1 @@\n\
         +// typed-degradation arm: Unknown -> UnsupportedDialectFeature (still surfaced)\n",
        sig = cluster.signature
    );
    let proposer = LlmProposer::new(CannedBackend { reply });
    let cand = proposer
        .propose(&cluster, "run", "commit")
        .expect("canned LLM reply parses into a valid CandidateDiff");
    assert_eq!(cand.repair_class, RepairClass::TypedDegradation);
    assert_eq!(cand.proposer, "llm:canned");
    cand.validate_r20()
        .expect("LLM candidate held to identical R20 bar");

    let candidate_path = write_candidate(&tmp, &cand);
    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let a = run_gate(&repo_root(), &candidate_path, &env).expect("gate runs on LLM candidate (1)");
    let two =
        run_gate(&repo_root(), &candidate_path, &env).expect("gate runs on LLM candidate (2)");
    assert_eq!(
        a, two,
        "LLM-driven gate must be byte-identical across two runs (I-DETERMINISM)"
    );
    assert_ne!(
        a.failing_stage(),
        Some("G7"),
        "honest canned LLM candidate must not be rejected at G7: {a:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
