# Cursus roadmap

Living progress tracker. Design tokens and IA: [`docs/DESIGN_SYSTEM.md`](docs/DESIGN_SYSTEM.md). Architecture: [`docs/architecture.md`](docs/architecture.md).

**Principle:** one product, one design language, one shared Rust brain (`cursus-core` + `libsimplex`), four native experiences.

---

## Milestone 1 — Linux foundation

| Status | Item |
|---|---|
| Done | Investigate SimpleX client integration (`libsimplex` C ABI) |
| Done | Architecture + canonical design system (`docs/DESIGN_SYSTEM.md`) |
| Done | Cargo workspace (`cursus-core`, `cursus-linux`) |
| Done | Fetch / load `libsimplex`; open Cursus DB (smoke-tested) |
| Done | Start chat; list conversations; send text through core |
| Done | Local Notes send path (`/_create *…`, not `/_send *…`) |
| Done | Load and display chat history (`/_get chat … count=…`) |
| Done | Adaptive layouts: mobile / compact / desktop on **logical** px |
| Done | Preserve nav section + selected chat across resize |
| Done | SVG nav icons (licensed) — icon-only, centered rail / bottom bar |
| Done | Nav/list hover & press keep icons readable (soft accent, never solid fill) |
| Done | Invite links: one-time invite, reusable address, paste-to-accept, copy |
| Done | Live inbound events via `chat_recv_msg_wait` → refresh chats/messages |
| Done | Mark chat read (`/_read chat`) when opening / viewing — clears unread badge |
| Done | Accept short HTTPS invites via `/connect` (not `/_connect userId`) |
| Done | Contact-request UX — Accept/Reject for address inbound (`/_accept` / `/_reject`) |
| Done | Chat context menu (right-click / long-press): read, favorite, mute, lists, clear, delete |
| Done | Collapsible connect card + send / chevron SVG icons |
| Done | Message statuses (sent/delivered receipts) in the bubble UI |
| Done | Basic file/image attach + download via SimpleX file APIs |
| Done | XFTP empty-file fix (`TMPDIR` on same FS as `files/`; see architecture.md) |
| Done | Inline image preview + click-to-open for completed local images |
| Done | Auto-receive images/voice/video (official size caps; no Download tap) |
| Done | Voice messages: record (ffmpeg m4a), draft send/cancel, waveform play |
| Done | Photo/video gallery (swipe) + video via mpv; files open with xdg-open |
| Done | Unicode emoji / rich content presentation polish |
| Done | Message reactions (👍👎😀😂😢❤️🚀✅; chips + picker on message menu) |
| Done | Message edit menu (right-click / long-press): reply, copy, forward, info, delete, select |
| Done | Richer attachment UX (progress bars, failure detail) |

## Milestone 2 — Desktop polish

| Status | Item |
|---|---|
| In progress | macOS app: native SwiftUI over cursus-core via `bridge/` (macOS 12+, see `macos/README.md`) |
| Done | Richer attachment UX (progress bars, failure detail) |
| Done | Notifications + optional tray (Linux) |
| Done | Light theme following OS preference (System / Light / Dark) |
| Done | Address auto-accept setting (libsimplex `/_address_settings`) |
| Done | Secret-span tap to reveal/hide |
| Done | Group reaction member list (`/_reaction members`) |
| Done | Own profile editor (`/_profile`, `/_profile_address`) + avatar data-URL ≤12500 |
| Done | Contact profile page (header tap / View contact; alias via `/_set alias`; mute/favorite/clear/delete) |

## Milestone 3 — Mobile bridge

| Status | Item |
|---|---|
| Done | Shared JSON bridge (`bridge/`) for every native app — replaces the UniFFI plan |
| In progress | Android app (Kotlin + Compose, `android/`): onboarding/unlock, chats, channels, conversation, connect, settings |
| Todo | iOS shell (SwiftUI) — same four destinations, `TabView` / split |

## Milestone 4 — Channels & calls

| Status | Item |
|---|---|
| Done | Create group (`/_group`) from Chats |
| Done | Create channel (`/_public group` + `/_servers` relays) |
| Done | Channels nav lists relay-backed groups (`useRelays`); open/message as chats |
| Done | Group settings parity: welcome message, short descr, prefs, roles, remove/block, leave/delete, link role |
| Partial | Calls: `/_call invite|reject|end` signaling + Calls UI; **WebRTC media bridge** (browser `call.html`) still next |

---

## Recently completed (detail)

- **Responsive UI** — centralized breakpoints in `linux/src/layout.rs` (`<650` mobile, `650–950` compact, `>950` desktop); aspect-ratio nudge; scale factor is diagnostic only.
- **`Failed reading: empty`** — fixed at source: Notes use `/_create *{id} json …`.
- **Message pane** — history bubbles after select/send.
- **Icons** — vendored SVGs under `linux/assets/icons/` with license attribution (`ATTRIBUTION.md`).
- **Invite links** — Chats panel: one-time invite (`/_connect`), address (`/_show_address` / `/_address`), accept pasted link, clipboard copy.
- **Live events** — Iced subscription owns `spawn_receiver`; `newChatItems` / connection events refresh UI.
- **Attachments** — send via `/_send` file/image; receive via `/freceive` + XFTP; image bubbles preview local JPEG/PNG/… when complete.
- **XFTP EXDEV** — documented under `docs/architecture.md` (TMPDIR pin + security notes); upstream `moveFileAcrossDevices` pending lib rebuild.
- **Emoji / rich text** — short emoji-only messages render large (48/36); SimpleX `formattedText` drives bold/italic/strike/snippet/colored/links (tap opens via `xdg-open`).
- **Reactions** — chips under bubbles; message menu hosts the 8 protocol emojis on top; tap toggles via `/_reaction`; live `chatItemReaction` refreshes history.
- **Message menu** — right-click / long-press: reaction picker + reply / copy / forward / info / delete / select; Escape closes; menu closes after reacting.
- **Deleted messages** — broadcast delete marks items (trash + italic “marked deleted”; Reveal/Hide via menu); second delete removes for me only (`internal`); live `chatItemsDeleted` refresh.
- **Reply quotes** — quoted preview in the bubble; tap jumps/highlights the original.
- **Attachment progress** — XFTP progress bar + `bytes / total` labels; failure detail from `fileStatus`; in-place progress patches avoid scroll jumps.
- **Jump to latest** — FAB when scrolled up into history.
- **Notifications + tray** — desktop alerts via notify-rust; optional KSNI tray (Show/Hide/Quit, close-to-tray); Settings toggles; `desktop.json` prefs.

## Known limitations (honest)

| Topic | Status |
|---|---|
| Custom SMP/XFTP server UI | Out of scope for M1 (defaults only) |
| Channel create without relays | Needs enabled chat relays via `/_servers` / network settings |
| In-app WebRTC media | Signaling works; media plane (localhost call.html + WS) still to port from official desktop |
| Building `libsimplex` inside `cargo build` | Not done; use `scripts/fetch-libs.sh` |
| UniFFI | Planned; not required for Linux M1 |
| Auto-accept address requests | Settings toggle; synced to core address settings |
| Verify security code / PQ / connection servers | Deferred (contact profile shows identity + everyday actions only) |
| Per-contact preference editor (`/_set prefs`) | Deferred |
| Group member profile page | Deferred |
| Multi-profile / user switching | Deferred |

## Suggested next feature slice

1. WebRTC media bridge for calls (serve `call.html` + WS like official desktop).
2. macOS shell sharing desktop UI modules + native nav chrome.
3. UniFFI API on `cursus-core` ahead of Android / iOS shells.

## Recently completed (desktop)

- **Notifications + tray** — `notify-rust` for inbound alerts when the window is not focused on that chat (respects mute); KSNI tray with Show / Hide / Quit; Settings toggles; close-to-tray; prefs in `~/.local/share/cursus/desktop.json`.
- **Light theme** — design-system light/dark tokens; Settings cycles System → Light → Dark (System reads GNOME color-scheme); persisted in `desktop.json`.
- **Address auto-accept** — Settings toggle writes `/_address_settings`; state synced from `/_show_address` after connect.
- **Secret spans** — tap `#secret#` rich text to reveal/hide (per message / segment).
- **Reaction members** — in groups, right-click or long-press a reaction chip to list who reacted (`/_reaction members`).
- **Profiles** — own profile editor in Settings; contact profile overlay from header / menu.
- **Own profile** — Settings → My profile: display name, bio (≤160 bytes), description, picture (JPEG data-URL ≤12500), share-address toggle (`/_profile` / `/_profile_address`).
- **Contact profile** — tap conversation header or chat menu “View contact”; alias (`/_set alias`); mute / favorite / clear / delete; avatar + bio from peer profile JSON.
