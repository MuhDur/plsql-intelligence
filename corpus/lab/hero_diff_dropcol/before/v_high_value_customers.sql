-- hero_diff_dropcol/before — view dependent on customers.legacy_segment.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (before state).
--
-- SELECT-references customers.legacy_segment directly.
-- After DROP COLUMN customers.legacy_segment, this view becomes INVALID.

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
