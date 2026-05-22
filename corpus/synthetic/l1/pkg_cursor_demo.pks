CREATE OR REPLACE PACKAGE cursor_demo
AS
    TYPE t_dept_summary IS RECORD (
        dept_id     NUMBER(4),
        dept_name   VARCHAR2(50),
        emp_count   NUMBER,
        avg_salary  NUMBER(12,2)
    );

    CURSOR c_all_departments IS
        SELECT department_id, department_name
        FROM departments
        ORDER BY department_name;

    PROCEDURE process_all_employees;
    FUNCTION get_dept_summary(p_dept_id NUMBER) RETURN t_dept_summary;
END cursor_demo;
