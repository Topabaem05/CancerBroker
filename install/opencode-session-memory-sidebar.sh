#!/bin/sh

set -eu

INSTALLER_REPO=${OPENCODE_SIDEBAR_INSTALLER_REPO:-"Topabaem05/CancerBroker"}
INSTALLER_REF=${OPENCODE_SIDEBAR_INSTALLER_REF:-"main"}
INSTALLER_CONTENT_PATH=${OPENCODE_SIDEBAR_INSTALLER_CONTENT_PATH:-"packaging/npm/opencode-session-memory-sidebar-installer/dist/opencode-session-memory-sidebar-installer.cjs"}
INSTALLER_URL=${OPENCODE_SIDEBAR_INSTALLER_URL:-"https://raw.githubusercontent.com/$INSTALLER_REPO/$INSTALLER_REF/$INSTALLER_CONTENT_PATH"}

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

if ! curl -fsSL "$INSTALLER_URL" -o "$INSTALLER_PATH"; then
  if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    gh api "repos/$INSTALLER_REPO/contents/$INSTALLER_CONTENT_PATH?ref=$INSTALLER_REF" --jq .content \
      | tr -d '\n' \
      | "$RUNTIME" -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
      > "$INSTALLER_PATH"
  else
    printf '%s\n' "Unable to download installer from $INSTALLER_URL." >&2
    printf '%s\n' "For private repositories, authenticate GitHub CLI and bootstrap this script with gh api." >&2
    exit 1
  fi
fi

exec "$RUNTIME" "$INSTALLER_PATH" "$@"
