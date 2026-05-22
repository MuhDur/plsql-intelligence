# plsql-bindgen — design + reference

> Generates type-safe Rust bindings for PL/SQL package calls.
> Layer 3 (plan.md §13).

`plsql-bindgen` consumes an `AnalysisRun` (Layer 2.5) and emits a Rust
module per PL/SQL package with strongly-typed wrappers, default-aware
parameters, structured error mapping, and explicit `BindingDiagnostic`s
for every construct the generator cannot reduce to a safe Rust API.

This document covers:

1. Public surface
2. Oracle → Rust type mapping (the §12.3 canonical table, as shipped)
3. Hard-parts caveats (every `BG_UNSUPPORTED_*` code, with rationale)
4. Manual-override patterns (per-construct workarounds the user can adopt)
5. Driver-capability matrix
6. Determinism + R13 contract

The source of truth for the mapping table is
`crates/plsql-bindgen/src/type_mapping.rs`. When this doc and that file
disagree, fix one or the other deliberately — never silently.

## 1. Public surface

| Function | Returns |
|----------|---------|
| `map_oracle_type(&OracleType, span, routine) -> TypeMapping` | Per-type mapping decision; `Supported(RustTypeRef)` or `UnsupportedConstruct(BindingDiagnostic)`. |
| `with_nullable(TypeMapping, bool) -> TypeMapping` | Wraps a supported mapping in `Option<T>`; passes through unsupported mappings unchanged. |
| `BindingDiagnostic::new_unsupported(code, routine, span)` | Constructs the typed diagnostic for unsupported constructs; carries remediation text built from `code`. |
| `BindingDiagnosticCode` | Enumerates every reason the generator refuses to emit. See §3. |
| `generate_for_package(&AnalysisRun, package_name)` *(planned, gated on `PLSQL-BG-004`)* | A `BindingArtifact` (Rust source + companion `.rs.rs`). |
| `generate_workspace(&AnalysisRun, output_dir)` *(planned)* | Per-package modules + a top-level `lib.rs`. |

Today the type-mapper and the diagnostic surface are complete; the
wrapper emitter (PLSQL-BG-004) is the remaining major piece.

## 2. Oracle → Rust type mapping

The canonical table from `plan.md` §12.3, as actually implemented in
`type_mapping.rs::map_oracle_type`:

| Oracle type | Rust type | Notes |
|-------------|-----------|-------|
| `NUMBER(p, 0)` where `p ≤ 18` | `i64` | Fits in i64 without precision loss. |
| `NUMBER(p, 0)` where `p > 18`, or `NUMBER` (no precision) | `rust_decimal::Decimal` | Conservative fallback — Oracle treats unconstrained `NUMBER` as full-range. |
| `NUMBER(p, m)` where `m ≠ 0` | `rust_decimal::Decimal` | Any fractional scale routes to `Decimal`. |
| `BINARY_FLOAT` | `f32` | |
| `BINARY_DOUBLE` | `f64` | |
| `VARCHAR2(n)` / `CHAR(n)` / `CLOB` | `String` | UTF-8; conversion at the driver layer. |
| `BLOB` / `RAW(n)` | `Vec<u8>` | Owned byte buffer. |
| `DATE` | `crate::oracle_types::OracleDateTime` | DB-side `DATE` carries time-of-day; our wrapper preserves both. |
| `TIMESTAMP(p)` | `crate::oracle_types::OracleTimestamp` | |
| `TIMESTAMP(p) WITH TIME ZONE` | `crate::oracle_types::OracleTimestampTz` | |
| `TIMESTAMP(p) WITH LOCAL TIME ZONE` | `crate::oracle_types::OracleTimestampLtz` | |
| `INTERVAL DAY TO SECOND` | `chrono::Duration` | Documented MINOR DEVIATION: `chrono::Duration` loses sub-millisecond precision; flagged in `/oracle` audit. |
| `INTERVAL YEAR TO MONTH` | `crate::oracle_types::IntervalYM` | Our own carrier type; chrono doesn't represent calendar months. |
| `BOOLEAN` (Oracle 23ai+ SQL `BOOLEAN`) | `bool` | Direct map. |
| PL/SQL `BOOLEAN` (legacy) | **Unsupported** | See `BG_UNSUPPORTED_BOOLEAN` in §3. Not driver-bindable through all clients. |
| `XMLTYPE` | `String` | Round-tripped as serialized XML. |
| `JSON` (Oracle 21c+) | `serde_json::Value` | Native JSON datatype. |
| `REF CURSOR` | **Unsupported** | See `BG_UNSUPPORTED_REF_CURSOR` in §3. Row shape unknown without an explicit override. |
| User-defined `OBJECT` types | `crate::types::<owner>::<Name>` | Generator emits a struct in the namespace named after `owner`. |
| `NESTED TABLE` | `Vec<Element>` | Element type runs through the mapper recursively; element-level failures propagate the diagnostic. |
| `VARRAY` | `Vec<Element>` | Same recursion as nested tables. |
| Associative array (PL/SQL only) | `std::collections::HashMap<K, V>` when key + value both supported | Otherwise emits `BG_UNSUPPORTED_ASSOC_ARRAY`. |
| Anything else | **Unsupported** | Generator never invents a mapping locally. |

Nullability is layered on top by `with_nullable(mapping, nullable)` —
`Option<T>` for `nullable = true`, `T` unchanged otherwise.

## 3. Hard-parts caveats — every `BG_UNSUPPORTED_*` code

`BindingDiagnosticCode` enumerates every construct the generator refuses
to wrap. Each variant carries a stable string code, a one-line message,
and a one-line remediation hint. The pattern is intentional:

> Never emit a partial wrapper. Emit a diagnostic that an agent can read.

| Code | When emitted | Why we refuse |
|------|--------------|---------------|
| `BG_UNSUPPORTED_BOOLEAN` | `PlsqlBoolean` parameter | PL/SQL `BOOLEAN` predates SQL `BOOLEAN` and is not bindable through every Oracle driver (rust-oracle ≥ 0.6 supports it on 23ai+ but not older releases). |
| `BG_UNSUPPORTED_REF_CURSOR` | `REF CURSOR` return | Row shape is unknown without an explicit override; emitting an `impl Stream<Item = Row>` would silently lose typing. |
| `BG_UNSUPPORTED_PIPELINED` | Pipelined function | Current driver surface treats pipelined functions as cursor-bound; we refuse rather than partially wrap. |
| `BG_UNSUPPORTED_ASSOC_ARRAY` | `AssociativeArray` whose key or value maps to another unsupported type | Recursive propagation — element failure escalates. |
| `BG_UNSUPPORTED_PLSQL_RECORD` | Package-scoped `RECORD` without an SQL `OBJECT` analogue | The driver cannot bind PL/SQL records directly; the user must mirror the shape in an SQL `OBJECT` type. |
| `BG_UNSUPPORTED_NESTED_TABLE_IN` | `NESTED TABLE` parameter without a matching SQL collection type | Inline package-local collections aren't bindable. |
| `BG_UNSUPPORTED_VARRAY_IN` | `VARRAY` parameter without a matching SQL type | Same as above. |
| `BG_NON_LITERAL_DEFAULT` | `DEFAULT` expression isn't a literal | Wrapper still emits but the caller must opt in — Oracle-side evaluation needed. |
| `BG_LONG_OR_LONG_RAW` | `LONG` / `LONG RAW` column | Legacy types; mapped to opaque bytes with caveat. |
| `BG_AUTONOMOUS_TX` | `PRAGMA AUTONOMOUS_TRANSACTION` | Wrapper emits; behavior is documented but not abstracted (autonomous tx isolation surfaces to the caller). |
| `BG_INVOKER_RIGHTS_NO_HINT` | `AUTHID CURRENT_USER` without runtime hint | Wrapper emits with caveat; the calling user's effective privileges differ at runtime. |
| `BG_OPAQUE_TYPE` | Oracle opaque type (`XMLTYPE` in some clients, `SYS.ANYDATA`) | Manual interop required. |
| `BG_OVERLOAD_AMBIGUITY` | Multiple overloads collapse to the same Rust signature | Generator cannot disambiguate; wrapper skipped. |
| `BG_WRAPPED_PACKAGE_BODY` | Source body is `WRAP`ped | Cannot read identifiers; wrapper skipped. |
| *(more added as new constructs surface)* | | |

The remediation text for each code is shipped inline in
`BindingDiagnosticCode::remediation_hint`; callers SHOULD render that
text into their developer-facing tooling rather than re-stating it.

## 4. Manual-override patterns

Each unsupported case has at least one documented escape hatch:

- **PL/SQL `BOOLEAN`** — Add a thin SQL wrapper that converts `BOOLEAN`
  to `NUMBER(1)` at the procedure boundary, then bind the wrapper.
- **`REF CURSOR`** — Add a `[row_shape]` override in
  `.plsql-bindgen.toml` mapping the routine to an explicit row type.
- **Pipelined function** — Open a cursor over the function manually and
  consume rows; the wrapper template lives in
  `docs/components/bindings.md` (this file, §6 below).
- **PL/SQL `RECORD`** — Define an SQL `OBJECT` type matching the
  `RECORD` shape; the generator will then bind the SQL type.
- **Nested table / varray parameter** — Move the collection type into
  the SQL namespace (`CREATE TYPE … AS …`) and reference it in the
  signature.
- **Non-literal `DEFAULT`** — Either accept the parameter as required
  in the wrapper, or pre-evaluate the default on the Rust side.
- **`LONG` / `LONG RAW`** — Migrate the column to `CLOB` / `BLOB` to
  get a driver-supported binding shape.
- **Associative array** — Convert to a SQL nested table type at the
  call site, then bind the nested table.
- **Overload ambiguity** — Disambiguate by adding parameter-name-based
  suffixes via `.plsql-bindgen.toml` overrides (planned: PLSQL-BG-007).

## 5. Driver-capability matrix

Today the generator targets `rust-oracle 0.6.x` against Oracle 19c
through 26ai. Capability gates that matter:

| Capability | rust-oracle 0.6 + Oracle ≥ 23ai | Earlier Oracle |
|-----------|---------------------------------|----------------|
| SQL `BOOLEAN` | ✓ | not present in catalog |
| PL/SQL `BOOLEAN` parameter | ✓ (limited) | depends on `OCI` build |
| `JSON` native | ✓ | falls back to `CLOB` |
| `INTERVAL DAY TO SECOND` | ✓ | ✓ |
| `INTERVAL YEAR TO MONTH` | ✓ | ✓ |
| `REF CURSOR` return | manual override required | manual override required |
| `XMLTYPE` | round-tripped as `String` | round-tripped as `String` |
| Pipelined function | manual cursor | manual cursor |

When a capability is missing the generator emits the matching
`BG_UNSUPPORTED_*` diagnostic rather than guessing a fallback.

## 6. Determinism + R13 contract

The generator is **deterministic** — given the same `AnalysisRun` it
produces byte-identical output. This is enforced by:

- Sorted iteration over package members (no `HashMap` traversal in the
  emit phase).
- Stable mapping table — every Oracle type has exactly one Rust answer
  or one diagnostic code.
- Diagnostics are sorted by `(routine, code, span.start)` before
  emission so re-running on identical inputs yields identical output.

The **R13 contract** applies end-to-end: every construct the generator
cannot reduce to a safe Rust API becomes a typed `BindingDiagnostic`
with a stable code and a remediation hint. The generator never silently
drops a routine, parameter, or column. If a future construct surfaces
that doesn't fit any current `BindingDiagnosticCode`, the change is to
add a new variant — never to widen an existing one or hide the case.

## Pointers

- Source: `crates/plsql-bindgen/src/`
- Type mapping: `crates/plsql-bindgen/src/type_mapping.rs`
- Diagnostic codes: `crates/plsql-bindgen/src/lib.rs` (`BindingDiagnosticCode`)
- Plan: `plan.md` §13 (Layer 3 Bindings Generator)
- Companion stub: `docs/components/bindgen.md` (1-page overview; this
  file is the deep reference)
- Upstream: `plsql-engine`, `plsql-catalog`, `plsql-symbols`
- Downstream: end-user Rust applications calling Oracle PL/SQL
