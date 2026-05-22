CREATE OR REPLACE PACKAGE BODY security_pkg
AS
    FUNCTION is_admin(p_user_id IN NUMBER) RETURN BOOLEAN
    IS
        v_role VARCHAR2(50);
    BEGIN
        SELECT role_name INTO v_role
        FROM user_roles
        WHERE user_id = p_user_id
          AND role_name = 'ADMIN';
        RETURN TRUE;
    EXCEPTION
        WHEN NO_DATA_FOUND THEN
            RETURN FALSE;
    END is_admin;

    PROCEDURE audit_action(
        p_user_id  IN NUMBER,
        p_action   IN VARCHAR2,
        p_target   IN VARCHAR2
    )
    IS
        PRAGMA AUTONOMOUS_TRANSACTION;
    BEGIN
        INSERT INTO audit_trail (user_id, action, target_object, action_date)
        VALUES (p_user_id, p_action, p_target, SYSDATE);
        COMMIT;
    END audit_action;
END security_pkg;
