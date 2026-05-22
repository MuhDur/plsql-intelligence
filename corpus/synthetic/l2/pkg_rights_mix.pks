-- L2 AUTHID DEFINER vs CURRENT_USER mix (PLSQL-LAB-004).
-- The same package owner runs the DEFINER variant under its own
-- privileges; the CURRENT_USER variant runs under the caller's. Symbol
-- + privilege resolution must distinguish these even when the source is
-- otherwise identical.

CREATE OR REPLACE PACKAGE pkg_definer_view AUTHID DEFINER
AS
    FUNCTION read_count(p_table VARCHAR2) RETURN NUMBER;
END pkg_definer_view;
/

CREATE OR REPLACE PACKAGE pkg_invoker_view AUTHID CURRENT_USER
AS
    FUNCTION read_count(p_table VARCHAR2) RETURN NUMBER;
END pkg_invoker_view;
/
