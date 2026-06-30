# Live Database Integration

Live Oracle connectivity is no longer implemented in this repository.

Use `oraclemcp` for:

- Oracle connection profiles and wallets.
- Live dictionary extraction.
- Agent-facing MCP tools.
- Database guard rails and audit runtime.
- Doctor checks for database connectivity.

Use `plsql-intelligence` for the offline engine after source and catalog
material is available:

1. Export source files and optional DBMS_METADATA DDL.
2. Produce or obtain a `CatalogSnapshot` JSON file when dictionary context
   is needed.
3. Run parser, graph, lineage, SAST, docs, bindings, or CI/CD prediction
   workflows locally.

The platform pages in this directory now document that handoff rather than
native database client setup.
