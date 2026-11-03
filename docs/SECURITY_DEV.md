# Cursus security tracker (development)

Living notes for what is encrypted, what is still plaintext, and how to test it.
Update this file when security-relevant behavior changes.

## Threat model (desktop M1+)

| Layer | Status | Notes |
|---|---|---|
| Message E2E (SMP / ratchet) | **Active** | Owned by official `libsimplex`; Cursus does not reimplement |
| Chat + agent DB at rest | **Active (user password)** | SQLCipher key = password from welcome / unlock; never stored on disk by Cursus |
| Local attachments at rest | **Active** | Receive: `/freceive … encrypt=on`. Send: `chat_encrypt_file` → CryptoFile under `files/` + `fileSource.cryptoArgs`. Keys in SQLCipher |
| Plaintext decrypt cache | **Ephemeral** | `tmp/plain/` for previews / open / mpv; wiped on session open (`wipe_dir_contents`) |
| Password / key in prefs | **Not stored** | Only `account.json` (display name + flags). Unlock re-enters password each launch |
| OS keyring (libsecret) | **Planned** | Optional unlock convenience — see below (not implemented yet) |
| Security code / PQ UI | Deferred | Core defaults still apply |
| Custom SMP/XFTP servers | Deferred | Defaults only |

## Planned: libsecret / Secret Service (like GNOME Fractal)

**Do not implement yet** — track here until we add an optional “remember unlock” path.

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
| `tmp/` | XFTP staging + `tmp/plain/` decrypt cache |

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
5. Quit Cursus — `tmp/` is wiped on next session open (`wipe_dir_contents`); `files/` ciphertext remains.
6. Unit: `Commands::receive_file` contains `encrypt=on`; `parse_file_info` reads `cryptoArgs`; `plaintext_cache_path` is stable per key/nonce.

## Open gaps / next hardening

- [ ] Optional “change DB password” in Settings (SQLCipher rekey)
- [ ] libsecret / Secret Service unlock convenience (Fractal-style; see section above)
- [ ] Wipe `tmp/plain/` on hide-to-tray / lock
- [ ] Verify security code UI
- [x] Ensure send path always uses CryptoFile when encrypt-on (`chat_encrypt_file` + `fileSource`)
- [ ] Automated integration test that writes encrypt=on receive and asserts ciphertext ≠ plaintext
- [ ] One-shot migrate: encrypt any leftover plaintext under `files/` that still has no `cryptoArgs`

## Changelog

| Date | Change |
|---|---|
| 2026-11-03 | Noted Fractal-style libsecret plan (deferred); contact Disappearing messages + Full deletion prefs |
| 2026-11-03 | Outgoing voice/files encrypt via `chat_encrypt_file` (voice was plaintext when recorded into `files/`) |
| 2026-11-03 | Welcome + SQLCipher password; `encrypt=on` receives; CryptoFile decrypt cache; this tracker |
| earlier | E2E via libsimplex; fixed `cursus-dev-key`; `encrypt=off` receives (removed) |
