CREATE OR REPLACE PACKAGE BODY conditional_pkg
AS
    PROCEDURE process_batch(
        p_batch_size IN PLS_INTEGER DEFAULT g_max_batch
    )
    IS
        TYPE t_ids IS TABLE OF NUMBER INDEX BY PLS_INTEGER;
        v_ids t_ids;
    BEGIN
        SELECT record_id
        BULK COLLECT INTO v_ids
        FROM staging
        WHERE processed_flag = 'N'
          AND ROWNUM <= p_batch_size;

        FORALL i IN 1 .. v_ids.COUNT
            UPDATE staging
            SET processed_flag = 'Y',
                processed_date = SYSDATE
            WHERE record_id = v_ids(i);

        COMMIT;
    $IF DBMS_DB_VERSION.VER_LE_12_1 $THEN
        DBMS_OUTPUT.PUT_LINE('Processed ' || v_ids.COUNT || ' rows (12c mode)');
    $ELSE
        DBMS_OUTPUT.PUT_LINE('Processed ' || v_ids.COUNT || ' rows (19c+ mode)');
    $END
    END process_batch;

    FUNCTION get_version RETURN VARCHAR2
    IS
    BEGIN
    $IF DBMS_DB_VERSION.VER_LE_12_1 $THEN
        RETURN 'Oracle 12c compatibility mode';
    $ELSIF DBMS_DB_VERSION.VER_LE_19 $THEN
        RETURN 'Oracle 19c mode';
    $ELSE
        RETURN 'Oracle 21c+ mode';
    $END
    END get_version;
END conditional_pkg;
