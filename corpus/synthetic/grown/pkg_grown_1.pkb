CREATE OR REPLACE PACKAGE BODY pkg_grown_1
AS
    PROCEDURE do_thing_1(p_id NUMBER)
    IS
    BEGIN
        NULL;
    END do_thing_1;

    FUNCTION compute_1(p_x NUMBER) RETURN NUMBER
    IS
    BEGIN
        RETURN p_x + 1;
    END compute_1;
END pkg_grown_1;
/
