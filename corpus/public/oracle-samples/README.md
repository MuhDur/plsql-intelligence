# Oracle sample schemas — HR / OE / SH (DDL subset)

Vendored DDL-only subset of the
[`oracle-samples/db-sample-schemas`](https://github.com/oracle-samples/db-sample-schemas)
repository (MIT license). The populate scripts and binary data files are
intentionally not vendored — code intelligence work needs schema shape, not
the bundled row data.

## Files

- `human_resources/hr_create.sql` — full HR schema DDL (tables, constraints, triggers).
- `human_resources/hr_code.sql` — HR PL/SQL code (procedures + functions).
- `order_entry/oe_views.sql` — OE views.
- `order_entry/coe_v3.sql` — OE structure v3.
- `order_entry/ccus_v3.sql` — OE customers.
- `order_entry/cord_v3.sql` — OE order types.
- `order_entry/cmnt_v3.sql` — OE comments.
- `sales_history/sh_create.sql` — SH schema DDL (tables + partitions).

## Refresh

Each ingested file's `[[file]]` entry in `corpus/manifest.toml` carries the
`source_url` for the raw GitHub content. The README's per-file `fetched_on`
date reflects the snapshot timestamp.

## Note on the `populate` scripts

`hr_populate.sql` (41kB), `sh_install.sql` cohorts, and the OE binary `.dmp`
artifacts are NOT vendored. Bring them in only if a downstream bead requires
actual row data — most parser / catalog / SAST workflows only need schema
DDL.
