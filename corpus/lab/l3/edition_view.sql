-- L3 realism: edition-based redefinition.
-- Editioning view + cross-edition trigger pattern.
CREATE OR REPLACE EDITIONING VIEW customers_v AS
    SELECT id, name, status_code
    FROM   customers;

CREATE OR REPLACE TRIGGER customers_xet
    INSTEAD OF INSERT OR UPDATE ON customers_v
    FOR EACH ROW
DECLARE
BEGIN
    -- depgraph should record both the editioning-view → base-table edge
    -- AND the trigger → view edge with EditionedObject hint.
    IF INSERTING THEN
        INSERT INTO customers (id, name, status_code)
        VALUES (:new.id, :new.name, :new.status_code);
    ELSIF UPDATING THEN
        UPDATE customers
            SET name = :new.name, status_code = :new.status_code
            WHERE id = :new.id;
    END IF;
END;
/
