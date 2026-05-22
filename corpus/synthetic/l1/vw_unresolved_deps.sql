CREATE OR REPLACE VIEW unresolved_deps AS
SELECT d.source_type,
       d.source_name,
       d.target_type,
       d.target_name,
       'MISSING' AS status
FROM dependencies d
WHERE NOT EXISTS (
    SELECT 1
    FROM all_objects o
    WHERE o.object_type = d.target_type
      AND o.object_name = d.target_name
)
UNION ALL
SELECT d.source_type,
       d.source_name,
       d.target_type,
       d.target_name,
       'INVALID' AS status
FROM dependencies d
JOIN all_objects o ON o.object_type = d.target_type
    AND o.object_name = d.target_name
WHERE o.status = 'INVALID';
