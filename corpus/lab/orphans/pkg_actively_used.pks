-- L2 lab fixture: NOT an orphan candidate.
-- This package is referenced by pkg_caller below, so a depgraph built
-- from this corpus should have an incoming Calls edge and detect_orphans
-- must NOT flag it.
CREATE OR REPLACE PACKAGE pkg_actively_used
AS
    PROCEDURE record_event(p_id NUMBER, p_payload VARCHAR2);
    FUNCTION lookup_label(p_id NUMBER) RETURN VARCHAR2;
END pkg_actively_used;
/
