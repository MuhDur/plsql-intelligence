# Release engineer demo — predict → plan → gate

You're about to merge a PL/SQL change to `pkg_employee_mgmt`. Before
it lands you want: what gets invalidated, what order to recompile,
and whether the change violates your release policy.

## Setup

```sh
git clone <repo> && cd oracle
cargo build --workspace
```

No Oracle connection needed.

## Step 1 — predict invalidations from the diff

```sh
cargo run -p plsql-cicd --example predict_hero_diff -- \
    --changeset corpus/lab/hero_diff/change.diff
```

Output: a list of objects predicted to invalidate, each with a
`reason` (PackageBodyOnlyChange, MaterializedViewRefreshAffected,
etc.), a `distance` (1 = direct dep, 2+ = transitive), and a
`confidence` band (High / Medium / Low / Opaque). Backed by
`crates/plsql-cicd/src/predict.rs`.

## Step 2 — plan a topologically-sorted DDL order

```sh
cargo run -p plsql-cicd --example plan_hero_diff
```

Output: the DDL fingerprint, the topological recompile order, and a
short rationale. Backed by `crates/plsql-cicd/src/plan.rs`.

## Step 3 — gate against your policy file

Drop a `.plsql-cicd-policy.toml` at the repo root:

```toml
max_invalidations = 25
blocked_kinds = ["TRIGGER"]
min_confidence = "medium"
blocking_unknown_reasons = ["WrappedSource", "DynamicSqlOpaque"]
```

Then:

```sh
cargo run -p plsql-cicd --example gate_hero_diff -- \
    --policy .plsql-cicd-policy.toml
```

Output: `allowed: true|false`, the list of `failures` (one per rule
that fired), and the full `policy_summary`. The same call with
`--pr-comment-json` returns the `plsql.cicd.gate_pr_comment` envelope
ready for `plsql post-pr-comment` (PLSQL-CICD-015).

## Step 4 — wire it into CI

Drop `examples/ci/gitlab-ci.yml` (or `.github/workflows/plsql-gate.yml`)
into your repo. Both reference workflows ship in `examples/ci/`. The
GitHub Actions workflow runs as a per-PR check; the GitLab CI variant
runs as a MR-discussion job.

## Reading the output

- `predicted_invalidations` shape: stable JSON via `RobotJsonEnvelope`
  (`crates/plsql-output/src/lib.rs`).
- `confidence` interpretation: High = catalog + lineage both agree;
  Medium = one source; Low = single-source with caveats; Opaque =
  blocked by a typed `UnknownReason` — read `unknown_reasons` for the
  remediation.

## Notes

- All `--example` invocations above are mock — substitute your real
  invocation pattern once PLSQL-CICD-007 (the CLI binary) lands.
- The CI gate is intentionally conservative — `blocked_kinds` defaults
  to `[]` so you opt-in per shop.
