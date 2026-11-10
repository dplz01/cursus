//! Builds SimpleX chat command strings (see bots/api/COMMANDS.md).

use crate::models::{ChatId, ChatSettings, ChatType, ProfileFields};

pub struct Commands;

impl Commands {
    pub fn show_active_user() -> &'static str {
        "/u"
    }

    pub fn create_active_user(display_name: &str) -> String {
        // Matches `/_create user` JSON used by official clients.
        let profile = serde_json::json!({
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

    /// Update active user profile (`/_profile {userId} {json}`).
    pub fn update_profile(user_id: i64, profile: &ProfileFields) -> String {
        let mut payload = serde_json::json!({
            "displayName": profile.display_name,
            "fullName": profile.full_name,
            "shortDescr": if profile.short_descr.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(profile.short_descr.clone())
            },
            "description": if profile.description.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(profile.description.clone())
            },
            "image": profile
                .image
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::Value::String(s.clone()))
                .unwrap_or(serde_json::Value::Null),
        });
        if let Some(link) = profile.contact_link.as_ref().filter(|s| !s.is_empty()) {
            payload["contactLink"] = serde_json::Value::String(link.clone());
        }
        if let Some(prefs) = profile.preferences.as_ref() {
            payload["preferences"] = prefs.clone();
        }
        format!("/_profile {user_id} {payload}")
    }

    /// Share / unshare the user address on the profile (`/_profile_address {userId} on|off`).
    pub fn profile_address(user_id: i64, on: bool) -> String {
        let flag = if on { "on" } else { "off" };
        format!("/_profile_address {user_id} {flag}")
    }

    /// Set a local contact alias (`/_set alias @{contactId} {alias}`).
    pub fn set_contact_alias(contact_id: i64, alias: &str) -> String {
        if alias.is_empty() {
            format!("/_set alias @{contact_id}")
        } else {
            format!("/_set alias @{contact_id} {alias}")
        }
    }

    /// Override contact feature prefs (`/_set prefs @{contactId} {json}`).
    ///
    /// Contact feature prefs (`timedMessages`, `fullDelete`, `reactions`, `voice`, `calls`, …).
    pub fn set_contact_prefs(contact_id: i64, prefs: &serde_json::Value) -> String {
        format!("/_set prefs @{contact_id} {prefs}")
    }

    /// Update reusable address settings (`/_address_settings {userId} {json}`).
    ///
    /// `auto_accept`: `Some(false)` → accept without incognito; `None` → disable.
    pub fn address_settings(user_id: i64, business_address: bool, auto_accept: Option<bool>) -> String {
        let settings = match auto_accept {
            Some(accept_incognito) => serde_json::json!({
                "businessAddress": business_address,
                "autoAccept": { "acceptIncognito": accept_incognito },
                "autoReply": null,
            }),
            None => serde_json::json!({
                "businessAddress": business_address,
                "autoAccept": null,
                "autoReply": null,
            }),
        };
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
    pub fn connect_link(_user_id: i64, link: &str, incognito: bool) -> String {
        if incognito {
            format!("/connect incognito {link}")
        } else {
            format!("/connect {link}")
        }
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
        let flag = if unread { "on" } else { "off" };
        format!("/_unread chat {} {flag}", chat.command_ref())
    }

    /// Update favorite / notification / receipt settings (`/_settings {ref} {json}`).
    pub fn set_chat_settings(chat: &ChatId, settings: &ChatSettings) -> String {
        let payload = serde_json::json!({
            "enableNtfs": settings.enable_ntfs.as_api(),
            "sendRcpts": settings.send_rcpts,
            "favorite": settings.favorite,
        });
        format!("/_settings {} {payload}", chat.command_ref())
    }

    /// Per-chat local message TTL (`/_ttl {userId} {ref} {seconds|default}`).
    /// `None` restores the user default (not “never”).
    pub fn set_chat_ttl(user_id: i64, chat: &ChatId, seconds: Option<i64>) -> String {
        match seconds {
            Some(secs) => format!("/_ttl {user_id} {} {secs}", chat.command_ref()),
            None => format!("/_ttl {user_id} {} default", chat.command_ref()),
        }
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
        let payload = serde_json::json!({ "emoji": null, "text": text });
        format!("/_create tag {payload}")
    }

    /// Assign lists/tags to a chat (`/_tags {ref} id,id` or bare ref to clear).
    pub fn set_chat_tags(chat: &ChatId, tag_ids: &[i64]) -> String {
        if tag_ids.is_empty() {
            format!("/_tags {}", chat.command_ref())
        } else {
            let ids = tag_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("/_tags {} {ids}", chat.command_ref())
        }
    }

    /// Send text to a chat.
    ///
    /// Direct/group use `/_send`. Local notes ("Saved messages") must use
    /// `/_create *{noteFolderId}` — `/_send *…` is not accepted by the core
    /// parser and returns `Failed reading: empty`.
    pub fn send_text(chat: &ChatId, text: &str) -> String {
        Self::send_text_quoted(chat, text, None)
    }

    /// Send text, optionally quoting another item (`quotedItemId`).
    pub fn send_text_quoted(chat: &ChatId, text: &str, quoted_item_id: Option<i64>) -> String {
        let msgs = serde_json::json!([{
            "fileSource": null,
            "quotedItemId": quoted_item_id,
            "msgContent": { "type": "text", "text": text },
            "mentions": {}
        }]);
        match chat.chat_type {
            ChatType::Local => format!("/_create *{} json {msgs}", chat.api_id),
            _ => format!(
                "/_send {} live=off ttl=default sign=off json {msgs}",
                chat.command_ref()
            ),
        }
    }

    /// Update an existing chat item's text (`/_update item …`).
    ///
    /// Only works while SimpleX marks the item `editable` (outgoing, not
    /// forwarded, typically within 24h). Core returns `invalidChatItemUpdate`
    /// once that window closes.
    pub fn update_text(chat: &ChatId, item_id: i64, text: &str) -> String {
        let payload = serde_json::json!({
            "msgContent": { "type": "text", "text": text },
            "mentions": {}
        });
        format!(
            "/_update item {} {item_id} live=off json {payload}",
            chat.command_ref()
        )
    }

    /// Send a file/image attachment.
    ///
    /// Uses composed-message `fileSource` (CryptoFile) when key/nonce are set —
    /// same shape as official clients with `privacyEncryptLocalFiles`.
    /// Images get a tiny placeholder thumbnail so the content type is `image`.
    pub fn send_file(
        chat: &ChatId,
        path: &str,
        file_name: &str,
        as_image: bool,
        crypto: Option<(&str, &str)>,
    ) -> String {
        // Minimal valid PNG used by SimpleX's own tests as an image preview.
        const TINY_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAIAQMAAAD+wSzIAAAABlBMVEX///+/v7+jQ3Y5AAAADklEQVQI12P4AIX8EAgALgAD/aNpbtEAAAAASUVORK5CYII=";
        let msg_content = if as_image {
            serde_json::json!({
                "type": "image",
                "text": "",
                "image": TINY_PNG,
            })
        } else {
            serde_json::json!({
                "type": "file",
                "text": file_name,
            })
        };
        Self::send_composed(chat, path, crypto, msg_content)
    }

    /// Send a voice message (`msgContent.type = voice`, duration in seconds).
    pub fn send_voice(
        chat: &ChatId,
        path: &str,
        duration_secs: u32,
        crypto: Option<(&str, &str)>,
    ) -> String {
        let msg_content = serde_json::json!({
            "type": "voice",
            "text": "",
            "duration": duration_secs,
        });
        Self::send_composed(chat, path, crypto, msg_content)
    }

    fn send_composed(
        chat: &ChatId,
        path: &str,
        crypto: Option<(&str, &str)>,
        msg_content: serde_json::Value,
    ) -> String {
        let file_source = match crypto {
            Some((key, nonce)) => serde_json::json!({
                "filePath": path,
                "cryptoArgs": { "fileKey": key, "fileNonce": nonce }
            }),
            None => serde_json::json!({
                "filePath": path,
                "cryptoArgs": null
            }),
        };
        let msgs = serde_json::json!([{
            "fileSource": file_source,
            "quotedItemId": null,
            "msgContent": msg_content,
            "mentions": {}
        }]);
        match chat.chat_type {
            ChatType::Local => format!("/_create *{} json {msgs}", chat.api_id),
            _ => format!(
                "/_send {} live=off ttl=default sign=off json {msgs}",
                chat.command_ref()
            ),
        }
    }

    /// Accept an incoming file (`/freceive <fileId> …`).
    ///
    /// Path is omitted so the core uses [`set_files_folder`]; that matches
    /// official clients and keeps relative paths inside the app files dir.
    ///
    /// `encrypt=on` stores CryptoFile ciphertext under `files/` (keys in DB).
    /// `approved_relays=off` makes the core refuse file servers that are not
    /// ours (`fileNotApproved`), so the sender can't learn our IP address
    /// without consent — the official "Protect IP address" default.
    /// `/_set receipts contacts|groups <userId> on|off` (keeps per-chat overrides).
    pub fn set_delivery_receipts(user_id: i64, groups: bool, on: bool) -> String {
        let scope = if groups { "groups" } else { "contacts" };
        let flag = if on { "on" } else { "off" };
        format!("/_set receipts {scope} {user_id} {flag}")
    }

    pub fn receive_file(file_id: i64, approved_relays: bool) -> String {
        let relays = if approved_relays { "on" } else { "off" };
        format!("/freceive {file_id} approved_relays={relays} encrypt=on")
    }

    /// Toggle core `encryptLocalFiles` (`/_files_encrypt on|off`).
    pub fn set_encrypt_local_files(on: bool) -> String {
        format!("/_files_encrypt {}", if on { "on" } else { "off" })
    }

    /// Add or remove an emoji reaction (`/_reaction {ref} {itemId} on|off {json}`).
    ///
    /// `emoji` must be one of the protocol-allowed chars (see `KNOWN_REACTIONS`).
    pub fn reaction(chat: &ChatId, item_id: i64, emoji: &str, add: bool) -> String {
        let flag = if add { "on" } else { "off" };
        let payload = serde_json::json!({ "type": "emoji", "emoji": emoji });
        format!(
            "/_reaction {} {item_id} {flag} {payload}",
            chat.command_ref()
        )
    }

    /// List group members who set a reaction (`/_reaction members {userId} #{groupId} {itemId} {json}`).
    pub fn reaction_members(user_id: i64, group_id: i64, item_id: i64, emoji: &str) -> String {
        let payload = serde_json::json!({ "type": "emoji", "emoji": emoji });
        format!("/_reaction members {user_id} #{group_id} {item_id} {payload}")
    }

    /// Delete chat items (`/_delete item {ref} {ids} broadcast|internal`).
    ///
    /// Use `broadcast` to mark-delete for the peer (when Full Delete is off the
    /// item stays with `itemDeleted`). Use `internal` to remove locally — required
    /// for items that are already marked deleted (`deletable` is false).
    pub fn delete_items(chat: &ChatId, item_ids: &[i64], broadcast: bool) -> String {
        let ids = item_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mode = if broadcast && !matches!(chat.chat_type, ChatType::Local) {
            "broadcast"
        } else {
            "internal"
        };
        format!("/_delete item {} {ids} {mode}", chat.command_ref())
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
        let ids = member_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("/_member role #{group_id} {ids} {role}")
    }

    /// Remove members (`/_remove #{groupId} {ids} messages=on|off`).
    pub fn remove_members(group_id: i64, member_ids: &[i64], with_messages: bool) -> String {
        let ids = member_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let flag = if with_messages { "on" } else { "off" };
        format!("/_remove #{group_id} {ids} messages={flag}")
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
        let payload = serde_json::json!({ "showMessages": show_messages });
        format!("/_member settings #{group_id} {member_id} {payload}")
    }

    /// Block / unblock members for everyone (`/_block #{gid} {ids} blocked=on|off`).
    pub fn block_members(group_id: i64, member_ids: &[i64], blocked: bool) -> String {
        let ids = member_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let flag = if blocked { "on" } else { "off" };
        format!("/_block #{group_id} {ids} blocked={flag}")
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
        if alias.is_empty() {
            format!("/_set alias #{group_id}")
        } else {
            format!("/_set alias #{group_id} {alias}")
        }
    }

    /// Update group profile (`/_group_profile #{groupId} {json}`) — full profile round-trip.
    pub fn update_group_profile(group_id: i64, profile: &crate::models::GroupProfileUpdate) -> String {
        let payload = serde_json::json!({
            "displayName": profile.display_name,
            "fullName": profile.full_name,
            "shortDescr": if profile.short_descr.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(profile.short_descr.clone())
            },
            "description": if profile.welcome_message.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(profile.welcome_message.clone())
            },
            "image": profile
                .image
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::Value::String(s.clone()))
                .unwrap_or(serde_json::Value::Null),
            "groupPreferences": profile
                .group_preferences
                .clone()
                .unwrap_or(serde_json::Value::Null),
            "memberAdmission": profile
                .member_admission
                .clone()
                .unwrap_or(serde_json::Value::Null),
            "publicGroup": profile
                .public_group
                .clone()
                .unwrap_or(serde_json::Value::Null),
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
        let profile = serde_json::json!({
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
        let relays = relay_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let profile = serde_json::json!({
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
    pub fn call_invite(contact_id: i64, media: crate::models::CallMedia) -> String {
        let payload = serde_json::json!({
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
        match before {
            Some(id) => format!("/_get items before={id} count={count}"),
            None => format!("/_get items count={count}"),
        }
    }

    pub fn call_get() -> &'static str {
        "/_call get"
    }

    /// Callee → caller: WebRTC offer (`/_call offer @{id} {WebRTCCallOffer}`).
    /// `rtc_session` / `rtc_ice_candidates` are LZString-base64 JSON, as the
    /// official clients send them.
    pub fn call_offer(
        contact_id: i64,
        media: crate::models::CallMedia,
        encryption: bool,
        rtc_session: &str,
        rtc_ice_candidates: &str,
    ) -> String {
        let payload = serde_json::json!({
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
        let payload = serde_json::json!({
            "rtcSession": rtc_session,
            "rtcIceCandidates": rtc_ice_candidates
        });
        format!("/_call answer @{contact_id} {payload}")
    }

    /// Late ICE candidates (`/_call extra @{id} {WebRTCExtraInfo}`).
    pub fn call_extra(contact_id: i64, rtc_ice_candidates: &str) -> String {
        let payload = serde_json::json!({ "rtcIceCandidates": rtc_ice_candidates });
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
        let ids = item_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "/_forward {} as_group=off {} {ids} ttl=default",
            to.command_ref(),
            from.command_ref()
        )
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}
