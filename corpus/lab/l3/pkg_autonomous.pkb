CREATE OR REPLACE PACKAGE BODY pkg_autonomous
AS
    PROCEDURE write_audit(p_event VARCHAR2)
    IS
        PRAGMA AUTONOMOUS_TRANSACTION;
    BEGIN
        INSERT INTO audit_log (event, recorded_at)
        VALUES (p_event, SYSTIMESTAMP);
        COMMIT;
    END write_audit;
END pkg_autonomous;
/
