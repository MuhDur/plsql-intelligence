# Hero demo — PR-integration walkthrough

End-to-end walkthrough that exercises every stage of the
`plsql cicd` pipeline against the synthetic L1 hero diff
(`corpus/lab/hero_diff/`). Demonstrates the full predict → plan →
gate → post-pr-comment cycle without needing a live Oracle instance.
(`PLSQL-CICD-021` / oracle-5vga.)

## What the demo proves

The L1 hero scenario: a developer opens a PR that renames the
`p_emp_id` parameter on `pkg_employee_mgmt.fire_employee` to
`p_employee_id`. Every caller using named notation
(`employee_mgmt.fire_employee(p_emp_id => 42)`) breaks. The CI gate
catches this before merge, posts a single self-editing comment on the
PR, and either blocks the merge or lets it through depending on
policy.

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

## Stage 2 — plan recompile order

```sh
cargo run --release -p plsql-cicd -- plan \
    --changeset corpus/lab/hero_diff/change.diff \
    --robot-json > /tmp/plan.json
```

`/tmp/plan.json` contains the topologically-sorted DDL order so a
deployment script can recompile dependent objects after the changed
package re-validates.

## Stage 3 — gate against policy

Drop a `.plsql-cicd-policy.toml` in your repo:

```toml
max_invalidations = 25
blocked_kinds = []
min_confidence = "medium"
blocking_unknown_reasons = ["WrappedSource", "DynamicSqlOpaque"]
```

Then:

```sh
cargo run --release -p plsql-cicd -- gate \
    --predict /tmp/predict.json \
    --policy .plsql-cicd-policy.toml \
    --pr-comment-json > /tmp/gate.json
```

`/tmp/gate.json` is the `plsql.cicd.gate_pr_comment` envelope
(`PrCommentEnvelope`, CICD-014). Inspect:

```sh
jq '.pr_comment.verdict, .pr_comment.headline' /tmp/gate.json
# "pass" or "fail"
# "plsql cicd gate: PASS — no policy violations" (or similar)
```

The body markdown is in `.pr_comment.body_md` — that's what gets
posted to the PR.

## Stage 4 — post the PR comment (GitHub example)

```sh
export PLSQL_GH_TOKEN='ghp_<your-token>'

cargo run --release -p plsql-cicd -- post-pr-comment \
    --envelope /tmp/gate.json \
    --platform github \
    --owner acme-corp \
    --repository billing-db \
    --pull-request 42
```

The first run creates the comment. The second (and every subsequent)
run edits the same comment in place — `find_existing_comment`
(CICD-016) scans the PR's existing comments for the
`<!-- plsql-cicd:gate v1 -->` HTML marker and turns the CREATE into a
PATCH automatically.

## Stage 5 — verify the integration is healthy

```sh
cargo run --release -p plsql-cicd -- doctor pr-integration \
    --platform github \
    --envelope /tmp/gate.json
```

Reports the `PrIntegrationDoctorReport` (CICD-022): token presence,
envelope schema version, last-comment status, and one-line
remediation hints. `posture: healthy` means the integration is
operational; `caution` or `unknown` surface the action needed.

## What the synthetic lab proves

Running this walkthrough end-to-end against `corpus/lab/hero_diff/`:

1. Confirms the **deterministic-output** invariant — same diff →
   byte-identical `gate.json` every time.
2. Exercises the **HTML-marker idempotence** path — the second
   `post-pr-comment` call must edit, not create.
3. Validates the **golden artifact** — `expected_what_breaks.json`
   should match the `gate.json.decision.failures` summary modulo
   ordering.
4. Verifies the **CI binding contract** — copy the
   [`github-actions.yml`](../../examples/ci/github-actions.yml)
   reference into a fresh repo, push a PR with the same diff, and
   the gate runs identically.

## Failure-mode walkthrough

Replace the policy file with a tighter version to force a `fail`:

```toml
max_invalidations = 0   # any invalidation blocks
```

Re-run stages 3-4. The gate now emits `verdict: fail`, the body lists
the violation, and the same `post-pr-comment` call edits the existing
comment in place — no duplicate comments per PR.

## Pointers

- L1 hero diff: `corpus/lab/hero_diff/` (PLSQL-LAB-002).
- Gate envelope schema: `crates/plsql-cicd/src/gate.rs::PrCommentEnvelope` (CICD-014).
- Comment poster: `crates/plsql-cicd/src/post_pr_comment.rs` (CICD-015 + CICD-016).
- PR-integration doctor: `pr_integration_doctor` (CICD-022).
- CI reference workflows: `examples/ci/` (CICD-017/018/019).
- Companion-repo plan: [`ci-cd.md`](ci-cd.md) (CICD-020).
