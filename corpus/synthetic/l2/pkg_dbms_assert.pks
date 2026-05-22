-- L2 DBMS_ASSERT scenarios (PLSQL-LAB-004).
-- Routines that pass user input through DBMS_ASSERT validators before
-- splicing into EXECUTE IMMEDIATE — SAST rules should recognise these
-- as sanitised (true negatives) rather than flagging them as injection
-- candidates (false positives).

CREATE OR REPLACE PACKAGE pkg_dbms_assert_demo
AS
    PROCEDURE delete_from_table(p_table VARCHAR2);
    PROCEDURE rename_column(p_table VARCHAR2, p_old VARCHAR2, p_new VARCHAR2);
    FUNCTION schema_owned_count(p_schema VARCHAR2, p_table VARCHAR2)
        RETURN NUMBER;
    PROCEDURE quote_label(p_label IN OUT VARCHAR2);
END pkg_dbms_assert_demo;
/
