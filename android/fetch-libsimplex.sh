#!/usr/bin/env bash
# Official SimpleX chat core for Android (arm64-v8a), taken from the official
# SimpleX Chat APK of a pinned release and checked against its SHA-256.
#
# simplex-chat-libs (used by scripts/fetch-libs.sh for desktop) ships no
# Android build; the official APK carries the same C ABI in libsimplex.so
# (chat_migrate_init, chat_send_cmd, chat_recv_msg_wait, hs_init_with_rtsopts…),
# depending only on the system libc / libm / libdl.
#
#   ./android/fetch-libsimplex.sh
#   SIMPLEX_APK_TAG=v7.1.2 SIMPLEX_APK_SHA256=… ./android/fetch-libsimplex.sh
set -euo pipefail

TAG="${SIMPLEX_APK_TAG:-v7.1.0}"
SHA256="${SIMPLEX_APK_SHA256:-5fa06aaa2dbbd0a18fec11625177db2b793ee8689069b056d456bac743e04e11}"
ABI=arm64-v8a

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/app/src/main/jniLibs/$ABI"
STAMP="$OUT/.simplex-tag"
if [[ -f "$OUT/libsimplex.so" && "$(cat "$STAMP" 2>/dev/null)" == "$TAG" ]]; then
  echo "libsimplex $TAG already in $OUT"
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
URL="https://github.com/simplex-chat/simplex-chat/releases/download/$TAG/simplex-aarch64.apk"
echo "Downloading $URL"
curl -fL --retry 3 -o "$TMP/simplex.apk" "$URL"
echo "$SHA256  $TMP/simplex.apk" | sha256sum -c -
unzip -q -o "$TMP/simplex.apk" "lib/$ABI/libsimplex.so" -d "$TMP/x"
mkdir -p "$OUT"
install -m644 "$TMP/x/lib/$ABI/libsimplex.so" "$OUT/libsimplex.so"
echo "$TAG" > "$STAMP"
echo "libsimplex $TAG → $OUT ($(du -h "$OUT/libsimplex.so" | cut -f1))"
