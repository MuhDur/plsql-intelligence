CREATE OR REPLACE PACKAGE BODY pkg_definer_view
AS
    FUNCTION read_count(p_table VARCHAR2) RETURN NUMBER
    IS
        v_count NUMBER;
    BEGIN
        EXECUTE IMMEDIATE 'SELECT COUNT(*) FROM ' || dbms_assert.sql_object_name(p_table)
            INTO v_count;
        RETURN v_count;
    END read_count;
END pkg_definer_view;
/

CREATE OR REPLACE PACKAGE BODY pkg_invoker_view
AS
    FUNCTION read_count(p_table VARCHAR2) RETURN NUMBER
    IS
        v_count NUMBER;
    BEGIN
        EXECUTE IMMEDIATE 'SELECT COUNT(*) FROM ' || dbms_assert.sql_object_name(p_table)
            INTO v_count;
        RETURN v_count;
    END read_count;
END pkg_invoker_view;
/
