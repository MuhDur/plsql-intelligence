# Rust developer demo — type-safe PL/SQL bindings

You're calling Oracle PL/SQL from a Rust application. `plsql-bindgen`
emits strongly-typed wrappers from an `AnalysisRun` so you don't
spell out bind variables, parameter modes, or result destructuring
by hand.

## Setup

```sh
git clone <repo> && cd oracle
cargo build --workspace
```

## Step 1 — the canonical type-mapping table

| Oracle type | Rust type |
|-------------|-----------|
| `NUMBER(p, 0)` where `p ≤ 18` | `i64` |
| `NUMBER` (unconstrained) or `NUMBER(p, m)` | `rust_decimal::Decimal` |
| `BINARY_FLOAT` / `BINARY_DOUBLE` | `f32` / `f64` |
| `VARCHAR2(n)` / `CHAR(n)` / `CLOB` | `String` |
| `BLOB` / `RAW(n)` | `Vec<u8>` |
| `DATE` | `crate::oracle_types::OracleDateTime` |
| `TIMESTAMP(p)` family | `crate::oracle_types::OracleTimestamp{,Tz,Ltz}` |
| `INTERVAL DAY TO SECOND` | `chrono::Duration` |
| `INTERVAL YEAR TO MONTH` | `crate::oracle_types::IntervalYM` |
| `BOOLEAN` (Oracle 23ai+) | `bool` |
| `JSON` (Oracle 21c+) | `serde_json::Value` |
| `OBJECT` types | generated struct under `crate::types::<owner>::<Name>` |
| `NESTED TABLE` / `VARRAY` | `Vec<Element>` |

Source of truth: `crates/plsql-bindgen/src/type_mapping.rs`.
Full reference with hard-parts caveats: `docs/components/bindings.md`.

## Step 2 — generate wrappers (planned)

```sh
plsql-bindgen --package BILLING.INVOICES_PKG \
              --output src/generated/billing \
              --target rust
```

(CLI lands with PLSQL-BG-012; today the wrapper emitter lives at
`plsql-bindgen::emit::emit_wrappers`.)

## Step 3 — call the wrapper

```rust
use generated::billing::invoices_pkg;

let invoice_id = invoices_pkg::create_invoice(
    &conn,
    /* p_customer_id */ 42_i64,
    /* p_amount      */ rust_decimal::Decimal::new(15000, 2), // $150.00
)?;

println!("Created invoice {invoice_id}");
```

`Defaulted<T>` semantics (`PLSQL-BG-010`) carry the
`Omit` vs `Null` vs `Value(T)` distinction for parameters with a
`DEFAULT` clause:

```rust
use plsql_runtime::Defaulted;

invoices_pkg::fire_employee(
    &conn,
    /* p_employee_id  */ Defaulted::Value(99_i64),
    /* p_reason       */ Defaulted::Null,            // pass NULL
    /* p_effective_at */ Defaulted::Omit,            // use Oracle-side default
)?;
```

## Step 4 — handle `BindingDiagnostic`s

The generator never emits a partial wrapper. When it can't reduce a
construct to a safe Rust API, it emits a typed `BindingDiagnostic`
with a stable code + a manual-override remediation:

| Code | Construct | Manual override |
|------|-----------|----------------|
| `BG_UNSUPPORTED_BOOLEAN` | PL/SQL `BOOLEAN` parameter | SQL wrapper converting to `NUMBER(1)` |
| `BG_UNSUPPORTED_REF_CURSOR` | `REF CURSOR` return | `.plsql-bindgen.toml` `[row_shape]` override |
| `BG_UNSUPPORTED_PIPELINED` | Pipelined function | manual cursor wrapper (template in bindings.md) |
| `BG_UNSUPPORTED_ASSOC_ARRAY` | Associative array param | convert to SQL nested table at call site |
| `BG_UNSUPPORTED_PLSQL_RECORD` | PL/SQL `RECORD` | mirror as SQL `OBJECT` type |
| `BG_OVERLOAD_AMBIGUITY` | Multiple overloads collapse | disambiguate via `.plsql-bindgen.toml` |
| ... | (full list in `BindingDiagnosticCode`) | |

## Step 5 — async / sync boundary

`OracleExecutor` trait is sync; the async variant is opt-in. Public
library APIs stay sync-first unless explicitly documented otherwise
(AGENTS.md). Wrap the executor in a tokio task if you need the call
inside an async context.

## Notes

- The generator is **deterministic** — same `AnalysisRun` produces
  byte-identical output. Tests that diff generated code work.
- ORA-NNNN codes round-trip into typed Rust errors, not strings.
- `plsql-bindgen` does not embed the Oracle driver — your
  application picks `rust-oracle 0.6+` (or whatever is current) and
  the generated wrappers call through it.
