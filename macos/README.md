# Cursus for macOS

Native SwiftUI app on top of the shared Rust core (`cursus-core`) and the
official SimpleX chat core (`libsimplex`). Runs on **macOS 12 Monterey or
newer**, Intel and Apple Silicon.

```
SwiftUI app (macos/Cursus)        views, Keychain, notifications, Dock badge
        │  JSON commands/events + two PCM audio functions
Rust bridge (../bridge)           static library over cursus-core
        │
cursus-core  →  libsimplex.dylib  (Contents/Frameworks)
```

The bridge is deliberately small — `cursus_call` (one JSON request, one JSON
reply), `cursus_next_event`, `cursus_init`, `cursus_free` — so the Swift side
needs no generated bindings and builds with any Swift 5.5+ toolchain that has
the macOS 12 SDK (Xcode 13.1 – 14.2 run on Monterey). JSON shapes live in
`../bridge/src/dto.rs` and are mirrored by `Cursus/Sources/Cursus/Models.swift`.

## Build

You need Xcode **or** just the Command Line Tools (`xcode-select --install`),
Rust (<https://rustup.rs>) and CMake (`brew install cmake`) for bundled Opus.

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
profile editor, delivery receipts, notifications, Dock badge, lock, contact /
group info panel (⌘I: local name, favourite, mute, address, members, clear /
leave / delete).

Chat, group and channel settings — the profile page (⌘I) fills the detail
pane like the Linux contact / group page, with its header (Back / Esc,
favourite, mute, preferences):

- **Every chat:** favourite, notifications (all / mentions / muted), delivery
  receipts (default / on / off), delete messages on this Mac after…, lists.
- **Contacts:** chat preferences — disappearing messages (+ time), delete for
  everyone, reactions, voice, calls — with what the contact allows.
- **Groups / channels:** owners edit name, bio, welcome message and picture,
  switch the nine group features (+ disappearing time) and member review;
  admins invite contacts (with a role) and manage the group link (role, delete);
  members page: message a member, change role, accept, hide messages, block for
  everyone, remove (optionally with their messages). New channels from the
  New chat sheet.

The rules (who allows what, group features, the JSON sent back) are the
core's `cursus_core::preferences`, shared with Linux; the bridge only turns
them into JSON for Swift. `cargo test -p cursus-bridge --test abi -- --ignored`
exercises the group settings against the real core.

## Look

Cursus's design system (`docs/DESIGN_SYSTEM.md`) on native macOS chrome: the
colour tokens of `linux/src/theme.rs` (`Theme` in `Platform.swift`), Cursus's
own icons (`Resources/icons`, vector PDFs of `linux/assets/icons`), the nav
rail, pane headers, cards, ghost tiles, link box and bubbles
(`Views/Components.swift`). The window keeps a plain native title bar
(traffic lights + "Cursus"); each pane draws its Cursus header below it, as
on Linux. Split view, menus, sheets, pickers, alerts and context menus stay
native.

## Parity with Linux

Same features as the Linux app, implemented natively; shared rules live in
`cursus-core`.

| Batch | Linux features | macOS |
|---|---|---|
| A. Messages | forward, message info, select several, jump to quoted message, reveal secret text / "marked deleted", who reacted | Done |
| B. Settings | theme, text size, app lock after idle, privacy (protect IP, auto-download, blur media, clean tracking links, chat previews), address settings, Enter sends | Done |
| C. Chat flow | "New messages" divider (new arrivals, and opening a chat with unread), follow only your own sends, `:emoji:` suggestions + live expansion, ↑ edits your last message | Done |
| D. Media | voice recording (draft: listen, discard, send), waveforms, video posters + playback, photo / video gallery | Done |
| E. Groups | "Chat with admins" (members) and "Chats with members" (moderators: threads, join requests with accept / remove, delete thread), private chat from a member page | Done |
| F. Calls | Shared native WebRTC/Opus, incoming/outgoing audio calls, mute, history and notifications | Implemented; Mac device validation pending |
## Audio calls

The Mac build enables the bridge's `calls` feature. It uses the same call
controller, frame encryption and Opus PCM adapter as Android. AVAudioEngine
converts the selected system input/output devices to 48 kHz mono PCM; voice
processing is enabled when the device supports it. Microphone permission
is requested before starting or accepting a call. Audio settings opens the
system's device controls; no iOS-only AVAudioSession APIs are used.

Start from a direct chat header or Calls → New audio call. Incoming calls
have Answer/Decline controls and actionable notifications. Escape/Back
minimizes the call; Hang up ends it. Closing a chat window keeps the call
running. Closing/locking the account and quitting end it. Existing automatic
account-lock settings still apply. Calls disable idle system sleep and App
Nap while active. Video and system CallKit integration are not included.

The app self-test checks the PCM C ABI and arbitrary-buffer capture framing
without opening a microphone. CI builds Intel and Apple Silicon bundles and
runs the shared controller/PCM regressions. This Linux development machine
cannot compile Apple frameworks or validate microphone, echo cancellation,
notifications, device changes or interoperability on a Mac; those checks
remain necessary on macOS 12 and a current macOS release.

## Current macOS design

The same Cursus blue, light/dark surfaces, icon set and grouped preference
rows remain shared with Linux and Android. On a current SDK, the Mac app
uses NavigationSplitView, a unified compact toolbar, native switches and
an inset rounded destination rail. With a macOS 26+ SDK/compiler and runtime,
the rail uses system Liquid Glass. Message history and settings stay opaque
for readability. Reduce Transparency or Increase Contrast selects an opaque
bordered rail. Older SDKs/runtimes retain the same layout with a native
HSplitView and solid rail; both Intel and Apple Silicon use these same rules.

`build.sh` selects `CURSUS_MODERN_SHELL` and `CURSUS_GLASS` from the installed
SDK. To get the new glass appearance, select Xcode 26 or later with
`xcode-select` before building. This does not raise the minimum supported
runtime above macOS 12. CI builds both architectures with the macOS 26 SDK
and also compiles the fallback UI without these flags.
