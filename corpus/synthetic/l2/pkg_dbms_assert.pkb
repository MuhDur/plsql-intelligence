CREATE OR REPLACE PACKAGE BODY pkg_dbms_assert_demo
AS
    -- Demonstrates each DBMS_ASSERT validator: sql_object_name,
    -- simple_sql_name, qualified_sql_name, schema_name, and
    -- enquote_name. SAST rules in plsql-scan should recognise each as
    -- mitigating an EXECUTE IMMEDIATE injection risk on the bound
    -- argument. SAST rules that don't track these false-positive on
    -- safe code.

    PROCEDURE delete_from_table(p_table VARCHAR2)
    IS
    BEGIN
        EXECUTE IMMEDIATE 'DELETE FROM ' || dbms_assert.sql_object_name(p_table);
    END delete_from_table;

    PROCEDURE rename_column(p_table VARCHAR2, p_old VARCHAR2, p_new VARCHAR2)
    IS
    BEGIN
        EXECUTE IMMEDIATE 'ALTER TABLE '
            || dbms_assert.sql_object_name(p_table)
            || ' RENAME COLUMN '
            || dbms_assert.simple_sql_name(p_old)
            || ' TO '
            || dbms_assert.simple_sql_name(p_new);
    END rename_column;

    FUNCTION schema_owned_count(p_schema VARCHAR2, p_table VARCHAR2)
        RETURN NUMBER
    IS
        v_count NUMBER;
    BEGIN
        EXECUTE IMMEDIATE 'SELECT COUNT(*) FROM '
            || dbms_assert.schema_name(p_schema)
            || '.'
            || dbms_assert.simple_sql_name(p_table)
            INTO v_count;
        RETURN v_count;
    END schema_owned_count;

    PROCEDURE quote_label(p_label IN OUT VARCHAR2)
    IS
    BEGIN
        p_label := dbms_assert.enquote_name(p_label);
    END quote_label;
END pkg_dbms_assert_demo;
/
