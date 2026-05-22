CREATE OR REPLACE PACKAGE BODY pkg_internal_only
AS
    PROCEDURE do_secret_thing(p_id NUMBER)
    IS
    BEGIN
        UPDATE secrets SET touched_at = SYSDATE WHERE id = p_id;
    END do_secret_thing;

    FUNCTION compute_hash(p_value VARCHAR2) RETURN VARCHAR2
    IS
    BEGIN
        RETURN standard_hash(p_value, 'SHA256');
    END compute_hash;
END pkg_internal_only;
/
