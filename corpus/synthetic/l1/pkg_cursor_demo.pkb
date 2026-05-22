CREATE OR REPLACE PACKAGE BODY cursor_demo
AS
    PROCEDURE process_all_employees
    IS
        CURSOR c_emp IS
            SELECT emp_id, emp_name, salary
            FROM employees
            WHERE active_flag = 'Y';

        v_emp c_emp%ROWTYPE;
    BEGIN
        OPEN c_emp;
        LOOP
            FETCH c_emp INTO v_emp;
            EXIT WHEN c_emp%NOTFOUND;
            -- Process each employee
            UPDATE employee_audit
            SET last_review = SYSDATE
            WHERE emp_id = v_emp.emp_id;
        END LOOP;
        CLOSE c_emp;
        COMMIT;
    EXCEPTION
        WHEN OTHERS THEN
            IF c_emp%ISOPEN THEN
                CLOSE c_emp;
            END IF;
            RAISE;
    END process_all_employees;

    FUNCTION get_dept_summary(p_dept_id NUMBER) RETURN t_dept_summary
    IS
        v_result t_dept_summary;
    BEGIN
        SELECT d.department_id,
               d.department_name,
               COUNT(e.emp_id),
               AVG(e.salary)
        INTO v_result.dept_id,
             v_result.dept_name,
             v_result.emp_count,
             v_result.avg_salary
        FROM departments d
        LEFT JOIN employees e ON e.department_id = d.department_id
        WHERE d.department_id = p_dept_id
        GROUP BY d.department_id, d.department_name;

        RETURN v_result;
    EXCEPTION
        WHEN NO_DATA_FOUND THEN
            RETURN NULL;
    END get_dept_summary;
END cursor_demo;
