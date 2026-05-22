CREATE OR REPLACE PACKAGE BODY pkg_legacy_purge
AS
    PROCEDURE purge_2018_partition
    IS
    BEGIN
        EXECUTE IMMEDIATE
            'ALTER TABLE event_log DROP PARTITION p_2018';
    END purge_2018_partition;
END pkg_legacy_purge;
/
