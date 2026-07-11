#!/usr/bin/env bash
# Update atlasfetch from GitHub releases
# Usage: update-atlasfetch.sh [--dry-run]

set -euo pipefail

REPO="mafuzyk/atlasfetch"
BINARY_NAME="atlasfetch"
INSTALL_DIR="${INSTALL_DIR:-/home/charlie/Projetos/atlas/target/release}"
SYMLINK_DIR="${SYMLINK_DIR:-/home/charlie/.local/bin}"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log() { echo -e "${GREEN}[update]${NC} $*"; }
warn() { echo -e "${YELLOW}[update]${NC} $*"; }
err() { echo -e "${RED}[update]${NC} $*"; }

get_latest_version() {
    curl -s "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' \
        | sed -E 's/.*"([^"]+)".*/\1/'
}

get_current_version() {
    if command -v "$BINARY_NAME" &>/dev/null; then
        "$BINARY_NAME" -V 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown"
    else
        echo "not-installed"
    fi
}

download_binary() {
    local version="$1"
    local arch="$(uname -m)"
    local asset_name=""

    case "$arch" in
        x86_64) asset_name="${BINARY_NAME}-${version}-x86_64-unknown-linux-gnu.tar.gz" ;;
        aarch64) asset_name="${BINARY_NAME}-${version}-aarch64-unknown-linux-gnu.tar.gz" ;;
        *) err "Unsupported architecture: $arch"; exit 1 ;;
    esac

    local url="https://github.com/${REPO}/releases/download/${version}/${asset_name}"
    local tmp_dir="$(mktemp -d)"
    local tmp_file="${tmp_dir}/${asset_name}"

    log "Downloading ${asset_name}..."
    if ! curl -L -o "$tmp_file" "$url"; then
        err "Failed to download release asset"
        rm -rf "$tmp_dir"
        exit 1
    fi

    log "Extracting..."
    tar -xzf "$tmp_file" -C "$tmp_dir"

    local binary_path="${tmp_dir}/${BINARY_NAME}"
    if [[ ! -f "$binary_path" ]]; then
        err "Binary not found in archive"
        rm -rf "$tmp_dir"
        exit 1
    fi

    echo "$binary_path"
}

main() {
    local dry_run=false
    [[ "${1:-}" == "--dry-run" ]] && dry_run=true

    log "Checking for updates..."
    local latest_version
    latest_version=$(get_latest_version)
    local current_version
    current_version=$(get_current_version)

    log "Current version: $current_version"
    log "Latest version:  $latest_version"

    if [[ "$current_version" == "$latest_version" ]]; then
        log "Already up to date!"
        exit 0
    fi

    if [[ "$dry_run" == true ]]; then
        log "Dry run: would update to $latest_version"
        exit 0
    fi

    local binary_path
    binary_path=$(download_binary "$latest_version")

    log "Installing to $INSTALL_DIR..."
    mkdir -p "$INSTALL_DIR"
    cp "$binary_path" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    log "Updating symlink in $SYMLINK_DIR..."
    mkdir -p "$SYMLINK_DIR"
    ln -sf "${INSTALL_DIR}/${BINARY_NAME}" "${SYMLINK_DIR}/${BINARY_NAME}"

    # Cleanup
    rm -rf "$(dirname "$binary_path")"

    log "Updated to ${latest_version}!"
    "$BINARY_NAME" -V
}

main "$@"
