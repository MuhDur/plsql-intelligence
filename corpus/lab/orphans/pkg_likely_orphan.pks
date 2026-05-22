-- L2 lab fixture: LIKELY-UNUSED orphan candidate.
-- Has zero incoming references in this corpus BUT has outgoing
-- references (it reads from event_log). detect_orphans should emit
-- OrphanConfidenceTier::LikelyUnused — the package reads things, but
-- nothing in the corpus points at it.
CREATE OR REPLACE PACKAGE pkg_likely_orphan
AS
    FUNCTION count_recent_events RETURN NUMBER;
END pkg_likely_orphan;
/
