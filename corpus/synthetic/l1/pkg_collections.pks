CREATE OR REPLACE PACKAGE collections_pkg
AS
    TYPE t_varray IS VARRAY(100) OF VARCHAR2(50);
    TYPE t_nested_table IS TABLE OF NUMBER;
    TYPE t_assoc_array IS TABLE OF VARCHAR2(200) INDEX BY VARCHAR2(100);

    FUNCTION split_string(
        p_input   IN VARCHAR2,
        p_delim   IN VARCHAR2 DEFAULT ','
    ) RETURN t_varray;

    PROCEDURE merge_arrays(
        p_arr1 IN OUT NOCOPY t_nested_table,
        p_arr2 IN t_nested_table
    );
END collections_pkg;
