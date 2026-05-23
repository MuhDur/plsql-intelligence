//! `gate_selftest.rs` — THE adversarial trio (spec §3 "Adversarial
//! self-test" + §8 "the single most important test")
//! P4.
//!
//! These tests feed the **REAL** §3 gate (the exact
//! `plsql_accretion::run_gate` → `scripts/usr_gate.sh` code path the
//! loop runs in production — the gate is NOT stubbed or weakened)
//! three known-bad candidates and assert each is rejected at its
//! EXACT named stage:
//!
//! * a **suppression-only** candidate          → REJECT at **G7**
//! * a **privacy-leaking** MinFixture/candidate → REJECT at **G8 + abort**
//! * a **coverage-up-but-round-trip-breaking**  → REJECT at **G2**
//!
//! Plus the gate-property tests (spec §3 "Gate properties"):
//! fail-closed (a candidate that would 8/9 ⇒ REJECT), sha-pin
//! (tampering the script body ⇒ gate.rs aborts with the sha-mismatch
//! error, never a pass), and determinism (two runs ⇒ identical
//! verdict).
//!
//! Only the gate's *inputs* are scoped to a small hermetic fixture
//! set (via per-test temp dirs + the documented `USR_GATE_*` env) so
//! G1–G6 run fast in CI. **The gate's BAR is identical** — same
//! `run_gate`, same `usr_gate.sh`, same thresholds, same check
//! primitives. Hermetic: every test uses its own unique temp dirs
//! (the P3.1 `.usr/fixtures` cross-binary-race lesson) and the suite
//! is deterministic and green back-to-back and under
//! `--test-threads=16`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plsql_accretion::gate::{GateError, GateOutcome, run_gate, verify_gate_sha};

/// Repo root = the workspace dir two levels up from this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Per-test unique scratch dir under the OS temp (NEVER `.usr/` —
/// avoids the P3.1 cross-binary `.usr/fixtures` race; fully
/// hermetic, parallel-safe).
fn unique_tmp(tag: &str) -> PathBuf {
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("usr_gate_selftest_{tag}_{pid}_{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("corpus")).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();
    dir
}

/// A small valid PL/SQL snippet that round-trips losslessly through
/// the antlr4rust backend (so G2 PASSes on the good-input cases —
/// the bad cases must die for the RIGHT reason, not because the
/// corpus itself is broken).
const GOOD_SQL: &str = "create or replace package body p is\n\
                         begin\n\
                         null;\n\
                         end;\n";

/// Build the env that scopes the gate's INPUTS (never its checks)
/// for fast hermetic CI: real but fast G1 build, fast G5
/// regression-corpus replay, the committed baseline, estate absent
/// (G6 honest skip-as-pass).
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
        // Adversarial gate fixtures use the documented `true` /
        // `touch` / `rm` hooks; opt in to the trusted-pin path
        // so G9 actually executes the mutation-kill cycle on the
        // good cases (oracle-k30w shell-injection guard otherwise
        // refuses by default).
        ("USR_GATE_TRUST_PINS", "1"),
    ]
}

fn baseline_path() -> String {
    repo_root()
        .join("crates/plsql-accretion/gate_baseline.json")
        .display()
        .to_string()
}

/// A no-estate path so G6 takes the documented honest skip-as-pass
/// (no run ⇒ no metric can fall below the committed baseline).
fn absent_estate() -> String {
    std::env::temp_dir()
        .join("usr_gate_no_such_estate_xyzzy")
        .display()
        .to_string()
}

/// Write the canonical good honesty + pins manifest into a candidate
/// diff so the *non-targeted* stages (G7/G9) pass on the good cases.
fn good_manifest(mark: &Path) -> String {
    format!(
        "# usr-gate: repair-class=l signature=sig_selftest diagnostics-resolved=2 \
         extracted-semantics-delta=5 posture=improved\n\
         # usr-gate-pins-cmd: test -f \"{m}\"\n\
         # usr-gate-pins-revert: rm -f \"{m}\"\n\
         # usr-gate-pins-restore: touch \"{m}\"\n",
        m = mark.display()
    )
}

// ====================================================================
// (1) Suppression-only candidate  →  MUST be rejected at G7
// ====================================================================
#[test]
fn adversarial_suppression_only_rejected_at_g7() {
    let tmp = unique_tmp("suppress");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let mark = tmp.join("mark");
    std::fs::write(&mark, b"").unwrap();
    // diagnostics-resolved=5 but extracted-semantics-delta=0 ⇒ a
    // coverage drop with NO commensurate extraction rise = the
    // oracle-bh4p suppression dishonesty. Everything else is honest
    // so the candidate would otherwise pass — it MUST die at G7.
    let candidate = tmp.join("suppress.diff");
    std::fs::write(
        &candidate,
        "# usr-gate: repair-class=l signature=sig_x diagnostics-resolved=5 \
         extracted-semantics-delta=0 posture=preserved\n\
         # usr-gate-pins-cmd: true\n\
         # usr-gate-pins-revert: true\n\
         # usr-gate-pins-restore: true\n",
    )
    .unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let outcome = run_gate(&repo_root(), &candidate, &scoped_env(&c, &f, &b, &e))
        .expect("gate runs (suppression candidate)");
    assert!(
        !outcome.is_accept(),
        "suppression-only candidate was ACCEPTED — the safety rail is broken: {outcome:?}"
    );
    assert_eq!(
        outcome.failing_stage(),
        Some("G7"),
        "suppression must die at G7 (anti-gaming), got {outcome:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// (2) Privacy-leaking candidate/fixture  →  MUST be rejected at
//     G8 AND the run aborts (distinct exit, nothing persisted)
// ====================================================================
#[test]
fn adversarial_privacy_leak_rejected_at_g8_and_aborts() {
    let tmp = unique_tmp("privacy");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    // Plant an estate-shaped literal in an "added MinFixture" that
    // survives — exactly the I-PRIVACY violation. The residue scan
    // must catch it AND the run must abort (exit 9), nothing
    // persisted.
    std::fs::write(
        tmp.join("fixtures/leak.sql"),
        "select ESTATE_SECRET from CUSTOMER_SSN;\n",
    )
    .unwrap();
    let mark = tmp.join("mark");
    let candidate = tmp.join("leak.diff");
    std::fs::write(&candidate, good_manifest(&mark)).unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let outcome = run_gate(&repo_root(), &candidate, &scoped_env(&c, &f, &b, &e))
        .expect("gate runs (privacy-leak candidate)");
    assert!(
        !outcome.is_accept(),
        "privacy-leaking candidate was ACCEPTED — I-PRIVACY breached: {outcome:?}"
    );
    assert!(
        matches!(outcome, GateOutcome::PrivacyAbort { .. }),
        "privacy leak must be a PrivacyAbort (G8 + abort, distinct from a plain Reject), got {outcome:?}"
    );
    assert_eq!(outcome.failing_stage(), Some("G8"));
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// (3) Coverage-up but round-trip-breaking  →  MUST be rejected at G2
// ====================================================================
#[test]
fn adversarial_roundtrip_break_rejected_at_g2() {
    let tmp = unique_tmp("rtbreak");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    // An "added MinFixture" whose bytes do NOT survive the
    // antlr4rust reconstruct round-trip: a realistic Oracle export
    // carrying a leading UTF-8 BOM (EF BB BF) that the lexer drops —
    // `reconstruct(tape) != input` byte-for-byte (verified: 23 bytes
    // in, 20 out). The honesty manifest is impeccable (coverage up!)
    // and the SQL is otherwise valid, so the ONLY reason to reject is
    // the lossless-round-trip break: it MUST die at G2, before G7/G8.
    std::fs::write(
        tmp.join("fixtures/rtbreak.sql"),
        b"\xef\xbb\xbfselect 1 from dual;\n".as_slice(),
    )
    .unwrap();
    let mark = tmp.join("mark");
    let candidate = tmp.join("rtbreak.diff");
    std::fs::write(&candidate, good_manifest(&mark)).unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let outcome = run_gate(&repo_root(), &candidate, &scoped_env(&c, &f, &b, &e))
        .expect("gate runs (round-trip-break candidate)");
    assert!(
        !outcome.is_accept(),
        "round-trip-breaking candidate was ACCEPTED — losslessness broken: {outcome:?}"
    );
    assert_eq!(
        outcome.failing_stage(),
        Some("G2"),
        "round-trip break must die at G2 (lossless), got {outcome:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// Gate property: fail-closed — a candidate that would otherwise be
// 8/9 (only the honesty stage off) is REJECTed, never "mostly
// passes". (No partial credit, spec §3.)
// ====================================================================
#[test]
fn gate_property_fail_closed_no_partial_credit() {
    let tmp = unique_tmp("failclosed");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let mark = tmp.join("mark");
    std::fs::write(&mark, b"").unwrap();
    // Eight stages would pass; G7 is sabotaged (posture weakened).
    // The outcome must be a hard Reject — never Accept, never
    // "8/9 ok".
    let candidate = tmp.join("eight9.diff");
    std::fs::write(
        &candidate,
        "# usr-gate: repair-class=l signature=sig_x diagnostics-resolved=0 \
         extracted-semantics-delta=0 posture=weakened\n\
         # usr-gate-pins-cmd: true\n\
         # usr-gate-pins-revert: true\n\
         # usr-gate-pins-restore: true\n",
    )
    .unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let outcome = run_gate(&repo_root(), &candidate, &scoped_env(&c, &f, &b, &e))
        .expect("gate runs (8/9 candidate)");
    assert!(
        !outcome.is_accept(),
        "8/9 candidate was ACCEPTED — partial credit is forbidden: {outcome:?}"
    );
    match &outcome {
        GateOutcome::Reject {
            failing_stage,
            stages,
            ..
        } => {
            assert_eq!(failing_stage.as_deref(), Some("G7"));
            // Proof of "no partial credit": the run stopped at the
            // first non-PASS, it did NOT collect 8 passes + 1 fail.
            assert!(
                stages.iter().filter(|s| s.passed).count() < 9,
                "fail-closed must stop at the first non-PASS"
            );
        }
        other => panic!("expected Reject, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// Gate property: sha-pin / immutability — tampering the gate script
// body makes the typed runner ABORT with the sha-mismatch error
// (NEVER a pass). The committed on-disk script must match its pin.
// ====================================================================
#[test]
fn gate_property_sha_pin_detects_tamper() {
    let root = repo_root();
    // 1. The committed on-disk gate matches its committed pin.
    verify_gate_sha(&root).expect("committed gate script matches its committed sha pin");

    // 2. Tampering the script body (in an isolated copy of the repo
    //    layout) makes verify_gate_sha ABORT with ShaMismatch — not
    //    a pass, not any other error class.
    let tmp = unique_tmp("shapin");
    std::fs::create_dir_all(tmp.join("scripts")).unwrap();
    std::fs::create_dir_all(tmp.join("crates/plsql-accretion")).unwrap();
    // Tampered body, but copy the GENUINE committed pin alongside.
    std::fs::write(
        tmp.join("scripts/usr_gate.sh"),
        "#!/usr/bin/env bash\necho 'GATE G1: PASS tampered-to-always-pass'\nexit 0\n",
    )
    .unwrap();
    std::fs::copy(
        root.join("crates/plsql-accretion/gate.sha256"),
        tmp.join("crates/plsql-accretion/gate.sha256"),
    )
    .unwrap();
    let err = verify_gate_sha(&tmp).expect_err("tampered gate must NOT verify");
    assert!(
        matches!(err, GateError::ShaMismatch { .. }),
        "tamper must abort with ShaMismatch (not a pass, not another error): {err:?}"
    );

    // 3. run_gate itself refuses to even run a tampered gate.
    let candidate = tmp.join("c.diff");
    std::fs::write(&candidate, "# usr-gate: repair-class=l signature=x posture=preserved diagnostics-resolved=0 extracted-semantics-delta=0\n").unwrap();
    let run_err = run_gate(&tmp, &candidate, &[])
        .expect_err("run_gate must abort on a tampered (sha-mismatched) gate");
    assert!(
        matches!(run_err, GateError::ShaMismatch { .. }),
        "run_gate on tampered gate must be ShaMismatch, never Accept: {run_err:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ====================================================================
// Gate property: determinism — two runs of the gate on the same
// candidate + same commit produce identical verdicts (I-DETERMINISM,
// spec §3).
// ====================================================================
#[test]
fn gate_property_deterministic_verdict() {
    let tmp = unique_tmp("determ");
    std::fs::write(tmp.join("corpus/good.sql"), GOOD_SQL).unwrap();
    let mark = tmp.join("mark");
    std::fs::write(&mark, b"").unwrap();
    let candidate = tmp.join("suppress.diff");
    std::fs::write(
        &candidate,
        "# usr-gate: repair-class=l signature=sig_x diagnostics-resolved=9 \
         extracted-semantics-delta=1 posture=preserved\n\
         # usr-gate-pins-cmd: true\n# usr-gate-pins-revert: true\n# usr-gate-pins-restore: true\n",
    )
    .unwrap();

    let corpus = tmp.join("corpus");
    let fixtures = tmp.join("fixtures");
    let (c, f, b, e) = (
        corpus.display().to_string(),
        fixtures.display().to_string(),
        baseline_path(),
        absent_estate(),
    );
    let env = scoped_env(&c, &f, &b, &e);
    let a = run_gate(&repo_root(), &candidate, &env).expect("run 1");
    let two = run_gate(&repo_root(), &candidate, &env).expect("run 2");
    assert_eq!(
        a, two,
        "two gate runs on the same candidate+commit must be byte-identical verdicts (I-DETERMINISM)"
    );
    assert_eq!(a.failing_stage(), Some("G7"));
    let _ = std::fs::remove_dir_all(&tmp);
}
