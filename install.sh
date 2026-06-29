#!/usr/bin/env bash
#
# plsql-intelligence installer
#
# One-liner install with cache buster:
#   curl -fsSL "https://raw.githubusercontent.com/MuhDur/plsql-intelligence/main/install.sh?$(date +%s)" | bash
#
# Or without cache buster:
#   curl -fsSL https://raw.githubusercontent.com/MuhDur/plsql-intelligence/main/install.sh | bash
#
# Options:
#   --quiet             Suppress non-error output
#   --no-gum            Disable gum formatting even when available
#   --force             Reinstall even if the selected version is already installed
#   --version <v>       Install a specific release tag/version
#   --bin-dir <dir>     Install binaries into dir (default: ~/.local/bin)
#   --easy-mode         Offer shell PATH updates when bin dir is not on PATH
#   --offline <tarball> Install from a pre-downloaded release tarball
#   --no-verify         Skip checksum/signature verification
#   --help              Show this help
#
set -euo pipefail
umask 022
shopt -s lastpipe 2>/dev/null || true

OWNER="MuhDur"
REPO="plsql-intelligence"
PROJECT_NAME="plsql-intelligence"
DESCRIPTION="Offline PL/SQL intelligence CLIs"
DEFAULT_BIN_DIR="${HOME}/.local/bin"

QUIET=0
NO_GUM=0
FORCE_INSTALL=0
EASY_MODE=0
NO_VERIFY=0
VERSION=""
BIN_DIR="$DEFAULT_BIN_DIR"
OFFLINE_TARBALL=""
HAS_GUM=0
TEMP_DIR=""

usage() {
  cat <<'USAGE'
plsql-intelligence installer

Usage:
  bash install.sh [options]
  curl -fsSL "https://raw.githubusercontent.com/MuhDur/plsql-intelligence/main/install.sh?$(date +%s)" | bash

Options:
  --quiet             Suppress non-error output
  --no-gum            Disable gum formatting even when available
  --force             Reinstall even if the selected version is already installed
  --version <v>       Install a specific release tag/version
  --bin-dir <dir>     Install binaries into dir (default: ~/.local/bin)
  --easy-mode         Offer shell PATH updates when bin dir is not on PATH
  --offline <tarball> Install from a pre-downloaded release tarball
  --no-verify         Skip checksum/signature verification
  --help              Show this help
USAGE
}

cleanup() {
  if [[ -n "${TEMP_DIR:-}" && -d "$TEMP_DIR" ]]; then
    rm -rf "$TEMP_DIR"
  fi
}
trap cleanup EXIT

detect_gum() {
  HAS_GUM=0
  if command -v gum >/dev/null 2>&1 && [[ -t 1 ]]; then
    HAS_GUM=1
  fi
}

info() {
  if [[ "$QUIET" -eq 1 ]]; then
    return 0
  fi
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 ]]; then
    gum style --foreground 39 "-> $*"
  else
    printf '\033[0;34m->\033[0m %s\n' "$*"
  fi
}

ok() {
  if [[ "$QUIET" -eq 1 ]]; then
    return 0
  fi
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 ]]; then
    gum style --foreground 42 "OK $*"
  else
    printf '\033[0;32mOK\033[0m %s\n' "$*"
  fi
}

warn() {
  if [[ "$QUIET" -eq 1 ]]; then
    return 0
  fi
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 ]]; then
    gum style --foreground 214 "WARN $*"
  else
    printf '\033[1;33mWARN\033[0m %s\n' "$*"
  fi
}

err() {
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 ]]; then
    gum style --foreground 196 "ERROR $*" >&2
  else
    printf '\033[0;31mERROR\033[0m %s\n' "$*" >&2
  fi
}

strip_ansi() {
  local esc
  esc=$(printf '\033')
  LC_ALL=C sed "s/${esc}\\[[0-9;]*m//g"
}

draw_box() {
  local color="$1"
  shift
  local lines=("$@")
  local max_width=0
  local line stripped len padding pad border inner_width i

  for line in "${lines[@]}"; do
    stripped=$(printf '%b' "$line" | strip_ansi)
    len=${#stripped}
    [[ "$len" -gt "$max_width" ]] && max_width=$len
  done

  inner_width=$((max_width + 4))
  border=""
  for ((i = 0; i < inner_width; i++)); do
    border+="═"
  done

  printf '\033[%sm╔%s╗\033[0m\n' "$color" "$border"
  for line in "${lines[@]}"; do
    stripped=$(printf '%b' "$line" | strip_ansi)
    len=${#stripped}
    padding=$((max_width - len))
    pad=""
    for ((i = 0; i < padding; i++)); do
      pad+=" "
    done
    printf '\033[%sm║\033[0m  %b%s  \033[%sm║\033[0m\n' "$color" "$line" "$pad" "$color"
  done
  printf '\033[%sm╚%s╝\033[0m\n' "$color" "$border"
}

run_with_spinner() {
  local title="$1"
  shift
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 && "$QUIET" -eq 0 ]]; then
    gum spin --spinner dot --title "$title" -- "$@"
  else
    info "$title"
    "$@"
  fi
}

show_header() {
  if [[ "$QUIET" -eq 1 ]]; then
    return 0
  fi
  if [[ "$HAS_GUM" -eq 1 && "$NO_GUM" -eq 0 ]]; then
    gum style \
      --border normal \
      --border-foreground 39 \
      --padding "0 1" \
      --margin "1 0" \
      "$(gum style --foreground 42 --bold "$PROJECT_NAME installer")" \
      "$(gum style --foreground 245 "$DESCRIPTION")"
  else
    draw_box "32" \
      "\033[1;32m${PROJECT_NAME} installer\033[0m" \
      "\033[0;90m${DESCRIPTION}\033[0m"
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --quiet)
        QUIET=1
        ;;
      --no-gum)
        NO_GUM=1
        ;;
      --force)
        FORCE_INSTALL=1
        ;;
      --easy-mode)
        EASY_MODE=1
        ;;
      --no-verify)
        NO_VERIFY=1
        ;;
      --version)
        shift
        [[ $# -gt 0 ]] || { err "--version requires a value"; exit 2; }
        VERSION="$1"
        ;;
      --bin-dir)
        shift
        [[ $# -gt 0 ]] || { err "--bin-dir requires a value"; exit 2; }
        BIN_DIR="$1"
        ;;
      --offline)
        shift
        [[ $# -gt 0 ]] || { err "--offline requires a tarball path"; exit 2; }
        OFFLINE_TARBALL="$1"
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        err "Unknown option: $1"
        usage >&2
        exit 2
        ;;
    esac
    shift
  done
}

create_temp_dir() {
  local temp_parent
  temp_parent="${TMPDIR:-/tmp}"
  TEMP_DIR=$(TMPDIR="$temp_parent" mktemp -d)
}

print_plan() {
  info "Repository: ${OWNER}/${REPO}"
  info "Install dir: $BIN_DIR"
  if [[ -n "$VERSION" ]]; then
    info "Version: $VERSION"
  else
    info "Version: latest release"
  fi
  if [[ -n "$OFFLINE_TARBALL" ]]; then
    info "Offline tarball: $OFFLINE_TARBALL"
  fi
  if [[ "$FORCE_INSTALL" -eq 1 ]]; then
    warn "Force reinstall enabled"
  fi
  if [[ "$NO_VERIFY" -eq 1 ]]; then
    warn "Checksum/signature verification disabled"
  fi
  if [[ "$EASY_MODE" -eq 1 ]]; then
    info "Easy mode PATH handling enabled"
  fi
}

main() {
  parse_args "$@"
  detect_gum
  create_temp_dir
  show_header
  print_plan
  ok "Installer scaffold initialized"
  info "Binary acquisition is implemented by the follow-up installer beads."
}

main "$@"
