//! Builds SimpleX chat command strings (see bots/api/COMMANDS.md).

use serde_json::{json, Value};

use crate::models::{CallMedia, ChatId, ChatSettings, ChatType, GroupProfileUpdate, ProfileFields};

pub struct Commands;

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// `1,2,3`
fn ids(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// JSON string, or `null` for an empty / missing one.
fn text_or_null(text: Option<&str>) -> Value {
    text.filter(|s| !s.is_empty()).map_or(Value::Null, Value::from)
}

/// `/_set alias <target>[ <alias>]` — no alias clears it.
fn set_alias(target: String, alias: &str) -> String {
    format!("/_set alias {target} {}", alias.trim()).trim_end().to_string()
}

impl Commands {
    pub fn show_active_user() -> &'static str {
        "/u"
    }

    pub fn create_active_user(display_name: &str) -> String {
        // Matches `/_create user` JSON used by official clients.
        let profile = json!({
            "profile": {
                "displayName": display_name,
                "fullName": "",
                "shortDescr": null
            },
            "pastTimestamp": false
        });
        format!("/_create user {profile}")
    }

    pub fn start_chat() -> &'static str {
        "/_start main=on"
    }

    pub fn stop_chat() -> &'static str {
        "/_stop"
    }

    /// Foreground / restore subscriptions (`/_app activate restore=on`).
    pub fn activate_chat() -> &'static str {
        "/_app activate restore=on"
    }

    /// Re-subscribe SMP connections (`/_resubscribe all`).
    pub fn resubscribe_all() -> &'static str {
        "/_resubscribe all"
    }

    /// Chat-layer temp dir (`/_temp_folder <path>`).
    pub fn set_temp_folder(path: &str) -> String {
        format!("/_temp_folder {path}")
    }

    /// App files dir (`/files_folder <path>`).
    pub fn set_files_folder(path: &str) -> String {
        format!("/files_folder {path}")
    }

    pub fn get_chats(user_id: i64) -> String {
        format!("/_get chats {user_id} pcc=on")
    }

    /// Load recent items for a chat (`/_get chat {ref} count=N`).
    pub fn get_chat(chat: &ChatId, count: u32) -> String {
        format!("/_get chat {} count={count}", chat.command_ref())
    }

    pub fn show_address(user_id: i64) -> String {
        format!("/_show_address {user_id}")
    }

    pub fn create_address(user_id: i64) -> String {
        format!("/_address {user_id}")
    }

    /// Delete the reusable address (`/_delete_address <userId>`); contacts
    /// made through it stay connected.
    pub fn delete_address(user_id: i64) -> String {
        format!("/_delete_address {user_id}")
    }

    /// Update active user profile (`/_profile {userId} {json}`).
    pub fn update_profile(user_id: i64, profile: &ProfileFields) -> String {
        let mut payload = json!({
            "displayName": profile.display_name,
            "fullName": profile.full_name,
            "shortDescr": text_or_null(Some(&profile.short_descr)),
            "description": text_or_null(Some(&profile.description)),
            "image": text_or_null(profile.image.as_deref()),
        });
        if let Some(link) = profile.contact_link.as_ref().filter(|s| !s.is_empty()) {
            payload["contactLink"] = json!(link);
        }
        if let Some(prefs) = profile.preferences.as_ref() {
            payload["preferences"] = prefs.clone();
        }
        format!("/_profile {user_id} {payload}")
    }

    /// Share / unshare the user address on the profile (`/_profile_address {userId} on|off`).
    pub fn profile_address(user_id: i64, on: bool) -> String {
        format!("/_profile_address {user_id} {}", on_off(on))
    }

    /// Set a local contact alias (`/_set alias @{contactId} {alias}`).
    pub fn set_contact_alias(contact_id: i64, alias: &str) -> String {
        set_alias(format!("@{contact_id}"), alias)
    }

    /// Override contact feature prefs (`/_set prefs @{contactId} {json}`).
    ///
    /// Contact feature prefs (`timedMessages`, `fullDelete`, `reactions`, `voice`, `calls`, …).
    pub fn set_contact_prefs(contact_id: i64, prefs: &Value) -> String {
        format!("/_set prefs @{contact_id} {prefs}")
    }

    /// Update reusable address settings (`/_address_settings {userId} {json}`).
    ///
    /// `auto_accept`: `Some(false)` → accept without incognito; `None` → disable.
    pub fn address_settings(user_id: i64, business_address: bool, auto_accept: Option<bool>) -> String {
        let settings = json!({
            "businessAddress": business_address,
            "autoAccept": auto_accept.map(|incognito| json!({ "acceptIncognito": incognito })),
            "autoReply": null,
        });
        format!("/_address_settings {user_id} {settings}")
    }

    /// One-time invitation link (`/_connect <userId> incognito=on|off`, no
    /// link argument). Official clients always send the incognito flag.
    pub fn create_invitation(user_id: i64, incognito: bool) -> String {
        format!("/_connect {user_id} incognito={}", on_off(incognito))
    }

    /// Switch an unused one-time invitation between the user's profile and a
    /// random incognito one (`/_set incognito :<connId> on|off`).
    pub fn set_connection_incognito(conn_id: i64, incognito: bool) -> String {
        format!("/_set incognito :{conn_id} {}", on_off(incognito))
    }

    /// Name a pending contact connection (`/_set alias :<connId> <name>`;
    /// no name clears it).
    pub fn set_connection_alias(conn_id: i64, alias: &str) -> String {
        set_alias(format!(":{conn_id}"), alias)
    }

    /// Delete a pending contact connection (`/_delete :<connId>`), e.g. the
    /// unused one-time invitation a regenerated link replaces.
    pub fn delete_pending_connection(conn_id: i64) -> String {
        format!("/_delete :{conn_id}")
    }

    /// Connect via invitation / address / group link.
    ///
    /// Prefer `/connect <target>` (not `/_connect <userId> <link>`). Short
    /// HTTPS invites (`https://…/i#…`, `/a#…`) are resolved by the core only
    /// on the stringy `/connect` path; `/_connect` expects an already-prepared
    /// `CreatedConnLink` and returns `invalidConnReq` for bare short links.
    /// `incognito` connects with a new random profile (`/connect incognito …`).
    pub fn connect_link(link: &str, incognito: bool) -> String {
        format!("/connect {}{link}", if incognito { "incognito " } else { "" })
    }

    /// Mark all items in a chat as read (`/_read chat {ref}`).
    pub fn read_chat(chat: &ChatId) -> String {
        format!("/_read chat {}", chat.command_ref())
    }

    /// Accept an inbound contact request (`/_accept incognito=off <contactReqId>`).
    pub fn accept_contact_request(contact_req_id: i64) -> String {
        format!("/_accept incognito=off {contact_req_id}")
    }

    /// Reject an inbound contact request (`/_reject <contactReqId>`).
    pub fn reject_contact_request(contact_req_id: i64) -> String {
        format!("/_reject {contact_req_id}")
    }

    /// Mark chat soft-unread / clear soft-unread (`/_unread chat {ref} on|off`).
    pub fn unread_chat(chat: &ChatId, unread: bool) -> String {
        format!("/_unread chat {} {}", chat.command_ref(), on_off(unread))
    }

    /// Update favorite / notification / receipt settings (`/_settings {ref} {json}`).
    pub fn set_chat_settings(chat: &ChatId, settings: &ChatSettings) -> String {
        let payload = json!({
            "enableNtfs": settings.enable_ntfs.as_api(),
            "sendRcpts": settings.send_rcpts,
            "favorite": settings.favorite,
        });
        format!("/_settings {} {payload}", chat.command_ref())
    }

    /// Per-chat local message TTL (`/_ttl {userId} {ref} {seconds|default}`).
    /// `None` restores the user default (not “never”).
    pub fn set_chat_ttl(user_id: i64, chat: &ChatId, seconds: Option<i64>) -> String {
        let ttl = seconds.map_or("default".to_string(), |secs| secs.to_string());
        format!("/_ttl {user_id} {} {ttl}", chat.command_ref())
    }

    /// Clear chat history (`/_clear chat {ref}`).
    pub fn clear_chat(chat: &ChatId) -> String {
        format!("/_clear chat {}", chat.command_ref())
    }

    /// Delete chat / contact (`/_delete {ref} full notify=on`).
    pub fn delete_chat(chat: &ChatId) -> String {
        format!("/_delete {} full notify=on", chat.command_ref())
    }

    pub fn get_chat_tags(user_id: i64) -> String {
        format!("/_get tags {user_id}")
    }

    pub fn create_chat_tag(text: &str) -> String {
        let payload = json!({ "emoji": null, "text": text });
        format!("/_create tag {payload}")
    }

    /// Assign lists/tags to a chat (`/_tags {ref} id,id` or bare ref to clear).
    pub fn set_chat_tags(chat: &ChatId, tag_ids: &[i64]) -> String {
        format!("/_tags {} {}", chat.command_ref(), ids(tag_ids)).trim_end().to_string()
    }

    /// Send one message: `content` is a `MsgContent` (see [`Self::text`],
    /// [`Self::image`], [`Self::file`], [`Self::voice`]), `file` its
    /// attachment and `quoted_item_id` the message it replies to.
    ///
    /// Direct chats and groups use `/_send`. Local notes must use
    /// `/_create *<id>` — the core's parser rejects `/_send *…`.
    pub fn send_message(chat: &ChatId, content: Value, file: Option<&OutgoingFile>, quoted_item_id: Option<i64>) -> String {
        let msgs = json!([{
            "fileSource": file.map(|f| json!({
                "filePath": f.path,
                "cryptoArgs": f.crypto.as_ref().map(|(key, nonce)| json!({ "fileKey": key, "fileNonce": nonce })),
            })),
            "quotedItemId": quoted_item_id,
            "msgContent": content,
            "mentions": {}
        }]);
        match chat.chat_type {
            ChatType::Local => format!("/_create *{} json {msgs}", chat.api_id),
            _ => format!("/_send {} live=off ttl=default sign=off json {msgs}", chat.command_ref()),
        }
    }

    pub fn text(text: &str) -> Value {
        json!({ "type": "text", "text": text })
    }

    /// A photo; `preview` is its small inline data-URL.
    pub fn image(preview: &str) -> Value {
        json!({ "type": "image", "text": "", "image": preview })
    }

    pub fn file() -> Value {
        json!({ "type": "file", "text": "" })
    }

    pub fn voice(duration_secs: u32) -> Value {
        json!({ "type": "voice", "text": "", "duration": duration_secs })
    }

    /// Update an existing chat item's text (`/_update item …`).
    ///
    /// Only works while SimpleX marks the item `editable` (outgoing, not
    /// forwarded, typically within 24h). Core returns `invalidChatItemUpdate`
    /// once that window closes.
    pub fn update_text(chat: &ChatId, item_id: i64, text: &str) -> String {
        let payload = json!({ "msgContent": Self::text(text), "mentions": {} });
        format!("/_update item {} {item_id} live=off json {payload}", chat.command_ref())
    }

    /// `/_set receipts contacts|groups <userId> on|off` (keeps per-chat overrides).
    pub fn set_delivery_receipts(user_id: i64, groups: bool, on: bool) -> String {
        let scope = if groups { "groups" } else { "contacts" };
        format!("/_set receipts {scope} {user_id} {}", on_off(on))
    }

    /// Accept an incoming file (`/freceive <fileId> …`).
    ///
    /// Path is omitted so the core uses [`Self::set_files_folder`]; that
    /// matches official clients and keeps relative paths inside the app
    /// files dir. `encrypt` stores CryptoFile ciphertext under `files/` (keys
    /// in the DB). `approved_relays=off` makes the core refuse file servers
    /// that are not ours (`fileNotApproved`), so the sender can't learn our IP
    /// address without consent — the official "Protect IP address" default.
    pub fn receive_file(file_id: i64, approved_relays: bool, encrypt: bool) -> String {
        format!("/freceive {file_id} approved_relays={} encrypt={}", on_off(approved_relays), on_off(encrypt))
    }

    /// Toggle core `encryptLocalFiles` (`/_files_encrypt on|off`).
    pub fn set_encrypt_local_files(on: bool) -> String {
        format!("/_files_encrypt {}", on_off(on))
    }

    /// Add or remove an emoji reaction (`/_reaction {ref} {itemId} on|off {json}`).
    ///
    /// `emoji` must be one of the protocol-allowed chars (see `KNOWN_REACTIONS`).
    pub fn reaction(chat: &ChatId, item_id: i64, emoji: &str, add: bool) -> String {
        let payload = json!({ "type": "emoji", "emoji": emoji });
        format!("/_reaction {} {item_id} {} {payload}", chat.command_ref(), on_off(add))
    }

    /// List group members who set a reaction (`/_reaction members {userId} #{groupId} {itemId} {json}`).
    pub fn reaction_members(user_id: i64, group_id: i64, item_id: i64, emoji: &str) -> String {
        let payload = json!({ "type": "emoji", "emoji": emoji });
        format!("/_reaction members {user_id} #{group_id} {item_id} {payload}")
    }

    /// Delete chat items (`/_delete item {ref} {ids} broadcast|internal`).
    ///
    /// Use `broadcast` to mark-delete for the peer (when Full Delete is off the
    /// item stays with `itemDeleted`). Use `internal` to remove locally — required
    /// for items that are already marked deleted (`deletable` is false).
    pub fn delete_items(chat: &ChatId, item_ids: &[i64], broadcast: bool) -> String {
        let mode = if broadcast && chat.chat_type != ChatType::Local { "broadcast" } else { "internal" };
        format!("/_delete item {} {} {mode}", chat.command_ref(), ids(item_ids))
    }

    /// List group members (`/_members #{groupId}`).
    pub fn list_members(group_id: i64) -> String {
        format!("/_members #{group_id}")
    }

    /// Invite a contact into a group (`/_add #{groupId} {contactId} {role}`).
    pub fn add_member(group_id: i64, contact_id: i64, role: &str) -> String {
        format!("/_add #{group_id} {contact_id} {role}")
    }

    /// Change member role(s) (`/_member role #{groupId} {ids} {role}`).
    pub fn set_member_role(group_id: i64, member_ids: &[i64], role: &str) -> String {
        format!("/_member role #{group_id} {} {role}", ids(member_ids))
    }

    /// Remove members (`/_remove #{groupId} {ids} messages=on|off`).
    pub fn remove_members(group_id: i64, member_ids: &[i64], with_messages: bool) -> String {
        format!("/_remove #{group_id} {} messages={}", ids(member_ids), on_off(with_messages))
    }

    /// Leave group (`/_leave #{groupId}`).
    pub fn leave_group(group_id: i64) -> String {
        format!("/_leave #{group_id}")
    }

    /// Accept a pending group invitation (`/_join #{groupId}`).
    pub fn join_group(group_id: i64) -> String {
        format!("/_join #{group_id}")
    }

    /// Local hide / show member messages (`/_member settings #{gid} {mid} {json}`).
    pub fn set_member_settings(group_id: i64, member_id: i64, show_messages: bool) -> String {
        let payload = json!({ "showMessages": show_messages });
        format!("/_member settings #{group_id} {member_id} {payload}")
    }

    /// Block / unblock members for everyone (`/_block #{gid} {ids} blocked=on|off`).
    pub fn block_members(group_id: i64, member_ids: &[i64], blocked: bool) -> String {
        format!("/_block #{group_id} {} blocked={}", ids(member_ids), on_off(blocked))
    }

    /// Accept pending member (`/_accept member #{gid} {mid} {role}`).
    pub fn accept_member(group_id: i64, member_id: i64, role: &str) -> String {
        format!("/_accept member #{group_id} {member_id} {role}")
    }

    /// Create a direct contact with a group member (`APICreateMemberContact`).
    pub fn create_member_contact(group_id: i64, member_id: i64) -> String {
        format!("/_create member contact #{group_id} {member_id}")
    }

    /// Send the direct-connection invitation for a member contact
    /// (`APISendMemberContactInvitation`, message content optional).
    pub fn invite_member_contact(contact_id: i64) -> String {
        format!("/_invite member contact @{contact_id}")
    }

    /// Local group alias (`/_set alias #{groupId} {alias}`).
    pub fn set_group_alias(group_id: i64, alias: &str) -> String {
        set_alias(format!("#{group_id}"), alias)
    }

    /// Update group profile (`/_group_profile #{groupId} {json}`) — full profile round-trip.
    pub fn update_group_profile(group_id: i64, profile: &GroupProfileUpdate) -> String {
        let payload = json!({
            "displayName": profile.display_name,
            "fullName": profile.full_name,
            "shortDescr": text_or_null(Some(&profile.short_descr)),
            "description": text_or_null(Some(&profile.welcome_message)),
            "image": text_or_null(profile.image.as_deref()),
            "groupPreferences": profile.group_preferences,
            "memberAdmission": profile.member_admission,
            "publicGroup": profile.public_group,
        });
        format!("/_group_profile #{group_id} {payload}")
    }

    /// Create a join link (`/_create link #{groupId} {role}`).
    pub fn create_group_link(group_id: i64, member_role: &str) -> String {
        format!("/_create link #{group_id} {member_role}")
    }

    /// Show existing group link (`/_get link #{groupId}`).
    pub fn get_group_link(group_id: i64) -> String {
        format!("/_get link #{group_id}")
    }

    /// Change default role for link joiners (`/_set link role #{groupId} {role}`).
    pub fn set_group_link_role(group_id: i64, role: &str) -> String {
        format!("/_set link role #{group_id} {role}")
    }

    /// Delete group link (`/_delete link #{groupId}`).
    pub fn delete_group_link(group_id: i64) -> String {
        format!("/_delete link #{group_id}")
    }

    /// Delete a member’s support chat (`/_delete member chat #{groupId} {groupMemberId}`).
    pub fn delete_member_support_chat(group_id: i64, group_member_id: i64) -> String {
        format!("/_delete member chat #{group_id} {group_member_id}")
    }

    /// Add a short / profile link (`/_short link #{groupId}`).
    pub fn short_group_link(group_id: i64) -> String {
        format!("/_short link #{group_id}")
    }

    /// Create a private group (`/_group {userId} incognito=off {json(GroupProfile)}`).
    pub fn new_group(user_id: i64, display_name: &str) -> String {
        let profile = json!({
            "displayName": display_name,
            "fullName": "",
            "shortDescr": null,
            "image": null,
        });
        // Official clients always emit `incognito=on|off` (see ApiNewGroup).
        format!("/_group {user_id} incognito=off {profile}")
    }

    /// Create a channel / public group (`/_public group {userId} incognito=off {relayIds} {json}`).
    pub fn new_public_group(user_id: i64, relay_ids: &[i64], display_name: &str) -> String {
        let relays = ids(relay_ids);
        let profile = json!({
            "displayName": display_name,
            "fullName": "",
            "shortDescr": null,
            "image": null,
            "groupPreferences": {
                "history": { "enable": "on" },
                "support": { "enable": "off" }
            }
        });
        // Match official ApiNewPublicGroup — include incognito= so relay ids parse as `_strP`.
        format!("/_public group {user_id} incognito=off {relays} {profile}")
    }

    /// User operator / chat-relay servers (`/_servers {userId}`).
    pub fn get_user_servers(user_id: i64) -> String {
        format!("/_servers {user_id}")
    }

    /// Start a WebRTC call invite (`/_call invite @{id} {CallType}`).
    pub fn call_invite(contact_id: i64, media: CallMedia) -> String {
        let payload = json!({
            "media": media.as_api(),
            "capabilities": { "encryption": true }
        });
        format!("/_call invite @{contact_id} {payload}")
    }

    pub fn call_reject(contact_id: i64) -> String {
        format!("/_call reject @{contact_id}")
    }

    pub fn call_end(contact_id: i64) -> String {
        format!("/_call end @{contact_id}")
    }

    /// Newest chat items across all chats (`before` = older than that item id).
    pub fn get_items(count: u32, before: Option<i64>) -> String {
        let before = before.map(|id| format!("before={id} ")).unwrap_or_default();
        format!("/_get items {before}count={count}")
    }

    /// Callee → caller: WebRTC offer (`/_call offer @{id} {WebRTCCallOffer}`).
    /// `rtc_session` / `rtc_ice_candidates` are LZString-base64 JSON, as the
    /// official clients send them.
    pub fn call_offer(
        contact_id: i64,
        media: CallMedia,
        encryption: bool,
        rtc_session: &str,
        rtc_ice_candidates: &str,
    ) -> String {
        let payload = json!({
            "callType": {
                "media": media.as_api(),
                "capabilities": { "encryption": encryption }
            },
            "rtcSession": {
                "rtcSession": rtc_session,
                "rtcIceCandidates": rtc_ice_candidates
            }
        });
        format!("/_call offer @{contact_id} {payload}")
    }

    /// Caller → callee: WebRTC answer (`/_call answer @{id} {WebRTCSession}`).
    pub fn call_answer(contact_id: i64, rtc_session: &str, rtc_ice_candidates: &str) -> String {
        let payload = json!({
            "rtcSession": rtc_session,
            "rtcIceCandidates": rtc_ice_candidates
        });
        format!("/_call answer @{contact_id} {payload}")
    }

    /// Late ICE candidates (`/_call extra @{id} {WebRTCExtraInfo}`).
    pub fn call_extra(contact_id: i64, rtc_ice_candidates: &str) -> String {
        let payload = json!({ "rtcIceCandidates": rtc_ice_candidates });
        format!("/_call extra @{contact_id} {payload}")
    }

    /// Media connection state for the call chat item
    /// (`connecting` | `connected` | `disconnected` | `failed`).
    pub fn call_status(contact_id: i64, status: &str) -> String {
        format!("/_call status @{contact_id} {status}")
    }

    /// Item metadata (`/_get item info {ref} {itemId}`).
    pub fn item_info(chat: &ChatId, item_id: i64) -> String {
        format!("/_get item info {} {item_id}", chat.command_ref())
    }

    /// Forward items to another chat (`/_forward {to} as_group=off {from} {ids} ttl=default`).
    pub fn forward_items(from: &ChatId, to: &ChatId, item_ids: &[i64]) -> String {
        format!("/_forward {} as_group=off {} {} ttl=default", to.command_ref(), from.command_ref(), ids(item_ids))
    }
}

/// An attachment as the core takes it in `fileSource`.
pub struct OutgoingFile {
    /// Name inside the files folder.
    pub path: String,
    /// CryptoFile key and nonce when the file is encrypted at rest.
    pub crypto: Option<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NtfFilter;

    fn direct(id: i64) -> ChatId {
        ChatId::new(ChatType::Direct, id)
    }

    #[test]
    fn messages_go_to_send_or_notes_create() {
        let text = Commands::send_message(&direct(2), Commands::text("hello"), None, Some(7));
        assert!(text.starts_with("/_send @2 live=off ttl=default sign=off json "), "{text}");
        assert!(text.contains(r#""quotedItemId":7"#) && text.contains(r#""fileSource":null"#), "{text}");
        // The core's parser rejects `/_send *…` for the notes folder.
        let note = Commands::send_message(&ChatId::new(ChatType::Local, 1), Commands::text("hello"), None, None);
        assert!(note.starts_with("/_create *1 json "), "{note}");
        let support = Commands::send_message(&ChatId::with_support(42, None), Commands::text("help"), None, None);
        assert!(support.starts_with("/_send #42(_support) "), "support sends keep the scope: {support}");
    }

    #[test]
    fn attachments_carry_their_file_source() {
        let plain = OutgoingFile { path: "doc.pdf".into(), crypto: None };
        let file = Commands::send_message(&direct(2), Commands::file(), Some(&plain), None);
        assert!(file.contains(r#""type":"file""#) && file.contains(r#""filePath":"doc.pdf""#), "{file}");
        assert!(file.contains(r#""cryptoArgs":null"#), "{file}");
        let encrypted = OutgoingFile { path: "voice.m4a".into(), crypto: Some(("k".into(), "n".into())) };
        let voice = Commands::send_message(&direct(2), Commands::voice(12), Some(&encrypted), None);
        assert!(voice.contains(r#""type":"voice""#) && voice.contains(r#""duration":12"#), "{voice}");
        assert!(voice.contains(r#""fileKey":"k""#) && voice.contains(r#""fileNonce":"n""#), "{voice}");
        let image = Commands::send_message(&direct(2), Commands::image("data:image/jpg;base64,xx"), Some(&plain), None);
        assert!(image.contains(r#""type":"image""#) && image.contains(r#""image":"data:image/jpg;base64,xx""#), "{image}");
        assert_eq!(Commands::receive_file(3, false, true), "/freceive 3 approved_relays=off encrypt=on");
        assert_eq!(Commands::receive_file(3, true, false), "/freceive 3 approved_relays=on encrypt=off");
        assert_eq!(Commands::set_encrypt_local_files(false), "/_files_encrypt off");
    }

    #[test]
    fn chat_commands() {
        let chat = direct(9);
        assert_eq!(Commands::get_chat(&ChatId::new(ChatType::Local, 1), 50), "/_get chat *1 count=50");
        assert_eq!(Commands::get_chat(&ChatId::with_support(42, Some(7)), 30), "/_get chat #42(_support:7) count=30");
        assert_eq!(Commands::read_chat(&chat), "/_read chat @9");
        assert_eq!(Commands::unread_chat(&chat, true), "/_unread chat @9 on");
        assert_eq!(Commands::clear_chat(&chat), "/_clear chat @9");
        assert_eq!(Commands::delete_chat(&chat), "/_delete @9 full notify=on");
        let settings = ChatSettings { enable_ntfs: NtfFilter::None, favorite: true, send_rcpts: Some(true) };
        assert_eq!(
            Commands::set_chat_settings(&chat, &settings),
            r#"/_settings @9 {"enableNtfs":"none","favorite":true,"sendRcpts":true}"#
        );
        assert_eq!(Commands::set_chat_ttl(1, &chat, Some(86400)), "/_ttl 1 @9 86400");
        assert_eq!(Commands::set_chat_ttl(1, &chat, None), "/_ttl 1 @9 default");
        assert_eq!(Commands::set_chat_tags(&chat, &[1, 2]), "/_tags @9 1,2");
        assert_eq!(Commands::set_chat_tags(&chat, &[]), "/_tags @9");
        assert_eq!(Commands::get_chat_tags(1), "/_get tags 1");
        assert_eq!(Commands::reaction(&chat, 99, "👍", true), r#"/_reaction @9 99 on {"emoji":"👍","type":"emoji"}"#);
        assert_eq!(
            Commands::reaction_members(1, 9, 42, "👍"),
            r#"/_reaction members 1 #9 42 {"emoji":"👍","type":"emoji"}"#
        );
        assert_eq!(Commands::delete_items(&chat, &[4, 5], true), "/_delete item @9 4,5 broadcast");
        assert_eq!(Commands::delete_items(&ChatId::new(ChatType::Local, 1), &[4], true), "/_delete item *1 4 internal");
        assert_eq!(Commands::forward_items(&chat, &direct(3), &[4, 5]), "/_forward @3 as_group=off @9 4,5 ttl=default");
        assert_eq!(Commands::get_items(5, None), "/_get items count=5");
        assert_eq!(Commands::get_items(5, Some(9)), "/_get items before=9 count=5");
    }

    #[test]
    fn connection_commands() {
        assert_eq!(Commands::create_invitation(1, true), "/_connect 1 incognito=on");
        assert_eq!(Commands::set_connection_incognito(7, true), "/_set incognito :7 on");
        assert_eq!(Commands::delete_pending_connection(7), "/_delete :7");
        assert_eq!(Commands::set_connection_alias(7, " For Anna "), "/_set alias :7 For Anna");
        assert_eq!(Commands::set_connection_alias(7, ""), "/_set alias :7");
        assert_eq!(Commands::connect_link("https://x/i#a", true), "/connect incognito https://x/i#a");
        assert_eq!(Commands::connect_link("https://x/i#a", false), "/connect https://x/i#a");
        assert_eq!(Commands::show_address(1), "/_show_address 1");
        assert_eq!(Commands::delete_address(3), "/_delete_address 3");
        let on = Commands::address_settings(1, false, Some(false));
        assert!(on.starts_with("/_address_settings 1 ") && on.contains(r#""autoAccept":{"acceptIncognito":false}"#), "{on}");
        assert!(Commands::address_settings(1, false, None).contains(r#""autoAccept":null"#));
        assert_eq!(Commands::accept_contact_request(1), "/_accept incognito=off 1");
        assert_eq!(Commands::reject_contact_request(1), "/_reject 1");
    }

    #[test]
    fn profile_commands() {
        let fields = ProfileFields { display_name: "Ada".into(), short_descr: "bio".into(), ..ProfileFields::default() };
        let cmd = Commands::update_profile(1, &fields);
        assert!(cmd.starts_with("/_profile 1 "), "{cmd}");
        assert!(cmd.contains(r#""displayName":"Ada""#) && cmd.contains(r#""shortDescr":"bio""#), "{cmd}");
        assert!(cmd.contains(r#""description":null"#) && cmd.contains(r#""image":null"#), "{cmd}");
        assert_eq!(Commands::profile_address(1, true), "/_profile_address 1 on");
        assert_eq!(Commands::set_contact_alias(3, "Bob"), "/_set alias @3 Bob");
        assert_eq!(Commands::set_contact_alias(3, ""), "/_set alias @3");
        assert_eq!(Commands::set_delivery_receipts(1, true, false), "/_set receipts groups 1 off");
        let prefs = json!({ "fullDelete": { "allow": "no" } });
        assert_eq!(Commands::set_contact_prefs(3, &prefs), r#"/_set prefs @3 {"fullDelete":{"allow":"no"}}"#);
    }

    #[test]
    fn group_commands() {
        let group = Commands::new_group(1, "Friends");
        assert!(group.starts_with("/_group 1 incognito=off ") && group.contains(r#""displayName":"Friends""#), "{group}");
        let channel = Commands::new_public_group(1, &[10, 20], "News");
        assert!(channel.starts_with("/_public group 1 incognito=off 10,20 "), "{channel}");
        assert_eq!(Commands::list_members(9), "/_members #9");
        assert_eq!(Commands::add_member(9, 3, "member"), "/_add #9 3 member");
        assert_eq!(Commands::set_member_role(9, &[4], "admin"), "/_member role #9 4 admin");
        assert_eq!(Commands::remove_members(9, &[4], false), "/_remove #9 4 messages=off");
        assert_eq!(Commands::block_members(9, &[4, 5], true), "/_block #9 4,5 blocked=on");
        assert_eq!(Commands::delete_member_support_chat(42, 7), "/_delete member chat #42 7");
        assert_eq!(Commands::create_group_link(9, "member"), "/_create link #9 member");
        assert_eq!(Commands::set_group_link_role(9, "observer"), "/_set link role #9 observer");
        assert_eq!(Commands::set_group_alias(9, "Work"), "/_set alias #9 Work");
        let profile = GroupProfileUpdate {
            display_name: "G".into(),
            image: Some("data:image/jpg;base64,xx".into()),
            ..GroupProfileUpdate::default()
        };
        let update = Commands::update_group_profile(9, &profile);
        assert!(update.starts_with("/_group_profile #9 "), "{update}");
        assert!(update.contains(r#""image":"data:image/jpg;base64,xx""#) && update.contains(r#""groupPreferences":null"#), "{update}");
    }

    #[test]
    fn call_commands() {
        let invite = Commands::call_invite(5, CallMedia::Audio);
        assert!(invite.starts_with("/_call invite @5 ") && invite.contains(r#""media":"audio""#), "{invite}");
        assert_eq!(Commands::call_reject(5), "/_call reject @5");
        assert_eq!(Commands::call_end(5), "/_call end @5");
        assert_eq!(Commands::call_status(5, "connected"), "/_call status @5 connected");
    }
}
