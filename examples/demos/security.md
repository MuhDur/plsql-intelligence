# Security auditor demo — privilege graph + cross-schema-write surface

You're auditing a PL/SQL codebase for least-privilege violations,
unexpected cross-schema writes, and dynamic SQL injection risk. The
`plsql-privileges` and `plsql-catalog` crates are the entry points.

## Setup

```sh
git clone <repo> && cd oracle
cargo build --workspace
```

## Step 1 — load the privilege model

The privilege model resolves `ALL_TAB_PRIVS` + `DBA_ROLE_PRIVS` +
`DBA_SYS_PRIVS` + AUTHID flags from PL/SQL source into a
`PrivilegeModel` (`crates/plsql-privileges/src/model.rs`):

```sh
cargo test -p plsql-privileges --lib -- --nocapture
```

Outputs:
- `privileges` — every resolved grant on every object.
- `public_grants` — grants to `PUBLIC` (a flag for review).
- `access_control` — `ACCESSIBLE BY` clause entries.
- `cross_schema_writes` — every place a unit in schema A writes to a
  table in schema B, with `runtime_ambiguity: Option<UnknownReason>`
  when the resolution couldn't be done statically.
- `synonym_paths` — privilege resolution that traversed a synonym;
  public synonyms are an audit hotspot (anyone with `CREATE PUBLIC
  SYNONYM` can retarget).

## Step 2 — read the doctor surface

```sh
cargo test -p plsql-privileges --lib doctor -- --nocapture
```

`PrivilegeDoctorReport` aggregates:
- `posture` — Clean / Caution / Unknown.
- `cross_schema_writes_total` + `cross_schema_writes_ambiguous`.
- `authorization_ambiguities_total` — authorizations that flip on
  runtime role state.
- `public_synonym_paths` — count.
- `remediation_hints` — one-line operator hints per finding class.

## Step 3 — VPD/RLS policies

`SchemaCatalog::vpd_policies` carries every `ALL_POLICIES` row. A
VPD-protected table reads through a generated `WHERE` clause; static
analysis without VPD will overstate reach.

```sh
cargo test -p plsql-catalog --lib vpd_policies -- --nocapture
```

Each `VpdPolicy` records the policy function
(`function_owner.function_package.function_name`) so you can pivot
into the source of the predicate generator.

## Step 4 — dynamic SQL evidence (planned)

`plsql-symbols::DynamicSqlEvidence` records every `EXECUTE IMMEDIATE`
fragment with bind-variable usage and DBMS_ASSERT detection. Today's
state: catalog + privilege model are shipped; the dynamic-SQL evidence
model is gated on PLSQL-SYM-005 (oracle-uie).

## Step 5 — SAST rule pack (planned)

The SAST rule pack (PLSQL-SAST-001..028) will land on top of the
`FactStore`. Rule families:
- `SEC001` EXECUTE IMMEDIATE injection
- `SEC002` DBMS_SQL.PARSE
- `SEC003` hardcoded credentials
- `SEC004` AUTHID CURRENT_USER caveats
- `SEC005` public synonym on sensitive objects
- `SEC006` GRANT TO PUBLIC
- `SEC007` REF CURSOR return
- `QUAL001..008` quality rules
- `PERF001..003` performance rules

Not yet shipped — track progress via `br list --status=open --json | jq` on `component:sast-engine`.

## Notes

- Privilege resolution that depends on runtime role state surfaces as
  `runtime_ambiguity: Some(UnknownReason::RuntimeGrantOrRole)`. Treat
  these as audit-required, not analysis errors.
- Public synonyms are a privilege-escalation hotspot. The privilege
  doctor counts them in `public_synonym_paths` and prompts review.
