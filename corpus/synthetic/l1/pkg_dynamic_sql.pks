CREATE OR REPLACE PACKAGE dynamic_sql_pkg
AS
    TYPE t_refcur IS REF CURSOR;

    FUNCTION get_table_count(
        p_table_name IN VARCHAR2,
        p_where      IN VARCHAR2 DEFAULT NULL
    ) RETURN NUMBER;

    PROCEDURE execute_ddl(
        p_sql IN VARCHAR2
    );

    FUNCTION open_dynamic_query(
        p_sql IN VARCHAR2
    ) RETURN t_refcur;
END dynamic_sql_pkg;
