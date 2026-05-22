CREATE OR REPLACE TRIGGER trg_check_salary
BEFORE INSERT OR UPDATE OF salary
ON employees
FOR EACH ROW
WHEN (NEW.salary IS NOT NULL)
DECLARE
    v_min_salary NUMBER(12,2);
    v_max_salary NUMBER(12,2);
BEGIN
    SELECT min_salary, max_salary
    INTO v_min_salary, v_max_salary
    FROM salary_grades
    WHERE grade_id = :new.grade_id;

    IF :new.salary < v_min_salary THEN
        RAISE_APPLICATION_ERROR(-20300,
            'Salary ' || :new.salary || ' is below minimum ' || v_min_salary ||
            ' for grade ' || :new.grade_id);
    END IF;

    IF :new.salary > v_max_salary THEN
        RAISE_APPLICATION_ERROR(-20301,
            'Salary ' || :new.salary || ' exceeds maximum ' || v_max_salary ||
            ' for grade ' || :new.grade_id);
    END IF;
EXCEPTION
    WHEN NO_DATA_FOUND THEN
        NULL; -- No salary grade defined, allow any salary
END trg_check_salary;
