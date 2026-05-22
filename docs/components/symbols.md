# plsql-symbols

Name resolution + overload identity for the semantic IR. Layer 2.

## Purpose

PL/SQL has a layered name-binding model: locals shadow package members,
which shadow same-schema objects, which can be reached via synonyms or
DB-link qualified names. `plsql-symbols` is the resolver that walks an
`SemanticModel` and decides who each identifier *actually* refers to.

## Surface

| Type | Purpose |
|------|---------|
| `DeclTable` | Materialised list of every declaration in a scope chain |
| `SymbolEntry` | Resolved binding (symbol id + decl site + visibility) |
| `OverloadSet` | Multi-arity / multi-signature group keyed by name |
| `ResolutionStrategy` | Records *how* a binding was reached (LocalLexical, PackageMemberLookup, SameSchemaLookup, SynonymExpansion, CatalogLookup, …) |

## Resolution strategies (plan §9.2)

1. **Local lexical** — current scope chain
2. **Package-internal** — within same package body
3. **Same-schema** — top-level objects in the calling schema
4. **Synonym expansion** — private then public synonyms
5. **DB-link** — qualified `name@dblink` traversal
6. **Catalog lookup** — fallback to the `CatalogSnapshot` for `%TYPE`,
   `%ROWTYPE`, and overload signatures we can't derive from source
7. **Manual mapping** — user-supplied overrides for cases we lose

Each strategy records its choice on the resolved binding so downstream
tools can show "we found this via synonym expansion (confidence: Medium)".

## Pointers

- Source: `crates/plsql-symbols/src/`
- Plan: `plan.md` §9.2 (resolution model), §8 (catalog), §17 (Oracle hazards)
- Downstream: `plsql-privileges`, `plsql-sqlsem`, `plsql-depgraph`
