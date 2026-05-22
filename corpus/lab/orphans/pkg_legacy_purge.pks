-- L2 lab fixture: HIGH-confidence orphan.
-- Nothing in this corpus references it; depgraph should record zero
-- incoming edges; detect_orphans MUST emit
-- OrphanConfidenceTier::HighConfidenceUnused.
CREATE OR REPLACE PACKAGE pkg_legacy_purge
AS
    PROCEDURE purge_2018_partition;
END pkg_legacy_purge;
/
