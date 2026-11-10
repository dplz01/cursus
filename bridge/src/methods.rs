//! Request dispatch: one arm per method the apps call (macOS, Android).

use std::path::PathBuf;

use cursus_core::preferences;
use cursus_core::{
    AccountMeta, ChatId, ChatPreview, ChatSettings, ChatType, FeatureAllow, GroupProfileUpdate,
    NtfFilter, ReceiveOutcome, Session, SessionConfig,
};
use serde_json::{json, Value};

use crate::dto::{self, parse_chat_ref};

fn str_param<'a>(p: &'a Value, key: &str) -> Result<&'a str, String> {
    p.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing string `{key}`"))
}

fn opt_str<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

fn i64_param(p: &Value, key: &str) -> Result<i64, String> {
    p.get(key).and_then(Value::as_i64).ok_or_else(|| format!("missing integer `{key}`"))
}

fn bool_param(p: &Value, key: &str) -> Result<bool, String> {
    p.get(key).and_then(Value::as_bool).ok_or_else(|| format!("missing bool `{key}`"))
}

fn chat_param(p: &Value) -> Result<cursus_core::ChatId, String> {
    parse_chat_ref(str_param(p, "chat")?)
}

/// "Remove tracking from links" for a message being sent; on unless the
/// app says otherwise, so a missing flag never leaks tracking codes.
fn clean_links(p: &Value) -> bool {
    p.get("cleanLinks").and_then(Value::as_bool).unwrap_or(true)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Current state of one chat (settings, preferences, group profile), so a
/// change to one field can keep the others.
fn find_chat(s: &Session, chat: &ChatId) -> Result<ChatPreview, String> {
    let id = ChatId { scope: None, ..chat.clone() };
    s.list_chats()
        .map_err(err)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "chat not found".to_string())
}

fn group_chat(s: &Session, group_id: i64) -> Result<ChatPreview, String> {
    find_chat(s, &ChatId { chat_type: ChatType::Group, api_id: group_id, scope: None })
}

/// The group's full profile, ready to send back with some fields changed.
fn group_profile(c: &ChatPreview) -> GroupProfileUpdate {
    GroupProfileUpdate {
        display_name: c.peer_profile.display_name.clone(),
        full_name: c.peer_profile.full_name.clone(),
        short_descr: c.peer_profile.short_descr.clone(),
        welcome_message: c.peer_profile.description.clone(),
        image: c.peer_profile.image.clone(),
        group_preferences: c.group_preferences.clone(),
        member_admission: c.member_admission.clone(),
        public_group: c.public_group.clone(),
    }
}

/// Group roles a member can be given (`relay` / `author` are protocol-only).
fn role_param(p: &Value, key: &str) -> Result<String, String> {
    let role = str_param(p, key)?;
    if cursus_core::GROUP_ROLES.contains(&role) {
        Ok(role.to_string())
    } else {
        Err(format!("unknown role `{role}`"))
    }
}

pub fn dispatch(method: &str, p: &Value) -> Result<Value, String> {
    match method {
        "appInfo" => Ok(app_info()),
        "open" => open(p),
        "close" => {
            crate::drop_session();
            // Decrypted media never outlives the session (as on Linux).
            cursus_core::wipe_plaintext_cache();
            Ok(Value::Null)
        }
        "wipeLocalData" => {
            crate::drop_session();
            cursus_core::wipe_local_data().map_err(err)?;
            Ok(Value::Null)
        }
        "selfTest" => self_test(p),
        "prepareLink" => prepare_link(p),
        // Composer: `:shortcode` suggestions for what's typed, accepting one,
        // and expanding completed `:shortcodes:` as you type (Linux too).
        "emojiSuggest" => {
            let text = str_param(p, "text")?;
            let hits = cursus_core::pending_shortcode_query(text)
                .map(|q| cursus_core::suggest_emoji_shortcodes(q, 6))
                .unwrap_or_default();
            Ok(Value::Array(
                hits.iter().map(|h| json!({ "shortcode": h.shortcode, "glyph": h.glyph })).collect(),
            ))
        }
        "acceptShortcode" => {
            Ok(json!(cursus_core::accept_shortcode(str_param(p, "text")?, str_param(p, "glyph")?)))
        }
        "expandShortcodes" => Ok(json!(cursus_core::expand_emoji_shortcodes(str_param(p, "text")?))),
        // A picked image as a profile picture (core media rule: SimpleX data-URL budget).
        "encodeProfileImage" => Ok(json!(cursus_core::media::encode_profile_image(std::path::Path::new(str_param(p, "path")?))?)),
        // Executables a sender named are never opened (core safety rule).
        "riskyFile" => Ok(json!(cursus_core::safety::is_risky_to_open(std::path::Path::new(str_param(p, "path")?)))),
        _ => with_session(method, p),
    }
}

/// A link clicked in a message: SimpleX links go to the connect flow; other
/// links only when safe (web / mail / phone), tracking codes removed when
/// `clean` is on. `{ url, simplex }`, or `{ url: null }` when it can't open.
fn prepare_link(p: &Value) -> Result<Value, String> {
    use cursus_core::safety::{safe_external_url, strip_tracking_params};
    let raw = str_param(p, "url")?;
    if let Some(kind) = cursus_core::SimplexLinkKind::from_uri(raw) {
        return Ok(json!({ "url": raw, "simplex": dto::simplex_kind(kind) }));
    }
    let clean = p.get("clean").and_then(Value::as_bool).unwrap_or(true);
    let url = safe_external_url(raw).map(|u| if clean { strip_tracking_params(&u) } else { u });
    Ok(json!({ "url": url, "simplex": null }))
}

fn app_info() -> Value {
    let account = cursus_core::load_account();
    json!({
        "setupComplete": cursus_core::setup_complete(),
        "databaseExists": cursus_core::database_exists(),
        "displayName": account.as_ref().map(|a| a.display_name.clone()),
        "encryptLocalFiles": account.map(|a| a.encrypt_local_files).unwrap_or(true),
        "dataDir": cursus_core::data_dir().to_string_lossy(),
        "version": env!("CARGO_PKG_VERSION"),
        // The quick-reaction set every Cursus app offers (core), as glyphs.
        "reactions": cursus_core::KNOWN_REACTIONS.iter()
            .map(|e| if *e == "❤" { "❤️" } else { *e })
            .collect::<Vec<_>>(),
    })
}

fn config(p: &Value, password: &str) -> SessionConfig {
    let mut cfg = SessionConfig {
        db_key: zeroize::Zeroizing::new(password.to_string()),
        ..SessionConfig::default()
    };
    // Inside Cursus.app the libraries live in Contents/Frameworks.
    if let Some(dir) = opt_str(p, "libsDir").filter(|d| !d.is_empty()) {
        cfg.libs_dir = PathBuf::from(dir);
    }
    cfg
}

/// Open (first run: create) the encrypted database and start the chat core.
fn open(p: &Value) -> Result<Value, String> {
    if crate::session().is_ok() {
        return Err("Already unlocked".into());
    }
    let password = str_param(p, "password")?;
    let display_name = opt_str(p, "displayName").unwrap_or("").trim().to_string();
    let encrypt = p.get("encryptLocalFiles").and_then(Value::as_bool).unwrap_or(true);
    let save = p.get("saveAccount").and_then(Value::as_bool).unwrap_or(false);

    let mut cfg = config(p, password);
    if !display_name.is_empty() {
        cfg.initial_display_name = display_name.clone();
    }
    cfg.encrypt_local_files = encrypt;
    let mut session = Session::open(cfg).map_err(err)?;
    session.start_chat_with_options(encrypt).map_err(err)?;
    // The core may have created a default profile: use the welcome name.
    if let Some(user) = session.active_profile() {
        if !display_name.is_empty() && user.display_name() != display_name {
            let mut fields = user.profile.clone();
            fields.display_name = display_name.clone();
            let _ = session.update_profile(fields);
        }
    }
    if save {
        let meta = AccountMeta { display_name: display_name.clone(), version: 1, encrypt_local_files: encrypt };
        cursus_core::save_account(&meta).map_err(err)?;
    }
    let session = crate::install_session(session)?;
    let user = session.refresh_user().map_err(err)?;
    Ok(dto::user(&user))
}

/// Load libsimplex and open a throwaway database: proves the library runs on
/// this Mac (`Cursus --self-test`, also used by CI).
fn self_test(p: &Value) -> Result<Value, String> {
    let dir = std::env::temp_dir().join(format!("cursus-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let mut cfg = config(p, "self-test");
    cfg.db_prefix = dir.join("simplex_v1");
    cfg.initial_display_name = "Self test".into();
    let libs = cfg.libs_dir.to_string_lossy().to_string();
    let result = (|| {
        let mut session = Session::open(cfg).map_err(err)?;
        session.start_chat().map_err(err)?;
        let chats = session.list_chats().map_err(err)?;
        let user = session.refresh_user().map_err(err)?;
        Ok::<_, String>(json!({ "libsDir": libs, "user": user.display_name(), "chats": chats.len() }))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn with_session(method: &str, p: &Value) -> Result<Value, String> {
    let s = crate::session()?;
    match method {
        "user" => Ok(dto::user(&s.refresh_user().map_err(err)?)),
        "updateProfile" => {
            let mut fields = s.refresh_user().map_err(err)?.profile;
            if let Some(v) = opt_str(p, "displayName") {
                fields.display_name = v.trim().to_string();
            }
            if let Some(v) = opt_str(p, "fullName") {
                fields.full_name = v.to_string();
            }
            if let Some(v) = opt_str(p, "shortDescr") {
                fields.short_descr = v.to_string();
            }
            if let Some(v) = opt_str(p, "description") {
                fields.description = v.to_string();
            }
            if let Some(v) = p.get("image") {
                fields.image = v.as_str().filter(|s| !s.is_empty()).map(str::to_string);
            }
            Ok(dto::user(&s.update_profile(fields).map_err(err)?))
        }
        "setDeliveryReceipts" => {
            let u = s.set_delivery_receipts(bool_param(p, "groups")?, bool_param(p, "on")?).map_err(err)?;
            Ok(dto::user(&u))
        }
        "setProfileAddress" => Ok(dto::user(&s.set_profile_address(bool_param(p, "on")?).map_err(err)?)),
        "listChats" => Ok(Value::Array(s.list_chats().map_err(err)?.iter().map(dto::chat).collect())),
        "listMessages" => {
            let count = p.get("count").and_then(Value::as_u64).unwrap_or(100).min(1000) as u32;
            let msgs = s.list_messages(&chat_param(p)?, count).map_err(err)?;
            // `joinedPrev` / `joinedNext`: the message continues the previous
            // run / the next one continues it (core rule, shapes the bubbles).
            let links = cursus_core::runs::run_links(&msgs);
            Ok(Value::Array(
                msgs.iter()
                    .zip(links)
                    .map(|(m, (prev, next))| {
                        let mut v = dto::message(m);
                        v["joinedPrev"] = Value::Bool(prev);
                        v["joinedNext"] = Value::Bool(next);
                        v
                    })
                    .collect(),
            ))
        }
        // Texts go out as every Cursus app sends them (core outgoing_text,
        // links cleaned of tracking codes unless `cleanLinks` is false).
        "sendText" => {
            let quote = p.get("quote").and_then(Value::as_i64);
            let text = cursus_core::outgoing_text(str_param(p, "text")?, clean_links(p));
            s.send_text_quoted(&chat_param(p)?, &text, quote).map_err(err)?;
            Ok(Value::Null)
        }
        "updateText" => {
            let text = cursus_core::outgoing_text(str_param(p, "text")?, clean_links(p));
            s.update_text(&chat_param(p)?, i64_param(p, "itemId")?, &text).map_err(err)?;
            Ok(Value::Null)
        }
        "deleteItems" => {
            let ids: Vec<i64> = p
                .get("itemIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            s.delete_items(&chat_param(p)?, &ids, bool_param(p, "forEveryone")?).map_err(err)?;
            Ok(Value::Null)
        }
        "react" => {
            s.set_reaction(&chat_param(p)?, i64_param(p, "itemId")?, str_param(p, "emoji")?, bool_param(p, "add")?)
                .map_err(err)?;
            Ok(Value::Null)
        }
        // Sent / received time, delivery status and text of one message.
        "messageInfo" => {
            let info = s.message_info(&chat_param(p)?, i64_param(p, "itemId")?).map_err(err)?;
            Ok(json!({ "time": info.time, "status": info.status, "text": info.text }))
        }
        // Forward messages of `chat` to `to`.
        "forward" => {
            let ids: Vec<i64> = p
                .get("itemIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            let to = cursus_core::ChatId { scope: None, ..dto::parse_chat_ref(str_param(p, "to")?)? };
            s.forward_items(&chat_param(p)?, &to, &ids).map_err(err)?;
            Ok(Value::Null)
        }
        // Groups: who set one reaction.
        "reactionMembers" => {
            let members = s
                .reaction_members(&chat_param(p)?, i64_param(p, "itemId")?, str_param(p, "emoji")?)
                .map_err(err)?;
            Ok(Value::Array(members.iter().map(|m| json!(m.display_name)).collect()))
        }
        // A recorded voice message (m4a) with its length in seconds.
        "sendVoice" => {
            let secs = p.get("duration").and_then(Value::as_u64).unwrap_or(0).min(u32::MAX as u64) as u32;
            s.send_voice(&chat_param(p)?, str_param(p, "path")?, secs).map_err(err)?;
            Ok(Value::Null)
        }
        "sendFile" => {
            s.send_file(&chat_param(p)?, str_param(p, "path")?).map_err(err)?;
            Ok(Value::Null)
        }
        "receiveFile" => {
            let approved = p.get("approved").and_then(Value::as_bool).unwrap_or(false);
            match s.receive_file(i64_param(p, "fileId")?, approved).map_err(err)? {
                ReceiveOutcome::Accepted => Ok(json!({ "accepted": true, "servers": [] })),
                ReceiveOutcome::NeedsApproval { servers } => Ok(json!({ "accepted": false, "servers": servers })),
            }
        }
        // Readable path for a received / sent file (decrypts CryptoFiles into
        // the private cache).
        "plainFile" => {
            let path = PathBuf::from(str_param(p, "path")?);
            match (opt_str(p, "key"), opt_str(p, "nonce")) {
                (Some(k), Some(n)) => Ok(json!(s.ensure_plaintext_file(&path, k, n).map_err(err)?.to_string_lossy())),
                _ => Ok(json!(path.to_string_lossy())),
            }
        }
        "markRead" => {
            s.mark_chat_read(&chat_param(p)?).map_err(err)?;
            Ok(Value::Null)
        }
        "markUnread" => {
            s.mark_chat_unread(&chat_param(p)?).map_err(err)?;
            Ok(Value::Null)
        }
        // Favourite / notifications / delivery receipts; fields left out keep
        // their current value. `sendReceipts`: true, false or null (default).
        "setChatSettings" => {
            let chat = chat_param(p)?;
            let current = find_chat(&s, &chat)?.settings;
            let settings = ChatSettings {
                enable_ntfs: opt_str(p, "notifications").map(NtfFilter::parse).unwrap_or(current.enable_ntfs),
                favorite: p.get("favorite").and_then(Value::as_bool).unwrap_or(current.favorite),
                send_rcpts: match p.get("sendReceipts") {
                    None => current.send_rcpts,
                    Some(v) => v.as_bool(),
                },
            };
            s.set_chat_settings(&chat, &settings).map_err(err)?;
            Ok(Value::Null)
        }
        // Delete messages on this Mac after `seconds` (null: the app default).
        "setChatTtl" => {
            let seconds = p.get("seconds").and_then(Value::as_i64);
            s.set_chat_ttl(&chat_param(p)?, seconds).map_err(err)?;
            Ok(Value::Null)
        }
        // `changes`: { feature: "no"|"yes"|"always", timedTtl: secs }.
        "setContactPrefs" => {
            let chat = chat_param(p)?;
            if chat.chat_type != ChatType::Direct {
                return Err("only contacts have chat preferences".into());
            }
            let c = find_chat(&s, &chat)?;
            let (prefs, merged) = (c.contact_preferences.as_ref(), c.contact_merged_preferences.as_ref());
            let changes = p.get("changes").cloned().unwrap_or_else(|| json!({}));
            // Every shown feature: the requested change, else its current value.
            let allows: Vec<(&str, FeatureAllow)> = preferences::CONTACT_FEATURES
                .iter()
                .map(|&key| {
                    let allow = changes
                        .get(key)
                        .and_then(Value::as_str)
                        .map(FeatureAllow::from_api)
                        .unwrap_or_else(|| preferences::contact_feature(prefs, merged, key).mine);
                    (key, allow)
                })
                .collect();
            let ttl = changes
                .get("timedTtl")
                .and_then(Value::as_i64)
                .or_else(|| preferences::contact_timed_ttl(prefs, merged))
                .unwrap_or(preferences::DEFAULT_TIMED_TTL_SECS);
            let json = preferences::contact_preferences_json(prefs, &allows, ttl);
            s.set_contact_prefs(chat.api_id, &json).map_err(err)?;
            Ok(Value::Null)
        }
        "chatTags" => Ok(Value::Array(s.list_chat_tags().map_err(err)?.iter().map(dto::tag).collect())),
        "createChatTag" => Ok(Value::Array(
            s.create_chat_tag(str_param(p, "text")?).map_err(err)?.iter().map(dto::tag).collect(),
        )),
        "setChatTags" => {
            let ids: Vec<i64> = p
                .get("tagIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            s.set_chat_tags(&chat_param(p)?, &ids).map_err(err)?;
            Ok(Value::Null)
        }
        "clearChat" => {
            s.clear_chat(&chat_param(p)?).map_err(err)?;
            Ok(Value::Null)
        }
        "deleteChat" => {
            s.delete_chat(&chat_param(p)?).map_err(err)?;
            Ok(Value::Null)
        }
        "createInvitation" => Ok(dto::link(&s.create_invitation_link().map_err(err)?)),
        "addressLink" => Ok(dto::link(&s.ensure_address_link().map_err(err)?)),
        "addressAutoAccept" => Ok(json!(s.address_auto_accept().map_err(err)?)),
        "setAddressAutoAccept" => Ok(json!(s.set_address_auto_accept(bool_param(p, "on")?).map_err(err)?)),
        "connect" => {
            s.connect_link(str_param(p, "link")?.trim()).map_err(err)?;
            Ok(Value::Null)
        }
        "acceptRequest" => {
            s.accept_contact_request(i64_param(p, "id")?).map_err(err)?;
            Ok(Value::Null)
        }
        "rejectRequest" => {
            s.reject_contact_request(i64_param(p, "id")?).map_err(err)?;
            Ok(Value::Null)
        }
        "joinGroup" => {
            s.join_group(i64_param(p, "groupId")?).map_err(err)?;
            Ok(Value::Null)
        }
        // Local name for a contact or group (empty clears it).
        "setAlias" => {
            let chat = chat_param(p)?;
            let alias = str_param(p, "alias")?;
            match chat.chat_type {
                cursus_core::ChatType::Direct => s.set_contact_alias(chat.api_id, alias).map_err(err)?,
                cursus_core::ChatType::Group => s.set_group_alias(chat.api_id, alias).map_err(err)?,
                _ => return Err("only contacts and groups have a local name".into()),
            };
            Ok(Value::Null)
        }
        "listMembers" => {
            let members = s.list_group_members(i64_param(p, "groupId")?).map_err(err)?;
            Ok(Value::Array(members.iter().map(dto::member).collect()))
        }
        "leaveGroup" => {
            s.leave_group(i64_param(p, "groupId")?).map_err(err)?;
            Ok(Value::Null)
        }
        "createGroup" => Ok(json!(s.create_group(str_param(p, "name")?.trim()).map_err(err)?.command_ref())),
        "createChannel" => {
            let (chat, link) = s.create_channel(str_param(p, "name")?.trim()).map_err(err)?;
            Ok(json!({ "ref": chat.command_ref(), "link": link }))
        }
        // Owners: name, bio, welcome message, picture ("" removes it).
        "updateGroupProfile" => {
            let gid = i64_param(p, "groupId")?;
            let mut profile = group_profile(&group_chat(&s, gid)?);
            if let Some(v) = opt_str(p, "name") {
                profile.display_name = v.to_string();
            }
            if let Some(v) = opt_str(p, "fullName") {
                profile.full_name = v.to_string();
            }
            if let Some(v) = opt_str(p, "shortDescr") {
                profile.short_descr = v.to_string();
            }
            if let Some(v) = opt_str(p, "welcome") {
                profile.welcome_message = v.to_string();
            }
            if let Some(v) = p.get("image") {
                profile.image = v.as_str().filter(|s| !s.is_empty()).map(str::to_string);
            }
            s.update_group_profile(gid, &profile).map_err(err)?;
            Ok(Value::Null)
        }
        // Owners: one group feature on / off (`ttl` for disappearing messages).
        "setGroupPref" => {
            let gid = i64_param(p, "groupId")?;
            let key = str_param(p, "key")?;
            if !preferences::GROUP_FEATURES.contains(&key) {
                return Err(format!("unknown group feature `{key}`"));
            }
            let mut profile = group_profile(&group_chat(&s, gid)?);
            preferences::set_group_feature(
                &mut profile.group_preferences,
                key,
                bool_param(p, "on")?,
                p.get("ttl").and_then(Value::as_i64),
            );
            s.update_group_profile(gid, &profile).map_err(err)?;
            Ok(Value::Null)
        }
        // Owners: admins review new members before they join.
        "setMemberReview" => {
            let gid = i64_param(p, "groupId")?;
            let mut profile = group_profile(&group_chat(&s, gid)?);
            profile.member_admission = bool_param(p, "on")?.then(|| json!({ "review": "all" }));
            s.update_group_profile(gid, &profile).map_err(err)?;
            Ok(Value::Null)
        }
        // Group / channel link (created on first use). Joiners: member / observer.
        "groupLink" => {
            let role = opt_str(p, "role").filter(|r| *r == "observer").unwrap_or("member");
            Ok(dto::link(&s.group_invite_link(i64_param(p, "groupId")?, role).map_err(err)?))
        }
        "setGroupLinkRole" => {
            let role = role_param(p, "role")?;
            s.set_group_link_role(i64_param(p, "groupId")?, &role).map_err(err)?;
            Ok(Value::Null)
        }
        "deleteGroupLink" => {
            s.delete_group_link(i64_param(p, "groupId")?).map_err(err)?;
            Ok(Value::Null)
        }
        "addMember" => {
            let role = role_param(p, "role")?;
            s.add_group_member(i64_param(p, "groupId")?, i64_param(p, "contactId")?, &role).map_err(err)?;
            Ok(Value::Null)
        }
        "setMemberRole" => {
            let role = role_param(p, "role")?;
            s.set_group_member_role(i64_param(p, "groupId")?, i64_param(p, "memberId")?, &role).map_err(err)?;
            Ok(Value::Null)
        }
        "removeMember" => {
            let with_messages = p.get("withMessages").and_then(Value::as_bool).unwrap_or(false);
            s.remove_group_member(i64_param(p, "groupId")?, i64_param(p, "memberId")?, with_messages)
                .map_err(err)?;
            Ok(Value::Null)
        }
        // Moderators: block for everyone.
        "blockMember" => {
            s.block_group_member(i64_param(p, "groupId")?, i64_param(p, "memberId")?, bool_param(p, "blocked")?)
                .map_err(err)?;
            Ok(Value::Null)
        }
        // Anyone: hide a member's messages on this Mac only.
        "setMemberShowMessages" => {
            s.set_member_show_messages(i64_param(p, "groupId")?, i64_param(p, "memberId")?, bool_param(p, "show")?)
                .map_err(err)?;
            Ok(Value::Null)
        }
        // Moderators: close a member's support thread (not for pending joiners).
        "deleteSupportChat" => {
            s.delete_member_support_chat(i64_param(p, "groupId")?, i64_param(p, "memberId")?).map_err(err)?;
            Ok(Value::Null)
        }
        "acceptMember" => {
            let role = opt_str(p, "role").unwrap_or("member");
            s.accept_group_member(i64_param(p, "groupId")?, i64_param(p, "memberId")?, role).map_err(err)?;
            Ok(Value::Null)
        }
        // Direct chat with a member (group allows direct messages): `@contactId`.
        "memberContact" => {
            let contact = s.create_member_contact(i64_param(p, "groupId")?, i64_param(p, "memberId")?).map_err(err)?;
            Ok(json!(format!("@{contact}")))
        }
        "setEncryptLocalFiles" => {
            let on = bool_param(p, "on")?;
            s.set_encrypt_local_files(on).map_err(err)?;
            if let Some(mut meta) = cursus_core::load_account() {
                meta.encrypt_local_files = on;
                cursus_core::save_account(&meta).map_err(err)?;
            }
            Ok(Value::Null)
        }
        other => Err(format!("unknown method: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_checked_and_cleaned() {
        let web = prepare_link(&json!({ "url": "https://a.b/x?utm_source=n&id=1" })).unwrap();
        assert_eq!(web["url"], "https://a.b/x?id=1");
        let kept = prepare_link(&json!({ "url": "https://a.b/x?utm_source=n", "clean": false })).unwrap();
        assert_eq!(kept["url"], "https://a.b/x?utm_source=n");
        assert_eq!(prepare_link(&json!({ "url": "file:///etc/passwd" })).unwrap()["url"], Value::Null);
        let sx = prepare_link(&json!({ "url": "https://simplex.chat/contact#/?v=2-7&smp=x" })).unwrap();
        assert!(sx["simplex"].is_string(), "{sx}");
    }

    #[test]
    fn composer_emoji_helpers() {
        let hits = dispatch("emojiSuggest", &json!({ "text": "hi :smi" })).unwrap();
        assert!(!hits.as_array().unwrap().is_empty());
        let done = dispatch("acceptShortcode", &json!({ "text": "hi :smi", "glyph": "😄" })).unwrap();
        assert_eq!(done, "hi 😄");
        assert_eq!(dispatch("emojiSuggest", &json!({ "text": "hi" })).unwrap(), json!([]));
    }
}
