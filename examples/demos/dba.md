# DBA demo — catalog inspection without touching production

You want to know what `plsql-intelligence` thinks of your schema
*before* you connect it to live Oracle. The catalog snapshot model is
the answer.

## Setup

```sh
git clone <repo> && cd oracle
cargo build --workspace
```

## Step 1 — load a snapshot from a JSON export

A snapshot is the offline-first catalog model — every object, column,
constraint, index, trigger, synonym, dependency, grant, db_link,
edition, editioning_view, VPD policy, and comment from `ALL_*` views.

```sh
cargo test -p plsql-catalog --lib snapshot_round_trips -- --nocapture
```

The shape (`crates/plsql-catalog/src/lib.rs::CatalogSnapshot`) is
back-compat by design — every field added since 1.0 is behind
`#[serde(default)]` so older snapshots keep deserializing.

## Step 2 — inspect what was captured

The doctor surface answers "what's in here?":

```sh
cargo test -p plsql-catalog --lib doctor_report -- --nocapture
```

Prints `CatalogDoctorReport`: object counts per `ObjectType`, capability
warnings (e.g. `DBA_OBJECTS` unreachable → narrower coverage),
PL/Scope availability per schema, and `MissingPermissionReport` rows
naming exactly which dictionary views the catalog user couldn't query
and the suggested GRANT.

## Step 3 — predict invalidations from a changeset

You have a planned change. Will it cascade?

```sh
cargo run -p plsql-cicd --example predict_hero_diff
```

The `predict` function takes a `ChangeSet` + a `CatalogSnapshot` and
returns an `InvalidationPrediction` with a `completeness_profile`
flag — set to `SourceOnly`, `CatalogAware`, or `LiveSnapshot` to
match your run mode.

## Step 4 — flag VPD/RLS exposure

The catalog records every `ALL_POLICIES` row. To find policies on
your hot tables:

```sh
cargo test -p plsql-catalog --lib vpd_policies -- --nocapture
```

Each `VpdPolicy` row carries the function reference
(`function_owner.function_package.function_name`), the per-DML flags
(`on_select`/`on_insert`/`on_update`/`on_delete`), and the `enabled`
bit. Disabled policies show as deployment debt.

## Step 5 — edition tree (EBR shops)

`CatalogSnapshot::editions` is the database-wide edition tree from
`ALL_EDITIONS`. `SchemaCatalog::editioning_views` lists the
editioning-view → base-table pairs from `ALL_EDITIONING_VIEWS`. If
your shop uses EBR, your analysis run must filter by edition or risk
double-counting objects.

## What's NOT in the snapshot

- AWR / ASH history (Performance Tuning Guide).
- V$/GV$ dynamic performance views (instance-local, not catalog).
- Audit Vault / DB Vault state (security-options domain).

Pointers in `LOW-LEVEL-CATALOGS.md` (vendored under
`~/.claude/skills/oracle/` when present).

## Notes

- The catalog never silently invents data — every blind spot becomes
  a typed `UnknownReason` (R13 invariant). Read those before drawing
  conclusions.
