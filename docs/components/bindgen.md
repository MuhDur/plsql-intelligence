# plsql-bindgen

Generates type-safe Rust bindings for PL/SQL package calls. Layer 3.

## Purpose

PL/SQL packages are difficult to call from Rust today: every call site
has to spell out bind variables, parameter modes, and result destructuring
by hand. `plsql-bindgen` consumes an `AnalysisRun` and emits a Rust
module per package with strongly-typed `async` wrappers, default-aware
arguments (`Defaulted<T>`), and structured error mapping.

## Surface (planned)

| Function | Returns |
|----------|---------|
| `generate_for_package(&AnalysisRun, package_name)` | A `BindingArtifact` (Rust source + companion `.rs.rs`) |
| `generate_workspace(&AnalysisRun, output_dir)` | Per-package modules + a top-level `lib.rs` |
| `BindingDiagnostic` | One per unsupported construct (overloads we can't disambiguate, `%TYPE` of opaque view, …) |

## Conventions

- **Sync-first public API.** `OracleExecutor` trait is sync; the async
  variant is opt-in. Public library APIs stay sync unless explicitly
  documented otherwise (AGENTS.md).
- **`Defaulted<T>` wrapper** captures whether a caller wanted the
  Oracle-side default or an explicit value.
- **Error mapping is structured.** ORA-NNNN codes become typed Rust
  errors, not strings.

## Pointers

- Source: `crates/plsql-bindgen/src/`
- Plan: `plan.md` §13 (Layer 3 Bindings Generator)
- Upstream: `plsql-engine`, `plsql-catalog`, `plsql-symbols`
- Downstream: end-user Rust applications calling Oracle PL/SQL
