#!/usr/bin/env bash
# Download Noto Sans + Noto Color Emoji into linux/assets/fonts for ARM/musl
# hosts that lack distro font packages. Run from repo root or scripts/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/linux/assets/fonts"
mkdir -p "$OUT"
TAG="${NOTO_RELEASE_TAG:-noto-monthly-release-2026.11.01}"
BASE="https://raw.githubusercontent.com/notofonts/notofonts.github.io/${TAG}/fonts"
# Upstream moved the CBDT color font out of fonts/ into 2D/fonts/.
EMOJI_URL="https://raw.githubusercontent.com/googlefonts/noto-emoji/main/2D/fonts/NotoColorEmoji.ttf"

fetch() {
  local url="$1" dest="$2"
  if [[ -f "$dest" && "${FORCE_FETCH:-}" != "1" ]]; then
    echo "keep $dest"
    return
  fi
  echo "fetch $url"
  curl -fL "$url" -o "$dest.tmp"
  mv "$dest.tmp" "$dest"
}

fetch "${BASE}/NotoSans/full/ttf/NotoSans-Regular.ttf" "$OUT/NotoSans-Regular.ttf"
fetch "${BASE}/NotoSans/full/ttf/NotoSans-Bold.ttf" "$OUT/NotoSans-Bold.ttf"
fetch "${BASE}/NotoSans/full/ttf/NotoSans-Italic.ttf" "$OUT/NotoSans-Italic.ttf"
# Color emoji — large; needed so hearts/reactions aren't dingbats.
fetch "$EMOJI_URL" "$OUT/NotoColorEmoji.ttf"
echo "Fonts ready in $OUT"
ls -lh "$OUT"
