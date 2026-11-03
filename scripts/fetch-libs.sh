#!/usr/bin/env bash
# Download official prebuilt libsimplex (same channel as simplex-chat Python package).
set -euo pipefail

# Release tag on https://github.com/simplex-chat/simplex-chat-libs/releases
# Prefer `core-*` builds (raw libsimplex). Override with SIMPLEX_LIBS_TAG=...
SIMPLEX_LIBS_TAG="${SIMPLEX_LIBS_TAG:-core-7.1.0.17}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/third_party/libsimplex"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$os-$arch" in
  linux-x86_64|linux-amd64) plat="linux-x86_64" ;;
  linux-aarch64|linux-arm64) plat="linux-aarch64" ;;
  darwin-x86_64) plat="macos-x86_64" ;;
  darwin-arm64) plat="macos-aarch64" ;;
  *)
    echo "Unsupported platform: $os $arch" >&2
    exit 1
    ;;
esac

url="https://github.com/simplex-chat/simplex-chat-libs/releases/download/${SIMPLEX_LIBS_TAG}/simplex-chat-libs-${plat}.zip"
echo "Fetching $url"
curl -fL "$url" -o "$TMP/libs.zip"
unzip -q "$TMP/libs.zip" -d "$TMP/extract"

rm -rf "$OUT"
mkdir -p "$OUT"
# Zip layouts vary: sometimes `libs/`, sometimes files at the root.
if [[ -d "$TMP/extract/libs" ]]; then
  cp -a "$TMP/extract/libs/." "$OUT/"
elif [[ -f "$TMP/extract/libsimplex.so" || -f "$TMP/extract/libsimplex.dylib" ]]; then
  cp -a "$TMP/extract/." "$OUT/"
else
  # Nested single directory
  inner="$(find "$TMP/extract" -maxdepth 2 -name 'libsimplex.so' -o -name 'libsimplex.dylib' | head -n1)"
  if [[ -n "$inner" ]]; then
    cp -a "$(dirname "$inner")/." "$OUT/"
  else
    echo "Could not find libsimplex in zip; contents:" >&2
    find "$TMP/extract" | head -n 50 >&2
    exit 1
  fi
fi

echo "Installed libsimplex into $OUT"
ls -la "$OUT" | head -n 20
echo
echo "Pinned SimpleX libs tag: ${SIMPLEX_LIBS_TAG}"
echo "Source correspondence: https://github.com/simplex-chat/simplex-chat (matching this libs release)"
printf '%s\n' "$SIMPLEX_LIBS_TAG" > "$OUT/INSTALLED_TAG"
