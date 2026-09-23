#!/usr/bin/env bash
# =============================================================================
# build.sh — assemble browser extension directories from shared/ source
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHARED="$SCRIPT_DIR/shared"
BROWSERS=(chrome edge firefox)

echo "==> Building browser extensions from shared/ source"

for browser in "${BROWSERS[@]}"; do
  target="$SCRIPT_DIR/$browser"
  echo "  -> $browser"
  mkdir -p "$target/icons"
  cp "$SHARED/background.js" "$target/background.js"
  cp "$SHARED/protocol.js" "$target/protocol.js"
  cp "$SHARED/content.js" "$target/content.js"
  cp "$SHARED/popup.html" "$target/popup.html"
  cp "$SHARED/popup.js" "$target/popup.js"
  cp "$SHARED/icons/"*.png "$target/icons/"
done

echo "==> Done. chrome/, edge/, firefox/ synced from shared/."
