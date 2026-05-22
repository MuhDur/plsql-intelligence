# corpus/lab/ — synthetic lab corpora + pre-computed fixtures

Pre-computed artifacts used by downstream beads (LIN, CICD, MCP) so they
can run against a stable estate without needing a live Oracle connection.

## Hero Scenarios

Two hero scenarios live here, serving different product narratives:

| Hero | Dir | Scenario | Showcases |
|------|-----|----------|-----------|
| **§1.4 DROP COLUMN** (PLSQL-LAB-008) | `hero_diff_dropcol/` | `ALTER TABLE customers DROP COLUMN legacy_segment` | **Commercial nucleus** — the homepage headline; 3 dependents go INVALID |
| LAB-002 param rename | `hero_diff/` | `p_emp_id → p_employee_id` on `pkg_employee_mgmt` | Lineage engine showcase — named-notation breakage detection |

The `hero_diff/` scenario (LAB-002) is a valid L1 lineage fixture with its own golden — **do not delete or modify it**.

## Corpus Levels

- `l1/` — L1 hero corpus: `customers` table + `legacy_segment` column + 3 dependent
  PL/SQL objects. Loaded by `examples/oracle-xe/setup.sh` into the DEMO schema on
  first boot (PLSQL-LAB-008).
- `l3/` — L3 realism fixtures (PLSQL-LAB-006): Oracle-specific complexity patterns
  (DB links, wrapped code, conditional compilation, opaque dynamic SQL).

## Snapshot Artifacts

- `snapshots/l1_billing.json` — synthetic billing schema (4 tables + 1
  view + 2 packages + 1 procedure + 1 sequence). Schema version
  `plsql.catalog.snapshot v1.1.0`.
- `snapshots/l2_billing_extended.json` — slot reserved for the L2
  estate; currently re-uses the L1 snapshot so downstream consumers can
  pin a stable filename until the L2 corpus grows.
- `plscope/l1_billing.json` — PL/Scope skeleton with empty
  identifier / reference / statement arrays per the schema documented
  in `plsql-catalog` `PlScopeSnapshot`.

## Refresh

```sh
cargo run -p plsql-catalog --example generate_lab_snapshots
```

The example uses `plsql_catalog::synthetic::billing_schema()` as the
source of truth so every refresh produces byte-identical output until
the synthetic builder itself changes.

## Round-trip guarantee

The catalog `catalog_snapshot_round_trips_through_versioned_json_document`
test in `crates/plsql-catalog/src/lib.rs` exercises the same
`export_snapshot_to_json` → `load_snapshot_from_json` path used by this
generator, so any future schema bump breaks the test before it breaks
downstream consumers.
