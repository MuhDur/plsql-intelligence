# Oracle XE 23ai demo container

Spins up a local Oracle XE 23ai instance pre-loaded with the
synthetic-lab L1/L2/L3 fixtures from `corpus/lab/`. Lets you exercise
`plsql-mcp`'s live-DB tool surface against a real Oracle without a paid
license. (PLSQL-LAB-007 / oracle-yik.)

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

## Wiring with plsql-mcp

Once the container is healthy:

```sh
cargo run -p plsql-mcp --features live-db -- doctor
```

The doctor surface should report `Instant Client present`, `connection
profile DEMO reachable`, and `permanently_read_only = false` (the
default safety posture). Use the `enable_writes` token flow before
issuing any DDL through MCP — see `crates/plsql-mcp/src/safety.rs`.

## Layered upgrade path

| Step | Command | What you get |
|------|---------|--------------|
| 0 | (none) | Static analysis only — `plsql-mcp doctor` with `--no-live-db`. |
| 1 | `make demo-oracle-xe` | XE container + DEMO schema + lab fixtures. |
| 2 | `cargo run -p plsql-mcp --features live-db -- serve` | MCP stdio loop bound to the container (post-`PLSQL-MCP-002`). |
| 3 | Call the change-impact tools against the same DSN | Change-impact tools (`what_breaks`, `release_gate`, etc.) — part of `plsql-mcp`. |

## Plan + bead references

- `plan.md` §6.2.8.1 — Synthetic Lab layering (L0 → L3).
- `corpus/lab/README.md` — fixture inventory.
- `oracle-yik` PLSQL-LAB-007 — this bead.
- `oracle-7nmg` PLSQL-MCP-LIVE-018 — integration tests E2E.
- `oracle-xcm` PLSQL-LAB-008 — persona-specific demo scripts (downstream
  of this bead).
