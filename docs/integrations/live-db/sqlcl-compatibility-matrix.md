# SQLcl MCP Compatibility

SQLcl compatibility belongs to the live MCP layer.

Use `oraclemcp` when comparing Oracle database connection behavior,
wallets, guarded writes, audit behavior, and SQLcl-style MCP operations.
Use this repository when comparing offline PL/SQL intelligence:

| Area | This repository |
| --- | --- |
| Parser and recovery | PL/SQL parser frontend with backend isolation |
| Catalog context | Offline `CatalogSnapshot` model |
| Dependency graph | Evidence-bearing object and column edges |
| Lineage | Impact, callers, dependencies, and change classification |
| SAST | Static rule surface over PL/SQL source and semantic facts |
| CI/CD planning | Change sets, invalidation prediction, release gates |

The live layer may call into these crates, but Oracle database sessions and
MCP transport remain outside this repository.
