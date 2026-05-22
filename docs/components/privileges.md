# plsql-privileges

Definer / invoker rights modelling + grant-aware reachability. Layer 2.

## Purpose

Oracle's privilege model is richer than most languages' — the same call
can succeed or fail depending on `AUTHID DEFINER` vs `AUTHID CURRENT_USER`,
roles in effect, and whether the caller can `SELECT` the target table.
`plsql-privileges` is where the workspace tracks that, so consumers like
`plsql-scan` (SAST) and `plsql-lineage` can answer "can role X call
procedure Y today?" without re-deriving the rules.

## Surface

| Type | Purpose |
|------|---------|
| `AuthidMode` | `Definer` or `CurrentUser` |
| `GrantSet` | What a grantee can do on a target object |
| `RoleClosure` | Transitive role membership for a session |
| `PrivilegeQuery` | "Can `<grantee>` perform `<action>` on `<object>`?" |
| `MissingPrivilegeReason` | Why a query came back negative (no grant, role not active, …) |

## Inputs

- `CatalogSnapshot.grants` — vendored from `ALL_TAB_PRIVS`
  (PLSQL-CAT-004) and `ALL_ROLE_PRIVS`
- `SemanticModel` — to detect `AUTHID DEFINER` vs `CURRENT_USER`
- `plsql-symbols` resolved bindings — to know what target object an
  identifier resolves to

## Pointers

- Source: `crates/plsql-privileges/src/`
- Plan: `plan.md` §9.3 (privilege model), §8 (catalog), §12 (SAST)
- Downstream: `plsql-scan`, `plsql-lineage`, `plsql-cicd`
