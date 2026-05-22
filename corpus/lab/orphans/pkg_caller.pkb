CREATE OR REPLACE PACKAGE BODY pkg_caller
AS
    PROCEDURE drive_event(p_id NUMBER, p_payload VARCHAR2)
    IS
        v_label VARCHAR2(200);
    BEGIN
        v_label := pkg_actively_used.lookup_label(p_id);
        pkg_actively_used.record_event(p_id, p_payload || ' (' || v_label || ')');
    END drive_event;
END pkg_caller;
/
