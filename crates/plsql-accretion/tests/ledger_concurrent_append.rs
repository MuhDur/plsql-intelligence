//! Multi-thread / multi-process safety for `Ledger::append` and
//! `AccretionLedger::append`.
//!
//! Without the advisory file lock added by the fix, two concurrent
//! `append()` calls against the same ledger directory both read the
//! same chain tip, both compute `parent = tip`, and both write —
//! producing a `ChainBroken` state at the second line that
//! `verify_chain` reports and which then bricks every future append.
//!
//! With the lock, the read→compute→write critical section is
//! serialized cross-thread (and, because the lock is a POSIX advisory
//! lock held on a sidecar file, cross-process too). Both writes still
//! land, the chain stays monotonic, and `verify_chain` returns Ok.

use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use plsql_accretion::{
    AccretionIndex, AccretionLedger, Ledger, LedgerBody, RepairClass,
};

fn unique_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let dir = std::env::temp_dir()
        .join("plsql-accretion-ledger-concurrent")
        .join(format!("{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("mkdir test dir");
    dir
}

fn body_with(sig: &str) -> LedgerBody {
    LedgerBody {
        estate_run_id: "run-xyz".to_string(),
        signature: sig.to_string(),
        diag_code: "PARSE-ANTLR4RUST-001".to_string(),
        antlr_rule_path: Some("text_scan>foo".to_string()),
        repair_class: RepairClass::Grammar,
        occurrence_count: 1,
        representative_min_fixtures: vec!["fix-1".to_string()],
        gate_verdict: None,
        landed_patch: None,
    }
}

#[test]
fn ledger_append_is_safe_under_concurrent_threads() {
    // Two threads, each appending a distinct body, releasing
    // simultaneously through a barrier. Without locking we observe
    // either a duplicate parent (ChainBroken) or a lost write; with
    // locking both writes land and the chain verifies.
    let dir = unique_dir("ledger");
    let n_threads = 8usize;
    let barrier = Arc::new(Barrier::new(n_threads));

    let handles: Vec<_> = (0..n_threads)
        .map(|i| {
            let dir = dir.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let ledger = Ledger::open(&dir).expect("open ledger");
                // All threads start their critical section at the
                // same instant — maximises the TOCTOU window.
                barrier.wait();
                ledger
                    .append(body_with(&format!("sig-{i}")))
                    .expect("append");
            })
        })
        .collect();
    for h in handles {
        h.join().expect("join");
    }

    let ledger = Ledger::open(&dir).expect("reopen");
    let entries = ledger.iter().expect("iter");
    assert_eq!(
        entries.len(),
        n_threads,
        "every concurrent append must land (no lost writes); got {entries:#?}"
    );
    ledger
        .verify_chain()
        .expect("chain must verify clean — concurrent appends must not break monotonicity");

    // The distinct signatures must all appear exactly once.
    let mut sigs: Vec<String> = entries.iter().map(|e| e.body.signature.clone()).collect();
    sigs.sort();
    sigs.dedup();
    assert_eq!(sigs.len(), n_threads, "every signature must land once");
}

#[test]
fn accretion_ledger_append_is_safe_under_concurrent_threads() {
    let dir = unique_dir("accretion");
    let n_threads = 8usize;
    let barrier = Arc::new(Barrier::new(n_threads));

    let handles: Vec<_> = (0..n_threads)
        .map(|i| {
            let dir = dir.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let ledger = AccretionLedger::open(&dir).expect("open accretion ledger");
                let index = AccretionIndex {
                    // Distinct payloads so idempotent-by-content does
                    // not collapse them — we want N concrete writes.
                    coverage_index: 1.0 + i as f64,
                    extracted_semantics_ratio: 1.0,
                    distinct_resolved_gap_signatures: i as u64,
                    computed_at_commit: format!("commit-{i}"),
                };
                barrier.wait();
                ledger
                    .append(&format!("HEAD-{i}"), index)
                    .expect("accretion append");
            })
        })
        .collect();
    for h in handles {
        h.join().expect("join");
    }

    let ledger = AccretionLedger::open(&dir).expect("reopen");
    let entries = ledger.iter().expect("iter");
    assert_eq!(
        entries.len(),
        n_threads,
        "every concurrent accretion append must land; got {entries:#?}"
    );
    ledger
        .verify_chain()
        .expect("accretion chain must verify clean under concurrency");
}

#[test]
fn ledger_append_lockfile_lives_next_to_the_ledger() {
    // The advisory lock is held on a sidecar file under the same dir
    // as the ledger so we don't try to lock the ledger file itself
    // (which is opened append-only on every append). The sidecar must
    // appear after at least one append.
    let dir = unique_dir("sidecar");
    let ledger = Ledger::open(&dir).expect("open");
    ledger.append(body_with("sig-once")).expect("append");
    let sidecar = dir.join("ledger.jsonl.lock");
    assert!(
        sidecar.exists(),
        "sidecar lock file must exist next to the ledger at {}",
        sidecar.display()
    );
}
