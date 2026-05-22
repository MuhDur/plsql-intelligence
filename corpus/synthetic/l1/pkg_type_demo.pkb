CREATE OR REPLACE TYPE BODY t_address AS
    MEMBER FUNCTION full_address RETURN VARCHAR2
    IS
    BEGIN
        RETURN self.street || ', ' || self.city || ', ' ||
               self.state || ' ' || self.zip_code;
    END full_address;
END;
/
