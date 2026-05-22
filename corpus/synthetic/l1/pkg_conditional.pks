CREATE OR REPLACE PACKAGE conditional_pkg
AS
    $IF DBMS_DB_VERSION.VER_LE_12_1 $THEN
        g_max_batch CONSTANT PLS_INTEGER := 1000;
    $ELSE
        g_max_batch CONSTANT PLS_INTEGER := 5000;
    $END

    PROCEDURE process_batch(
        p_batch_size IN PLS_INTEGER DEFAULT g_max_batch
    );
    FUNCTION get_version RETURN VARCHAR2;
END conditional_pkg;
