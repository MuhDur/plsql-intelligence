CREATE OR REPLACE VIEW high_earners AS
SELECT e.emp_id,
       e.emp_name,
       e.salary,
       d.department_name,
       m.emp_name AS manager_name
FROM employees e
JOIN departments d ON d.department_id = e.department_id
LEFT JOIN employees m ON m.emp_id = e.manager_id
WHERE e.salary > (
    SELECT AVG(salary) * 1.5
    FROM employees
    WHERE department_id = e.department_id
);
