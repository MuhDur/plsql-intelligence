#!/usr/bin/env bash
# First-boot bootstrap that loads the synthetic lab corpus into Oracle XE
# 23ai's FREEPDB1 pluggable database. Wired in by docker-compose as
# `/opt/oracle/scripts/startup/01-load-lab.sh`.
#
# The Oracle XE image runs every script under `/opt/oracle/scripts/startup/`
# in lexical order on first boot ONLY. Subsequent `make demo-oracle-xe`
# invocations reuse the persistent volume — fixtures stay loaded.

set -euo pipefail

LAB_ROOT="/opt/oracle/lab"
TARGET_DB="${1:-FREEPDB1}"

if [[ ! -d "$LAB_ROOT" ]]; then
  echo "[plsql-intelligence-xe] No corpus mounted at $LAB_ROOT — skipping fixture load." >&2
  exit 0
fi

echo "[plsql-intelligence-xe] Loading synthetic lab corpus into $TARGET_DB..."

# Create a dedicated DEMO schema for the lab fixtures so the SYSTEM user
# stays untouched and a downstream agent can drop+recreate DEMO without
# affecting the rest of the database.
sqlplus -s "system/${ORACLE_PWD}@//localhost:1521/$TARGET_DB" <<'EOSQL'
whenever sqlerror exit failure;
create user DEMO identified by "DemoLab#2026" default tablespace USERS quota unlimited on USERS;
grant connect, resource, create view, create synonym, create database link, create type to DEMO;
EOSQL

# Load the fixtures in order: L1 (hero) → L2 (extended) → L3 (realism).
for level in l1 l2 l3; do
  level_dir="$LAB_ROOT/$level"
  [[ -d "$level_dir" ]] || continue
  echo "[plsql-intelligence-xe] Loading $level fixtures..."
  for sql in "$level_dir"/*.sql "$level_dir"/*.pks "$level_dir"/*.pkb; do
    [[ -e "$sql" ]] || continue
    echo "  - $sql"
    sqlplus -s "DEMO/DemoLab#2026@//localhost:1521/$TARGET_DB" <<EOF
whenever sqlerror continue;
@$sql
exit;
EOF
  done
done

echo "[plsql-intelligence-xe] Lab corpus loaded. Connection string:"
echo "  DEMO/DemoLab#2026@//localhost:1521/$TARGET_DB"
