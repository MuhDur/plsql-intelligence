# Contributing — oraclemcp

oraclemcp is built **in place** inside the `plsql-intelligence` workspace (the
`oraclemcp-*` crates) and extracted to its own repo later (plan §0). These notes
cover the conventions a contributor needs.

## The one-way dependency boundary (non-negotiable)

The engine-free `oraclemcp-*` core crates (`-error`, `-config`, `-audit`,
`-telemetry`, `-db`, `-guard`, `-auth`, `-core`) must **never** import a
`plsql-*` engine crate. Engine intelligence reaches the core by the engine-side
code implementing the core's `Tool`/registry and `SideEffectOracle` contracts —
the core never reaches into the engine. CI enforces this
(`scripts/oraclemcp_boundary_lint.sh`). A new tool handler receives engine
results as `AnalysisRun` / `DepGraph` / `CatalogSnapshot` parameters; it does not
`use plsql_engine`.

## The Oracle async/sync pattern (learn this first)

The `oracle` crate is **synchronous**. The one invariant above all: an
`oracle::Connection` is **never held across an `.await`**. All DB I/O crosses an
explicit `tokio::task::spawn_blocking` boundary (see `oraclemcp-db::OraclePool`
and `oraclemcp-core::server`), and ownership-enforcement by the compiler keeps it
true. CPU-bound classification also runs off the async executor.

## Safety discipline

- The whole workspace is `#![forbid(unsafe_code)]`. (Note: `std::env::set_var` is
  `unsafe` under edition 2024 and therefore forbidden — fold wallet locations
  into EZConnect-Plus descriptors instead of mutating `TNS_ADMIN`.)
- The classifier is **fail-closed**. Any change that could clear something to
  `Safe` must be covered by the adversarial corpus
  (`crates/oraclemcp-guard/tests/adversarial_corpus.rs`) and must not regress the
  fuzz target. Never weaken the fail-closed law to admit a statement.
- All TTLs use the monotonic clock (`oraclemcp-guard::MonotonicDeadline`), never
  the wall clock.
- Audit is out-of-band + fsync-before-execute for Guarded+; never log bind
  values or secrets.

## Quality gates (run before a PR)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo deny check
bash scripts/oraclemcp_boundary_lint.sh
```

Build the specific crate you touched (`cargo test -p oraclemcp-<x>`) for a fast
loop; the `oraclemcp-*` crates compile without the ANTLR engine. Live-DB tests
are gated behind `--features live-xe` and skip with a banner when no Oracle is
reachable, so they never fail CI without a database.

## Errors

`thiserror` for library errors; the agent-facing `oraclemcp_error::ErrorEnvelope`
(`isError:true` + a machine-stable `error_class` + actionable next steps) at the
tool boundary — never raw JSON-RPC error codes.

## Tests

Every bead ships its tests: unit + (where a DB is needed) a `live-xe`-gated
integration test. Safety features need an adversarial corpus **and** a
real-Oracle test before backing a production claim (plan §12).
