#!/usr/bin/env bash
# Regenerate the color profile gallery images.
# Usage: ./scripts/generate-gallery.sh [hitmap_binary]
#   If no binary is specified, uses the current project's release build.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

BIN="${1:-$PROJECT_DIR/target/release/hitmap}"
GALLERY="$PROJECT_DIR/docs/images/gallery"
LIGHT_REPO="$HOME/projects/bi-and-analytics"
DARK_REPO="$HOME/projects/chronova"

PROFILES=("github" "aurora" "ocean" "fire" "catppuccin-latte" "catppuccin-frappe" "catppuccin-macchiato" "catppuccin-mocha")

mkdir -p "$GALLERY"

if [ ! -x "$BIN" ]; then
  echo "Building release binary..."
  cargo --manifest-path "$PROJECT_DIR/Cargo.toml" build --release
  BIN="$PROJECT_DIR/target/release/hitmap"
fi

echo "Generating gallery with: $BIN"

for p in "${PROFILES[@]}"; do
  "$BIN" render --last 1y --theme light --color-profile "$p" --output "$GALLERY/${p}-light.png" "$LIGHT_REPO"
  echo "  light  $p"
  "$BIN" render --last 1y --theme dark --color-profile "$p" --output "$GALLERY/${p}-dark.png" "$DARK_REPO"
  echo "  dark   $p"
done

echo "Done — 16 images in $GALLERY"
