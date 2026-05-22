CREATE OR REPLACE PACKAGE BODY pkg_db_link_caller
AS
    PROCEDURE refresh_remote_summary(p_id NUMBER)
    IS
    BEGIN
        -- Cross-database call: depgraph records this as DbLink.
        remote_app.refresh_pkg.do_refresh@remote_db(p_id);
        -- And the corresponding read from a remote table:
        INSERT INTO local_mirror (id, snapshot_at, label)
        SELECT id, SYSDATE, label
            FROM remote_app.summary_v@remote_db
            WHERE id = p_id;
    END refresh_remote_summary;
END pkg_db_link_caller;
/
