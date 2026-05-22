# plsql-doc

Markdown + HTML documentation generator for PL/SQL packages. Layer 3.

## Purpose

Given an `AnalysisRun`, `plsql-doc` produces human-readable documentation
for every package, procedure, function, table, view, and trigger the run
covers — including embedded dependency graphs and a per-object Trust
Block that surfaces low-confidence resolution decisions to the reader.

## Surface (planned)

| Function | Returns |
|----------|---------|
| `render_package(&AnalysisRun, package_name)` | Markdown for a single package |
| `render_object(&AnalysisRun, logical_id)` | Markdown for any catalog object |
| `render_site(&AnalysisRun, output_dir)` | Multi-page static-site dump |
| `render_envelope(&AnalysisRun)` | `RobotJsonEnvelope` with site manifest |

## Layout conventions

- One Markdown file per package / standalone object.
- Embedded SVG (via `plsql-render::svg`) for dependency subgraphs.
- Inline Markdown tables (via `plsql-render::markdown::table`) for
  signatures, grants, indexes.
- Trust Block at the top of every page: completeness counters, low-
  confidence accessor list, link to `doctor` output.

## Pointers

- Source: `crates/plsql-doc/src/`
- Plan: `plan.md` §11 (Layer 3 Documentation Generator), §1.5 (Trust Block)
- Upstream: `plsql-engine`, `plsql-render`, `plsql-lineage`
