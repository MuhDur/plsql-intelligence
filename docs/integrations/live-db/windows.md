# Live Database Handoff on Windows

Windows live database setup belongs in `oraclemcp`.

This repository has no Windows-specific database client loader, wallet
resolver, stdio server, or database session runtime. Keep those pieces in
the MCP/live-DB repository and pass this engine only local inputs:

- PL/SQL source trees.
- DBMS_METADATA export directories.
- `CatalogSnapshot` JSON documents.

Useful local checks on Windows:

```powershell
cargo test --workspace --all-targets
scripts/offline_boundary_lint.sh
scripts/offline_honesty_grep.sh
```
