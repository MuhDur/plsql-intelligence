-- L3 realism: AUTONOMOUS_TRANSACTION pragma.
-- Affects transaction scope; bindings generator + privilege model must
-- recognise that this routine runs in an independent transaction.
CREATE OR REPLACE PACKAGE pkg_autonomous
AS
    PROCEDURE write_audit(p_event VARCHAR2);
END pkg_autonomous;
/
