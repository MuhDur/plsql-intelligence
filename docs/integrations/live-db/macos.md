# Live Database Handoff on macOS

macOS live database setup belongs in `oraclemcp`.

This repository has no macOS-specific database client loader, wallet
resolver, stdio server, or database session runtime. Keep those pieces in
the MCP/live-DB repository and pass this engine only local inputs:

- PL/SQL source trees.
- DBMS_METADATA export directories.
- `CatalogSnapshot` JSON documents.

Useful local checks on macOS:

```sh
cargo test --workspace --all-targets
scripts/offline_boundary_lint.sh
scripts/offline_honesty_grep.sh
```
