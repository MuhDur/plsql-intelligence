-- hero_diff_dropcol/before — customers table WITH legacy_segment column.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (before state).
--
-- This is the production state before the DBA runs:
--   ALTER TABLE customers DROP COLUMN legacy_segment;
--
-- Three dependent objects reference legacy_segment:
--   - v_high_value_customers  (VIEW)
--   - pkg_customer_report     (PACKAGE BODY)
--   - proc_segment_summary    (PROCEDURE)
-- All three become INVALID after the drop.

CREATE TABLE customers (
    customer_id     NUMBER(10)    NOT NULL,
    customer_name   VARCHAR2(200) NOT NULL,
    email           VARCHAR2(320),
    phone           VARCHAR2(40),
    region          VARCHAR2(60),
    legacy_segment  VARCHAR2(30),
    created_at      DATE          DEFAULT SYSDATE,
    CONSTRAINT customers_pk PRIMARY KEY (customer_id)
);
