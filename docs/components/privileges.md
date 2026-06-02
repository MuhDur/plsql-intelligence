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
  (PLSQL-CAT-004). `ALL_TAB_PRIVS` has no user/role discriminator
  column, so each grantee is classified against
  `CatalogSnapshot.known_users` (loaded from `ALL_USERS`): a grantee
  that is not a known user is recorded as `Grantee::Role`. When the
  user set could not be loaded the grantee class is undetermined and is
  treated conservatively as a role (R13 — never a fail-toward-permissive
  direct user grant). Role-mediated grants are downgraded to Low
  confidence with a `RuntimeGrantOrRole` ambiguity by the resolver.
- `SemanticModel` — to detect `AUTHID DEFINER` vs `CURRENT_USER`
- `plsql-symbols` resolved bindings — to know what target object an
  identifier resolves to

## Pointers

- Source: `crates/plsql-privileges/src/`
- Plan: `plan.md` §9.3 (privilege model), §8 (catalog), §12 (SAST)
- Downstream: `plsql-scan`, `plsql-lineage`, `plsql-cicd`
