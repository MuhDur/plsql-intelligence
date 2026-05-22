CREATE OR REPLACE PACKAGE BODY pkg_likely_orphan
AS
    FUNCTION count_recent_events RETURN NUMBER
    IS
        v_count NUMBER;
    BEGIN
        SELECT COUNT(*)
            INTO v_count
            FROM event_log
            WHERE recorded_at > SYSDATE - 30;
        RETURN v_count;
    END count_recent_events;
END pkg_likely_orphan;
/
