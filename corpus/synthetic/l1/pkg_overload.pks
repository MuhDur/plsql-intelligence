CREATE OR REPLACE PACKAGE overload_pkg
AS
    PROCEDURE process(p_id IN NUMBER);
    PROCEDURE process(p_name IN VARCHAR2);
    PROCEDURE process(p_id IN NUMBER, p_name IN VARCHAR2);
    FUNCTION format_value(p_num NUMBER) RETURN VARCHAR2;
    FUNCTION format_value(p_date DATE) RETURN VARCHAR2;
    FUNCTION format_value(p_str VARCHAR2) RETURN VARCHAR2;
END overload_pkg;
