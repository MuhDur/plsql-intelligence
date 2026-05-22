-- L3 realism: conditional compilation ($IF / $ELSIF / $END).
-- Selected-source view depends on AnalysisProfile::plsql_ccflags;
-- inactive regions must carry inactive-region provenance per WS-010.
CREATE OR REPLACE PACKAGE pkg_cc_flags
AS
    $IF $$debug $THEN
    PROCEDURE log_debug(p_msg VARCHAR2);
    $END
    PROCEDURE run;
END pkg_cc_flags;
/
