CREATE OR REPLACE TYPE t_address AS OBJECT (
    street   VARCHAR2(200),
    city     VARCHAR2(100),
    state    VARCHAR2(50),
    zip_code VARCHAR2(20),
    MEMBER FUNCTION full_address RETURN VARCHAR2
);
/
