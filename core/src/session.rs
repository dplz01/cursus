use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

use crate::commands::Commands;
use crate::error::{Error, Result};
use crate::ffi::LibSimplex;
use crate::models::{
    parse_conn_link, parse_event, ApiEnvelope, ChatId, ChatMessage, ChatPreview, ChatSettings,
    ChatTag, ChatType, ConnLink, DeliveryStatus, Event, FileInfo, FileKind, FileTransferState,
    CallMedia, ChatRelayInfo, GroupMemberInfo, NtfFilter, ProfileFields, ReactionCount,
    ReactionMember, UserProfile, KNOWN_REACTIONS,
};
use crate::paths::{
    data_dir, db_prefix, default_libs_dir, files_dir, fits_in_ram_cache, free_bytes,
    plaintext_cache_name, plaintext_cache_path, resolve_local_file, runtime_cache_dir,
    temp_dir,
};

#[derive(Clone)]
pub struct SessionConfig {
    pub libs_dir: PathBuf,
    pub db_prefix: PathBuf,
    /// SQLCipher key chosen at welcome / unlock (never written to disk by
    /// Cursus). Wiped from memory when the config is dropped.
    pub db_key: zeroize::Zeroizing<String>,
    pub migration_confirm: String,
    /// Display name for a brand-new active user (welcome wizard).
    pub initial_display_name: String,
    /// Enable `/_files_encrypt on` after start (CryptoFile at rest).
    pub encrypt_local_files: bool,
}

/// Never print the database key (logs, panics, debuggers).
impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("libs_dir", &self.libs_dir)
            .field("db_prefix", &self.db_prefix)
            .field("db_key", &"***")
            .field("migration_confirm", &self.migration_confirm)
            .field("initial_display_name", &self.initial_display_name)
            .field("encrypt_local_files", &self.encrypt_local_files)
            .finish()
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            libs_dir: default_libs_dir(),
            db_prefix: db_prefix(),
            db_key: zeroize::Zeroizing::new(String::new()),
            migration_confirm: "yesUp".into(),
            initial_display_name: "Cursus".into(),
            encrypt_local_files: true,
        }
    }
}

/// Open SimpleX chat session backed by `libsimplex`.
pub struct Session {
    lib: Arc<LibSimplex>,
    ctrl: *mut std::ffi::c_void,
    /// Cached active user; mutex so profile updates work through `Arc<Session>`.
    user: Mutex<Option<UserProfile>>,
    /// Mirror of `/_files_encrypt` — encrypt outgoing attachments before send.
    encrypt_local_files: std::sync::atomic::AtomicBool,
}

/// Opaque chat controller pointer. Official clients use it concurrently from
/// a command thread and a receive thread.
#[derive(Copy, Clone)]
struct CtrlHandle(*mut std::ffi::c_void);
unsafe impl Send for CtrlHandle {}
unsafe impl Sync for CtrlHandle {}

// Session moves / shares between the UI runtime and blocking tasks after connect.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

struct ReceiveWorker {
    lib: Arc<LibSimplex>,
    ctrl: CtrlHandle,
    tx: mpsc::UnboundedSender<Event>,
}

impl ReceiveWorker {
    fn run(self) {
        loop {
            // Short wait so the worker notices a dropped subscriber (UI quit /
            // session swap) within ~1s; a 5-minute wait kept tokio's blocking
            // pool busy and froze runtime shutdown on Quit.
            if self.tx.is_closed() {
                break;
            }
            match self.lib.recv_msg_wait(self.ctrl.0, 1_000_000) {
                Ok(None) => continue,
                Ok(Some(raw)) => match serde_json::from_str::<ApiEnvelope>(&raw) {
                    Ok(env) => match env.into_result() {
                        Ok(value) => {
                            if self.tx.send(parse_event(value)).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            let brief = truncate_chars(&err.to_string(), 160);
                            tracing::warn!("event envelope error: {brief}");
                        }
                    },
                    Err(err) => match serde_json::from_str::<Value>(&raw) {
                        Ok(value) => {
                            if let Some(result) = value.get("result").cloned() {
                                let _ = self.tx.send(parse_event(result));
                            } else {
                                let _ = self.tx.send(Event::Raw(value));
                            }
                        }
                        // Never log `raw`: events carry message text and profiles.
                        Err(_) => tracing::warn!("bad event JSON: {err} ({} bytes)", raw.len()),
                    },
                },
                Err(err) => {
                    tracing::error!("chat_recv_msg_wait failed: {err}");
                    break;
                }
            }
        }
    }
}

impl Session {
    pub fn open(config: SessionConfig) -> Result<Self> {
        tracing::info!("session open: prepare dirs");
        // Owner-only data dir: SQLCipher DBs, log and prefs live here.
        ensure_private_dir(&data_dir())?;
        ensure_private_dir(&files_dir())?;
        let tmp = temp_dir();
        // Drop everything left in tmp/: XFTP staging after crashes and, above
        // all, the `plain/` decrypt cache. Decrypted attachments must not
        // outlive the session that needed them (at-rest encryption).
        wipe_dir_contents(&tmp);
        if let Some(ram) = runtime_cache_dir() {
            wipe_dir_contents(&ram);
        }
        ensure_private_dir(&tmp)?;
        ensure_private_dir(&tmp.join("plain"))?;
        // Agent XFTP decrypts under getTemporaryDirectory(); pin TMPDIR so the
        // subsequent rename into files_dir stays on one filesystem (see temp_dir
        // + docs/architecture.md).
        // SAFETY: single-threaded init before libsimplex / agent threads start.
        unsafe {
            std::env::set_var("TMPDIR", &tmp);
            std::env::set_var("TMP", &tmp);
            std::env::set_var("TEMP", &tmp);
        }
        if let Some(parent) = config.db_prefix.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::info!(libs = %config.libs_dir.display(), "session open: load libsimplex");
        let lib = Arc::new(LibSimplex::load(Some(&config.libs_dir))?);
        tracing::info!(
            db = %config.db_prefix.display(),
            "session open: migrate_init (SQLCipher)"
        );
        let ctrl = lib.migrate_init(
            &config.db_prefix,
            &config.db_key,
            &config.migration_confirm,
        )?;
        tracing::info!("session open: migrate_init ok");

        let mut session = Self {
            lib,
            ctrl,
            user: Mutex::new(None),
            encrypt_local_files: std::sync::atomic::AtomicBool::new(config.encrypt_local_files),
        };
        let name = if config.initial_display_name.trim().is_empty() {
            "Cursus"
        } else {
            config.initial_display_name.trim()
        };
        tracing::info!("session open: ensure_user");
        session.ensure_user(name)?;
        tracing::info!("session open: ready");
        Ok(session)
    }

    fn set_user(&self, user: UserProfile) {
        if let Ok(mut guard) = self.user.lock() {
            *guard = Some(user);
        }
    }

    fn with_user<R>(&self, f: impl FnOnce(&UserProfile) -> R) -> Option<R> {
        self.user
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(f))
    }

    pub fn start_chat(&mut self) -> Result<Value> {
        self.start_chat_with_options(true)
    }

    pub fn start_chat_with_options(&mut self, encrypt_local_files: bool) -> Result<Value> {
        tracing::info!("start_chat");
        let started = self.send(Commands::start_chat())?;
        let files = files_dir().to_string_lossy().to_string();
        let tmp = temp_dir().to_string_lossy().to_string();
        let _ = self.send(&Commands::set_temp_folder(&tmp));
        let _ = self.send(&Commands::set_files_folder(&files));
        // Official default: encrypt files at rest (CryptoFile). Keys live in SQLCipher.
        self.encrypt_local_files
            .store(encrypt_local_files, std::sync::atomic::Ordering::Relaxed);
        let _ = self.send(&Commands::set_encrypt_local_files(encrypt_local_files));
        // Activate / resubscribe run after the UI receive loop is up — doing them
        // here has hung unlock when the agent blocked before any recv thread.
        tracing::info!("start_chat ok (activate deferred)");
        Ok(started)
    }

    /// Subscribe transports after the event receiver is running.
    pub fn activate_network(&self) -> Result<()> {
        tracing::info!("activate_chat + resubscribeAll");
        let _ = self.send(Commands::activate_chat());
        let _ = self.send(Commands::resubscribe_all());
        Ok(())
    }

    /// Decrypt a CryptoFile for viewing (cached for the session). Returns the
    /// plaintext path: in the RAM-backed runtime dir when it has room
    /// ([`runtime_cache_dir`]), else under `tmp/plain/` on disk.
    pub fn ensure_plaintext_file(
        &self,
        encrypted: &std::path::Path,
        key: &str,
        nonce: &str,
    ) -> Result<PathBuf> {
        let name = plaintext_cache_name(encrypted, key, nonce);
        let ram = runtime_cache_dir().map(|d| d.join("plain").join(&name));
        let disk = plaintext_cache_path(encrypted, key, nonce);
        for cached in ram.iter().chain(std::iter::once(&disk)) {
            if std::fs::metadata(cached).is_ok_and(|m| m.is_file() && m.len() > 0) {
                return Ok(cached.clone());
            }
        }
        let size = std::fs::metadata(encrypted).map(|m| m.len()).unwrap_or(u64::MAX);
        if let Some(ram) = ram {
            let room = ram
                .parent()
                .and_then(free_bytes)
                .is_some_and(|free| fits_in_ram_cache(size, free));
            if room {
                if let Some(parent) = ram.parent() {
                    ensure_private_dir(parent.parent().unwrap_or(parent))?;
                    ensure_private_dir(parent)?;
                }
                match self.lib.decrypt_file(encrypted, key, nonce, &ram) {
                    Ok(()) => return Ok(ram),
                    // e.g. tmpfs filled up meanwhile: drop the partial file, use disk.
                    Err(err) => {
                        let _ = std::fs::remove_file(&ram);
                        tracing::warn!(%err, "RAM decrypt failed; using disk cache");
                    }
                }
            }
        }
        if let Some(parent) = disk.parent() {
            ensure_private_dir(parent)?;
        }
        if let Err(err) = self.lib.decrypt_file(encrypted, key, nonce, &disk) {
            let _ = std::fs::remove_file(&disk);
            return Err(err);
        }
        Ok(disk)
    }

    pub fn list_chats(&self) -> Result<Vec<ChatPreview>> {
        let value = self.send(&Commands::get_chats(self.user_id()?))?;
        Ok(parse_chat_previews(&value))
    }

    pub fn send_text(&self, chat: &ChatId, text: &str) -> Result<Value> {
        self.send_text_quoted(chat, text, None)
    }

    pub fn send_text_quoted(
        &self,
        chat: &ChatId,
        text: &str,
        quoted_item_id: Option<i64>,
    ) -> Result<Value> {
        self.send(&Commands::send_text_quoted(chat, text, quoted_item_id))
    }

    pub fn update_text(&self, chat: &ChatId, item_id: i64, text: &str) -> Result<Value> {
        self.send(&Commands::update_text(chat, item_id, text))
    }

    pub fn delete_items(&self, chat: &ChatId, item_ids: &[i64], broadcast: bool) -> Result<Value> {
        if item_ids.is_empty() {
            return Err(Error::Command("no items to delete".into()));
        }
        self.send(&Commands::delete_items(chat, item_ids, broadcast))
    }

    pub fn item_info(&self, chat: &ChatId, item_id: i64) -> Result<Value> {
        self.send(&Commands::item_info(chat, item_id))
    }

    pub fn forward_items(
        &self,
        from: &ChatId,
        to: &ChatId,
        item_ids: &[i64],
    ) -> Result<Value> {
        if item_ids.is_empty() {
            return Err(Error::Command("no items to forward".into()));
        }
        self.send(&Commands::forward_items(from, to, item_ids))
    }

    pub fn send_file(&self, chat: &ChatId, path: &str) -> Result<Value> {
        let path = path.trim();
        if path.is_empty() {
            return Err(Error::Command("empty file path".into()));
        }
        let src = std::path::Path::new(path);
        let file_name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let as_image = is_image_name(&file_name);
        let prepared = self.prepare_outgoing_file(src, &file_name)?;
        self.send(&Commands::send_file(
            chat,
            &prepared.cmd_path,
            &file_name,
            as_image,
            prepared.crypto.as_ref().map(|(k, n)| (k.as_str(), n.as_str())),
        ))
    }

    pub fn send_voice(&self, chat: &ChatId, path: &str, duration_secs: u32) -> Result<Value> {
        let path = path.trim();
        if path.is_empty() {
            return Err(Error::Command("empty voice path".into()));
        }
        if duration_secs == 0 {
            return Err(Error::Command("voice message too short".into()));
        }
        let src = std::path::Path::new(path);
        let file_name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("voice.m4a")
            .to_string();
        let prepared = self.prepare_outgoing_file(src, &file_name)?;
        self.send(&Commands::send_voice(
            chat,
            &prepared.cmd_path,
            duration_secs,
            prepared.crypto.as_ref().map(|(k, n)| (k.as_str(), n.as_str())),
        ))
    }

    /// When local-file encryption is on: encrypt `src` into `files/` and return a
    /// relative CryptoFile path + args (official client pattern). Otherwise pass
    /// the absolute plaintext path through.
    fn prepare_outgoing_file(
        &self,
        src: &std::path::Path,
        preferred_name: &str,
    ) -> Result<PreparedOutgoingFile> {
        if !self.encrypt_local_files.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(PreparedOutgoingFile {
                cmd_path: src.to_string_lossy().into_owned(),
                crypto: None,
            });
        }
        if !src.is_file() {
            return Err(Error::Command(format!(
                "outgoing file missing: {}",
                src.display()
            )));
        }
        ensure_private_dir(&files_dir())?;
        let dest_name = unique_files_name(preferred_name);
        let dest = files_dir().join(&dest_name);
        // Never encrypt in-place over the source — write ciphertext beside it.
        if dest == src {
            return Err(Error::Command(
                "refusing to encrypt over the plaintext source path".into(),
            ));
        }
        let (key, nonce) = self.lib.encrypt_file(self.ctrl, src, &dest)?;
        // Drop the plaintext recording / picker copy when it lives under our data dir.
        let data = data_dir();
        if src.starts_with(&data) {
            let _ = std::fs::remove_file(src);
        }
        Ok(PreparedOutgoingFile {
            // Relative to filesFolder — matches official CryptoFile sends.
            cmd_path: dest_name,
            crypto: Some((key, nonce)),
        })
    }

    /// Accept an incoming file. With `approved_relays` false the core refuses
    /// file servers we don't use; that comes back as
    /// [`ReceiveOutcome::NeedsApproval`] instead of an error.
    pub fn receive_file(&self, file_id: i64, approved_relays: bool) -> Result<ReceiveOutcome> {
        std::fs::create_dir_all(files_dir()).map_err(|e| Error::Command(e.to_string()))?;
        let cmd = Commands::receive_file(file_id, approved_relays);
        let raw = self.lib.send_cmd(self.ctrl, &cmd)?;
        let env: ApiEnvelope = serde_json::from_str(&raw)?;
        if let Some(servers) = env.error.as_ref().and_then(file_not_approved_servers) {
            return Ok(ReceiveOutcome::NeedsApproval { servers });
        }
        env.into_result().map(|_| ReceiveOutcome::Accepted).map_err(|err| {
            tracing::warn!(cmd = %redact_command(&cmd), error = %err, "receive failed");
            err
        })
    }

    /// Toggle a reaction on a chat item (`add` true → on, false → off).
    pub fn set_reaction(
        &self,
        chat: &ChatId,
        item_id: i64,
        emoji: &str,
        add: bool,
    ) -> Result<Value> {
        let emoji = normalize_reaction_emoji(emoji)
            .ok_or_else(|| Error::Command(format!("unsupported reaction: {emoji}")))?;
        self.send(&Commands::reaction(chat, item_id, emoji, add))
    }

    pub fn list_messages(&self, chat: &ChatId, count: u32) -> Result<Vec<ChatMessage>> {
        let value = self.send(&Commands::get_chat(chat, count))?;
        Ok(parse_chat_messages(&value))
    }

    /// Ensure a reusable contact address exists and return its link.
    pub fn ensure_address_link(&self) -> Result<ConnLink> {
        let user_id = self.user_id()?;
        match self.send(&Commands::show_address(user_id)) {
            Ok(value) => parse_conn_link(
                &value,
                "/contactLink/connLinkContact/connFullLink",
                "/contactLink/connLinkContact/connShortLink",
            )
            .ok_or_else(|| Error::Command(format!("address missing link: {value}"))),
            Err(_) => {
                let value = self.send(&Commands::create_address(user_id))?;
                parse_conn_link(
                    &value,
                    "/connLinkContact/connFullLink",
                    "/connLinkContact/connShortLink",
                )
                .or_else(|| {
                    parse_conn_link(
                        &value,
                        "/contactLink/connLinkContact/connFullLink",
                        "/contactLink/connLinkContact/connShortLink",
                    )
                })
                .ok_or_else(|| Error::Command(format!("created address missing link: {value}")))
            }
        }
    }

    /// Whether the reusable address auto-accepts inbound contact requests.
    pub fn address_auto_accept(&self) -> Result<bool> {
        let user_id = self.user_id()?;
        let value = match self.send(&Commands::show_address(user_id)) {
            Ok(v) => v,
            Err(_) => {
                // No address yet — create one (defaults: auto-accept off).
                self.send(&Commands::create_address(user_id))?
            }
        };
        Ok(parse_address_auto_accept(&value))
    }

    /// Enable/disable address auto-accept (`acceptIncognito=false` when on).
    pub fn set_address_auto_accept(&self, enabled: bool) -> Result<bool> {
        let user_id = self.user_id()?;
        // Ensure address exists so settings apply to a real contact link.
        let _ = self.ensure_address_link()?;
        let current = self.send(&Commands::show_address(user_id)).ok();
        let business = current
            .as_ref()
            .and_then(|v| {
                v.pointer("/contactLink/addressSettings/businessAddress")
                    .or_else(|| v.pointer("/addressSettings/businessAddress"))
                    .and_then(|b| b.as_bool())
            })
            .unwrap_or(false);
        let auto = if enabled { Some(false) } else { None };
        let value = self.send(&Commands::address_settings(user_id, business, auto))?;
        Ok(parse_address_auto_accept(&value))
    }

    /// Group members who reacted with `emoji` on `item_id`.
    pub fn reaction_members(
        &self,
        chat: &ChatId,
        item_id: i64,
        emoji: &str,
    ) -> Result<Vec<ReactionMember>> {
        if !matches!(chat.chat_type, ChatType::Group) {
            return Err(Error::Command(
                "reaction members only available for groups".into(),
            ));
        }
        let emoji = normalize_reaction_emoji(emoji)
            .ok_or_else(|| Error::Command(format!("unsupported reaction: {emoji}")))?;
        let value = self.send(&Commands::reaction_members(
            self.user_id()?,
            chat.api_id,
            item_id,
            emoji,
        ))?;
        Ok(parse_reaction_members(&value))
    }

    /// Create a one-time invitation link.
    pub fn create_invitation_link(&self) -> Result<ConnLink> {
        let value = self.send(&Commands::create_invitation(self.user_id()?))?;
        parse_conn_link(
            &value,
            "/connLinkInvitation/connFullLink",
            "/connLinkInvitation/connShortLink",
        )
        .ok_or_else(|| Error::Command(format!("invitation missing link: {value}")))
    }

    /// Accept an invitation / address / group link (short HTTPS or full `simplex:` URI).
    pub fn connect_link(&self, link: &str) -> Result<Value> {
        let link = link.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == '“' || c == '”');
        let link = link.trim();
        if link.is_empty() {
            return Err(Error::Command("empty invite link".into()));
        }
        // `/connect` resolves short links; `/_connect <userId> …` does not.
        self.send(&Commands::connect_link(self.user_id()?, link))
    }

    /// Clear unread state for a chat (badge goes to zero after list refresh).
    pub fn mark_chat_read(&self, chat: &ChatId) -> Result<Value> {
        let read = self.send(&Commands::read_chat(chat))?;
        // `/_unread` rejects group support scopes (`not supported`); skip it there.
        if chat.scope.is_none() {
            let _ = self.send(&Commands::unread_chat(chat, false));
        }
        Ok(read)
    }

    pub fn mark_chat_unread(&self, chat: &ChatId) -> Result<Value> {
        self.send(&Commands::unread_chat(chat, true))
    }

    pub fn set_chat_settings(&self, chat: &ChatId, settings: &ChatSettings) -> Result<Value> {
        self.send(&Commands::set_chat_settings(chat, settings))
    }

    /// Per-chat local wipe TTL (`/_ttl …`). `None` = restore user default.
    pub fn set_chat_ttl(&self, chat: &ChatId, seconds: Option<i64>) -> Result<Value> {
        self.send(&Commands::set_chat_ttl(self.user_id()?, chat, seconds))
    }

    pub fn clear_chat(&self, chat: &ChatId) -> Result<Value> {
        self.send(&Commands::clear_chat(chat))
    }

    pub fn delete_chat(&self, chat: &ChatId) -> Result<Value> {
        self.send(&Commands::delete_chat(chat))
    }

    pub fn list_chat_tags(&self) -> Result<Vec<ChatTag>> {
        let value = self.send(&Commands::get_chat_tags(self.user_id()?))?;
        Ok(parse_chat_tags(&value))
    }

    pub fn create_chat_tag(&self, text: &str) -> Result<Vec<ChatTag>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Command("list name required".into()));
        }
        let value = self.send(&Commands::create_chat_tag(text))?;
        Ok(parse_chat_tags(&value))
    }

    pub fn set_chat_tags(&self, chat: &ChatId, tag_ids: &[i64]) -> Result<Value> {
        self.send(&Commands::set_chat_tags(chat, tag_ids))
    }

    pub fn accept_contact_request(&self, contact_req_id: i64) -> Result<Value> {
        self.send(&Commands::accept_contact_request(contact_req_id))
    }

    pub fn reject_contact_request(&self, contact_req_id: i64) -> Result<Value> {
        self.send(&Commands::reject_contact_request(contact_req_id))
    }

    /// Active user profile (clone of session cache).
    pub fn active_profile(&self) -> Option<UserProfile> {
        self.user.lock().ok().and_then(|g| g.clone())
    }

    /// Switch at-rest encryption for files received / sent from now on.
    pub fn set_encrypt_local_files(&self, on: bool) -> Result<()> {
        self.send(&Commands::set_encrypt_local_files(on))?;
        self.encrypt_local_files
            .store(on, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Delivery receipts for all contacts (`groups` false) or small groups.
    pub fn set_delivery_receipts(&self, groups: bool, on: bool) -> Result<UserProfile> {
        let user_id = self.user_id()?;
        self.send(&Commands::set_delivery_receipts(user_id, groups, on))?;
        self.refresh_user()
    }

    /// Refresh `/u` into the session cache.
    pub fn refresh_user(&self) -> Result<UserProfile> {
        let value = self.send(Commands::show_active_user())?;
        let user = parse_user(&value)?;
        self.set_user(user.clone());
        Ok(user)
    }

    /// Update own profile and refresh local cache from the response when possible.
    pub fn update_profile(&self, fields: ProfileFields) -> Result<UserProfile> {
        let user_id = self.user_id()?;
        let value = self.send(&Commands::update_profile(user_id, &fields))?;
        let user = parse_user(&value)
            .or_else(|_| {
                if let Some(p) = value.get("toProfile") {
                    let mut u = self.active_profile().unwrap_or(UserProfile {
                        user_id,
                        profile: ProfileFields::default(),
                        send_rcpts_contacts: true,
                        send_rcpts_small_groups: true,
                    });
                    u.profile = parse_profile_fields(Some(p));
                    Ok(u)
                } else {
                    Err(Error::Command(format!("profile update missing user: {value}")))
                }
            })
            .or_else(|_| {
                let refreshed = self.send(Commands::show_active_user())?;
                parse_user(&refreshed)
            })?;
        self.set_user(user.clone());
        Ok(user)
    }

    /// Share / unshare the reusable address on the profile.
    pub fn set_profile_address(&self, on: bool) -> Result<UserProfile> {
        let user_id = self.user_id()?;
        if on {
            let _ = self.ensure_address_link()?;
        }
        let _ = self.send(&Commands::profile_address(user_id, on))?;
        self.refresh_user()
    }

    /// Local alias for a direct contact (`/_set alias @{id}`).
    pub fn set_contact_alias(&self, contact_id: i64, alias: &str) -> Result<Value> {
        self.send(&Commands::set_contact_alias(contact_id, alias.trim()))
    }

    /// Contact feature prefs — Disappearing messages / Full deletion (`/_set prefs`).
    pub fn set_contact_prefs(&self, contact_id: i64, prefs: &Value) -> Result<Value> {
        self.send(&Commands::set_contact_prefs(contact_id, prefs))
    }

    /// Create a private group; returns the new group id when present in the response.
    pub fn create_group(&self, display_name: &str) -> Result<ChatId> {
        let name = display_name.trim();
        if name.is_empty() {
            return Err(Error::Command("group name required".into()));
        }
        let user_id = self.user_id()?;
        let value = self.send(&Commands::new_group(user_id, name))?;
        let group_id = value
            .pointer("/groupInfo/groupId")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::Command(format!("group create missing id: {value}")))?;
        Ok(ChatId {
            chat_type: ChatType::Group,
            api_id: group_id,
            scope: None,
        })
    }

    /// Enabled chat-relay ids from `/_servers` (for channel creation).
    pub fn list_chat_relays(&self) -> Result<Vec<ChatRelayInfo>> {
        let user_id = self.user_id()?;
        let value = self.send(&Commands::get_user_servers(user_id))?;
        Ok(parse_chat_relays(&value))
    }

    /// Create a channel (public / relay group). Picks up to 3 enabled relays.
    pub fn create_channel(&self, display_name: &str) -> Result<(ChatId, Option<String>)> {
        let name = display_name.trim();
        if name.is_empty() {
            return Err(Error::Command("channel name required".into()));
        }
        let relays = self.list_chat_relays()?;
        let relay_ids: Vec<i64> = relays.iter().map(|r| r.chat_relay_id).take(3).collect();
        if relay_ids.is_empty() {
            return Err(Error::Command(
                "no chat relays available — enable relays in SimpleX network settings first".into(),
            ));
        }
        let user_id = self.user_id()?;
        let value = self.send(&Commands::new_public_group(user_id, &relay_ids, name))?;
        if value.get("type").and_then(|v| v.as_str()) == Some("publicGroupCreationFailed") {
            return Err(Error::Command(
                "channel creation failed — check relay connectivity".into(),
            ));
        }
        let group_id = value
            .pointer("/groupInfo/groupId")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::Command(format!("channel create missing id: {value}")))?;
        // Prefer the profile short channel link (`/c#`) — same URI official apps share.
        // Full `simplex:/contact#` links make joiners wait on the host forever
        // (channels ignore direct host joins; relays are in short-link data).
        let link = parse_profile_channel_link(&value)
            .or_else(|| parse_group_link(&value))
            .map(|l| shareable_conn_uri(&l));
        if let Some(ref l) = link {
            if !is_channel_short_link(l) {
                return Err(Error::Command(format!(
                    "channel created without a /c# short link (got {l}). Try again or check relays."
                )));
            }
        }
        Ok((
            ChatId {
                chat_type: ChatType::Group,
                api_id: group_id,
            scope: None,
        },
            link,
        ))
    }

    /// List members of a group / channel.
    pub fn delete_member_support_chat(
        &self,
        group_id: i64,
        group_member_id: i64,
    ) -> Result<Value> {
        self.send(&Commands::delete_member_support_chat(
            group_id,
            group_member_id,
        ))
    }

    pub fn list_group_members(&self, group_id: i64) -> Result<Vec<GroupMemberInfo>> {
        let value = self.send(&Commands::list_members(group_id))?;
        Ok(parse_group_members(&value))
    }

    /// Invite a direct contact into the group with the given role.
    pub fn add_group_member(
        &self,
        group_id: i64,
        contact_id: i64,
        role: &str,
    ) -> Result<Value> {
        let role = if role.is_empty() { "member" } else { role };
        self.send(&Commands::add_member(group_id, contact_id, role))
    }

    pub fn set_group_member_role(
        &self,
        group_id: i64,
        member_id: i64,
        role: &str,
    ) -> Result<Value> {
        self.send(&Commands::set_member_role(group_id, &[member_id], role))
    }

    pub fn remove_group_member(
        &self,
        group_id: i64,
        member_id: i64,
        with_messages: bool,
    ) -> Result<Value> {
        self.send(&Commands::remove_members(
            group_id,
            &[member_id],
            with_messages,
        ))
    }

    pub fn leave_group(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::leave_group(group_id))
    }

    /// Accept a group invitation received in a direct chat (`/_join #id`).
    pub fn join_group(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::join_group(group_id))
    }

    pub fn set_member_show_messages(
        &self,
        group_id: i64,
        member_id: i64,
        show_messages: bool,
    ) -> Result<Value> {
        self.send(&Commands::set_member_settings(
            group_id,
            member_id,
            show_messages,
        ))
    }

    pub fn block_group_member(
        &self,
        group_id: i64,
        member_id: i64,
        blocked: bool,
    ) -> Result<Value> {
        self.send(&Commands::block_members(group_id, &[member_id], blocked))
    }

    /// Open (or create) a direct chat with a group member, as the official
    /// “Send direct message” does: `/_create member contact` then
    /// `/_invite member contact`. Returns the new contact id.
    pub fn create_member_contact(&self, group_id: i64, member_id: i64) -> Result<i64> {
        let value = self.send(&Commands::create_member_contact(group_id, member_id))?;
        let contact_id = value
            .pointer("/contact/contactId")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::Command(format!("newMemberContact missing contact: {value}")))?;
        self.send(&Commands::invite_member_contact(contact_id))?;
        Ok(contact_id)
    }

    pub fn accept_group_member(
        &self,
        group_id: i64,
        member_id: i64,
        role: &str,
    ) -> Result<Value> {
        let role = if role.is_empty() { "member" } else { role };
        self.send(&Commands::accept_member(group_id, member_id, role))
    }

    pub fn set_group_alias(&self, group_id: i64, alias: &str) -> Result<Value> {
        self.send(&Commands::set_group_alias(group_id, alias.trim()))
    }

    /// Update group profile (name, welcome message, short descr, image, prefs).
    pub fn update_group_profile(
        &self,
        group_id: i64,
        profile: &crate::models::GroupProfileUpdate,
    ) -> Result<Value> {
        let name = profile.display_name.trim();
        if name.is_empty() {
            return Err(Error::Command("group name required".into()));
        }
        let mut profile = profile.clone();
        profile.display_name = name.to_string();
        profile.welcome_message = profile.welcome_message.trim().to_string();
        profile.short_descr = profile.short_descr.trim().to_string();
        self.send(&Commands::update_group_profile(group_id, &profile))
    }

    /// Ensure a member join link exists and return a displayable URL.
    ///
    /// Channels must use the `/c#` short link created with `/_public group`.
    /// Falling back to `/_create link` produces a private-group `/g#` URI that
    /// official apps reject as `invalidConnReq` (entity id mismatch).
    pub fn group_invite_link(&self, group_id: i64, member_role: &str) -> Result<ConnLink> {
        let role = if member_role.is_empty() {
            "member"
        } else {
            member_role
        };
        let existing = self.send(&Commands::get_group_link(group_id));
        if let Ok(value) = &existing {
            let is_channel = value
                .pointer("/groupInfo/useRelays")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_channel {
                if let Some(link) = parse_profile_channel_link(value)
                    .or_else(|| parse_group_link(value))
                    .filter(|l| is_channel_short_link(l))
                {
                    let link = shareable_conn_uri(&link);
                    return Ok(ConnLink {
                        display: link.clone(),
                        full: link.clone(),
                        short: Some(link),
                    });
                }
                return Err(Error::Command(
                    "this channel has no usable short link (need simplex:/c#… or https://…/c#…). \
Create a new channel — private-group style links won’t join."
                        .into(),
                ));
            }
            if let Some(links) = parse_group_conn_link(value) {
                return Ok(links);
            }
        }
        let created = self.send(&Commands::create_group_link(group_id, role))?;
        parse_group_conn_link(&created)
            .ok_or_else(|| Error::Command(format!("group link missing: {created}")))
    }

    pub fn set_group_link_role(&self, group_id: i64, role: &str) -> Result<Value> {
        self.send(&Commands::set_group_link_role(group_id, role))
    }

    pub fn delete_group_link(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::delete_group_link(group_id))
    }

    pub fn short_group_link(&self, group_id: i64) -> Result<ConnLink> {
        let value = self.send(&Commands::short_group_link(group_id))?;
        parse_group_conn_link(&value)
            .ok_or_else(|| Error::Command(format!("short group link missing: {value}")))
    }

    /// Invite a contact to an audio/video call (signaling).
    pub fn call_invite(&self, contact_id: i64, media: CallMedia) -> Result<Value> {
        self.send(&Commands::call_invite(contact_id, media))
    }

    pub fn call_reject(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::call_reject(contact_id))
    }

    pub fn call_end(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::call_end(contact_id))
    }

    fn user_id(&self) -> Result<i64> {
        self.with_user(|u| u.user_id)
            .ok_or_else(|| Error::Command("no active user".into()))
    }

    pub fn send(&self, cmd: &str) -> Result<Value> {
        tracing::debug!(cmd = %redact_command(cmd), "chat_send_cmd");
        let raw = self.lib.send_cmd(self.ctrl, cmd)?;
        let env: ApiEnvelope = serde_json::from_str(&raw)?;
        env.into_result().map_err(|err| match err {
            Error::Command(message) => {
                // SimpleX error payloads embed full group/contact JSON — keep a
                // short summary so opening chats/settings does not flood the log.
                let brief = truncate_chars(message.as_str(), 160);
                tracing::warn!(cmd = %redact_command(cmd), error = %brief, "chat command failed");
                Error::Command(message)
            }
            other => other,
        })
    }

    /// Spawn a background task that forwards SimpleX events.
    pub fn spawn_receiver(self: &Arc<Self>) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = ReceiveWorker {
            lib: Arc::clone(&self.lib),
            ctrl: CtrlHandle(self.ctrl),
            tx,
        };
        tokio::task::spawn_blocking(move || worker.run());
        rx
    }

    fn ensure_user(&mut self, display_name: &str) -> Result<()> {
        match self.send(Commands::show_active_user()) {
            Ok(value) => {
                self.set_user(parse_user(&value)?);
                Ok(())
            }
            Err(_) => {
                let value = self.send(&Commands::create_active_user(display_name))?;
                self.set_user(parse_user(&value)?);
                Ok(())
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.ctrl.is_null() {
            let _ = self.lib.close_store(self.ctrl);
            self.ctrl = std::ptr::null_mut();
        }
    }
}

fn parse_user(value: &Value) -> Result<UserProfile> {
    let user = value
        .get("user")
        .or_else(|| value.get("fromUser"))
        .or_else(|| value.get("toUser"))
        .ok_or_else(|| Error::Command(format!("activeUser missing user: {value}")))?;
    let user_id = user
        .get("userId")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| Error::Command("user.userId missing".into()))?;
    let profile = parse_profile_fields(user.get("profile"));
    let flag = |k: &str| user.get(k).and_then(|v| v.as_bool()).unwrap_or(true);
    Ok(UserProfile {
        user_id,
        profile,
        send_rcpts_contacts: flag("sendRcptsContacts"),
        send_rcpts_small_groups: flag("sendRcptsSmallGroups"),
    })
}

fn parse_profile_fields(value: Option<&Value>) -> ProfileFields {
    let Some(p) = value else {
        return ProfileFields {
            display_name: "Cursus".into(),
            ..ProfileFields::default()
        };
    };
    ProfileFields {
        display_name: p
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("Cursus")
            .to_string(),
        full_name: p
            .get("fullName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        short_descr: p
            .get("shortDescr")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: p
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        image: p
            .get("image")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.starts_with("data:image/"))
            .map(str::to_string),
        contact_link: p
            .get("contactLink")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        preferences: p.get("preferences").cloned(),
    }
}

fn parse_chat_previews(value: &Value) -> Vec<ChatPreview> {
    let Some(chats) = value.get("chats").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    chats.iter().filter_map(parse_one_chat).collect()
}

/// Request id that still needs Accept/Reject, or `None` if already accepted.
///
/// Mirrors SimpleX clients' `nextAcceptContactRequest`: `contactRequestId` remains
/// on the contact after `/_accept` until the request row is deleted, so presence
/// alone must not keep the Accept bar up.
fn pending_accept_request_id(contact: &Value) -> Option<i64> {
    let req_id = contact
        .get("contactRequestId")
        .or_else(|| contact.pointer("/contactRequest/contactRequestId"))
        .and_then(|v| v.as_i64())?;
    // libsimplex encodes ConnStatus as a sum type: `{"type":"sndReady"}`
    // (not a bare string). Older fixtures may still use a string.
    let status = conn_status_tag(contact.pointer("/activeConn/connStatus").unwrap_or(&Value::Null));
    match status.as_deref() {
        // No connection yet, or still in the pre-accept handshake.
        None | Some("new") | Some("prepared") => Some(req_id),
        // Joined / accepted / snd-ready / ready / … — Accept bar must hide.
        _ => None,
    }
}

/// Extract ConnStatus tag from JSON (`"snd-ready"` or `{"type":"sndReady"}`).
fn conn_status_tag(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    value
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn parse_one_chat(chat: &Value) -> Option<ChatPreview> {
    let info = chat.get("chatInfo")?;
    let type_name = info.get("type")?.as_str()?;
    let mut pending_request_id = None;
    let mut settings = ChatSettings::default();
    let mut tag_ids = Vec::new();
    let mut peer_profile = ProfileFields::default();
    let mut local_alias = String::new();
    let mut is_channel = false;
    let mut membership_role = None;
    let mut group_preferences = None;
    let mut member_admission = None;
    let mut public_group = None;
    let mut contact_preferences = None;
    let mut contact_merged_preferences = None;
    let mut members_require_attention = 0;
    let mut membership_support_unread = 0;
    let mut membership_has_support_chat = false;
    let (chat_type, api_id, title) = match type_name {
        "direct" => {
            let contact = info.get("contact")?;
            let id = contact.get("contactId")?.as_i64()?;
            peer_profile = parse_profile_fields(contact.get("profile"));
            local_alias = contact
                .get("localAlias")
                .or_else(|| contact.pointer("/profile/localAlias"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let peer_name = contact
                .pointer("/profile/displayName")
                .or_else(|| contact.get("localDisplayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Contact")
                .to_string();
            // Prefer the local alias for the list title when set.
            let name = if local_alias.is_empty() {
                peer_name
            } else {
                local_alias.clone()
            };
            // Match official clients' `nextAcceptContactRequest`: contactRequestId
            // stays set after Accept until the request row is deleted, so only
            // treat as pending when there is no usable connection yet.
            pending_request_id = pending_accept_request_id(contact);
            settings = parse_chat_settings(contact.get("chatSettings"));
            tag_ids = parse_tag_ids(contact.get("chatTags"));
            contact_preferences = contact.get("userPreferences").cloned();
            contact_merged_preferences = contact.get("mergedPreferences").cloned();
            (ChatType::Direct, id, name)
        }
        "contactRequest" => {
            // Standalone request row (some core versions).
            let req = info.get("contactRequest")?;
            let req_id = req.get("contactRequestId")?.as_i64()?;
            peer_profile = parse_profile_fields(req.get("profile"));
            let name = req
                .get("localDisplayName")
                .or_else(|| req.pointer("/profile/displayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Request")
                .to_string();
            pending_request_id = Some(req_id);
            (ChatType::Direct, req_id, name)
        }
        "group" => {
            let group = info.get("groupInfo")?;
            let id = group.get("groupId")?.as_i64()?;
            let gp = group
                .get("groupProfile")
                .or_else(|| group.get("profile"));
            peer_profile = parse_profile_fields(gp);
            local_alias = group
                .get("localAlias")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            membership_role = group
                .pointer("/membership/memberRole")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            group_preferences = gp.and_then(|p| p.get("groupPreferences").cloned());
            member_admission = gp.and_then(|p| p.get("memberAdmission").cloned());
            public_group = gp.and_then(|p| p.get("publicGroup").cloned());
            let peer_name = group
                .pointer("/groupProfile/displayName")
                .or_else(|| group.get("localDisplayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Group")
                .to_string();
            let name = if local_alias.is_empty() {
                peer_name
            } else {
                local_alias.clone()
            };
            settings = parse_chat_settings(group.get("chatSettings"));
            tag_ids = parse_tag_ids(group.get("chatTags"));
            is_channel = group
                .get("useRelays")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || group
                    .pointer("/groupProfile/publicGroup/groupType")
                    .and_then(|v| v.as_str())
                    == Some("channel");
            members_require_attention = group
                .get("membersRequireAttention")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            membership_has_support_chat = group.pointer("/membership/supportChat").is_some();
            membership_support_unread = group
                .pointer("/membership/supportChat/unread")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (ChatType::Group, id, name)
        }
        "local" => {
            let folder = info.get("noteFolder")?;
            let id = folder.get("noteFolderId")?.as_i64()?;
            peer_profile.display_name = "Notes".into();
            (ChatType::Local, id, "Notes".into())
        }
        _ => return None,
    };

    let mut preview = chat_list_item_preview(chat.get("chatItems").and_then(|v| v.as_array()));
    if pending_request_id.is_some() && preview.is_empty() {
        preview = "Incoming contact request — accept to chat.".into();
    }
    let unread = chat
        .pointer("/chatStats/unreadCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let unread_chat = chat
        .pointer("/chatStats/unreadChat")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let last_ts = chat
        .get("chatItems")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.pointer("/meta/itemTs"))
        .or_else(|| info.pointer("/contact/chatTs"))
        .or_else(|| info.pointer("/groupInfo/chatTs"))
        .or_else(|| info.pointer("/contact/updatedAt"))
        .or_else(|| info.pointer("/groupInfo/updatedAt"))
        .or_else(|| info.pointer("/noteFolder/chatTs"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let chat_item_ttl = info
        .pointer("/contact/chatItemTTL")
        .or_else(|| info.pointer("/groupInfo/chatItemTTL"))
        .or_else(|| info.get("chatItemTTL"))
        .and_then(|v| v.as_i64());

    Some(ChatPreview {
        id: ChatId::new(chat_type, api_id),
        title,
        preview,
        unread,
        unread_chat,
        settings,
        tag_ids,
        chat_item_ttl,
        pending_request_id,
        peer_profile,
        local_alias,
        is_channel,
        membership_role,
        group_preferences,
        member_admission,
        public_group,
        contact_preferences,
        contact_merged_preferences,
        members_require_attention,
        membership_support_unread,
        membership_has_support_chat,
        last_ts,
    })
}

fn parse_support_chat(value: Option<&Value>) -> Option<crate::models::GroupSupportChat> {
    let sc = value?;
    Some(crate::models::GroupSupportChat {
        unread: sc.get("unread").and_then(|v| v.as_i64()).unwrap_or(0),
        member_attention: sc
            .get("memberAttention")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        mentions: sc.get("mentions").and_then(|v| v.as_i64()).unwrap_or(0),
    })
}

fn parse_one_group_member(m: &Value) -> Option<GroupMemberInfo> {
    let id = m
        .get("groupMemberId")
        .or_else(|| m.pointer("/groupMemberId"))
        .and_then(|v| v.as_i64())?;
    let role = m
        .get("memberRole")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .to_string();
    // Skip relay service accounts in the UI list.
    if role.eq_ignore_ascii_case("relay") {
        return None;
    }
    let status = m
        .get("memberStatus")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // Match official CLI: hide rejected / removed / left.
    if matches!(
        status.as_str(),
        "rejected" | "removed" | "left"
    ) {
        return None;
    }
    // Official GroupMember.chatViewName: memberProfile.localAlias, else displayName.
    // Do not prefer localDisplayName — that is the unique contact name, not the alias.
    let display_name = member_view_name(m);
    let contact_id = m
        .get("memberContactId")
        .and_then(|v| v.as_i64())
        .filter(|id| *id > 0);
    let blocked_by_admin = m
        .get("blockedByAdmin")
        .or_else(|| m.get("memberBlockedByAdmin"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let show_messages = m
        .pointer("/memberSettings/showMessages")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let support_chat = parse_support_chat(m.get("supportChat"));
    let image = nonempty_str(m.pointer("/memberProfile/image"))
        .or_else(|| nonempty_str(m.pointer("/profile/image")));
    let public_name = nonempty_str(m.pointer("/memberProfile/displayName"))
        .or_else(|| nonempty_str(m.pointer("/profile/displayName")))
        .unwrap_or_else(|| display_name.clone());
    let short_descr = nonempty_str(m.pointer("/memberProfile/shortDescr"))
        .or_else(|| nonempty_str(m.pointer("/profile/shortDescr")));
    Some(GroupMemberInfo {
        group_member_id: id,
        contact_id,
        display_name,
        role,
        status,
        blocked_by_admin,
        show_messages,
        support_chat,
        image,
        public_name,
        short_descr,
        is_me: false,
    })
}

fn parse_group_members(value: &Value) -> Vec<GroupMemberInfo> {
    // Official view lists `membership : members` — the current user lives under
    // groupInfo.membership and is not duplicated in the members array.
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let membership = value
        .pointer("/group/groupInfo/membership")
        .or_else(|| value.pointer("/groupInfo/membership"))
        .or_else(|| value.pointer("/membership"));
    if let Some(mut m) = membership.and_then(parse_one_group_member) {
        m.is_me = true;
        seen.insert(m.group_member_id);
        out.push(m);
    }
    let members = value
        .pointer("/group/members")
        .or_else(|| value.get("members"))
        .and_then(|v| v.as_array());
    if let Some(members) = members {
        for m in members {
            let Some(info) = parse_one_group_member(m) else {
                continue;
            };
            if seen.insert(info.group_member_id) {
                out.push(info);
            }
        }
    }
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Channel short links use contact-conn type `C` → `/c#` (not private-group `/g#`).
pub fn is_channel_short_link(link: &str) -> bool {
    let l = link.trim();
    (l.starts_with("https://") || l.starts_with("simplex:/")) && l.contains("/c#")
}

/// Any SimpleX short invite / address / group / channel URI (`https://` or `simplex:/`).
pub fn is_short_conn_link(link: &str) -> bool {
    let l = link.trim();
    let kind = l.contains("/g#")
        || l.contains("/c#")
        || l.contains("/i#")
        || l.contains("/a#")
        || l.contains("/r#");
    kind && (l.starts_with("https://") || l.starts_with("simplex:/"))
}

/// Match official `simplexChatLink`: rewrite full `simplex:/…` URIs for sharing;
/// leave short links untouched (converting `simplex:/c#` → `https://simplex.chat/c#` breaks hosts).
pub fn shareable_conn_uri(link: &str) -> String {
    let l = link.trim();
    if is_short_conn_link(l) {
        return l.to_string();
    }
    if let Some(rest) = l.strip_prefix("simplex:/") {
        format!("https://simplex.chat/{rest}")
    } else {
        l.to_string()
    }
}

fn parse_profile_channel_link(value: &Value) -> Option<String> {
    // Official ChannelLinkQRCodeSection shares `groupProfile.publicGroup.groupLink`.
    value
        .pointer("/groupInfo/groupProfile/publicGroup/groupLink")
        .or_else(|| value.pointer("/groupProfile/publicGroup/groupLink"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Private-group link in both forms (short when the core has one, full always).
fn parse_group_conn_link(value: &Value) -> Option<ConnLink> {
    let conn = value
        .pointer("/groupLink/connLinkContact")
        .or_else(|| value.pointer("/groupLink/connLinkInvitation"))
        .or_else(|| value.get("connLinkContact"))?;
    let full = conn
        .get("connFullLink")
        .and_then(|v| v.as_str())
        .map(shareable_conn_uri);
    let short = conn
        .get("connShortLink")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(shareable_conn_uri);
    let full = full.or_else(|| short.clone())?;
    Some(ConnLink {
        display: short.clone().unwrap_or_else(|| full.clone()),
        full,
        short,
    })
}

fn parse_group_link(value: &Value) -> Option<String> {
    // Prefer short links (channels need relay metadata embedded in `/c#` data).
    value
        .pointer("/groupLink/connLinkContact/connShortLink")
        .or_else(|| value.pointer("/groupLink/connLinkInvitation/connShortLink"))
        .or_else(|| value.pointer("/connLinkContact/connShortLink"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| parse_profile_channel_link(value))
        .or_else(|| {
            value
                .pointer("/groupLink/connLinkContact/connFullLink")
                .or_else(|| value.pointer("/groupLink/connLinkInvitation/connFullLink"))
                .or_else(|| value.pointer("/connLinkContact/connFullLink"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

fn parse_chat_relays(value: &Value) -> Vec<ChatRelayInfo> {
    let mut out = Vec::new();
    let Some(servers) = value.get("userServers").and_then(|v| v.as_array()) else {
        return out;
    };
    for op in servers {
        // Skip disabled operators when present.
        if op
            .pointer("/operator/enabled")
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            continue;
        }
        let Some(relays) = op.get("chatRelays").and_then(|v| v.as_array()) else {
            continue;
        };
        for relay in relays {
            let enabled = relay
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let deleted = relay
                .get("deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !enabled || deleted {
                continue;
            }
            let Some(id) = relay.get("chatRelayId").and_then(|v| v.as_i64()) else {
                continue;
            };
            let display_name = relay
                .get("displayName")
                .or_else(|| relay.pointer("/relayProfile/displayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Relay")
                .to_string();
            out.push(ChatRelayInfo {
                chat_relay_id: id,
                display_name,
            });
        }
    }
    out
}

fn parse_chat_settings(value: Option<&Value>) -> ChatSettings {
    let Some(value) = value else {
        return ChatSettings::default();
    };
    let enable_ntfs = value
        .get("enableNtfs")
        .and_then(|v| v.as_str())
        .map(NtfFilter::parse)
        .unwrap_or(NtfFilter::All);
    let favorite = value
        .get("favorite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let send_rcpts = value.get("sendRcpts").and_then(|v| {
        if v.is_null() {
            None
        } else {
            v.as_bool()
        }
    });
    ChatSettings {
        enable_ntfs,
        favorite,
        send_rcpts,
    }
}

fn parse_tag_ids(value: Option<&Value>) -> Vec<i64> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default()
}

fn parse_chat_tags(value: &Value) -> Vec<ChatTag> {
    let Some(tags) = value
        .get("userTags")
        .or_else(|| value.get("chatTags"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    tags.iter()
        .filter_map(|tag| {
            let id = tag.get("chatTagId")?.as_i64()?;
            let text = tag
                .get("chatTagText")
                .and_then(|v| v.as_str())
                .unwrap_or("List")
                .to_string();
            let emoji = tag
                .get("chatTagEmoji")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(ChatTag { id, text, emoji })
        })
        .collect()
}

fn parse_chat_messages(value: &Value) -> Vec<ChatMessage> {
    let Some(items) = value
        .pointer("/chat/chatItems")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    items.iter().filter_map(parse_one_message).collect()
}

fn parse_one_message(item: &Value) -> Option<ChatMessage> {
    let item_id = item.pointer("/meta/itemId")?.as_i64()?;
    let deleted = parse_deleted_state(item);
    let mut text = item
        .pointer("/meta/itemText")
        .and_then(|v| v.as_str())
        .or_else(|| item.pointer("/content/msgContent/text").and_then(|v| v.as_str()))
        .or_else(|| item.pointer("/content/text").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let dir = item
        .pointer("/chatDir/type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let outgoing = matches!(
        dir,
        "directSnd" | "groupSnd" | "localSnd" | "snd" | "groupSndRcv"
    ) || dir.contains("Snd");
    let created_at = item
        .pointer("/meta/itemTs")
        .or_else(|| item.pointer("/meta/createdAt"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let delivery = item
        .pointer("/meta/itemStatus")
        .map(DeliveryStatus::parse)
        .unwrap_or(if outgoing {
            DeliveryStatus::Pending
        } else {
            DeliveryStatus::Delivered
        });
    let file = parse_file_info(item);
    let group_invitation = parse_group_invitation(item);
    // File-only items often have empty itemText — synthesize a label.
    if text.is_empty() {
        if deleted.is_some() {
            // Keep the row so the UI can show “marked deleted” / “deleted”.
        } else if let Some(inv) = group_invitation.as_ref() {
            text = if outgoing {
                format!("You invited them to join {}", inv.group_name)
            } else {
                format!("Invited to join {} as {}", inv.group_name, inv.member_role)
            };
        } else {
            let f = file.as_ref()?;
            text = f.file_name.clone();
        }
    } else if text.starts_with('📎') {
        // Older rows / core itemText may still prefix a paperclip glyph.
        text = text.trim_start_matches('📎').trim().to_string();
    }
    let segments = crate::rich_text::parse_formatted_text(item.get("formattedText"));
    // Expand leftover `:shortcode:` in plain text (and plain segments).
    let text = crate::rich_text::expand_emoji_shortcodes(&text);
    let segments: Vec<_> = segments
        .into_iter()
        .map(|mut seg| {
            if matches!(seg.style, crate::rich_text::TextStyle::Plain) {
                seg.text = crate::rich_text::expand_emoji_shortcodes(&seg.text);
            }
            seg
        })
        .collect();
    let reactions = if deleted.is_some() {
        Vec::new()
    } else {
        parse_reactions(item.get("reactions"))
    };
    let quote = parse_quote(item.get("quotedItem"));
    let is_announcement = is_announcement_content(item);
    let sender = if outgoing || is_announcement {
        None
    } else {
        parse_message_sender(item)
    };
    // Group events often need the member name in front (SimpleX CIEventView).
    let text = if is_announcement {
        format_announcement_text(item, &text)
    } else {
        text
    };
    let edited = item
        .pointer("/meta/itemEdited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let editable = item
        .pointer("/meta/editable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Some(ChatMessage {
        item_id,
        text,
        segments,
        outgoing,
        created_at,
        delivery,
        file,
        reactions,
        quote,
        deleted,
        sender,
        is_announcement,
        group_invitation,
        edited,
        editable,
    })
}

fn parse_group_invitation(item: &Value) -> Option<crate::models::GroupInvitationInfo> {
    let content = item.get("content")?;
    let ty = content.get("type")?.as_str()?;
    if ty != "rcvGroupInvitation" && ty != "sndGroupInvitation" {
        return None;
    }
    let inv = content.get("groupInvitation")?;
    let group_id = inv.get("groupId")?.as_i64()?;
    let group_name = inv
        .pointer("/groupProfile/displayName")
        .or_else(|| inv.get("localDisplayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("Group")
        .to_string();
    let member_role = content
        .get("memberRole")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .to_string();
    let status = inv
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending")
        .to_string();
    Some(crate::models::GroupInvitationInfo {
        group_id,
        group_name,
        member_role,
        status,
    })
}

/// Non-message protocol items → centered announcement rows in the UI.
fn is_announcement_content(item: &Value) -> bool {
    matches!(
        item.pointer("/content/type").and_then(|v| v.as_str()),
        Some(
            "rcvGroupEvent"
                | "sndGroupEvent"
                | "rcvDirectEvent"
                | "rcvConnEvent"
                | "sndConnEvent"
                | "rcvChatFeature"
                | "sndChatFeature"
                | "rcvChatPreference"
                | "sndChatPreference"
                | "rcvGroupFeature"
                | "sndGroupFeature"
                | "rcvChatFeatureRejected"
                | "rcvGroupFeatureRejected"
                | "rcvDirectE2EEInfo"
                | "sndDirectE2EEInfo"
                | "rcvGroupE2EEInfo"
                | "sndGroupE2EEInfo"
                | "chatBanner"
        )
    )
}

fn format_announcement_text(item: &Value, item_text: &str) -> String {
    let member_name = parse_message_sender(item).map(|s| s.display_name);
    let Some(name) = member_name else {
        return item_text.to_string();
    };
    if item_text.is_empty() {
        return name;
    }
    if item_text
        .get(..name.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(&name))
    {
        item_text.to_string()
    } else {
        format!("{name} {item_text}")
    }
}

fn parse_message_sender(item: &Value) -> Option<crate::models::MessageSender> {
    let dir = item.get("chatDir")?;
    let dir_type = dir.get("type").and_then(|v| v.as_str()).unwrap_or("");
    // groupRcv embeds the author; channelRcv is the channel itself (no member).
    if !dir_type.eq_ignore_ascii_case("groupRcv") && !dir_type.contains("GroupRcv") {
        return None;
    }
    let member = dir.get("groupMember")?;
    let group_member_id = member.get("groupMemberId")?.as_i64()?;
    let display_name = member_view_name(member);
    let contact_id = member
        .get("memberContactId")
        .and_then(|v| v.as_i64())
        .filter(|id| *id > 0);
    let image = member
        .pointer("/profile/image")
        .or_else(|| member.pointer("/memberProfile/image"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.starts_with("data:image/"))
        .map(str::to_string);
    let role = member
        .get("memberRole")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .to_string();
    Some(crate::models::MessageSender {
        group_member_id,
        contact_id,
        display_name,
        image,
        role,
    })
}

/// Official `chatViewName` for a group member JSON object.
fn member_view_name(member: &Value) -> String {
    nonempty_str(member.pointer("/memberProfile/localAlias"))
        .or_else(|| nonempty_str(member.pointer("/profile/localAlias")))
        .or_else(|| nonempty_str(member.get("localAlias")))
        .or_else(|| nonempty_str(member.pointer("/memberProfile/displayName")))
        .or_else(|| nonempty_str(member.pointer("/profile/displayName")))
        .or_else(|| nonempty_str(member.get("localDisplayName")))
        .unwrap_or_else(|| "Member".into())
}

fn nonempty_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_deleted_state(item: &Value) -> Option<crate::models::DeletedState> {
    use crate::models::DeletedState;
    if item.pointer("/meta/itemDeleted").is_some()
        && !item
            .pointer("/meta/itemDeleted")
            .map(|v| v.is_null())
            .unwrap_or(true)
    {
        return Some(DeletedState::Marked);
    }
    match item.pointer("/content/type").and_then(|v| v.as_str()) {
        Some("sndDeleted" | "rcvDeleted") => Some(DeletedState::Content),
        _ => None,
    }
}

/// List-row preview for the latest chat item.
///
/// Deleted / marked-deleted items must not leak `itemText` (invite links etc.);
/// match official clients with a short tombstone label instead.
fn chat_list_item_preview(items: Option<&Vec<Value>>) -> String {
    use crate::models::DeletedState;
    let Some(items) = items else {
        return String::new();
    };
    let Some(item) = items.first() else {
        return String::new();
    };
    match parse_deleted_state(item) {
        Some(DeletedState::Content) => return "deleted".into(),
        Some(DeletedState::Marked) => return "marked deleted".into(),
        None => {}
    }
    let preview = item
        .pointer("/meta/itemText")
        .or_else(|| item.pointer("/content/msgContent/text"))
        .or_else(|| item.pointer("/content/text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    crate::rich_text::expand_emoji_shortcodes(&preview)
}

fn parse_quote(value: Option<&Value>) -> Option<crate::models::MessageQuote> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let text = value
        .pointer("/content/text")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("text").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let text = crate::rich_text::expand_emoji_shortcodes(&text);
    if text.is_empty() {
        // Image/file quotes still get a stub label when possible.
        let stub = value
            .pointer("/content/type")
            .and_then(|v| v.as_str())
            .unwrap_or("message");
        if stub == "text" {
            return None;
        }
    }
    let dir = value
        .pointer("/chatDir/type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let outgoing = dir.contains("Snd");
    let item_id = value.get("itemId").and_then(|v| v.as_i64());
    let text = if text.is_empty() {
        match value.pointer("/content/type").and_then(|v| v.as_str()) {
            Some("image") => "Photo".into(),
            Some("video") => "Video".into(),
            Some("voice") => "Voice message".into(),
            Some("file") => "File".into(),
            _ => "Message".into(),
        }
    } else {
        text
    };
    Some(crate::models::MessageQuote {
        item_id,
        text,
        outgoing,
    })
}

fn parse_reactions(value: Option<&Value>) -> Vec<ReactionCount> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let emoji = item
                .pointer("/reaction/emoji")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    // Some encodings nest differently; tolerate string reaction.
                    item.pointer("/reaction")
                        .and_then(|v| v.as_str())
                })?;
            let emoji = normalize_reaction_emoji(emoji)?.to_string();
            let user_reacted = item
                .get("userReacted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let total = item
                .get("totalReacted")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if total == 0 {
                return None;
            }
            Some(ReactionCount {
                emoji,
                user_reacted,
                total,
            })
        })
        .collect()
}

/// Map display forms (❤️) to the protocol char (❤); reject unknowns.
fn normalize_reaction_emoji(emoji: &str) -> Option<&'static str> {
    let trimmed = emoji.trim();
    if trimmed == "❤" || trimmed == "❤️" || trimmed == "\u{2764}\u{FE0F}" {
        return Some("❤");
    }
    KNOWN_REACTIONS.iter().copied().find(|e| *e == trimmed)
}

fn parse_address_auto_accept(value: &Value) -> bool {
    value
        .pointer("/contactLink/addressSettings/autoAccept")
        .or_else(|| value.pointer("/addressSettings/autoAccept"))
        .map(|v| !v.is_null())
        .unwrap_or(false)
}

fn parse_reaction_members(value: &Value) -> Vec<ReactionMember> {
    let Some(arr) = value
        .get("memberReactions")
        .or_else(|| value.pointer("/reactionMembers/memberReactions"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| {
            let member = row.get("groupMember")?;
            let name = member_view_name(member);
            if name == "Member" && nonempty_str(member.get("localDisplayName")).is_none() {
                return None;
            }
            Some(ReactionMember {
                display_name: name,
            })
        })
        .collect()
}

fn is_image_name(name: &str) -> bool {
    matches!(
        std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp")
    )
}

fn is_audio_name(name: &str) -> bool {
    matches!(
        std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("m4a" | "aac" | "mp3" | "ogg" | "opus" | "wav" | "flac")
    )
}

fn is_video_name(name: &str) -> bool {
    matches!(
        std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "mkv" | "webm" | "m4v" | "avi" | "3gp" | "3g2" | "mpeg" | "mpg" | "ts")
    )
}

fn parse_file_info(item: &Value) -> Option<FileInfo> {
    let file = item.get("file")?;
    let file_id = file.get("fileId")?.as_i64()?;
    let file_name = file
        .get("fileName")
        .and_then(|v| v.as_str())
        .unwrap_or("file")
        .to_string();
    let file_size = file.get("fileSize").and_then(|v| v.as_i64()).unwrap_or(0);
    let status_ty = file
        .pointer("/fileStatus/type")
        .and_then(|v| v.as_str())
        .or_else(|| file.get("fileStatus").and_then(|v| v.as_str()))
        .unwrap_or("");
    let state = match status_ty {
        "rcvInvitation" => FileTransferState::Offer,
        "rcvAccepted" | "rcvTransfer" | "sndTransfer" | "sndStored" => FileTransferState::InProgress,
        "rcvComplete" | "sndComplete" => FileTransferState::Complete,
        "rcvCancelled" | "sndCancelled" | "rcvError" | "sndError" | "rcvAborted"
        | "rcvWarning" | "sndWarning" => FileTransferState::Failed,
        _ => FileTransferState::Other,
    };
    let (transferred, transfer_total) = match status_ty {
        "rcvTransfer" => (
            file.pointer("/fileStatus/rcvProgress")
                .and_then(|v| v.as_i64()),
            file.pointer("/fileStatus/rcvTotal")
                .and_then(|v| v.as_i64()),
        ),
        "sndTransfer" => (
            file.pointer("/fileStatus/sndProgress")
                .and_then(|v| v.as_i64()),
            file.pointer("/fileStatus/sndTotal")
                .and_then(|v| v.as_i64()),
        ),
        _ => (None, None),
    };
    let error_detail = file
        .pointer("/fileStatus/rcvFileError/fileError/type")
        .or_else(|| file.pointer("/fileStatus/sndFileError/fileError/type"))
        .or_else(|| file.pointer("/fileStatus/rcvFileError/type"))
        .or_else(|| file.pointer("/fileStatus/sndFileError/type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            if matches!(state, FileTransferState::Failed) {
                Some(status_ty.to_string())
            } else {
                None
            }
        });
    let kind = msg_content_kind(item).unwrap_or_else(|| {
        if is_image_name(&file_name) {
            FileKind::Image
        } else if is_video_name(&file_name) {
            FileKind::Video
        } else if is_audio_name(&file_name) {
            FileKind::Voice
        } else {
            FileKind::File
        }
    });
    let duration_secs = item
        .pointer("/content/msgContent/duration")
        .or_else(|| item.pointer("/msgContent/duration"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let crypto_args = file.pointer("/fileSource/cryptoArgs");
    let crypto_key = crypto_args
        .and_then(|v| v.get("fileKey").or_else(|| v.get("key")))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let crypto_nonce = crypto_args
        .and_then(|v| v.get("fileNonce").or_else(|| v.get("nonce")))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let encrypted = crypto_key.is_some() && crypto_nonce.is_some();
    let local_path = file
        .pointer("/fileSource/filePath")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(resolve_local_file)
        .or_else(|| {
            // Incoming complete rows sometimes omit fileSource briefly; fall back
            // to the configured files folder + display name when complete.
            if matches!(state, FileTransferState::Complete) {
                Some(files_dir().join(&file_name))
            } else {
                None
            }
        });
    Some(FileInfo {
        file_id,
        file_name,
        file_size,
        state,
        kind,
        duration_secs,
        local_path,
        encrypted,
        crypto_key,
        crypto_nonce,
        transferred,
        transfer_total,
        error_detail,
    })
}

fn msg_content_kind(item: &Value) -> Option<FileKind> {
    let ty = item
        .pointer("/content/msgContent/type")
        .or_else(|| item.pointer("/msgContent/type"))
        .and_then(|v| v.as_str())?;
    Some(FileKind::from_msg_content_type(ty))
}

/// Auto-receive candidates from a live core event (official: `newChatItems` only).
pub fn auto_receive_candidates(event: &Event) -> Vec<(i64, String)> {
    let Event::NewChatItems(value) = event else {
        return Vec::new();
    };
    let Some(items) = value.get("chatItems").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items.iter().filter_map(auto_receive_from_chat_item).collect()
}

/// Incoming file offers the official client would auto-accept from a chat-item JSON blob.
pub fn auto_receive_from_chat_item(aci: &Value) -> Option<(i64, String)> {
    let item = aci.get("chatItem").unwrap_or(aci);
    // Only inbound.
    let dir = item
        .pointer("/chatDir/type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let outgoing = matches!(
        dir,
        "directSnd" | "groupSnd" | "localSnd" | "snd" | "groupSndRcv"
    ) || dir.contains("Snd");
    if outgoing {
        return None;
    }
    let file = parse_file_info(item)?;
    if file.should_auto_receive() {
        Some((file.file_id, file.file_name))
    } else {
        None
    }
}

struct PreparedOutgoingFile {
    /// Path embedded in the send command (relative name under `files/` when encrypted).
    cmd_path: String,
    crypto: Option<(String, String)>,
}

fn unique_files_name(preferred: &str) -> String {
    let preferred = preferred.trim();
    let preferred = if preferred.is_empty() {
        "file.bin"
    } else {
        preferred
    };
    let candidate = files_dir().join(preferred);
    if !candidate.exists() {
        return preferred.to_string();
    }
    let path = std::path::Path::new(preferred);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for i in 1..10_000 {
        let name = format!("{stem}-{i}{ext}");
        if !files_dir().join(&name).exists() {
            return name;
        }
    }
    format!(
        "{stem}-{}{ext}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

fn ensure_private_dir(path: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| Error::Command(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Command for logs: the verb plus chat / item references (`@5`, `#3`,
/// numbers, `key=value` flags), never free text, JSON, links or file paths.
/// Commands carry message bodies, profiles, invite links and file names.
pub(crate) fn redact_command(cmd: &str) -> String {
    let mut out = String::new();
    let mut hidden = 0usize;
    for (i, tok) in cmd.split_whitespace().enumerate() {
        let is_ref = |t: &str| {
            let body = t.trim_start_matches(['@', '#', '*', ':']);
            !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '(' || c == ')')
        };
        let is_flag = |t: &str| {
            t.split_once('=').is_some_and(|(k, v)| {
                k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && v.len() <= 8
                    && v.chars().all(|c| c.is_ascii_alphanumeric())
            })
        };
        let keep = i == 0 || (hidden == 0 && (is_ref(tok) || is_flag(tok) || tok == "json" || tok == "chat"));
        if keep {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(tok);
        } else {
            hidden += tok.len() + 1;
        }
    }
    if hidden > 0 {
        out.push_str(&format!(" …({hidden} bytes hidden)"));
    }
    out
}

/// Result of [`Session::receive_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveOutcome {
    Accepted,
    /// The file sits on servers we don't use (hostnames). Downloading would
    /// show them our IP address; ask the user, then retry approved.
    NeedsApproval { servers: Vec<String> },
}

/// Hostnames from a `fileNotApproved` chat error, if that's what this is.
fn file_not_approved_servers(err: &Value) -> Option<Vec<String>> {
    let et = err.get("errorType").unwrap_or(err);
    if et.get("type").and_then(|v| v.as_str()) != Some("fileNotApproved") {
        return None;
    }
    let servers = et
        .get("unknownServers")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str())
                .map(|s| {
                    // `xftp://fingerprint@host:port` → `host`
                    let host = s.rsplit('@').next().unwrap_or(s);
                    host.split([':', '/', ',']).next().unwrap_or(host).to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(servers)
}

/// Best-effort removal of everything inside `path` (the directory stays).
fn wipe_dir_contents(path: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        // `symlink_metadata`: never follow a link out of our directory.
        let is_dir = std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir());
        if is_dir {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Delete decrypted attachments, playback transcodes and unsent voice
/// recordings (`tmp/plain`, `tmp/play`, `tmp/rec`). Call when the session ends
/// so plaintext media never stays on disk after the app quits. XFTP staging
/// is left alone (may be mid-transfer); it is wiped on the next session open.
pub fn wipe_plaintext_cache() {
    let tmp = temp_dir();
    for sub in ["plain", "play", "rec"] {
        wipe_dir_contents(&tmp.join(sub));
    }
    if let Some(ram) = runtime_cache_dir() {
        wipe_dir_contents(&ram);
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_command;

    #[test]
    fn file_not_approved_lists_server_hosts() {
        let err = serde_json::json!({"type": "error", "errorType": {
            "type": "fileNotApproved", "fileId": 9,
            "unknownServers": ["xftp://abc=@files.evil.example:443", "xftp://k@other.org"]
        }});
        assert_eq!(
            super::file_not_approved_servers(&err),
            Some(vec!["files.evil.example".to_string(), "other.org".to_string()])
        );
        let other = serde_json::json!({"type": "error", "errorType": {"type": "fileCancelled"}});
        assert_eq!(super::file_not_approved_servers(&other), None);
    }

    #[test]
    fn wipe_empties_dir_without_following_symlinks() {
        let root = std::env::temp_dir().join(format!("cursus-wipe-{}", std::process::id()));
        let inside = root.join("plain");
        let outside = root.join("keep");
        std::fs::create_dir_all(inside.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(inside.join("photo.jpg"), b"x").unwrap();
        std::fs::write(inside.join("sub/voice.wav"), b"x").unwrap();
        std::fs::write(outside.join("precious.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();
        super::wipe_dir_contents(&inside);
        assert_eq!(std::fs::read_dir(&inside).unwrap().count(), 0);
        assert!(outside.join("precious.txt").is_file(), "followed a symlink");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn logged_commands_never_contain_content() {
        let send = r#"/_send @5 json [{"msgContent":{"type":"text","text":"my secret"}}]"#;
        let r = redact_command(send);
        assert!(r.starts_with("/_send @5 json"), "{r}");
        assert!(!r.contains("secret"), "{r}");
        let link = redact_command("/connect https://smp9.simplex.im/i#abcdef");
        assert!(!link.contains("simplex.im"), "{link}");
        assert_eq!(redact_command("/_get chat #3 count=100"), "/_get chat #3 count=100");
        let alias = redact_command("/_set alias @7 Mum");
        assert!(!alias.contains("Mum"), "{alias}");
        let profile = redact_command(r#"/_profile 1 {"displayName":"Davide"}"#);
        assert!(!profile.contains("Davide"), "{profile}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        chat_list_item_preview, parse_file_info, parse_one_message, pending_accept_request_id,
    };
    use crate::models::FileTransferState;
    use serde_json::json;

    #[test]
    fn chat_list_preview_hides_marked_deleted_content() {
        let items = vec![json!({
            "meta": {
                "itemId": 1,
                "itemText": "https://smp12.simplex.im/i#secret",
                "itemDeleted": { "type": "deleted", "deletedTs": "2026-11-05T10:00:00Z" }
            },
            "content": { "type": "sndMsgContent", "msgContent": { "type": "text", "text": "https://smp12.simplex.im/i#secret" } }
        })];
        assert_eq!(chat_list_item_preview(Some(&items)), "marked deleted");
    }

    #[test]
    fn chat_list_preview_hides_full_deleted_content() {
        let items = vec![json!({
            "meta": { "itemId": 2, "itemText": "still here" },
            "content": { "type": "sndDeleted", "deleted": { "type": "deleted" } }
        })];
        assert_eq!(chat_list_item_preview(Some(&items)), "deleted");
    }

    #[test]
    fn chat_list_preview_keeps_live_text() {
        let items = vec![json!({
            "meta": { "itemId": 3, "itemText": "hello" },
            "content": { "type": "sndMsgContent", "msgContent": { "type": "text", "text": "hello" } }
        })];
        assert_eq!(chat_list_item_preview(Some(&items)), "hello");
    }

    #[test]
    fn pending_when_request_has_no_connection() {
        let contact = json!({ "contactRequestId": 7 });
        assert_eq!(pending_accept_request_id(&contact), Some(7));
    }

    #[test]
    fn pending_when_connection_still_new() {
        let contact = json!({
            "contactRequestId": 7,
            "activeConn": { "connStatus": { "type": "new" } }
        });
        assert_eq!(pending_accept_request_id(&contact), Some(7));
    }

    #[test]
    fn not_pending_after_accept_snd_ready() {
        // Core keeps contactRequestId after Accept; connStatus advances.
        // JSON uses sum-type encoding: {"type":"sndReady"}.
        let contact = json!({
            "contactRequestId": 7,
            "activeConn": { "connStatus": { "type": "sndReady" } }
        });
        assert_eq!(pending_accept_request_id(&contact), None);
    }

    #[test]
    fn not_pending_after_ready_string_status() {
        let contact = json!({
            "contactRequestId": 7,
            "activeConn": { "connStatus": "ready" }
        });
        assert_eq!(pending_accept_request_id(&contact), None);
    }

    #[test]
    fn not_pending_without_request_id() {
        let contact = json!({ "activeConn": { "connStatus": { "type": "new" } } });
        assert_eq!(pending_accept_request_id(&contact), None);
    }

    #[test]
    fn member_view_name_prefers_local_alias() {
        let m = json!({
            "groupMemberId": 9,
            "memberRole": "member",
            "memberStatus": "complete",
            "localDisplayName": "alice",
            "memberProfile": {
                "displayName": "alice",
                "localAlias": "Ali"
            }
        });
        let parsed = super::parse_one_group_member(&m).expect("member");
        assert_eq!(parsed.display_name, "Ali");
    }

    #[test]
    fn group_event_is_announcement() {
        let item = json!({
            "chatDir": {
                "type": "groupRcv",
                "groupMember": {
                    "groupMemberId": 3,
                    "localDisplayName": "Ada",
                    "memberRole": "member",
                    "profile": { "displayName": "Ada" }
                }
            },
            "meta": {
                "itemId": 42,
                "itemTs": "2026-11-03T16:37:00Z",
                "itemText": "updated group profile",
                "itemStatus": { "type": "rcvRead" }
            },
            "content": {
                "type": "rcvGroupEvent",
                "rcvGroupEvent": { "type": "groupUpdated", "groupProfile": {} }
            }
        });
        let msg = parse_one_message(&item).expect("event");
        assert!(msg.is_announcement);
        assert!(msg.text.contains("Ada"));
        assert!(msg.text.contains("updated group profile"));
        assert!(msg.sender.is_none());
    }

    #[test]
    fn file_info_resolves_relative_source_path() {
        let item = json!({
            "file": {
                "fileId": 9,
                "fileName": "shot.jpg",
                "fileSize": 12,
                "fileStatus": { "type": "rcvComplete" },
                "fileSource": { "filePath": "shot.jpg", "cryptoArgs": null }
            }
        });
        let info = parse_file_info(&item).expect("file");
        assert_eq!(info.state, FileTransferState::Complete);
        assert!(!info.encrypted);
        let path = info.local_path.expect("path");
        assert!(path.ends_with("files/shot.jpg") || path.ends_with("files\\shot.jpg"));
    }

    #[test]
    fn file_info_parses_crypto_args() {
        let item = json!({
            "file": {
                "fileId": 3,
                "fileName": "secret.bin",
                "fileSize": 99,
                "fileStatus": { "type": "rcvComplete" },
                "fileSource": {
                    "filePath": "secret.bin",
                    "cryptoArgs": { "fileKey": "abcKEY", "fileNonce": "xyzNONCE" }
                }
            }
        });
        let info = parse_file_info(&item).expect("file");
        assert!(info.encrypted);
        assert_eq!(info.crypto_key.as_deref(), Some("abcKEY"));
        assert_eq!(info.crypto_nonce.as_deref(), Some("xyzNONCE"));
    }

    #[test]
    fn auto_receive_small_image_offer() {
        use super::auto_receive_from_chat_item;
        use crate::models::MAX_IMAGE_SIZE_AUTO_RCV;
        let aci = json!({
            "chatItem": {
                "chatDir": { "type": "directRcv" },
                "content": { "msgContent": { "type": "image", "text": "", "image": "" } },
                "file": {
                    "fileId": 42,
                    "fileName": "pic.jpg",
                    "fileSize": MAX_IMAGE_SIZE_AUTO_RCV,
                    "fileStatus": { "type": "rcvInvitation" }
                }
            }
        });
        assert_eq!(
            auto_receive_from_chat_item(&aci),
            Some((42, "pic.jpg".into()))
        );
    }

    #[test]
    fn no_auto_receive_generic_file() {
        use super::auto_receive_from_chat_item;
        let aci = json!({
            "chatItem": {
                "chatDir": { "type": "directRcv" },
                "content": { "msgContent": { "type": "file", "text": "" } },
                "file": {
                    "fileId": 7,
                    "fileName": "doc.pdf",
                    "fileSize": 1024,
                    "fileStatus": { "type": "rcvInvitation" }
                }
            }
        });
        assert_eq!(auto_receive_from_chat_item(&aci), None);
    }
}
