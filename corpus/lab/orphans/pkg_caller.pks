-- L2 lab fixture: caller package that references pkg_actively_used.
-- Its presence prevents pkg_actively_used from being flagged as an orphan.
CREATE OR REPLACE PACKAGE pkg_caller
AS
    PROCEDURE drive_event(p_id NUMBER, p_payload VARCHAR2);
END pkg_caller;
/
