# plsql-cicd

Change-impact prediction + recompile cascade planning for CI/CD. Layer 5.

## Purpose

Pre-deploy, a release engineer needs to know: given this changeset, what
will invalidate at recompile time? Which order do we need to recompile in?
Which database features (editioning, materialised view refresh, synonym
retargeting) require special handling? `plsql-cicd` answers those
questions and surfaces them as a `DeploymentPlan` consumers can act on.

## Surface

| Type | Purpose |
|------|---------|
| `ChangeSet` | Input — set of changed `logical_id`s with classification |
| `InvalidationPrediction` | Output of `predict <changeset>` — list of affected objects + reasons |
| `DeploymentPlan` | Topologically ordered recompile plan + caveats |
| `RecompileMode` | `source-only`, `catalog-aware`, `live-snapshot` |

## Robot JSON Contract

`plsql cicd predict --robot-json` emits the
`plsql.cicd.change_impact` schema at version `1.0.0`. The public Rust
builder is `change_impact_envelope(prediction, compile_error_flags)`;
the frozen contract is covered by
`crates/plsql-cicd/tests/golden/change_impact_payload.json`.

Stable top-level payload sections:

- `summary` — mode, invalidation count, recompile count, uncertainty
  count, compile-error flag count, and maximum lineage distance.
- `invalidated_objects_by_kind` — deterministic object-type counts for
  CI summaries and PR badges.
- `invalidations` — one row per predicted invalidation, with stable
  reason code, confidence, and hop distance.
- `recompile_plan` — object-level recompile guidance in deterministic
  order.
- `compile_error_flags` — verification/compile failure flags that the
  CLI or Action can render without changing the schema.
- `lineage_notes` — `lineage.*` attributes such as number of impact
  results, transitive rows, and unresolved logical ids.
- `uncertainties` and `completeness` — R13 blind spots and the
  evidence posture that produced the prediction.

## Modes

- **`source-only`** — no catalog input; best-effort heuristic over source diffs.
- **`catalog-aware`** — uses a `CatalogSnapshot` matching the target environment.
- **`live-snapshot`** — connects, extracts a fresh snapshot, then predicts.

## Distinctions the predictor must make (plan §15)

- Package spec change vs body-only change
- Standalone procedure / function signature change
- Table additive DDL vs destructive DDL
- Type evolution
- Synonym retargeting
- Grant / revoke
- Editioned object change
- Materialised view refresh-affecting change

Each distinct case maps to a different `InvalidationPrediction` reason
and a different position in the `DeploymentPlan`.

## Pointers

- Source: `crates/plsql-cicd/src/`
- Plan: `plan.md` §15 (Layer 5 CI/CD Recompilation Cascade)
- Upstream: `plsql-lineage`, `plsql-catalog`, `plsql-engine`
