#!/usr/bin/env bash
# Offline engine dependency boundary lint (oracle-jfqh.23).
#
# The PL/SQL intelligence workspace is an offline, sync-first Rust engine. Live
# Oracle connectivity and MCP serving belong in oraclemcp, not in first-party
# plsql-* crates. This gate fails on direct runtime/server/driver dependencies
# and on any resolved normal dependency graph edge to Oracle/MCP driver crates.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BANNED_DIRECT_REGEX='^(oraclemcp($|-)|oracle$|oracledb$|odpic-sys$|asupersync$|tokio$|hyper$|axum$|tonic$|reqwest$|rmcp$|async-std$|smol$|r2d2$)'
BANNED_GRAPH_REGEX='^(oraclemcp($|-)|oracle$|oracledb$|odpic-sys$)'

usage() {
  cat <<'USAGE'
Usage: scripts/offline_boundary_lint.sh [--self-test]

Checks first-party plsql-* crates for offline-engine boundary violations.

Fails on direct normal dependencies on:
  oraclemcp-*, oracle, oracledb, odpic-sys, asupersync, tokio, hyper, axum,
  tonic, reqwest, rmcp, async-std, smol, r2d2

Also fails if the resolved normal dependency graph contains:
  oraclemcp-*, oracle, oracledb, odpic-sys
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "OFFLINE-BOUNDARY: FAIL required tool not found: ${tool}" >&2
    exit 2
  fi
}

forbidden_direct_deps_from_metadata() {
  local metadata="$1"
  jq -r --arg banned "${BANNED_DIRECT_REGEX}" '
    .packages[]
    | select(.name | startswith("plsql-"))
    | .name as $package
    | .manifest_path as $manifest
    | .dependencies[]?
    | select(.kind == null)
    | select(.name | test($banned))
    | [$package, .name, (.rename // ""), (.optional | tostring), $manifest]
    | @tsv
  ' <<<"${metadata}"
}

forbidden_graph_deps_from_tree() {
  local tree="$1"
  awk '{ print $1 }' <<<"${tree}" \
    | grep -E "${BANNED_GRAPH_REGEX}" \
    | sort -u \
    || true
}

run_metadata_gate() {
  local metadata="$1"
  local label="$2"
  local violations

  violations="$(forbidden_direct_deps_from_metadata "${metadata}")"
  if [[ -z "${violations}" ]]; then
    echo "OFFLINE-BOUNDARY: PASS no direct forbidden normal dependencies in plsql-* crates (${label})"
    return 0
  fi

  echo "package	dependency	rename	optional	manifest"
  echo "OFFLINE-BOUNDARY: FAIL direct forbidden dependency detected (${label})"
  echo "${violations}"
  return 1
}

run_graph_gate() {
  local tree="$1"
  local label="$2"
  local violations

  violations="$(forbidden_graph_deps_from_tree "${tree}")"
  if [[ -z "${violations}" ]]; then
    echo "OFFLINE-BOUNDARY: PASS no Oracle/MCP driver crates in normal dependency graph (${label})"
    return 0
  fi

  echo "OFFLINE-BOUNDARY: FAIL forbidden Oracle/MCP driver crate in normal dependency graph (${label})"
  echo "${violations}"
  return 1
}

self_test() {
  local planted_metadata planted_tree rc_graph rc_metadata
  planted_metadata='{
    "packages": [
      {
        "name": "plsql-engine",
        "manifest_path": "/tmp/plsql-engine/Cargo.toml",
        "dependencies": [
          {
            "name": "asupersync",
            "rename": null,
            "kind": null,
            "optional": false
          },
          {
            "name": "oraclemcp-db",
            "rename": null,
            "kind": null,
            "optional": false
          }
        ]
      },
      {
        "name": "plsql-catalog",
        "manifest_path": "/tmp/plsql-catalog/Cargo.toml",
        "dependencies": [
          {
            "name": "serde",
            "rename": null,
            "kind": null,
            "optional": false
          }
        ]
      }
    ]
  }'
  planted_tree='plsql-engine v0.1.0 (/tmp/plsql-engine)
serde v1.0.0
oraclemcp-db v0.3.0
oracle v0.6.2'

  set +e
  run_metadata_gate "${planted_metadata}" "self-test planted direct deps"
  rc_metadata=$?
  run_graph_gate "${planted_tree}" "self-test planted graph deps"
  rc_graph=$?
  set -e

  if [[ ${rc_metadata} -eq 1 && ${rc_graph} -eq 1 ]]; then
    echo "OFFLINE-BOUNDARY: PASS planted direct and graph violations were rejected"
    return 0
  fi

  echo "OFFLINE-BOUNDARY: FAIL planted violations were not rejected" >&2
  return 1
}

main() {
  local mode="${1:-}"
  if [[ $# -gt 1 ]]; then
    usage >&2
    exit 2
  fi

  case "${mode}" in
    "")
      ;;
    "--self-test")
      require_tool jq
      self_test
      exit $?
      ;;
    "-h" | "--help")
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac

  require_tool cargo
  require_tool jq

  cd "${REPO_ROOT}"
  echo "== offline engine boundary lint =="
  echo "repo=${REPO_ROOT}"
  echo "scope=direct plsql-* normal deps plus resolved workspace normal graph"

  local metadata rc_graph rc_metadata tree
  metadata="$(cargo metadata --format-version=1 --locked --no-deps)"
  tree="$(cargo tree --workspace --locked -e normal --prefix none --format '{p}')"

  set +e
  run_metadata_gate "${metadata}" "workspace"
  rc_metadata=$?
  run_graph_gate "${tree}" "workspace"
  rc_graph=$?
  set -e

  if [[ ${rc_metadata} -ne 0 || ${rc_graph} -ne 0 ]]; then
    exit 1
  fi
}

main "$@"
