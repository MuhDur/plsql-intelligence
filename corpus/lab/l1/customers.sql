-- L1 hero corpus: customers table WITH legacy_segment column.
-- This is the "before" state — the column exists.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario.
--
-- Loaded by examples/oracle-xe/setup.sh into the DEMO schema.
-- Idempotent: drops existing table if present before recreating.

BEGIN
    EXECUTE IMMEDIATE 'DROP TABLE customers CASCADE CONSTRAINTS';
EXCEPTION WHEN OTHERS THEN NULL;
END;
/

CREATE TABLE customers (
    customer_id     NUMBER(10)    NOT NULL,
    customer_name   VARCHAR2(200) NOT NULL,
    email           VARCHAR2(320),
    phone           VARCHAR2(40),
    region          VARCHAR2(60),
    -- legacy_segment: the column the §1.4 hero DROP COLUMN demo targets.
    -- Populated by the legacy CRM migration (pre-2024). Retained for
    -- reporting while downstream PL/SQL objects still reference it.
    legacy_segment  VARCHAR2(30),
    created_at      DATE          DEFAULT SYSDATE,
    CONSTRAINT customers_pk PRIMARY KEY (customer_id)
);

COMMENT ON TABLE  customers                  IS 'L1 hero corpus: customer master (includes legacy_segment, §1.4 demo)';
COMMENT ON COLUMN customers.legacy_segment   IS 'Legacy CRM segmentation tier — referenced by v_high_value_customers, pkg_customer_report, proc_segment_summary. DROP COLUMN breaks those objects.';
