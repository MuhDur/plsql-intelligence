CREATE OR REPLACE PACKAGE security_pkg
    AUTHID CURRENT_USER
    ACCESSIBLE BY (package admin_utils)
AS
    FUNCTION is_admin(p_user_id IN NUMBER) RETURN BOOLEAN;
    PROCEDURE audit_action(
        p_user_id  IN NUMBER,
        p_action   IN VARCHAR2,
        p_target   IN VARCHAR2
    );
END security_pkg;
