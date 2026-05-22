# plsql-lineage

Cross-object impact, dependency, and change-classification queries for the
PL/SQL Intelligence Engine. Layer 4.

## Purpose

`plsql-lineage` is the customer-facing product surface for lineage questions.
Where `plsql-depgraph` (Layer 2) owns the raw graph + evidence, `plsql-lineage`
packages the most useful traversals into typed APIs, schema-versioned wire
envelopes, and rendered reports that release engineers, DBAs, governance
teams, and security reviewers can attach to audit and change tickets with
confidence.

The crate exists because the gap this product targets — *Oracle change-impact
+ recompile assurance with explicit uncertainty accounting* — is not covered
by query-history-driven lineage products or file-based SAST scanners. We
answer questions in the customer's terms ("what breaks if I change this?",
"who reads this column?", "show me the unsafe dynamic-SQL paths between
these two objects") rather than forcing them to walk a graph manually.

## Public API surface

| Function | Returns | Schema |
|----------|---------|--------|
| `impact(graph, node, max_depth)` | `LineageResult` | `IMPACT_SCHEMA` |
| `dependencies(graph, node, max_depth)` | `LineageResult` | `DEPENDENCIES_SCHEMA` |
| `callers(graph, target)` | `CallersResult` | `CALLERS_SCHEMA` |
| `column_readers(graph, column)` | `ColumnAccessResult` | `COLUMN_ACCESS_SCHEMA` |
| `column_writers(graph, column)` | `ColumnAccessResult` | `COLUMN_ACCESS_SCHEMA` |
| `unsafe_paths(graph, from, to, max_depth, max_paths)` | `UnsafePathsResult` | `UNSAFE_PATHS_SCHEMA` |
| `recompile_order(graph, set)` | `RecompilePlan` | `RECOMPILE_ORDER_SCHEMA` |
| `classify_git_diff(repo, before, after)` | `SemanticChangeSet` | `CLASSIFY_CHANGE_SCHEMA` |
| `classify_dir_diff(before, after)` | `SemanticChangeSet` | `CLASSIFY_CHANGE_SCHEMA` |
| `classify_rename(changes, hints)` | `RenameClassification` | `CLASSIFY_RENAME_SCHEMA` |
| `compare_oracle_deps(graph, snapshot, interner)` | `CompareOracleDepsReport` | `COMPARE_ORACLE_DEPS_SCHEMA` |
| `detect_orphans(graph, assume_incomplete_augmentation)` | `OrphanCandidatesReport` | `ORPHAN_CANDIDATES_SCHEMA` |
| `doctor(graph)` | `LineageDoctorReport` | `DOCTOR_SCHEMA` |
| `explain_edge` / `explain_node` / `explain_path` | `LineageExplanation` | `EXPLAIN_SCHEMA` |
| `impact_to_graphml(result)` | GraphML `String` | `LINEAGE_GRAPHML_SCHEMA` |
| `impact_to_html(result)` | HTML `String` | `LINEAGE_HTML_SCHEMA` |
| `impact_to_html_with_compare(result, compare)` | HTML `String` | `LINEAGE_HTML_SCHEMA` |
| `orphans_to_markdown(report)` | Markdown `String` | (no envelope; usually wrapped) |
| `orphans_to_html(report)` | HTML `String` | (no envelope; usually wrapped) |

Every envelope-returning function has a matching `*_envelope` wrapper that
wraps the payload with a `RobotJsonEnvelope` carrying the schema id +
version inline (R10 — `--robot-json` mandate).

## Confidence model

Every lineage edge carries a `Confidence` tier:

| Tier | Meaning |
|------|---------|
| `Exact` | All inputs known; deterministic resolution. |
| `Heuristic` | Resolution required heuristic inference (catalog inference, dynamic-SQL shape analysis, public-grant traversal). |
| `Unknown` | Resolution unknown; an `UnknownReason` accompanies the edge. |

Aggregation rules (plan §14.4):

- **Path confidence** is the *min* over edges in the path — a path is no more
  auditable than its weakest edge.
- **Node confidence** when multiple paths reach the same node is the *max*
  of those path-confidences — the engine publishes the strongest proof it
  can support.

`Unknown` edges flow through to the `LineageResult::unknown_edges` field
with their `UnknownReason` discriminator preserved, so consumers (the
Trust Block UI, SAST reachability, MCP audit responses) can show *why* the
engine couldn't be more certain.

## Operation semantics

### `impact(node)` — downstream walk

Walks outgoing edges from `node` to find every object that may be
affected by a change to it. BFS by default; depth-bounded via `max_depth`.

Per plan §14, the impact result MUST also carry:

- `affected_nodes` — the reachable set with per-node `path_confidence` + `hops`
- `unknown_edges` — every path crossing an `OpaqueDynamic` / `Unknown`-confidence
  edge, so the Trust Block can surface them

### `dependencies(node)` — upstream walk

Walks incoming edges from `node` to find every object the node depends on.
Same depth-bounding semantics. Returns `LineageResult` with `direction =
Upstream` so consumers can distinguish from `impact` payloads.

### `callers(target)`

First-hop reverse-edge query, filtered to `EdgeKind::Calls`. Where
`dependencies` returns every upstream node regardless of edge kind,
`callers` answers the much narrower question "who invokes this routine?".
Triggers / type methods that fire on the routine also surface here.

### `column_readers(column)` / `column_writers(column)`

Reverse-edge queries filtered to column-access edge kinds:

- `column_readers`: `ReadsColumn` (exact) + `ReadsUnknownColumnOfTable`
  (conservative fallback when the depgraph could only attribute the
  read to the owning table)
- `column_writers`: `WritesColumn` + `WritesUnknownColumnOfTable` +
  `DerivesColumn` (the latter records value-flow into a column rather
  than a literal write)

Each `ColumnAccessor` carries `is_unknown_column_of_table` so column-
rename audits know which rows must be hand-checked.

`ColumnAccessResult.resolution_error: Option<String>` distinguishes
"column found, no accessors" (`None`) from "column node missing"
(`Some(reason)`).

### `unsafe_paths(from, to)`

DFS search for every path from `from` to `to` (up to `max_depth` edges
and `max_paths` total) that contains at least one *unsafe* edge — where
"unsafe" means `EdgeKind::OpaqueDynamic` or `Confidence::Unknown`.

Defaults: `UNSAFE_PATHS_DEFAULT_MAX_DEPTH = 8`,
`UNSAFE_PATHS_DEFAULT_MAX_PATHS = 100`. Each `UnsafePath` reports per-
edge unsafe markers + an `overall_confidence` (min of path edges).

### `recompile_order(set)`

Topological sort of a changed-object set respecting Oracle's invalidation
order: spec before body, type before consumers, package before dependent
view. Output is consumed by `plsql-cicd plan` (PLSQL-CICD-003) to drive
the actual recompile.

### `classify_*` (change classification)

Family of functions that compare two source revisions and emit a
`SemanticChangeSet`:

- `classify_git_diff(repo, before_ref, after_ref)` — drives `git diff
  --name-status` (rename detection via leading byte: `b'R' | b'C'`)
- `classify_dir_diff(before_dir, after_dir)` — pure filesystem walk
- (catalog-aware diff is the consumer's responsibility — pass a
  pre-computed `SemanticChangeSet` directly)

### `classify_rename(changes, hints)`

Pairs `Created`/`Dropped` records into rename candidates using
externally-supplied hints. Hint priority order:

1. `explicit_mappings` → `Confidence::Exact`
2. `persistent_id_pairs` → `Confidence::Exact`
3. `git_renames` above `GIT_RENAME_THRESHOLD = 70` → `Confidence::Heuristic`

Unmatched deletes/creates stay in their bucket — the classifier *never*
invents a rename without a hint (plan §10.3: a false-positive rename is
worse than a delete+create split).

### `compare_oracle_deps(graph, snapshot, interner)`

Thin wrapper over `DepGraph::cross_check_with_catalog` (DEP-014) that
renames the depgraph classification into the customer-facing
"Oracle sees / engine sees / uncertain" vocabulary (plan §1.5):

- `oracle_only` — Oracle records this dependency, engine missed it
- `engine_only` — engine has this edge, Oracle doesn't track it
- `kind_mismatches` — both record, disagree on kind
- `expected_gaps` — engine emits, ALL_DEPENDENCIES doesn't represent
  (OpaqueDynamic / DbLink / Constrains / TriggersOn)

### `detect_orphans(graph, assume_incomplete_augmentation)`

Zero-incoming-edge classifier. Returns `OrphanCandidatesReport` with
per-candidate `OrphanConfidenceTier`:

- `HighConfidenceUnused` — no incoming, no outgoing edges
- `LikelyUnused` — no incoming, has outgoing
- `Inconclusive` — emitted when `assume_incomplete_augmentation` is
  true (catalog grants / synonyms / scheduler / DB-link augmentation
  isn't loaded — the absence of inbound edges cannot prove non-use)

The classifier is deliberately conservative: it prefers `Inconclusive`
over a false `HighConfidenceUnused` whenever evidence is thin (plan
§1.5).

### `doctor(graph)` — Trust Block source

Returns `LineageDoctorReport`: total nodes / edges / confidence
distribution / dominant `UnknownReason` discriminators. The CLI doctor
command, MCP `meta.trust_block`, and the customer-facing HTML report
all read from this report.

## Reports

The crate ships three rendered report formats:

- `impact_to_graphml(result)` — subgraph GraphML for yEd / Gephi /
  Cytoscape. Anchor / affected / unknown-reason node roles surface as
  data keys. Unknown edges become synthetic `unknown::<reason>` nodes
  so uncertainty is visible.
- `impact_to_html(result)` — self-contained HTML5 document with
  embedded SVG impact subgraph (concentric rings by hop distance) +
  Markdown summary table.
- `impact_to_html_with_compare(result, compare)` — same as above plus
  an "Oracle sees / engine sees / uncertain" section that splices the
  `CompareOracleDepsReport` into a comparison table.
- `orphans_to_markdown(report)` / `orphans_to_html(report)` — orphan
  candidates partitioned by tier, with a Trust Block at the top and
  AUDIT-only remediation at the bottom. **Never emits DROP scripts.**

## Example reports

### Impact result for a dropped column

```json
{
  "schema_id": "plsql.lineage.impact",
  "schema_version": "1.0.0",
  "payload": {
    "query": {
      "anchor": "billing.customers.legacy_segment",
      "direction": "downstream",
      "max_depth": null,
      "min_confidence": null
    },
    "edges": [
      {
        "source": "billing.customers.legacy_segment",
        "target": "billing.customer_report_v",
        "kind": "ReadsColumn",
        "confidence": "exact"
      }
    ],
    "unknown_edges": [],
    "affected_nodes": [
      {
        "logical_id": "billing.customer_report_v",
        "hops": 1,
        "path_confidence": "exact"
      }
    ]
  }
}
```

### Orphan candidates Markdown excerpt

```markdown
## High confidence (unused) (1)

- `billing.legacy_purge_pkg` (`PackageBody`)
  - no incoming edges in depgraph (identity_kind = PackageBody)

## AUDIT statements (observation, not deletion)

> Apply these AUDIT statements to confirm non-use over the configured
> observation window (30/60/90 days). **No DROP statements are emitted**
> — that decision belongs to a human reviewing AUDIT findings.

```sql
AUDIT ALL ON billing.legacy_purge_pkg BY ACCESS;  -- high-confidence-unused candidate
```

## Customer guarantees (plan §1.5 Trust Block)

- Every result publishes its completeness counts via `doctor`.
- Low-confidence edges are tagged with their `UnknownReason` rather
  than dropped.
- `unsafe_paths` exposes the dynamic-SQL audit trail by construction.
- Reports never emit destructive remediation; AUDIT statements only.
- HTML / GraphML / Markdown exports preserve every reason annotation.

## Versioning + compatibility

Every wire envelope carries a `(schema_id, schema_version)` tuple from
`plsql_output::SchemaDescriptor`. Consumers should:

1. **Pin the major version** — e.g., consume `plsql.lineage.impact` v1.x
   and refuse to deserialize v2.x payloads without an explicit migration
   step.
2. **Tolerate additive minor bumps** — new optional fields can land in
   a minor version. Mandatory fields stay stable until a major bump.
3. **Validate via `matches_schema()`** — every envelope exposes
   `envelope.matches_schema(SCHEMA)` for callers that want a one-shot
   compatibility check before processing the payload.

The `LINEAGE_SCHEMAS: [SchemaDescriptor; 14]` const enumerates every
schema this crate emits. Each is registered in the workspace's
`OUTPUT_SCHEMAS` constellation via re-export.

## R-rule conformance

`plsql-lineage` adheres to the R-rules in `plan.md` §4:

- **R10 (`--robot-json`):** every public function with side-output has a
  matching `*_envelope` wrapper that emits a stable-schema JSON payload.
- **R13 (no silent uncertainty):** every blind spot is a typed
  `UnknownReason` discriminator on `LineageResult.unknown_edges` —
  never dropped, never coerced to a scalar score.
- **R17 (no telemetry by default):** the crate has no network I/O. The
  only optional dependency that touches the OS is `tempfile` (test-only).
- **R20 (parser backend isolation):** the crate imports from
  `plsql-depgraph` and `plsql-catalog` but never from
  `plsql-parser-antlr` — depgraph is the public-API boundary for
  parse-derived data.

## Test coverage

- 79 unit tests in `src/lib.rs` (impact_tests + classify_tests + tests
  modules)
- Fixtures: `dependency_fixture` (linear chain), `branching_graph`
  (mixed confidences), `unsafe_paths_fixture`, `column_access_fixture`,
  `callers_fixture`, `rename_changeset`, `build_cross_check_fixture`
- Every public function has at least one happy-path test plus an
  empty-input / unknown-anchor / no-accessors edge case
- Schema envelope tests assert `matches_schema()` returns true for
  every emitted envelope

## Pointers

- Source: `crates/plsql-lineage/src/lib.rs` (~2900 LoC, 79 unit tests)
- Cargo.toml: `crates/plsql-lineage/Cargo.toml`
- Plan: `plan.md` §14 (Layer 4 Lineage), §1.5 (Trust Block), §10 (depgraph)
- Upstream: `plsql-depgraph`, `plsql-render`, `plsql-output`, `plsql-core`, `plsql-catalog`
- Downstream: `plsql-cicd`, MCP `plsql-mcp`, lineage CLI
- Schemas: 14 versioned descriptors (`LINEAGE_SCHEMAS` const)
