-- L2 synonym scenarios (PLSQL-LAB-004).
-- Private and public synonyms pointing at a base table; downstream
-- code should resolve through both.

CREATE OR REPLACE SYNONYM employees_syn FOR employees;

CREATE OR REPLACE PUBLIC SYNONYM employees_pub_syn FOR hr.employees;

-- A synonym chain: emp_alias → employees_syn → employees.
CREATE OR REPLACE SYNONYM emp_alias FOR employees_syn;
