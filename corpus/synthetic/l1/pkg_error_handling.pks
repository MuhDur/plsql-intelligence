CREATE OR REPLACE PACKAGE error_handling
AS
    e_business_rule EXCEPTION;
    PRAGMA EXCEPTION_INIT(e_business_rule, -20100);

    e_validation_error EXCEPTION;
    PRAGMA EXCEPTION_INIT(e_validation_error, -20101);

    PROCEDURE transfer_funds(
        p_from_acct IN NUMBER,
        p_to_acct   IN NUMBER,
        p_amount    IN NUMBER
    );

    PROCEDURE log_error(
        p_package   IN VARCHAR2,
        p_procedure IN VARCHAR2,
        p_sqlcode   IN NUMBER,
        p_sqlerrm   IN VARCHAR2
    );
END error_handling;
