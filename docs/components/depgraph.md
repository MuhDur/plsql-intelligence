# plsql-depgraph

Stable, evidence-bearing dependency graph for PL/SQL objects. Layer 2.

## Purpose

The dependency graph is the load-bearing data structure for every Layer 3+
consumer: lineage, SAST, docs, CI/CD planning, MCP servers. It must answer
"what does object X read / write / call?" while never losing the evidence
behind each edge.

## Surface

| Type | Purpose |
|------|---------|
| `DepGraph` | The graph itself (nodes + edges + provenance + evidence) |
| `Node` / `NodeId` | A first-class object (table, view, procedure, column, …) |
| `Edge` / `EdgeId` | A directed edge with `EdgeKind` + `Confidence` |
| `EdgeKind` | `Calls`, `Reads`, `Writes`, `ReadsColumn`, `WritesColumn`, `DerivesColumn`, `ReadsUnknownColumnOfTable`, `WritesUnknownColumnOfTable`, `TriggersOn`, `DependsOnType`, `Constrains`, `OpaqueDynamic`, `DbLink`, `References` |
| `Provenance` | Where this edge came from (file id + span + parse rule + resolution strategy) |
| `NodeIdentityKind` | `Table`, `View`, `MaterializedView`, `Sequence`, `Type`, `TypeAttribute`, `Trigger`, `Column`, `Constraint`, `Synonym`, `PackageProcedure`, … |

## Operations

| Op | Purpose |
|----|---------|
| `query_neighbors(selector)` | Outgoing edges (downstream impact) |
| `query_reverse_neighbors(selector)` | Incoming edges (upstream dependencies) |
| `query_path(from, to)` | Shortest path with evidence |
| `to_graphml(interner)` | Full-graph GraphML export |
| `explain_edge` / `explain_node` | Customer-facing reason for a specific edge or node |

## Invariants

- **Stable node ids across runs.** `NodeId` is interned; ids do not rotate
  between analysis runs.
- **Every edge carries a `Confidence`.** Edges below `High` carry an
  `UnknownReason` discriminator on their provenance record.
- **No silent fallbacks.** When we can't pin a column reference we emit
  `ReadsUnknownColumnOfTable` (or the write equivalent) so consumers know
  the access is conservative.

## CLI

`plsql-depgraph` is exposed as a public binary. `--robot-json` envelopes
satisfy R10; `doctor` reports graph completeness; `explain` surfaces
per-edge / per-node / per-path reasoning.

## Pointers

- Source: `crates/plsql-depgraph/src/{lib.rs,main.rs}`
- Plan: `plan.md` §10 (Layer 2 dependency graph), §12 (SAST consumers)
- Downstream: `plsql-lineage`, `plsql-scan`, `plsql-cicd`, MCP tools
