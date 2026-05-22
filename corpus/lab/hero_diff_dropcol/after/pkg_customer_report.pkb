-- hero_diff_dropcol/after — package body updated to remove legacy_segment references.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (after state).
--
-- Every reference to customers.legacy_segment has been removed:
--   1. get_customers_by_segment: now filters by region instead
--   2. get_segment_summary: groups by region only
--   3. audit_segment_access: %TYPE anchor removed; parameter p_segment is
--      now validated against a segment_code column (or simply logged)

CREATE OR REPLACE PACKAGE BODY pkg_customer_report
AS

    FUNCTION get_customers_by_segment(
        p_segment IN VARCHAR2
    ) RETURN SYS_REFCURSOR
    IS
        v_cur SYS_REFCURSOR;
    BEGIN
        -- After DROP COLUMN: filter by region (p_segment now maps to region).
        OPEN v_cur FOR
            SELECT customer_id, customer_name, email, region, created_at
            FROM   customers
            WHERE  region = p_segment
            ORDER BY customer_name;
        RETURN v_cur;
    EXCEPTION
        WHEN OTHERS THEN
            RAISE_APPLICATION_ERROR(-20010,
                'pkg_customer_report.get_customers_by_segment failed: ' || SQLERRM);
    END get_customers_by_segment;

    FUNCTION get_segment_summary RETURN t_segment_tab
    IS
        v_result t_segment_tab;
        v_idx    PLS_INTEGER := 1;

        CURSOR c_summary IS
            SELECT region AS seg_name, COUNT(*) AS seg_count, region AS seg_region
            FROM   customers
            WHERE  region IS NOT NULL
            GROUP BY region
            ORDER BY region;
    BEGIN
        FOR rec IN c_summary LOOP
            v_result(v_idx).segment_name   := rec.seg_name;
            v_result(v_idx).customer_count := rec.seg_count;
            v_result(v_idx).region         := rec.seg_region;
            v_idx := v_idx + 1;
        END LOOP;
        RETURN v_result;
    EXCEPTION
        WHEN OTHERS THEN
            RAISE_APPLICATION_ERROR(-20011,
                'pkg_customer_report.get_segment_summary failed: ' || SQLERRM);
    END get_segment_summary;

    PROCEDURE audit_segment_access(
        p_customer_id IN NUMBER,
        p_segment     IN VARCHAR2
    )
    IS
        v_region customers.region%TYPE;
    BEGIN
        -- After DROP COLUMN: validate against region instead.
        SELECT region
        INTO   v_region
        FROM   customers
        WHERE  customer_id = p_customer_id;

        IF v_region != p_segment THEN
            RAISE_APPLICATION_ERROR(-20012,
                'Region mismatch for customer ' || p_customer_id ||
                ': stored=' || v_region || ', supplied=' || p_segment);
        END IF;
    EXCEPTION
        WHEN NO_DATA_FOUND THEN
            RAISE_APPLICATION_ERROR(-20013,
                'Customer not found: ' || p_customer_id);
    END audit_segment_access;

END pkg_customer_report;
