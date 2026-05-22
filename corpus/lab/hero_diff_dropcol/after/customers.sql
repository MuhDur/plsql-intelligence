-- hero_diff_dropcol/after — customers table WITHOUT legacy_segment column.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (after state).
--
-- This is the state after the DBA runs:
--   ALTER TABLE customers DROP COLUMN legacy_segment;
--
-- The "after" representation for documentation/diff purposes.
-- The live test uses the actual Oracle DDL to drop the column.

CREATE TABLE customers (
    customer_id     NUMBER(10)    NOT NULL,
    customer_name   VARCHAR2(200) NOT NULL,
    email           VARCHAR2(320),
    phone           VARCHAR2(40),
    region          VARCHAR2(60),
    created_at      DATE          DEFAULT SYSDATE,
    CONSTRAINT customers_pk PRIMARY KEY (customer_id)
);
