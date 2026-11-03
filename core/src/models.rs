use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatType {
    Direct,
    Group,
    Local,
    Other,
}

/// Group message scope — member support (“Chat with admins”).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupChatScope {
    /// `None` = own thread with admins; `Some(id)` = that member’s thread (moderator view).
    MemberSupport { group_member_id: Option<i64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChatId {
    pub chat_type: ChatType,
    pub api_id: i64,
    /// Set for scoped group chats (`#id(_support)` / `#id(_support:gmId)`).
    pub scope: Option<GroupChatScope>,
}

impl ChatId {
    pub fn new(chat_type: ChatType, api_id: i64) -> Self {
        Self {
            chat_type,
            api_id,
            scope: None,
        }
    }

    pub fn with_support(group_id: i64, group_member_id: Option<i64>) -> Self {
        Self {
            chat_type: ChatType::Group,
            api_id: group_id,
            scope: Some(GroupChatScope::MemberSupport { group_member_id }),
        }
    }

    pub fn without_scope(&self) -> Self {
        Self {
            chat_type: self.chat_type,
            api_id: self.api_id,
            scope: None,
        }
    }

    pub fn same_base(&self, other: &ChatId) -> bool {
        self.chat_type == other.chat_type && self.api_id == other.api_id
    }

    pub fn command_ref(&self) -> String {
        let prefix = match self.chat_type {
            ChatType::Direct => "@",
            ChatType::Group => "#",
            ChatType::Local => "*",
            ChatType::Other => "@",
        };
        let base = format!("{prefix}{}", self.api_id);
        match self.scope {
            Some(GroupChatScope::MemberSupport {
                group_member_id: None,
            }) => format!("{base}(_support)"),
            Some(GroupChatScope::MemberSupport {
                group_member_id: Some(gm),
            }) => format!("{base}(_support:{gm})"),
            None => base,
        }
    }
}

/// Active-user / contact profile fields from SimpleX `Profile` JSON.
#[derive(Debug, Clone, Default)]
pub struct ProfileFields {
    pub display_name: String,
    pub full_name: String,
    pub short_descr: String,
    pub description: String,
    /// `data:image/(png|jpg);base64,…` or empty when unset.
    pub image: Option<String>,
    /// Present when the user shares their address on the profile.
    pub contact_link: Option<String>,
    /// Raw preferences object to round-trip on `/_profile` updates.
    pub preferences: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub user_id: i64,
    pub profile: ProfileFields,
}

impl UserProfile {
    pub fn display_name(&self) -> &str {
        &self.profile.display_name
    }

    pub fn shares_address(&self) -> bool {
        self.profile
            .contact_link
            .as_ref()
            .is_some_and(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtfFilter {
    All,
    Mentions,
    None,
}

impl NtfFilter {
    pub fn as_api(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mentions => "mentions",
            Self::None => "none",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "none" => Self::None,
            "mentions" => Self::Mentions,
            _ => Self::All,
        }
    }

    /// Cycle mute modes the way the official app does for direct chats.
    pub fn next_direct(self) -> Self {
        match self {
            Self::All => Self::None,
            Self::None | Self::Mentions => Self::All,
        }
    }

    pub fn is_muted(self) -> bool {
        !matches!(self, Self::All)
    }
}

#[derive(Debug, Clone)]
pub struct ChatSettings {
    pub enable_ntfs: NtfFilter,
    pub favorite: bool,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            enable_ntfs: NtfFilter::All,
            favorite: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatTag {
    pub id: i64,
    pub text: String,
    pub emoji: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatPreview {
    pub id: ChatId,
    pub title: String,
    pub preview: String,
    pub unread: i64,
    /// Soft unread flag (`/_unread chat … on`) independent of item counts.
    pub unread_chat: bool,
    pub settings: ChatSettings,
    pub tag_ids: Vec<i64>,
    /// Present when this direct chat is still an inbound address request.
    /// Accept with `/_accept <id>` before sending is allowed.
    pub pending_request_id: Option<i64>,
    /// Peer / group profile snapshot for list + profile page.
    pub peer_profile: ProfileFields,
    /// Local alias for the contact / group (`/_set alias`); empty when unset.
    pub local_alias: String,
    /// `groupInfo.useRelays` — SimpleX channels are relay-backed groups.
    pub is_channel: bool,
    /// Own membership role in a group (`owner` / `admin` / …); `None` for DMs.
    pub membership_role: Option<String>,
    /// Raw `groupProfile.groupPreferences` for round-trip on `/_group_profile`.
    pub group_preferences: Option<serde_json::Value>,
    /// Raw `groupProfile.memberAdmission`.
    pub member_admission: Option<serde_json::Value>,
    /// Raw `groupProfile.publicGroup` (channels / public groups).
    pub public_group: Option<serde_json::Value>,
    /// Direct-chat `userPreferences` (what we allow for this contact).
    pub contact_preferences: Option<serde_json::Value>,
    /// Direct-chat `mergedPreferences` (you + contact allow, for feature UI).
    pub contact_merged_preferences: Option<serde_json::Value>,
    /// Moderators: count of members needing support attention.
    pub members_require_attention: i64,
    /// Own membership support-chat unread (members chatting with admins).
    pub membership_support_unread: i64,
    /// Whether `membership.supportChat` exists (thread already opened).
    pub membership_has_support_chat: bool,
}

/// Editable group profile payload for `/_group_profile` (full round-trip).
#[derive(Debug, Clone, Default)]
pub struct GroupProfileUpdate {
    pub display_name: String,
    pub full_name: String,
    /// Short bio under the name (≤160 chars); JSON `shortDescr`.
    pub short_descr: String,
    /// Welcome message shown to new members; JSON `description`.
    pub welcome_message: String,
    pub image: Option<String>,
    pub group_preferences: Option<serde_json::Value>,
    pub member_admission: Option<serde_json::Value>,
    pub public_group: Option<serde_json::Value>,
}

/// Chat relay available for `/_public group` (from `/_servers`).
#[derive(Debug, Clone)]
pub struct ChatRelayInfo {
    pub chat_relay_id: i64,
    pub display_name: String,
}

/// Per-member support chat stats (`supportChat` on GroupMember).
#[derive(Debug, Clone, Default)]
pub struct GroupSupportChat {
    pub unread: i64,
    pub member_attention: i64,
    pub mentions: i64,
}

/// Member row from `/_members #{groupId}`.
#[derive(Debug, Clone)]
pub struct GroupMemberInfo {
    pub group_member_id: i64,
    pub contact_id: Option<i64>,
    pub display_name: String,
    pub role: String,
    /// Protocol `memberStatus` (e.g. `active`, `pending_review`, `invited`).
    pub status: String,
    /// Blocked for everyone by a moderator (`memberBlockedByAdmin`).
    pub blocked_by_admin: bool,
    /// Local hide (`memberSettings.showMessages == false`).
    pub show_messages: bool,
    /// Present once a support thread exists with this member.
    pub support_chat: Option<GroupSupportChat>,
}

/// Roles used by group admin UI (excludes `relay` / `author`).
pub const GROUP_ROLES: &[&str] = &["observer", "member", "moderator", "admin", "owner"];

/// Rank for comparing roles (higher = more privilege).
pub fn group_role_rank(role: &str) -> u8 {
    match role {
        "observer" => 1,
        "member" => 2,
        "moderator" => 3,
        "admin" => 4,
        "owner" => 5,
        _ => 0,
    }
}

/// Active / pending WebRTC call (signaling only until media bridge is attached).
#[derive(Debug, Clone)]
pub struct CallInfo {
    pub contact_id: i64,
    pub peer_name: String,
    pub outgoing: bool,
    pub media: CallMedia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallMedia {
    Audio,
    Video,
}

impl CallMedia {
    pub fn as_api(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Other { type_name: String, text: String },
}

/// Outgoing delivery / receipt state from `meta.itemStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    /// Queued locally (`sndNew`).
    Pending,
    /// Reached the relay (`sndSent`).
    Sent,
    /// Peer confirmed receipt (`sndRcvd`).
    Delivered,
    /// Send failed (`sndError` / `sndErrorAuth`).
    Error,
    /// Transient send warning (`sndWarning`).
    Warning,
}

impl DeliveryStatus {
    pub fn parse(status: &Value) -> Self {
        let ty = status
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match ty {
            "sndSent" => Self::Sent,
            "sndRcvd" => Self::Delivered,
            "sndError" | "sndErrorAuth" => Self::Error,
            "sndWarning" => Self::Warning,
            // sndNew and anything unexpected while sending.
            _ => Self::Pending,
        }
    }

    /// Compact receipt mark for the bubble footer.
    pub fn receipt_mark(self) -> &'static str {
        match self {
            Self::Pending => "···",
            Self::Sent => "✓",
            Self::Delivered => "✓✓",
            Self::Error => "!",
            Self::Warning => "!",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTransferState {
    /// Incoming offer waiting for Accept.
    Offer,
    /// Accepted / transferring / uploading.
    InProgress,
    /// Available locally (sent complete or received complete).
    Complete,
    /// Cancelled or failed.
    Failed,
    Other,
}

/// Matches official SimpleX auto-receive size caps
/// (`MAX_*_SIZE_AUTO_RCV` in mobile Utils / FileUtils).
pub const MAX_IMAGE_SIZE_AUTO_RCV: i64 = 261_120 * 2; // ~510 KiB
pub const MAX_VOICE_SIZE_AUTO_RCV: i64 = 261_120 * 2;
pub const MAX_VIDEO_SIZE_AUTO_RCV: i64 = 1_047_552; // ~1023 KiB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Video,
    Voice,
    File,
}

impl FileKind {
    pub fn from_msg_content_type(ty: &str) -> Self {
        match ty {
            "image" => Self::Image,
            "video" => Self::Video,
            "voice" => Self::Voice,
            _ => Self::File,
        }
    }

    /// Whether the official client would auto-accept this offer (privacyAcceptImages on).
    pub fn should_auto_receive(self, file_size: i64, state: FileTransferState) -> bool {
        if !matches!(state, FileTransferState::Offer) {
            return false;
        }
        match self {
            Self::Image => file_size > 0 && file_size <= MAX_IMAGE_SIZE_AUTO_RCV,
            Self::Video => file_size > 0 && file_size <= MAX_VIDEO_SIZE_AUTO_RCV,
            // Official skips voice already in rcvAccepted; Offer-only is enough here.
            Self::Voice => file_size > 0 && file_size <= MAX_VOICE_SIZE_AUTO_RCV,
            Self::File => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: i64,
    pub file_name: String,
    pub file_size: i64,
    pub state: FileTransferState,
    pub kind: FileKind,
    /// Voice/video duration in whole seconds (SimpleX `msgContent.duration`).
    pub duration_secs: Option<u32>,
    /// Absolute path when the bytes are available locally (resolved against `files_dir` when relative).
    pub local_path: Option<std::path::PathBuf>,
    /// True when SimpleX stored CryptoFile args (encrypted at rest).
    pub encrypted: bool,
    /// Base64url CryptoFile key (present when `encrypted`).
    pub crypto_key: Option<String>,
    /// Base64url CryptoFile nonce (present when `encrypted`).
    pub crypto_nonce: Option<String>,
    /// Bytes transferred so far (`rcvProgress` / `sndProgress` / live XFTP events).
    pub transferred: Option<i64>,
    /// Expected total for the current transfer (may differ briefly from `file_size`).
    pub transfer_total: Option<i64>,
    /// Human-readable error/warning from `fileStatus` when failed.
    pub error_detail: Option<String>,
}

impl FileInfo {
    pub fn should_auto_receive(&self) -> bool {
        self.kind.should_auto_receive(self.file_size, self.state)
    }

    /// 0.0..=1.0 when progress is known.
    pub fn progress_fraction(&self) -> Option<f32> {
        let done = self.transferred? as f32;
        let total = self
            .transfer_total
            .or(Some(self.file_size).filter(|n| *n > 0))? as f32;
        if total <= 0.0 {
            return None;
        }
        Some((done / total).clamp(0.0, 1.0))
    }
}

/// Protocol-allowed reaction emojis (`MREmojiChar` in SimpleX).
/// Heart is stored/sent as ❤ (U+2764); UI should show ❤️.
pub const KNOWN_REACTIONS: &[&str] = &["👍", "👎", "😀", "😂", "😢", "❤", "🚀", "✅"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionCount {
    /// Protocol emoji char (❤ without VS16).
    pub emoji: String,
    pub user_reacted: bool,
    pub total: u32,
}

/// One group member who set a reaction (`MemberReaction` from libsimplex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionMember {
    pub display_name: String,
}

impl ReactionCount {
    /// Glyph for display (❤ → ❤️).
    pub fn display_glyph(&self) -> String {
        if self.emoji == "❤" || self.emoji == "❤️" {
            "❤️".into()
        } else {
            self.emoji.clone()
        }
    }
}

/// How a chat item was removed / hidden (matches SimpleX `itemDeleted` / deleted content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletedState {
    /// `meta.itemDeleted` set — content still in DB; official UI shows “marked deleted”.
    Marked,
    /// Legacy / full-delete content (`sndDeleted` / `rcvDeleted`).
    Content,
}

/// Quoted parent message for a reply bubble.
#[derive(Debug, Clone)]
pub struct MessageQuote {
    pub item_id: Option<i64>,
    pub text: String,
    pub outgoing: bool,
}

/// Sender of an incoming group / channel message (`chatDir.groupMember`).
#[derive(Debug, Clone)]
pub struct MessageSender {
    pub group_member_id: i64,
    pub display_name: String,
    pub image: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub item_id: i64,
    pub text: String,
    /// SimpleX markdown segments (`formattedText`); empty → render `text` plain.
    pub segments: Vec<crate::rich_text::TextSegment>,
    /// True when this device sent / created the item (incl. Notes).
    pub outgoing: bool,
    pub created_at: Option<String>,
    /// Meaningful for outgoing items; ignored for incoming.
    pub delivery: DeliveryStatus,
    pub file: Option<FileInfo>,
    /// Aggregated reactions on this item.
    pub reactions: Vec<ReactionCount>,
    /// Parent message when this item is a reply.
    pub quote: Option<MessageQuote>,
    /// Soft/hard deletion state from SimpleX.
    pub deleted: Option<DeletedState>,
    /// Present for incoming group messages (and some channel items).
    pub sender: Option<MessageSender>,
    /// Protocol / group event — render as centered mid-text, not a bubble.
    pub is_announcement: bool,
}

impl ChatMessage {
    /// Short emoji-only text message (no file) → large emoji bubble.
    pub fn is_emoji_only(&self) -> bool {
        !self.is_announcement
            && self.deleted.is_none()
            && self.file.is_none()
            && crate::rich_text::is_short_emoji(&self.text)
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ConnLink {
    /// Prefer short HTTPS link when present; otherwise full `simplex:` URI.
    pub display: String,
    pub full: String,
    pub short: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Raw JSON event from `chat_recv_msg_wait` until typed decoding expands.
    Raw(Value),
    NewChatItems(Value),
    ChatItemsStatusesUpdated(Value),
    ChatStarted,
    ContactConnected(Value),
    ReceivedContactRequest(Value),
    Other { type_name: String, body: Value },
}

impl Event {
    /// Whether the chat list should be refreshed after this event.
    pub fn refreshes_chats(&self) -> bool {
        match self {
            Self::NewChatItems(_)
            | Self::ContactConnected(_)
            | Self::ReceivedContactRequest(_) => true,
            Self::Other { type_name, .. } => matches!(
                type_name.as_str(),
                "contactConnected"
                    | "contactUpdated"
                    | "contactsList"
                    | "receivedContactRequest"
                    | "contactRequestRejected"
                    | "acceptingContactRequest"
                    | "contactAdded"
                    | "chatAdded"
                    | "groupJoined"
                    | "groupCreated"
                    | "publicGroupCreated"
                    // Cleared when channel relays finish publishing short-link data
                    // (`creating_in_progress` → 0); without this reload channels stay hidden.
                    | "groupLinkDataUpdated"
                    | "groupRelayUpdated"
                    | "groupRelays"
                    | "sentGroupInvitation"
                    | "groupUpdated"
                    | "groupMembers"
                    | "userContactLinkCreated"
                    | "callInvitation"
                    | "callOffer"
                    | "callAnswer"
                    | "callEnded"
                    | "calling"
                    | "invitation"
                    | "sentConfirmation"
                    | "sentInvitation"
                    | "newChatItems"
                    | "sndFileCompleteXFTP"
                    | "sndFileProgressXFTP"
                    | "rcvFileComplete"
                    | "rcvFileProgressXFTP"
                    | "rcvFileAccepted"
                    | "rcvFileStart"
                    | "rcvFileError"
                    | "rcvFileWarning"
                    | "sndFileError"
                    | "sndFileWarning"
                    | "chatItemsDeleted"
                    | "groupChatItemsDeleted"
                    | "chatItemUpdated"
                    | "chatItemReaction"
            ),
            _ => false,
        }
    }

    /// Whether the open conversation should reload message rows (file progress, etc.).
    pub fn refreshes_messages(&self) -> bool {
        match self {
            Self::NewChatItems(_) | Self::ChatItemsStatusesUpdated(_) => true,
            Self::Other { type_name, .. } => matches!(
                type_name.as_str(),
                "sndFileCompleteXFTP"
                    | "sndFileProgressXFTP"
                    | "sndFileRedirectStartXFTP"
                    | "rcvFileComplete"
                    | "rcvFileProgressXFTP"
                    | "rcvFileAccepted"
                    | "rcvFileStart"
                    | "chatItemUpdated"
                    | "chatItemReaction"
                    | "chatItemsDeleted"
                    | "groupChatItemsDeleted"
            ),
            _ => false,
        }
    }

    /// Chat id hinted by a `newChatItems` payload, if present.
    pub fn chat_id_hint(&self) -> Option<ChatId> {
        let Event::NewChatItems(value) = self else {
            return None;
        };
        let info = value
            .pointer("/chatItems/0/chatInfo")
            .or_else(|| value.get("chatInfo"))?;
        parse_chat_info_id(info)
    }

    /// Live XFTP progress patches: `(file_id, transferred, total)`.
    pub fn file_progress_updates(&self) -> Option<(i64, i64, i64)> {
        let Event::Other { type_name, body } = self else {
            return None;
        };
        match type_name.as_str() {
            "rcvFileProgressXFTP" => {
                let file_id = body
                    .pointer("/rcvFileTransfer/fileId")
                    .or_else(|| body.pointer("/chatItem/chatItem/file/fileId"))
                    .or_else(|| body.pointer("/chatItem/file/fileId"))
                    .and_then(|v| v.as_i64())?;
                let transferred = body.get("receivedSize").and_then(|v| v.as_i64())?;
                let total = body.get("totalSize").and_then(|v| v.as_i64())?;
                Some((file_id, transferred, total))
            }
            "sndFileProgressXFTP" => {
                let file_id = body
                    .pointer("/sndFileTransfer/fileId")
                    .or_else(|| body.pointer("/chatItem/chatItem/file/fileId"))
                    .or_else(|| body.pointer("/chatItem/file/fileId"))
                    .and_then(|v| v.as_i64())?;
                let transferred = body
                    .get("sentSize")
                    .or_else(|| body.get("receivedSize"))
                    .and_then(|v| v.as_i64())?;
                let total = body.get("totalSize").and_then(|v| v.as_i64())?;
                Some((file_id, transferred, total))
            }
            _ => None,
        }
    }

    /// Status patches from `chatItemsStatusesUpdated` (item id → delivery).
    pub fn status_updates(&self) -> Vec<(ChatId, i64, DeliveryStatus)> {
        let Event::ChatItemsStatusesUpdated(value) = self else {
            return Vec::new();
        };
        let Some(items) = value.get("chatItems").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|aci| {
                let info = aci.get("chatInfo")?;
                let chat_id = parse_chat_info_id(info)?;
                let item = aci.get("chatItem")?;
                let item_id = item.pointer("/meta/itemId")?.as_i64()?;
                let status = item
                    .pointer("/meta/itemStatus")
                    .map(DeliveryStatus::parse)
                    .unwrap_or(DeliveryStatus::Pending);
                Some((chat_id, item_id, status))
            })
            .collect()
    }
}

fn parse_chat_info_id(info: &Value) -> Option<ChatId> {
    let type_name = info.get("type")?.as_str()?;
    match type_name {
        "direct" => Some(ChatId {
            chat_type: ChatType::Direct,
            api_id: info.pointer("/contact/contactId")?.as_i64()?,
            scope: None,
        }),
        "group" => {
            let api_id = info.pointer("/groupInfo/groupId")?.as_i64()?;
            let scope = parse_group_chat_scope(info.get("groupChatScope"));
            Some(ChatId {
                chat_type: ChatType::Group,
                api_id,
                scope,
            })
        }
        "local" => Some(ChatId {
            chat_type: ChatType::Local,
            api_id: info.pointer("/noteFolder/noteFolderId")?.as_i64()?,
            scope: None,
        }),
        _ => None,
    }
}

/// `groupChatScope` from SimpleX `ChatInfo` JSON (`type: memberSupport`).
fn parse_group_chat_scope(value: Option<&Value>) -> Option<GroupChatScope> {
    let scope = value?;
    if scope.is_null() {
        return None;
    }
    match scope.get("type").and_then(|v| v.as_str())? {
        "memberSupport" => {
            let group_member_id = scope
                .pointer("/groupMember/groupMemberId")
                .and_then(|v| v.as_i64());
            Some(GroupChatScope::MemberSupport { group_member_id })
        }
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiEnvelope {
    #[serde(default)]
    #[allow(dead_code)]
    pub remote_host_id: Option<i64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}

impl ApiEnvelope {
    pub fn into_result(self) -> crate::Result<Value> {
        if let Some(err) = self.error {
            return Err(crate::Error::Command(format_chat_error(&err)));
        }
        self.result
            .ok_or_else(|| crate::Error::Command("missing result and error in API envelope".into()))
    }
}

/// Turn SimpleX error JSON into a short, human-readable line.
fn format_chat_error(err: &Value) -> String {
    let ty = err
        .pointer("/errorType/type")
        .or_else(|| err.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("error");
    match ty {
        "commandError" => err
            .pointer("/errorType/message")
            .or_else(|| err.get("message"))
            .and_then(|v| v.as_str())
            .map(|m| format!("chat command error: {m}"))
            .unwrap_or_else(|| err.to_string()),
        "userExists" => {
            let name = err
                .pointer("/errorType/contactName")
                .and_then(|v| v.as_str())
                .unwrap_or("user");
            format!("user already exists: {name}")
        }
        _ => {
            if let Some(msg) = err.pointer("/errorType/message").and_then(|v| v.as_str()) {
                format!("{ty}: {msg}")
            } else {
                format!("{ty}: {err}")
            }
        }
    }
}

pub(crate) fn parse_event(value: Value) -> Event {
    let type_name = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    match type_name.as_str() {
        "newChatItems" => Event::NewChatItems(value),
        "chatItemsStatusesUpdated" => Event::ChatItemsStatusesUpdated(value),
        "chatStarted" => Event::ChatStarted,
        "contactConnected" => Event::ContactConnected(value),
        "receivedContactRequest" => Event::ReceivedContactRequest(value),
        _ => Event::Other {
            type_name,
            body: value,
        },
    }
}

pub(crate) fn parse_conn_link(value: &Value, full_path: &str, short_path: &str) -> Option<ConnLink> {
    let full = value
        .pointer(full_path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;
    let short = value
        .pointer(short_path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let display = short.clone().unwrap_or_else(|| full.clone());
    Some(ConnLink {
        display,
        full,
        short,
    })
}
