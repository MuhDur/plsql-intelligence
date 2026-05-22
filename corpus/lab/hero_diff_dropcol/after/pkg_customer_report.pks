-- hero_diff_dropcol/after — package spec (unchanged after DROP COLUMN).
-- PLSQL-LAB-008 / §1.4 DROP COLUMN hero scenario (after state).
--
-- The spec itself is unchanged: it does not reference legacy_segment.
-- The public API is preserved; callers do not need to change.

CREATE OR REPLACE PACKAGE pkg_customer_report
AS
    TYPE t_segment_rec IS RECORD (
        segment_name   VARCHAR2(30),
        customer_count NUMBER(10),
        region         VARCHAR2(60)
    );

    TYPE t_segment_tab IS TABLE OF t_segment_rec INDEX BY PLS_INTEGER;

    FUNCTION get_customers_by_segment(
        p_segment IN VARCHAR2
    ) RETURN SYS_REFCURSOR;

    FUNCTION get_segment_summary RETURN t_segment_tab;

    PROCEDURE audit_segment_access(
        p_customer_id IN NUMBER,
        p_segment     IN VARCHAR2
    );

END pkg_customer_report;
