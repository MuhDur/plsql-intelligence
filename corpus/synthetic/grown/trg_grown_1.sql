CREATE OR REPLACE TRIGGER trg_grown_1
BEFORE INSERT ON base_table_1
FOR EACH ROW
BEGIN
    :new.id := NVL(:new.id, 0);
END;
/
