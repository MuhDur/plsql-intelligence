-- L3 realism: spec with NO corresponding body in this corpus.
-- depgraph should record the SpecDeclaration node but no
-- BodyImplementation; doctor() should flag a missing-body warning.
CREATE OR REPLACE PACKAGE pkg_spec_no_body
AS
    PROCEDURE will_never_compile;
    FUNCTION still_no_body RETURN NUMBER;
END pkg_spec_no_body;
/
