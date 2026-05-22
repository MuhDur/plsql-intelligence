CREATE OR REPLACE PACKAGE employee_mgmt
AS
    -- Employee management package
    TYPE t_emp_rec IS RECORD (
        emp_id    NUMBER(10),
        emp_name  VARCHAR2(100),
        salary    NUMBER(12,2),
        hire_date DATE
    );

    TYPE t_emp_tab IS TABLE OF t_emp_rec INDEX BY PLS_INTEGER;

    PROCEDURE hire_employee(
        p_name     IN VARCHAR2,
        p_salary   IN NUMBER,
        p_dept_id  IN NUMBER
    );

    PROCEDURE fire_employee(
        p_emp_id IN NUMBER
    );

    FUNCTION get_salary(
        p_emp_id IN NUMBER
    ) RETURN NUMBER;

    FUNCTION count_employees(
        p_dept_id IN NUMBER
    ) RETURN PLS_INTEGER;
END employee_mgmt;
