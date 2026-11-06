#!/usr/bin/env bash
# Download official prebuilt libsimplex (same channel as simplex-chat Python package).
#
# Supports linux/macos × x86_64/aarch64. Override with:
#   SIMPLEX_LIBS_TAG=core-… ./scripts/fetch-libs.sh
#   SIMPLEX_LIBS_PLAT=linux-aarch64 ./scripts/fetch-libs.sh
set -euo pipefail

# Release tag on https://github.com/simplex-chat/simplex-chat-libs/releases
# Prefer `core-*` builds (raw libsimplex).
# core-7.1.0.17 = simplex-chat 1fd7ed8327d8481299215ca3240eb0140400924a
# (simplexmq d1f97d38f7a01f112efb6be753a1b45a40777826) — see licenses/NOTICE.md.
SIMPLEX_LIBS_TAG="${SIMPLEX_LIBS_TAG:-core-7.1.0.17}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/third_party/libsimplex"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
# Match SimpleX desktop scripts: ARCH="$(uname -m)" (aarch64 / x86_64 / arm64).
arch="$(uname -m)"

normalize_arch() {
  case "$1" in
    x86_64|amd64) printf 'x86_64' ;;
    aarch64|arm64|armv8l|armv8b|aarch64_be) printf 'aarch64' ;;
    *) printf '%s' "$1" ;;
  esac
}

arch_norm="$(normalize_arch "$arch")"

if [[ -n "${SIMPLEX_LIBS_PLAT:-}" ]]; then
  plat="$SIMPLEX_LIBS_PLAT"
else
  case "$os-$arch_norm" in
    linux-x86_64) plat="linux-x86_64" ;;
    linux-aarch64) plat="linux-aarch64" ;;
    darwin-x86_64) plat="macos-x86_64" ;;
    darwin-aarch64) plat="macos-aarch64" ;;
    *)
      echo "Unsupported platform: uname -s=$(uname -s) uname -m=$arch (normalized=$arch_norm)" >&2
      echo "Supported: linux/macos × x86_64|aarch64 (also: amd64, arm64, armv8l)." >&2
      echo "Override with SIMPLEX_LIBS_PLAT=linux-aarch64 if needed." >&2
      exit 1
      ;;
  esac
fi

echo "Host: uname -m=$arch → $arch_norm; platform zip=$plat"

# Official zips are glibc-linked. Warn early on musl (Alpine, many pmOS builds).
if [[ "$os" == "linux" ]]; then
  if [[ -e /lib/ld-musl-aarch64.so.1 || -e /lib/ld-musl-x86_64.so.1 || -e /usr/lib/ld-musl-aarch64.so.1 ]]; then
    echo "WARNING: musl detected. Official libsimplex is glibc-linked." >&2
    echo "  Install: apk add gcompat libucontext" >&2
    echo "  Then build Cursus on this device (musl builds include the glibc compat layer)." >&2
  elif [[ "$arch_norm" == "aarch64" && ! -e /lib/ld-linux-aarch64.so.1 && ! -e /lib64/ld-linux-aarch64.so.1 ]]; then
    echo "WARNING: /lib/ld-linux-aarch64.so.1 not found — glibc loader missing." >&2
  fi
fi

url="https://github.com/simplex-chat/simplex-chat-libs/releases/download/${SIMPLEX_LIBS_TAG}/simplex-chat-libs-${plat}.zip"
echo "Fetching $url"
if ! curl -fL "$url" -o "$TMP/libs.zip"; then
  echo "Download failed for $plat (tag ${SIMPLEX_LIBS_TAG})." >&2
  echo "Check assets at https://github.com/simplex-chat/simplex-chat-libs/releases/tag/${SIMPLEX_LIBS_TAG}" >&2
  exit 1
fi
unzip -q "$TMP/libs.zip" -d "$TMP/extract"

rm -rf "$OUT"
mkdir -p "$OUT"
# Zip layouts vary: sometimes `libs/`, sometimes files at the root.
if [[ -d "$TMP/extract/libs" ]]; then
  cp -a "$TMP/extract/libs/." "$OUT/"
elif [[ -f "$TMP/extract/libsimplex.so" || -f "$TMP/extract/libsimplex.dylib" ]]; then
  cp -a "$TMP/extract/." "$OUT/"
else
  # Nested single directory (parentheses required for find -o precedence).
  inner="$(find "$TMP/extract" -maxdepth 3 \( -name 'libsimplex.so' -o -name 'libsimplex.dylib' \) | head -n1)"
  if [[ -n "$inner" ]]; then
    cp -a "$(dirname "$inner")/." "$OUT/"
  else
    echo "Could not find libsimplex in zip; contents:" >&2
    find "$TMP/extract" | head -n 50 >&2
    exit 1
  fi
fi

lib=
if [[ -f "$OUT/libsimplex.so" ]]; then
  lib="$OUT/libsimplex.so"
elif [[ -f "$OUT/libsimplex.dylib" ]]; then
  lib="$OUT/libsimplex.dylib"
else
  echo "Install incomplete: no libsimplex shared library under $OUT" >&2
  ls -la "$OUT" | head -n 30 >&2
  exit 1
fi

# Refuse a wrong-arch binary (common when copying third_party from another machine).
if command -v file >/dev/null 2>&1; then
  info="$(file -b "$lib" || true)"
  echo "Installed binary: $info"
  case "$arch_norm" in
    aarch64)
      if ! grep -qiE 'aarch64|arm64|arm aarch64' <<<"$info"; then
        echo "ERROR: expected an aarch64 libsimplex but got:" >&2
        echo "  $info" >&2
        echo "Re-run on the ARM device (do not copy x86_64 third_party/libsimplex)." >&2
        exit 1
      fi
      ;;
    x86_64)
      if ! grep -qiE 'x86-64|x86_64|Intel 80386|X86-64' <<<"$info"; then
        echo "ERROR: expected an x86_64 libsimplex but got:" >&2
        echo "  $info" >&2
        exit 1
      fi
      ;;
  esac
fi

printf '%s\n' "$SIMPLEX_LIBS_TAG" > "$OUT/INSTALLED_TAG"
printf '%s\n' "$plat" > "$OUT/INSTALLED_PLAT"

echo "Installed libsimplex into $OUT"
ls -la "$OUT" | head -n 20
echo
echo "Pinned SimpleX libs tag: ${SIMPLEX_LIBS_TAG} ($plat)"
echo "Source correspondence: https://github.com/simplex-chat/simplex-chat (matching this libs release)"
