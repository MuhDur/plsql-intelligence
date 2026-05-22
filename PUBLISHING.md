# Publishing checklist

Maintainer notes for taking this repository public. Nothing here runs
automatically; publishing is a deliberate manual step.

## State of this repository

This tree has been prepared for an open-source release:

- No references to any private estate or private company remain in the
  source, docs, scripts, corpus, or configuration. The private estate is
  addressed only through the `PLSQL_PRIVATE_ESTATE` environment variable.
- Every crate is dual-licensed `Apache-2.0 OR MIT`. There is no
  source-available or commercial-tier code; the MCP server is a single
  unified crate (`plsql-mcp`).
- The whole workspace is `#![forbid(unsafe_code)]` and builds, tests, and
  clippy-clean on stable Rust 1.85+.

If this tree was assembled as a fresh repository, its git history begins
at the initial commit and carries none of the above either:

```sh
git log --all --oneline | wc -l   # expect a small, clean initial history
```

## Before the first push

1. **Replace the badge owner.** `README.md` uses `OWNER` as a placeholder
   in the CI and USR Loop badge URLs. Replace `OWNER` with your GitHub
   org or username:

   ```sh
   sed -i 's#github.com/OWNER/plsql-intelligence#github.com/<you>/plsql-intelligence#g' README.md
   ```

2. **Run the full gate locally:**

   ```sh
   cargo build --workspace
   cargo test  --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

3. **Confirm the working tree is clean** — no stray absolute paths,
   private hostnames, or internal project identifiers, and `git status`
   shows only intended files.

## Pushing

```sh
# create an empty repo named plsql-intelligence on GitHub first, then:
git remote add origin git@github.com:<you>/plsql-intelligence.git
git push -u origin main
```

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
`plsql-output`, `plsql-render`, `plsql-store`, then the parser, IR,
catalog, engine, product, and MCP layers last). The vendored ANTLR
grammar under `crates/plsql-parser-antlr/grammars/` is Apache-2.0 and is
already declared in that crate's `include` list.
