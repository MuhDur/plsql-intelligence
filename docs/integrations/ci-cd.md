# CI/CD integration

Reference workflows for embedding the `plsql cicd` family
(predict → plan → gate → post-pr-comment) into your CI/CD platform of
choice. (`PLSQL-CICD-020` / oracle-vri2.)

## Reference workflows shipped in this repo

All four mirror the same five-stage shape:

1. Compute the changeset (unified diff against the merge target).
2. `plsql cicd predict` — emit `target/predict.json`.
3. `plsql cicd plan` — emit `target/plan.json`.
4. `plsql cicd gate --pr-comment-json` — emit `target/gate.json`
   (the `plsql.cicd.gate_pr_comment` envelope, CICD-014).
5. `plsql cicd post-pr-comment` — POST/EDIT the PR/MR comment via
   the platform API (CICD-015 + CICD-016 idempotent update).

| Platform | File | Auth env var | Status |
|---|---|---|---|
| GitHub Actions | [`examples/ci/github-actions.yml`](../../examples/ci/github-actions.yml) | `PLSQL_GH_TOKEN` (or `GITHUB_TOKEN`) | shipped |
| GitLab CI | [`examples/ci/gitlab-ci.yml`](../../examples/ci/gitlab-ci.yml) | `PLSQL_GL_TOKEN` | shipped |
| Jenkins (Multibranch) | [`examples/ci/Jenkinsfile.groovy`](../../examples/ci/Jenkinsfile.groovy) | Jenkins credentials store (`plsql-gh-token` / `plsql-gl-token`) | shipped |
| GitHub Actions gate-only | [`.github/workflows/plsql-gate.yml`](../../.github/workflows/plsql-gate.yml) | n/a (own-repo gate template) | shipped |

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

- `gate.json` schema → `crates/plsql-cicd/src/gate.rs` (`PrCommentEnvelope`).
- Idempotent edit logic → `crates/plsql-cicd/src/post_pr_comment.rs`
  (`find_existing_comment` + `build_request`).
- PR-integration doctor → `crates/plsql-cicd/src/post_pr_comment.rs`
  (`pr_integration_doctor`).
- HTML marker version → `<!-- plsql-cicd:gate v1 -->` (bumps with
  envelope schema version).

## Bead chain

CICD-014 (envelope) → CICD-015 (post-pr-comment library) → CICD-016
(idempotent find-existing) → CICD-017 (`.github/workflows/plsql-gate.yml`
reference) → CICD-018 (`.gitlab-ci.yml` reference) → CICD-019
(`Jenkinsfile.groovy` reference) → CICD-020 (this file, companion-repo
cross-links) → CICD-022 (PR-integration doctor).

When the companion repo lands, this page gains a `Releases &
versioning` section pinning template tag → engine version pairs.
