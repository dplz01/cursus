//! An open SimpleX chat session: every command the apps send goes through
//! [`Session`]; replies are turned into models by [`crate::parse`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::commands::{Commands, OutgoingFile};
use crate::error::{Error, Result};
use crate::ffi::LibSimplex;
use crate::invitations::{self, Invitation};
use crate::json::JsonExt;
use crate::models::{
    parse_event, ApiEnvelope, CallMedia, CallRecord, ChatId, ChatMessage, ChatPreview, ChatRelayInfo,
    ChatSettings, ChatTag, ChatType, ConnLink, Event, GroupMemberInfo, GroupProfileUpdate, MessageInfo,
    ProfileFields, ReactionMember, UserProfile,
};
use crate::parse;
use crate::paths::{
    data_dir, db_prefix, default_libs_dir, ensure_private_dir, files_dir, fits_in_ram_cache, free_bytes,
    plaintext_cache_name, plaintext_cache_path, runtime_cache_dir, temp_dir, wipe_dir_contents,
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
}

/// Never print the database key (logs, panics, debuggers).
impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("libs_dir", &self.libs_dir)
            .field("db_prefix", &self.db_prefix)
            .field("db_key", &"***")
            .finish_non_exhaustive()
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
        }
    }
}

/// Open SimpleX chat session backed by `libsimplex`.
pub struct Session {
    lib: Arc<LibSimplex>,
    ctrl: *mut std::ffi::c_void,
    /// The active user, kept current by every call that changes it.
    user: Mutex<Option<UserProfile>>,
    /// Mirror of `/_files_encrypt`: attachments are stored as CryptoFiles.
    encrypt_local_files: AtomicBool,
    /// Unused own one-time links, from the last chat-list load and kept up
    /// to date by create / rename / delete (see `crate::invitations`).
    pending: Mutex<Vec<Invitation>>,
}

// The chat controller is used concurrently from command and receive
// threads, as in the official clients.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

/// A poisoned lock only means another thread panicked mid-update; the data
/// (a cached profile, a list of links) is still the best we have.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn command_error(message: impl Into<String>) -> Error {
    Error::Command(message.into())
}

/// `role`, or `member` when none was chosen.
fn role_or_member(role: &str) -> &str {
    if role.is_empty() {
        "member"
    } else {
        role
    }
}

impl Session {
    pub fn open(config: SessionConfig) -> Result<Self> {
        // Owner-only data dir: SQLCipher DBs, log and prefs live here.
        ensure_private_dir(&data_dir())?;
        ensure_private_dir(&files_dir())?;
        // Nothing decrypted or half-transferred survives from the last run
        // (crash, SIGKILL): attachments are only ever plaintext for a session.
        wipe_dir_contents(&temp_dir());
        if let Some(ram) = runtime_cache_dir() {
            wipe_dir_contents(&ram);
        }
        ensure_private_dir(&temp_dir().join("plain"))?;
        if let Some(parent) = config.db_prefix.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::info!(libs = %config.libs_dir.display(), "session open: load libsimplex");
        let lib = LibSimplex::shared(Some(&config.libs_dir))?;
        tracing::info!(db = %config.db_prefix.display(), "session open: open database");
        let ctrl = lib.migrate_init(&config.db_prefix, &config.db_key, &config.migration_confirm)?;
        let session = Self {
            lib,
            ctrl,
            user: Mutex::new(None),
            encrypt_local_files: AtomicBool::new(true),
            pending: Mutex::new(Vec::new()),
        };
        // The active user; a new database gets one with the welcome name.
        let name = Some(config.initial_display_name.trim()).filter(|n| !n.is_empty()).unwrap_or("Cursus");
        let user = match session.send(Commands::show_active_user()) {
            Ok(value) => value,
            Err(_) => session.send(&Commands::create_active_user(name))?,
        };
        session.set_user(parse::user(&user)?);
        tracing::info!("session open: ready");
        Ok(session)
    }

    /// Start the chat core. `encrypt_local_files`: keep attachments encrypted
    /// on disk (CryptoFile, keys in the database) — the official default.
    ///
    /// The network is not touched yet: call [`Self::activate_network`] once
    /// events are being received (subscribing before has hung unlock).
    pub fn start_chat(&self, encrypt_local_files: bool) -> Result<()> {
        self.send(Commands::start_chat())?;
        let _ = self.send(&Commands::set_temp_folder(&temp_dir().to_string_lossy()));
        let _ = self.send(&Commands::set_files_folder(&files_dir().to_string_lossy()));
        // Not worth failing the unlock over: files then stay encrypted.
        if let Err(err) = self.set_encrypt_local_files(encrypt_local_files) {
            tracing::warn!(%err, "could not set file encryption");
        }
        Ok(())
    }

    /// Subscribe transports after the event receiver is running.
    pub fn activate_network(&self) -> Result<()> {
        tracing::info!("activate chat, resubscribe");
        let _ = self.send(Commands::activate_chat());
        let _ = self.send(Commands::resubscribe_all());
        Ok(())
    }

    /// Run one chat command and return its result.
    pub fn send(&self, cmd: &str) -> Result<Value> {
        tracing::debug!(cmd = %redact_command(cmd), "chat_send_cmd");
        let raw = self.lib.send_cmd(self.ctrl, cmd)?;
        serde_json::from_str::<ApiEnvelope>(&raw)?.into_result().inspect_err(|err| {
            // Error payloads embed whole contacts / groups: log the start only.
            let brief: String = err.to_string().chars().take(160).collect();
            tracing::warn!(cmd = %redact_command(cmd), error = %brief, "chat command failed");
        })
    }

    /// Forward core events to the returned channel from a background thread
    /// (call inside a tokio runtime). The thread ends when the receiver is dropped.
    pub fn spawn_receiver(self: &Arc<Self>) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        // Owning the session keeps `Drop` (→ `chat_close_store`) from running
        // while this thread is inside `chat_recv_msg_wait`.
        let session = Arc::clone(self);
        tokio::task::spawn_blocking(move || session.receive_events(tx));
        rx
    }

    fn receive_events(&self, tx: mpsc::UnboundedSender<Event>) {
        let mut failures = 0;
        // Wait a second at a time, so a dropped receiver (quit, session
        // swap) is noticed quickly and never holds up runtime shutdown.
        while !tx.is_closed() {
            let raw = match self.lib.recv_msg_wait(self.ctrl, 1_000_000) {
                Ok(None) => continue,
                Ok(Some(raw)) => raw,
                Err(err) => {
                    // Don't go deaf on one bad read, don't spin on a dead core.
                    failures += 1;
                    tracing::error!("chat_recv_msg_wait failed ({failures}): {err}");
                    if failures >= 10 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            };
            failures = 0;
            // Never log `raw`: events carry message text and profiles.
            match serde_json::from_str::<ApiEnvelope>(&raw).map_err(Error::from).and_then(ApiEnvelope::into_result) {
                Ok(value) => {
                    if tx.send(parse_event(value)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let brief: String = err.to_string().chars().take(160).collect();
                    tracing::warn!("bad event ({} bytes): {brief}", raw.len());
                }
            }
        }
    }

    // ── User ────────────────────────────────────────────────────────────

    fn set_user(&self, user: UserProfile) {
        *lock(&self.user) = Some(user);
    }

    /// The active user as last seen (no core round trip).
    pub fn active_profile(&self) -> Option<UserProfile> {
        lock(&self.user).clone()
    }

    fn user_id(&self) -> Result<i64> {
        lock(&self.user).as_ref().map(|u| u.user_id).ok_or_else(|| command_error("no active user"))
    }

    /// Re-read the active user from the core.
    pub fn refresh_user(&self) -> Result<UserProfile> {
        let user = parse::user(&self.send(Commands::show_active_user())?)?;
        self.set_user(user.clone());
        Ok(user)
    }

    pub fn update_profile(&self, fields: ProfileFields) -> Result<UserProfile> {
        let value = self.send(&Commands::update_profile(self.user_id()?, &fields))?;
        // The reply carries the user, or (nothing changed) only the profile.
        let user = match (parse::user(&value), value.get("toProfile"), self.active_profile()) {
            (Ok(user), ..) => user,
            (Err(_), Some(profile), Some(user)) => UserProfile { profile: parse::profile_fields(Some(profile)), ..user },
            _ => return self.refresh_user(),
        };
        self.set_user(user.clone());
        Ok(user)
    }

    /// Share / unshare the reusable address on the profile.
    pub fn set_profile_address(&self, on: bool) -> Result<UserProfile> {
        if on {
            self.ensure_address_link()?;
        }
        self.send(&Commands::profile_address(self.user_id()?, on))?;
        self.refresh_user()
    }

    /// Delivery receipts for all contacts (`groups` false) or small groups.
    ///
    /// The core writes the DB but keeps serving its in-memory user from `/u`
    /// until restart, so the cached profile is updated here instead (as the
    /// official app does).
    pub fn set_delivery_receipts(&self, groups: bool, on: bool) -> Result<UserProfile> {
        self.send(&Commands::set_delivery_receipts(self.user_id()?, groups, on))?;
        let mut guard = lock(&self.user);
        let user = guard.as_mut().ok_or_else(|| command_error("no active user"))?;
        if groups {
            user.send_rcpts_small_groups = on;
        } else {
            user.send_rcpts_contacts = on;
        }
        Ok(user.clone())
    }

    // ── Chat list ───────────────────────────────────────────────────────

    /// The chat list. Also refreshes [`Self::pending_invitations`] from the
    /// same reply (pending connections ride along with `pcc=on`).
    pub fn list_chats(&self) -> Result<Vec<ChatPreview>> {
        let value = self.send(&Commands::get_chats(self.user_id()?))?;
        *lock(&self.pending) = invitations::pending_invitations(&value);
        Ok(parse::chat_previews(&value))
    }

    /// Clear unread state for a chat (badge goes to zero after list refresh).
    pub fn mark_chat_read(&self, chat: &ChatId) -> Result<Value> {
        let read = self.send(&Commands::read_chat(chat))?;
        // `/_unread` rejects group support scopes (`not supported`).
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

    /// Delete this chat's messages from this device after `seconds`
    /// (`None`: the user default).
    pub fn set_chat_ttl(&self, chat: &ChatId, seconds: Option<i64>) -> Result<Value> {
        self.send(&Commands::set_chat_ttl(self.user_id()?, chat, seconds))
    }

    pub fn clear_chat(&self, chat: &ChatId) -> Result<Value> {
        self.send(&Commands::clear_chat(chat))
    }

    /// Delete a chat. A group you're still in but don't own can't be deleted
    /// for everyone (`groupUserRole`): leave it — members are told — then
    /// delete it from this device, as the official apps do.
    pub fn delete_chat(&self, chat: &ChatId) -> Result<Value> {
        match self.send(&Commands::delete_chat(chat)) {
            Err(Error::Command(msg)) if chat.chat_type == ChatType::Group && is_group_role_error(&msg) => {
                self.leave_group(chat.api_id)?;
                self.send(&Commands::delete_chat(chat))
            }
            other => other,
        }
    }

    pub fn list_chat_tags(&self) -> Result<Vec<ChatTag>> {
        Ok(parse::chat_tags(&self.send(&Commands::get_chat_tags(self.user_id()?))?))
    }

    pub fn create_chat_tag(&self, text: &str) -> Result<Vec<ChatTag>> {
        let text = Some(text.trim()).filter(|t| !t.is_empty()).ok_or_else(|| command_error("list name required"))?;
        Ok(parse::chat_tags(&self.send(&Commands::create_chat_tag(text))?))
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

    pub fn accept_member_contact(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::accept_member_contact(contact_id))
    }

    pub fn reject_member_contact(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::reject_member_contact(contact_id))
    }

    pub fn set_contact_alias(&self, contact_id: i64, alias: &str) -> Result<Value> {
        self.send(&Commands::set_contact_alias(contact_id, alias))
    }

    /// What you allow this contact (`cursus_core::preferences` builds `prefs`).
    pub fn set_contact_prefs(&self, contact_id: i64, prefs: &Value) -> Result<Value> {
        self.send(&Commands::set_contact_prefs(contact_id, prefs))
    }

    // ── Messages ────────────────────────────────────────────────────────

    /// The newest `count` messages of a chat, oldest first.
    pub fn list_messages(&self, chat: &ChatId, count: u32) -> Result<Vec<ChatMessage>> {
        Ok(parse::chat_messages(&self.send(&Commands::get_chat(chat, count))?))
    }

    /// Send a text, optionally as a reply to `quoted_item_id`.
    pub fn send_text(&self, chat: &ChatId, text: &str, quoted_item_id: Option<i64>) -> Result<Value> {
        self.send(&Commands::send_message(chat, Commands::text(text), None, quoted_item_id))
    }

    pub fn update_text(&self, chat: &ChatId, item_id: i64, text: &str) -> Result<Value> {
        self.send(&Commands::update_text(chat, item_id, text))
    }

    pub fn delete_items(&self, chat: &ChatId, item_ids: &[i64], broadcast: bool) -> Result<Value> {
        if item_ids.is_empty() {
            return Err(command_error("no items to delete"));
        }
        self.send(&Commands::delete_items(chat, item_ids, broadcast))
    }

    pub fn forward_items(&self, from: &ChatId, to: &ChatId, item_ids: &[i64]) -> Result<Value> {
        if item_ids.is_empty() {
            return Err(command_error("no items to forward"));
        }
        self.send(&Commands::forward_items(from, to, item_ids))
    }

    /// One message's details; for your group messages also who received
    /// it, with names and pictures from the member list.
    pub fn message_info(&self, chat: &ChatId, item_id: i64) -> Result<MessageInfo> {
        let mut info = MessageInfo::from_api(&self.send(&Commands::item_info(chat, item_id))?);
        if !info.receipts.is_empty() && chat.chat_type == ChatType::Group {
            // Names are a nicety: a failed member list still shows the counts.
            let members = self.list_group_members(chat.api_id).unwrap_or_default();
            info.name_members(&members);
        }
        Ok(info)
    }

    /// Add (`add`) or remove your reaction on a message.
    pub fn set_reaction(&self, chat: &ChatId, item_id: i64, emoji: &str, add: bool) -> Result<Value> {
        self.send(&Commands::reaction(chat, item_id, known_reaction(emoji)?, add))
    }

    /// Group members who reacted with `emoji` on `item_id`.
    pub fn reaction_members(&self, chat: &ChatId, item_id: i64, emoji: &str) -> Result<Vec<ReactionMember>> {
        if chat.chat_type != ChatType::Group {
            return Err(command_error("reaction members only available for groups"));
        }
        let cmd = Commands::reaction_members(self.user_id()?, chat.api_id, item_id, known_reaction(emoji)?);
        Ok(parse::reaction_members(&self.send(&cmd)?))
    }

    // ── Files ───────────────────────────────────────────────────────────

    /// Send a file. Photos go as the official clients send them: re-encoded
    /// without metadata, small enough to download by themselves, with an
    /// inline preview (see [`crate::media::prepare_outgoing_image`]).
    pub fn send_file(&self, chat: &ChatId, path: &str) -> Result<Value> {
        let src = Path::new(path.trim());
        let name = src.file_name().and_then(|s| s.to_str()).ok_or_else(|| command_error("no file to send"))?;
        let staging = temp_dir().join(format!("send-{}", unique_stamp()));
        let image = parse::is_image_name(name)
            .then(|| crate::media::prepare_outgoing_image(src, &staging))
            .and_then(|prepared| prepared.inspect_err(|err| tracing::warn!(%err, "not sent as a photo")).ok());
        let sent = match &image {
            Some(image) => {
                let name = image.path.file_name().and_then(|s| s.to_str()).unwrap_or(name);
                self.store_outgoing(&image.path, name)
                    .and_then(|file| self.send(&Commands::send_message(chat, Commands::image(&image.preview), Some(&file), None)))
            }
            None => self
                .store_outgoing(src, name)
                .and_then(|file| self.send(&Commands::send_message(chat, Commands::file(), Some(&file), None))),
        };
        let _ = std::fs::remove_dir_all(&staging);
        sent
    }

    /// Send a recorded voice message of `duration_secs`.
    pub fn send_voice(&self, chat: &ChatId, path: &str, duration_secs: u32) -> Result<Value> {
        if duration_secs == 0 {
            return Err(command_error("voice message too short"));
        }
        let src = Path::new(path.trim());
        let name = src.file_name().and_then(|s| s.to_str()).ok_or_else(|| command_error("no voice message to send"))?;
        let file = self.store_outgoing(src, name)?;
        self.send(&Commands::send_message(chat, Commands::voice(duration_secs), Some(&file), None))
    }

    /// Put `src` into the files folder for sending, as the official clients
    /// do: encrypted as a CryptoFile when local files are encrypted, else
    /// copied. The core is given the name inside the folder, so the message
    /// never depends on where the user's original is.
    fn store_outgoing(&self, src: &Path, preferred_name: &str) -> Result<OutgoingFile> {
        if !src.is_file() {
            return Err(command_error("the file to send is missing"));
        }
        ensure_private_dir(&files_dir())?;
        let name = unique_files_name(preferred_name);
        let dest = files_dir().join(&name);
        let crypto = if self.encrypt_local_files.load(Ordering::Relaxed) {
            Some(self.lib.encrypt_file(self.ctrl, src, &dest)?)
        } else {
            std::fs::copy(src, &dest)?;
            None
        };
        // Our own plaintext (a recording, a re-encoded photo) is not kept.
        if src.starts_with(data_dir()) {
            let _ = std::fs::remove_file(src);
        }
        Ok(OutgoingFile { path: name, crypto })
    }

    /// Accept an incoming file. With `approved_relays` false the core refuses
    /// file servers we don't use; that comes back as
    /// [`ReceiveOutcome::NeedsApproval`] instead of an error.
    pub fn receive_file(&self, file_id: i64, approved_relays: bool) -> Result<ReceiveOutcome> {
        ensure_private_dir(&files_dir())?;
        let encrypt = self.encrypt_local_files.load(Ordering::Relaxed);
        let cmd = Commands::receive_file(file_id, approved_relays, encrypt);
        let env: ApiEnvelope = serde_json::from_str(&self.lib.send_cmd(self.ctrl, &cmd)?)?;
        if let Some(servers) = env.error.as_ref().and_then(file_not_approved_servers) {
            return Ok(ReceiveOutcome::NeedsApproval { servers });
        }
        env.into_result().map(|_| ReceiveOutcome::Accepted)
    }

    /// Switch at-rest encryption for files received / sent from now on.
    pub fn set_encrypt_local_files(&self, on: bool) -> Result<()> {
        self.send(&Commands::set_encrypt_local_files(on))?;
        self.encrypt_local_files.store(on, Ordering::Relaxed);
        Ok(())
    }

    /// Decrypt a CryptoFile for viewing (cached for the session). Returns the
    /// plaintext path: in the RAM-backed runtime dir when it has room
    /// ([`runtime_cache_dir`]), else under `tmp/plain/` on disk.
    pub fn ensure_plaintext_file(&self, encrypted: &Path, key: &str, nonce: &str) -> Result<PathBuf> {
        let ram = runtime_cache_dir().map(|d| d.join("plain").join(plaintext_cache_name(encrypted, key, nonce)));
        let disk = plaintext_cache_path(encrypted, key, nonce);
        if let Some(cached) = ram.iter().chain([&disk]).find(|p| p.is_file()) {
            return Ok(cached.clone());
        }
        let size = std::fs::metadata(encrypted).map(|m| m.len()).unwrap_or(u64::MAX);
        if let Some(ram) = ram.filter(|ram| free_bytes(ram).is_some_and(|free| fits_in_ram_cache(size, free))) {
            match self.decrypt_into(encrypted, key, nonce, &ram) {
                Ok(()) => return Ok(ram),
                // e.g. the tmpfs filled up meanwhile.
                Err(err) => tracing::warn!(%err, "RAM decrypt failed; using disk cache"),
            }
        }
        self.decrypt_into(encrypted, key, nonce, &disk)?;
        Ok(disk)
    }

    /// Decrypt next to `dest`, then move into place: a cached file is always
    /// complete, also while another thread decrypts the same attachment.
    fn decrypt_into(&self, encrypted: &Path, key: &str, nonce: &str, dest: &Path) -> Result<()> {
        let dir = dest.parent().ok_or_else(|| command_error("no cache directory"))?;
        if let Some(parent) = dir.parent() {
            ensure_private_dir(parent)?;
        }
        ensure_private_dir(dir)?;
        let partial = dir.join(format!(".part-{}", unique_stamp()));
        let done = self
            .lib
            .decrypt_file(encrypted, key, nonce, &partial)
            .and_then(|()| Ok(std::fs::rename(&partial, dest)?));
        if done.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        done
    }

    // ── Address and invitations ─────────────────────────────────────────

    /// The reusable address, or `None` when the user has none — never
    /// creates one (`/_show_address` fails without an address).
    pub fn address_link(&self) -> Result<Option<ConnLink>> {
        match self.send(&Commands::show_address(self.user_id()?)) {
            Ok(value) => Ok(parse::address_link(&value)),
            Err(Error::Command(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The reusable address, created when there is none yet.
    pub fn ensure_address_link(&self) -> Result<ConnLink> {
        if let Some(link) = self.address_link()? {
            return Ok(link);
        }
        let created = self.send(&Commands::create_address(self.user_id()?))?;
        parse::address_link(&created).ok_or_else(|| command_error("the new address has no link"))
    }

    /// Delete the reusable address: nobody new can use it; contacts made
    /// through it stay connected (the official apps' "Delete address").
    pub fn delete_address(&self) -> Result<()> {
        self.send(&Commands::delete_address(self.user_id()?))?;
        // The profile no longer carries it.
        let _ = self.refresh_user();
        Ok(())
    }

    /// Whether the address accepts contact requests by itself (false
    /// without an address).
    pub fn address_auto_accept(&self) -> Result<bool> {
        Ok(self.send(&Commands::show_address(self.user_id()?)).is_ok_and(|v| parse::address_auto_accept(&v)))
    }

    /// Switch address auto-accept (never incognito); creates the address
    /// when there is none.
    pub fn set_address_auto_accept(&self, enabled: bool) -> Result<bool> {
        let user_id = self.user_id()?;
        self.ensure_address_link()?;
        let business = self.send(&Commands::show_address(user_id)).is_ok_and(|v| parse::address_is_business(&v));
        let value = self.send(&Commands::address_settings(user_id, business, enabled.then_some(false)))?;
        Ok(parse::address_auto_accept(&value))
    }

    /// Unused one-time links the user created, newest first, as of the last
    /// chat-list load and the invitation calls since (no core round trip).
    pub fn pending_invitations(&self) -> Vec<Invitation> {
        lock(&self.pending).clone()
    }

    fn update_pending_invitation(&self, conn_id: i64, update: impl FnOnce(&mut Invitation)) {
        if let Some(invitation) = lock(&self.pending).iter_mut().find(|i| i.conn_id == conn_id) {
            update(invitation);
        }
    }

    /// A new one-time link — what opening the New page ("+") does.
    pub fn create_invitation(&self, incognito: bool) -> Result<Invitation> {
        let value = self.send(&Commands::create_invitation(self.user_id()?, incognito))?;
        let inv = invitations::parse_created_invitation(&value).ok_or_else(|| command_error("the invitation has no link"))?;
        lock(&self.pending).insert(0, inv.clone());
        Ok(inv)
    }

    /// "New link": delete `previous` (the user confirmed) and create a fresh one.
    pub fn regenerate_invitation(&self, previous: Option<i64>, incognito: bool) -> Result<Invitation> {
        if let Some(conn_id) = previous {
            // Used meanwhile (now a contact) or already gone: nothing to delete.
            let _ = self.delete_invitation(conn_id);
        }
        self.create_invitation(incognito)
    }

    /// Delete an unused one-time link; whoever has it can no longer connect.
    pub fn delete_invitation(&self, conn_id: i64) -> Result<()> {
        let result = self.send(&Commands::delete_pending_connection(conn_id));
        // Gone from the list either way: deleted now, or already used / gone.
        lock(&self.pending).retain(|i| i.conn_id != conn_id);
        result.map(|_| ())
    }

    /// Name a pending link so the user knows whom it was for (empty clears).
    pub fn set_invitation_alias(&self, conn_id: i64, alias: &str) -> Result<String> {
        let value = self.send(&Commands::set_connection_alias(conn_id, alias))?;
        let alias = invitations::parse_alias_updated(&value).ok_or_else(|| command_error("unexpected alias reply"))?;
        self.update_pending_invitation(conn_id, |i| i.alias = alias.clone());
        Ok(alias)
    }

    /// Switch an unused invitation to / from a random incognito profile; the
    /// link stays the same. Returns the new state.
    pub fn set_invitation_incognito(&self, conn_id: i64, incognito: bool) -> Result<bool> {
        let value = self.send(&Commands::set_connection_incognito(conn_id, incognito))?;
        let on = invitations::parse_incognito_updated(&value).ok_or_else(|| command_error("unexpected incognito reply"))?;
        self.update_pending_invitation(conn_id, |i| i.incognito = on);
        Ok(on)
    }

    /// Accept an invitation / address / group link (short HTTPS or full
    /// `simplex:` URI), with a new random profile when `incognito`.
    pub fn connect_link(&self, link: &str, incognito: bool) -> Result<Value> {
        let link = link.trim().trim_matches(['"', '\'', '`', '“', '”']).trim();
        if link.is_empty() || link.chars().any(char::is_whitespace) {
            return Err(command_error("Paste a single SimpleX link."));
        }
        // `/connect` resolves short links; `/_connect <userId> …` does not.
        let result = self.send(&Commands::connect_link(link, incognito))?;
        if result.tag() == "connectionPlan" {
            return Err(command_error("The link was recognized, but the core did not start a connection."));
        }
        Ok(result)
    }

    // ── Groups and channels ─────────────────────────────────────────────

    fn created_group_id(value: &Value) -> Result<ChatId> {
        let id = value.i64_at("/groupInfo/groupId").ok_or_else(|| command_error("the new group has no id"))?;
        Ok(ChatId::new(ChatType::Group, id))
    }

    pub fn create_group(&self, display_name: &str) -> Result<ChatId> {
        let name = Some(display_name.trim()).filter(|n| !n.is_empty()).ok_or_else(|| command_error("group name required"))?;
        Self::created_group_id(&self.send(&Commands::new_group(self.user_id()?, name))?)
    }

    /// Enabled chat relays (`/_servers`), which channels are created on.
    pub fn list_chat_relays(&self) -> Result<Vec<ChatRelayInfo>> {
        Ok(parse::chat_relays(&self.send(&Commands::get_user_servers(self.user_id()?))?))
    }

    /// Create a channel on up to 3 enabled relays; returns it with its
    /// `/c#` link when the core already has one.
    pub fn create_channel(&self, display_name: &str) -> Result<(ChatId, Option<String>)> {
        let name = Some(display_name.trim()).filter(|n| !n.is_empty()).ok_or_else(|| command_error("channel name required"))?;
        let relays: Vec<i64> = self.list_chat_relays()?.iter().map(|r| r.chat_relay_id).take(3).collect();
        if relays.is_empty() {
            return Err(command_error("no chat relays available — enable relays in SimpleX network settings first"));
        }
        let value = self.send(&Commands::new_public_group(self.user_id()?, &relays, name))?;
        if value.tag() == "publicGroupCreationFailed" {
            return Err(command_error("channel creation failed — check relay connectivity"));
        }
        Ok((Self::created_group_id(&value)?, parse::channel_link(&value).map(|l| l.display)))
    }

    pub fn list_group_members(&self, group_id: i64) -> Result<Vec<GroupMemberInfo>> {
        Ok(parse::group_members(&self.send(&Commands::list_members(group_id))?))
    }

    /// Invite a direct contact into the group with the given role.
    pub fn add_group_member(&self, group_id: i64, contact_id: i64, role: &str) -> Result<Value> {
        self.send(&Commands::add_member(group_id, contact_id, role_or_member(role)))
    }

    pub fn set_group_member_role(&self, group_id: i64, member_id: i64, role: &str) -> Result<Value> {
        self.send(&Commands::set_member_role(group_id, &[member_id], role))
    }

    pub fn remove_group_member(&self, group_id: i64, member_id: i64, with_messages: bool) -> Result<Value> {
        self.send(&Commands::remove_members(group_id, &[member_id], with_messages))
    }

    /// A moderator lets a pending member in (`/_accept member`).
    pub fn accept_group_member(&self, group_id: i64, member_id: i64, role: &str) -> Result<Value> {
        self.send(&Commands::accept_member(group_id, member_id, role_or_member(role)))
    }

    /// Block / unblock a member for everyone (moderators).
    pub fn block_group_member(&self, group_id: i64, member_id: i64, blocked: bool) -> Result<Value> {
        self.send(&Commands::block_members(group_id, &[member_id], blocked))
    }

    /// Show / hide a member's messages on this device only.
    pub fn set_member_show_messages(&self, group_id: i64, member_id: i64, show_messages: bool) -> Result<Value> {
        self.send(&Commands::set_member_settings(group_id, member_id, show_messages))
    }

    pub fn delete_member_support_chat(&self, group_id: i64, group_member_id: i64) -> Result<Value> {
        self.send(&Commands::delete_member_support_chat(group_id, group_member_id))
    }

    /// Open (or create) a direct chat with a group member, as the official
    /// "Send direct message" does. Returns the contact id.
    pub fn create_member_contact(&self, group_id: i64, member_id: i64) -> Result<i64> {
        let value = self.send(&Commands::create_member_contact(group_id, member_id))?;
        let contact_id = value.i64_at("/contact/contactId").ok_or_else(|| command_error("the member contact has no id"))?;
        self.send(&Commands::invite_member_contact(contact_id))?;
        Ok(contact_id)
    }

    pub fn leave_group(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::leave_group(group_id))
    }

    /// Accept a group invitation received in a direct chat.
    pub fn join_group(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::join_group(group_id))
    }

    pub fn set_group_alias(&self, group_id: i64, alias: &str) -> Result<Value> {
        self.send(&Commands::set_group_alias(group_id, alias))
    }

    /// Update the whole group profile (name, welcome message, bio, picture,
    /// preferences): fields left out of `profile` are cleared.
    pub fn update_group_profile(&self, group_id: i64, profile: &GroupProfileUpdate) -> Result<Value> {
        let profile = GroupProfileUpdate {
            display_name: profile.display_name.trim().to_string(),
            welcome_message: profile.welcome_message.trim().to_string(),
            short_descr: profile.short_descr.trim().to_string(),
            ..profile.clone()
        };
        if profile.display_name.is_empty() {
            return Err(command_error("group name required"));
        }
        self.send(&Commands::update_group_profile(group_id, &profile))
    }

    /// Read an existing invite without creating one on page entry.
    pub fn existing_group_link(&self, group_id: i64) -> Result<Option<ConnLink>> {
        let response = self.send(&Commands::get_group_link(group_id))?;
        Ok(parse::group_link(&response).or_else(|| parse::channel_link(&response)))
    }

    /// The link people join with, created for a private group that has none
    /// (`member_role`: what joiners become). A channel only ever has the
    /// `/c#` link it was created with: a `/_create link` one would be a
    /// private-group link, which official apps reject for channels.
    pub fn group_invite_link(&self, group_id: i64, member_role: &str) -> Result<ConnLink> {
        if let Ok(value) = self.send(&Commands::get_group_link(group_id)) {
            if value.bool_at("/groupInfo/useRelays").unwrap_or(false) {
                return parse::channel_link(&value)
                    .ok_or_else(|| command_error("this channel has no usable link — create a new channel"));
            }
            if let Some(link) = parse::group_link(&value) {
                return Ok(link);
            }
        }
        let created = self.send(&Commands::create_group_link(group_id, role_or_member(member_role)))?;
        parse::group_link(&created).ok_or_else(|| command_error("the new group link is missing"))
    }

    pub fn set_group_link_role(&self, group_id: i64, role: &str) -> Result<Value> {
        self.send(&Commands::set_group_link_role(group_id, role))
    }

    pub fn delete_group_link(&self, group_id: i64) -> Result<Value> {
        self.send(&Commands::delete_group_link(group_id))
    }

    /// Add the short form to a group link made before short links existed.
    pub fn short_group_link(&self, group_id: i64) -> Result<ConnLink> {
        parse::group_link(&self.send(&Commands::short_group_link(group_id))?)
            .ok_or_else(|| command_error("the short group link is missing"))
    }

    // ── Calls (signaling; media is `cursus-call`) ───────────────────────

    pub fn call_invite(&self, contact_id: i64, media: CallMedia) -> Result<Value> {
        self.send(&Commands::call_invite(contact_id, media))
    }

    pub fn call_reject(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::call_reject(contact_id))
    }

    pub fn call_end(&self, contact_id: i64) -> Result<Value> {
        self.send(&Commands::call_end(contact_id))
    }

    pub fn call_offer(
        &self,
        contact_id: i64,
        media: CallMedia,
        encryption: bool,
        rtc_session: &str,
        rtc_ice_candidates: &str,
    ) -> Result<Value> {
        self.send(&Commands::call_offer(contact_id, media, encryption, rtc_session, rtc_ice_candidates))
    }

    pub fn call_answer(&self, contact_id: i64, rtc_session: &str, rtc_ice_candidates: &str) -> Result<Value> {
        self.send(&Commands::call_answer(contact_id, rtc_session, rtc_ice_candidates))
    }

    pub fn call_extra(&self, contact_id: i64, rtc_ice_candidates: &str) -> Result<Value> {
        self.send(&Commands::call_extra(contact_id, rtc_ice_candidates))
    }

    pub fn call_status(&self, contact_id: i64, status: &str) -> Result<Value> {
        self.send(&Commands::call_status(contact_id, status))
    }

    /// Recent calls, newest first. The core keeps each call as a chat item
    /// in the contact's chat; this pages back through the newest call items
    /// of all chats (filtered on their text, at most `scan_items`) until
    /// `limit` calls are found, falling back to one direct chat at a time.
    pub fn call_history(&self, limit: usize, scan_items: u32) -> Result<Vec<CallRecord>> {
        self.call_history_progress(limit, scan_items, |_| {})
    }

    /// Publish useful results after each page instead of hiding them until the
    /// entire bounded scan finishes. No message bodies are retained by callers.
    pub fn call_history_progress(
        &self,
        limit: usize,
        scan_items: u32,
        mut progress: impl FnMut(&[CallRecord]),
    ) -> Result<Vec<CallRecord>> {
        let scanned = scan_call_history(
            limit,
            scan_items,
            100,
            |count, before| self.send(&Commands::get_call_items(count, before)),
            &mut progress,
        );
        match scanned {
            Ok(calls) => Ok(calls),
            // `/_get items` loads every item's chat, and the core fails the
            // whole page on one it can't load, e.g. an item left in a group
            // member's deleted support chat ("no support chat"). Calls only
            // live in direct chats: ask each of those instead.
            Err(err) => {
                tracing::warn!(%err, "call history: all-chats scan failed, scanning direct chats");
                let direct: Vec<ChatId> = self
                    .list_chats()?
                    .into_iter()
                    .map(|chat| chat.id)
                    .filter(|id| id.chat_type == ChatType::Direct)
                    .collect();
                let per_chat = (scan_items / 4).max(limit as u32).max(1);
                scan_direct_call_history(limit, &direct, per_chat, |chat, count| {
                    self.send(&Commands::get_chat_call_items(chat, count))
                }, progress)
            }
        }
    }
}

/// Call history from each direct chat in turn (newest chats first). A chat
/// the core can't load is skipped rather than hiding every other call.
fn scan_direct_call_history(
    limit: usize,
    chats: &[ChatId],
    per_chat: u32,
    mut fetch: impl FnMut(&ChatId, u32) -> Result<Value>,
    mut progress: impl FnMut(&[CallRecord]),
) -> Result<Vec<CallRecord>> {
    let mut calls: Vec<CallRecord> = Vec::new();
    if limit == 0 {
        return Ok(calls);
    }
    for chat in chats {
        let value = match fetch(chat, per_chat) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(%err, chat = chat.api_id, "call history: chat skipped");
                continue;
            }
        };
        let Some(info) = value.pointer("/chat/chatInfo") else { continue };
        let previous = calls.len();
        calls.extend(value.array_at("/chat/chatItems").iter().filter_map(|item| {
            CallRecord::parse(&serde_json::json!({ "chatInfo": info, "chatItem": item }))
        }));
        if calls.len() != previous {
            calls.sort_by_key(|r| std::cmp::Reverse(r.item_id));
            calls.dedup_by_key(|r| r.item_id);
            calls.truncate(limit);
            progress(&calls);
        }
    }
    Ok(calls)
}

fn scan_call_history(
    limit: usize,
    scan_items: u32,
    first_page: u32,
    mut fetch: impl FnMut(u32, Option<i64>) -> Result<Value>,
    mut progress: impl FnMut(&[CallRecord]),
) -> Result<Vec<CallRecord>> {
    const PAGE: u32 = 250;
    let mut calls = Vec::new();
    let mut before = None;
    let mut scanned = 0;
    while scanned < scan_items && calls.len() < limit {
        let page = if before.is_none() { first_page } else { PAGE };
        let count = page.min(scan_items - scanned);
        let value = fetch(count, before)?;
        let items = value.array_at("/chatItems");
        scanned += items.len() as u32;
        let previous = calls.len();
        calls.extend(items.iter().filter_map(CallRecord::parse));
        calls.sort_by_key(|r| std::cmp::Reverse(r.item_id));
        calls.truncate(limit);
        if calls.len() != previous {
            progress(&calls);
        }
        let oldest = items
            .iter()
            .filter_map(|aci| aci.i64_at("/chatItem/meta/itemId"))
            .min();
        if oldest.is_none() || oldest == before || (items.len() as u32) < count {
            break;
        }
        before = oldest;
    }
    Ok(calls)
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.ctrl.is_null() {
            // Stop the chat (network, timers) before closing its database,
            // as the official clients do.
            let _ = self.send(Commands::stop_chat());
            let _ = self.lib.close_store(self.ctrl);
            self.ctrl = std::ptr::null_mut();
        }
    }
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
    let err = err.get("errorType").unwrap_or(err);
    (err.tag() == "fileNotApproved").then(|| {
        err.array_at("/unknownServers")
            .iter()
            .filter_map(Value::as_str)
            // `xftp://fingerprint@host:port` → `host`
            .map(|s| s.rsplit('@').next().unwrap_or(s).split([':', '/', ',']).next().unwrap_or(s).to_string())
            .collect()
    })
}

/// The chat error for an action your group role doesn't allow.
fn is_group_role_error(message: &str) -> bool {
    message.starts_with("groupUserRole")
}

fn known_reaction(emoji: &str) -> Result<&'static str> {
    parse::reaction_emoji(emoji).ok_or_else(|| command_error("unsupported reaction"))
}

/// Different on every call in this process (names of temporary files).
fn unique_stamp() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

/// `preferred` (its file name only), or `stem-N.ext` when that is taken in
/// the files folder.
fn unique_files_name(preferred: &str) -> String {
    let path = Path::new(preferred.trim());
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file.bin");
    if !files_dir().join(name).exists() {
        return name.to_string();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (1..)
        .map(|i| format!("{stem}-{i}{ext}"))
        .find(|candidate| !files_dir().join(candidate).exists())
        .unwrap_or_else(|| format!("{stem}-{}{ext}", unique_stamp()))
}

/// Command for logs: the verb plus chat / item references (`@5`, `#3`,
/// numbers, `key=value` flags), never free text, JSON, links or file paths.
/// Commands carry message bodies, profiles, invite links and file names.
fn redact_command(cmd: &str) -> String {
    let is_ref = |t: &str| {
        let body = t.trim_start_matches(['@', '#', '*', ':']);
        !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '(' | ')'))
    };
    let is_flag = |t: &str| {
        t.split_once('=').is_some_and(|(k, v)| {
            k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && v.len() <= 8
                && v.chars().all(|c| c.is_ascii_alphanumeric())
        })
    };
    let mut out = String::new();
    let mut hidden = 0;
    for (i, tok) in cmd.split_whitespace().enumerate() {
        // Once something was hidden, everything after it is too.
        if i == 0 || (hidden == 0 && (is_ref(tok) || is_flag(tok) || tok == "json" || tok == "chat")) {
            if i > 0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn call_history_publishes_before_the_next_page_and_respects_the_budget() {
        let published = std::cell::Cell::new(0);
        let mut requested = Vec::new();
        let mut updates = Vec::new();
        let calls = scan_call_history(100, 350, 100, |count, before| {
            if before.is_some() { assert_eq!(published.get(), 1, "first calls must be visible before the next query"); }
            requested.push((count, before));
            let newest = before.map_or(500, |id| id - 1);
            Ok(json!({"chatItems": (0..count).map(|offset| json!({
                "chatInfo": {"type": "direct", "contact": {"contactId": 7, "profile": {"displayName": "Alice"}}},
                "chatItem": {"meta": {"itemId": newest - i64::from(offset)}, "content": {"type": if offset == 0 { "rcvCall" } else { "rcvMsgContent" }, "status": "ended", "duration": 42}}
            })).collect::<Vec<_>>() }))
        }, |calls| { published.set(published.get() + 1); updates.push(calls.to_vec()); }).unwrap();
        assert_eq!(requested, vec![(100, None), (250, Some(401))]);
        assert_eq!(updates.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(
            calls.iter().map(|call| call.item_id).collect::<Vec<_>>(),
            vec![500, 400]
        );
        assert!(scan_call_history(
            0,
            350,
            100,
            |_, _| panic!("empty request must not query"),
            |_| {}
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn direct_call_history_skips_chats_the_core_cannot_load() {
        let chats = [ChatId::new(ChatType::Direct, 1), ChatId::new(ChatType::Direct, 2), ChatId::new(ChatType::Direct, 3)];
        let mut updates = 0;
        let calls = scan_direct_call_history(10, &chats, 50, |chat, count| {
            assert_eq!(count, 50);
            if chat.api_id == 2 {
                return Err(command_error("no support chat"));
            }
            Ok(json!({"chat": {
                "chatInfo": {"type": "direct", "contact": {"contactId": chat.api_id, "profile": {"displayName": "Bob"}}},
                "chatItems": [
                    {"meta": {"itemId": chat.api_id * 10}, "content": {"type": "sndCall", "status": "ended", "duration": 41}},
                    {"meta": {"itemId": chat.api_id * 10 + 1}, "content": {"type": "sndMsgContent"}}
                ]
            }}))
        }, |_| updates += 1).unwrap();
        assert_eq!(calls.iter().map(|c| (c.item_id, c.contact_id)).collect::<Vec<_>>(), vec![(30, 3), (10, 1)]);
        assert!(calls.iter().all(|c| c.outgoing && c.duration_secs == 41));
        assert_eq!(updates, 2);
    }

    #[test]
    fn file_not_approved_lists_server_hosts() {
        let err = json!({"type": "error", "errorType": {
            "type": "fileNotApproved", "fileId": 9,
            "unknownServers": ["xftp://abc=@files.evil.example:443", "xftp://k@other.org"]
        }});
        assert_eq!(
            file_not_approved_servers(&err),
            Some(vec!["files.evil.example".to_string(), "other.org".to_string()])
        );
        let other = json!({"type": "error", "errorType": {"type": "fileCancelled"}});
        assert_eq!(file_not_approved_servers(&other), None);
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

    /// The error a member gets for `/_delete #group` (from a real session).
    #[test]
    fn member_delete_error_is_recognised() {
        let msg = r#"groupUserRole: {"errorType":{"requiredRole":"owner","type":"groupUserRole"},"type":"error"}"#;
        assert!(is_group_role_error(msg));
        assert!(!is_group_role_error("noActiveUser: {}"));
    }

    #[test]
    fn stored_file_names_stay_inside_the_files_folder() {
        assert_eq!(unique_files_name("../../etc/passwd"), "passwd");
        assert_eq!(unique_files_name("  "), "file.bin");
        assert_ne!(unique_stamp(), unique_stamp());
    }
}

/// Against the real libsimplex (`scripts/fetch-libs.sh`), without network.
#[cfg(test)]
mod live_tests {
    use super::*;

    fn open(dir: &Path) -> Session {
        let config = SessionConfig {
            db_prefix: dir.join("simplex_v1"),
            db_key: zeroize::Zeroizing::new("smoke".to_string()),
            initial_display_name: "Smoke".into(),
            ..SessionConfig::default()
        };
        let session = Session::open(config).expect("open session");
        session.start_chat(true).expect("start chat");
        session
    }

    #[test]
    fn display_names_preserve_literal_markers_in_the_real_core() {
        let _startup = crate::LIBSIMPLEX_STARTUP
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("cursus-rich-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = open(&dir);
        let fields = ProfileFields {
            display_name: "Team *Cursus* 🚀".into(),
            short_descr: "Build _nice_ things".into(),
            ..ProfileFields::default()
        };
        let user = session
            .update_profile(fields)
            .expect("save inline markdown in a valid SimpleX name");
        assert_eq!(user.profile.display_name, "Team *Cursus* 🚀");
        assert_eq!(user.profile.short_descr, "Build _nice_ things");
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The call-history queries parse in the real core and filter on the
    /// item text (calls need the network, so notes stand in for them).
    #[test]
    fn call_item_search_filters_in_the_real_core() {
        let _startup = crate::LIBSIMPLEX_STARTUP
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("cursus-call-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let session = open(&dir);
        let notes = session.list_chats().unwrap().into_iter().find(|c| c.id.chat_type == ChatType::Local).unwrap().id;
        session.send_text(&notes, "plain note", None).unwrap();
        session.send_text(&notes, "outgoing call: ended 00:41", None).unwrap();
        let texts = |items: &[Value]| -> Vec<String> {
            items.iter().filter_map(|i| i.pointer("/chatItem/meta/itemText").or_else(|| i.pointer("/meta/itemText")))
                .filter_map(Value::as_str).map(str::to_string).collect()
        };
        let all = session.send(&Commands::get_call_items(50, None)).expect("all-chats call search");
        assert_eq!(texts(all.array_at("/chatItems")), vec!["outgoing call: ended 00:41"]);
        let one = session.send(&Commands::get_chat_call_items(&notes, 50)).expect("one-chat call search");
        assert_eq!(texts(one.array_at("/chat/chatItems")), vec!["outgoing call: ended 00:41"]);
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_send_and_reopen() {
        let _startup = crate::LIBSIMPLEX_STARTUP
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("cursus-smoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let session = open(&dir);
        assert_eq!(
            session.active_profile().expect("user").display_name(),
            "Smoke"
        );
        let chats = session.list_chats().expect("list chats");
        // What every app does right after the password opened the DB: the
        // lock-screen verifier, with the core running.
        crate::app_lock::remember("smoke");
        assert!(crate::app_lock::verify("smoke"));

        // The notes folder takes messages without any network.
        let notes = chats.iter().find(|c| c.id.chat_type == ChatType::Local).expect("notes").id.clone();
        session.send_text(&notes, "hello :smile:", None).expect("send");
        let messages = session.list_messages(&notes, 10).expect("messages");
        assert_eq!(messages.last().map(|m| m.outgoing), Some(true));
        assert!(messages.last().is_some_and(|m| m.text.starts_with("hello ")));
        assert_eq!(session.address_link().expect("address"), None, "reading never creates an address");
        assert!(!session.address_auto_accept().expect("auto accept"));

        // A photo: stored encrypted, sent with a preview, readable again.
        let photo = dir.join("photo.png");
        image::RgbImage::from_pixel(800, 600, image::Rgb([200, 30, 30])).save(&photo).unwrap();
        session.send_file(&notes, photo.to_str().unwrap()).expect("send photo");
        let sent = session.list_messages(&notes, 10).expect("messages").pop().expect("photo message");
        let file = sent.file.expect("file");
        assert_eq!(file.kind, crate::models::FileKind::Image);
        assert!(file.preview.as_deref().is_some_and(|p| p.starts_with("data:image/jpg;base64,")));
        let stored = file.local_path.expect("stored path");
        assert!(stored.starts_with(files_dir()) && stored.ends_with("photo.jpg"), "{}", stored.display());
        assert!(file.encrypted && image::open(&stored).is_err(), "ciphertext on disk");
        let (key, nonce) = (file.crypto_key.unwrap(), file.crypto_nonce.unwrap());
        let plain = session.ensure_plaintext_file(&stored, &key, &nonce).expect("decrypt");
        assert_eq!(image::image_dimensions(&plain).unwrap(), (800, 600));
        assert_eq!(session.ensure_plaintext_file(&stored, &key, &nonce).unwrap(), plain, "cached");
        assert!(photo.is_file(), "the user's original is left alone");

        // With encryption off the file is copied in, readable.
        session.set_encrypt_local_files(false).expect("encryption off");
        let note = dir.join("note.txt");
        std::fs::write(&note, b"plain words").unwrap();
        session.send_file(&notes, note.to_str().unwrap()).expect("send file");
        let file = session.list_messages(&notes, 10).unwrap().pop().and_then(|m| m.file).expect("file");
        assert_eq!((file.kind, file.encrypted), (crate::models::FileKind::File, false));
        assert_eq!(std::fs::read(file.local_path.expect("stored path")).unwrap(), b"plain words");

        // Receipt switches are served from the cached user and persisted.
        assert!(!session.set_delivery_receipts(false, false).expect("receipts").send_rcpts_contacts);
        drop(session);
        let session = open(&dir);
        assert!(!session.refresh_user().expect("user").send_rcpts_contacts, "persisted");
        drop(session);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(data_dir());
    }
}
