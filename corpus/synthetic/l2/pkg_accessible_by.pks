-- L2 ACCESSIBLE BY scenarios (PLSQL-LAB-004).
-- Restricts who can call the package's procedures at PL/SQL elaboration
-- time. Downstream symbol resolution should report "not accessible from
-- <caller>" diagnostics when a forbidden caller invokes these.

CREATE OR REPLACE PACKAGE pkg_internal_only
    ACCESSIBLE BY (PACKAGE pkg_orchestrator, PROCEDURE p_root)
AS
    PROCEDURE do_secret_thing(p_id NUMBER);
    FUNCTION compute_hash(p_value VARCHAR2) RETURN VARCHAR2;
END pkg_internal_only;
/
