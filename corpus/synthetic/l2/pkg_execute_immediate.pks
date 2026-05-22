-- L2 EXECUTE IMMEDIATE coverage with and without binds
-- (PLSQL-LAB-004). Used to validate that dynamic-SQL analysis
-- distinguishes safe (parameter-bound) from opaque (string-concatenated)
-- execution sites.

CREATE OR REPLACE PACKAGE pkg_execute_immediate_demo
AS
    PROCEDURE update_with_binds(p_id NUMBER, p_amount NUMBER);
    PROCEDURE update_no_binds(p_id NUMBER, p_amount NUMBER);
    PROCEDURE truncate_static_string;
    FUNCTION fetch_one_with_binds(p_id NUMBER) RETURN NUMBER;
END pkg_execute_immediate_demo;
/
