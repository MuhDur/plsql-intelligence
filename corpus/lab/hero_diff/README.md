# L1 hero diff — `what-breaks` golden artifact

PLSQL-LAB-002. The hero scenario for the `what-breaks --change`
demo: a single-procedure change to `pkg_employee_mgmt` (the L1 seed
hero package) that exercises every layer of the lineage engine —
signature classification, body classification, callers traversal,
and the orphan-vs-cross-impact verdict.

## Scenario

Operator wants to rename the procedure parameter `p_emp_id` to
`p_employee_id` across the public-facing API of `pkg_employee_mgmt`.
This is a textbook breaking change: every caller that uses named
notation (`employee_mgmt.fire_employee(p_emp_id => 42)`) must be
updated.

## Files

| File | Purpose |
|---|---|
| `before/pkg_employee_mgmt.pks` | Original package spec |
| `before/pkg_employee_mgmt.pkb` | Original package body |
| `after/pkg_employee_mgmt.pks` | Spec after the rename |
| `after/pkg_employee_mgmt.pkb` | Body after the rename |
| `change.diff` | Unified diff (what `parse_change_file` consumes) |
| `expected_what_breaks.json` | Golden artifact the engine should emit |

## How the golden was authored

The expected report mirrors the shape of `LineageResult` (the type
returned by `plsql_lineage::impact`) extended with the
`SemanticChangeSet` envelope from
`plsql_lineage::parse_change_file`. Each call site that uses named
notation against the renamed parameter is listed as a breaking
caller; positional callers are listed under
`positional_callers_still_compile` to document the safe path.

The fixture is intentionally tiny — one package, three call sites —
so the golden artifact can be reviewed by a human in under one
minute. Larger end-to-end coverage lives under
`corpus/lab/l3/`.

## /oracle skill anchors

* PL/SQL Language Reference §13 (Parameter Modes and Named
  Notation) — semantics of `=>` argument binding.
* DATABASE-REFERENCE.md PL/SQL routing — for the broader release
  matrix; the rename pattern works identically on 19c/21c/23ai/26ai.
