CREATE OR REPLACE VIEW dept_summary AS
SELECT d.department_id,
       d.department_name,
       COUNT(e.emp_id) AS emp_count,
       MIN(e.salary) AS min_salary,
       MAX(e.salary) AS max_salary,
       AVG(e.salary) AS avg_salary,
       SUM(e.salary) AS total_salary
FROM departments d
LEFT JOIN employees e ON e.department_id = d.department_id
    AND e.active_flag = 'Y'
GROUP BY d.department_id, d.department_name;
