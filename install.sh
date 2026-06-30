#!/usr/bin/env bash
#
# plsql-intelligence installer
#
# One-liner install with cache buster:
#   curl -fsSL "https://github.com/MuhDur/plsql-intelligence/releases/latest/download/install.sh?$(date +%s)" | bash
#
# Or from the development branch:
#   curl -fsSL "https://raw.githubusercontent.com/MuhDur/plsql-intelligence/main/install.sh?$(date +%s)" | bash
#
# Options:
#   --quiet             Suppress non-error output
#   --no-gum            Disable gum formatting even when available
#   --force             Reinstall even if the selected version is already installed
#   --version <v>       Install a specific release tag/version
#   --bin-dir <dir>     Install binaries into dir (default: ~/.local/bin)
#   --easy-mode         Offer shell PATH updates when bin dir is not on PATH
#   --offline <tarball> Install from a local tarball; include SHA256SUMS in the
#                       archive or place it next to the tarball
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
GITHUB_API_BASE="https://api.github.com/repos/${OWNER}/${REPO}"
GITHUB_RELEASES_URL="https://github.com/${OWNER}/${REPO}/releases"
PINNED_FALLBACK_VERSION="v0.7.0"
COSIGN_IDENTITY_RE="${COSIGN_IDENTITY_RE:-^https://github.com/${OWNER}/${REPO}/.github/workflows/.*$}"
COSIGN_OIDC_ISSUER="${COSIGN_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"

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
TARGET=""
FROM_SOURCE=0
TARGET_REASON=""
IS_WSL=0
TARGET_OS=""
TARGET_ARCH=""
ASSET_EXT=""
RESOLVED_VERSION=""
VERSION_SOURCE=""
SHA256SUMS_FILE=""
NO_VERIFY_WARNED=0
PROXY_URL=""
RELEASE_BINS=(plsql plsql-depgraph)
DOWNLOADED_BINS=()
DOWNLOADED_PATHS=()
LOCK_DIR=""
LOCK_HELD=0
PATH_STATUS=""
COMPLETIONS_STATUS=""

usage() {
  cat <<'USAGE'
plsql-intelligence installer

Usage:
  bash install.sh [options]
  curl -fsSL "https://github.com/MuhDur/plsql-intelligence/releases/latest/download/install.sh?$(date +%s)" | bash

Options:
  --quiet             Suppress non-error output
  --no-gum            Disable gum formatting even when available
  --force             Reinstall even if the selected version is already installed
  --version <v>       Install a specific release tag/version
  --bin-dir <dir>     Install binaries into dir (default: ~/.local/bin)
  --easy-mode         Offer shell PATH updates when bin dir is not on PATH
  --offline <tarball> Install from a local tarball; include SHA256SUMS in the
                      archive or place it next to the tarball
  --no-verify         Skip checksum/signature verification
  --help              Show this help
USAGE
}

cleanup() {
  release_lock || true
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

setup_proxy() {
  PROXY_URL=""
  if [[ -n "${HTTPS_PROXY:-}" ]]; then
    PROXY_URL="$HTTPS_PROXY"
    info "Using HTTPS proxy: $HTTPS_PROXY"
  elif [[ -n "${HTTP_PROXY:-}" ]]; then
    PROXY_URL="$HTTP_PROXY"
    info "Using HTTP proxy: $HTTP_PROXY"
  fi
}

curl_with_proxy() {
  if [[ -n "$PROXY_URL" ]]; then
    curl --proxy "$PROXY_URL" "$@"
  else
    curl "$@"
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

mark_from_source() {
  FROM_SOURCE=1
  TARGET="source"
  TARGET_REASON="$1"
}

detect_wsl() {
  local kernel_release

  kernel_release=$(uname -r 2>/dev/null || printf 'unknown')
  if printf '%s' "$kernel_release" | grep -qiE 'microsoft|wsl'; then
    IS_WSL=1
    return 0
  fi
  if [[ -r /proc/version ]] && grep -qiE 'microsoft|wsl' /proc/version; then
    IS_WSL=1
    return 0
  fi
  IS_WSL=0
}

detect_platform() {
  local os arch os_norm arch_norm

  os=$(uname -s 2>/dev/null || printf 'unknown')
  arch=$(uname -m 2>/dev/null || printf 'unknown')
  os_norm=$(printf '%s' "$os" | tr '[:upper:]' '[:lower:]')
  arch_norm=$(printf '%s' "$arch" | tr '[:upper:]' '[:lower:]')

  case "$arch_norm" in
    x86_64|amd64)
      arch_norm="x86_64"
      TARGET_ARCH="$arch_norm"
      ;;
    aarch64|arm64)
      arch_norm="aarch64"
      TARGET_ARCH="$arch_norm"
      ;;
    *)
      TARGET_ARCH="$arch_norm"
      mark_from_source "unsupported CPU architecture: $arch"
      return 0
      ;;
  esac

  case "$os_norm" in
    linux)
      TARGET_OS="linux"
      TARGET="${arch_norm}-unknown-linux-musl"
      detect_wsl
      ;;
    darwin)
      TARGET_OS="macos"
      TARGET="${arch_norm}-apple-darwin"
      ;;
    *)
      TARGET_OS="$os_norm"
      mark_from_source "unsupported operating system: $os"
      ;;
  esac
}

github_api_latest_tag() {
  local response tag

  command -v curl >/dev/null 2>&1 || return 1
  response=$(
    curl_with_proxy -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "User-Agent: ${PROJECT_NAME}-installer" \
      "${GITHUB_API_BASE}/releases/latest" 2>/dev/null || true
  )
  tag=$(printf '%s\n' "$response" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | sed -n '1p')
  [[ -n "$tag" ]] || return 1
  printf '%s\n' "$tag"
}

github_redirect_latest_tag() {
  local effective_url tag

  command -v curl >/dev/null 2>&1 || return 1
  effective_url=$(
    curl_with_proxy -fsSIL \
      -H "User-Agent: ${PROJECT_NAME}-installer" \
      -o /dev/null \
      -w '%{url_effective}' \
      "${GITHUB_RELEASES_URL}/latest" 2>/dev/null || true
  )
  tag=$(printf '%s\n' "$effective_url" | sed -n 's#.*/releases/tag/\([^/?#]*\).*#\1#p' | sed -n '1p')
  [[ -n "$tag" ]] || return 1
  printf '%s\n' "$tag"
}

resolve_version() {
  local tag

  if [[ -n "$VERSION" ]]; then
    RESOLVED_VERSION="$VERSION"
    VERSION_SOURCE="flag"
    return 0
  fi

  if [[ -n "$OFFLINE_TARBALL" ]]; then
    RESOLVED_VERSION="offline"
    VERSION_SOURCE="offline tarball"
    return 0
  fi

  if tag=$(github_api_latest_tag); then
    RESOLVED_VERSION="$tag"
    VERSION_SOURCE="GitHub API"
    return 0
  fi

  if tag=$(github_redirect_latest_tag); then
    RESOLVED_VERSION="$tag"
    VERSION_SOURCE="GitHub redirect"
    return 0
  fi

  RESOLVED_VERSION="$PINNED_FALLBACK_VERSION"
  VERSION_SOURCE="pinned fallback"
}

download_file() {
  local url="$1"
  local output="$2"

  command -v curl >/dev/null 2>&1 || return 1
  curl_with_proxy -fsSL \
    --connect-timeout 10 \
    --retry 2 \
    --retry-delay 1 \
    -H "User-Agent: ${PROJECT_NAME}-installer" \
    "$url" \
    -o "$output"
}

try_download_file() {
  local url="$1"
  local output="$2"

  download_file "$url" "$output" >/dev/null 2>&1
}

asset_name_for() {
  local bin="$1"
  printf '%s-%s%s\n' "$bin" "$TARGET" "$ASSET_EXT"
}

simple_asset_name_for() {
  local bin="$1"
  printf '%s-%s-%s%s\n' "$bin" "$TARGET_OS" "$TARGET_ARCH" "$ASSET_EXT"
}

ensure_checksums() {
  local output versioned_url latest_url

  [[ "$NO_VERIFY" -eq 0 ]] || return 0
  [[ -n "$SHA256SUMS_FILE" ]] && return 0

  output="$TEMP_DIR/SHA256SUMS"
  versioned_url="${GITHUB_RELEASES_URL}/download/${RESOLVED_VERSION}/SHA256SUMS"
  latest_url="${GITHUB_RELEASES_URL}/latest/download/SHA256SUMS"

  if try_download_file "$versioned_url" "$output"; then
    SHA256SUMS_FILE="$output"
    return 0
  fi
  if try_download_file "$latest_url" "$output"; then
    SHA256SUMS_FILE="$output"
    return 0
  fi

  err "Could not download SHA256SUMS; use --no-verify only if you accept the risk"
  return 1
}

checksum_for_asset() {
  local asset_name="$1"

  awk -v name="$asset_name" '
    $2 == name { print $1; found = 1; exit }
    END { if (!found) exit 1 }
  ' "$SHA256SUMS_FILE"
}

verify_checksum() {
  local asset_name="$1"
  local file="$2"
  local expected actual

  ensure_checksums
  expected=$(checksum_for_asset "$asset_name") || {
    err "SHA256SUMS has no entry for $asset_name"
    return 1
  }

  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{ print $1 }')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{ print $1 }')
  else
    err "No SHA256 tool found; install sha256sum or shasum, or pass --no-verify"
    return 1
  fi

  if [[ "$actual" != "$expected" ]]; then
    err "Checksum mismatch for $asset_name"
    err "  Expected: $expected"
    err "  Got:      $actual"
    return 1
  fi
  ok "SHA256 verified: $asset_name"
}

warn_no_verify_once() {
  if [[ "$NO_VERIFY_WARNED" -eq 0 ]]; then
    warn "Checksum/signature verification disabled"
    NO_VERIFY_WARNED=1
  fi
}

verify_sigstore() {
  local asset_name="$1"
  local file="$2"
  local asset_url="$3"
  local bundle_file

  [[ "$NO_VERIFY" -eq 0 ]] || {
    warn_no_verify_once
    return 0
  }

  if ! command -v cosign >/dev/null 2>&1; then
    warn "cosign not found; skipping optional Sigstore verification for $asset_name"
    return 0
  fi

  bundle_file="$TEMP_DIR/${asset_name}.sigstore"
  if ! try_download_file "${asset_url}.sigstore" "$bundle_file"; then
    warn "No Sigstore bundle found for $asset_name; checksum verification already passed"
    return 0
  fi

  if cosign verify-blob \
    --bundle "$bundle_file" \
    --certificate-identity-regexp "$COSIGN_IDENTITY_RE" \
    --certificate-oidc-issuer "$COSIGN_OIDC_ISSUER" \
    "$file" >/dev/null 2>&1; then
    ok "Sigstore verified: $asset_name"
  else
    err "Sigstore verification failed for $asset_name"
    return 1
  fi
}

verify_downloaded_asset() {
  local asset_name="$1"
  local file="$2"
  local asset_url="$3"

  if [[ "$NO_VERIFY" -eq 1 ]]; then
    warn_no_verify_once
    return 0
  fi

  verify_checksum "$asset_name" "$file"
  verify_sigstore "$asset_name" "$file" "$asset_url"
}

download_prebuilt_binary() {
  local bin="$1"
  local asset_name simple_name output url

  asset_name=$(asset_name_for "$bin")
  output="$TEMP_DIR/$asset_name"

  url="${GITHUB_RELEASES_URL}/download/${RESOLVED_VERSION}/${asset_name}"
  info "Downloading $asset_name"
  if try_download_file "$url" "$output"; then
    verify_downloaded_asset "$asset_name" "$output" "$url"
    DOWNLOADED_BINS+=("$bin")
    DOWNLOADED_PATHS+=("$output")
    return 0
  fi

  url="${GITHUB_RELEASES_URL}/latest/download/${asset_name}"
  if try_download_file "$url" "$output"; then
    verify_downloaded_asset "$asset_name" "$output" "$url"
    DOWNLOADED_BINS+=("$bin")
    DOWNLOADED_PATHS+=("$output")
    return 0
  fi

  simple_name=$(simple_asset_name_for "$bin")
  output="$TEMP_DIR/$simple_name"
  url="${GITHUB_RELEASES_URL}/latest/download/${simple_name}"
  if try_download_file "$url" "$output"; then
    verify_downloaded_asset "$simple_name" "$output" "$url"
    DOWNLOADED_BINS+=("$bin")
    DOWNLOADED_PATHS+=("$output")
    return 0
  fi

  return 1
}

download_prebuilt_binaries() {
  local bin

  DOWNLOADED_BINS=()
  DOWNLOADED_PATHS=()
  for bin in "${RELEASE_BINS[@]}"; do
    download_prebuilt_binary "$bin" || return 1
  done
}

install_binary() {
  local source="$1"
  local bin="$2"
  local destination="$BIN_DIR/$bin$ASSET_EXT"

  mkdir -p "$BIN_DIR"
  install -m 0755 "$source" "$destination"
  ok "Installed $bin -> $destination"
}

install_marker_path() {
  printf '%s/.plsql-intelligence-install\n' "$BIN_DIR"
}

write_install_marker() {
  local marker

  marker=$(install_marker_path)
  {
    printf 'version=%s\n' "$RESOLVED_VERSION"
    printf 'target=%s\n' "$TARGET"
    printf 'source=%s\n' "$VERSION_SOURCE"
  } > "$marker"
}

install_marker_matches() {
  local marker

  marker=$(install_marker_path)
  [[ -f "$marker" ]] || return 1
  grep -Fxq "version=$RESOLVED_VERSION" "$marker" || return 1
  grep -Fxq "target=$TARGET" "$marker" || return 1
}

install_downloaded_binaries() {
  local i

  for ((i = 0; i < ${#DOWNLOADED_BINS[@]}; i++)); do
    install_binary "${DOWNLOADED_PATHS[$i]}" "${DOWNLOADED_BINS[$i]}"
  done
}

archive_binary_path() {
  local extract_dir="$1"
  local bin="$2"
  local found

  found=$(find "$extract_dir" -type f \( -name "$bin" -o -name "${bin}${ASSET_EXT}" -o -name "$(asset_name_for "$bin")" \) -print | sed -n '1p')
  [[ -n "$found" ]] || return 1
  printf '%s\n' "$found"
}

offline_checksums_path() {
  local archive="$1"
  local extract_dir="$2"
  local archive_dir extracted adjacent

  extracted=$(find "$extract_dir" -type f -name SHA256SUMS -print | sed -n '1p')
  if [[ -n "$extracted" ]]; then
    printf '%s\n' "$extracted"
    return 0
  fi

  archive_dir=$(dirname "$archive")
  adjacent="$archive_dir/SHA256SUMS"
  if [[ -f "$adjacent" ]]; then
    printf '%s\n' "$adjacent"
    return 0
  fi

  return 1
}

install_from_archive() {
  local archive="$1"
  local extract_dir bin source source_name

  [[ -f "$archive" ]] || {
    err "Offline tarball not found: $archive"
    return 1
  }
  command -v tar >/dev/null 2>&1 || {
    err "tar is required to install from an offline archive"
    return 1
  }

  extract_dir="$TEMP_DIR/offline"
  mkdir -p "$extract_dir"
  tar -xf "$archive" -C "$extract_dir"

  if [[ "$NO_VERIFY" -eq 0 ]]; then
    SHA256SUMS_FILE=$(offline_checksums_path "$archive" "$extract_dir") || {
      err "Offline install requires SHA256SUMS inside the archive or next to it; pass --no-verify only if you accept the risk"
      return 1
    }
    info "Using offline checksums: $SHA256SUMS_FILE"
  fi

  for bin in "${RELEASE_BINS[@]}"; do
    source=$(archive_binary_path "$extract_dir" "$bin") || {
      err "Offline archive does not contain $bin"
      return 1
    }
    if [[ "$NO_VERIFY" -eq 0 ]]; then
      source_name=$(basename "$source")
      verify_checksum "$source_name" "$source"
    fi
    install_binary "$source" "$bin"
  done
}

source_package_for_bin() {
  case "$1" in
    plsql)
      printf 'plsql-cicd\n'
      ;;
    plsql-depgraph)
      printf 'plsql-depgraph\n'
      ;;
    *)
      return 1
      ;;
  esac
}

build_from_source() {
  local cargo_root bin package built_bin

  command -v cargo >/dev/null 2>&1 || {
    err "No prebuilt binary found and cargo is not installed for source fallback"
    return 1
  }

  cargo_root="$TEMP_DIR/cargo-install"
  for bin in "${RELEASE_BINS[@]}"; do
    package=$(source_package_for_bin "$bin")
    run_with_spinner "Building $bin from source" \
      cargo install \
        --git "https://github.com/${OWNER}/${REPO}.git" \
        --tag "$RESOLVED_VERSION" \
        --locked \
        --root "$cargo_root" \
        --bin "$bin" \
        "$package"
    built_bin="$cargo_root/bin/$bin"
    [[ -x "$built_bin" ]] || {
      err "Source build did not produce $built_bin"
      return 1
    }
    install_binary "$built_bin" "$bin"
  done
}

binary_matches_version() {
  local binary="$1"
  local output version_without_v

  output=$("$binary" --version 2>/dev/null || true)
  version_without_v="${RESOLVED_VERSION#v}"
  printf '%s\n' "$output" | grep -Fq "$RESOLVED_VERSION" && return 0
  printf '%s\n' "$output" | grep -Fq "$version_without_v"
}

already_installed() {
  local bin path

  [[ "$FORCE_INSTALL" -eq 0 ]] || return 1
  [[ "$RESOLVED_VERSION" != "offline" ]] || return 1

  for bin in "${RELEASE_BINS[@]}"; do
    path="$BIN_DIR/$bin$ASSET_EXT"
    [[ -x "$path" ]] || return 1
  done

  if install_marker_matches; then
    ok "Requested release already installed in $BIN_DIR"
    return 0
  fi

  for bin in "${RELEASE_BINS[@]}"; do
    path="$BIN_DIR/$bin$ASSET_EXT"
    binary_matches_version "$path" || return 1
  done

  ok "Requested version already installed in $BIN_DIR"
}

install_binaries() {
  if already_installed; then
    return 0
  fi

  if [[ -n "$OFFLINE_TARBALL" ]]; then
    install_from_archive "$OFFLINE_TARBALL"
    write_install_marker
    return
  fi

  if [[ "$FROM_SOURCE" -eq 1 ]]; then
    build_from_source
    write_install_marker
    return
  fi

  if download_prebuilt_binaries; then
    install_downloaded_binaries
    write_install_marker
    return
  fi

  warn "No complete prebuilt release asset set found for $TARGET; building from source"
  build_from_source
  write_install_marker
}

install_lock_dir() {
  local safe_bin_dir

  safe_bin_dir=$(printf '%s' "$BIN_DIR" | sed 's#[^A-Za-z0-9_.-]#_#g')
  printf '%s/plsql-intelligence-install-%s.lock\n' "${TMPDIR:-/tmp}" "$safe_bin_dir"
}

process_is_alive() {
  local pid="$1"

  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  kill -0 "$pid" >/dev/null 2>&1
}

release_lock() {
  [[ "$LOCK_HELD" -eq 1 && -n "$LOCK_DIR" ]] || return 0
  rm -f "$LOCK_DIR/pid"
  rmdir "$LOCK_DIR" 2>/dev/null || true
  LOCK_HELD=0
}

acquire_install_lock() {
  local pid

  LOCK_DIR=$(install_lock_dir)
  if mkdir "$LOCK_DIR" 2>/dev/null; then
    printf '%s\n' "$$" > "$LOCK_DIR/pid"
    LOCK_HELD=1
    return 0
  fi

  if [[ -f "$LOCK_DIR/pid" ]]; then
    pid=$(sed -n '1p' "$LOCK_DIR/pid")
    if process_is_alive "$pid"; then
      err "Another installer is running for $BIN_DIR (pid $pid)"
      return 1
    fi
    warn "Removing stale installer lock for $BIN_DIR"
    rm -f "$LOCK_DIR/pid"
    if rmdir "$LOCK_DIR" 2>/dev/null && mkdir "$LOCK_DIR" 2>/dev/null; then
      printf '%s\n' "$$" > "$LOCK_DIR/pid"
      LOCK_HELD=1
      return 0
    fi
  fi

  err "Could not acquire installer lock: $LOCK_DIR"
  return 1
}

available_kb_for_path() {
  local path="$1"

  while [[ ! -e "$path" && "$path" != "/" ]]; do
    path=$(dirname "$path")
  done
  df -Pk "$path" 2>/dev/null | awk 'NR == 2 { print $4 }'
}

network_reachable() {
  command -v curl >/dev/null 2>&1 || return 1
  curl_with_proxy -fsSI \
    --connect-timeout 5 \
    --retry 1 \
    -H "User-Agent: ${PROJECT_NAME}-installer" \
    "$GITHUB_RELEASES_URL" >/dev/null 2>&1
}

version_line_for_binary() {
  local binary="$1"

  if [[ -x "$binary" ]]; then
    "$binary" --version 2>/dev/null | sed -n '1p'
  fi
}

report_existing_install() {
  local bin path version

  for bin in "${RELEASE_BINS[@]}"; do
    path="$BIN_DIR/$bin$ASSET_EXT"
    if [[ -x "$path" ]]; then
      version=$(version_line_for_binary "$path")
      if [[ -n "$version" ]]; then
        info "Existing $bin: $version"
      else
        info "Existing $bin: present, version unavailable"
      fi
    else
      info "Existing $bin: not installed"
    fi
  done
}

preflight_checks() {
  local available_kb

  info "Running preflight checks"
  mkdir -p "$BIN_DIR" || {
    err "Could not create install directory: $BIN_DIR"
    return 1
  }
  [[ -w "$BIN_DIR" ]] || {
    err "Install directory is not writable: $BIN_DIR"
    return 1
  }

  available_kb=$(available_kb_for_path "$BIN_DIR")
  if [[ -n "$available_kb" && "$available_kb" -lt 10240 ]]; then
    err "At least 10 MB free space is required in $BIN_DIR"
    return 1
  fi
  if [[ -n "$available_kb" ]]; then
    info "Free space: $((available_kb / 1024)) MB"
  else
    warn "Could not determine free disk space for $BIN_DIR"
  fi

  if [[ -z "$OFFLINE_TARBALL" ]]; then
    if network_reachable; then
      info "Network: GitHub reachable"
    else
      warn "Network preflight could not reach GitHub; installer will continue and may fall back to source"
    fi
  else
    info "Network: skipped for offline install"
  fi

  report_existing_install
}

path_has_bin_dir() {
  case ":$PATH:" in
    *":$BIN_DIR:"*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

append_path_to_rc() {
  local rc_file="$1"
  local line

  line="export PATH=\"$BIN_DIR:\$PATH\""
  touch "$rc_file"
  if grep -Fxq "$line" "$rc_file"; then
    return 0
  fi
  printf '\n%s\n' "$line" >> "$rc_file"
}

maybe_add_path() {
  local updated=()
  local rc_file

  if path_has_bin_dir; then
    PATH_STATUS="$BIN_DIR is already on PATH"
    return 0
  fi

  if [[ "$EASY_MODE" -eq 1 ]]; then
    for rc_file in "$HOME/.zshrc" "$HOME/.bashrc"; do
      append_path_to_rc "$rc_file"
      updated+=("$rc_file")
    done
    PATH_STATUS="PATH export added to shell rc files"
    ok "Added PATH export to ${updated[*]}"
  else
    PATH_STATUS="$BIN_DIR is not on PATH"
    warn "$PATH_STATUS; add: export PATH=\"$BIN_DIR:\$PATH\""
  fi
}

completion_paths_for_shell() {
  local shell_name="$1"
  local data_home config_home

  data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
  config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  case "$shell_name" in
    bash)
      printf '%s/bash-completion/completions/plsql\n' "$data_home"
      ;;
    zsh)
      printf '%s/zsh/site-functions/_plsql\n' "$data_home"
      ;;
    fish)
      printf '%s/fish/completions/plsql.fish\n' "$config_home"
      ;;
    *)
      return 1
      ;;
  esac
}

install_completions() {
  local plsql_bin="$BIN_DIR/plsql$ASSET_EXT"
  local shell_name destination installed=()

  [[ -x "$plsql_bin" ]] || {
    COMPLETIONS_STATUS="skipped; plsql binary not found"
    warn "Shell completions skipped; plsql binary not found"
    return 0
  }

  if ! "$plsql_bin" completions bash >/dev/null 2>&1; then
    COMPLETIONS_STATUS="not supported by installed plsql"
    warn "Shell completions skipped; installed plsql has no completions subcommand"
    return 0
  fi

  for shell_name in bash zsh fish; do
    destination=$(completion_paths_for_shell "$shell_name")
    mkdir -p "$(dirname "$destination")"
    if "$plsql_bin" completions "$shell_name" > "$destination"; then
      installed+=("$shell_name")
    else
      warn "Could not generate $shell_name completions"
    fi
  done

  if [[ "${#installed[@]}" -gt 0 ]]; then
    COMPLETIONS_STATUS="installed for ${installed[*]}"
    ok "Shell completions $COMPLETIONS_STATUS"
  else
    COMPLETIONS_STATUS="not installed"
  fi
}

summary_line_for_binary() {
  local bin="$1"
  local path="$BIN_DIR/$bin$ASSET_EXT"
  local version

  if [[ -x "$path" ]]; then
    version=$(version_line_for_binary "$path")
    if [[ -n "$version" ]]; then
      printf '%s: %s\n' "$bin" "$version"
      return 0
    fi
    printf '%s: installed\n' "$bin"
    return 0
  fi
  printf '%s: missing\n' "$bin"
}

final_summary() {
  local uninstall

  [[ "$QUIET" -eq 0 ]] || return 0
  uninstall="remove plsql, plsql-depgraph, .plsql-intelligence-install from bin dir"
  draw_box "36" \
    "Installed $(summary_line_for_binary plsql)" \
    "Installed $(summary_line_for_binary plsql-depgraph)" \
    "Bin dir: $BIN_DIR" \
    "PATH: ${PATH_STATUS:-not checked}" \
    "Completions: ${COMPLETIONS_STATUS:-not checked}" \
    "Uninstall: $uninstall"
}

create_temp_dir() {
  local temp_parent
  temp_parent="${TMPDIR:-/tmp}"
  TEMP_DIR=$(TMPDIR="$temp_parent" mktemp -d)
}

print_plan() {
  info "Repository: ${OWNER}/${REPO}"
  info "Install dir: $BIN_DIR"
  if [[ "$FROM_SOURCE" -eq 1 ]]; then
    warn "No prebuilt target selected: $TARGET_REASON"
    info "Target: build from source"
  else
    info "Target: $TARGET"
  fi
  if [[ "$IS_WSL" -eq 1 ]]; then
    warn "WSL detected; continuing with Linux static target $TARGET"
  fi
  if [[ -n "$RESOLVED_VERSION" ]]; then
    info "Version: ${RESOLVED_VERSION} (${VERSION_SOURCE})"
  else
    info "Version: unresolved"
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
  setup_proxy
  detect_platform
  resolve_version
  create_temp_dir
  acquire_install_lock
  show_header
  preflight_checks
  print_plan
  install_binaries
  maybe_add_path
  install_completions
  final_summary
  ok "Installation complete"
}

main "$@"
