CREATE OR REPLACE PACKAGE BODY collections_pkg
AS
    FUNCTION split_string(
        p_input   IN VARCHAR2,
        p_delim   IN VARCHAR2 DEFAULT ','
    ) RETURN t_varray
    IS
        v_result t_varray := t_varray();
        v_pos    PLS_INTEGER := 1;
        v_next   PLS_INTEGER;
        v_token  VARCHAR2(200);
    BEGIN
        IF p_input IS NULL THEN
            RETURN v_result;
        END IF;

        LOOP
            v_next := INSTR(p_input, p_delim, v_pos);
            IF v_next = 0 THEN
                v_token := SUBSTR(p_input, v_pos);
            ELSE
                v_token := SUBSTR(p_input, v_pos, v_next - v_pos);
            END IF;

            v_result.EXTEND;
            v_result(v_result.COUNT) := TRIM(v_token);

            EXIT WHEN v_next = 0;
            v_pos := v_next + LENGTH(p_delim);
        END LOOP;

        RETURN v_result;
    END split_string;

    PROCEDURE merge_arrays(
        p_arr1 IN OUT NOCOPY t_nested_table,
        p_arr2 IN t_nested_table
    )
    IS
    BEGIN
        IF p_arr2 IS NULL THEN
            RETURN;
        END IF;

        FOR i IN 1 .. p_arr2.COUNT LOOP
            p_arr1.EXTEND;
            p_arr1(p_arr1.COUNT) := p_arr2(i);
        END LOOP;
    END merge_arrays;
END collections_pkg;
