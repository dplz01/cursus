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

## Prerequisites (Linux milestone)

- Rust stable (1.88+ recommended)
- Official SimpleX native library for your OS/arch

Fetch prebuilt libraries (same source as the official Python package). The script picks the zip from `uname -m` (`x86_64` or `aarch64` / `arm64`):

```bash
./scripts/fetch-libs.sh
# ARM example: uname -m → aarch64 → simplex-chat-libs-linux-aarch64.zip
```

Or build from [simplex-chat](https://github.com/simplex-chat/simplex-chat) with `scripts/desktop/build-lib-linux.sh` and copy `libs/linux-x86_64/` or `libs/linux-aarch64/` into `third_party/libsimplex/`. Do not copy an x86_64 `third_party/libsimplex` tree onto an ARM device.

## Build and run (Linux)

```bash
./scripts/fetch-libs.sh
cargo run -p cursus-linux
```

Works on **x86_64 and aarch64** Linux hosts (PinePhone, ARM laptops, Asahi, etc.) once the matching libs zip is installed.

**Musl (Alpine / musl postmarketOS):** prebuilt `libsimplex` is glibc-linked. Cursus builds a small `libcursus_musl_glibc_compat.so` shim (fcntl64, stat64, fortify wrappers) and preloads it with `gcompat` before `dlopen`. Install:

```bash
apk add gcompat libucontext
cargo build -p cursus-linux   # rebuilds the shim
cargo run -p cursus-linux
```

Still best-effort — if a new `symbol not found` appears, report it so the shim can grow.

Data is stored under `$XDG_DATA_HOME/cursus` (or `~/.local/share/cursus`), separate from the official SimpleX desktop app.

## Documentation

- [Architecture](docs/architecture.md) — SimpleX integration, crates, FFI, framework choices
- [Design system](docs/DESIGN_SYSTEM.md) — shared visual identity (one product, four native UIs)
- [Roadmap](roadmap.md) — milestones and known limitations

## License

Cursus application code: **AGPL-3.0-or-later** (aligned with SimpleX Chat).

`libsimplex` is AGPL; linking or distributing it requires complying with that license and pointing at the matching SimpleX source tag (see `SIMPLEX_LIBS_TAG` in `scripts/fetch-libs.sh`).
