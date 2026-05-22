CREATE OR REPLACE PACKAGE BODY pkg_actively_used
AS
    PROCEDURE record_event(p_id NUMBER, p_payload VARCHAR2)
    IS
    BEGIN
        INSERT INTO event_log (id, payload, recorded_at)
        VALUES (p_id, p_payload, SYSDATE);
    END record_event;

    FUNCTION lookup_label(p_id NUMBER) RETURN VARCHAR2
    IS
        v_label VARCHAR2(200);
    BEGIN
        SELECT label INTO v_label FROM labels WHERE id = p_id;
        RETURN v_label;
    END lookup_label;
END pkg_actively_used;
/
