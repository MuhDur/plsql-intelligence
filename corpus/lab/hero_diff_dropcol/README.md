# L1 hero diff — DROP COLUMN `customers.legacy_segment` golden artifact

PLSQL-LAB-008. The **§1.4 commercial nucleus** hero scenario: a DBA drops
a column from the `customers` table and wants to know, before running the
DDL in production, which PL/SQL objects will break.

This is the **real** homepage hero — the purchase-justification scenario
the product leads with in every customer-visible artifact.

## The Question

> I'm about to run `ALTER TABLE customers DROP COLUMN legacy_segment` in
> production. **What breaks?**

## What Oracle Confirms (Ground Truth)

The following objects reference `customers.legacy_segment` and become
`INVALID` immediately when the column is dropped:

| Object | Type | Why It Breaks |
|--------|------|---------------|
| `v_high_value_customers` | VIEW | SELECT-references the column directly in select list + WHERE clause |
| `pkg_customer_report` | PACKAGE BODY | Three internal references: WHERE clause, GROUP BY, %TYPE anchor + SELECT |
| `proc_segment_summary` | PROCEDURE | %TYPE anchor on the column + SELECT in FOR loop cursor |

The `pkg_customer_report` PACKAGE SPEC stays **VALID** — it does not
reference `legacy_segment` — so callers of the package API do not need
to change their call sites.

## Files

| File | Purpose |
|------|---------|
| `before/customers.sql` | `customers` table WITH `legacy_segment` column |
| `before/v_high_value_customers.sql` | View SELECT-ing `legacy_segment` |
| `before/pkg_customer_report.pks` | Package spec (no column ref; stays VALID) |
| `before/pkg_customer_report.pkb` | Package body (three column refs; goes INVALID) |
| `before/proc_segment_summary.sql` | Standalone procedure (%TYPE anchor; goes INVALID) |
| `after/customers.sql` | `customers` table WITHOUT `legacy_segment` |
| `after/v_high_value_customers.sql` | View rewritten to not reference the column |
| `after/pkg_customer_report.pks` | Spec unchanged |
| `after/pkg_customer_report.pkb` | Body rewritten to remove column references |
| `after/proc_segment_summary.sql` | Procedure rewritten to remove column references |
| `change.diff` | Unified diff (what `parse_change_file` consumes) |
| `expected_what_breaks.json` | Golden artifact the engine should emit |

## Two Heroes in `corpus/lab/`

The lab has two hero scenarios serving different product narratives:

| Hero | Corpus dir | Scenario | Showcases |
|------|-----------|----------|-----------|
| §1.4 DROP COLUMN | `hero_diff_dropcol/` | `ALTER TABLE customers DROP COLUMN legacy_segment` | **Commercial nucleus** — the homepage headline, the sales demo |
| LAB-002 param rename | `hero_diff/` | `p_emp_id → p_employee_id` on `pkg_employee_mgmt` | Lineage engine showcase — named-notation breakage detection |

Do **not** delete or modify `hero_diff/` — it is a valid L1 lineage
fixture for LAB-002 with its own golden (`expected_what_breaks.json`
identifying named-notation callers that break).

## How the Golden Was Authored

`expected_what_breaks.json` was authored by:

1. Loading `before/` into a scratch Oracle XE 23ai schema (`HEROCOL_T_<pid>`).
2. Executing `ALTER TABLE customers DROP COLUMN legacy_segment`.
3. Querying `ALL_OBJECTS.STATUS` — Oracle itself confirmed the three dependents are INVALID.
4. Transcribing the Oracle-verified breakage set into the golden.

The live test `crates/plsql-mcp/tests/hero_demo_dropcol_live_xe.rs`
repeats this end-to-end and asserts the agent-discovered breakage set
matches this golden.

## /oracle skill anchors

* PL/SQL Language Reference — Column Dependency Invalidation: when a
  table column is dropped, Oracle marks all stored objects that reference
  that column as INVALID immediately.
* DATABASE-REFERENCE.md §ALL_DEPENDENCIES — `ALL_OBJECTS.STATUS` reflects
  compile state; INVALID objects recompile lazily on next access.
* plan.md §1.4 — This scenario is the canonical product proof; it must
  render cleanly with the Trust Block intact (§1.5) before any GA release.
