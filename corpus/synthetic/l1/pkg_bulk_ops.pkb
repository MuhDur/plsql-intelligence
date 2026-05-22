CREATE OR REPLACE PACKAGE BODY bulk_ops
AS
    PROCEDURE bulk_update_salaries(
        p_dept_id   IN NUMBER,
        p_pct_raise IN NUMBER
    )
    IS
        TYPE t_rec IS RECORD (
            emp_id  NUMBER(10),
            salary  NUMBER(12,2)
        );
        TYPE t_rec_tab IS TABLE OF t_rec INDEX BY PLS_INTEGER;
        v_emps t_rec_tab;
    BEGIN
        SELECT emp_id, salary
        BULK COLLECT INTO v_emps
        FROM employees
        WHERE department_id = p_dept_id;

        FORALL i IN 1 .. v_emps.COUNT
            UPDATE employees
            SET salary = v_emps(i).salary * (1 + p_pct_raise / 100)
            WHERE emp_id = v_emps(i).emp_id;

        COMMIT;
    END bulk_update_salaries;

    PROCEDURE bulk_delete_inactive
    IS
        v_ids t_id_tab;
    BEGIN
        SELECT emp_id
        BULK COLLECT INTO v_ids
        FROM employees
        WHERE active_flag = 'N'
          AND termination_date < ADD_MONTHS(SYSDATE, -24);

        FORALL i IN 1 .. v_ids.COUNT
            DELETE FROM employee_audit WHERE emp_id = v_ids(i);

        FORALL i IN 1 .. v_ids.COUNT
            DELETE FROM employees WHERE emp_id = v_ids(i);

        COMMIT;
    END bulk_delete_inactive;

    FUNCTION fetch_employee_names(p_dept_id NUMBER) RETURN t_name_tab
    IS
        v_names t_name_tab;
    BEGIN
        SELECT emp_name
        BULK COLLECT INTO v_names
        FROM employees
        WHERE department_id = p_dept_id
        ORDER BY emp_name;
        RETURN v_names;
    END fetch_employee_names;
END bulk_ops;
