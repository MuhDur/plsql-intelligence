//! One-off generator for `corpus/lab/snapshots/` pre-computed fixtures
//! (`PLSQL-LAB-005`).
//!
//! Run with:
//!
//! ```text
//! cargo run -p plsql-catalog --example generate_lab_snapshots
//! ```
//!
//! Emits two snapshot JSON documents into `corpus/lab/snapshots/` so
//! downstream beads (LIN, CICD, MCP) can run against a stable estate
//! without needing a live Oracle connection.

use std::path::Path;

use plsql_catalog::{export_snapshot_to_json, synthetic::billing_schema};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = Path::new("corpus/lab/snapshots");
    std::fs::create_dir_all(out_dir)?;

    let l1 = billing_schema();
    export_snapshot_to_json(&l1, &out_dir.join("l1_billing.json"))?;
    println!(
        "generated {} (schemas: {}, objects: {})",
        out_dir.join("l1_billing.json").display(),
        l1.schemas.len(),
        l1.schemas.values().map(|s| s.objects.len()).sum::<usize>(),
    );

    // L2 currently re-uses billing_schema until the synthetic L2 corpus
    // grows up; the file slot is reserved so downstream tools can pin a
    // larger snapshot once it exists.
    export_snapshot_to_json(&l1, &out_dir.join("l2_billing_extended.json"))?;
    println!(
        "generated {} (schemas: {})",
        out_dir.join("l2_billing_extended.json").display(),
        l1.schemas.len()
    );

    // PL/Scope skeleton — empty fixture with the right shape, so
    // downstream CAT-011 / SYM consumers can build against it without
    // a live DB. The full PL/Scope snapshot will be added once a real
    // extraction fixture exists.
    let plscope_dir = Path::new("corpus/lab/plscope");
    std::fs::create_dir_all(plscope_dir)?;
    let plscope_path = plscope_dir.join("l1_billing.json");
    let body = serde_json::json!({
        "schema_id": "plsql.lab.plscope",
        "schema_version": "1.0.0",
        "schemas": {
            "BILLING": {
                "availability": "identifiers_and_statements",
                "identifiers": [],
                "references": [],
                "statements": [],
                "collected_at": "2026-05-15T12:00:00Z",
                "source_hash": null,
                "warnings": []
            }
        }
    });
    std::fs::write(&plscope_path, serde_json::to_string_pretty(&body)?)?;
    println!("generated {}", plscope_path.display());

    Ok(())
}
