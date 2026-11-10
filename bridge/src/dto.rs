//! JSON shapes the Swift app decodes (`Models.swift` mirrors these). Kept
//! here, not as serde derives on the core models, so the core can change its
//! internals without breaking the app.

use cursus_core::{
    parse_chat_info_id, ChatId, ChatMessage, ChatPreview, ChatTag, ChatType, ConnLink, DeletedState,
    DeliveryStatus, Event, FileInfo, FileKind, FileTransferState, FormatColor, GroupChatScope,
    GroupMemberInfo, SimplexLinkKind, TextSegment, TextStyle, UserProfile,
};
use cursus_core::preferences;
use cursus_core::roles;
use serde_json::{json, Value};

/// Parse a chat reference (`@3`, `#9`, `#9(_support)`, `#9(_support:4)`, `*1`).
pub fn parse_chat_ref(raw: &str) -> Result<ChatId, String> {
    let bad = || format!("bad chat reference: {raw}");
    let mut chars = raw.chars();
    let chat_type = match chars.next() {
        Some('@') => ChatType::Direct,
        Some('#') => ChatType::Group,
        Some('*') => ChatType::Local,
        _ => return Err(bad()),
    };
    let rest = chars.as_str();
    let (id, scope) = match rest.split_once('(') {
        None => (rest, None),
        Some((id, scope)) => {
            let inner = scope.strip_suffix(')').ok_or_else(bad)?;
            let member = match inner.strip_prefix("_support") {
                Some("") => None,
                Some(m) => Some(m.strip_prefix(':').ok_or_else(bad)?.parse::<i64>().map_err(|_| bad())?),
                None => return Err(bad()),
            };
            (id, Some(GroupChatScope::MemberSupport { group_member_id: member }))
        }
    };
    let api_id = id.parse::<i64>().map_err(|_| bad())?;
    if scope.is_some() && chat_type != ChatType::Group {
        return Err(bad());
    }
    Ok(ChatId { chat_type, api_id, scope })
}

fn chat_type_name(t: ChatType) -> &'static str {
    match t {
        ChatType::Direct => "direct",
        ChatType::Group => "group",
        ChatType::Local => "local",
        ChatType::Other => "other",
    }
}

pub fn user(u: &UserProfile) -> Value {
    let p = &u.profile;
    json!({
        "userId": u.user_id,
        "displayName": p.display_name,
        "fullName": p.full_name,
        "shortDescr": p.short_descr,
        "description": p.description,
        "image": p.image,
        "contactLink": p.contact_link,
        "sendRcptsContacts": u.send_rcpts_contacts,
        "sendRcptsSmallGroups": u.send_rcpts_small_groups,
    })
}

pub fn link(l: &ConnLink) -> Value {
    json!({ "display": l.display, "full": l.full, "short": l.short })
}

pub fn invitation(i: &cursus_core::Invitation) -> Value {
    json!({
        "link": link(&i.link),
        "connId": i.conn_id,
        "incognito": i.incognito,
        "alias": i.alias,
        "createdAt": i.created_at,
    })
}

pub fn chat(c: &ChatPreview) -> Value {
    let title = if c.local_alias.trim().is_empty() { c.title.clone() } else { c.local_alias.clone() };
    json!({
        "ref": c.id.command_ref(),
        "type": chat_type_name(c.id.chat_type),
        "apiId": c.id.api_id,
        "title": title,
        // Readable SimpleX link labels, as in the Linux list.
        "preview": cursus_core::humanize_simplex_links(&c.preview),
        "unread": c.unread,
        "unreadChat": c.unread_chat,
        "notifications": c.settings.enable_ntfs.as_api(),
        "favorite": c.settings.favorite,
        "pendingRequestId": c.pending_request_id,
        "image": c.peer_profile.image,
        // Public name (ignoring the local alias), for the info panel.
        "name": if c.peer_profile.display_name.trim().is_empty() { c.title.clone() } else { c.peer_profile.display_name.clone() },
        "alias": c.local_alias,
        "fullName": c.peer_profile.full_name,
        "shortDescr": c.peer_profile.short_descr,
        "description": c.peer_profile.description,
        "contactLink": c.peer_profile.contact_link,
        "isChannel": c.is_channel,
        "role": c.membership_role,
        "lastTs": c.last_ts,
        // Settings shown in the info panel.
        "sendReceipts": c.settings.send_rcpts,
        "chatItemTtl": c.chat_item_ttl,
        "tagIds": c.tag_ids,
        "contactPrefs": (c.id.chat_type == ChatType::Direct).then(|| contact_prefs(c)),
        "groupPrefs": (c.id.chat_type == ChatType::Group).then(|| group_prefs(c)),
        "memberReview": c.member_admission.as_ref().and_then(|m| m.get("review")).is_some(),
        "membersRequireAttention": c.members_require_attention,
        // What your role allows here (cursus_core::roles), for groups.
        "canEditProfile": c.membership_role.as_deref().is_some_and(roles::can_edit_group_profile),
        "canManageMembers": c.membership_role.as_deref().is_some_and(roles::can_manage_members),
        "canModerate": c.membership_role.as_deref().is_some_and(roles::can_moderate),
        "assignableRoles": roles::assignable_roles(c.membership_role.as_deref().unwrap_or("")),
        // Your own "Chat with admins" thread.
        "supportUnread": c.membership_support_unread,
        "hasSupportChat": c.membership_has_support_chat,
    })
}

/// `{ features: { key: { mine, contact, enabled } }, timedTtl }` — the rules
/// are `cursus_core::preferences`; this is only their JSON for the app.
fn contact_prefs(c: &ChatPreview) -> Value {
    let (prefs, merged) = (c.contact_preferences.as_ref(), c.contact_merged_preferences.as_ref());
    let features: serde_json::Map<String, Value> = preferences::CONTACT_FEATURES
        .iter()
        .map(|key| {
            let f = preferences::contact_feature(prefs, merged, key);
            let v = json!({ "mine": f.mine.as_api(), "contact": f.contact.as_api(), "enabled": f.enabled_for_user });
            (key.to_string(), v)
        })
        .collect();
    let ttl = preferences::contact_timed_ttl(prefs, merged).unwrap_or(preferences::DEFAULT_TIMED_TTL_SECS);
    json!({ "features": features, "timedTtl": ttl })
}

/// `{ features: { key: { on } }, timedTtl }`.
fn group_prefs(c: &ChatPreview) -> Value {
    let prefs = c.group_preferences.as_ref();
    let features: serde_json::Map<String, Value> = preferences::GROUP_FEATURES
        .iter()
        .map(|key| (key.to_string(), json!({ "on": preferences::group_feature_on(prefs, key) })))
        .collect();
    let ttl = preferences::group_timed_ttl(prefs).unwrap_or(preferences::DEFAULT_TIMED_TTL_SECS);
    json!({ "features": features, "timedTtl": ttl })
}

pub fn tag(t: &ChatTag) -> Value {
    json!({ "id": t.id, "text": t.text, "emoji": t.emoji })
}

pub fn member(m: &GroupMemberInfo) -> Value {
    json!({
        "id": m.group_member_id,
        "contactId": m.contact_id,
        "name": m.display_name,
        "publicName": m.public_name,
        "role": m.role,
        "status": m.status,
        "image": m.image,
        "shortDescr": m.short_descr,
        "isMe": m.is_me,
        "blocked": m.blocked_by_admin,
        "showMessages": m.show_messages,
        "invited": roles::member_status_is_invited(&m.status),
        "pending": roles::member_status_is_pending(&m.status),
        // Their support thread with the moderators, when one exists.
        "support": m.support_chat.as_ref().map(|c| json!({
            "unread": c.unread, "attention": c.member_attention, "mentions": c.mentions,
        })),
    })
}

fn color_name(c: FormatColor) -> &'static str {
    match c {
        FormatColor::Red => "red",
        FormatColor::Green => "green",
        FormatColor::Blue => "blue",
        FormatColor::Yellow => "yellow",
        FormatColor::Cyan => "cyan",
        FormatColor::Magenta => "magenta",
        FormatColor::Black => "black",
        FormatColor::White => "white",
    }
}

pub fn simplex_kind(k: SimplexLinkKind) -> &'static str {
    match k {
        SimplexLinkKind::Invitation => "invitation",
        SimplexLinkKind::Address => "address",
        SimplexLinkKind::Group => "group",
        SimplexLinkKind::Channel => "channel",
        SimplexLinkKind::Relay => "relay",
    }
}

fn segment(s: &TextSegment) -> Value {
    let (style, color) = match s.style {
        TextStyle::Plain => ("plain", None),
        TextStyle::Bold => ("bold", None),
        TextStyle::Italic => ("italic", None),
        TextStyle::Strike => ("strike", None),
        TextStyle::Snippet => ("snippet", None),
        TextStyle::Secret => ("secret", None),
        TextStyle::Small => ("small", None),
        TextStyle::Colored { color } => ("colored", Some(color_name(color))),
        TextStyle::Uri => ("uri", None),
        TextStyle::Email => ("email", None),
        TextStyle::Phone => ("phone", None),
        TextStyle::Mention => ("mention", None),
        TextStyle::Command => ("command", None),
    };
    let simplex = s.simplex.map(simplex_kind);
    json!({ "text": s.text, "style": style, "color": color, "link": s.link, "simplex": simplex })
}

fn file(f: &FileInfo) -> Value {
    let state = match f.state {
        FileTransferState::Offer => "offer",
        FileTransferState::InProgress => "inProgress",
        FileTransferState::Complete => "complete",
        FileTransferState::Failed => "failed",
        FileTransferState::Other => "other",
    };
    let kind = match f.kind {
        FileKind::Image => "image",
        FileKind::Video => "video",
        FileKind::Voice => "voice",
        FileKind::File => "file",
    };
    json!({
        "id": f.file_id,
        "name": f.file_name,
        "size": f.file_size,
        "state": state,
        "kind": kind,
        "duration": f.duration_secs,
        "path": f.local_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "key": f.crypto_key,
        "nonce": f.crypto_nonce,
        "progress": f.progress_fraction(),
        "error": f.error_detail,
        // The official client downloads this by itself (core rule).
        "autoReceive": f.should_auto_receive(),
    })
}

pub fn message(m: &ChatMessage) -> Value {
    let delivery = match m.delivery {
        DeliveryStatus::Pending => "pending",
        DeliveryStatus::Sent => "sent",
        DeliveryStatus::Delivered => "delivered",
        DeliveryStatus::Error => "error",
        DeliveryStatus::Warning => "warning",
    };
    let deleted = m.deleted.map(|d| match d {
        DeletedState::Marked => "marked",
        DeletedState::Content => "deleted",
    });
    json!({
        "id": m.item_id,
        "text": m.text,
        // With a file: only what the sender typed (`text` is a summary).
        "caption": m.caption,
        "segments": m.segments.iter().map(segment).collect::<Vec<_>>(),
        "outgoing": m.outgoing,
        "createdAt": m.created_at,
        "delivery": delivery,
        "file": m.file.as_ref().map(file),
        "reactions": m.reactions.iter().map(|r| json!({
            "emoji": r.emoji, "glyph": r.display_glyph(), "mine": r.user_reacted, "total": r.total,
        })).collect::<Vec<_>>(),
        "quote": m.quote.as_ref().map(|q| json!({ "itemId": q.item_id, "text": q.text, "outgoing": q.outgoing })),
        "deleted": deleted,
        "sender": m.sender.as_ref().map(|s| json!({
            "memberId": s.group_member_id, "contactId": s.contact_id,
            "name": s.display_name, "image": s.image, "role": s.role,
        })),
        "announcement": m.is_announcement,
        "groupInvitation": m.group_invitation.as_ref().map(|g| json!({
            "groupId": g.group_id, "name": g.group_name, "role": g.member_role,
            "status": g.status, "pending": m.group_invite_pending(),
        })),
        "edited": m.edited,
        "canEdit": m.can_edit_text(),
        "emojiOnly": m.is_emoji_only(),
    })
}

fn event_name(e: &Event) -> String {
    match e {
        Event::Raw(_) => "raw".into(),
        Event::NewChatItems(_) => "newChatItems".into(),
        Event::ChatItemsStatusesUpdated(_) => "chatItemsStatusesUpdated".into(),
        Event::ChatStarted => "chatStarted".into(),
        Event::ContactConnected(_) => "contactConnected".into(),
        Event::ReceivedContactRequest(_) => "receivedContactRequest".into(),
        Event::Other { type_name, .. } => type_name.clone(),
    }
}

fn chat_title_from_info(info: &Value) -> Option<String> {
    [
        "/contact/localAlias",
        "/contact/profile/displayName",
        "/contact/localDisplayName",
        "/groupInfo/localAlias",
        "/groupInfo/groupProfile/displayName",
        "/groupInfo/localDisplayName",
    ]
    .iter()
    .filter_map(|p| info.pointer(p).and_then(Value::as_str))
    .find(|s| !s.trim().is_empty())
    .map(str::to_string)
}

/// Incoming messages in a `newChatItems` event, for notifications.
fn incoming(e: &Event) -> Vec<Value> {
    let Event::NewChatItems(value) = e else { return Vec::new() };
    let Some(items) = value.get("chatItems").and_then(Value::as_array) else { return Vec::new() };
    items
        .iter()
        .filter_map(|aci| {
            let info = aci.get("chatInfo")?;
            let chat = parse_chat_info_id(info)?;
            let item = aci.get("chatItem").unwrap_or(aci);
            let dir = item.pointer("/chatDir/type").and_then(Value::as_str).unwrap_or("");
            if dir.contains("Snd") || dir == "local" {
                return None;
            }
            let text = item
                .pointer("/meta/itemText")
                .and_then(Value::as_str)
                .or_else(|| item.pointer("/content/msgContent/text").and_then(Value::as_str))
                .unwrap_or("");
            let sender = item.pointer("/chatDir/groupMember/memberProfile/displayName").and_then(Value::as_str);
            Some(json!({
                "chat": chat.command_ref(),
                "title": chat_title_from_info(info).unwrap_or_else(|| "Cursus".into()),
                "sender": sender,
                "text": text,
            }))
        })
        .collect()
}

pub fn event(e: &Event) -> Value {
    let progress = e.file_progress_updates().map(|(id, done, total)| {
        json!({ "fileId": id, "transferred": done, "total": total })
    });
    json!({
        "type": event_name(e),
        "refreshChats": e.refreshes_chats(),
        "refreshMessages": e.refreshes_messages(),
        "chat": e.chat_hint().map(|c| c.command_ref()),
        "progress": progress,
        "incoming": incoming(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_refs_round_trip() {
        for raw in ["@3", "#9", "*1", "#9(_support)", "#9(_support:4)"] {
            assert_eq!(parse_chat_ref(raw).unwrap().command_ref(), raw);
        }
        for bad in ["", "3", "@x", "@3(_support)", "#9(_support:)", "#9(x)", "#9(_support"] {
            assert!(parse_chat_ref(bad).is_err(), "{bad}");
        }
    }
}
