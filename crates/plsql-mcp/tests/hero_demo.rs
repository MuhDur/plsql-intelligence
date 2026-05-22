//! Hero-demo end-to-end golden test (§1.4).
//!
//! Drives the `what_breaks` change-analysis tool over the canonical
//! "someone is about to `ALTER TABLE … DROP COLUMN`" scenario and
//! golden-pins the payload. Golden = deterministic structural
//! assertions on the canonical JSON (workspace idiom, no `insta`).
//! What is pinned, and why each matters:
//!
//! * a destructive table DDL is carried through to a stable
//!   `InvalidationPrediction` whose `mode` is echoed;
//! * the payload is **byte-identical across runs** (the demo must
//!   never flake on stage);
//! * it round-trips through JSON (the MCP wire contract).
//!
//! The exact `predicted_invalidations` set is `plsql-cicd`'s own
//! tested concern; this test pins the *contract* (determinism +
//! wire-stability), not predict()'s internal rules.

use plsql_core::SymbolInterner;
use plsql_mcp::run_what_breaks;

use plsql_cicd::{ChangeSet, ChangedObject, ChangedObjectKind, PredictMode};

/// The §1.4 hero scenario: `ALTER TABLE HR.EMPLOYEES DROP COLUMN …`
/// — a destructive table DDL the agent wants `what_breaks` on.
fn hero_changeset() -> ChangeSet {
    let mut interner = SymbolInterner::new();
    let owner = interner.intern_schema_name("HR").expect("schema");
    let name = interner
        .intern("EMPLOYEES")
        .map(plsql_core::ObjectName::from)
        .expect("object");
    ChangeSet {
        origin: None,
        objects: vec![ChangedObject {
            owner,
            name,
            kind: ChangedObjectKind::TableDestructiveDdl,
            new_hash: None,
            previous_hash: None,
            file_paths: vec![std::path::PathBuf::from("hr/employees.sql")],
            uncertainties: vec![],
        }],
        unclassified_files: vec![],
    }
}

#[test]
fn hero_demo_what_breaks_golden_payload() {
    let cs = hero_changeset();
    let pred = run_what_breaks(&cs, PredictMode::CatalogAware);

    // Contract: the requested mode is echoed verbatim.
    assert_eq!(pred.mode, PredictMode::CatalogAware);

    // The destructive DDL is carried into a well-formed
    // prediction that survives the MCP JSON wire round-trip.
    let json = serde_json::to_string(&pred).expect("payload serializes");
    assert!(json.contains("predicted_invalidations"));
    assert!(json.contains("recompile_order"));
    let back: plsql_cicd::InvalidationPrediction =
        serde_json::from_str(&json).expect("payload round-trips");
    assert_eq!(back, pred);
}

#[test]
fn hero_demo_payload_is_byte_identical_across_runs() {
    // The stage demo must never flake: identical input ->
    // identical bytes, every invocation.
    let cs = hero_changeset();
    let a = run_what_breaks(&cs, PredictMode::CatalogAware);
    let b = run_what_breaks(&cs, PredictMode::CatalogAware);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "what_breaks must be deterministic for the hero demo"
    );
}

#[test]
fn hero_demo_distinct_from_empty_changeset() {
    // Sanity: the destructive-DDL scenario is not silently
    // equivalent to "nothing changed" — the demo would be
    // worthless if a DROP COLUMN predicted the same as an empty
    // changeset.
    let empty = run_what_breaks(&ChangeSet::empty(), PredictMode::CatalogAware);
    let hero = run_what_breaks(&hero_changeset(), PredictMode::CatalogAware);
    assert_eq!(
        empty.invalidation_count(),
        0,
        "empty changeset breaks nothing"
    );
    // The hero changeset carries a destructive object; even if
    // graphless prediction yields no downstream invalidations,
    // the input object set differs, so the payloads must differ.
    assert_ne!(
        serde_json::to_string(&empty).unwrap(),
        serde_json::to_string(&hero).unwrap(),
        "DROP COLUMN scenario must not serialize identically to an empty changeset"
    );
}
