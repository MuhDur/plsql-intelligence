#!/usr/bin/env bash
# Offline engine honesty-grep gate (oracle-jfqh.23).
#
# Fails if release-visible text drifts back into stale server/runtime claims or
# over-claims. Tests, fuzz targets, and draft plans are excluded because they
# may intentionally carry negative examples.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

PATTERN='read[- ]only binary|read[- ]only[- ]only|safe[- ]by[- ]default|safe by construction|fully audited|independently[- ]audited dependencies|1\.0-frozen|tamper[- ]evident audit|\bPAM\b|(^|[^[:alnum:]_])requires?([^[:alnum:]_]|$)[^.]*nightly|(^|[^[:alnum:]_])requires?([^[:alnum:]_]|$)[^.]*asupersync|(^|[^[:alnum:]_])requires?([^[:alnum:]_]|$)[^.]*instant[- ]client|(^|[^[:alnum:]_])requires?([^[:alnum:]_]|$)[^.]*two[- ]servers?|two[- ]server architecture|dual[- ]server architecture|plsql-mcp server|plsql-mcp mcp server'

usage() {
  cat <<'USAGE'
Usage: scripts/offline_honesty_grep.sh [--self-test]

Fails on forbidden release-visible framing:
  read-only binary / read-only only
  safe-by-default / safe by construction
  fully audited / independently-audited dependencies
  1.0-frozen
  uncaveated tamper-evident audit / PAM
  requires nightly
  requires asupersync
  requires Instant Client
  requires two servers / two-server architecture / dual-server architecture
  plsql-mcp server / plsql-mcp MCP server

Append `honesty-allow: <reason>` to the same line only for historical notes,
pattern definitions, or negative examples that must quote forbidden wording.
USAGE
}

scan_stream() {
  local label="$1"
  local line_no=0
  local found=0
  local text

  while IFS= read -r text; do
    line_no=$((line_no + 1))
    case "${text}" in
      *honesty-allow*) continue ;;
    esac
    if grep -qiE "${PATTERN}" <<<"${text}"; then
      printf 'FORBIDDEN framing  %s:%s:%s\n' \
        "${label}" \
        "${line_no}" \
        "${text#"${text%%[![:space:]]*}"}"
      found=1
    fi
  done

  if [ "${found}" -eq 0 ]; then
    return 0
  fi
  return 1
}

scan_file() {
  local file="$1"
  local found=0
  local line text

  while IFS=: read -r line text; do
    [ -n "${line}" ] || continue
    case "${text}" in
      *honesty-allow*) continue ;;
    esac
    printf 'FORBIDDEN framing  %s:%s:%s\n' \
      "${file}" \
      "${line}" \
      "${text#"${text%%[![:space:]]*}"}"
    found=1
  done < <(grep -niE "${PATTERN}" "${file}" 2>/dev/null || true)

  if [ "${found}" -eq 0 ]; then
    return 0
  fi
  return 1
}

self_test() {
  if scan_stream "self-test" <<'EOF'
This product is a safe-by-default, fully audited, 1.0-frozen read-only binary.
It requires nightly, asupersync, Instant Client, and two servers.
EOF
  then
    echo "offline-honesty-grep: SELF-TEST FAIL - planted violation was accepted." >&2
    return 1
  fi
  echo "offline-honesty-grep: SELF-TEST PASS - planted violation rejected."
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
  usage
  exit 0
fi

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

if [ "$#" -gt 0 ]; then
  usage >&2
  exit 2
fi

mapfile -t FILES < <(
  git ls-files \
    README.md PUBLISHING.md CHANGELOG.md Cargo.toml docs crates .github \
    | grep -E '\.(md|rs|toml|ya?ml)$|(^|/)Dockerfile$' \
    | grep -vE '(^|/)tests?/|tests\.rs$|/fuzz/|^docs/plans/'
)

violations=0
for f in "${FILES[@]}"; do
  [ -n "${f}" ] || continue
  if ! scan_file "${f}"; then
    violations=$((violations + 1))
  fi
done

if [ "${violations}" -gt 0 ]; then
  echo "offline-honesty-grep: FAIL - forbidden framing in ${violations} file(s)."
  echo "Reframe the claim, or add a same-line 'honesty-allow: <reason>' marker for historical/negative-test text."
  exit 1
fi

echo "offline-honesty-grep: OK - no forbidden release-visible framing."
