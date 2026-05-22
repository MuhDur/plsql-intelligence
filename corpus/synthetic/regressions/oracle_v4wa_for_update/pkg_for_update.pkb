-- Minimized public-synthetic regression fixture for oracle-v4wa.
--
-- Reproduces the unbounded cross-file analysis recursion that
-- stack-overflowed `plsql-engine analyze` on
-- corpus/synthetic/l1/pkg_error_handling.pkb. The trigger is a
-- `SELECT … FOR UPDATE;` whose `FOR UPDATE` tail the text-scanner
-- mis-classifies as a non-shrinking FOR/Bare loop body; the
-- call-site / table-access extraction then re-lowers the *same*
-- string forever. The depth guard (oracle-v4wa) must make this
-- analyze exit 0 and surface a typed AnalysisRecursionLimit
-- diagnostic for this unit instead of aborting.
CREATE OR REPLACE PACKAGE BODY pkg_for_update
AS
    PROCEDURE lock_row(p_id IN NUMBER)
    IS
        v_bal NUMBER;
    BEGIN
        SELECT balance INTO v_bal
        FROM accounts
        WHERE account_id = p_id
        FOR UPDATE;

        UPDATE accounts SET balance = balance - 1 WHERE account_id = p_id;
        COMMIT;
    END lock_row;
END pkg_for_update;
/
