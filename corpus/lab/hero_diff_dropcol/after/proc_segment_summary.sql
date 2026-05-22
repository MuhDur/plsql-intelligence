-- hero_diff_dropcol/after — standalone procedure updated to remove legacy_segment.
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (after state).
--
-- The %TYPE anchor and all legacy_segment column references have been removed.
-- Now summarises by region instead.

CREATE OR REPLACE PROCEDURE proc_segment_summary(
    p_region IN VARCHAR2 DEFAULT NULL
)
IS
    v_region customers.region%TYPE;
    v_count  NUMBER;
BEGIN
    FOR rec IN (
        SELECT region AS seg_name, COUNT(*) AS cnt
        FROM   customers
        WHERE  (p_region IS NULL OR region = p_region)
          AND  region IS NOT NULL
        GROUP BY region
        ORDER BY COUNT(*) DESC
    ) LOOP
        v_region := rec.seg_name;
        v_count  := rec.cnt;
        DBMS_OUTPUT.PUT_LINE('Region: ' || v_region || '  Count: ' || v_count);
    END LOOP;
EXCEPTION
    WHEN OTHERS THEN
        RAISE_APPLICATION_ERROR(-20020,
            'proc_segment_summary failed: ' || SQLERRM);
END proc_segment_summary;
