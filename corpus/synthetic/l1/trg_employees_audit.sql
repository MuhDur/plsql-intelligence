CREATE OR REPLACE TRIGGER trg_employees_audit
BEFORE INSERT OR UPDATE OR DELETE
ON employees
FOR EACH ROW
DECLARE
    v_action VARCHAR2(10);
BEGIN
    IF INSERTING THEN
        v_action := 'INSERT';
        :new.created_date := SYSDATE;
        :new.created_by := SYS_CONTEXT('USERENV', 'SESSION_USER');
    ELSIF UPDATING THEN
        v_action := 'UPDATE';
        :new.modified_date := SYSDATE;
        :new.modified_by := SYS_CONTEXT('USERENV', 'SESSION_USER');

        IF :old.salary != :new.salary THEN
            INSERT INTO salary_history (emp_id, old_salary, new_salary, change_date)
            VALUES (:old.emp_id, :old.salary, :new.salary, SYSDATE);
        END IF;
    ELSIF DELETING THEN
        v_action := 'DELETE';
    END IF;

    INSERT INTO employee_audit_log (emp_id, action, action_date, old_values, new_values)
    VALUES (
        COALESCE(:new.emp_id, :old.emp_id),
        v_action,
        SYSDATE,
        CASE WHEN :old.emp_id IS NOT NULL THEN
            :old.emp_name || '|' || :old.salary || '|' || :old.department_id
        END,
        CASE WHEN :new.emp_id IS NOT NULL THEN
            :new.emp_name || '|' || :new.salary || '|' || :new.department_id
        END
    );
END trg_employees_audit;
