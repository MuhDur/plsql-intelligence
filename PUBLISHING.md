# Publishing checklist

Maintainer notes for taking this repository public. Nothing here runs
automatically; publishing is a deliberate manual step.

## State of this repository

This tree has been prepared for an open-source release:

- No references to any private estate or private company remain in the
  source, docs, scripts, corpus, or configuration. The private estate is
  addressed only through the `PLSQL_PRIVATE_ESTATE` environment variable.
- Every crate is dual-licensed `Apache-2.0 OR MIT`. There is no
  source-available or commercial-tier code. MCP serving belongs in the
  separate `oraclemcp` repository; this tree publishes only the offline
  PL/SQL engine crates and CLIs.
- The offline workspace is `#![forbid(unsafe_code)]` and builds, tests, and
  clippy-clean on the stable toolchain used by the default CI profile.

If this tree was assembled as a fresh repository, its git history begins
at the initial commit and carries none of the above either:

```sh
git log --all --oneline | wc -l   # expect a small, clean initial history
```

## Forking

The README badge URLs and the workspace `Cargo.toml` `repository` /
`homepage` fields point at `github.com/MuhDur/plsql-intelligence`. If
you fork, replace `MuhDur` with your handle:

```sh
sed -i 's#github.com/MuhDur/plsql-intelligence#github.com/<you>/plsql-intelligence#g' README.md Cargo.toml
```

## Local gate before pushing changes

```sh
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Confirm `git status` shows only intended files — no stray absolute
paths, private hostnames, or internal project identifiers.

`.github/workflows/ci.yml` and `.github/workflows/usr.yml` run on the
first push. The USR Loop nightly acceptance proof honestly SKIPs on CI
runners (no private estate) and is only fully exercised on a host where
`PLSQL_PRIVATE_ESTATE` points at a real estate.

## After the push

- Set the repository social-preview image (Settings → General → Social
  preview) from `.github/assets/hero.svg`.
- The `About Contributions` section in the README states the
  no-outside-contributions policy; consider disabling the PR template or
  pinning an issue that restates it.

## Optional: publishing crates to crates.io

The workspace is pre-1.0; the API can still move. If you publish to
crates.io, publish in dependency order, leaves first (`plsql-core`,
`plsql-render`, `plsql-store`, then output/parser/catalog foundations,
semantic crates, and product crates such as `plsql-engine`,
`plsql-depgraph`, `plsql-cicd`, and `plsql-accretion` last). The
vendored ANTLR grammar under `crates/plsql-parser-antlr/grammars/` is
Apache-2.0 and is already declared in that crate's `include` list.
