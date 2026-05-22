CREATE OR REPLACE PACKAGE BODY pkg_opaque_dynamic
AS
    -- L3 realism: opaque dynamic SQL.
    -- EXECUTE IMMEDIATE on a runtime-built string with no static
    -- residue depgraph could pin down; should emit
    -- EdgeKind::OpaqueDynamic + UnknownReason::DynamicSqlOpaque.

    PROCEDURE run_unknown(p_sql_fragment VARCHAR2)
    IS
        v_full VARCHAR2(4000);
    BEGIN
        -- The fragment shape is unknown at parse time — depgraph
        -- cannot determine the target table/object.
        v_full := 'BEGIN ' || p_sql_fragment || '; END;';
        EXECUTE IMMEDIATE v_full;
    END run_unknown;

    PROCEDURE dispatch(p_action VARCHAR2)
    IS
        v_sql VARCHAR2(200);
    BEGIN
        -- Same hazard pattern: which procedure gets called depends on
        -- p_action's value at runtime.
        v_sql := 'BEGIN ops_pkg.do_' || p_action || '(); END;';
        EXECUTE IMMEDIATE v_sql;
    END dispatch;
END pkg_opaque_dynamic;
/
