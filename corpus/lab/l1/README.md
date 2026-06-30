# `corpus/lab/l1/` — L1 hero corpus (§1.4 DROP COLUMN scenario)

PLSQL-LAB-008. The **commercial nucleus** hero corpus: a `customers` table
with a `legacy_segment` column that three PL/SQL objects depend on.

This corpus is the fixture for the product's homepage hero scenario:

> **Know what breaks before you DROP COLUMN customers.legacy_segment.**

## Objects

| File | Object | Type | References `legacy_segment`? |
|------|--------|------|-------------------------------|
| `customers.sql` | `customers` | TABLE | defines it |
| `v_high_value_customers.sql` | `v_high_value_customers` | VIEW | yes (SELECT list + WHERE) |
| `pkg_customer_report.pks` | `pkg_customer_report` | PACKAGE SPEC | no (stays VALID after drop) |
| `pkg_customer_report.pkb` | `pkg_customer_report` | PACKAGE BODY | yes (3 references) |
| `proc_segment_summary.sql` | `proc_segment_summary` | PROCEDURE | yes (%TYPE anchor + SELECT) |

## Loaded By

`examples/oracle-xe/setup.sh` loops `for level in l1 l2 l3` and loads
every `.sql`, `.pks`, `.pkb` in lexical order into the `DEMO` schema.
The files are idempotent (`CREATE OR REPLACE` for PL/SQL; table uses a
`BEGIN EXECUTE IMMEDIATE 'DROP TABLE …' EXCEPTION WHEN OTHERS THEN NULL; END;`
guard).

## Hero diff corpus

The scenario golden (before/after/change.diff/expected_what_breaks.json)
lives in `corpus/lab/hero_diff_dropcol/`. Live Oracle replay now belongs
in `oraclemcp`; this repo keeps the offline corpus and expected impact
artifact.

## Two heroes in `corpus/lab/`

| Hero | Dir | Scenario | Showcases |
|------|-----|----------|-----------|
| §1.4 DROP COLUMN | `hero_diff_dropcol/` | `ALTER TABLE customers DROP COLUMN legacy_segment` | **Commercial nucleus** homepage demo |
| LAB-002 param rename | `hero_diff/` | `p_emp_id → p_employee_id` | Lineage engine — named-notation breakage |
