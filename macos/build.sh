#!/usr/bin/env bash
# Build Cursus.app for this Mac: Rust core bridge + SwiftUI app + official
# libsimplex, assembled into macos/build/Cursus.app and ad-hoc signed.
#
# Needs: Xcode or the Command Line Tools (Swift 5.7+, i.e. Xcode 14.2 on
# macOS 12), Rust (rustup), curl, unzip. No Xcode project involved.
#
#   ./macos/build.sh             # release build
#   CONFIG=debug ./macos/build.sh
#   ./macos/build.sh --run       # build, then launch with logs in the terminal
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAC="$ROOT/macos"
CONFIG="${CONFIG:-release}"
OUT="$MAC/build"
APP="$OUT/Cursus.app"
RUN=0
[[ "${1:-}" == "--run" ]] && RUN=1

# Oldest supported macOS; the Rust side and the Swift side must agree.
export MACOSX_DEPLOYMENT_TARGET=12.0

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
need() {
  command -v "$1" >/dev/null 2>&1 || { echo "Missing '$1'. $2" >&2; exit 1; }
}

[[ "$(uname -s)" == "Darwin" ]] || { echo "Run this on a Mac." >&2; exit 1; }
need swift "Install Xcode or the Command Line Tools: xcode-select --install"
need cargo "Install Rust: https://rustup.rs (then open a new terminal)"
need curl ""
need unzip ""

SDK="$(xcrun --sdk macosx --show-sdk-version 2>/dev/null || echo 0)"
if [[ "${SDK%%.*}" -lt 12 ]]; then
  echo "The selected macOS SDK ($SDK) is too old; Cursus needs the macOS 12 SDK." >&2
  echo "Install Xcode 13.1+ or the Command Line Tools (xcode-select --install)," >&2
  echo "then select it: sudo xcode-select -s /Library/Developer/CommandLineTools" >&2
  exit 1
fi

case "$(uname -m)" in
  x86_64) TRIPLE=x86_64-apple-darwin; PLAT=macos-x86_64 ;;
  arm64) TRIPLE=aarch64-apple-darwin; PLAT=macos-aarch64 ;;
  *) echo "Unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac

say "Swift $(swift --version 2>/dev/null | head -1 | sed 's/.*Swift version \([0-9.]*\).*/\1/'), $(rustc --version), $TRIPLE"

# 1. Official SimpleX core for this Mac (re-fetched if third_party/ holds
#    another platform's build, e.g. after copying the repo from Linux).
LIBS="$ROOT/third_party/libsimplex"
if [[ "$(cat "$LIBS/INSTALLED_PLAT" 2>/dev/null)" != "$PLAT" || ! -f "$LIBS/libsimplex.dylib" ]]; then
  say "Fetching libsimplex ($PLAT)"
  SIMPLEX_LIBS_PLAT="$PLAT" "$ROOT/scripts/fetch-libs.sh"
fi

# 2. Rust bridge (static library).
say "Building the Rust core bridge"
CARGO_FLAGS=(--target "$TRIPLE" -p cursus-bridge --locked)
[[ "$CONFIG" == "release" ]] && CARGO_FLAGS+=(--release)
(cd "$ROOT" && cargo build "${CARGO_FLAGS[@]}")
RUST_LIB_DIR="$ROOT/target/$TRIPLE/$CONFIG"
[[ -f "$RUST_LIB_DIR/libcursus_bridge.a" ]] || { echo "Missing $RUST_LIB_DIR/libcursus_bridge.a" >&2; exit 1; }

# 3. SwiftUI app.
say "Building the SwiftUI app"
SWIFT_FLAGS=(-c "$CONFIG" --package-path "$MAC/Cursus" -Xlinker -L -Xlinker "$RUST_LIB_DIR")
swift build "${SWIFT_FLAGS[@]}"
BIN_DIR="$(swift build "${SWIFT_FLAGS[@]}" --show-bin-path)"

# 4. Bundle.
say "Assembling Cursus.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Frameworks" "$APP/Contents/Resources/licenses"
cp "$BIN_DIR/Cursus" "$APP/Contents/MacOS/Cursus"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
sed -e "s/@VERSION@/$VERSION/" -e "s/@BUILD@/$BUILD/" "$MAC/Resources/Info.plist" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# libsimplex + its Haskell runtime libraries (found via @loader_path).
cp -a "$LIBS"/*.dylib "$APP/Contents/Frameworks/"

# Icon from the shared logo PNGs (iconutil naming: icon_<pt>x<pt>[@2x].png).
ICONSET="$OUT/AppIcon.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
LOGO="$ROOT/linux/assets/logo"
for spec in 16x16:16 16x16@2x:32 32x32:32 32x32@2x:64 128x128:128 128x128@2x:256 \
            256x256:256 256x256@2x:512 512x512:512; do
  cp "$LOGO/cursus-${spec#*:}.png" "$ICONSET/icon_${spec%%:*}.png"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns" || echo "warning: iconutil failed; no app icon" >&2
rm -rf "$ICONSET"

# Cursus's own icons (vector PDF templates of linux/assets/icons).
mkdir -p "$APP/Contents/Resources/icons"
cp "$MAC/Resources/icons/"*.pdf "$APP/Contents/Resources/icons/"

# License notices travel with the binary (AGPL + statically linked crates,
# icon credits).
cp "$ROOT/LICENSE" "$APP/Contents/Resources/licenses/"
cp "$ROOT"/licenses/* "$APP/Contents/Resources/licenses/"
cp "$ROOT/linux/assets/icons/ATTRIBUTION.md" "$APP/Contents/Resources/licenses/ICONS-ATTRIBUTION.md"

# 5. Ad-hoc signature (required to run at all on Apple Silicon; harmless on Intel).
say "Signing (ad-hoc)"
find "$APP/Contents/Frameworks" -name '*.dylib' -exec codesign --force --sign - --timestamp=none {} +
codesign --force --sign - --timestamp=none "$APP"

say "Self-test: loading libsimplex"
"$APP/Contents/MacOS/Cursus" --self-test

say "Done: $APP"
echo "Open it with:  open \"$APP\"   (or drag it to /Applications)"

if [[ "$RUN" == 1 ]]; then
  exec "$APP/Contents/MacOS/Cursus"
fi
