# CI/CD integration

Reference workflows for embedding the release-0.5.0
`plsql predict --robot-json` change-impact surface into CI/CD.
(`PLSQL-CICD-020` / oracle-vri2.)

## GitHub Action shipped in this repo

The reusable Action lives at
`.github/actions/plsql-change-impact/action.yml`. It performs three
steps:

1. Run `plsql predict --robot-json` from a changeset source, a
   before/after directory pair, or a PR git range.
2. Render the stable `plsql.cicd.change_impact` envelope into a PR
   comment that names invalidation count, recompile candidates,
   compile-error flags, uncertainty count, and max dependency
   distance.
3. POST or PATCH one GitHub issue comment carrying the stable
   `<!-- plsql-cicd:change-impact v1 -->` marker.

The Action leaves both the raw prediction JSON and rendered Markdown
comment as outputs so downstream jobs can upload or inspect them.

The self-test workflow
`.github/workflows/plsql-change-impact-selftest.yml` builds the F.5
`plsql` binary, runs the Action on a fixture changeset with
`post-comment: "false"`, and asserts the expected blast-radius
comment. That is the CI proof for F.6.

## Reference workflows shipped in this repo

The GitHub files mirror the same shape:

1. Checkout with `fetch-depth: 0` so `--git-range` can classify the PR
   diff.
2. Install the `plsql` CLI.
3. Call `.github/actions/plsql-change-impact` with
   `git-range: ${{ github.event.pull_request.base.sha }}..${{ github.sha }}`.
4. Upload the raw `plsql.cicd.change_impact` JSON and rendered
   comment body.

| Platform | File | Auth env var | Status |
|---|---|---|---|
| GitHub Actions | [`examples/ci/github-actions.yml`](../../examples/ci/github-actions.yml) | `GITHUB_TOKEN` | shipped |
| GitHub Actions self-test | [`.github/workflows/plsql-change-impact-selftest.yml`](../../.github/workflows/plsql-change-impact-selftest.yml) | n/a (`post-comment: "false"`) | shipped |
| GitHub Actions reference gate | [`.github/workflows/plsql-gate.yml`](../../.github/workflows/plsql-gate.yml) | `GITHUB_TOKEN` | shipped |
| GitLab CI | [`examples/ci/gitlab-ci.yml`](../../examples/ci/gitlab-ci.yml) | `PLSQL_GL_TOKEN` | legacy template, outside F.6 Action self-test |
| Jenkins (Multibranch) | [`examples/ci/Jenkinsfile.groovy`](../../examples/ci/Jenkinsfile.groovy) | Jenkins credentials store (`plsql-gh-token` / `plsql-gl-token`) | legacy template, outside F.6 Action self-test |

## Companion templates repo (planned)

A separate Apache-2.0 repo at
`https://github.com/plsql-intelligence/ci-templates` (planned) will
mirror these files plus carry **production-vetted variants**:

- Pinned runner images (e.g.
  `registry.gitlab.com/plsql-intelligence/runner:0.x`) so a downstream
  consumer doesn't need to build their own toolchain image.
- Hardened secrets handling: opinionated `withCredentials` / `secrets`
  blocks that minimise the surface where an Oracle DSN or PR-poster
  token can leak into logs.
- Composite GitHub Actions + reusable GitLab CI templates so a
  downstream consumer copies a single workflow line instead of
  vendoring the whole file.
- Versioned tags matching the `plsql-intelligence` release line, so
  `actions/checkout@v4` and `image: …/runner:0.4` move in lockstep
  with the gate's payload schema.

The split exists because the *contract* (this repo's `examples/ci/`
files) and the *operational templates* (the companion repo) have
different cadences: the contract changes with the gate schema
version; the templates change with platform UI / API drift. Keeping
them in separate repos lets the operational variants be tagged and
pinned independently.

### Pointers from the companion repo back here

- Change-impact schema → `crates/plsql-cicd/src/predict.rs`
  (`ChangeImpactEnvelope`).
- GitHub Action → `.github/actions/plsql-change-impact/action.yml`.
- Action self-test →
  `.github/workflows/plsql-change-impact-selftest.yml`.
- Future gate-comment schema → `crates/plsql-cicd/src/gate.rs`
  (`PrCommentEnvelope`).
- Idempotent edit logic → `crates/plsql-cicd/src/post_pr_comment.rs`
  (`find_existing_comment` + `build_request`).
- PR-integration doctor → `crates/plsql-cicd/src/post_pr_comment.rs`
  (`pr_integration_doctor`).
- GitHub Action HTML marker version →
  `<!-- plsql-cicd:change-impact v1 -->` (bumps with envelope schema
  version).

## Bead chain

CICD-014 (envelope) → CICD-015 (post-pr-comment library) → CICD-016
(idempotent find-existing) → CICD-017 (`.github/workflows/plsql-gate.yml`
reference) → CICD-018 (`.gitlab-ci.yml` reference) → CICD-019
(`Jenkinsfile.groovy` reference) → CICD-020 (this file, companion-repo
cross-links) → CICD-022 (PR-integration doctor).

When the companion repo lands, this page gains a `Releases &
versioning` section pinning template tag → engine version pairs.
