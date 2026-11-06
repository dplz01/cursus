# Cursus

Cursus is a modern, open-source, cross-platform client for the [SimpleX](https://simplex.chat) messaging protocol.

It uses a **shared Rust core** on top of the official SimpleX chat library (`libsimplex`), and native UI layers per platform.

| Platform | UI |
|---|---|
| Linux | Rust + Iced |
| macOS | Rust + Iced (same desktop crate family) |
| Android | Kotlin + Jetpack Compose → UniFFI |
| iOS | Swift + SwiftUI → UniFFI |

## Status

**Milestone 1 (Linux foundation):** shared core + Iced Linux client — messaging, invites, attachments/voice/gallery, reactions, message menu, replies. See [`roadmap.md`](roadmap.md).

**Next (Milestone 2):** macOS shell sharing desktop modules. Mobile UniFFI shells and channels/calls are later milestones. Light/dark theming (System / Light / Dark) ships on Linux.

Other platforms are scaffolded but not implemented yet.

## Repository layout

```text
cursus/
├── core/                 # Shared Rust library (protocol session, models, services)
├── linux/                # Linux desktop application
├── macos/                # macOS application (later)
├── android/              # Android application (later)
├── ios/                  # iOS application (later)
├── docs/                 # Architecture and design
├── scripts/              # Tooling (fetch libsimplex, …)
└── third_party/libsimplex/  # Downloaded/built native libraries (not committed)
```

Each platform directory is meant to be extractable into its own GitHub repository later.

## Install

Prebuilt packages for **x86_64** and **ARM64** are on the [releases page](https://github.com/dplz01/cursus/releases/latest). `$(uname -m)` picks the right one for your device.

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
```

On musl (Alpine, postmarketOS) first run `apk add gcompat libucontext build-base`.

## Documentation

- [Architecture](docs/architecture.md) — SimpleX integration, crates, FFI, framework choices
- [Design system](docs/DESIGN_SYSTEM.md) — shared visual identity (one product, four native UIs)
- [Roadmap](roadmap.md) — milestones and known limitations

## License

Cursus application code: **AGPL-3.0-or-later** (aligned with SimpleX Chat).

`libsimplex` is AGPL; linking or distributing it requires complying with that license and pointing at the matching SimpleX source tag (see `SIMPLEX_LIBS_TAG` in `scripts/fetch-libs.sh`).
