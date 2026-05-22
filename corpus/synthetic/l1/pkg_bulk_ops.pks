CREATE OR REPLACE PACKAGE bulk_ops
AS
    TYPE t_id_tab IS TABLE OF NUMBER(10) INDEX BY PLS_INTEGER;
    TYPE t_name_tab IS TABLE OF VARCHAR2(200) INDEX BY PLS_INTEGER;

    PROCEDURE bulk_update_salaries(
        p_dept_id   IN NUMBER,
        p_pct_raise IN NUMBER
    );

    PROCEDURE bulk_delete_inactive;
    FUNCTION fetch_employee_names(p_dept_id NUMBER) RETURN t_name_tab;
END bulk_ops;
