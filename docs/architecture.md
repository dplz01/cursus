# Cursus architecture

## 1. How Cursus talks to SimpleX

Cursus does **not** reimplement SMP, the double ratchet, or SimpleX Chat command processing.

The official chat core is distributed as a native shared library (`libsimplex.so` / `.dylib` / `.dll`), built from [simplex-chat](https://github.com/simplex-chat/simplex-chat). That library already links [simplexmq](https://github.com/simplex-chat/simplexmq) (SMP agent) and owns:

- SQLCipher chat + agent databases
- Contact / group / message protocol
- Queue subscriptions to SimpleX relays
- File transfer (inline / XFTP)

Third-party clients (official Python, Node.js, iOS, Android/desktop via JNI) all use the same small C ABI exported from `src/Simplex/Chat/Mobile.hs`:

| C function | Role |
|---|---|
| `hs_init_with_rtsopts` | Start the Haskell runtime once |
| `chat_migrate_init` | Open/migrate DB, return opaque `ctrl` |
| `chat_send_cmd` / `chat_send_cmd_retry` | Send a **command string**, get **JSON** back on the same call |
| `chat_recv_msg_wait` | Block for async **events** (incoming messages, status updates) |
| `chat_close_store` | Close databases |

Commands are the same text language documented in SimpleX [`bots/api/COMMANDS.md`](https://github.com/simplex-chat/simplex-chat/blob/stable/bots/api/COMMANDS.md) (examples: `/_start main=on`, `/_send @5 …`). Responses and events are JSON with a `result` or `error` envelope.

```text
┌─────────────────────────────┐
│  Platform UI                │  Iced / Compose / SwiftUI
│  (linux, macos, …)          │
└──────────────┬──────────────┘
               │ Rust API (models + async services)
┌──────────────▼──────────────┐
│  cursus-core                │  session, state, command builders
└──────────────┬──────────────┘
               │ C FFI (libloading)
┌──────────────▼──────────────┐
│  libsimplex (+ GHC runtime) │  official SimpleX Chat core
└──────────────┬──────────────┘
               │ SMP / XFTP
┌──────────────▼──────────────┐
│  Default SimpleX relays     │
└─────────────────────────────┘
```

**Trust model for `libsimplex`:** the Cursus repo does not vendor Haskell sources. `scripts/fetch-libs.sh` downloads a version-pinned zip from [simplex-chat-libs](https://github.com/simplex-chat/simplex-chat-libs/releases) (currently the `core-*` tags that ship raw `libsimplex` for `linux-x86_64`, `linux-aarch64`, macOS, Windows). Anyone can instead build from SimpleX source with `scripts/desktop/build-lib-linux.sh` and copy `libs/linux-x86_64/` or `libs/linux-aarch64/` into `third_party/libsimplex/`. The pinned tag files `third_party/libsimplex/INSTALLED_TAG` and `INSTALLED_PLAT` are the verification path—not “trust a binary we uploaded once.”

Default servers and default chat options are used; no custom relay UI in milestone 1.

## 2. What lives in Rust vs platform code

### Shared Rust (`cursus-core`)

- Load and initialize `libsimplex`
- Session lifecycle (migrate DB, start chat, receive loop)
- Command string builders and JSON decoding into domain models
- In-memory application state (users, chats, open conversation, send/receive errors)
- File attachment helpers that call into core file APIs when needed
- Chat rules every app shares — e.g. `preferences` (what you and a contact
  allow, group features, the JSON sent back). Apps keep only how these are
  shown (labels, menus); never copy such a rule into an app or a bridge
- Future: UniFFI surface for Android/iOS

### Platform-specific

| Concern | Owner |
|---|---|
| Windows, widgets, navigation chrome | `linux` / `macos` / `android` / `ios` |
| Notifications, tray, file pickers | platform |
| Permissions, background execution | Android / iOS native APIs |
| OS menus, shortcuts, window state | desktop crates |
| Visual tokens applied to widgets | each UI toolkit, following `docs/DESIGN_SYSTEM.md` |

`cursus-core` must never depend on Iced, Compose, or SwiftUI.

## 3. Four applications sharing code

```text
cursus-core  ←── linux (Iced, links core as Rust crate)
             ←── macos (Iced, shares most UI modules with linux)
             ←── android (Kotlin UI + UniFFI bindings to core)
             ←── ios (SwiftUI + UniFFI bindings to core)
```

Desktop apps depend on `cursus-core` as a normal Cargo path dependency. Mobile apps consume a UniFFI-generated Kotlin/Swift API over the same crate (milestone after Linux works). UniFFI is the planned interoperability layer; it is not required for the Linux milestone.

Each top-level platform directory (`linux/`, `macos/`, `android/`, `ios/`) stays independently understandable so it can become its own GitHub repository later.

## 4. Library and framework choices

### SimpleX integration

| Choice | Why |
|---|---|
| Official `libsimplex` via C ABI | Correct protocol stack; matches Python/Node/iOS clients |
| Runtime load with `libloading` | No link-time GHC dependency; same pattern as Python `ctypes` |
| Pin `SIMPLEX_LIBS_VERSION` | Reproducible builds; AGPL source correspondence |

Rejected: reimplementing SMP in Rust; embedding the full SimpleX git tree as a submodule for every build; linking `libapp-lib.so` (JVM/JNI shim only).

### Desktop GUI: Iced (not Slint)

Evaluated **Iced** and **Slint** for Linux + macOS.

| Criterion | Iced | Slint |
|---|---|---|
| License | MIT | GPL-3 / royalty-free / commercial |
| Language | Pure Rust (Elm architecture) | `.slint` DSL + Rust |
| Linux + macOS | Yes | Yes |
| Accessibility | Weaker historically; improving | Stronger AccessKit integration |
| Designer tooling | Code-first | Visual editor / live preview |
| Fit for OSS messenger | Simple dependency story | License dual-track needs care for AGPL app |

**Decision for milestone 1: Iced.** Reasons:

1. Pure Rust keeps the desktop stack one language and one ownership model.
2. MIT avoids mixing Slint’s royalty-free/GPL options into an AGPL client early.
3. Elm-style `Model / Message / update` maps cleanly onto chat events from `cursus-core`.
4. Same framework can be shared between `linux/` and `macos/` with thin OS integration modules.

Revisit Slint if accessibility or designer workflow becomes a blocker. Do not pick a GUI only because it markets “more platforms.”

### Mobile (later)

- Android: Kotlin + Jetpack Compose (native)
- iOS: Swift + SwiftUI (native)
- Bridge: UniFFI from `cursus-core` (investigate `uniffi-rs`; fall back to `cbindgen` + thin C header only if UniFFI blocks)

### Other Rust crates (keep minimal)

- `serde` / `serde_json` — decode SimpleX JSON
- `tokio` — receive loop + command dispatch without blocking the UI thread
- `libloading` — load `libsimplex`
- `thiserror` / `anyhow` — errors (prefer `thiserror` in core)
- `directories` — XDG paths on Linux

Avoid large “framework” crates for state management beyond what Iced already provides.

## 5. Repository organization

```text
cursus/
├── Cargo.toml              # workspace
├── core/                   # cursus-core
├── linux/                  # cursus-linux (Iced)
├── macos/                  # placeholder → later Iced desktop
├── android/                # placeholder → Compose + UniFFI
├── ios/                    # placeholder → SwiftUI + UniFFI
├── docs/
│   ├── architecture.md     # this file
│   └── DESIGN_SYSTEM.md
├── roadmap.md
├── scripts/fetch-libs.sh
└── third_party/libsimplex/ # gitignored binaries
```

### FFI boundaries (summary)

1. **UI → core:** Rust function calls / UniFFI methods (`start`, `list_chats`, `send_text`, subscribe to `Event`).
2. **core → libsimplex:** C ABI only; all strings UTF-8; every returned `char*` is `malloc`’d and must be `free`’d.
3. **libsimplex → network:** owned entirely by SimpleX; Cursus never opens SMP sockets itself.

### Threading

- One dedicated thread (or Tokio task) blocks in `chat_recv_msg_wait`.
- UI thread never calls into `libsimplex` directly; it sends requests to the session actor and applies events.
- `hs_init_with_rtsopts` runs exactly once per process, before any `chat_*` call.

### Data directories

| App | Data root |
|---|---|
| Official SimpleX desktop | `~/.local/share/simplex` |
| Cursus | `~/.local/share/cursus` |

Layout under the Cursus data root:

| Path | Role |
|---|---|
| `simplex_v1_{chat,agent}.db` | SQLCipher databases |
| `files/` | Received (and some sent) attachment bytes via `/files_folder` |
| `tmp/` | Process temp (`TMPDIR` / `/_temp_folder`) for XFTP decrypt + chat temp |

Never share the official app’s lock file or databases.

### XFTP receive + `TMPDIR` (development note)

**Symptom we hit:** UI showed “transferring…” / “Saving…”, while `~/.local/share/cursus/files/<name>` stayed **0 bytes**. The agent had already decrypted the payload under `/tmp/…_rcv.xftp/xftp.decrypted`.

**Root cause:** SimpleX chat completes an XFTP receive with `renameFile` from the process temporary directory into the app files folder. On many Linux desktops `/tmp` is **tmpfs** and the home data dir is on disk, so `rename(2)` fails with **EXDEV** (cross-device). The core left the empty placeholder created at accept time and never copied the decrypted blob across.

**Practical Cursus fix (preferred over post-receive scavengers):**

1. Create `~/.local/share/cursus/tmp` on the **same filesystem** as `files/`.
2. Before opening `libsimplex`, set `TMPDIR` / `TMP` / `TEMP` to that directory so the agent’s `getTemporaryDirectory()` stays on-disk next to `files/`.
3. After `/_start`, also send `/_temp_folder <tmp>` and `/files_folder <files>` so chat-layer paths match official clients.
4. Accept receives with `/freceive <id> approved_relays=on encrypt=off` (no save path) so the core uses the configured files folder.

**Rejected as too fragile / hacky:**

- Polling file size after `freceive` returns “accepted” and treating accept as complete.
- Scanning leftover `*_rcv.xftp` dirs under system `/tmp` and copying into `files/` if the destination is still empty.

A proper upstream fix is `rename` with copy+delete fallback on EXDEV (patched in the local SimpleX tree as `moveFileAcrossDevices`); that only helps Cursus once `libsimplex` is rebuilt from that tree. Until then, the `TMPDIR` pin is the shippable client-side mitigation.

#### Security analysis of the `TMPDIR` pin

| Risk | Severity | Notes / mitigation |
|---|---|---|
| Broader temp surface under the data dir | Low–Med | All process temp (not only XFTP) lands in `~/.local/share/cursus/tmp`. Same user ACL as the DB (`0700`-style home); still more durable than tmpfs. Prefer creating `tmp/` with `0o700` and periodic cleanup of stale `*_rcv.xftp` trees. |
| Secrets linger on disk | Med | XFTP decrypt staging and other temp files survive reboot (unlike tmpfs). Encrypted DBs remain in SQLCipher, but **plaintext attachment bytes** and transient crypto material in temp may persist until deleted. Mitigate: wipe `tmp/` on clean shutdown / startup; keep `encrypt=off` only while we lack CryptoFile decrypt in the UI path, then revisit. |
| Multi-instance / shared `TMPDIR` races | Low | Two Cursus processes sharing one profile already contend on the DB lock; sharing `tmp/` adds rename/unlink races on leftover dirs. One profile → one running client is the assumed model. |
| Env mutation (`set_var`) | Low | Setting `TMPDIR` is process-global and must happen **before** agent threads start. Do not toggle it later; do not inherit a hostile caller `TMPDIR` without overriding. Document that wrappers should not force `TMPDIR` back to `/tmp`. |
| Path injection via folder commands | Low | `/files_folder` and `/_temp_folder` take absolute paths we control from `directories::ProjectDirs`. Never pass user-controlled path strings into those commands. |
| Fallback scavenger (explicitly not shipped) | High if done naïvely | Copying arbitrary files from `/tmp/*_rcv.xftp` into the chat files dir could import another user’s or process’s leftover decrypts on a shared `/tmp`, or race with an active transfer. That is why the scavenger approach stays rejected. |
| `encrypt=off` for receives | Med (product) | Local files are plaintext on disk under `files/`. Acceptable for M1 desktop with home-dir DAC; not a substitute for disk encryption / future `encryptLocalFiles` + CryptoFile read support. |

**Bottom line:** pinning temp next to the profile is a **correctness** fix with bounded, disk-persistence tradeoffs. It is safer than scavenging system `/tmp`, and it should be paired with restrictive directory modes and temp cleanup—not treated as a long-term substitute for an EXDEV-safe move in `libsimplex`.

### Auto-receive (match official app)

Official iOS / multiplatform clients call `receiveFile(…, auto: true)` from the `newChatItems` handler when `privacyAcceptImages` is on and the item is:

- **image** ≤ `MAX_IMAGE_SIZE_AUTO_RCV` (~510 KiB), or
- **video** ≤ `MAX_VIDEO_SIZE_AUTO_RCV` (~1023 KiB), or
- **voice** ≤ `MAX_VOICE_SIZE_AUTO_RCV` (~510 KiB)

Cursus mirrors that: on `newChatItems` (and when loading history that still has `rcvInvitation` offers), eligible files are `/freceive`’d without a Download tap. Generic files and oversize media keep a manual download control.

## 6. First milestone scope

Working Linux app that:

1. Loads pinned `libsimplex`
2. Creates/opens a Cursus profile DB
3. Starts chat against default relays
4. Lists conversations
5. Sends and receives text (emoji included as Unicode text)
6. Basic file/image attach via SimpleX file commands when wired

Channels and Calls navigation exist as UI sections; protocol features are only exposed when SimpleX APIs support them (documented in `roadmap.md`). No fake call/channel backends.
