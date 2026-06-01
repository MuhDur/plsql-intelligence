# Least-privilege Oracle user for oraclemcp (read-only profiles)

The **DB-privilege ceiling is the only hard boundary** (plan §5.5, §6.3 layer A).
For shared / untrusted / production-read targets, connect with a least-privilege
Oracle user so write/DDL/admin are impossible at the engine regardless of any
server-side state.

## Recommended minimal grants (read-only)

```sql
-- A dedicated, read-only service user for oraclemcp.
CREATE USER oraclemcp_ro IDENTIFIED BY <managed-secret>;
GRANT CREATE SESSION TO oraclemcp_ro;

-- Dictionary read for introspection (prefer scoping over SELECT ANY DICTIONARY
-- where your security posture requires it):
GRANT SELECT ANY DICTIONARY TO oraclemcp_ro;   -- or specific ALL_*/DBA_* grants

-- Object SELECT only on the schemas the agent should read:
GRANT SELECT ON app_owner.some_table TO oraclemcp_ro;
-- (repeat per object, or use a read-only role)
```

**Never** grant `DBA`, `RESOURCE`, `DROP ANY`, `CREATE ANY`, write privileges, or
roles that confer them. Beware indirect write capability via: non-default roles a
`SET ROLE` could enable, `PUBLIC` grants, `ANY` system privileges, proxy /
`CONNECT THROUGH`, and `EXECUTE` on `DEFINER`-rights packages that write — the
server cannot enumerate all of these (undecidable in practice), which is exactly
why the least-privilege user is the real wall.

## How the server reinforces this

- Mark the connection profile `protected = true` (production): `max_level` is
  pinned at `READ_ONLY` and immutable for the process; `SET ROLE` and
  non-allowlisted `ALTER SESSION` are blocked (so a session can't enable a
  write-bearing role post-connect).
- The server issues `SET TRANSACTION READ ONLY` whenever the level is
  `READ_ONLY` (layer B), so a *misclassified* direct DML still raises
  `ORA-01456` at the engine.
- The fail-closed classifier (layer C) refuses anything not provably read-only.

Three independent locks (least-privilege user + `protected` ceiling + standby
auto-detection) mean no single failure grants write access to a protected target.
