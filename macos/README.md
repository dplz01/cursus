# Cursus for macOS

Native SwiftUI app on top of the shared Rust core (`cursus-core`) and the
official SimpleX chat core (`libsimplex`). Runs on **macOS 12 Monterey or
newer**, Intel and Apple Silicon.

```
SwiftUI app (macos/Cursus)        views, Keychain, notifications, Dock badge
        │  4 C functions, JSON in/out
Rust bridge (macos/bridge)        static library over cursus-core
        │
cursus-core  →  libsimplex.dylib  (Contents/Frameworks)
```

The bridge is deliberately small — `cursus_call` (one JSON request, one JSON
reply), `cursus_next_event`, `cursus_init`, `cursus_free` — so the Swift side
needs no generated bindings and builds with any Swift 5.5+ toolchain that has
the macOS 12 SDK (Xcode 13.1 – 14.2 run on Monterey). JSON shapes live in
`bridge/src/dto.rs` and are mirrored by `Cursus/Sources/Cursus/Models.swift`.

## Build

You need Xcode **or** just the Command Line Tools (`xcode-select --install`),
and Rust (<https://rustup.rs>).

```sh
./macos/build.sh          # → macos/build/Cursus.app (release, ad-hoc signed)
./macos/build.sh --run    # build and launch with logs in the terminal
```

The script fetches the official `libsimplex` for your Mac, builds the Rust
bridge and the SwiftUI app, assembles and signs `Cursus.app`, and finally runs
`Cursus --self-test`, which loads `libsimplex` and opens a throwaway database
— if that passes, the core works on this Mac.

Logs: `CURSUS_LOG=debug ./macos/build/Cursus.app/Contents/MacOS/Cursus`.

CI (`.github/workflows/macos.yml`) builds the app on Intel and Apple Silicon
runners and uploads `Cursus-macos-<arch>.zip`. Downloaded builds are not
notarized: right-click → Open the first time.

## Data

`~/Library/Application Support/chat.Cursus.cursus/` — the SQLCipher databases
(encrypted with the password chosen at first launch) and received files.
"Remember password" stores that password in the login Keychain.

## Status

Done: profile setup / unlock (Keychain), chats and channels lists, conversation
with formatting, replies, edits, deletes, reactions, images, voice playback,
files (download, open, save, drag-and-drop send), contact requests, group
invitations, one-time invites / address with QR, connect via link, new group,
profile editor, delivery receipts, notifications, Dock badge, lock.

Next: audio calls (the Linux WebRTC engine moves into the bridge), voice
recording, group management, chat tags.
