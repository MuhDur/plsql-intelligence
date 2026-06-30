# Oracle XE 23ai demo container

Spins up a local Oracle XE 23ai instance pre-loaded with the
synthetic-lab L1/L2/L3 fixtures from `corpus/lab/`. This is now a
downstream live-integration fixture for `oraclemcp` or other catalog
snapshot exporters; the normal `plsql-intelligence` engine path remains
offline and does not connect to it. (PLSQL-LAB-007 / oracle-yik.)

## What you get

- Oracle XE 23.7 (the `:lite` image, ~3 GB compressed).
- `FREEPDB1` pluggable database listening on `localhost:1521`.
- A `DEMO` schema seeded with `corpus/lab/l1/`, `l2/`, `l3/` fixtures.
- Persistent storage so subsequent `make demo-oracle-xe` runs reuse the
  loaded fixtures.

## One-shot quickstart

```sh
# From the repo root.
make demo-oracle-xe          # docker compose up -d + health-wait
make demo-oracle-xe-status   # tail the loader log
make demo-oracle-xe-down     # docker compose down (data volume preserved)
make demo-oracle-xe-purge    # docker compose down --volumes (start fresh)
```

The container is healthy when `make demo-oracle-xe-status` shows the
`Lab corpus loaded` line.

## Connection string

```
DEMO/DemoLab#2026@//localhost:1521/FREEPDB1
```

The SYSTEM password is `DemoPlsqlIntel#2026` by default. Override via:

```sh
ORACLE_PWD='your-password' make demo-oracle-xe
```

## License + provenance

Oracle XE 23ai is licensed under the **Oracle Free Use Terms and
Conditions (FUTC)**. Read <https://www.oracle.com/database/free/>
before redistributing the resulting image. Pulling the container image
requires `docker login container-registry.oracle.com` once and clicking
through the FUTC accept screen.

The container is **a development tool, not a production target**. Do
not embed secrets in the volume; the bind-mount is intentionally
read-only.

## Wiring with a live consumer

Once the container is healthy:

- Use `oraclemcp` for MCP/live-DB workflows against the `DEMO` schema.
- Use a catalog snapshot exporter to turn the schema into JSON, then feed
  that snapshot back to this repo's offline CLIs.
- For this repository alone, the same lab files are consumed directly as
  offline corpus fixtures; no Oracle connection is part of the normal
  build, test, or install path.

## Layered upgrade path

| Step | Command | What you get |
|------|---------|--------------|
| 0 | (none) | Offline analysis over source files and catalog snapshots. |
| 1 | `make demo-oracle-xe` | XE container + DEMO schema + lab fixtures. |
| 2 | Run `oraclemcp` or a snapshot exporter against the DSN | Live extraction outside this repo. |
| 3 | Feed the exported snapshot to `plsql` / `plsql-depgraph` | Offline impact and dependency analysis. |

## Plan + bead references

- `plan.md` §6.2.8.1 — Synthetic Lab layering (L0 → L3).
- `corpus/lab/README.md` — fixture inventory.
- `oracle-yik` PLSQL-LAB-007 — this bead.
- oraclemcp integration beads — live extraction and MCP end-to-end tests.
- `oracle-xcm` PLSQL-LAB-008 — persona-specific demo scripts (downstream
  of this bead).
