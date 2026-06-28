#!/usr/bin/env bash
# oraclemcp pin currency guard (oracle-plsql-converge-0lnu.15.14).
#
# Fails if a first-party plsql-* crate depends directly on an oraclemcp-* crate
# without an exact pin, or if crates.io has a newer published SemVer version
# than the exact pin. The self-test uses an override map so it proves stale-pin
# failure without waiting for a real crates.io release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: scripts/oraclemcp_pin_guard.sh [--self-test]

Checks direct normal dependencies from first-party plsql-* crates to
oraclemcp-* crates:
  - every dependency requirement must be an exact =x.y.z pin
  - the exact pin must match the latest published crates.io version

Self-test plants a stale =0.3.0 pin against an overridden latest 0.4.0 and
requires the guard to reject it.
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "ORACLEMCP-PIN-GUARD: FAIL required tool not found: ${tool}" >&2
    exit 2
  fi
}

direct_oraclemcp_deps_from_metadata() {
  local metadata="$1"
  jq -r '
    .packages[]
    | select(.name | startswith("plsql-"))
    | .name as $package
    | .manifest_path as $manifest
    | .dependencies[]?
    | select(.kind == null)
    | select(.name | startswith("oraclemcp-"))
    | [$package, .name, .req, (.optional | tostring), $manifest]
    | @tsv
  ' <<<"${metadata}"
}

exact_version_from_req() {
  local req="$1"
  if [[ "${req}" =~ ^=([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    printf '%s.%s.%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}"
    return 0
  fi
  return 1
}

simple_semver() {
  local version="$1"
  [[ "${version}" =~ ^[0-9]+[.][0-9]+[.][0-9]+$ ]]
}

version_gt() {
  local left="$1"
  local right="$2"
  local left_major left_minor left_patch right_major right_minor right_patch

  simple_semver "${left}" || return 2
  simple_semver "${right}" || return 2

  IFS=. read -r left_major left_minor left_patch <<<"${left}"
  IFS=. read -r right_major right_minor right_patch <<<"${right}"

  (( left_major > right_major )) && return 0
  (( left_major < right_major )) && return 1
  (( left_minor > right_minor )) && return 0
  (( left_minor < right_minor )) && return 1
  (( left_patch > right_patch ))
}

latest_override_for_crate() {
  local crate="$1"
  if [[ -z "${ORACLEMCP_PIN_GUARD_LATEST_OVERRIDES:-}" ]]; then
    return 1
  fi
  awk -v crate="${crate}" '
    $1 == crate {
      print $2
      found = 1
      exit
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' <<<"${ORACLEMCP_PIN_GUARD_LATEST_OVERRIDES}"
}

latest_published_version() {
  local crate="$1"
  local info latest

  if latest="$(latest_override_for_crate "${crate}")"; then
    printf '%s\n' "${latest}"
    return 0
  fi

  if ! info="$(cargo info "${crate}" --locked 2>&1)"; then
    echo "ORACLEMCP-PIN-GUARD: FAIL cargo info failed for ${crate}" >&2
    echo "${info}" >&2
    return 2
  fi
  latest="$(awk -F': ' '$1 == "version" {print $2; exit}' <<<"${info}")"
  if [[ -z "${latest}" ]]; then
    echo "ORACLEMCP-PIN-GUARD: FAIL could not parse latest version for ${crate}" >&2
    echo "${info}" >&2
    return 2
  fi
  printf '%s\n' "${latest}"
}

run_pin_gate() {
  local metadata="$1"
  local label="$2"
  local rows row_count failures

  rows="$(direct_oraclemcp_deps_from_metadata "${metadata}")"
  if [[ -z "${rows}" ]]; then
    echo "ORACLEMCP-PIN-GUARD: FAIL no direct oraclemcp-* dependencies found (${label})" >&2
    return 1
  fi

  row_count=0
  failures=0
  printf 'package\tcrate\tpin\tlatest\tstatus\tmanifest\n'
  while IFS=$'\t' read -r package crate req optional manifest; do
    local pinned latest status
    row_count=$((row_count + 1))
    status="PASS"

    if ! pinned="$(exact_version_from_req "${req}")"; then
      printf '%s\t%s\t%s\t-\tFAIL non-exact dependency requirement\t%s\n' \
        "${package}" "${crate}" "${req}" "${manifest}"
      failures=$((failures + 1))
      continue
    fi

    if ! latest="$(latest_published_version "${crate}")"; then
      printf '%s\t%s\t%s\t-\tFAIL latest lookup failed\t%s\n' \
        "${package}" "${crate}" "${req}" "${manifest}"
      failures=$((failures + 1))
      continue
    fi

    if version_gt "${latest}" "${pinned}"; then
      status="FAIL stale pin"
      failures=$((failures + 1))
    elif ! simple_semver "${latest}"; then
      status="FAIL unparsable latest SemVer"
      failures=$((failures + 1))
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
      "${package}" "${crate}" "${req}" "${latest}" "${status}" "${manifest}"
  done <<<"${rows}"

  if [[ "${row_count}" -eq 0 ]]; then
    echo "ORACLEMCP-PIN-GUARD: FAIL no direct oraclemcp-* dependency rows scanned (${label})" >&2
    return 1
  fi

  if [[ "${failures}" -gt 0 ]]; then
    echo "ORACLEMCP-PIN-GUARD: FAIL ${failures} stale or malformed pin(s) detected (${label})" >&2
    return 1
  fi

  echo "ORACLEMCP-PIN-GUARD: PASS ${row_count} direct oraclemcp-* pin(s) match latest published versions (${label})"
}

self_test() {
  local planted_metadata overrides rc
  planted_metadata='{
    "packages": [
      {
        "name": "plsql-mcp",
        "manifest_path": "/tmp/plsql-mcp/Cargo.toml",
        "dependencies": [
          {
            "name": "oraclemcp-core",
            "req": "=0.3.0",
            "kind": null,
            "optional": false
          },
          {
            "name": "oraclemcp-db",
            "req": "=0.4.0",
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
            "name": "oraclemcp-db",
            "req": "=0.4.0",
            "kind": null,
            "optional": true
          }
        ]
      }
    ]
  }'
  overrides=$'oraclemcp-core 0.4.0\noraclemcp-db 0.4.0'

  set +e
  ORACLEMCP_PIN_GUARD_LATEST_OVERRIDES="${overrides}" \
    run_pin_gate "${planted_metadata}" "self-test planted stale pin"
  rc=$?
  set -e

  if [[ "${rc}" -eq 1 ]]; then
    echo "ORACLEMCP-PIN-GUARD: SELF-TEST PASS planted stale pin was rejected"
    return 0
  fi

  echo "ORACLEMCP-PIN-GUARD: SELF-TEST FAIL planted stale pin was accepted" >&2
  return 1
}

main() {
  local mode="${1:-}"
  if [[ "$#" -gt 1 ]]; then
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
  echo "== oraclemcp pin guard =="
  echo "repo=${REPO_ROOT}"
  echo "scope=direct normal oraclemcp-* dependencies of first-party plsql-* crates"

  local metadata
  metadata="$(cargo metadata --format-version=1 --locked --no-deps)"
  run_pin_gate "${metadata}" "workspace"
}

main "$@"
