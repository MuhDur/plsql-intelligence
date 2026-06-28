#!/usr/bin/env bash
# plsql-mcp convergence boundary lint (oracle-plsql-converge-0lnu.15.12).
#
# Fails if a first-party plsql-* crate adds a direct normal dependency on the
# retired thick Oracle driver, the thin driver below oraclemcp-db, or a runtime /
# server stack that would pierce the asupersync boundary. Transitive `oracledb`
# through the published `oraclemcp-db` adapter is expected; direct metadata is
# the gate, and `cargo tree -e normal -i <crate>` is printed as evidence.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BANNED_DIRECT_DEPS=(
  oracle
  oracledb
  tokio
  hyper
  axum
  tonic
  reqwest
  rmcp
  async-std
  smol
  r2d2
  odpic-sys
)

usage() {
  cat <<'USAGE'
Usage: scripts/plsql_mcp_boundary_lint.sh [--self-test]

Checks direct normal dependencies for first-party plsql-* crates. A direct hit
on oracle/oracledb/tokio/hyper/axum/tonic/reqwest/rmcp/async-std/smol/r2d2/
odpic-sys fails. Direct oraclemcp-db edges are allowed only at the explicit
adapter seams. Transitive trees are printed for audit context.
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "BOUNDARY-LINT: FAIL required tool not found: ${tool}" >&2
    exit 2
  fi
}

forbidden_direct_deps_from_metadata() {
  local metadata="$1"
  jq -r '
    def banned:
      ["oracle", "oracledb", "tokio", "hyper", "axum", "tonic", "reqwest",
       "rmcp", "async-std", "smol", "r2d2", "odpic-sys"];

    .packages[]
    | select(.name | startswith("plsql-"))
    | .name as $package
    | .manifest_path as $manifest
    | .dependencies[]?
    | select(.kind == null)
    | select(.name as $dep | banned | index($dep))
    | [$package, .name, (.rename // ""), (.optional | tostring), $manifest]
    | @tsv
  ' <<<"${metadata}"
}

unapproved_oraclemcp_db_edges_from_metadata() {
  local metadata="$1"
  jq -r '
    def allowed:
      ["plsql-mcp:oraclemcp-db", "plsql-catalog:oraclemcp-db"];

    .packages[]
    | select(.name | startswith("plsql-"))
    | .name as $package
    | .manifest_path as $manifest
    | .dependencies[]?
    | select(.kind == null)
    | select(.name == "oraclemcp-db")
    | select((($package + ":" + .name) as $edge | allowed | index($edge)) | not)
    | [$package, .name, (.rename // ""), (.optional | tostring), $manifest]
    | @tsv
  ' <<<"${metadata}"
}

run_metadata_gate() {
  local metadata="$1"
  local label="$2"
  local adapter_violations violations

  violations="$(forbidden_direct_deps_from_metadata "${metadata}")"
  adapter_violations="$(unapproved_oraclemcp_db_edges_from_metadata "${metadata}")"
  if [[ -z "${violations}" && -z "${adapter_violations}" ]]; then
    echo "BOUNDARY-LINT: PASS no direct forbidden normal dependencies in plsql-* crates (${label})"
    return 0
  fi

  echo "package	dependency	rename	optional	manifest"
  if [[ -n "${violations}" ]]; then
    echo "BOUNDARY-LINT: FAIL direct forbidden dependency detected (${label})"
    echo "${violations}"
  fi
  if [[ -n "${adapter_violations}" ]]; then
    echo "BOUNDARY-LINT: FAIL unapproved direct oraclemcp-db adapter edge detected (${label})"
    echo "${adapter_violations}"
  fi
  return 1
}

print_cargo_tree_context() {
  local dep output

  echo "== cargo tree evidence for banned crates =="
  for dep in "${BANNED_DIRECT_DEPS[@]}"; do
    if output="$(cargo tree --locked -e normal -i "${dep}" 2>&1)"; then
      echo "-- ${dep}: present in the normal dependency graph"
      echo "${output}"
      echo
    elif grep -q "did not match any packages" <<<"${output}"; then
      echo "-- ${dep}: not present in the normal dependency graph"
    else
      echo "BOUNDARY-LINT: FAIL cargo tree probe failed for ${dep}" >&2
      echo "${output}" >&2
      return 2
    fi
  done
}

self_test() {
  local planted_metadata rc
  planted_metadata='{
    "packages": [
      {
        "name": "plsql-mcp",
        "manifest_path": "/tmp/plsql-mcp/Cargo.toml",
        "dependencies": [
          {
            "name": "tokio",
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
        "name": "plsql-engine",
        "manifest_path": "/tmp/plsql-engine/Cargo.toml",
        "dependencies": [
          {
            "name": "oraclemcp-db",
            "rename": null,
            "kind": null,
            "optional": false
          }
        ]
      }
    ]
  }'

  set +e
  run_metadata_gate "${planted_metadata}" "self-test planted tokio"
  rc=$?
  set -e

  if [[ ${rc} -eq 1 ]]; then
    echo "BOUNDARY-LINT: PASS planted direct tokio + unapproved adapter violations were rejected"
    return 0
  fi

  echo "BOUNDARY-LINT: FAIL planted direct violations were not rejected" >&2
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
  echo "== plsql-mcp boundary lint =="
  echo "repo=${REPO_ROOT}"
  echo "scope=direct normal dependencies of plsql-* workspace crates"

  local metadata rc
  metadata="$(cargo metadata --format-version=1 --locked --no-deps)"

  set +e
  run_metadata_gate "${metadata}" "workspace"
  rc=$?
  set -e

  print_cargo_tree_context
  exit "${rc}"
}

main "$@"
