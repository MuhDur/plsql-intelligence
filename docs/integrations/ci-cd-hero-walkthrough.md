# Hero demo — PR-integration walkthrough

End-to-end walkthrough that exercises the release-0.5.0
`plsql predict --robot-json` and GitHub Action path against the
synthetic L1 hero diff (`corpus/lab/hero_diff/`). Demonstrates the
predict → render → PR-comment cycle without needing a live Oracle
instance.
(`PLSQL-CICD-021` / oracle-5vga.)

## What the demo proves

The L1 hero scenario: a developer opens a PR that renames the
`p_emp_id` parameter on `pkg_employee_mgmt.fire_employee` to
`p_employee_id`. Every caller using named notation
(`employee_mgmt.fire_employee(p_emp_id => 42)`) breaks. The CI Action
catches this before merge, posts a single self-editing comment on the
PR, and leaves the raw robot JSON available for a later policy gate.

## Stage 0 — clone + build

```sh
git clone <repo> && cd oracle
cargo build --workspace --release
```

No live database needed. The synthetic L1 corpus + hero diff golden
ship with the repo.

## Stage 1 — predict invalidations

```sh
cargo run --release -p plsql-cicd --bin plsql -- predict \
    --source-kind diff corpus/lab/hero_diff/change.diff \
    --robot-json > /tmp/predict.json
```

`/tmp/predict.json` contains the `plsql.cicd.change_impact`
envelope: summary counts, invalidations by kind, invalidation rows,
recompile guidance, compile-error flags, lineage notes,
uncertainties, and the completeness posture. The schema is frozen at
version `1.0.0` by
`crates/plsql-cicd/tests/golden/change_impact_payload.json`.

When a fixture already has an offline lineage impact artifact, pass it
through the transitive wrapper:

```sh
cargo run --release -p plsql-cicd --bin plsql -- predict \
    --robot-json \
    --source-kind changeset-json corpus/lab/hero_diff/changeset.json \
    --lineage-impact corpus/lab/hero_diff/impact.json \
    --lineage-metadata corpus/lab/hero_diff/lineage-metadata.json \
    > /tmp/predict.json
```

`--lineage-metadata` is the explicit logical-id-to-object map used to
lower `LineageResult` rows into the stable numeric-symbol payload.

## Stage 2 — render the GitHub Action comment locally

The reusable Action renders the same `/tmp/predict.json` into a
Markdown comment with these fields:

- invalidation count
- recompile candidates
- compile-error flags
- uncertainty count
- max dependency distance

In GitHub Actions this rendering happens inside
`.github/actions/plsql-change-impact/action.yml`. Its self-test builds
the CLI, feeds a fixture changeset into the Action, and asserts the
expected `<!-- plsql-cicd:change-impact v1 -->` marker plus the
expected invalidation count.

## Stage 3 — run the Action on a PR

The reference workflow calls the Action directly:

```yaml
- name: Predict and comment on PL/SQL blast radius
  id: impact
  uses: ./.github/actions/plsql-change-impact
  with:
    plsql-bin: plsql
    git-range: ${{ github.event.pull_request.base.sha }}..${{ github.sha }}
    mode: catalog-aware
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

The first run creates the PR comment. Later runs edit the same comment
in place by searching existing issue comments for
`<!-- plsql-cicd:change-impact v1 -->`.

## Stage 4 — verify the integration is healthy

The release-0.5.0 self-test is
`.github/workflows/plsql-change-impact-selftest.yml`. It runs with
`post-comment: "false"` so CI proves the Action and comment renderer
without requiring a fixture pull request or a live token.

## What the synthetic lab proves

Running this walkthrough end-to-end against `corpus/lab/hero_diff/`:

1. Confirms the **deterministic-output** invariant — same diff →
   byte-identical `plsql.cicd.change_impact` JSON every time.
2. Exercises the **HTML-marker idempotence** path in the GitHub
   Action — the second Action run edits the existing comment.
3. Validates the **golden artifact** — `expected_what_breaks.json`
   should match the reported invalidation and compile-error summary
   modulo ordering.
4. Verifies the **CI binding contract** — copy the
   [`github-actions.yml`](../../examples/ci/github-actions.yml)
   reference into a fresh repo, push a PR with the same diff, and
   the Action runs identically.

## Pointers

- L1 hero diff: `corpus/lab/hero_diff/` (PLSQL-LAB-002).
- Change-impact envelope schema: `crates/plsql-cicd/src/predict.rs::ChangeImpactEnvelope`.
- GitHub Action: `.github/actions/plsql-change-impact/action.yml`.
- Action self-test: `.github/workflows/plsql-change-impact-selftest.yml`.
- CI reference workflows: `examples/ci/` (CICD-017/018/019).
- Companion-repo plan: [`ci-cd.md`](ci-cd.md) (CICD-020).
