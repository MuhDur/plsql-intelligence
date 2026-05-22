-- hero_diff_dropcol/after — view updated to remove legacy_segment reference.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (after state).
--
-- After DROP COLUMN customers.legacy_segment, the view must be rewritten
-- to not reference the dropped column.  This "after" version selects
-- by region instead (the developer's chosen migration path).

CREATE OR REPLACE VIEW v_high_value_customers AS
    SELECT
        customer_id,
        customer_name,
        email,
        region,
        created_at
    FROM customers
    WHERE region IS NOT NULL;
