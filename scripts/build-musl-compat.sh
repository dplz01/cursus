#!/usr/bin/env bash
# Build libcursus_musl_glibc_compat.so into third_party/libsimplex/.
# Used with: cargo run/build/install -p cursus-linux --features musl-glibc-compat
# Run on the musl device (Alpine / postmarketOS). Do not run on glibc hosts
# (produces a stub without fcntl64 — do not copy that to the phone).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/core/native/musl_glibc_compat.c"
OUT="$ROOT/third_party/libsimplex/libcursus_musl_glibc_compat.so"
CC="${CC:-cc}"

mkdir -p "$(dirname "$OUT")"

if [[ -e /lib/ld-musl-aarch64.so.1 || -e /usr/lib/ld-musl-aarch64.so.1 \
   || -e /lib/ld-musl-x86_64.so.1 || -e /usr/lib/ld-musl-x86_64.so.1 ]]; then
  IMPL=1
else
  echo "WARNING: not a musl host — shim will be empty (no fcntl64)." >&2
  IMPL=0
fi

"$CC" -shared -fPIC -O2 -Wall "-DCURSUS_MUSL_SHIM_IMPL=$IMPL" -o "$OUT" "$SRC"
echo "wrote $OUT"

if command -v nm >/dev/null 2>&1; then
  if nm -D --defined-only "$OUT" | grep -q 'fcntl64$'; then
    echo "ok: exports fcntl64"
  else
    echo "ERROR: $OUT does not export fcntl64 (wrong host or build)." >&2
    exit 1
  fi
fi
