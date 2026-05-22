CREATE OR REPLACE PACKAGE BODY error_handling
AS
    PROCEDURE transfer_funds(
        p_from_acct IN NUMBER,
        p_to_acct   IN NUMBER,
        p_amount    IN NUMBER
    )
    IS
        v_balance NUMBER(15,2);
    BEGIN
        IF p_amount <= 0 THEN
            RAISE_APPLICATION_ERROR(-20101, 'Amount must be positive');
        END IF;

        SELECT balance INTO v_balance
        FROM accounts
        WHERE account_id = p_from_acct
        FOR UPDATE;

        IF v_balance < p_amount THEN
            RAISE_APPLICATION_ERROR(-20100, 'Insufficient funds');
        END IF;

        UPDATE accounts
        SET balance = balance - p_amount
        WHERE account_id = p_from_acct;

        UPDATE accounts
        SET balance = balance + p_amount
        WHERE account_id = p_to_acct;

        INSERT INTO transaction_log (from_acct, to_acct, amount, txn_date)
        VALUES (p_from_acct, p_to_acct, p_amount, SYSDATE);

        COMMIT;
    EXCEPTION
        WHEN e_business_rule THEN
            ROLLBACK;
            log_error('ERROR_HANDLING', 'TRANSFER_FUNDS', SQLCODE, SQLERRM);
            RAISE;
        WHEN e_validation_error THEN
            ROLLBACK;
            log_error('ERROR_HANDLING', 'TRANSFER_FUNDS', SQLCODE, SQLERRM);
            RAISE;
        WHEN OTHERS THEN
            ROLLBACK;
            log_error('ERROR_HANDLING', 'TRANSFER_FUNDS', SQLCODE, SQLERRM);
            RAISE_APPLICATION_ERROR(-20999, 'Unexpected error in transfer_funds');
    END transfer_funds;

    PROCEDURE log_error(
        p_package   IN VARCHAR2,
        p_procedure IN VARCHAR2,
        p_sqlcode   IN NUMBER,
        p_sqlerrm   IN VARCHAR2
    )
    IS
        PRAGMA AUTONOMOUS_TRANSACTION;
    BEGIN
        INSERT INTO error_log (package_name, procedure_name, error_code,
                               error_message, log_date)
        VALUES (p_package, p_procedure, p_sqlcode, p_sqlerrm, SYSDATE);
        COMMIT;
    END log_error;
END error_handling;
