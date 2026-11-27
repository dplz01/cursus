//! libsimplex JSON → Cursus models: the user, the chat list, messages,
//! files, group members and connection links.

use serde_json::Value;

use crate::error::{Error, Result};
use crate::json::JsonExt;
use crate::models::{
    ChatId, ChatMessage, ChatPreview, ChatRelayInfo, ChatSettings, ChatTag, ChatType, ConnLink,
    DeletedState, DeliveryStatus, Event, FileInfo, FileKind, FileTransferState, GroupInvitationInfo,
    GroupMemberInfo, GroupSupportChat, MessageQuote, MessageSender, NtfFilter, PreviewKind, ProfileFields,
    ReactionCount, ReactionMember, UserProfile, KNOWN_REACTIONS,
};
use crate::paths::{files_dir, resolve_local_file};
use crate::rich_text::{expand_emoji_shortcodes, parse_formatted_text, TextStyle, TextSegment};

// ── User and profiles ───────────────────────────────────────────────────

pub(crate) fn user(value: &Value) -> Result<UserProfile> {
    let user = value
        .first(&["/user", "/fromUser", "/toUser"])
        .ok_or_else(|| Error::Command("reply carries no user".into()))?;
    let flag = |path| user.bool_at(path).unwrap_or(true);
    Ok(UserProfile {
        user_id: user.i64_at("/userId").ok_or_else(|| Error::Command("user.userId missing".into()))?,
        profile: profile_fields(user.get("profile")),
        send_rcpts_contacts: flag("/sendRcptsContacts"),
        send_rcpts_small_groups: flag("/sendRcptsSmallGroups"),
    })
}

/// A profile picture: only inline `data:image/…` URLs are ever shown.
fn image_data_url(owner: &Value, path: &str) -> Option<String> {
    owner.nonempty(path).filter(|s| s.starts_with("data:image/"))
}

pub(crate) fn profile_fields(value: Option<&Value>) -> ProfileFields {
    let Some(p) = value else {
        return ProfileFields { display_name: "Cursus".into(), ..ProfileFields::default() };
    };
    ProfileFields {
        display_name: p.str_at("/displayName").unwrap_or("Cursus").to_string(),
        full_name: p.text("/fullName"),
        short_descr: p.text("/shortDescr"),
        description: p.text("/description"),
        image: image_data_url(p, "/image"),
        contact_link: p.nonempty("/contactLink"),
        preferences: p.get("preferences").cloned(),
    }
}

// ── Chat list ───────────────────────────────────────────────────────────

/// Request id that still needs Accept / Reject. Mirrors the official
/// `nextAcceptContactRequest`: `contactRequestId` stays on the contact after
/// `/_accept` until the request row is deleted, so only a contact with no
/// usable connection yet counts.
fn pending_accept_request_id(contact: &Value) -> Option<i64> {
    let req_id = contact
        .first(&["/contactRequestId", "/contactRequest/contactRequestId"])
        .and_then(Value::as_i64)?;
    // ConnStatus is a sum type (`{"type":"sndReady"}`); older cores sent a string.
    let status = contact.pointer("/activeConn/connStatus");
    let status = status.and_then(|s| s.as_str().or_else(|| s.str_at("/type")));
    matches!(status, None | Some("new" | "prepared")).then_some(req_id)
}

fn chat_preview(chat: &Value) -> Option<ChatPreview> {
    let info = chat.get("chatInfo")?;
    let alias_or = |alias: &str, name: String| if alias.is_empty() { name } else { alias.to_string() };
    let mut c = match info.tag() {
        "direct" => {
            let contact = info.get("contact")?;
            let alias = contact.first_nonempty(&["/localAlias", "/profile/localAlias"]).unwrap_or_default();
            let name = contact
                .first_nonempty(&["/profile/displayName", "/localDisplayName"])
                .unwrap_or_else(|| "Contact".into());
            let mut c = ChatPreview::new(
                ChatId::new(ChatType::Direct, contact.i64_at("/contactId")?),
                alias_or(&alias, name),
            );
            c.peer_profile = profile_fields(contact.get("profile"));
            c.local_alias = alias;
            c.pending_request_id = pending_accept_request_id(contact);
            let status = contact.pointer("/activeConn/connStatus");
            let status = status.and_then(|s| s.as_str().or_else(|| s.str_at("/type")));
            if contact.get("groupDirectInv").is_some_and(Value::is_object)
                && matches!(status, None | Some("new" | "prepared"))
                && contact.str_at("/contactStatus").unwrap_or("active") == "active"
            {
                c.pending_request_id = contact.i64_at("/contactId");
                c.member_request = true;
            }
            c.settings = chat_settings(contact.get("chatSettings"));
            c.tag_ids = tag_ids(contact);
            c.contact_preferences = contact.get("userPreferences").cloned();
            c.contact_merged_preferences = contact.get("mergedPreferences").cloned();
            c
        }
        // Standalone request row (some core versions).
        "contactRequest" => {
            let req = info.get("contactRequest")?;
            let req_id = req.i64_at("/contactRequestId")?;
            let name = req
                .first_nonempty(&["/localDisplayName", "/profile/displayName"])
                .unwrap_or_else(|| "Request".into());
            let mut c = ChatPreview::new(ChatId::new(ChatType::Direct, req_id), name);
            c.peer_profile = profile_fields(req.get("profile"));
            c.pending_request_id = Some(req_id);
            c
        }
        "group" => {
            let group = info.get("groupInfo")?;
            let profile = group.first(&["/groupProfile", "/profile"]);
            let field = |key: &str| profile.and_then(|p| p.get(key)).cloned();
            let alias = group.nonempty("/localAlias").unwrap_or_default();
            let name = group
                .first_nonempty(&["/groupProfile/displayName", "/localDisplayName"])
                .unwrap_or_else(|| "Group".into());
            let mut c =
                ChatPreview::new(ChatId::new(ChatType::Group, group.i64_at("/groupId")?), alias_or(&alias, name));
            c.peer_profile = profile_fields(profile);
            c.local_alias = alias;
            c.membership_role = group.str_at("/membership/memberRole").map(str::to_string);
            c.group_preferences = field("groupPreferences");
            c.member_admission = field("memberAdmission");
            c.public_group = field("publicGroup");
            c.settings = chat_settings(group.get("chatSettings"));
            c.tag_ids = tag_ids(group);
            c.is_channel = group.bool_at("/useRelays").unwrap_or(false)
                || group.str_at("/groupProfile/publicGroup/groupType") == Some("channel");
            c.members_require_attention = group.i64_at("/membersRequireAttention").unwrap_or(0);
            c.membership_has_support_chat = group.pointer("/membership/supportChat").is_some();
            c.membership_support_unread = group.i64_at("/membership/supportChat/unread").unwrap_or(0);
            c
        }
        "local" => {
            let mut c = ChatPreview::new(
                ChatId::new(ChatType::Local, info.i64_at("/noteFolder/noteFolderId")?),
                "Notes".into(),
            );
            c.peer_profile.display_name = "Notes".into();
            c
        }
        _ => return None,
    };

    let last_item = chat.array_at("/chatItems").first();
    if let Some((text, kind, segments)) = last_item.and_then(item_preview) {
        c.preview = text;
        c.preview_kind = kind;
        c.preview_segments = segments;
    }
    if c.pending_request_id.is_some() && c.preview.is_empty() {
        c.preview = if c.member_request { "Direct message request from a group member — accept or decline." }
            else { "Incoming contact request — accept to chat." }.into();
    }
    c.unread = chat.i64_at("/chatStats/unreadCount").unwrap_or(0);
    c.unread_chat = chat.bool_at("/chatStats/unreadChat").unwrap_or(false);
    c.last_ts = last_item
        .and_then(|item| item.str_at("/meta/itemTs"))
        .or_else(|| {
            ["/contact/chatTs", "/groupInfo/chatTs", "/contact/updatedAt", "/groupInfo/updatedAt", "/noteFolder/chatTs"]
                .iter()
                .find_map(|p| info.str_at(p))
        })
        .map(str::to_string);
    c.chat_item_ttl = info
        .first(&["/contact/chatItemTTL", "/groupInfo/chatItemTTL", "/chatItemTTL"])
        .and_then(Value::as_i64);
    Some(c)
}

/// List-row preview of a chat item, or `None` when it is no message at all
/// (encryption banners, feature changes, members joining…), which the list
/// skips for the last real message. Deleted items never show their old text
/// (it may be an invite link): a short tombstone instead, as the official
/// apps. Files show what they are; the apps lead with a matching icon.
pub(crate) fn item_preview(item: &Value) -> Option<Preview> {
    if let Some(state) = deleted_state(item) {
        return Some((state.label().into(), PreviewKind::Text, Vec::new()));
    }
    if let Some(call) = call_label(item) {
        return Some((call, PreviewKind::Call, Vec::new()));
    }
    if is_announcement(item) {
        return None;
    }
    let content = item.first(&["/content/msgContent", "/msgContent"]);
    let caption = || {
        let typed = content.and_then(|c| c.str_at("/text")).unwrap_or_default().trim();
        (!typed.is_empty()).then(|| expand_emoji_shortcodes(typed))
    };
    let kind = content.and_then(|c| c.str_at("/type")).map(FileKind::from_msg_content_type).or_else(|| {
        // A file with no message type: going by its name.
        item.str_at("/file/fileName").map(kind_from_name)
    });
    let is_file = item.get("file").is_some_and(|f| !f.is_null());
    let preview = match kind {
        Some(FileKind::Image) => (caption().unwrap_or_else(|| "Image".into()), PreviewKind::Image),
        Some(FileKind::Video) => (caption().unwrap_or_else(|| "Video".into()), PreviewKind::Video),
        Some(FileKind::Voice) => {
            let secs = content.and_then(|c| c.i64_at("/duration")).unwrap_or(0).max(0);
            let label = if secs > 0 {
                format!("Voice message · {}:{:02}", secs / 60, secs % 60)
            } else {
                "Voice message".into()
            };
            (label, PreviewKind::Voice)
        }
        _ if is_file => {
            let name = item.str_at("/file/fileName").unwrap_or("File");
            (caption().unwrap_or_else(|| name.to_string()), PreviewKind::File)
        }
        _ => {
            let text = item_text(item);
            let text = text.strip_prefix('📎').map(str::trim).unwrap_or(text);
            let text = expand_emoji_shortcodes(text);
            // Bold, italic, links… as in the bubble.
            let segments = item_segments(item, false, &text);
            return Some((text, PreviewKind::Text, segments));
        }
    };
    Some((preview.0, preview.1, Vec::new()))
}

/// A chat list preview: its text, what kind of message it is, and the
/// text's SimpleX formatting (empty: plain).
pub(crate) type Preview = (String, PreviewKind, Vec<TextSegment>);

/// The newest real message among `items` (oldest first), as [`item_preview`].
pub(crate) fn latest_preview(items: &[Value]) -> Option<Preview> {
    items.iter().rev().find_map(item_preview)
}

/// The chat list, and for each chat whose newest item is no message (see
/// [`item_preview`]) that item's id: the caller looks further back.
pub(crate) fn chat_previews_with_gaps(value: &Value) -> Vec<(ChatPreview, Option<i64>)> {
    value
        .array_at("/chats")
        .iter()
        .filter_map(|chat| {
            let preview = chat_preview(chat)?;
            let last = chat.array_at("/chatItems").first();
            let gap = last.filter(|item| item_preview(item).is_none()).and_then(|item| item.i64_at("/meta/itemId"));
            Some((preview, gap))
        })
        .collect()
}

fn chat_settings(value: Option<&Value>) -> ChatSettings {
    let Some(v) = value else { return ChatSettings::default() };
    ChatSettings {
        enable_ntfs: v.str_at("/enableNtfs").map(NtfFilter::parse).unwrap_or(NtfFilter::All),
        favorite: v.bool_at("/favorite").unwrap_or(false),
        send_rcpts: v.bool_at("/sendRcpts"),
    }
}

fn tag_ids(owner: &Value) -> Vec<i64> {
    owner.array_at("/chatTags").iter().filter_map(Value::as_i64).collect()
}

pub(crate) fn chat_tags(value: &Value) -> Vec<ChatTag> {
    let tags = value.first(&["/userTags", "/chatTags"]).and_then(Value::as_array);
    tags.into_iter()
        .flatten()
        .filter_map(|tag| {
            Some(ChatTag {
                id: tag.i64_at("/chatTagId")?,
                text: tag.str_at("/chatTagText").unwrap_or("List").to_string(),
                emoji: tag.str_at("/chatTagEmoji").map(str::to_string),
            })
        })
        .collect()
}

/// Enabled chat relays of enabled operators in a `/_servers` reply.
pub(crate) fn chat_relays(value: &Value) -> Vec<ChatRelayInfo> {
    value
        .array_at("/userServers")
        .iter()
        .filter(|op| op.bool_at("/operator/enabled") != Some(false))
        .flat_map(|op| op.array_at("/chatRelays"))
        .filter(|relay| relay.bool_at("/enabled").unwrap_or(true) && !relay.bool_at("/deleted").unwrap_or(false))
        .filter_map(|relay| {
            Some(ChatRelayInfo {
                chat_relay_id: relay.i64_at("/chatRelayId")?,
                display_name: relay
                    .first_nonempty(&["/displayName", "/relayProfile/displayName"])
                    .unwrap_or_else(|| "Relay".into()),
            })
        })
        .collect()
}

// ── Group members ───────────────────────────────────────────────────────

/// A member's name as the official `chatViewName`: your alias for them,
/// else their profile name (never the unique `localDisplayName` first).
fn member_name(member: &Value) -> Option<String> {
    member.first_nonempty(&[
        "/memberProfile/localAlias",
        "/profile/localAlias",
        "/localAlias",
        "/memberProfile/displayName",
        "/profile/displayName",
        "/localDisplayName",
    ])
}

fn group_member(m: &Value) -> Option<GroupMemberInfo> {
    let role = m.str_at("/memberRole").unwrap_or("member").to_string();
    let status = m.text("/memberStatus");
    // Relay service accounts and people who are gone are not listed
    // (as the official CLI).
    if role.eq_ignore_ascii_case("relay") || matches!(status.as_str(), "rejected" | "removed" | "left") {
        return None;
    }
    let display_name = member_name(m).unwrap_or_else(|| "Member".into());
    Some(GroupMemberInfo {
        group_member_id: m.i64_at("/groupMemberId")?,
        contact_id: m.i64_at("/memberContactId").filter(|id| *id > 0),
        public_name: m
            .first_nonempty(&["/memberProfile/displayName", "/profile/displayName"])
            .unwrap_or_else(|| display_name.clone()),
        display_name,
        role,
        status,
        blocked_by_admin: m
            .first(&["/blockedByAdmin", "/memberBlockedByAdmin"])
            .and_then(Value::as_bool)
            .unwrap_or(false),
        show_messages: m.bool_at("/memberSettings/showMessages").unwrap_or(true),
        support_chat: m.get("supportChat").filter(|sc| !sc.is_null()).map(|sc| GroupSupportChat {
            unread: sc.i64_at("/unread").unwrap_or(0),
            member_attention: sc.i64_at("/memberAttention").unwrap_or(0),
            mentions: sc.i64_at("/mentions").unwrap_or(0),
        }),
        image: image_data_url(m, "/memberProfile/image").or_else(|| image_data_url(m, "/profile/image")),
        short_descr: m.first_nonempty(&["/memberProfile/shortDescr", "/profile/shortDescr"]),
        is_me: false,
    })
}

/// You first (the core keeps your membership apart), then everyone else.
pub(crate) fn group_members(value: &Value) -> Vec<GroupMemberInfo> {
    let me = value
        .first(&["/group/groupInfo/membership", "/groupInfo/membership", "/membership"])
        .and_then(group_member)
        .map(|m| GroupMemberInfo { is_me: true, ..m });
    let others = value.first(&["/group/members", "/members"]).and_then(Value::as_array);
    let mut seen = std::collections::HashSet::new();
    me.into_iter()
        .chain(others.into_iter().flatten().filter_map(group_member))
        .filter(|m| seen.insert(m.group_member_id))
        .collect()
}

pub(crate) fn reaction_members(value: &Value) -> Vec<ReactionMember> {
    let rows = value.first(&["/memberReactions", "/reactionMembers/memberReactions"]).and_then(Value::as_array);
    rows.into_iter()
        .flatten()
        .filter_map(|row| Some(ReactionMember { display_name: member_name(row.get("groupMember")?)? }))
        .collect()
}

// ── Messages ────────────────────────────────────────────────────────────

pub(crate) fn chat_messages(value: &Value) -> Vec<ChatMessage> {
    value.array_at("/chat/chatItems").iter().filter_map(message).collect()
}

fn is_outgoing(item: &Value) -> bool {
    let dir = item.str_at("/chatDir/type").unwrap_or_default();
    dir == "snd" || dir.contains("Snd")
}

/// The core's one-line text of an item (its summary for files and events).
fn item_text(item: &Value) -> &str {
    ["/meta/itemText", "/content/msgContent/text", "/content/text"]
        .iter()
        .find_map(|p| item.str_at(p))
        .unwrap_or_default()
}

fn deleted_state(item: &Value) -> Option<DeletedState> {
    match item.str_at("/meta/itemDeleted/type") {
        Some("blocked") => return Some(DeletedState::Hidden),
        Some("blockedByAdmin") => return Some(DeletedState::BlockedByAdmin),
        _ if item.is_set("/meta/itemDeleted") => return Some(DeletedState::Marked),
        _ => {}
    }
    matches!(item.str_at("/content/type"), Some("sndDeleted" | "rcvDeleted")).then_some(DeletedState::Content)
}

pub(crate) fn message(item: &Value) -> Option<ChatMessage> {
    let item_id = item.i64_at("/meta/itemId")?;
    let deleted = deleted_state(item);
    let outgoing = is_outgoing(item);
    let file = file_info(item);
    let group_invitation = group_invitation(item);
    let is_announcement = is_announcement(item);

    let mut text = item_text(item).to_string();
    if text.is_empty() && deleted.is_none() {
        // Items without a summary: an invitation card or a bare file.
        text = match &group_invitation {
            Some(inv) if outgoing => format!("You invited them to join {}", inv.group_name),
            Some(inv) => format!("Invited to join {} as {}", inv.group_name, inv.member_role),
            None => file.as_ref()?.file_name.clone(),
        };
    }
    // Older rows prefix a paperclip glyph.
    let text = expand_emoji_shortcodes(text.strip_prefix('📎').map(str::trim).unwrap_or(&text));
    let text = if is_announcement { announcement_text(item, text) } else { text };

    let segments = item_segments(item, is_announcement, &text);
    Some(ChatMessage {
        item_id,
        text,
        // Only what the sender typed; `text` may be the core's summary.
        caption: match (&file, deleted) {
            (Some(_), None) => expand_emoji_shortcodes(item.str_at("/content/msgContent/text").unwrap_or_default().trim()),
            _ => String::new(),
        },
        segments,
        outgoing,
        created_at: item.first_nonempty(&["/meta/itemTs", "/meta/createdAt"]),
        delivery: match item.pointer("/meta/itemStatus") {
            Some(status) => DeliveryStatus::parse(status),
            None if outgoing => DeliveryStatus::Pending,
            None => DeliveryStatus::Delivered,
        },
        file,
        reactions: if deleted.is_some() { Vec::new() } else { reactions(item) },
        quote: item.get("quotedItem").and_then(quote),
        deleted,
        sender: if outgoing || is_announcement { None } else { message_sender(item) },
        is_announcement,
        group_invitation,
        edited: item.bool_at("/meta/itemEdited").unwrap_or(false),
        editable: item.bool_at("/meta/editable").unwrap_or(false),
    })
}

/// SimpleX formatting of an item's text (bubbles and the chat list preview).
fn item_segments(item: &Value, is_announcement: bool, text: &str) -> Vec<TextSegment> {
    // Metadata may already have had its original markers stripped by the
    // pinned core. Apply Cursus syntax to the sender's actual source instead.
    let source = item.str_at("/content/msgContent/text").unwrap_or(text);
    let mut segments = if source.contains('*') && !is_announcement {
        crate::rich_text::profile_text(source)
    } else {
        parse_formatted_text(item.get("formattedText"))
    };
    for seg in segments.iter_mut().filter(|s| s.style == TextStyle::Plain) {
        seg.text = expand_emoji_shortcodes(&seg.text);
    }
    segments
}

fn group_invitation(item: &Value) -> Option<GroupInvitationInfo> {
    let content = item.get("content")?;
    if !matches!(content.tag(), "rcvGroupInvitation" | "sndGroupInvitation") {
        return None;
    }
    let inv = content.get("groupInvitation")?;
    Some(GroupInvitationInfo {
        group_id: inv.i64_at("/groupId")?,
        group_name: inv
            .first_nonempty(&["/groupProfile/displayName", "/localDisplayName"])
            .unwrap_or_else(|| "Group".into()),
        member_role: content.str_at("/memberRole").unwrap_or("member").to_string(),
        status: inv.str_at("/status").unwrap_or("pending").to_string(),
    })
}

/// Protocol items (group events, feature changes, calls…) that the history
/// shows as a centred line, not a bubble.
fn is_announcement(item: &Value) -> bool {
    matches!(
        item.str_at("/content/type"),
        Some(
            "rcvGroupEvent" | "sndGroupEvent" | "rcvDirectEvent" | "rcvConnEvent" | "sndConnEvent"
                | "rcvChatFeature" | "sndChatFeature" | "rcvChatPreference" | "sndChatPreference"
                | "rcvGroupFeature" | "sndGroupFeature" | "rcvChatFeatureRejected"
                | "rcvGroupFeatureRejected" | "rcvDirectE2EEInfo" | "sndDirectE2EEInfo"
                | "rcvGroupE2EEInfo" | "sndGroupE2EEInfo" | "chatBanner" | "sndCall" | "rcvCall"
        )
    )
}

/// Calls get their own wording; group events are led by the member's name.
fn announcement_text(item: &Value, text: String) -> String {
    if let Some(label) = call_label(item) {
        return label;
    }
    match message_sender(item).map(|s| s.display_name) {
        None => text,
        Some(name) if text.is_empty() => name,
        Some(name) if text.get(..name.len()).is_some_and(|p| p.eq_ignore_ascii_case(&name)) => text,
        Some(name) => format!("{name} {text}"),
    }
}

/// "Outgoing call · 0:42", "Missed call", … for a `sndCall` / `rcvCall` item.
fn call_label(item: &Value) -> Option<String> {
    let content = item.get("content")?;
    let outgoing = match content.tag() {
        "sndCall" => true,
        "rcvCall" => false,
        _ => return None,
    };
    let secs = content.i64_at("/duration").unwrap_or(0).max(0);
    let direction = if outgoing { "Outgoing call" } else { "Incoming call" };
    Some(match (outgoing, content.str_at("/status").unwrap_or_default()) {
        (_, "ended") if secs >= 3600 => format!("{direction} · {}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
        (_, "ended") if secs > 0 => format!("{direction} · {}:{:02}", secs / 60, secs % 60),
        (true, "missed" | "pending" | "rejected") => "Call not answered".into(),
        (false, "missed" | "pending") => "Missed call".into(),
        (false, "rejected") => "Call declined".into(),
        (_, "error") => "Call failed".into(),
        (_, "accepted" | "negotiated" | "progress") => format!("{direction} in progress"),
        _ => direction.into(),
    })
}

/// Author of a received group message (channel posts have no member).
fn message_sender(item: &Value) -> Option<MessageSender> {
    let dir = item.get("chatDir")?;
    if !dir.tag().to_ascii_lowercase().contains("grouprcv") {
        return None;
    }
    let member = dir.get("groupMember")?;
    Some(MessageSender {
        group_member_id: member.i64_at("/groupMemberId")?,
        contact_id: member.i64_at("/memberContactId").filter(|id| *id > 0),
        display_name: member_name(member).unwrap_or_else(|| "Member".into()),
        image: image_data_url(member, "/profile/image").or_else(|| image_data_url(member, "/memberProfile/image")),
        role: member.str_at("/memberRole").unwrap_or("member").to_string(),
    })
}

fn quote(value: &Value) -> Option<MessageQuote> {
    if value.is_null() {
        return None;
    }
    let kind = value.str_at("/content/type");
    let text = expand_emoji_shortcodes(value.first(&["/content/text", "/text"]).and_then(Value::as_str).unwrap_or_default());
    let text = match (text.is_empty(), kind) {
        (false, _) => text,
        (true, Some("text")) => return None,
        (true, Some("image")) => "Photo".into(),
        (true, Some("video")) => "Video".into(),
        (true, Some("voice")) => "Voice message".into(),
        (true, Some("file")) => "File".into(),
        (true, _) => "Message".into(),
    };
    Some(MessageQuote { item_id: value.i64_at("/itemId"), text, outgoing: is_outgoing(value) })
}

fn reactions(item: &Value) -> Vec<ReactionCount> {
    item.array_at("/reactions")
        .iter()
        .filter_map(|r| {
            // Some encodings carry the reaction as a bare string.
            let emoji = r.str_at("/reaction/emoji").or_else(|| r.str_at("/reaction"))?;
            let total = r.pointer("/totalReacted").and_then(Value::as_u64).unwrap_or(0) as u32;
            (total > 0).then_some(ReactionCount {
                emoji: reaction_emoji(emoji)?.to_string(),
                user_reacted: r.bool_at("/userReacted").unwrap_or(false),
                total,
            })
        })
        .collect()
}

/// The protocol character of a reaction (❤️ → ❤); `None` for unknown ones.
pub(crate) fn reaction_emoji(emoji: &str) -> Option<&'static str> {
    let emoji = emoji.trim().trim_end_matches('\u{FE0F}');
    KNOWN_REACTIONS.iter().copied().find(|e| *e == emoji)
}

// ── Files ───────────────────────────────────────────────────────────────

fn has_extension(name: &str, extensions: &[&str]) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub(crate) fn is_image_name(name: &str) -> bool {
    has_extension(name, &["jpg", "jpeg", "png", "gif", "webp", "bmp"])
}

/// What a file without a message type looks like, going by its name.
fn kind_from_name(name: &str) -> FileKind {
    if is_image_name(name) {
        FileKind::Image
    } else if has_extension(name, &["mp4", "mov", "mkv", "webm", "m4v", "avi", "3gp", "3g2", "mpeg", "mpg", "ts"]) {
        FileKind::Video
    } else if has_extension(name, &["m4a", "aac", "mp3", "ogg", "opus", "wav", "flac"]) {
        FileKind::Voice
    } else {
        FileKind::File
    }
}

pub(crate) fn file_info(item: &Value) -> Option<FileInfo> {
    let file = item.get("file")?;
    let file_name = file.str_at("/fileName").unwrap_or("file").to_string();
    let file_size = file.i64_at("/fileSize").unwrap_or(0);
    let status = file.str_at("/fileStatus/type").or_else(|| file.str_at("/fileStatus")).unwrap_or_default();
    let state = match status {
        "rcvInvitation" => FileTransferState::Offer,
        "rcvAccepted" | "rcvTransfer" | "sndTransfer" | "sndStored" => FileTransferState::InProgress,
        "rcvComplete" | "sndComplete" => FileTransferState::Complete,
        "rcvCancelled" | "sndCancelled" | "rcvError" | "sndError" | "rcvAborted" | "rcvWarning" | "sndWarning" => {
            FileTransferState::Failed
        }
        _ => FileTransferState::Other,
    };
    let progress = |done: &str, total: &str| (file.i64_at(done), file.i64_at(total));
    let (transferred, transfer_total) = match status {
        "rcvTransfer" => progress("/fileStatus/rcvProgress", "/fileStatus/rcvTotal"),
        "sndTransfer" => progress("/fileStatus/sndProgress", "/fileStatus/sndTotal"),
        _ => (None, None),
    };
    let error_detail = [
        "/fileStatus/rcvFileError/fileError/type",
        "/fileStatus/sndFileError/fileError/type",
        "/fileStatus/rcvFileError/type",
        "/fileStatus/sndFileError/type",
    ]
    .iter()
    .find_map(|p| file.str_at(p))
    .or((state == FileTransferState::Failed).then_some(status))
    .map(str::to_string);
    let content = item.first(&["/content/msgContent", "/msgContent"]);
    let crypto = |keys: [&str; 2]| keys.iter().find_map(|k| file.nonempty(&format!("/fileSource/cryptoArgs/{k}")));
    let (crypto_key, crypto_nonce) = (crypto(["fileKey", "key"]), crypto(["fileNonce", "nonce"]));
    let local_path = match file.nonempty("/fileSource/filePath") {
        Some(stored) => Some(resolve_local_file(&stored)),
        // Complete rows sometimes omit fileSource briefly: the file is then
        // in the files folder under its name. The name is the sender's — keep
        // only its last component so it can't point outside the folder.
        None if state == FileTransferState::Complete => {
            std::path::Path::new(&file_name).file_name().map(|name| files_dir().join(name))
        }
        None => None,
    };
    Some(FileInfo {
        file_id: file.i64_at("/fileId")?,
        kind: content
            .and_then(|c| c.str_at("/type"))
            .map(FileKind::from_msg_content_type)
            .unwrap_or_else(|| kind_from_name(&file_name)),
        duration_secs: content.and_then(|c| c.pointer("/duration")).and_then(Value::as_u64).map(|n| n as u32),
        file_name,
        file_size,
        state,
        local_path,
        encrypted: crypto_key.is_some() && crypto_nonce.is_some(),
        crypto_key,
        crypto_nonce,
        transferred,
        transfer_total,
        error_detail,
        preview: content.and_then(|c| image_data_url(c, "/image")),
    })
}

/// Files in a `newChatItems` event that the official apps download without
/// a tap (small images, voice messages and videos): `(file id, name)`.
pub fn auto_receive_candidates(event: &Event) -> Vec<(i64, String)> {
    let Event::NewChatItems(value) = event else { return Vec::new() };
    value
        .array_at("/chatItems")
        .iter()
        .filter_map(|aci| {
            let item = aci.get("chatItem").unwrap_or(aci);
            let file = file_info(item).filter(|f| !is_outgoing(item) && f.should_auto_receive())?;
            Some((file.file_id, file.file_name))
        })
        .collect()
}

// ── Connection links ────────────────────────────────────────────────────

/// A link in both forms from the `connFullLink` / `connShortLink` pair at
/// `base`; `display` is the short one when the core has it.
pub(crate) fn conn_link(value: &Value, base: &str) -> Option<ConnLink> {
    let full = value.str_at(&format!("{base}/connFullLink"))?.to_string();
    let short = value.nonempty(&format!("{base}/connShortLink"));
    Some(ConnLink { display: short.clone().unwrap_or_else(|| full.clone()), full, short })
}

/// The user's address in a `/_show_address` or `/_address` reply.
pub(crate) fn address_link(value: &Value) -> Option<ConnLink> {
    conn_link(value, "/contactLink/connLinkContact").or_else(|| conn_link(value, "/connLinkContact"))
}

pub(crate) fn address_auto_accept(value: &Value) -> bool {
    ["/contactLink/addressSettings/autoAccept", "/addressSettings/autoAccept"].iter().any(|p| value.is_set(p))
}

pub(crate) fn address_is_business(value: &Value) -> bool {
    value
        .first(&["/contactLink/addressSettings/businessAddress", "/addressSettings/businessAddress"])
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Channel short links are of contact-connection type `C`: `/c#`, not the
/// private groups' `/g#`.
pub fn is_channel_short_link(link: &str) -> bool {
    let l = link.trim();
    (l.starts_with("https://") || l.starts_with("simplex:/")) && l.contains("/c#")
}

fn is_short_conn_link(link: &str) -> bool {
    (link.starts_with("https://") || link.starts_with("simplex:/"))
        && ["/g#", "/c#", "/i#", "/a#", "/r#"].iter().any(|kind| link.contains(kind))
}

/// A link as it is shared (official `simplexChatLink`): full `simplex:/…`
/// URIs become `https://simplex.chat/…`; short links stay as they are
/// (rewriting their host breaks them).
pub(crate) fn shareable_conn_uri(link: &str) -> String {
    let l = link.trim();
    match l.strip_prefix("simplex:/") {
        Some(rest) if !is_short_conn_link(l) => format!("https://simplex.chat/{rest}"),
        _ => l.to_string(),
    }
}

/// A private group's join link in a `/_get link` / `/_create link` reply.
pub(crate) fn group_link(value: &Value) -> Option<ConnLink> {
    let conn = value.first(&["/groupLink/connLinkContact", "/groupLink/connLinkInvitation", "/connLinkContact"])?;
    let short = conn.nonempty("/connShortLink").map(|s| shareable_conn_uri(&s));
    let full = conn.str_at("/connFullLink").map(shareable_conn_uri).or_else(|| short.clone())?;
    Some(ConnLink { display: short.clone().unwrap_or_else(|| full.clone()), full, short })
}

/// A channel's `/c#` link: the one in its profile (what the official apps
/// share), else the group link's short form. `None` when neither is a
/// channel link — a private-group style link can't join a channel.
pub(crate) fn channel_link(value: &Value) -> Option<ConnLink> {
    let link = value
        .first_nonempty(&["/groupInfo/groupProfile/publicGroup/groupLink", "/groupProfile/publicGroup/groupLink"])
        .or_else(|| group_link(value)?.short)
        .map(|l| shareable_conn_uri(&l))
        .filter(|l| is_channel_short_link(l))?;
    Some(ConnLink { display: link.clone(), full: link.clone(), short: Some(link) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MAX_IMAGE_SIZE_AUTO_RCV;
    use serde_json::json;

    fn last_item_preview(item: Value) -> String {
        let chat = json!({
            "chatInfo": { "type": "local", "noteFolder": { "noteFolderId": 1 } },
            "chatItems": [item]
        });
        chat_preview(&chat).expect("chat").preview
    }

    #[test]
    fn blocked_items_read_as_blocked_not_deleted() {
        let item = |deleted: Value| json!({
            "meta": { "itemId": 1, "itemText": "hi", "itemDeleted": deleted },
            "content": { "type": "rcvMsgContent", "msgContent": { "type": "text", "text": "hi" } }
        });
        let blocked = message(&item(json!({ "type": "blocked" }))).unwrap();
        assert_eq!(blocked.deleted.map(DeletedState::label), Some("blocked"));
        assert!(DeletedState::Hidden.can_reveal());
        let by_admin = message(&item(json!({ "type": "blockedByAdmin" }))).unwrap();
        assert_eq!(by_admin.deleted, Some(DeletedState::BlockedByAdmin));
        assert_eq!(message(&item(Value::Null)).unwrap().deleted, None);
    }

    #[test]
    fn chat_list_preview_hides_deleted_content() {
        let marked = json!({
            "meta": { "itemId": 1, "itemText": "https://smp12.simplex.im/i#secret",
                      "itemDeleted": { "type": "deleted", "deletedTs": "2026-11-05T10:00:00Z" } },
            "content": { "type": "sndMsgContent", "msgContent": { "type": "text", "text": "https://smp12.simplex.im/i#secret" } }
        });
        assert_eq!(last_item_preview(marked), "marked deleted");
        let full = json!({
            "meta": { "itemId": 2, "itemText": "still here" },
            "content": { "type": "sndDeleted", "deleted": { "type": "deleted" } }
        });
        assert_eq!(last_item_preview(full), "deleted");
        let live = json!({
            "meta": { "itemId": 3, "itemText": "hello" },
            "content": { "type": "sndMsgContent", "msgContent": { "type": "text", "text": "hello" } }
        });
        assert_eq!(last_item_preview(live), "hello");
    }

    #[test]
    fn chat_list_preview_names_media_and_skips_banners() {
        let item = |content: Value, file: Value| json!({
            "meta": { "itemId": 9, "itemText": "summary" },
            "content": content,
            "file": file,
        });
        let kind_of = |item: Value| {
            let chat = json!({
                "chatInfo": { "type": "local", "noteFolder": { "noteFolderId": 1 } },
                "chatItems": [item]
            });
            let c = chat_preview(&chat).expect("chat");
            (c.preview, c.preview_kind)
        };
        let msg = |mc: Value| json!({ "type": "rcvMsgContent", "msgContent": mc });
        let file = json!({ "fileId": 1, "fileName": "report.pdf", "fileSize": 10 });

        let banner = item(json!({ "type": "rcvDirectE2EEInfo", "e2eeInfo": {} }), Value::Null);
        assert_eq!(kind_of(banner.clone()), (String::new(), PreviewKind::Text));
        assert!(item_preview(&item(json!({ "type": "chatBanner" }), Value::Null)).is_none());
        assert!(item_preview(&item(json!({ "type": "rcvChatFeature", "feature": "timedMessages" }), Value::Null)).is_none());

        assert_eq!(kind_of(item(msg(json!({ "type": "image", "text": "", "image": "x" })), file.clone())), ("Image".into(), PreviewKind::Image));
        assert_eq!(kind_of(item(msg(json!({ "type": "image", "text": "At the crag", "image": "x" })), file.clone())), ("At the crag".into(), PreviewKind::Image));
        assert_eq!(kind_of(item(msg(json!({ "type": "video", "text": "", "image": "x", "duration": 4 })), file.clone())), ("Video".into(), PreviewKind::Video));
        assert_eq!(kind_of(item(msg(json!({ "type": "voice", "text": "", "duration": 65 })), file.clone())), ("Voice message · 1:05".into(), PreviewKind::Voice));
        assert_eq!(kind_of(item(msg(json!({ "type": "file", "text": "" })), file.clone())), ("report.pdf".into(), PreviewKind::File));
        assert_eq!(kind_of(item(msg(json!({ "type": "text", "text": "hi" })), Value::Null)), ("summary".into(), PreviewKind::Text));
        let call = item(json!({ "type": "sndCall", "status": "ended", "duration": 42 }), Value::Null);
        assert_eq!(kind_of(call), ("Outgoing call · 0:42".into(), PreviewKind::Call));

        // Formatting, as in the bubble.
        let styled = json!({
            "meta": { "itemId": 9, "itemText": "zio pera" },
            "content": msg(json!({ "type": "text", "text": "*zio pera*" })),
        });
        let (_, kind, segments) = item_preview(&styled).unwrap();
        assert_eq!(kind, PreviewKind::Text);
        assert!(segments.iter().any(|s| s.style == TextStyle::Italic && s.text == "zio pera"), "{segments:?}");

        // Looking back past events finds the newest real message.
        let older = item(msg(json!({ "type": "text", "text": "hi" })), Value::Null);
        assert_eq!(latest_preview(&[older, banner]).map(|p| (p.0, p.1)), Some(("summary".into(), PreviewKind::Text)));
        let list = json!({ "chats": [{
            "chatInfo": { "type": "local", "noteFolder": { "noteFolderId": 1 } },
            "chatItems": [item(json!({ "type": "chatBanner" }), Value::Null)]
        }]});
        assert_eq!(chat_previews_with_gaps(&list)[0].1, Some(9));
    }

    #[test]
    fn contact_request_is_pending_until_connected() {
        let pending = |contact: Value| pending_accept_request_id(&contact);
        assert_eq!(pending(json!({ "contactRequestId": 7 })), Some(7));
        assert_eq!(pending(json!({ "contactRequestId": 7, "activeConn": { "connStatus": { "type": "new" } } })), Some(7));
        // The core keeps contactRequestId after Accept; connStatus advances.
        assert_eq!(pending(json!({ "contactRequestId": 7, "activeConn": { "connStatus": { "type": "sndReady" } } })), None);
        assert_eq!(pending(json!({ "contactRequestId": 7, "activeConn": { "connStatus": "ready" } })), None);
        assert_eq!(pending(json!({ "activeConn": { "connStatus": { "type": "new" } } })), None);
    }

    #[test]
    fn direct_chat_title_prefers_the_alias() {
        let chat = json!({
            "chatInfo": { "type": "direct", "contact": {
                "contactId": 4, "localAlias": "Mum", "localDisplayName": "ada_1",
                "profile": { "displayName": "Ada", "image": "https://evil.example/x.png" },
                "chatSettings": { "enableNtfs": "none", "favorite": true, "sendRcpts": null },
                "chatTags": [2, 5]
            }},
            "chatItems": [],
            "chatStats": { "unreadCount": 3, "unreadChat": true }
        });
        let c = chat_preview(&chat).expect("chat");
        assert_eq!((c.title.as_str(), c.local_alias.as_str()), ("Mum", "Mum"));
        assert_eq!(c.peer_profile.display_name, "Ada");
        assert_eq!(c.peer_profile.image, None, "only inline pictures");
        assert!(c.settings.favorite && c.settings.enable_ntfs == NtfFilter::None && c.settings.send_rcpts.is_none());
        assert_eq!((c.unread, c.unread_chat, c.tag_ids), (3, true, vec![2, 5]));
    }

    #[test]
    fn members_list_starts_with_you_and_skips_the_gone() {
        let member = |id: i64, status: &str, role: &str| {
            json!({ "groupMemberId": id, "memberRole": role, "memberStatus": status,
                    "localDisplayName": "alice", "memberProfile": { "displayName": "alice", "localAlias": "Ali" } })
        };
        let reply = json!({ "group": {
            "groupInfo": { "membership": member(1, "complete", "owner") },
            "members": [member(2, "complete", "member"), member(3, "left", "member"),
                        member(4, "complete", "relay"), member(1, "complete", "owner")]
        }});
        let members = group_members(&reply);
        assert_eq!(members.iter().map(|m| m.group_member_id).collect::<Vec<_>>(), vec![1, 2]);
        assert!(members[0].is_me && !members[1].is_me);
        assert_eq!(members[1].display_name, "Ali", "your alias for them");
        assert_eq!(members[1].public_name, "alice");
    }

    #[test]
    fn call_item_is_quiet_label() {
        let call = |dir: &str, ty: &str, status: &str, secs: i64| {
            message(&json!({
                "chatDir": { "type": dir },
                "meta": { "itemId": 7, "itemTs": "2026-11-07T00:09:45Z",
                    "itemText": "outgoing call: ended (00:02)", "itemStatus": { "type": "sndSent" } },
                "content": { "type": ty, "status": status, "duration": secs }
            }))
            .expect("call")
        };
        let msg = call("directSnd", "sndCall", "ended", 42);
        assert!(msg.is_announcement);
        assert_eq!(msg.text, "Outgoing call · 0:42");
        assert_eq!(call("directRcv", "rcvCall", "missed", 0).text, "Missed call");
        assert_eq!(call("directRcv", "rcvCall", "ended", 3725).text, "Incoming call · 1:02:05");
    }

    #[test]
    fn group_event_is_announcement() {
        let item = json!({
            "chatDir": { "type": "groupRcv", "groupMember": {
                "groupMemberId": 3, "localDisplayName": "Ada", "memberRole": "member",
                "profile": { "displayName": "Ada" }
            }},
            "meta": { "itemId": 42, "itemTs": "2026-11-03T16:37:00Z",
                      "itemText": "updated group profile", "itemStatus": { "type": "rcvRead" } },
            "content": { "type": "rcvGroupEvent", "rcvGroupEvent": { "type": "groupUpdated", "groupProfile": {} } }
        });
        let msg = message(&item).expect("event");
        assert!(msg.is_announcement);
        assert_eq!(msg.text, "Ada updated group profile");
        assert!(msg.sender.is_none());
    }

    #[test]
    fn reactions_use_the_protocol_character() {
        assert_eq!(reaction_emoji("❤️"), Some("❤"));
        assert_eq!(reaction_emoji(" 👍 "), Some("👍"));
        assert_eq!(reaction_emoji("🦄"), None);
        let item = json!({ "reactions": [
            { "reaction": { "type": "emoji", "emoji": "❤" }, "userReacted": true, "totalReacted": 2 },
            { "reaction": { "type": "emoji", "emoji": "👍" }, "userReacted": false, "totalReacted": 0 }
        ]});
        let parsed = reactions(&item);
        assert_eq!(parsed.len(), 1, "empty reactions are dropped");
        assert!(parsed[0].user_reacted && parsed[0].total == 2);
    }

    fn file_item(name: &str, status: &str, source: Value) -> Value {
        json!({ "file": { "fileId": 9, "fileName": name, "fileSize": 12,
                          "fileStatus": { "type": status }, "fileSource": source } })
    }

    #[test]
    fn file_info_resolves_paths_inside_the_files_folder() {
        let info = file_info(&file_item("shot.jpg", "rcvComplete", json!({ "filePath": "shot.jpg", "cryptoArgs": null })))
            .expect("file");
        assert_eq!(info.state, FileTransferState::Complete);
        assert!(!info.encrypted);
        assert_eq!(info.local_path, Some(files_dir().join("shot.jpg")));
        // No fileSource yet: the sender's name must not lead out of files/.
        let sneaky = file_info(&file_item("../../.bashrc", "rcvComplete", Value::Null)).expect("file");
        assert_eq!(sneaky.local_path, Some(files_dir().join(".bashrc")));
        let offer = file_info(&file_item("a.pdf", "rcvInvitation", Value::Null)).expect("file");
        assert_eq!((offer.local_path, offer.kind), (None, FileKind::File));
    }

    #[test]
    fn file_info_parses_crypto_args_and_errors() {
        let source = json!({ "filePath": "secret.bin", "cryptoArgs": { "fileKey": "abcKEY", "fileNonce": "xyzNONCE" } });
        let info = file_info(&file_item("secret.bin", "rcvComplete", source)).expect("file");
        assert!(info.encrypted);
        assert_eq!((info.crypto_key.as_deref(), info.crypto_nonce.as_deref()), (Some("abcKEY"), Some("xyzNONCE")));
        let failed = file_info(&file_item("x.bin", "rcvError", Value::Null)).expect("file");
        assert_eq!((failed.state, failed.error_detail.as_deref()), (FileTransferState::Failed, Some("rcvError")));
    }

    #[test]
    fn auto_receive_only_small_incoming_media() {
        let offer = |dir: &str, kind: &str, size: i64| {
            Event::NewChatItems(json!({ "chatItems": [{ "chatItem": {
                "chatDir": { "type": dir },
                "content": { "msgContent": { "type": kind, "text": "" } },
                "file": { "fileId": 42, "fileName": "pic.jpg", "fileSize": size, "fileStatus": { "type": "rcvInvitation" } }
            }}]}))
        };
        let small = MAX_IMAGE_SIZE_AUTO_RCV;
        assert_eq!(auto_receive_candidates(&offer("directRcv", "image", small)), vec![(42, "pic.jpg".to_string())]);
        assert!(auto_receive_candidates(&offer("directRcv", "image", small + 1)).is_empty());
        assert!(auto_receive_candidates(&offer("directRcv", "file", 1024)).is_empty());
        assert!(auto_receive_candidates(&offer("directSnd", "image", small)).is_empty());
    }

    #[test]
    fn message_star_styles_use_source_instead_of_the_core_summary() {
        let item = json!({
            "meta": {"itemId": 123, "itemText": "bold italic"},
            "chatDir": {"type": "directRcv"},
            "content": {"type": "rcvMsgContent", "msgContent": {
                "type": "text", "text": "**bold** *italic*"
            }},
            "formattedText": [{"text": "bold italic", "format": {"type": "bold"}}]
        });
        let parsed = message(&item).unwrap();
        assert!(parsed.segments.iter().any(|s| s.text == "bold" && s.style == TextStyle::Bold));
        assert!(parsed.segments.iter().any(|s| s.text == "italic" && s.style == TextStyle::Italic));
    }

    /// The core's summary ("voice message (00:03)") is for previews; the
    /// caption is only what the sender typed.
    #[test]
    fn file_messages_split_summary_and_caption() {
        let item = |content: Value, item_text: &str| {
            message(&json!({
                "meta": { "itemId": 7, "itemText": item_text, "itemTs": "2026-11-08T11:39:00Z" },
                "chatDir": { "type": "directRcv" },
                "content": { "type": "rcvMsgContent", "msgContent": content },
                "file": { "fileId": 3, "fileName": "voice.m4a", "fileSize": 9000, "fileStatus": { "type": "rcvComplete" } }
            }))
            .unwrap()
        };
        let voice = item(json!({ "type": "voice", "text": "", "duration": 3 }), "voice message (00:03)");
        assert_eq!((voice.text.as_str(), voice.caption.as_str()), ("voice message (00:03)", ""));
        assert_eq!(voice.file.unwrap().duration_secs, Some(3));
        let photo = item(json!({ "type": "image", "text": "  sunset!  ", "image": "data:image/jpg;base64,xx" }), "sunset!");
        assert_eq!(photo.caption, "sunset!");
        assert_eq!(photo.file.unwrap().preview.as_deref(), Some("data:image/jpg;base64,xx"));
    }

    #[test]
    fn links_are_shared_in_their_working_form() {
        assert!(is_channel_short_link("simplex:/c#abc?h=smp18.simplex.im"));
        assert!(is_channel_short_link("https://smp18.simplex.im/c#abc"));
        assert!(!is_channel_short_link("https://smp18.simplex.im/g#abc"));
        assert!(!is_channel_short_link("simplex:/contact#/?v=1"));
        assert_eq!(shareable_conn_uri("simplex:/c#abc?h=smp18.simplex.im"), "simplex:/c#abc?h=smp18.simplex.im");
        assert_eq!(shareable_conn_uri("simplex:/contact#/?v=1&smp=x"), "https://simplex.chat/contact#/?v=1&smp=x");
        let reply = json!({ "groupLink": { "connLinkContact": {
            "connFullLink": "simplex:/contact#/?v=1", "connShortLink": "https://smp.example/g#abc" } } });
        let link = group_link(&reply).expect("link");
        assert_eq!((link.display.as_str(), link.full.as_str()), ("https://smp.example/g#abc", "https://simplex.chat/contact#/?v=1"));
        assert_eq!(channel_link(&reply), None, "a /g# link can't join a channel");
        let channel = json!({ "groupInfo": { "groupProfile": { "publicGroup": { "groupLink": "https://smp.example/c#xyz" } } } });
        assert_eq!(channel_link(&channel).expect("channel").display, "https://smp.example/c#xyz");
    }
    #[test]
    fn group_dm_requests_keep_contact_identity_until_accepted() {
        let mut contact = json!({"contactId": 42, "contactStatus": "active", "profile": {"displayName": "Member"},
            "groupDirectInv": {"groupDirectInvLink": "simplex:test", "memberRemoved": false}});
        let pending = chat_preview(&json!({"chatInfo": {"type": "direct", "contact": contact}})).unwrap();
        assert_eq!(pending.id, ChatId::new(ChatType::Direct, 42));
        assert_eq!(pending.pending_request_id, Some(42));
        assert!(pending.member_request);
        assert!(pending.preview.contains("Direct message request"));
        for state in ["new", "prepared", "joined", "sndReady", "ready"] {
            contact["activeConn"] = json!({"connStatus": {"type": state}});
            let chat = chat_preview(&json!({"chatInfo": {"type": "direct", "contact": contact}})).unwrap();
            assert_eq!(chat.member_request, matches!(state, "new" | "prepared"), "{state}");
            assert_eq!(chat.pending_request_id.is_some(), matches!(state, "new" | "prepared"), "{state}");
        }
    }
    #[test]
    fn group_dm_events_refresh_the_chat_list() {
        for name in ["newMemberContactReceivedInv", "memberContactAccepted", "newMemberContactSentInv"] {
            let event = crate::models::Event::Other { type_name: name.into(), body: json!({}) };
            assert!(event.refreshes_chats(), "{name}");
        }
    }

}
