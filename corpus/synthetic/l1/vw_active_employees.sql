CREATE OR REPLACE VIEW active_employees AS
SELECT e.emp_id,
       e.emp_name,
       e.salary,
       d.department_name,
       e.hire_date,
       ROUND(MONTHS_BETWEEN(SYSDATE, e.hire_date) / 12, 1) AS years_employed
FROM employees e
JOIN departments d ON d.department_id = e.department_id
WHERE e.active_flag = 'Y';
