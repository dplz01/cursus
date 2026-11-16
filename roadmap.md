# Cursus status

What each app can do today. Design: [`docs/DESIGN_SYSTEM.md`](docs/DESIGN_SYSTEM.md). Architecture: [`docs/architecture.md`](docs/architecture.md). Security: [`docs/SECURITY_DEV.md`](docs/SECURITY_DEV.md).

**Principle:** one product, one design language, one shared Rust brain (`cursus-core` + `libsimplex`), native apps.

Last updated: 2026-11-16.

## How to read this

| Mark | Meaning |
|---|---|
| **Works** | In the app before the 2026-11-16 round and not changed by it, or exercised by an automated test that runs the real code against libsimplex. Not re-checked by hand in that round. |
| **Test** | Written or changed in the 2026-11-16 round. It builds and the automated tests pass, but nobody has used it in the running app yet. Treat as unproven. |
| **No** | Not implemented. |

Automated coverage: `cargo test --workspace` (core live test: open, send text, send photo, encrypt, decrypt, reopen); `cargo test -p cursus-bridge --test abi -- --ignored` (the JSON API end to end: profile, groups, lists, timers, preferences, close and reopen). Nothing automated drives a real window, a phone, the network between two users, or a Mac.

## Features

| Feature | Linux | Android | macOS |
|---|---|---|---|
| **Account** | | | |
| Create profile, unlock with database password | Works | Works | Works |
| Remember password (keyring / Keystore / Keychain) | Works | Works | Works |
| App lock (idle / background), back-off after wrong passwords | Works | Test (back-off moved into the model) | Works |
| Reopen right after locking; profile name survives unlock | Test | Test | Test |
| **Connecting** | | | |
| One-time links: create, copy, name, delete, incognito | Works | Works | Works |
| Address: create, show, share on profile, auto-accept, delete | Test (reading auto-accept no longer creates an address) | Test | Test |
| Paste / open a link, connect (also incognito) | Works | Works | Works |
| Contact requests: accept / reject | Works | Works | Works |
| **Chat list** | | | |
| Chats and Channels, unread, favourite, mute, mark read / unread | Works | Works | Works |
| Clear / delete chat | Works | Works | Works |
| Lists ("Add to list") | Works | Test | Works |
| **Messages** | | | |
| Send, reply, edit, delete (for me / for everyone) | Works | Works | Works |
| Rich text, emoji shortcodes, secret text | Works | Works | Works |
| Reactions, who reacted (groups) | Works | Works | Works |
| Forward, message info | Works | Works | Works |
| Select several: delete, forward | Works | Test | Works |
| Reveal a "marked deleted" message | Works | Test | Works |
| Load earlier messages (beyond the newest page) | Test | Test | No (newest 120 only) |
| Delivery status ticks | Works | Works | Works |
| Search in a chat / in the list | No | No | No |
| Mentions, link previews, live messages, reports | No | No | No |
| **Files and media** | | | |
| Send a file | Test (now always copied / encrypted into `files/`) | Test | Test |
| Send a photo: re-encoded, no metadata, ≤ 255 KiB, real preview | Test | Test | Test |
| Send a photo or video in original quality | No (only as a non-image file type) | No | No |
| Send a video as a video message (poster, duration) | No (goes as a file) | No | No |
| Receive files, progress, unknown-server approval | Test (encryption now follows the setting) | Test (progress patched in place) | Test |
| Automatic download of small media, obeying both settings | Works | Test (bridge now follows the settings) | Test (Swift not compiled) |
| Sender's inline preview before download | No | Test | No |
| Photo viewer / gallery | Works | Works | Works |
| Video playback | Test (refused unless really a video) | External app only | Works |
| Voice messages: record, draft, send | Works | Works | Works |
| Voice messages: play, waveform | Test (playback refused unless really audio) | Works | Works |
| Save a file elsewhere | Works | Works | Works |
| Open a file with another app | Test (playlists and HTML now refused) | Test (same list) | Test (same list) |
| **Contacts and groups** | | | |
| Contact page: alias, preferences, receipts | Works | Works | Works |
| Disappearing messages; "Delete after (this device only)" | Works | Test (device-only timer is new) | Works |
| Create group / channel | Works | Works | Works |
| Group page: profile, features, member review, link | Works | Works | Works |
| Members: roles, remove, block, hide, accept, message | Works | Works | Works |
| Chat with admins / chats with members | Works | Works | Works |
| Delete a member's support chat | Works | Test | Works |
| Group member profile page | No | No | No |
| **Calls** | | | |
| Audio calls | Works | No | No |
| Call history (Calls page) | Works | No (page is empty) | No |
| Video calls | No | No | No |
| **Settings** | | | |
| Theme | Works | Works | Works |
| Text size | Works | Test | Works |
| Privacy switches (previews, blur, protect IP, auto-download, encrypt files, clean links, receipts) | Works | Works | Works |
| Hide notification text | Works | Test (now on by default, as on Linux) | Works |
| Tray, close to tray | Works | Not applicable | Not applicable |
| Receive in background | Works (tray) | Test (service now re-unlocks or stops after a restart) | Works |
| **Not in any app** | | | |
| Verify security code | No | No | No |
| Several profiles, hidden profiles | No | No | No |
| Custom SMP / XFTP servers, proxy / Tor settings | No | No | No |
| Change database password, export / import | No | No | No |
| iOS app | — | — | — |

## What to test first

Changed code that everything else depends on:

1. **Sending a photo and a file**, with "Encrypt stored files" on and off, to an official SimpleX client and back. Check the photo arrives with a preview, downloads by itself, and carries no EXIF.
2. **Receiving** an image, a voice message and a larger file, with "Download automatically" on and off and "Protect IP" on and off (Android and macOS go through new bridge code).
3. **Lock → unlock** quickly, and quit (the chat is now stopped before its database closes). Watch for a hang on quit on Linux.
4. **Long chats**: "Load earlier messages" several times; scrolling should stay where it was.
5. **Linux scrolling** in a busy group with pictures (the history cache key changed).
6. **musl phone build**: the compatibility library is now looked for in the data directory instead of `/tmp`; this edit was not compiled.
7. **Android**: kill the app from recents with "Receive in background" on, every new screen in the Android column above.
8. **macOS**: build; the download-settings sync in `Preferences.swift` / `CursusApp.swift` is new Swift that was never compiled.

## Known gaps, in rough order of importance

1. **Calls on Android and macOS.** `cursus-call` is shared but must be built for Android (its bundled libopus needs the NDK's CMake toolchain), given an audio device per platform, and a call state machine in the bridge — today that lives in `linux/src/call_flow.rs`.
2. **No way to send an original photo.** Official SimpleX offers two paths: pick from the gallery (compressed) or attach as a file (untouched). Cursus has one attach button.
3. **Video messages** are sent as plain files.
4. **Search.**
5. The rest of the "Not in any app" rows.

## Other limits

| Topic | Status |
|---|---|
| Channel creation | Needs enabled chat relays in the core's default servers |
| Building `libsimplex` | Not part of `cargo build`; use `scripts/fetch-libs.sh` |
| Android | arm64 only (SimpleX publishes its core for arm64-v8a) |
| Linux `main.rs` / `ui.rs` | About 20,000 lines in two files; not yet split up |
