CREATE OR REPLACE VIEW audit_report AS
SELECT a.audit_id,
       u.username,
       a.action,
       a.target_object,
       a.action_date,
       CASE
           WHEN a.action LIKE '%DELETE%' THEN 'HIGH'
           WHEN a.action LIKE '%UPDATE%' THEN 'MEDIUM'
           ELSE 'LOW'
       END AS risk_level
FROM audit_trail a
JOIN app_users u ON u.user_id = a.user_id
WHERE a.action_date >= SYSDATE - 90;
