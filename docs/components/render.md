# plsql-render

Low-level HTML / Markdown / SVG rendering primitives. Layer 0.

## Purpose

Higher-level documentation, lineage, and SAST reports share a small set of
output primitives (HTML page chrome, Markdown tables, SVG node graphs).
Centralising them here keeps the rendering layer auditable, prevents
duplication across `plsql-doc`, `plsql-scan`, `plsql-lineage`, and friends,
and makes it trivial to retheme everything from one place.

## Surface

| Module | Function | Purpose |
|--------|----------|---------|
| `html` | `shell(title, body)` | Wraps body in a styled HTML5 document |
| `markdown` | `table(headers, rows)` | Renders a Markdown pipe table |
| `graphviz` | `escape_label`, `quote_id` | DOT-language escaping helpers |
| `svg` | `GraphView` trait + `node_graph(view)` | Renders nodes + edges as SVG |

## Conventions

- **No content semantics.** `plsql-render` knows nothing about lineage,
  catalog metadata, etc. It receives pre-shaped `GraphNode` / `GraphEdge`
  values and emits XML. Component-specific layout (concentric rings for
  impact, layered for depgraph) lives in the consumer.
- **Pure functions.** No I/O, no globals — every helper takes data in and
  returns a `String`.
- **Escaping is built in.** Callers do not need to pre-escape user-supplied
  labels.

## Pointers

- Source: `crates/plsql-render/src/lib.rs`
- Plan: `plan.md` §6.2 (Layer 0 components), §11 (documentation generator)
- Consumers: `plsql-doc`, `plsql-lineage`, `plsql-depgraph`, `plsql-scan`
