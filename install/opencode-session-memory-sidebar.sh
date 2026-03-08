#!/bin/sh

set -eu

INSTALLER_URL=${OPENCODE_SIDEBAR_INSTALLER_URL:-"https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/packaging/npm/opencode-session-memory-sidebar-installer/dist/opencode-session-memory-sidebar-installer.cjs"}

if ! command -v curl >/dev/null 2>&1; then
  printf '%s\n' "curl is required to fetch the installer." >&2
  exit 1
fi

if command -v node >/dev/null 2>&1; then
  RUNTIME=node
elif command -v bun >/dev/null 2>&1; then
  RUNTIME=bun
else
  printf '%s\n' "node or bun is required to run the installer." >&2
  exit 1
fi

TMP_DIR=$(mktemp -d)
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

INSTALLER_PATH="$TMP_DIR/opencode-session-memory-sidebar-installer.cjs"
curl -fsSL "$INSTALLER_URL" -o "$INSTALLER_PATH"

exec "$RUNTIME" "$INSTALLER_PATH" "$@"
