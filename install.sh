#!/usr/bin/env bash
#
# vibe-control — macOS command-line installer.
#
# Downloads the latest universal .dmg from the GitHub Releases page, installs
# vibe-control.app into /Applications, and clears the Gatekeeper quarantine flag
# (the build is ad-hoc signed, not notarized — see the release notes).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/reiple/vibe-control/main/install.sh | bash
#
# Optional: pin a version with VC_VERSION, e.g.
#   curl -fsSL .../install.sh | VC_VERSION=v0.1.0 bash

set -euo pipefail

REPO="reiple/vibe-control"
APP_NAME="vibe-control.app"
INSTALL_DIR="/Applications"

log() { printf '\033[1;32m==>\033[0m %s\n' "$1"; }
err() { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || err "install.sh is for macOS. On Windows use install.ps1."
command -v curl >/dev/null 2>&1 || err "curl is required."

# Resolve the release API URL (latest by default, or a pinned tag).
if [ -n "${VC_VERSION:-}" ]; then
  api_url="https://api.github.com/repos/${REPO}/releases/tags/${VC_VERSION}"
else
  api_url="https://api.github.com/repos/${REPO}/releases/latest"
fi

log "Looking up release from ${REPO} ..."
release_json="$(curl -fsSL -H "Accept: application/vnd.github+json" "$api_url")" \
  || err "could not reach GitHub Releases (is the repo public and a release published?)."

# Prefer a universal dmg; fall back to any .dmg. No jq dependency — parse the
# browser_download_url fields for .dmg assets.
dmg_url="$(printf '%s\n' "$release_json" \
  | grep -o '"browser_download_url":[[:space:]]*"[^"]*\.dmg"' \
  | sed -E 's/.*"(https[^"]+)".*/\1/' \
  | grep -i 'universal' | head -n1)"
if [ -z "$dmg_url" ]; then
  dmg_url="$(printf '%s\n' "$release_json" \
    | grep -o '"browser_download_url":[[:space:]]*"[^"]*\.dmg"' \
    | sed -E 's/.*"(https[^"]+)".*/\1/' | head -n1)"
fi
[ -n "$dmg_url" ] || err "no .dmg asset found in the release."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
dmg="$tmp/vibe-control.dmg"

log "Downloading $(basename "$dmg_url") ..."
curl -fL --progress-bar "$dmg_url" -o "$dmg" || err "download failed."

log "Mounting disk image ..."
mount_point="$(hdiutil attach "$dmg" -nobrowse -readonly | grep -Eo '/Volumes/[^ ]+.*' | tail -n1)"
[ -n "$mount_point" ] && [ -d "$mount_point" ] || err "failed to mount the dmg."
# shellcheck disable=SC2064
trap "hdiutil detach '$mount_point' -quiet >/dev/null 2>&1 || true; rm -rf '$tmp'" EXIT

src="$mount_point/$APP_NAME"
[ -d "$src" ] || err "$APP_NAME not found inside the dmg."

log "Installing to ${INSTALL_DIR}/${APP_NAME} ..."
rm -rf "${INSTALL_DIR:?}/${APP_NAME}"
if ! cp -R "$src" "$INSTALL_DIR/" 2>/dev/null; then
  log "Permission needed for ${INSTALL_DIR} — retrying with sudo."
  sudo cp -R "$src" "$INSTALL_DIR/"
fi

# Clear the quarantine flag so the unsigned/ad-hoc build opens without the
# "damaged / cannot be opened" Gatekeeper block.
log "Clearing Gatekeeper quarantine ..."
xattr -dr com.apple.quarantine "${INSTALL_DIR}/${APP_NAME}" 2>/dev/null \
  || sudo xattr -dr com.apple.quarantine "${INSTALL_DIR}/${APP_NAME}" 2>/dev/null || true

log "Installed. Launch with:  open -a vibe-control"
log "First run asks for Automation + Accessibility permission — grant it in System Settings › Privacy & Security."
