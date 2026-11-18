# Cursus

Cursus is a modern, open-source, cross-platform client for the [SimpleX](https://simplex.chat) messaging protocol.

It uses a **shared Rust core** on top of the official SimpleX chat library (`libsimplex`), and native UI layers per platform.

| Platform | UI |
|---|---|
| Linux | Rust + Iced |
| macOS | Swift + SwiftUI → shared Rust bridge |
| Android | Kotlin + Jetpack Compose → JNI → shared Rust bridge |
| iOS | Planned Swift + SwiftUI shell |

## Status

Linux and Android support messaging, groups, channels, attachments, voice messages,
reactions, profiles and settings, with layouts for phones and larger screens.
Audio calls work on Linux and Android through the shared native WebRTC engine.
The macOS app now shares that controller, with native audio and call UI pending Mac device validation.
macOS has an experimental native SwiftUI client; iOS remains a scaffold.

The v0.1.2 checkpoint includes an Android preview APK. See the
[release notes](docs/releases/v0.1.2.md) and [roadmap](roadmap.md).

## Repository layout

```text
cursus/
├── core/                 # Shared Rust library (protocol session, models, services)
├── linux/                # Linux desktop application
├── macos/                # macOS SwiftUI application
├── android/              # Android Compose application
├── ios/                  # iOS application (later)
├── docs/                 # Architecture and design
├── scripts/              # Tooling (fetch libsimplex, …)
└── third_party/libsimplex/  # Downloaded/built native libraries (not committed)
```

Each platform directory is meant to be extractable into its own GitHub repository later.

## Install

Prebuilt packages for **x86_64** and **ARM64** are on the [releases page](https://github.com/dplz01/cursus/releases/latest). `$(uname -m)` picks the right one for your device.

### Android

Download `cursus-android-arm64-v8a.apk` from the release page and install it
on Android 8.0 or newer (64-bit ARM). This preview is debug-signed; see the
[Android build and signing notes](android/README.md).

### Flatpak

Any distro, glibc or musl (postmarketOS included):

```sh
curl -LO https://github.com/dplz01/cursus/releases/latest/download/cursus-$(uname -m).flatpak
flatpak install --user cursus-$(uname -m).flatpak
```

On Hyprland, let opened files float (gives Cursus access to the compositor):
`flatpak override --user --filesystem=xdg-run/hypr:ro chat.cursus.Cursus`

### AppImage

glibc distros:

```sh
curl -LO https://github.com/dplz01/cursus/releases/latest/download/Cursus-$(uname -m).AppImage
chmod +x Cursus-$(uname -m).AppImage
./Cursus-$(uname -m).AppImage
```

The first start adds Cursus to your app launcher (`--uninstall-desktop` removes it).

## Build from source

```sh
git clone https://github.com/dplz01/cursus && cd cursus
```

### Flatpak

```sh
flatpak install --user flathub org.flatpak.Builder
flatpak run org.flatpak.Builder --user --install --install-deps-from=flathub \
  --force-clean build-dir flatpak/chat.cursus.Cursus.yml
```

To install on another device of the same architecture, export a bundle instead:

```sh
flatpak run org.flatpak.Builder --repo=repo --force-clean build-dir flatpak/chat.cursus.Cursus.yml
flatpak build-bundle repo cursus.flatpak chat.cursus.Cursus
flatpak install --user cursus.flatpak
```

### Cargo

```sh
./scripts/fetch-libs.sh     # official libsimplex for this architecture
./scripts/fetch-fonts.sh
cargo install --path linux  # or: cargo run -p cursus-linux
cursus --install-desktop    # add to the app launcher (also done on first start)
```

On musl (Alpine, postmarketOS) first run `apk add gcompat libucontext build-base`.

## Documentation

- [Architecture](docs/architecture.md) — SimpleX integration, crates, FFI, framework choices
- [Design system](docs/DESIGN_SYSTEM.md) — shared visual identity (one product, four native UIs)
- [Roadmap](roadmap.md) — milestones and known limitations

## License

Cursus is **AGPL-3.0-or-later** (like SimpleX Chat). Releases bundle the official SimpleX core (`libsimplex`, AGPL-3.0, built from simplex-chat [`1fd7ed8`](https://github.com/simplex-chat/simplex-chat/tree/1fd7ed8327d8481299215ca3240eb0140400924a)), Rust crates, Noto fonts (OFL 1.1) and icons under their own licenses — see [`licenses/NOTICE.md`](licenses/NOTICE.md).
