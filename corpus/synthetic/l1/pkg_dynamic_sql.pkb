CREATE OR REPLACE PACKAGE BODY dynamic_sql_pkg
AS
    FUNCTION get_table_count(
        p_table_name IN VARCHAR2,
        p_where      IN VARCHAR2 DEFAULT NULL
    ) RETURN NUMBER
    IS
        v_sql   VARCHAR2(4000);
        v_count NUMBER;
    BEGIN
        v_sql := 'SELECT COUNT(*) FROM ' || DBMS_ASSERT.SQL_OBJECT_NAME(p_table_name);
        IF p_where IS NOT NULL THEN
            v_sql := v_sql || ' WHERE ' || p_where;
        END IF;

        EXECUTE IMMEDIATE v_sql INTO v_count;
        RETURN v_count;
    EXCEPTION
        WHEN OTHERS THEN
            RETURN -1;
    END get_table_count;

    PROCEDURE execute_ddl(
        p_sql IN VARCHAR2
    )
    IS
    BEGIN
        EXECUTE IMMEDIATE p_sql;
    END execute_ddl;

    FUNCTION open_dynamic_query(
        p_sql IN VARCHAR2
    ) RETURN t_refcur
    IS
        v_cursor t_refcur;
    BEGIN
        OPEN v_cursor FOR p_sql;
        RETURN v_cursor;
    END open_dynamic_query;
END dynamic_sql_pkg;
