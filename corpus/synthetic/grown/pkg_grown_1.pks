CREATE OR REPLACE PACKAGE pkg_grown_1
AS
    PROCEDURE do_thing_1(p_id NUMBER);
    FUNCTION compute_1(p_x NUMBER) RETURN NUMBER;
END pkg_grown_1;
/
