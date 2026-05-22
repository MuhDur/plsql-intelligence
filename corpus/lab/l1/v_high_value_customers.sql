-- L1 hero corpus: view dependent on customers.legacy_segment.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario.
--
-- This view SELECT-references customers.legacy_segment directly.
-- When the column is dropped, this view becomes INVALID immediately
-- (Oracle marks it so on next access or recompile attempt).

CREATE OR REPLACE VIEW v_high_value_customers AS
    SELECT
        customer_id,
        customer_name,
        email,
        region,
        legacy_segment,
        created_at
    FROM customers
    WHERE legacy_segment IS NOT NULL;
