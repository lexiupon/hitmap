#!/usr/bin/env bash
# Reproduce the README preview images in docs/images/.
#
# Usage:
#   ./scripts/capture-readme-previews.sh [--repo PATH] [--bin PATH] [--out-dir PATH]
#
# Examples:
#   ./scripts/capture-readme-previews.sh --repo ~/projects/chronova
#   ./scripts/capture-readme-previews.sh --bin ./target/release/hitmap --repo .
#
# Optional environment variables for text preview tuning:
#   HITMAP_TEXT_FONT=/path/to/font.ttf
#   HITMAP_TEXT_FONT_SIZE=18
#   HITMAP_TEXT_PADDING_X=24
#   HITMAP_TEXT_PADDING_Y=20

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

BIN="${PROJECT_DIR}/target/release/hitmap"
REPO="$PROJECT_DIR"
OUT_DIR="$PROJECT_DIR/docs/images"
LAST="1y"
THEME="light"
COLOR_PROFILE="github"
TEXT_COLUMNS="120"
TEXT_CAPTURE="$SCRIPT_DIR/capture-text-render.py"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Rebuild the README preview images:
  - $PROJECT_DIR/docs/images/hitmap-kitty.png
  - $PROJECT_DIR/docs/images/hitmap-text.png

Options:
  --bin PATH            hitmap binary to use
  --repo PATH           repository to render (default: $PROJECT_DIR)
  --out-dir PATH        output directory (default: $OUT_DIR)
  --last RANGE          rolling window for both previews (default: $LAST)
  --theme THEME         theme to render (default: $THEME)
  --color-profile NAME  color profile to render (default: $COLOR_PROFILE)
  --text-columns N      width used for the text preview image (default: $TEXT_COLUMNS)
  -h, --help            show this help

Text preview rendering can be adjusted with:
  HITMAP_TEXT_FONT, HITMAP_TEXT_FONT_SIZE, HITMAP_TEXT_PADDING_X, HITMAP_TEXT_PADDING_Y
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --bin)
      BIN="$2"
      shift 2
      ;;
    --repo)
      REPO="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --last)
      LAST="$2"
      shift 2
      ;;
    --theme)
      THEME="$2"
      shift 2
      ;;
    --color-profile)
      COLOR_PROFILE="$2"
      shift 2
      ;;
    --text-columns)
      TEXT_COLUMNS="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

if [ ! -f "$TEXT_CAPTURE" ]; then
  echo "Missing helper script: $TEXT_CAPTURE" >&2
  exit 1
fi

if [ ! -x "$BIN" ]; then
  echo "Building release binary..."
  cargo --manifest-path "$PROJECT_DIR/Cargo.toml" build --release
  BIN="$PROJECT_DIR/target/release/hitmap"
fi

if [ ! -d "$REPO/.git" ]; then
  echo "Repository path does not look like a git repository: $REPO" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
KITTY_OUT="$OUT_DIR/hitmap-kitty.png"
TEXT_OUT="$OUT_DIR/hitmap-text.png"

echo "Generating kitty preview..."
"$BIN" render \
  --last "$LAST" \
  --theme "$THEME" \
  --color-profile "$COLOR_PROFILE" \
  --output "$KITTY_OUT" \
  "$REPO"

echo "Generating text preview..."
python3 "$TEXT_CAPTURE" \
  --bin "$BIN" \
  --repo "$REPO" \
  --output "$TEXT_OUT" \
  --last "$LAST" \
  --theme "$THEME" \
  --color-profile "$COLOR_PROFILE" \
  --columns "$TEXT_COLUMNS"

echo "Done:"
echo "  $KITTY_OUT"
echo "  $TEXT_OUT"
