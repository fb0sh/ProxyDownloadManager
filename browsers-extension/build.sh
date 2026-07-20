#!/usr/bin/env bash
# =============================================================================
# build.sh — assemble browser extension directories from shared/ source
#
# Usage: ./build.sh
#
# Copies shared/background.js and shared/icons/ into chrome/, edge/, firefox/.
# Keeps per-browser manifest.json intact (they genuinely differ).
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHARED="$SCRIPT_DIR/shared"
BROWSERS=(chrome edge firefox)

echo "==> Building browser extensions from shared/ source"

for browser in "${BROWSERS[@]}"; do
  target="$SCRIPT_DIR/$browser"
  echo "  -> $browser"

  # Copy background.js
  cp "$SHARED/background.js" "$target/background.js"

  # Copy icons
  mkdir -p "$target/icons"
  cp "$SHARED/icons/"*.png "$target/icons/"
done

echo "==> Done. chrome/, edge/, firefox/ synced from shared/."
