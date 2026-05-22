# `corpus/lab/l3/` — L3 realism fixtures

L3 corpus expansion per `PLSQL-LAB-006`. Each file exercises one
Oracle-specific complexity the engine + lineage layers must model:

| File | Feature | Expected analyzer reaction |
|------|---------|----------------------------|
| `pkg_db_link_caller.{pks,pkb}` | Cross-schema call via `@remote_db` | `EdgeKind::DbLink` |
| `wrapped_pkg.pkb` | Wrapped package body | `UnknownReason::WrappedSource` |
| `spec_with_missing_body.pks` | Spec with no body | doctor missing-body warning |
| `pkg_cc_flags.pks` | `$IF $$debug $THEN` conditional compilation | selected-source view per `AnalysisProfile::plsql_ccflags` |
| `pkg_autonomous.{pks,pkb}` | `PRAGMA AUTONOMOUS_TRANSACTION` | independent-transaction marker on the routine binding |
| `edition_view.sql` | Editioning view + cross-edition INSTEAD OF trigger | `EditionedObject` hint + trigger→view edge |
| `pkg_opaque_dynamic.{pks,pkb}` | EXECUTE IMMEDIATE with runtime-built SQL | `EdgeKind::OpaqueDynamic` + `UnknownReason::DynamicSqlOpaque` |

These fixtures intentionally include patterns the analyzer **cannot**
fully resolve from source alone — the engine's job is to record the
uncertainty (typed `UnknownReason`) rather than silently drop the
edge or guess.

## Test harness contract

Downstream tests against this corpus:

1. Parse + lower each file via `plsql-parser-antlr::lower::lower_source`
2. Build a depgraph fact set using the lab catalog snapshot + plscope
   data under `corpus/lab/{snapshots,plscope}/`
3. Assert: every L3 file produces at least one `UnknownReason` or
   `OpaqueDynamic` edge attributable to its specific feature pattern.

## Notes

- All files are synthetic; no private estate reference (C5/C6).
- The `wrapped_pkg.pkb` marker is a structural placeholder — real
  Oracle wrapping emits a binary `a000000` header + base64 payload.
  The ingestion path treats this fixture the same way for the
  WrappedSource code path.
