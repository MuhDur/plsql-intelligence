#!/usr/bin/env bash
# USR Loop — §4 Accretion Monotonic Tripwire (PLSQL-USR-001, P6).
#
# Makes "accretive" a VERIFIED property (spec §1 I-MONOTONIC-VALUE,
# §4). Computes the §4 `coverage_index` and asserts it is monotonic
# non-decreasing across releases. Wired into CI as a required check;
# a release that lowers the index FAILs here.
#
#   coverage_index = extracted_semantics_ratio
#                      (over a FROZEN public benchmark set, corpus-
#                       derived, NEVER private estate code —
#                       reproducible by anyone)
#                  + distinct_resolved_gap_signatures
#                      (count of signature classes the loop has
#                       PERMANENTLY closed, read from the append-only
#                       provenance Ledger's landed entries)
#
# It appends the measurement to the append-only, content-addressed
# `accretion_ledger.jsonl` (its own tamper-evident hash chain) and
# asserts `coverage_index(HEAD) >= coverage_index(last release tag)`.
#
# FIRST-RUN SEMANTICS (documented, spec §4): if no release tag exists
# yet (or `--baseline-ref` names an unknown ref), there is no prior
# point a regression could be measured against, so the run
# deterministically SEEDS the monotone floor and PASSes. Every
# subsequent run compares against that committed history.
#
#   Usage: scripts/accretion_tripwire.sh [<git-ref>] [<baseline-ref>]
#   Exit:  0 = index monotone non-decreasing (or floor seeded)
#          1 = coverage_index DROPPED (I-MONOTONIC-VALUE violated)
#
# Deterministic + re-runnable: the index is a pure function of the
# frozen corpus scan + the Ledger; no wall-clock is persisted (the
# only time-like field is the git ref, itself deterministic). Running
# twice at the same ref is an idempotent no-op append.
#
# AGENTS.md C5/C6: this NEVER touches a private estate — the benchmark
# set is the committed public corpus. The accretion compounding is thus
# public + auditable, never asserted by vibes.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}" || { echo "TRIPWIRE: FAIL cannot cd repo root"; exit 1; }

# Frozen public benchmark set (corpus-derived, NEVER private estate code).
BENCH="${USR_TRIPWIRE_BENCH:-${REPO_ROOT}/corpus/synthetic/l1}"
GIT_REF="${1:-HEAD}"
# Auto-detect the most recent release tag as the monotone baseline;
# none yet ⇒ first run seeds the floor (documented above).
BASELINE_REF="${2:-$(git -C "${REPO_ROOT}" describe --tags --abbrev=0 2>/dev/null || true)}"

if [[ ! -d "${BENCH}" ]]; then
  echo "TRIPWIRE: FAIL frozen benchmark set missing: ${BENCH}"
  exit 1
fi

echo "== USR §4 accretion tripwire =="
echo "benchmark (public, never a private estate): ${BENCH}"
echo "git_ref=${GIT_REF}  baseline_ref=${BASELINE_REF:-<none — first run seeds the monotone floor>}"

ARGS=(run -q -p usr-loop -- ledger tripwire "${BENCH}" --git-ref "${GIT_REF}")
if [[ -n "${BASELINE_REF}" ]]; then
  ARGS+=(--baseline-ref "${BASELINE_REF}")
fi

OUT="$(CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/cargo-target}" cargo "${ARGS[@]}" 2>/tmp/usr_tripwire.log)"
RC=$?
echo "${OUT}"

if [[ ${RC} -ne 0 ]]; then
  echo "TRIPWIRE: FAIL coverage_index regressed — I-MONOTONIC-VALUE violated; a release may NOT lower it (spec §4)"
  exit 1
fi
echo "TRIPWIRE: PASS coverage_index monotone non-decreasing (spec §4 / §1 I-MONOTONIC-VALUE)"
exit 0
