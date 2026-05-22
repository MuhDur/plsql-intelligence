//! P3 integration test (spec §10, P3 exit criterion): drive the
//! real `usr-loop cluster` + `usr-loop ledger verify` binaries over
//! a PUBLIC synthetic fixture (NEVER the private estate — this test must
//! stay public).
//!
//! Mirrors `scan_integration.rs`: a single committed public fixture
//! (`corpus/synthetic/l2/syn_employees.sql`) is isolated in a temp
//! estate so the scan never touches the overflow-triggering sibling.

use std::process::Command;

fn isolated_estate() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let fixture = repo_root.join("corpus/synthetic/l2/syn_employees.sql");
    assert!(fixture.exists(), "public synthetic fixture missing");
    let estate = std::env::temp_dir().join(format!(
        "usr_loop_p3_it_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&estate).expect("mk estate");
    std::fs::copy(&fixture, estate.join("syn_employees.sql")).expect("copy fixture");
    estate
}

#[test]
fn usr_loop_cluster_emits_valid_envelope_and_ledger_verifies() {
    let estate = isolated_estate();
    let bin = env!("CARGO_BIN_EXE_usr-loop");

    // Run the whole test from a private temp cwd so the `.usr/`
    // ledger/fixtures it writes never collide with the repo's.
    let cwd = std::env::temp_dir().join(format!("usr_loop_p3_cwd_{}", std::process::id()));
    std::fs::create_dir_all(&cwd).expect("mk cwd");

    // --- cluster ---
    let out = Command::new(bin)
        .current_dir(&cwd)
        .args(["--robot-json", "cluster"])
        .arg(&estate)
        .output()
        .expect("spawn usr-loop cluster");
    assert!(
        out.status.success(),
        "usr-loop cluster non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(v["format"], "plsql-robot-json");
    assert_eq!(v["schema_id"], "plsql.usr.gap_cluster");
    assert_eq!(v["schema_version"]["major"], 1);
    let clusters = v["payload"].as_array().expect("payload array");
    assert!(!clusters.is_empty(), "expected >=1 GapCluster");
    for c in clusters {
        assert!(c["signature"].is_string());
        assert!(c["occurrence_count"].as_u64().unwrap() >= 1);
        assert!(c["representative_min_fixtures"].is_array());
    }
    // Privacy spot-check.
    for forbidden in ["employees_syn", "SYNONYM", "employees"] {
        assert!(
            !stdout.contains(forbidden),
            "I-PRIVACY VIOLATION: {forbidden:?} leaked into cluster output"
        );
    }

    // --- ledger append then verify ---
    let out = Command::new(bin)
        .current_dir(&cwd)
        .args(["--robot-json", "ledger", "append"])
        .arg(&estate)
        .output()
        .expect("spawn ledger append");
    assert!(
        out.status.success(),
        "ledger append non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(bin)
        .current_dir(&cwd)
        .args(["--robot-json", "ledger", "verify"])
        .output()
        .expect("spawn ledger verify");
    assert!(
        out.status.success(),
        "ledger verify must pass on a freshly-appended ledger: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("verify json");
    assert_eq!(v["status"], "ok");

    // --- tamper-evidence: corrupt one byte → verify fails ---
    let ledger_file = cwd.join(".usr/ledger/ledger.jsonl");
    let original = std::fs::read_to_string(&ledger_file).expect("read ledger");
    assert!(!original.is_empty(), "ledger should have entries");
    let corrupted = original.replacen("PARSE", "PARSX", 1);
    if corrupted != original {
        std::fs::write(&ledger_file, &corrupted).expect("corrupt");
        let out = Command::new(bin)
            .current_dir(&cwd)
            .args(["ledger", "verify"])
            .output()
            .expect("spawn ledger verify (corrupt)");
        assert!(
            !out.status.success(),
            "verify MUST fail on a corrupted ledger (tamper-evidence, I-PROVENANCE)"
        );
    }
}
