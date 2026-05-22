//! Integration test (spec §10, P1 exit criterion): drive the real
//! `usr-loop scan` binary over a PUBLIC synthetic fixture (NEVER
//! the private estate — this test must stay public) and assert ≥1 GapRecord
//! arrives in a valid `plsql.usr.gap_record` v1 envelope.
//!
//! `corpus/synthetic/l2/syn_employees.sql` is a committed public
//! fixture (`CREATE OR REPLACE SYNONYM …`) that the engine lowers
//! to `IR_DDL_NOT_LOWERED` diagnostics — a deterministic, source-
//! free gap source. The whole `corpus/synthetic` tree is *not*
//! scanned: a sibling fixture (`pkg_error_handling.pkb`) trips a
//! pre-existing `plsql-engine` parser stack overflow (an honest
//! external gap, unrelated to plsql-accretion — reported in P1).

use std::process::Command;

#[test]
fn usr_loop_scan_emits_valid_envelope_over_public_synthetic() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // tools/usr-loop → repo root.
    let repo_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let fixture = repo_root.join("corpus/synthetic/l2/syn_employees.sql");
    assert!(
        fixture.exists(),
        "public synthetic fixture missing: {}",
        fixture.display()
    );

    // Isolate the single fixture in a fresh temp estate so the
    // scan never touches the overflow-triggering sibling. The estate
    // is unique per (pid, nanos) so two test binaries running in
    // parallel under cargo never share it.
    let uniq = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let estate = std::env::temp_dir().join(format!("usr_loop_it_estate_{uniq}"));
    std::fs::create_dir_all(&estate).expect("mk estate");
    std::fs::copy(&fixture, estate.join("syn_employees.sql")).expect("copy fixture");

    // Run the spawned binary from a private temp cwd. `usr-loop scan`
    // derives its `.usr/fixtures` persist root from `current_dir()`
    // (tools/usr-loop/src/main.rs `run_scan` →
    // `minimize_estate_gaps` → `persist_min_fixture`), so WITHOUT
    // this the binary writes privacy-proven fixtures into the SHARED
    // repo `<repo>/.usr/fixtures`, racing `plsql-accretion`'s
    // parallel `privacy.rs` snapshot reader (the PLSQL-USR-001
    // I-DETERMINISM flake). Mirrors the isolation already used by
    // `cluster_ledger_integration.rs`. Production default is
    // unchanged: `cargo run` from the repo still uses `<repo>/.usr`.
    let cwd = std::env::temp_dir().join(format!("usr_loop_it_cwd_{uniq}"));
    std::fs::create_dir_all(&cwd).expect("mk cwd");

    let bin = env!("CARGO_BIN_EXE_usr-loop");
    let out = Command::new(bin)
        .current_dir(&cwd)
        .args(["--robot-json", "scan"])
        .arg(&estate)
        .output()
        .expect("spawn usr-loop");

    assert!(
        out.status.success(),
        "usr-loop scan exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout not JSON ({e}): {stdout}"));

    assert_eq!(v["format"], "plsql-robot-json");
    assert_eq!(v["schema_id"], "plsql.usr.gap_record");
    assert_eq!(v["schema_version"]["major"], 1);
    let payload = v["payload"].as_array().expect("payload array");
    assert!(
        !payload.is_empty(),
        "expected >=1 GapRecord from the public synthetic fixture"
    );

    // Privacy spot-check: no fixture identifier may appear anywhere
    // in the serialized batch.
    for forbidden in ["employees_syn", "SYNONYM", "employees"] {
        assert!(
            !stdout.contains(forbidden),
            "I-PRIVACY VIOLATION: {forbidden:?} leaked into scan output"
        );
    }
    for rec in payload {
        assert!(rec["signature"].is_string());
        assert!(rec["diag_code"].is_string());
        assert_eq!(rec["occurrence_count"], 1);
        assert!(rec["span_shape"].is_array());
    }
}
