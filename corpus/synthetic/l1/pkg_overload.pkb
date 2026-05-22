CREATE OR REPLACE PACKAGE BODY overload_pkg
AS
    PROCEDURE process(p_id IN NUMBER)
    IS
    BEGIN
        DELETE FROM staging WHERE record_id = p_id;
    END process;

    PROCEDURE process(p_name IN VARCHAR2)
    IS
    BEGIN
        DELETE FROM staging WHERE record_name = p_name;
    END process;

    PROCEDURE process(p_id IN NUMBER, p_name IN VARCHAR2)
    IS
    BEGIN
        DELETE FROM staging
        WHERE record_id = p_id
          AND record_name = p_name;
    END process;

    FUNCTION format_value(p_num NUMBER) RETURN VARCHAR2
    IS
    BEGIN
        RETURN TO_CHAR(p_num, 'FM999,999,990.00');
    END format_value;

    FUNCTION format_value(p_date DATE) RETURN VARCHAR2
    IS
    BEGIN
        RETURN TO_CHAR(p_date, 'YYYY-MM-DD HH24:MI:SS');
    END format_value;

    FUNCTION format_value(p_str VARCHAR2) RETURN VARCHAR2
    IS
    BEGIN
        RETURN UPPER(TRIM(p_str));
    END format_value;
END overload_pkg;
