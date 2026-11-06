# Cursus security tracker (development)

Living notes for what is encrypted, what is still plaintext, and how to test it.
Update this file when security-relevant behavior changes.

## Threat model (desktop M1+)

| Layer | Status | Notes |
|---|---|---|
| Message E2E (SMP / ratchet) | **Active** | Owned by official `libsimplex`; Cursus does not reimplement |
| Chat + agent DB at rest | **Active (user password)** | SQLCipher key = password from welcome / unlock; never stored on disk by Cursus |
| Local attachments at rest | **Active** | Receive: `/freceive … encrypt=on`. Send: `chat_encrypt_file` → CryptoFile under `files/` + `fileSource.cryptoArgs`. Keys in SQLCipher |
| Plaintext decrypt cache | **Ephemeral, RAM-first** | Decrypts go to `$XDG_RUNTIME_DIR/cursus/plain` (per-user tmpfs, emptied at logout/reboot) when it has room (`fits_in_ram_cache`), else `tmp/plain/`; playback WAVs in `…/cursus/play`. Wiped on quit, on SIGTERM/SIGHUP/SIGINT and panic (`exit_wipe`), and on session open (crash / SIGKILL) |
| Data dir permissions | **Active** | `~/.local/share/cursus` forced to `0700`, files `0600` at startup (DBs, log, prefs were world-readable) |
| Logs | **Redacted** | Commands logged as verb + chat refs only (`redact_command`); raw events never logged; log rotates at 2 MB |
| Links from messages | **Allowlisted + cleaned** | Only `http(s)`, `mailto` (query stripped) and `tel` reach `xdg-open` (`safe_external_url`); tracking params (`utm_*`, `fbclid`, …) removed (`strip_tracking_params`); always behind a confirm sheet |
| Notifications | **Text hidden by default** | Chat name + “New message” only (`hide_notification_text`); turning it off shows a warning |
| IP address vs file servers | **Protected by default** | `/freceive … approved_relays=off`; files on servers we don't use need confirmation (`ReceiveOutcome::NeedsApproval`), auto-receive skips them — mirrors official “Protect IP address” |
| Password in memory | **Zeroized** | `Secret` (zeroize-on-drop, `Debug` = `***`) for form fields / messages / connect; `SessionConfig.db_key` `Zeroizing`; FFI key `CString` wiped after `chat_migrate_init`. iced's own text-input copies and libsimplex's SQLCipher key are outside our control |
| Image decoding | **Bounded** | Photos: 128 MiB / 12000 px per decode; avatars: 16 MiB / 2048 px (decompression bombs refused) |
| Close to tray | **Off by default** | Warning before enabling: app stays unlocked, cache kept until quit |
| Opening attachments | **Guarded** | Executable / installer types (`.exe`, `.jar`, `.desktop`, scripts…) are never opened by tap (`is_risky_to_open`); Save as… only |
| Password / key in prefs | **Not stored** | Only `account.json` (display name + flags). Unlock re-enters password each launch |
| App lock | **Opt-in** | Re-asks the DB password after 30 s–15 min idle, on focus loss ("Immediately"), when hidden to tray, or Ctrl+L. Verified against an in-memory Argon2id hash made when the password opened the DB (`app_lock.rs`; core `/db test key` rejects spaces/non-ASCII). Locking wipes decrypted media + previews, closes overlays; notifications show only "Cursus — New message". Back-off after 3 misses (2 s → 60 s) |
| OS keyring unlock | **Opt-in** | "Unlock automatically at start": DB password in the Secret Service via `oo7` (GNOME Keyring / KWallet / KeePassXC), verified before storing, deleted when turned off; falls back to the password prompt |
| Screen privacy | **Settings** | Chat-list message previews (default on), blur photos until tapped (default off) |
| Delivery receipts / auto-download / file encryption / link cleaning | **Settings** | Mirrors official defaults (receipts on, auto-download on, encrypt on); link cleaning on |
| Security code / PQ UI | Deferred | Core defaults still apply |
| Custom SMP/XFTP servers | Deferred | Defaults only |

## Secret Service unlock (implemented, opt-in)

Implemented as "Unlock automatically at start" (Privacy & security → Lock). Design notes kept below.

[Fractal](https://gitlab.gnome.org/World/fractal) keeps Matrix session secrets in the desktop Secret Service (historically `libsecret`, now often via Rust `oo7` / portal). Pattern we should mirror:

| Piece | Intent for Cursus |
|---|---|
| Schema / attributes | e.g. `chat.cursus.db-key`, attrs: `profile` / `display_name` / schema version — **not** the secret itself |
| Secret payload | SQLCipher password only (never write it to `account.json` or `desktop.json`) |
| Unlock UX | Optional “Store password in keyring” on welcome / unlock; default remains type-each-launch |
| Fallback | If Secret Service missing / locked / denied → current password prompt |
| Wipe | First-run wipe / account delete must also delete the keyring item |

Still out of scope until then: auto-unlock without any user gesture; syncing the key to cloud.

## On-disk layout (`~/.local/share/cursus`)

| Path | Contents |
|---|---|
| `account.json` | Display name, `encrypt_local_files`, schema version — **no password** |
| `simplex_v1_chat.db` / `_agent.db` | SQLCipher (password) |
| `files/` | Received/sent attachment ciphertext when encrypt-on |
| `tmp/` | XFTP staging, `rec/` voice drafts, `plain/` disk fallback for decrypts |
| `$XDG_RUNTIME_DIR/cursus/` | RAM-backed `plain/` decrypt cache + `play/` transcodes (outside the data dir) |

## First-run / wipe (simulate brand-new install)

```bash
# Quit Cursus first, then:
rm -rf ~/.local/share/cursus
# Optional: also clear desktop prefs
rm -f ~/.local/share/cursus/desktop.json   # already removed by rm -rf above
cargo run -p cursus-linux
```

Expected: welcome → name → password → Next → chats.

Unlock test: quit and relaunch → password prompt → same profile.

Wrong password: migration / open fails with an error; data untouched.

## Attachment encryption test plan

1. Wipe + complete welcome (password set, encrypt on).
2. Receive an image from another SimpleX client (or send to self via second profile later).
3. On disk after receive completes:

```bash
ls -la ~/.local/share/cursus/files/
file ~/.local/share/cursus/files/*   # should NOT report JPEG/PNG/MP4 for ciphertext
# while the app is open and previewed:
ls -la ~/.local/share/cursus/tmp/plain/   # decrypted cache for UI
```

4. In Cursus: bubble preview, media viewer, open, save-as, and voice play use `tmp/plain/` (not raw `files/`).
5. Quit Cursus — `tmp/plain/`, `tmp/play/`, `tmp/rec/` are emptied immediately; everything in `tmp/` is wiped again on the next session open (crash / logout case); `files/` ciphertext remains.
6. Unit: `Commands::receive_file` contains `encrypt=on`; `parse_file_info` reads `cryptoArgs`; `plaintext_cache_path` is stable per key/nonce.

## Open gaps / next hardening

- [ ] Optional “change DB password” in Settings (SQLCipher rekey)
- [x] Secret Service unlock convenience (opt-in, `oo7`)
- [ ] Delete the keyring entry from a future "wipe local data" flow
- [x] App lock (re-ask password after idle / when hidden to tray; wipes the cache on lock)
- [x] Hide message text in desktop notifications (default on)
- [x] Zeroize the password after unlock (best effort; see table)
- [x] Protect IP address from unknown XFTP servers
- [ ] Verify security code UI
- [x] Ensure send path always uses CryptoFile when encrypt-on (`chat_encrypt_file` + `fileSource`)
- [ ] Automated integration test that writes encrypt=on receive and asserts ciphertext ≠ plaintext
- [ ] One-shot migrate: encrypt any leftover plaintext under `files/` that still has no `cryptoArgs`

## Changelog

| Date | Change |
|---|---|
| 2026-11-06 | App lock (touch + keyboard lock screen, in-memory Argon2id verifier), opt-in keyring unlock, chat-list previews / blur media / auto-download / encrypt files / clean links / delivery receipts settings; settings regrouped (Privacy & security, Notifications & tray); warnings on every privacy-lowering switch |
| 2026-11-06 | Privacy settings: hidden notification text + Protect IP (both default on, warnings when turning off); close-to-tray off by default (+ warning); password zeroized; RAM-first decrypt cache + wipe on signals / panic; lower image decode limits; link tracking params stripped |
| 2026-11-06 | Security audit: decrypt cache no longer persisted across sessions (was kept forever); 0700/0600 data dir; command/event log redaction + rotation; decrypt retry loop fixed; link scheme allowlist; executable attachments not opened |
| 2026-11-03 | Noted Fractal-style libsecret plan (deferred); contact Disappearing messages + Full deletion prefs |
| 2026-11-03 | Outgoing voice/files encrypt via `chat_encrypt_file` (voice was plaintext when recorded into `files/`) |
| 2026-11-03 | Welcome + SQLCipher password; `encrypt=on` receives; CryptoFile decrypt cache; this tracker |
| earlier | E2E via libsimplex; fixed `cursus-dev-key`; `encrypt=off` receives (removed) |
