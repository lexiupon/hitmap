#!/usr/bin/env bash
# Regenerate the text renderer gallery images and markdown page.
# Usage: ./scripts/generate-text-gallery.sh [hitmap_binary]
#   If no binary is specified, uses the current project's release build.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

BIN="${1:-$PROJECT_DIR/target/release/hitmap}"
TEXT_CAPTURE="$SCRIPT_DIR/capture-text-render.py"
GALLERY_DIR="$PROJECT_DIR/docs/images/text-gallery"
GALLERY_MD="$PROJECT_DIR/docs/TEXT_GALLERY.md"
LIGHT_REPO="$HOME/projects/bi-and-analytics"
DARK_REPO="$HOME/projects/chronova"
LAST="1y"
TEXT_COLUMNS="${HITMAP_TEXT_GALLERY_COLUMNS:-120}"

PROFILES=("github" "aurora" "ocean" "fire" "catppuccin-latte" "catppuccin-frappe" "catppuccin-macchiato" "catppuccin-mocha")

profile_title() {
  case "$1" in
    github) echo "GitHub" ;;
    aurora) echo "Aurora" ;;
    ocean) echo "Ocean" ;;
    fire) echo "Fire" ;;
    catppuccin-latte) echo "Catppuccin Latte" ;;
    catppuccin-frappe) echo "Catppuccin Frappé" ;;
    catppuccin-macchiato) echo "Catppuccin Macchiato" ;;
    catppuccin-mocha) echo "Catppuccin Mocha" ;;
    *) echo "$1" ;;
  esac
}

profile_description() {
  case "$1" in
    github) echo "Classic GitHub greens — high contrast, familiar." ;;
    aurora) echo "Pale yellow → warm yellow → green-yellow → deep green." ;;
    ocean) echo "Subtle teal to deep blue." ;;
    fire) echo "Warm red to deep crimson." ;;
    catppuccin-latte) echo "Official Catppuccin Latte palette — vibrant, pastel-toned." ;;
    catppuccin-frappe) echo "Official Catppuccin Frappé palette — muted, soft tones." ;;
    catppuccin-macchiato) echo "Official Catppuccin Macchiato palette — cool, slightly desaturated." ;;
    catppuccin-mocha) echo "Official Catppuccin Mocha palette — the original, most vibrant Catppuccin." ;;
    *) echo "" ;;
  esac
}

write_markdown() {
  cat > "$GALLERY_MD" <<'EOF'
# Text Renderer Gallery

Each profile is shown in **light** and **dark** themes, captured from representative commit histories with varied activity so palette differences remain visible in text mode.

---
EOF

  for profile in "${PROFILES[@]}"; do
    title="$(profile_title "$profile")"
    description="$(profile_description "$profile")"
    cat >> "$GALLERY_MD" <<EOF

## $title

$description

| Light | Dark |
| :---: | :---: |
| <img src="images/text-gallery/${profile}-light.png" alt="$title light text renderer"> | <img src="images/text-gallery/${profile}-dark.png" alt="$title dark text renderer"> |
EOF
  done
}

mkdir -p "$GALLERY_DIR"

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

for repo in "$LIGHT_REPO" "$DARK_REPO"; do
  if [ ! -d "$repo/.git" ]; then
    echo "Repository path does not look like a git repository: $repo" >&2
    exit 1
  fi
done

echo "Generating text gallery with: $BIN"
for profile in "${PROFILES[@]}"; do
  python3 "$TEXT_CAPTURE" \
    --bin "$BIN" \
    --repo "$LIGHT_REPO" \
    --output "$GALLERY_DIR/${profile}-light.png" \
    --last "$LAST" \
    --theme light \
    --color-profile "$profile" \
    --columns "$TEXT_COLUMNS"
  echo "  light  $profile"

  python3 "$TEXT_CAPTURE" \
    --bin "$BIN" \
    --repo "$DARK_REPO" \
    --output "$GALLERY_DIR/${profile}-dark.png" \
    --last "$LAST" \
    --theme dark \
    --color-profile "$profile" \
    --columns "$TEXT_COLUMNS"
  echo "  dark   $profile"
done

write_markdown

echo "Done — 16 images in $GALLERY_DIR"
echo "Updated $GALLERY_MD"
