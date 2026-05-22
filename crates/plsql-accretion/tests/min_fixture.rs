//! P2 determinism + oracle-correctness suite (spec §8 unit/property
//! rows, §1 I-DETERMINISM, `PLSQL-USR-001`).

use plsql_accretion::{
    DEFAULT_MAX_BYTES, GapRecord, build_min_fixture, capture_gaps_with_commit, minimize_estate_gaps,
};
use plsql_engine::{AnalysisRequest, analyze_project};

fn unique_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "plsql-usr-mft-{}-{tag}-{n}-{t}",
        std::process::id()
    ))
}

fn first_gap(source: &str, tag: u32) -> GapRecord {
    let dir = unique_dir(&tag.to_string());
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("a.sql"), source).unwrap();
    let mut req = AnalysisRequest {
        project_root: dir.clone(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = analyze_project(req).expect("engine analyze");
    capture_gaps_with_commit(&run, "mft")
        .into_iter()
        .next()
        .expect("input must produce a repairable gap")
}

#[test]
fn determinism_two_builds_byte_identical() {
    let source =
        "CREATE TABLE billing_accounts (id NUMBER, holder VARCHAR2(80), balance NUMBER);\n";
    let gap = first_gap(source, 1);

    let a = build_min_fixture(source, &gap, DEFAULT_MAX_BYTES).expect("build a");
    let b = build_min_fixture(source, &gap, DEFAULT_MAX_BYTES).expect("build b");

    assert_eq!(a.id, b.id, "I-DETERMINISM: fixture id must be stable");
    assert_eq!(
        a.source, b.source,
        "I-DETERMINISM: fixture source must be byte-identical"
    );
    assert_eq!(
        a.redaction_manifest.redacted_sha256, b.redaction_manifest.redacted_sha256,
        "I-DETERMINISM: proof hash must be stable"
    );
    assert_eq!(
        a.privacy_proof_id().unwrap(),
        b.privacy_proof_id().unwrap(),
        "I-DETERMINISM: privacy_proof_id must be stable"
    );
}

#[test]
fn oracle_gates_on_signature_byte_equality() {
    // The fixture for gap-of-input-A must itself still reproduce
    // A's signature, and that signature must byte-equal the
    // target's (the oracle's contract).
    let source = "CREATE TABLE t_acct (id NUMBER, note VARCHAR2(40));\n";
    let gap = first_gap(source, 2);
    let fx = build_min_fixture(source, &gap, DEFAULT_MAX_BYTES).expect("build");
    assert_eq!(
        fx.signature, gap.signature,
        "fixture must carry the byte-identical target signature"
    );

    // Re-capture on the fixture's own source: it must reproduce a
    // GapRecord whose signature equals the target's. (The oracle
    // inside build_min_fixture already enforced this; we re-prove
    // it here from the outside so the test is not vacuous.)
    let dir = unique_dir("recap");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("a.sql"), &fx.source).unwrap();
    let mut req = AnalysisRequest {
        project_root: dir.clone(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = analyze_project(req).expect("re-analyze fixture");
    let recap = capture_gaps_with_commit(&run, "mft");
    assert!(
        recap.iter().any(|g| g.signature == gap.signature
            && g.diag_code == gap.diag_code
            && g.antlr_rule_path == gap.antlr_rule_path),
        "the stored fixture must still reproduce the target (code,rule,signature)"
    );
}

#[test]
fn oracle_rejects_different_signature_candidate() {
    // Oracle correctness: the predicate gates on (code, rule,
    // signature) byte-equality. Take a REAL gap, then fabricate a
    // target whose `signature` is deliberately different. No
    // candidate derived from the source can ever produce that
    // fabricated signature, so `build_min_fixture` must honestly
    // refuse with `NotReproducible` — never fabricate a fixture by
    // matching on code/rule alone.
    let source = "CREATE TABLE t_oracle (id NUMBER, note VARCHAR2(40));\n";
    let real = first_gap(source, 6);

    // Sanity: the real target IS reproducible (control).
    let ok = build_min_fixture(source, &real, DEFAULT_MAX_BYTES);
    assert!(ok.is_ok(), "control: real target must be reproducible");

    // Now mutate ONLY the signature — same code, same rule.
    let mut wrong = real.clone();
    wrong.signature = "deadbeef".repeat(8); // 64-hex, never produced
    assert_eq!(wrong.diag_code, real.diag_code);
    assert_eq!(wrong.antlr_rule_path, real.antlr_rule_path);
    assert_ne!(wrong.signature, real.signature);

    let res = build_min_fixture(source, &wrong, DEFAULT_MAX_BYTES);
    assert!(
        matches!(res, Err(plsql_accretion::AccretionError::NotReproducible)),
        "oracle must reject a different-signature target (gates on \
         signature byte-equality, not code/rule alone), got {res:?}"
    );
}

/// Capture a gap from `source`, then build its fixture *only* via
/// the provenance seed: `minimize_estate_gaps` is pointed at an
/// **empty** estate dir so the size-ordered file search has nothing
/// to chew on — the only way `min_fixture_id` gets stamped is the
/// gap-provenance synthetic seed (task §2.2 breadth keystone).
fn minimise_via_provenance_only(source: &str, tag: &str) -> (GapRecord, std::path::PathBuf) {
    let mut gap = first_gap(source, 900);
    let empty_estate = unique_dir(&format!("empty-{tag}"));
    std::fs::create_dir_all(&empty_estate).unwrap();
    let repo = unique_dir(&format!("repo-{tag}"));
    std::fs::create_dir_all(&repo).unwrap();
    let mut recs = [gap.clone()];
    minimize_estate_gaps(&empty_estate, &repo, &mut recs, DEFAULT_MAX_BYTES);
    gap = recs[0].clone();
    (gap, repo)
}

#[test]
fn text_scan_comment_shaped_gap_yields_proven_fixture_from_provenance() {
    // A `text_scan>comment` gap (the ×131 private-estate class that had NO
    // fixture under size-ordered seeding). With an empty estate the
    // ONLY route to a fixture is the gap's own rule-path provenance
    // seed — proving the breadth keystone in isolation.
    let src = "x '\nCOMMENT ON TABLE billing_secrets IS 'pii leak';\n";
    let (gap, repo) = minimise_via_provenance_only(src, "comment");
    assert_eq!(
        gap.antlr_rule_path.as_deref(),
        Some("text_scan>comment"),
        "control: this input is the comment text-scan class"
    );
    assert!(
        gap.min_fixture_id.is_some(),
        "the provenance seed must yield a privacy-proven fixture for \
         text_scan>comment even with NO estate files"
    );
    assert!(gap.privacy_proof_id.is_some(), "privacy proof must exist");
    // The stored synthetic fixture must carry zero estate bytes.
    let id = gap.min_fixture_id.unwrap();
    let stored = std::fs::read_to_string(repo.join(".usr/fixtures").join(format!("{id}.sql")))
        .expect("fixture persisted");
    assert!(
        !stored.contains("billing_secrets") && !stored.contains("pii leak"),
        "I-PRIVACY: no original estate byte in the stored fixture: {stored:?}"
    );
}

#[test]
fn structured_unit_statement_gap_yields_proven_fixture_from_provenance() {
    // A structured `unit_statement>*` parse-tree gap also minimises
    // from its own provenance with an empty estate. (The malformed
    // tail yields a PARSE-ANTLR4RUST-001 first, then the structured
    // IR_DDL_NOT_LOWERED — we target the structured one.)
    let src = "CREATE TABLE acct_pii (id NUMBER) §;\n";
    let dir = unique_dir("unit-src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.sql"), src).unwrap();
    let mut req = AnalysisRequest {
        project_root: dir.clone(),
        ..AnalysisRequest::default()
    };
    req.cache.enabled = false;
    let run = analyze_project(req).expect("engine analyze");
    let mut recs: Vec<GapRecord> = capture_gaps_with_commit(&run, "mft")
        .into_iter()
        .filter(|g| {
            g.antlr_rule_path
                .as_deref()
                .is_some_and(|p| p.starts_with("unit_statement>"))
        })
        .collect();
    assert!(
        !recs.is_empty(),
        "control: input must produce a structured unit_statement>* gap"
    );
    let repo = unique_dir("unit-repo");
    std::fs::create_dir_all(&repo).unwrap();
    let empty_estate = unique_dir("unit-empty");
    std::fs::create_dir_all(&empty_estate).unwrap();
    minimize_estate_gaps(&empty_estate, &repo, &mut recs, DEFAULT_MAX_BYTES);
    let gap = &recs[0];
    assert!(
        gap.min_fixture_id.is_some() && gap.privacy_proof_id.is_some(),
        "structured unit_statement>* gap must minimise from provenance \
         with NO estate files: {:?}",
        gap.antlr_rule_path
    );
}

#[test]
fn provenance_minimisation_is_deterministic() {
    // I-DETERMINISM: two independent provenance-only minimisations of
    // the same input yield the byte-identical fixture id + proof id.
    let src = "x '\nDROP TABLE legacy_audit;\n";
    let (a, _) = minimise_via_provenance_only(src, "det-a");
    let (b, _) = minimise_via_provenance_only(src, "det-b");
    assert_eq!(
        a.min_fixture_id, b.min_fixture_id,
        "same input ⇒ byte-identical fixture id"
    );
    assert_eq!(
        a.privacy_proof_id, b.privacy_proof_id,
        "same input ⇒ byte-identical privacy proof id"
    );
    assert!(a.min_fixture_id.is_some(), "must have minimised");
}

#[test]
fn fixture_within_byte_cap() {
    let source = "CREATE TABLE capped (id NUMBER, payload VARCHAR2(200));\n";
    let gap = first_gap(source, 5);
    let fx = build_min_fixture(source, &gap, DEFAULT_MAX_BYTES).expect("build");
    assert!(
        fx.source.len() <= DEFAULT_MAX_BYTES,
        "fixture must respect the byte cap"
    );
}
