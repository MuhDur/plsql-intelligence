CREATE OR REPLACE PACKAGE BODY pkg_execute_immediate_demo
AS
    -- Demonstrates EXECUTE IMMEDIATE with and without bind variables.
    -- Lineage's dynamic-SQL inference should produce different
    -- confidence tiers for these:
    --   * `*_with_binds` — Heuristic (string + USING binds)
    --   * `*_no_binds`   — Opaque (pure string concatenation)
    --   * `*_static_string` — Exact (string literal known at parse time)

    PROCEDURE update_with_binds(p_id NUMBER, p_amount NUMBER)
    IS
    BEGIN
        EXECUTE IMMEDIATE
            'UPDATE invoices SET amount = :amt WHERE id = :id'
            USING p_amount, p_id;
    END update_with_binds;

    PROCEDURE update_no_binds(p_id NUMBER, p_amount NUMBER)
    IS
    BEGIN
        EXECUTE IMMEDIATE
            'UPDATE invoices SET amount = ' || p_amount
            || ' WHERE id = ' || p_id;
    END update_no_binds;

    PROCEDURE truncate_static_string
    IS
    BEGIN
        EXECUTE IMMEDIATE 'TRUNCATE TABLE staging_invoices';
    END truncate_static_string;

    FUNCTION fetch_one_with_binds(p_id NUMBER) RETURN NUMBER
    IS
        v_amount NUMBER;
    BEGIN
        EXECUTE IMMEDIATE
            'SELECT amount FROM invoices WHERE id = :id'
            INTO v_amount
            USING p_id;
        RETURN v_amount;
    END fetch_one_with_binds;
END pkg_execute_immediate_demo;
/
