//! P3 unit/integration tests (spec §2 step [C], §2.1, §4,
//! `PLSQL-USR-001`): clustering/dedup, the append-only tamper-evident
//! ledger, and the corpus-only accretion index.
//!
//! These do NOT touch the frozen `gap.rs` signature, the
//! `SignatureOracle`, or `fixture.rs` scrub/privacy — they consume
//! `GapRecord`s the way the loop produces them and assert the P3
//! contracts only.

use std::collections::BTreeMap;

use plsql_accretion::{
    BenchmarkRecord, GapRecord, Ledger, LedgerBody, RepairClass, cluster_gaps, cluster_gaps_with,
    compute_accretion_index,
};

/// A minimal synthetic `GapRecord` builder — provenance only, no
/// source. `signature` is supplied directly (we are testing
/// *grouping by* the frozen signature, never re-deriving it).
fn rec(
    signature: &str,
    diag_code: &str,
    rule: Option<&str>,
    fixture: Option<(&str, &str)>,
    commit: &str,
) -> GapRecord {
    let (min_fixture_id, privacy_proof_id) = match fixture {
        Some((f, p)) => (Some(f.to_string()), Some(p.to_string())),
        None => (None, None),
    };
    GapRecord {
        signature: signature.to_string(),
        diag_code: diag_code.to_string(),
        antlr_rule_path: rule.map(str::to_string),
        unknown_reason: None,
        span_shape: vec!["KW".to_string()],
        estate_run_id: "run-abc".to_string(),
        occurrence_count: 1,
        first_seen_commit: commit.to_string(),
        min_fixture_id,
        repair_class: RepairClass::Grammar,
        privacy_proof_id,
    }
}

#[test]
fn cluster_dedups_same_signature() {
    // 5 occurrences of ONE signature → exactly 1 cluster,
    // occurrence_count == 5.
    let recs: Vec<GapRecord> = (0..5)
        .map(|i| {
            rec(
                "sigA",
                "PARSE-ANTLR4RUST-001",
                Some("text_scan>create_table"),
                Some(("fixA", "proofA")),
                &format!("commit{i:02}"),
            )
        })
        .collect();
    let clusters = cluster_gaps(&recs);
    assert_eq!(
        clusters.len(),
        1,
        "same signature must collapse to 1 cluster"
    );
    let c = &clusters[0];
    assert_eq!(c.signature, "sigA");
    assert_eq!(c.occurrence_count, 5, "occurrence_count = sum of members");
    assert_eq!(
        c.representative_min_fixtures,
        vec!["fixA".to_string()],
        "deduped distinct fixture"
    );
    // first_seen_commit = byte-minimum across members ("commit00").
    assert_eq!(c.first_seen_commit, "commit00");
}

#[test]
fn distinct_signatures_stay_distinct_clusters() {
    // Two different signatures → 2 clusters (fine-grained signature
    // preserved; never coarsened/re-derived).
    let recs = vec![
        rec("sigA", "PARSE-ANTLR4RUST-001", Some("a"), None, "c1"),
        rec("sigA", "PARSE-ANTLR4RUST-001", Some("a"), None, "c1"),
        rec("sigB", "IR_DDL_NOT_LOWERED", Some("b"), None, "c1"),
    ];
    let clusters = cluster_gaps(&recs);
    assert_eq!(clusters.len(), 2, "two signatures → two clusters");
    let by_sig: BTreeMap<_, _> = clusters.iter().map(|c| (c.signature.clone(), c)).collect();
    assert_eq!(by_sig["sigA"].occurrence_count, 2);
    assert_eq!(by_sig["sigB"].occurrence_count, 1);
}

#[test]
fn cluster_representatives_capped_and_smallest_first() {
    // 4 distinct privacy-proven fixtures for one signature, with a
    // size map; default K=3 keeps the 3 SMALLEST, smallest-first.
    let recs = vec![
        rec("s", "c", Some("r"), Some(("big", "p1")), "c1"),
        rec("s", "c", Some("r"), Some(("small", "p2")), "c1"),
        rec("s", "c", Some("r"), Some(("mid", "p3")), "c1"),
        rec("s", "c", Some("r"), Some(("huge", "p4")), "c1"),
        rec("s", "c", Some("r"), Some(("small", "p2")), "c1"), // dup id
    ];
    let mut sizes = BTreeMap::new();
    sizes.insert("small".to_string(), 10u64);
    sizes.insert("mid".to_string(), 50u64);
    sizes.insert("big".to_string(), 200u64);
    sizes.insert("huge".to_string(), 9000u64);
    let clusters = cluster_gaps_with(&recs, 3, &sizes);
    assert_eq!(clusters.len(), 1);
    let c = &clusters[0];
    assert_eq!(c.occurrence_count, 5, "all members counted incl. dup");
    assert_eq!(
        c.representative_min_fixtures,
        vec!["small".to_string(), "mid".to_string(), "big".to_string()],
        "≤K=3 smallest distinct fixtures, smallest-source-first"
    );
}

#[test]
fn cluster_with_no_proven_fixture_is_still_valid() {
    // A fixture id WITHOUT a privacy proof must NOT become a
    // representative (I-PRIVACY) — but the cluster still exists and
    // counts the occurrences (honest, R13).
    let mut r = rec("sigX", "PARSE-ANTLR4RUST-001", Some("x"), None, "c1");
    r.min_fixture_id = Some("unproven".to_string());
    r.privacy_proof_id = None;
    let clusters = cluster_gaps(&[r]);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].occurrence_count, 1);
    assert!(
        clusters[0].representative_min_fixtures.is_empty(),
        "no privacy proof ⇒ no representative, but cluster still valid"
    );
}

#[test]
fn cluster_is_deterministic() {
    // Same input (in two different member orders) → byte-identical
    // serialized clusters. No HashMap iteration / RNG / wall-clock.
    let a = vec![
        rec("z", "c", Some("r"), Some(("f3", "p3")), "c3"),
        rec("a", "c", Some("r"), Some(("f1", "p1")), "c1"),
        rec("m", "c", Some("r"), Some(("f2", "p2")), "c2"),
        rec("a", "c", Some("r"), Some(("f1", "p1")), "c1"),
    ];
    let mut b = a.clone();
    b.reverse();
    let ca = serde_json::to_string(&cluster_gaps(&a)).unwrap();
    let cb = serde_json::to_string(&cluster_gaps(&b)).unwrap();
    assert_eq!(
        ca, cb,
        "clustering must be byte-identical regardless of input order"
    );
    // And stable across repeated runs.
    assert_eq!(ca, serde_json::to_string(&cluster_gaps(&a)).unwrap());
}

#[test]
fn cluster_mixed_repair_class_same_signature_is_order_invariant() {
    // I-DETERMINISM regression guard (the real `usr_acceptance.sh`
    // STEP 8 bug). `repair_class` is NOT an input to the frozen
    // `signature` (`sha256(diag_code, antlr_rule_path,
    // token-kind-shape)`), so one signature can fold records of
    // *mixed* `repair_class` — e.g. the same `IR_DDL_NOT_LOWERED` /
    // `text_scan>create` shape, some occurrences carrying a typed
    // `UnknownReason` (⇒ `TypedDegradation`/`d`), some not (⇒
    // `Lowering`/`l`). Pre-fix, `cluster_gaps` took the class from the
    // FIRST record seen, so the persisted `target_cluster.json`
    // flipped `l`↔`d` whenever `run.diagnostics` order changed (it is
    // NOT stable across the §3 gate's `cargo build --workspace`
    // recompile between the two acceptance runs). The folded cluster
    // MUST be a pure function of the record *set*, not its order.
    let mk = |rc: RepairClass| GapRecord {
        signature: "245d2a92".to_string(),
        diag_code: "IR_DDL_NOT_LOWERED".to_string(),
        antlr_rule_path: Some("text_scan>create".to_string()),
        unknown_reason: None,
        span_shape: vec!["KW".to_string()],
        estate_run_id: "run-abc".to_string(),
        occurrence_count: 1,
        first_seen_commit: "c1a29e3".to_string(),
        min_fixture_id: Some("ad82b25c".to_string()),
        repair_class: rc,
        privacy_proof_id: Some("proofX".to_string()),
    };
    // Same multiset of occurrences, fed in two opposite orders.
    let order_ld = vec![mk(RepairClass::Lowering), mk(RepairClass::TypedDegradation)];
    let order_dl = vec![mk(RepairClass::TypedDegradation), mk(RepairClass::Lowering)];

    let ca = serde_json::to_string(&cluster_gaps(&order_ld)).unwrap();
    let cb = serde_json::to_string(&cluster_gaps(&order_dl)).unwrap();
    assert_eq!(
        ca, cb,
        "a mixed-repair_class signature must fold to a byte-identical \
         cluster regardless of record order (I-DETERMINISM; the \
         usr_acceptance.sh STEP 8 root cause)"
    );

    // The folded class is the deterministic `Ord`-minimum
    // (`Grammar < Lowering < TypedDegradation < Unrepairable`), not
    // a coin-flip on iteration order: here `Lowering` ("l").
    let clusters = cluster_gaps(&order_dl);
    assert_eq!(clusters.len(), 1, "one signature → one cluster");
    assert_eq!(
        clusters[0].repair_class,
        RepairClass::Lowering,
        "folded repair_class must be the stable Ord-minimum, never \
         the first-seen record's class"
    );
}

fn sample_body(sig: &str) -> LedgerBody {
    LedgerBody {
        estate_run_id: "run-1".to_string(),
        signature: sig.to_string(),
        diag_code: "PARSE-ANTLR4RUST-001".to_string(),
        antlr_rule_path: Some("text_scan>create_table".to_string()),
        repair_class: RepairClass::Grammar,
        occurrence_count: 7,
        representative_min_fixtures: vec!["fix1".to_string()],
        gate_verdict: None,
        landed_patch: None,
    }
}

#[test]
fn ledger_is_append_only_and_tamper_evident() {
    let tmp = tempdir();
    let ledger = Ledger::open(&tmp).unwrap();
    ledger.append(sample_body("sig1")).unwrap();
    ledger.append(sample_body("sig2")).unwrap();
    ledger.append(sample_body("sig3")).unwrap();
    // Clean chain verifies.
    ledger.verify_chain().expect("fresh chain must verify");
    assert_eq!(ledger.iter().unwrap().len(), 3);

    let path = ledger.path().to_path_buf();
    let original = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = original.lines().collect();

    // (1) EDIT a byte in the middle entry's body → chain breaks,
    // naming a line.
    let edited = original.replacen("sig2", "sigX", 1);
    std::fs::write(&path, &edited).unwrap();
    let err = ledger.verify_chain().unwrap_err();
    assert!(
        err.to_string().contains("chain broken"),
        "edit must break the chain: {err}"
    );

    // (2) REORDER two entries → chain breaks (parent mismatch).
    let reordered = format!("{}\n{}\n{}\n", lines[1], lines[0], lines[2]);
    std::fs::write(&path, &reordered).unwrap();
    let err = ledger.verify_chain().unwrap_err();
    assert!(
        err.to_string().contains("chain broken"),
        "reorder must break the chain: {err}"
    );

    // (3) TRUNCATE the tail (drop last entry then verify the
    // remaining prefix is still self-consistent — truncation of a
    // *prefix* dependency is the detectable case; here we instead
    // corrupt by removing the FIRST line so every parent link
    // dangles).
    let truncated = format!("{}\n{}\n", lines[1], lines[2]);
    std::fs::write(&path, &truncated).unwrap();
    let err = ledger.verify_chain().unwrap_err();
    assert!(
        err.to_string().contains("chain broken"),
        "truncation/removal must break the chain: {err}"
    );

    // Restore → verifies again (proves the breaks were real, not
    // a always-fail check).
    std::fs::write(&path, &original).unwrap();
    ledger.verify_chain().expect("restored chain verifies");
}

#[test]
fn ledger_has_no_public_mutate_or_delete_api() {
    // Structural/compile-time proof: the ONLY state-changing method
    // on Ledger is `append`. There is no `update`/`delete`/`set`/
    // `remove`/`truncate`. This test documents + locks that — if a
    // mutating method is ever added, the reviewer must consciously
    // edit this test, which is the tripwire.
    let src = include_str!("../src/ledger.rs");
    for forbidden in [
        "pub fn update",
        "pub fn delete",
        "pub fn remove",
        "pub fn set_",
        "pub fn truncate",
        "pub fn overwrite",
    ] {
        assert!(
            !src.contains(forbidden),
            "append-only violated: found `{forbidden}` in ledger.rs"
        );
    }
    assert!(src.contains("pub fn append"), "append must exist");
}

#[test]
fn ledger_append_is_idempotent_by_content() {
    let tmp = tempdir();
    let ledger = Ledger::open(&tmp).unwrap();
    let id1 = ledger.append(sample_body("sigDup")).unwrap();
    // Appending the SAME logical entry again at the tip is a no-op.
    let id2 = ledger.append(sample_body("sigDup")).unwrap();
    assert_eq!(id1, id2, "idempotent: identical content ⇒ same id");
    assert_eq!(
        ledger.iter().unwrap().len(),
        1,
        "idempotent append must not grow the chain"
    );
    ledger.verify_chain().unwrap();
    // A different body DOES append.
    ledger.append(sample_body("sigOther")).unwrap();
    assert_eq!(ledger.iter().unwrap().len(), 2);
    ledger.verify_chain().unwrap();
}

#[test]
fn accretion_index_is_deterministic_and_corpus_only() {
    // Pure function of corpus-derived inputs (never the private estate). Same
    // inputs + commit → byte-identical index.
    let bench = vec![
        BenchmarkRecord {
            objects_with_extracted_semantics: 80,
            objects_unrecognized: 20,
            resolved_signatures: vec!["sigA".to_string(), "sigB".to_string()],
        },
        BenchmarkRecord {
            objects_with_extracted_semantics: 10,
            objects_unrecognized: 0,
            // sigB repeated → still ONE distinct resolved signature.
            resolved_signatures: vec!["sigB".to_string()],
        },
    ];
    let i1 = compute_accretion_index(&bench, "deadbeef");
    let i2 = compute_accretion_index(&bench, "deadbeef");
    assert_eq!(
        serde_json::to_string(&i1).unwrap(),
        serde_json::to_string(&i2).unwrap(),
        "index must be byte-identical for identical inputs"
    );
    // ratio = 90 / 110; distinct resolved = 2 (sigA, sigB).
    assert!((i1.extracted_semantics_ratio - (90.0 / 110.0)).abs() < 1e-12);
    assert_eq!(i1.distinct_resolved_gap_signatures, 2);
    assert!((i1.coverage_index - (90.0 / 110.0 + 2.0)).abs() < 1e-12);
    assert_eq!(i1.computed_at_commit, "deadbeef");

    // Nothing-attempted ⇒ honest 0.0 ratio (never a fabricated 1.0).
    let empty = compute_accretion_index(&[], "c");
    assert_eq!(empty.extracted_semantics_ratio, 0.0);
    assert_eq!(empty.coverage_index, 0.0);
}

// --- minimal temp-dir helper (no external dev-dep) -----------------

fn tempdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("usr-ledger-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}
