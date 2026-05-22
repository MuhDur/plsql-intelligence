CREATE OR REPLACE PACKAGE BODY employee_mgmt
AS
    PROCEDURE hire_employee(
        p_name     IN VARCHAR2,
        p_salary   IN NUMBER,
        p_dept_id  IN NUMBER
    )
    IS
    BEGIN
        INSERT INTO employees (emp_name, salary, dept_id, hire_date)
        VALUES (p_name, p_salary, p_dept_id, SYSDATE);
        COMMIT;
    EXCEPTION
        WHEN DUP_VAL_ON_INDEX THEN
            ROLLBACK;
            RAISE_APPLICATION_ERROR(-20001, 'Employee already exists');
    END hire_employee;

    PROCEDURE fire_employee(
        p_emp_id IN NUMBER
    )
    IS
    BEGIN
        DELETE FROM employees WHERE emp_id = p_emp_id;
        IF SQL%ROWCOUNT = 0 THEN
            RAISE_APPLICATION_ERROR(-20002, 'Employee not found');
        END IF;
        COMMIT;
    END fire_employee;

    FUNCTION get_salary(
        p_emp_id IN NUMBER
    ) RETURN NUMBER
    IS
        v_salary NUMBER(12,2);
    BEGIN
        SELECT salary INTO v_salary
        FROM employees
        WHERE emp_id = p_emp_id;
        RETURN v_salary;
    EXCEPTION
        WHEN NO_DATA_FOUND THEN
            RETURN NULL;
    END get_salary;

    FUNCTION count_employees(
        p_dept_id IN NUMBER
    ) RETURN PLS_INTEGER
    IS
        v_count PLS_INTEGER;
    BEGIN
        SELECT COUNT(*) INTO v_count
        FROM employees
        WHERE dept_id = p_dept_id;
        RETURN v_count;
    END count_employees;
END employee_mgmt;
