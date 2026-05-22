-- See pkg_opaque_dynamic.pkb for the opaque-dynamic-SQL hazards.
CREATE OR REPLACE PACKAGE pkg_opaque_dynamic
AS
    PROCEDURE run_unknown(p_sql_fragment VARCHAR2);
    PROCEDURE dispatch(p_action VARCHAR2);
END pkg_opaque_dynamic;
/
