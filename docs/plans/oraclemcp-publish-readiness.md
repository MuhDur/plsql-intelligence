# oraclemcp — publish-readiness checklist (Phase E)

Status as of 2026-06-08. This is the turnkey handoff for extracting and
publishing the engine-free `oraclemcp-*` MCP core as the standalone public
`oraclemcp` project. Everything below the **"BLOCKED — needs operator"** line
requires a credential or an outward-facing decision; everything above it is
verified.

## Verified prerequisites (green)

- **Boundary holds.** The 8 `oraclemcp-*` crates form a closed dependency set:
  none imports any `plsql-*` engine crate (checked per-crate; the CI
  `oraclemcp-boundary` job + `scripts/oraclemcp_boundary_lint.sh` enforce it).
  Internal DAG (topological build/publish order):

  ```
  oraclemcp-error                     (leaf)
  oraclemcp-telemetry  → error
  oraclemcp-audit      → error
  oraclemcp-guard      → audit, error
  oraclemcp-config     → error, guard
  oraclemcp-db         → error, guard
  oraclemcp-auth       → audit, error, guard
  oraclemcp-core       → all of the above
  ```

- **Names available.** All 8 names — `oraclemcp-core`, `oraclemcp-error`,
  `oraclemcp-guard`, `oraclemcp-db`, `oraclemcp-audit`, `oraclemcp-auth`,
  `oraclemcp-telemetry`, `oraclemcp-config` — return 404 on the crates.io API
  (unclaimed) as of 2026-06-08.

- **`#![forbid(unsafe_code)]`** on every `oraclemcp-*` crate (soundness audit
  confirmed airtight).

- **Green.** The crates build and test inside the workspace (part of the 2603
  passing tests; `clippy -D warnings` clean; `cargo-deny` clean).

## Metadata flips required before publish (deliberate, ~30 min)

Each is currently set the way Phase A wants (a bounded in-workspace module), and
must be flipped at extraction:

1. **`publish = false` → publishable.** Every `oraclemcp-*/Cargo.toml` has
   `publish = false` (intentional Phase-A gate). Remove the line.
2. **Path deps need a version.** Inter-`oraclemcp` deps are `path`-only
   (`oraclemcp-error = { path = "../oraclemcp-error" }`). crates.io requires a
   version on a path dep — change to
   `{ path = "...", version = "0.1.0" }` (or, post-extraction, version-only).
3. **`repository` / `homepage`.** The crates inherit `[workspace.package]`
   pointing at `MuhDur/plsql-intelligence`; point them at the new
   `MuhDur/oraclemcp` repo.
4. **Per-crate `readme`, `keywords`, `categories`** for the crates.io page
   (`oracle`, `mcp`, `plsql`, `database`; categories `database`,
   `development-tools`).

## Extraction recipe (reversible until the final push)

`git-filter-repo` is installed. From a fresh clone:

```sh
git clone https://github.com/MuhDur/plsql-intelligence /tmp/oraclemcp-extract
cd /tmp/oraclemcp-extract
git filter-repo \
  --path crates/oraclemcp-core/   --path crates/oraclemcp-error/ \
  --path crates/oraclemcp-guard/  --path crates/oraclemcp-db/ \
  --path crates/oraclemcp-audit/  --path crates/oraclemcp-auth/ \
  --path crates/oraclemcp-telemetry/ --path crates/oraclemcp-config/ \
  --path LICENSE-APACHE --path LICENSE-MIT \
  --path-rename crates/:crates/
# then: write a new root Cargo.toml ([workspace] over crates/oraclemcp-*),
#       a new README.md (outline below), the metadata flips above, and a
#       minimal CI (fmt/clippy/test/deny) + release workflow;
#       `cargo build && cargo test && cargo publish --dry-run -p oraclemcp-error`.
```

History is preserved (filter-repo keeps each crate's commits). The boundary
guarantees the extracted tree compiles standalone.

## Publish order (topological)

`oraclemcp-error` → `oraclemcp-telemetry` → `oraclemcp-audit` →
`oraclemcp-guard` → `oraclemcp-config` → `oraclemcp-db` → `oraclemcp-auth` →
`oraclemcp-core`. Wait for each to index on crates.io before the next that
depends on it.

## oraclemcp README outline (to write in the extracted repo)

- Hero + one-line: "Safe-by-default Oracle MCP server core, in pure Rust."
- Why: the fail-closed SQL guard (operating-level ladder, byte-verified
  guarded writes), the engine-free boundary, agent-first UX (per-tool schemas,
  structured error envelopes + fuzzy suggestions, zero-arg `oracle_capabilities`
  discovery, RuntimeStateRequired degradation), `#![forbid(unsafe_code)]`.
- Crate map (the DAG above), with a one-line role per crate.
- Quick start: add `oraclemcp-core`, register tools, serve over stdio/TCP.
- Safety model: the SQL guard's fail-closed invariants (the hardened classifier).
- License Apache-2.0 OR MIT; vendored grammar attribution N/A here (engine-free).

---

## BLOCKED — needs operator

- **crates.io token.** No `CARGO_REGISTRY_TOKEN` env and no
  `~/.cargo/credentials`. `cargo publish` cannot run without it. Provide a token
  (`cargo login`) or run the publish steps yourself.
- **New public repo + final push.** Creating `MuhDur/oraclemcp` and pushing the
  extracted history is an outward-facing, one-way action — left for an explicit
  go-ahead.
- **Docker image** (Instant Client base) and **MCP-registry listing** are the
  last two Phase-E steps (`oracle-qmwz.5.3` / `.5.5`), gated on the publish.
