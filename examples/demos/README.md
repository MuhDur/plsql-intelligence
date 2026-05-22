# Persona-specific demo scripts

Short, copy-pasteable walkthroughs of `plsql-intelligence` for five
operator personas. Each script assumes a clean `git clone` and the
default no-DB demo path; the live-DB variant points at
`make demo-oracle-xe` (PLSQL-LAB-007) when the Oracle XE container is
needed.

| Persona | Script | What it demonstrates |
|---------|--------|---------------------|
| Release engineer | [release-engineer.md](release-engineer.md) | predict → plan → gate cycle on a hero-diff changeset |
| DBA | [dba.md](dba.md) | catalog snapshot inspection + invalidation prediction without live DB |
| Security auditor | [security.md](security.md) | privilege graph + cross-schema-write surface + VPD policy detection |
| Governance / compliance | [governance.md](governance.md) | doctor outputs + Trust Block + audit-log evidence |
| Rust developer | [rust-dev.md](rust-dev.md) | type-safe bindings to PL/SQL packages |

Plan reference: `plan.md` §6.2.8.1 (Synthetic Lab) — these scripts
exercise the L1 hero corpus + the lab fixtures.
