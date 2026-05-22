-- L3 realism: cross-schema call via DB-link.
-- depgraph should emit EdgeKind::DbLink for any reference resolved
-- through `@remote_db`.
CREATE OR REPLACE PACKAGE pkg_db_link_caller
AS
    PROCEDURE refresh_remote_summary(p_id NUMBER);
END pkg_db_link_caller;
/
