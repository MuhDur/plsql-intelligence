# plsql-intelligence Architecture

Status: offline engine pivot, June 2026.

`plsql-intelligence` is a Rust workspace for offline Oracle PL/SQL code
intelligence. It parses source, loads catalog snapshots, builds semantic
models, computes dependency and lineage data, and emits documentation,
binding, SAST, and CI/CD planning artifacts.

The repository no longer ships an MCP server or live Oracle connection
runtime. Those concerns live in `oraclemcp`. This repository provides the
offline engine and reusable crates that `oraclemcp` can embed or call.

## Boundary

The core boundary is intentionally simple:

- This repo: offline PL/SQL parser, catalog snapshot model, semantic
  analysis, graph, lineage, SAST, docs, bindings, CI/CD prediction, and
  accretion tooling.
- `oraclemcp`: MCP transport, live Oracle sessions, connection profiles,
  guard rails, audit runtime, and agent-facing database tools.
- Exchange format: source trees, DBMS_METADATA exports, and
  `CatalogSnapshot` JSON documents.

No first-party `plsql-*` crate may depend on Oracle drivers, MCP runtime
crates, async server stacks, or `oraclemcp-*` crates. The offline boundary
is enforced by `scripts/offline_boundary_lint.sh`.

## Layers

The workspace follows the layer graph in `plan.md`:

| Layer | Role | Main crates |
| --- | --- | --- |
| 0 | Shared foundations | `plsql-core`, `plsql-output`, `plsql-render`, `plsql-store` |
| 1 | Parser frontend and backend isolation | `plsql-parser`, `plsql-parser-antlr` |
| 1.5 | Project and catalog context | `plsql-project`, `plsql-catalog` |
| 2 | Semantic model and dependency facts | `plsql-ir`, `plsql-symbols`, `plsql-privileges`, `plsql-depgraph`, `plsql-sast` |
| 2.5 | Analysis orchestration | `plsql-engine` |
| 3 | Product outputs | `plsql-doc`, `plsql-bindgen` |
| 4 | Lineage and impact | `plsql-lineage` |
| 5 | CI/CD and accretion loops | `plsql-cicd`, `plsql-accretion` |

Lower layers do not import higher layers. Parser backend generated types
stay private to the backend crate; downstream crates see only the
`ParseBackend` surface.

## Data Flow

```text
source files / DDL exports
        |
        v
plsql-project -> plsql-parser -> AST / diagnostics
        |              |
        |              v
        |        plsql-ir / plsql-symbols
        v              |
CatalogSnapshot -------+
        |
        v
plsql-engine AnalysisRun
        |
        +--> plsql-depgraph
        +--> plsql-lineage
        +--> plsql-sast
        +--> plsql-doc
        +--> plsql-bindgen
        +--> plsql-cicd
```

Every user-visible output carries the same honesty rule: uncertainty is
represented as typed diagnostics and `UnknownReason` values rather than
being dropped or converted into fake confidence.

## Catalog Model

`plsql-catalog` owns `CatalogSnapshot`, the offline representation of
Oracle dictionary state. It is structural, not row-level: objects,
columns, signatures, grants, synonyms, constraints, triggers, indexes,
dependencies, and PL/Scope facts.

Current ingestion paths are offline:

- JSON snapshot loading.
- DBMS_METADATA directory loading.
- Synthetic builders for tests and examples.

Live extraction belongs in `oraclemcp`, which can produce snapshots for
this engine without making Oracle connectivity part of this workspace.

## Public Surfaces

The current public surfaces are libraries and CLIs:

- `plsql-depgraph` for dependency graph inspection.
- `plan-lint` for `plan.md` structural checks.
- `corpus-license-check`, corpus growth tools, and parser drift checks.
- `usr-loop` and `plsql-accretion` for the self-healing fixture loop.
- Library APIs for parse, catalog, engine, lineage, SAST, docs, bindings,
  and CI/CD prediction.

Agent-facing MCP tools are an integration layer owned by `oraclemcp`.

## Verification

The normal local profile is stable-channel and offline:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
scripts/offline_boundary_lint.sh
scripts/offline_honesty_grep.sh
```

The repository also keeps parser code generation drift checks, corpus
license checks, accretion gate selftests, and targeted golden tests. Live
Oracle smoke tests are transition or downstream coverage, not the product
path for this repo.

## Cross-References

- [`plan.md`](../plan.md) - authoritative architecture and release plan.
- [`README.md`](../README.md) - public-facing project entry point.
- [`docs/components/`](components/) - crate-level design notes.
- [`docs/oraclemcp/`](oraclemcp/) - handoff notes for the MCP/live-DB
  repository.
