#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_plsql")
}

fn fixture_root(label: &str) -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(format!(
            "plsql-cli-{label}-{}-{nanos}-{id}",
            std::process::id()
        ))
}

#[test]
fn predict_robot_json_accepts_changeset_source_after_subcommand() {
    let root = fixture_root("direct");
    std::fs::create_dir_all(&root).expect("fixture root");
    let changeset = root.join("changeset.json");
    std::fs::write(
        &changeset,
        r#"{
  "origin": null,
  "objects": [
    {
      "owner": 0,
      "name": 1,
      "kind": "PackageSpec",
      "new_hash": null,
      "previous_hash": null,
      "file_paths": [],
      "uncertainties": []
    }
  ],
  "unclassified_files": []
}
"#,
    )
    .expect("write changeset");

    let out = Command::new(bin())
        .args([
            "predict",
            "--robot-json",
            "--source-kind",
            "changeset-json",
            changeset.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("run plsql predict");

    assert!(out.status.success(), "predict exits 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = stdout.trim_end();
    assert!(
        !trimmed.contains('\n'),
        "robot-json stdout must be single-line: {trimmed:?}"
    );
    let value: serde_json::Value = serde_json::from_str(trimmed).expect("json stdout");
    assert_eq!(value["format"], "plsql-robot-json");
    assert_eq!(value["schema_id"], "plsql.cicd.change_impact");
    assert_eq!(value["schema_version"]["major"], 1);
    assert_eq!(value["payload"]["summary"]["invalidation_count"], 1);
    assert_eq!(value["payload"]["summary"]["max_distance"], 1);
}

#[test]
fn predict_robot_json_composes_offline_lineage_impacts() {
    let root = fixture_root("lineage");
    std::fs::create_dir_all(&root).expect("fixture root");
    let changeset = root.join("changeset.json");
    let impact = root.join("impact.json");
    let metadata = root.join("metadata.json");

    std::fs::write(
        &changeset,
        r#"{
  "origin": null,
  "objects": [
    {
      "owner": 0,
      "name": 1,
      "kind": "TableDestructiveDdl",
      "new_hash": null,
      "previous_hash": null,
      "file_paths": [],
      "uncertainties": []
    }
  ],
  "unclassified_files": []
}
"#,
    )
    .expect("write changeset");
    std::fs::write(
        &impact,
        r#"{
  "query": {
    "anchor": "BILLING.CUSTOMERS",
    "direction": "downstream",
    "max_depth": null,
    "min_confidence": null
  },
  "edges": [],
  "unknown_edges": [],
  "affected_nodes": [
    {
      "logical_id": "BILLING.REPORT_PKG",
      "hops": 1,
      "path_confidence": "exact"
    },
    {
      "logical_id": "BILLING.REPORT_VIEW",
      "hops": 2,
      "path_confidence": "heuristic"
    }
  ]
}
"#,
    )
    .expect("write impact");
    std::fs::write(
        &metadata,
        r#"{
  "objects": [
    {
      "logical_id": "BILLING.REPORT_PKG",
      "owner_symbol": 0,
      "name_symbol": 2,
      "object_type": "PACKAGE",
      "force_compile": true
    },
    {
      "logical_id": "BILLING.REPORT_VIEW",
      "owner_symbol": 0,
      "name_symbol": 3,
      "object_type": "VIEW",
      "force_compile": true
    }
  ]
}
"#,
    )
    .expect("write metadata");

    let out = Command::new(bin())
        .args([
            "predict",
            "--robot-json",
            "--source-kind",
            "changeset-json",
            "--lineage-impact",
            impact.to_str().expect("utf8 impact"),
            "--lineage-metadata",
            metadata.to_str().expect("utf8 metadata"),
            changeset.to_str().expect("utf8 changeset"),
        ])
        .output()
        .expect("run plsql predict");

    assert!(out.status.success(), "predict exits 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json stdout");
    assert_eq!(value["payload"]["summary"]["invalidation_count"], 3);
    assert_eq!(value["payload"]["summary"]["recompile_count"], 2);
    assert_eq!(value["payload"]["summary"]["max_distance"], 2);
    assert_eq!(
        value["payload"]["attributes"]["lineage.transitive_invalidations"],
        "2"
    );
}

#[test]
fn doctor_robot_json_is_single_json_object() {
    let out = Command::new(bin())
        .args(["doctor", "--robot-json"])
        .output()
        .expect("run plsql doctor");

    assert!(out.status.success(), "doctor exits 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let trimmed = stdout.trim_end();
    assert!(
        !trimmed.contains('\n'),
        "doctor --robot-json must be single-line"
    );
    let value: serde_json::Value = serde_json::from_str(trimmed).expect("json stdout");
    assert_eq!(value["binary"], "plsql");
    assert_eq!(value["status"], "ok");
    assert_eq!(
        value["schemas"]["change_impact"]["id"],
        "plsql.cicd.change_impact"
    );
}

#[test]
fn missing_changeset_source_emits_robot_error_envelope() {
    let out = Command::new(bin())
        .args([
            "predict",
            "--robot-json",
            "/nonexistent-plsql-changeset-source.json",
        ])
        .output()
        .expect("run plsql predict");

    assert!(!out.status.success(), "missing source exits nonzero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("changeset source does not exist"),
        "stderr carries human diagnostic: {stderr}"
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json stdout");
    assert_eq!(value["format"], "plsql-robot-json");
    assert_eq!(value["schema_id"], "plsql.cicd.error_envelope");
    assert_eq!(value["payload"]["kind"], "error");
    assert_eq!(value["payload"]["code"], "changeset_source_missing");
}
