//! Cursus Linux desktop client (Iced).
//!
//! Responsive shell + SimpleX session wiring.

mod avatar_cache;
mod desktop;
mod hover;
mod icons;
mod kinetic;
mod layout;
mod media;
mod theme;
mod touch_mouse;
mod ui;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use iced::futures::{sink::SinkExt, stream};
use iced::font::Font;
use iced::widget::container;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::text_editor;
use iced::{Element, Length, Subscription, Task, Theme};
use tracing_subscriber::EnvFilter;

use cursus_core::{
    auto_receive_candidates, data_dir, files_dir, group_role_rank, is_channel_short_link,
    load_account, merge_seeded_channels, prune_resolved_seeds, save_account, setup_complete,
    AccountMeta, CallInfo, CallMedia, ChatId, ChatMessage, ChatPreview, ChatSettings, ChatTag,
    ChatType, ConnLink, DeliveryStatus, Event, FileTransferState, GroupMemberInfo,
    GroupProfileUpdate, ProfileFields, Session, SessionConfig, UserProfile, GROUP_ROLES,
};

use crate::layout::{resolve, Density, LayoutMode};
use crate::ui::{shell, NavSection};

/// Hold duration for chat / bubble context menus (touch).
const LONG_PRESS_MS: u64 = 520;
/// Finger movement beyond this cancels long-press and treats the gesture as a scroll.
const PRESS_MOVE_SLOP: f32 = 14.0;

/// Shared with the event subscription so routine mouse moves do **not** publish
/// App Messages (each Message rebuilds the whole view — deadly in group chats).
///
/// Process static: `iced::event::listen_with` only accepts `fn` pointers (no captures).
struct PointerGate {
    last: Mutex<(f32, f32)>,
    anchor: Mutex<Option<(f32, f32)>>,
    moved: AtomicBool,
    hover_ok: AtomicBool,
    /// True between FingerPressed and FingerLifted (touch scroll session).
    touch_down: AtomicBool,
    /// Last gesture involved a finger (so release handlers honor `moved`).
    touch_gesture: AtomicBool,
    /// Subscription published `PointerPressed` (long-press timer may be live).
    long_press_msg: AtomicBool,
}

impl PointerGate {
    const fn new() -> Self {
        Self {
            last: Mutex::new((0.0, 0.0)),
            anchor: Mutex::new(None),
            moved: AtomicBool::new(false),
            hover_ok: AtomicBool::new(true),
            touch_down: AtomicBool::new(false),
            touch_gesture: AtomicBool::new(false),
            long_press_msg: AtomicBool::new(false),
        }
    }

    fn last_xy(&self) -> (f32, f32) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_last(&self, x: f32, y: f32) {
        if let Ok(mut g) = self.last.lock() {
            *g = (x, y);
        }
    }

    fn begin_press(&self, x: f32, y: f32) {
        self.set_last(x, y);
        if let Ok(mut a) = self.anchor.lock() {
            *a = Some((x, y));
        }
        self.moved.store(false, Ordering::Relaxed);
        self.touch_down.store(true, Ordering::Relaxed);
        self.touch_gesture.store(true, Ordering::Relaxed);
    }

    fn clear_press(&self) {
        if let Ok(mut a) = self.anchor.lock() {
            *a = None;
        }
        self.touch_down.store(false, Ordering::Relaxed);
    }

    fn touch_ended(&self) {
        self.hover_ok.store(false, Ordering::Relaxed);
        self.clear_press();
        // Keep `moved` + `touch_gesture` until release handlers in this frame
        // read `tap_ok()`, then the next `begin_press` / `reset_gesture` clears them.
    }

    /// Mouse click — not a touch gesture; don't inherit the last scroll's `moved`.
    fn mouse_click_reset(&self) {
        self.reset_gesture();
        self.hover_ok.store(true, Ordering::Relaxed);
    }

    /// Drop sticky scroll/tap state (Back, successful open, etc.).
    fn reset_gesture(&self) {
        self.moved.store(false, Ordering::Relaxed);
        self.touch_gesture.store(false, Ordering::Relaxed);
        self.touch_down.store(false, Ordering::Relaxed);
        self.long_press_msg.store(false, Ordering::Relaxed);
        if let Ok(mut a) = self.anchor.lock() {
            *a = None;
        }
    }

    fn tap_ok(&self) -> bool {
        if self.touch_gesture.load(Ordering::Relaxed) {
            !self.moved.load(Ordering::Relaxed)
        } else {
            true
        }
    }

    /// Update tracking; return a Message only when App must react.
    fn filter_move(&self, x: f32, y: f32) -> Option<Message> {
        self.set_last(x, y);
        let mut publish = false;
        if !self.hover_ok.swap(true, Ordering::Relaxed) {
            publish = true;
        }
        let anchor = self.anchor.lock().ok().and_then(|a| *a);
        if let Some((ax, ay)) = anchor {
            if !self.moved.load(Ordering::Relaxed) {
                let dx = x - ax;
                let dy = y - ay;
                if dx * dx + dy * dy > PRESS_MOVE_SLOP * PRESS_MOVE_SLOP {
                    self.moved.store(true, Ordering::Relaxed);
                    // Wake App only when a long-press timer was armed — otherwise
                    // every scroll past slop rebuilt the whole shell (ARM stall).
                    if self.long_press_msg.load(Ordering::Relaxed) {
                        publish = true;
                    }
                }
            }
        }
        if publish {
            Some(Message::PointerMoved { x, y })
        } else {
            None
        }
    }
}

static POINTER_GATE: PointerGate = PointerGate::new();

// Hover targets for the event subscription (long-press only when a row/bubble
// is hot — otherwise FingerPressed must not rebuild the whole UI, or every
// scroll gesture starts with a one-frame stall on ARM).
// iced delivers window events on the UI thread; HoverSlots uses Rc/Cell (not Sync).
thread_local! {
    static HOVER_FOR_GATE: RefCell<Option<hover::HoverSlots>> = const { RefCell::new(None) };
}

fn hover_target_hot() -> bool {
    // Message bubbles intentionally omitted: menus are double-tap via HoverArea,
    // and treating every bubble press as hot rebuilt the UI on each scroll start.
    HOVER_FOR_GATE.with(|g| {
        g.borrow().as_ref().is_some_and(|slots| {
            slots.chat.get().is_some() || slots.reaction.borrow().is_some()
        })
    })
}

/// Jump-to-latest FAB gate — only publishes when “away” flips, so scroll does not
/// rebuild the chat on every wheel tick (that was the original lag).
struct HistoryAwayGate {
    away: AtomicBool,
}

impl HistoryAwayGate {
    const fn new() -> Self {
        Self {
            away: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        self.away.store(false, Ordering::Relaxed);
    }

    /// Mark scrolled-up; `Some(true)` only on the false→true edge.
    fn mark_away(&self) -> Option<bool> {
        if self.away.swap(true, Ordering::Relaxed) {
            None
        } else {
            Some(true)
        }
    }

    /// Mark at newest end; `Some(false)` only on the true→false edge.
    fn mark_at_end(&self) -> Option<bool> {
        if self.away.swap(false, Ordering::Relaxed) {
            Some(false)
        } else {
            None
        }
    }

    fn set(&self, away: bool) {
        self.away.store(away, Ordering::Relaxed);
    }
}

static HISTORY_AWAY_GATE: HistoryAwayGate = HistoryAwayGate::new();

fn wheel_scrolls_up(delta: iced::mouse::ScrollDelta) -> bool {
    match delta {
        // Positive y = finger/wheel content moves down = view older messages (away from end).
        iced::mouse::ScrollDelta::Lines { y, .. } => y > 0.0,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y > 0.0,
    }
}

fn wheel_scrolls_down(delta: iced::mouse::ScrollDelta) -> bool {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y < 0.0,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y < 0.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMenuAction {
    ViewContact,
    ViewGroup,
    MarkRead,
    MarkUnread,
    ToggleFavorite,
    ToggleMute,
    AddToList,
    Clear,
    Delete,
}

#[derive(Debug, Clone)]
pub enum ChatMenuPane {
    Actions,
    ConfirmClear,
    ConfirmDelete,
    Lists {
        tags: Vec<ChatTag>,
        new_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct ChatContextMenu {
    pub index: usize,
    pub chat_id: ChatId,
    pub title: String,
    pub pane: ChatMenuPane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgMenuAction {
    Reply,
    Copy,
    Edit,
    Forward,
    Info,
    Delete,
    Select,
    /// Reveal or hide original content of a marked-deleted item.
    ToggleReveal,
    /// Save a local media/file attachment via the system Save As dialog.
    Download,
}

#[derive(Debug, Clone)]
pub enum MsgMenuPane {
    Actions,
    ConfirmDelete,
    ForwardPick,
    Info {
        item_id: i64,
        time: Option<String>,
        status: Option<String>,
        text: String,
        /// Local size · transfer status (kept out of the image bubble).
        file_detail: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MessageMenu {
    pub item_id: i64,
    pub pane: MsgMenuPane,
}

#[derive(Debug, Clone)]
pub struct ReactionMembersOverlay {
    pub item_id: i64,
    pub emoji: String,
    pub names: Vec<String>,
    pub loading: bool,
    pub error: Option<String>,
}

/// Editable own-profile draft (Settings → My profile).
#[derive(Debug, Clone)]
pub struct OwnProfileDraft {
    pub fields: ProfileFields,
    pub dirty: bool,
}

/// Contact preference allow level (`FeatureAllowed` in libsimplex).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureAllow {
    No,
    Yes,
    Always,
}

impl FeatureAllow {
    pub fn label(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Yes => "yes",
            Self::Always => "always",
        }
    }

    pub fn is_on(self) -> bool {
        !matches!(self, Self::No)
    }

    /// Cycle `no` → `yes` → `always` → `no`.
    pub fn cycle(self) -> Self {
        match self {
            Self::No => Self::Yes,
            Self::Yes => Self::Always,
            Self::Always => Self::No,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "yes" | "on" | "true" => Self::Yes,
            "always" => Self::Always,
            _ => Self::No,
        }
    }

    pub fn as_api(self) -> &'static str {
        self.label()
    }
}

/// Contact / peer profile overlay (direct chats).
#[derive(Debug, Clone)]
pub struct ContactProfileOverlay {
    pub chat_index: usize,
    pub contact_id: i64,
    pub title: String,
    pub fields: ProfileFields,
    pub alias: String,
    pub alias_dirty: bool,
    pub muted: bool,
    pub favorite: bool,
    /// Disappearing messages — `no` / `yes` / `always`.
    pub timed_allow: FeatureAllow,
    /// Peer allow for disappearing messages (read-only).
    pub timed_contact_allows: FeatureAllow,
    /// TTL seconds when disappearing messages are on (SimpleX `ttl`).
    pub timed_ttl_secs: i64,
    /// Full deletion — `no` / `yes` / `always`.
    pub full_delete_allow: FeatureAllow,
    pub full_delete_contact_allows: FeatureAllow,
    /// Negotiated: deletes can fully remove for us / for them.
    pub full_delete_for_user: bool,
    pub full_delete_for_contact: bool,
    pub reactions_allow: FeatureAllow,
    pub reactions_contact_allows: FeatureAllow,
    pub voice_allow: FeatureAllow,
    pub voice_contact_allows: FeatureAllow,
    pub calls_allow: FeatureAllow,
    pub calls_contact_allows: FeatureAllow,
    /// Raw prefs for merge-on-save.
    pub contact_preferences: Option<serde_json::Value>,
}

/// Create group or channel name dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    Group,
    Channel,
}

#[derive(Debug, Clone)]
pub struct CreateChatOverlay {
    pub kind: CreateKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSettingsPane {
    Overview,
    AddMember,
    Preferences,
    MemberInfo,
    ConfirmLeave,
    ConfirmDelete,
}

/// Member support (“Chat with admins”) overlay for a group.
#[derive(Debug, Clone)]
pub enum SupportPane {
    /// Moderator list of member support threads.
    List,
    /// Scoped support conversation (`member_id = None` ⇒ own chat with admins).
    Thread {
        member_id: Option<i64>,
        title: String,
        /// Return to list on Back when opened as a moderator.
        from_list: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SupportOverlay {
    pub group_id: i64,
    pub pane: SupportPane,
    pub members: Vec<GroupMemberInfo>,
    pub members_loading: bool,
}

/// Group / channel settings page state.
#[derive(Debug, Clone)]
pub struct GroupSettingsOverlay {
    pub group_id: i64,
    pub title: String,
    pub name: String,
    pub full_name: String,
    /// Short bio under the name (`shortDescr`).
    pub short_descr: String,
    /// Welcome message for new members (`description` in protocol JSON).
    pub welcome_message: String,
    /// Profile image data-URL (`data:image/…`), same budget as own profile.
    pub image: Option<String>,
    pub is_channel: bool,
    pub members: Vec<GroupMemberInfo>,
    pub members_loading: bool,
    pub link: Option<String>,
    /// Default role granted to link joiners.
    pub link_member_role: String,
    /// Role used when inviting a contact from AddMember.
    pub invite_role: String,
    pub my_role: String,
    pub alias: String,
    pub alias_dirty: bool,
    pub muted: bool,
    pub favorite: bool,
    pub group_preferences: Option<serde_json::Value>,
    pub member_admission: Option<serde_json::Value>,
    pub public_group: Option<serde_json::Value>,
    pub selected_member_id: Option<i64>,
    pub pane: GroupSettingsPane,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct ReplyTarget {
    pub item_id: i64,
    pub preview: String,
}

/// Composer is rewriting an existing outgoing item instead of sending new.
#[derive(Debug, Clone)]
pub struct EditTarget {
    pub item_id: i64,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct ItemInfoData {
    pub item_id: i64,
    pub time: Option<String>,
    pub status: Option<String>,
    pub text: String,
}

/// Pre-session gate: welcome (first run) or unlock (SQLCipher password).
#[derive(Debug, Clone)]
pub enum AuthScreen {
    WelcomeName {
        name: String,
    },
    WelcomePassword {
        name: String,
        password: String,
        confirm: String,
    },
    Unlock {
        display_name: String,
        password: String,
    },
}

#[derive(Debug, Clone)]
struct ConnectRequest {
    password: String,
    display_name: String,
    encrypt_local_files: bool,
    /// Persist `account.json` after a successful open (welcome only).
    save_account: bool,
}

fn init_file_logging() {
    let log_path = data_dir().join("cursus.log");
    let _ = std::fs::create_dir_all(data_dir());
    let env = EnvFilter::from_default_env().add_directive("cursus=info".parse().unwrap());
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(env)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
            tracing::info!(path = %log_path.display(), "logging to file");
        }
        Err(err) => {
            tracing_subscriber::fmt()
                .with_env_filter(env)
                .with_writer(std::io::stderr)
                .init();
            eprintln!("cursus: could not open {}: {err}", log_path.display());
        }
    }
}

pub fn main() -> iced::Result {
    // Behind `--features musl-glibc-compat`: re-exec with LD_PRELOAD shim.
    #[cfg(feature = "musl-glibc-compat")]
    cursus_core::ensure_ld_preload_reexec();

    init_file_logging();

    // Prefer Noto Sans as UI font (no dingbat ❤) and register color emoji for
    // Advanced shaping fallback — otherwise hearts render as ugly text glyphs.
    //
    // `daemon` (not `application`) so hide-to-tray can close the window on
    // Wayland — Mode::Hidden / set_visible(false) is a no-op there — and keep
    // the process alive until Quit.
    let mut app = iced::daemon(App::new, App::update, App::view)
        .title(App::title)
        .theme(|app: &App, _| app.iced_theme())
        .default_font(Font::with_name("Noto Sans"))
        .subscription(App::subscription);
    for path in [
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/NotoColorEmoji/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            tracing::info!(path, "loaded emoji font");
            app = app.font(bytes);
            break;
        }
    }

    app.run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(NavSection),
    Back,
    /// Logical client-area size from Iced (not framebuffer pixels).
    WindowResized(iced::Size),
    /// DPI scale factor; diagnostics only — never used for breakpoints.
    ScaleFactor(f32),
    Connect,
    Connected(Result<String, String>),
    SelectChat(usize),
    /// Multi-line composer (`text_editor`) edit action.
    ComposerEdit(text_editor::Action),
    /// Insert a string into the composer (OSK / multi-codepoint text).
    ComposerInsert(String),
    /// Accept highlighted / clicked emoji shortcode suggestion.
    EmojiSuggestAccept,
    EmojiSuggestSelect(usize),
    EmojiSuggestNext,
    EmojiSuggestPrev,
    Send,
    Sent(Result<(), String>),
    Edited(Result<(), String>),
    AttachFile,
    FileAttached(Result<(), String>),
    ReceiveFile { file_id: i64, file_name: String },
    FileReceived(Result<String, String>),
    /// Open a saved attachment with the OS default handler.
    OpenLocalFile(PathBuf),
    /// Ask before opening a URI from rich-text (http / simplex / mailto / tel).
    OpenUrl(String),
    ConfirmOpenUrl,
    CancelOpenUrl,
    /// Add or remove a reaction (`emoji` is a protocol char / display form).
    ToggleReaction { item_id: i64, emoji: String },
    ReactionDone(Result<(), String>),
    /// Group only: show who reacted with this emoji.
    ShowReactionMembers { item_id: i64, emoji: String },
    ReactionMembersLoaded {
        item_id: i64,
        emoji: String,
        result: Result<Vec<String>, String>,
    },
    CloseReactionMembers,
    ReactionChipLongPress {
        item_id: i64,
        emoji: String,
        generation: u64,
    },
    OpenMessageMenu(i64),
    CloseMessageMenu,
    MsgMenuAction(MsgMenuAction),
    MsgMenuConfirmDelete,
    MsgMenuCancelConfirm,
    MsgBubbleReleased(i64),
    MsgBubbleLongPress { item_id: i64, generation: u64 },
    CancelReply,
    CancelEdit,
    /// ArrowUp (empty composer) — edit the latest editable outgoing message.
    EditLastOutgoing,
    /// Jump the history viewport to a quoted parent item.
    JumpToQuoted(i64),
    /// Clear the temporary focus ring around a jumped-to message.
    ClearFocusItem { generation: u64 },
    /// Snap chat history to the newest message.
    ScrollToLatest,
    /// User scrolled up in history — show jump-to-latest (edge-triggered).
    HistoryAwayChanged(bool),
    /// Reveal / hide original content of a marked-deleted item.
    ToggleRevealDeleted(i64),
    /// Reveal / hide a `#secret#` rich-text span.
    ToggleSecretReveal { item_id: i64, index: usize },
    /// Desktop notification preference toggles (Settings).
    ToggleDesktopNotifications,
    ToggleTray,
    ToggleCloseToTray,
    /// Cycle System → Light → Dark appearance.
    CycleThemePreference,
    /// Cycle Small → Default → Large → Extra large UI text.
    CycleTextSize,
    /// First-run / unlock gate.
    WelcomeNameChanged(String),
    WelcomeNameNext,
    WelcomePasswordChanged(String),
    WelcomeConfirmChanged(String),
    WelcomeBack,
    WelcomeSubmit,
    UnlockPasswordChanged(String),
    UnlockSubmit,
    /// Decrypt CryptoFile → `tmp/plain` for preview / open.
    DecryptFileDone {
        encrypted: PathBuf,
        result: Result<PathBuf, String>,
    },
    /// Sync address auto-accept with libsimplex.
    ToggleAddressAutoAccept,
    AddressAutoAcceptSynced(Result<bool, String>),
    /// Post-unlock SMP activate / resubscribe finished (or failed non-fatally).
    NetworkActivated(Result<(), String>),
    /// Own profile editor (Settings).
    OwnProfileDisplayName(String),
    OwnProfileBio(String),
    OwnProfileDescription(String),
    PickOwnProfileImage,
    OwnProfileImagePicked(Result<String, String>),
    ClearOwnProfileImage,
    SaveOwnProfile,
    OwnProfileSaved(Result<UserProfile, String>),
    ToggleProfileAddress,
    ProfileAddressToggled(Result<UserProfile, String>),
    /// Contact profile overlay.
    OpenContactProfile,
    CloseContactProfile,
    CloseContactProfileAfterSave,
    ContactAliasChanged(String),
    SaveContactAlias,
    ContactAliasSaved(Result<(), String>),
    ContactProfileMute,
    ContactProfileFavorite,
    ContactProfileClear,
    ContactProfileDelete,
    /// Direct-chat feature prefs (contact Chat features) — cycle no/yes/always.
    CycleContactTimedMessages,
    SetContactTimedTtl(i64),
    CycleContactFullDelete,
    CycleContactReactions,
    CycleContactVoice,
    CycleContactCalls,
    ContactPrefsSaved(Result<(), String>),
    /// Create group / channel.
    OpenCreateGroup,
    OpenCreateChannel,
    CloseCreateChat,
    CreateChatNameChanged(String),
    ConfirmCreateChat,
    /// `(kind, chat_id, invite_link, title)` — title is needed to seed channels
    /// into the list before `creating_in_progress` clears on the core side.
    CreateChatDone(Result<(CreateKind, ChatId, Option<String>, String), String>),
    /// Group settings + members / prefs / link.
    OpenGroupSettings,
    CloseGroupSettings,
    CloseGroupSettingsAfterSave,
    /// Member support (“Chat with admins”).
    OpenSupportChatList,
    OpenUserSupportChat,
    OpenMemberSupportChat { member_id: i64, title: String },
    CloseSupportOverlay,
    SupportMembersLoaded(Result<Vec<GroupMemberInfo>, String>),
    DeleteMemberSupportChat(i64),
    MemberSupportChatDeleted(Result<(), String>),
    GroupSettingsNameChanged(String),
    GroupSettingsShortDescrChanged(String),
    GroupSettingsWelcomeChanged(String),
    GroupSettingsAliasChanged(String),
    SaveGroupAlias,
    GroupAliasSaved(Result<(), String>),
    /// Accept / reject a group invite card in a direct chat (`/_join` / `/_leave`).
    AcceptGroupInvitation(i64),
    RejectGroupInvitation(i64),
    GroupInvitationHandled(Result<String, String>),
    GroupSettingsMute,
    GroupSettingsFavorite,
    PickGroupProfileImage,
    GroupProfileImagePicked(Result<String, String>),
    ClearGroupProfileImage,
    SaveGroupProfile,
    GroupProfileSaved(Result<(), String>),
    GroupMembersLoaded(Result<Vec<GroupMemberInfo>, String>),
    OpenAddGroupMember,
    CloseAddGroupMember,
    GroupInviteRoleCycle,
    AddGroupMember(i64),
    GroupMemberAdded(Result<(), String>),
    OpenGroupMember(i64),
    CloseGroupMember,
    GroupMemberSetRole(String),
    GroupMemberRoleDone(Result<(), String>),
    GroupMemberRemove { with_messages: bool },
    GroupMemberRemoved(Result<(), String>),
    GroupMemberToggleBlockSelf,
    GroupMemberBlockSelfDone(Result<(), String>),
    GroupMemberToggleBlockAll,
    GroupMemberBlockAllDone(Result<(), String>),
    GroupMemberAccept,
    GroupMemberAccepted(Result<(), String>),
    OpenGroupPreferences,
    CloseGroupPreferences,
    GroupPrefToggle(&'static str),
    /// Set disappearing-message TTL for the group (when timedMessages is on).
    SetGroupTimedTtl(i64),
    GroupAdmissionToggle,
    SaveGroupPreferences,
    EnsureGroupLink,
    GroupLinkReady(Result<String, String>),
    CopyGroupLink,
    GroupLinkRoleCycle,
    GroupLinkRoleSet(Result<(), String>),
    DeleteGroupLink,
    GroupLinkDeleted(Result<(), String>),
    AddGroupShortLink,
    GroupShortLinkReady(Result<String, String>),
    OpenMemberDirectChat,
    OpenLeaveGroupConfirm,
    ConfirmLeaveGroup,
    GroupLeft(Result<(), String>),
    OpenDeleteGroupConfirm,
    ConfirmDeleteGroup,
    GroupDeleted(Result<String, String>),
    ClearGroupChat,
    GroupChatCleared(Result<String, String>),
    /// Calls (signaling via `/_call …`; media uses peer WebRTC when available).
    StartAudioCall(i64),
    AcceptIncomingCall,
    RejectIncomingCall,
    EndActiveCall,
    CallActionDone(Result<String, String>),
    WindowFocused(bool),
    WindowCloseRequested,
    /// Main window finished opening (daemon / re-show from tray).
    MainWindowOpened(iced::window::Id),
    TrayActions(Vec<desktop::TrayAction>),
    ShowFromTray,
    HideToTray,
    QuitApp,
    ForwardToChat(usize),
    ForwardDone(Result<(), String>),
    DeleteItemsDone(Result<(), String>),
    ItemInfoLoaded(Result<ItemInfoData, String>),
    ClearSelection,
    DeleteSelected,
    /// Start/stop in-composer voice recording (ffmpeg → m4a draft).
    ToggleVoiceRecord,
    /// Recording stopped — review before send.
    VoiceDraftReady(Result<VoiceDraft, String>),
    SendVoiceDraft,
    CancelVoiceDraft,
    VoiceSent(Result<(), String>),
    PlayVoice { file_id: i64, path: PathBuf, duration_secs: u32 },
    StopVoicePlayback,
    VoiceTick,
    /// GTK-style inertial fling after a touch scroll.
    ScrollFlingStart { id: String, velocity_y: f32 },
    ScrollFlingCancel { id: String },
    ScrollFlingTick,
    WaveformReady { path: PathBuf, peaks: Result<Vec<f32>, String> },
    /// Async video poster for chat-bubble previews.
    VideoThumbReady { path: PathBuf, thumb: Option<PathBuf> },
    /// Deferred media warm-up after the chat view has painted once.
    WarmChatMedia,
    ImagePreviewReady {
        path: PathBuf,
        preview: Option<(PathBuf, u32, u32)>,
    },
    MediaSaved(Result<String, String>),
    RefreshChats,
    /// Coalesced chat-list refresh after high-frequency core events.
    RefreshChatsDebounced(u64),
    ChatsLoaded(Result<Vec<ChatPreview>, String>),
    /// `gen` discards stale loads when the user switches chats quickly.
    MessagesLoaded {
        gen: u64,
        result: Result<Vec<ChatMessage>, String>,
    },
    /// Second-phase media warm (posters) after decrypt/waveforms start.
    WarmChatVisuals,
    ChatMarkedRead(Result<ChatId, String>),
    AcceptContactRequest(i64),
    RejectContactRequest(i64),
    ContactRequestHandled {
        req_id: i64,
        result: Result<String, String>,
    },
    InviteInputChanged(String),
    CreateInvitation,
    EnsureAddress,
    AcceptInvite,
    InviteReady(Result<ConnLink, String>),
    InviteAccepted(Result<String, String>),
    CopyInviteLink,
    OpenCompose,
    CloseCompose,
    CoreEvent(Event),
    /// Pointer moved (mouse or touch) — cancels long-press when dragged.
    PointerMoved { x: f32, y: f32 },
    /// Touch / left-button down — may start long-press on hovered row/bubble.
    PointerPressed { x: f32, y: f32 },
    /// Touch finger up — suppress sticky hover (iced keeps cursor on last touch).
    PointerTouchEnded,
    /// Short tap on a chat row (only if the press did not become a scroll).
    ChatRowReleased(usize),
    /// Fired after LONG_PRESS_MS if the finger/button is still down.
    ChatRowLongPress { index: usize, generation: u64 },
    OpenChatMenu(usize),
    CloseChatMenu,
    ChatMenuAction(ChatMenuAction),
    ChatMenuDoClear,
    ChatMenuDoDelete,
    ChatMenuCancelConfirm,
    ChatMenuOpenLists(Result<Vec<ChatTag>, String>),
    ChatMenuNewListChanged(String),
    ChatMenuCreateList,
    ChatMenuToggleTag(i64),
    ChatMenuTagsUpdated(Result<String, String>),
    ChatMenuDone(Result<String, String>),
    /// Esc — close media viewer first, else chat menu.
    Escape,
}

struct App {
    nav: NavSection,
    /// Logical window width (DIP).
    width: f32,
    /// Logical window height (DIP).
    height: f32,
    /// Window scale factor (physical = logical * scale). Not used for layout.
    scale_factor: f32,
    status: String,
    session: Option<Arc<Session>>,
    /// Handoff slot so `Session` never has to live inside a Clone `Message`.
    pending_session: Arc<Mutex<Option<Arc<Session>>>>,
    /// `None` once SQLCipher is unlocked and the session is live.
    auth: Option<AuthScreen>,
    chats: Vec<ChatPreview>,
    /// Group ids for channels seeded locally while core hides `creating_in_progress`.
    /// Cleared when core lists the channel or the user deletes it (prevents ghosts).
    seeded_channel_ids: HashSet<i64>,
    /// Chat being deleted — used to drop the row if core already removed it (`groupNotFound`).
    pending_delete_id: Option<ChatId>,
    selected: Option<usize>,
    /// Stable id so selection survives chat list refresh / resize.
    selected_id: Option<ChatId>,
    messages: Vec<ChatMessage>,
    /// Encrypted attachment path → decrypted cache under `tmp/plain/`.
    plaintext_files: HashMap<PathBuf, PathBuf>,
    composer: text_editor::Content,
    connecting: bool,
    invite_input: String,
    invite_link: Option<String>,
    /// Full-page compose menu (invite / address / new group / channel).
    compose_open: bool,
    /// When false, `Hovered` button styles match idle (fixes sticky touch hover).
    hover_ok: bool,
    /// True while Accept/Reject is in flight — blocks double-submit.
    contact_request_busy: bool,
    /// Request ids already accepted/rejected this session. Core can keep
    /// `contactRequestId` with `connStatus=new` after Accept, which would
    /// otherwise resurrect the Accept bar on the next chat-list refresh.
    handled_contact_requests: HashSet<i64>,
    chat_menu: Option<ChatContextMenu>,
    /// Bumped to cancel an in-flight long-press timer.
    long_press_generation: u64,
    long_press_index: Option<usize>,
    /// Hover targets for long-press (updated by widgets without App Messages).
    hover: hover::HoverSlots,
    /// Screen position where the active press began.
    press_anchor: Option<(f32, f32)>,
    /// True once the press moved past [`PRESS_MOVE_SLOP`] (scroll / drag).
    press_moved: bool,
    /// Last known pointer position (for mouse ButtonPressed without coordinates).
    last_pointer: (f32, f32),
    /// File ids already auto-/manually receiving this session (dedupe freceive).
    receiving_files: HashSet<i64>,
    /// Auto-receive queue deferred until after the first chat paint.
    pending_auto_receives: Vec<(i64, String)>,
    /// Active ffmpeg voice capture, if any.
    voice_rec: Option<VoiceRec>,
    /// Recorded voice waiting for send/cancel.
    voice_draft: Option<VoiceDraft>,
    /// In-bubble voice playback.
    voice_playback: Option<VoicePlayback>,
    /// Cached waveform peaks keyed by absolute path.
    waveforms: std::collections::HashMap<PathBuf, Vec<f32>>,
    /// Cached video poster frames keyed by absolute media path.
    video_thumbs: std::collections::HashMap<PathBuf, PathBuf>,
    /// In-flight video thumb extractions (avoid duplicate ffmpeg jobs).
    video_thumbs_pending: HashSet<PathBuf>,
    /// Cached rounded still-image previews (UI thread must not decode).
    image_previews: std::collections::HashMap<PathBuf, (PathBuf, u32, u32)>,
    image_previews_pending: HashSet<PathBuf>,
    /// After opening a chat, pin history to the newest message once it loads.
    pin_history_end: bool,
    /// Link waiting for “Open link?” confirmation.
    pending_url: Option<String>,
    /// Highlighted row in the `:shortcode` autocomplete list.
    emoji_suggest_index: usize,
    /// Per-message context menu (reactions + actions).
    message_menu: Option<MessageMenu>,
    /// Long-press tracking for message bubbles.
    msg_long_press_generation: u64,
    msg_long_press_item: Option<i64>,
    /// Composer reply target.
    reply_to: Option<ReplyTarget>,
    editing: Option<EditTarget>,
    /// Multi-select mode for messages.
    selection_mode: bool,
    selected_items: HashSet<i64>,
    /// Marked-deleted items the user has revealed (show original text).
    revealed_deleted: HashSet<i64>,
    /// Revealed `#secret#` spans: item_id → segment indices.
    revealed_secrets: HashMap<i64, HashSet<usize>>,
    /// Group reaction member list overlay.
    reaction_members: Option<ReactionMembersOverlay>,
    /// Long-press tracking for reaction chips (groups → members list).
    reaction_chip_press_generation: u64,
    reaction_chip_press: Option<(i64, String)>,
    /// Temporary highlight after JumpToQuoted.
    focus_item: Option<i64>,
    focus_item_generation: u64,
    /// True when the history scrollable is away from the newest messages.
    history_away_from_end: bool,
    /// Linux desktop notification / tray preferences.
    desktop_prefs: desktop::DesktopPrefs,
    tray: Option<desktop::AppTray>,
    /// Last known window focus (for suppressing notifications while reading).
    window_focused: bool,
    /// Open UI window id; `None` while hidden to tray (daemon keeps running).
    main_window: Option<iced::window::Id>,
    /// Ensures libsimplex is loaded only after the first GPU window is up.
    core_connect_launched: bool,
    /// Settings → My profile draft.
    own_profile: OwnProfileDraft,
    /// Active inertial fling after a touch scroll (GTK-style kinetic feel).
    scroll_fling: Option<ScrollFling>,
    /// Bumps on each message history request; stale `MessagesLoaded` are ignored.
    messages_load_gen: u64,
    /// Bumps on each scheduled chat-list refresh; only the latest debounce fires.
    chats_refresh_gen: u64,
    /// Direct-contact profile overlay.
    contact_profile: Option<ContactProfileOverlay>,
    /// Create group / channel dialog.
    create_chat: Option<CreateChatOverlay>,
    /// Group / channel settings overlay.
    group_settings: Option<GroupSettingsOverlay>,
    /// Member support chat overlay (chat with admins).
    support: Option<SupportOverlay>,
    /// Outgoing or accepted active call (signaling).
    active_call: Option<CallInfo>,
    /// Ringing inbound call waiting for accept/reject.
    incoming_call: Option<CallInfo>,
}

struct VoiceRec {
    path: PathBuf,
    child: Child,
    started: Instant,
}

#[derive(Debug, Clone)]
pub struct VoiceDraft {
    pub path: PathBuf,
    pub duration_secs: u32,
    pub peaks: Vec<f32>,
}

struct VoicePlayback {
    file_id: i64,
    #[allow(dead_code)]
    path: PathBuf,
    duration_secs: u32,
    started: Instant,
    child: Child,
}

/// Inertial scroll after finger-up (GTK ScrolledWindow kinetic equivalent).
struct ScrollFling {
    id: String,
    /// Pixels per second along iced AbsoluteOffset.y.
    velocity_y: f32,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut prefs = desktop::load_prefs();
        let tray = if prefs.tray {
            desktop::AppTray::try_new()
        } else {
            None
        };
        if prefs.tray && tray.is_none() {
            prefs.tray = false;
            prefs.close_to_tray = false;
            desktop::save_prefs(&prefs);
        }
        theme::set_light_active(prefs.theme.resolve_light());
        let pending_session = Arc::new(Mutex::new(None));
        let (main_window, open_window) = open_main_window();
        let auth = if setup_complete() {
            let name = load_account()
                .map(|a| a.display_name)
                .unwrap_or_else(|| "Cursus".into());
            Some(AuthScreen::Unlock {
                display_name: name,
                password: String::new(),
            })
        } else {
            Some(AuthScreen::WelcomeName {
                name: String::new(),
            })
        };
        let status = if setup_complete() {
            "Enter your database password.".into()
        } else {
            "Welcome — set your name to begin.".into()
        };
        (
            Self {
                nav: NavSection::Chats,
                width: 1180.0,
                height: 720.0,
                scale_factor: 1.0,
                status,
                session: None,
                pending_session: Arc::clone(&pending_session),
                auth,
                chats: Vec::new(),
                seeded_channel_ids: HashSet::new(),
                pending_delete_id: None,
                selected: None,
                selected_id: None,
                messages: Vec::new(),
                plaintext_files: HashMap::new(),
                composer: text_editor::Content::new(),
                connecting: false,
                invite_input: String::new(),
                invite_link: None,
                compose_open: false,
                hover_ok: true,
                contact_request_busy: false,
                handled_contact_requests: HashSet::new(),
                chat_menu: None,
                long_press_generation: 0,
                long_press_index: None,
                hover: {
                    let slots = hover::HoverSlots::default();
                    HOVER_FOR_GATE.with(|g| {
                        *g.borrow_mut() = Some(slots.clone());
                    });
                    slots
                },
                press_anchor: None,
                press_moved: false,
                last_pointer: (0.0, 0.0),
                receiving_files: HashSet::new(),
                pending_auto_receives: Vec::new(),
                voice_rec: None,
                voice_draft: None,
                voice_playback: None,
                waveforms: std::collections::HashMap::new(),
                video_thumbs: std::collections::HashMap::new(),
                video_thumbs_pending: HashSet::new(),
                image_previews: HashMap::new(),
                image_previews_pending: HashSet::new(),
                pin_history_end: false,
                messages_load_gen: 0,
                chats_refresh_gen: 0,
                pending_url: None,
                emoji_suggest_index: 0,
                message_menu: None,
                msg_long_press_generation: 0,
                msg_long_press_item: None,
                reply_to: None,
                editing: None,
                selection_mode: false,
                selected_items: HashSet::new(),
                revealed_deleted: HashSet::new(),
                revealed_secrets: HashMap::new(),
                reaction_members: None,
                reaction_chip_press_generation: 0,
                reaction_chip_press: None,
                focus_item: None,
                focus_item_generation: 0,
                history_away_from_end: false,
                desktop_prefs: prefs,
                tray,
                window_focused: true,
                main_window: Some(main_window),
                core_connect_launched: false,
                own_profile: OwnProfileDraft {
                    fields: ProfileFields {
                        display_name: "Cursus".into(),
                        ..ProfileFields::default()
                    },
                    dirty: false,
                },
                scroll_fling: None,
                contact_profile: None,
                create_chat: None,
                group_settings: None,
                support: None,
                active_call: None,
                incoming_call: None,
            },
            // Daemon runs App::new tasks before the wgpu compositor boots.
            // Loading libsimplex that early exports Haskell ZSTD_* symbols that
            // collide with Mesa/Vulkan's libzstd → SIGSEGV. Open the window
            // first; connect from MainWindowOpened (same ordering as iced::application).
            open_window,
        )
    }

    fn title(&self, _window: iced::window::Id) -> String {
        "Cursus".into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let window = iced::event::listen_with(|event, _status, _id| match event {
            iced::Event::Window(iced::window::Event::Resized(size)) => {
                Some(Message::WindowResized(size))
            }
            iced::Event::Window(iced::window::Event::Opened { size, .. }) => {
                Some(Message::WindowResized(size))
            }
            iced::Event::Window(iced::window::Event::Focused) => {
                Some(Message::WindowFocused(true))
            }
            iced::Event::Window(iced::window::Event::Unfocused) => {
                Some(Message::WindowFocused(false))
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => {
                Some(Message::WindowCloseRequested)
            }
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                POINTER_GATE.filter_move(position.x, position.y)
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                // Mouse clicks must not inherit `moved` from a prior touch scroll.
                POINTER_GATE.mouse_click_reset();
                None
            }
            // Long-press is touch-only; mouse uses right-click for menus so
            // left-clicks on buttons/composer are never mistaken for holds.
            // Critical: only publish PointerPressed when a hover target is hot.
            // Publishing on every finger-down rebuilds the whole view and is
            // the “tap vs swipe” stall on ARM scrollables.
            iced::Event::Touch(iced::touch::Event::FingerPressed { position, .. }) => {
                POINTER_GATE.begin_press(position.x, position.y);
                if hover_target_hot() {
                    POINTER_GATE
                        .long_press_msg
                        .store(true, Ordering::Relaxed);
                    Some(Message::PointerPressed {
                        x: position.x,
                        y: position.y,
                    })
                } else {
                    POINTER_GATE
                        .long_press_msg
                        .store(false, Ordering::Relaxed);
                    None
                }
            }
            iced::Event::Touch(iced::touch::Event::FingerMoved { position, .. }) => {
                let prev = POINTER_GATE.last_xy();
                let moved_msg = POINTER_GATE.filter_move(position.x, position.y);
                // History FAB edge flips without requiring PointerMoved (which we
                // no longer publish on ordinary scrolls).
                if POINTER_GATE.touch_down.load(Ordering::Relaxed)
                    && POINTER_GATE.moved.load(Ordering::Relaxed)
                {
                    let dy = position.y - prev.1;
                    if dy > 2.0 {
                        if let Some(away) = HISTORY_AWAY_GATE.mark_away() {
                            return Some(Message::HistoryAwayChanged(away));
                        }
                    } else if dy < -2.0 {
                        if let Some(away) = HISTORY_AWAY_GATE.mark_at_end() {
                            return Some(Message::HistoryAwayChanged(away));
                        }
                    }
                }
                moved_msg
            }
            iced::Event::Touch(iced::touch::Event::FingerLifted { .. })
            | iced::Event::Touch(iced::touch::Event::FingerLost { .. }) => {
                let armed = POINTER_GATE.long_press_msg.swap(false, Ordering::Relaxed);
                POINTER_GATE.touch_ended();
                // Only wake App when a long-press press was published — otherwise
                // every finger-up rebuilt the shell after a plain scroll.
                if armed {
                    Some(Message::PointerTouchEnded)
                } else {
                    None
                }
            }
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) => {
                use iced::keyboard::key::Named;
                match key {
                    // Handlers no-op when the matching overlay is closed.
                    // Emoji suggest Tab/Shift+Tab/Enter are handled in the
                    // composer key_binding so they don't fight the editor.
                    iced::keyboard::Key::Named(Named::Escape) => Some(Message::Escape),
                    _ => None,
                }
            }
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                if wheel_scrolls_up(delta) {
                    HISTORY_AWAY_GATE
                        .mark_away()
                        .map(Message::HistoryAwayChanged)
                } else if wheel_scrolls_down(delta) {
                    HISTORY_AWAY_GATE
                        .mark_at_end()
                        .map(Message::HistoryAwayChanged)
                } else {
                    None
                }
            }
            _ => None,
        });

        let events = match self.session.clone() {
            Some(session) => {
                // Restart only when the session Arc identity changes.
                Subscription::run_with(SessionSub(session), core_event_stream)
            }
            None => Subscription::none(),
        };

        // 100ms is enough for the playhead; 50ms forced a full chat view rebuild
        // twenty times a second on ARM.
        let voice_tick = if self.voice_playback.is_some() {
            iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::VoiceTick)
        } else {
            Subscription::none()
        };

        let fling_tick = if self.scroll_fling.is_some() {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::ScrollFlingTick)
        } else {
            Subscription::none()
        };

        let tray_tick = if let Some(tray) = self.tray.as_ref() {
            let show = tray.show_id.clone();
            let hide = tray.hide_id.clone();
            let quit = tray.quit_id.clone();
            Subscription::run_with(
                (show, hide, quit),
                tray_action_stream,
            )
        } else {
            Subscription::none()
        };

        Subscription::batch([window, events, voice_tick, fling_tick, tray_tick])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(section) => {
                // Commit dirty settings/profile drafts when leaving Settings.
                let leaving_settings =
                    self.nav == NavSection::Settings && section != NavSection::Settings;
                self.nav = section;
                self.compose_open = false;
                // Keep an open conversation when moving Chats ↔ Channels if it
                // still belongs in the destination; otherwise clear.
                let keep = self.selected.and_then(|i| self.chats.get(i)).is_some_and(|c| {
                    match section {
                        NavSection::Chats => !c.is_channel,
                        NavSection::Channels => c.is_channel,
                        _ => false,
                    }
                });
                if !keep {
                    self.selected = None;
                    self.selected_id = None;
                    self.messages.clear();
                }
                if leaving_settings && self.own_profile.dirty {
                    return self.update(Message::SaveOwnProfile);
                }
                Task::none()
            }
            Message::Back => {
                // Back is often a button press after scrolling the chat — clear
                // sticky press_moved or the chat list cannot open another row.
                self.clear_pointer_gesture();
                if let Some(support) = self.support.as_ref() {
                    match &support.pane {
                        SupportPane::Thread { from_list: true, .. } => {
                            return self.open_support_list(support.group_id);
                        }
                        SupportPane::Thread { from_list: false, .. } | SupportPane::List => {
                            return self.close_support_overlay();
                        }
                    }
                }
                if self.group_settings.as_ref().is_some_and(|g| {
                    !matches!(g.pane, GroupSettingsPane::Overview)
                }) {
                    if let Some(g) = self.group_settings.as_mut() {
                        g.pane = GroupSettingsPane::Overview;
                        g.selected_member_id = None;
                    }
                    return Task::none();
                }
                if self.group_settings.is_some() {
                    self.group_settings = None;
                    return Task::none();
                }
                if self.contact_profile.is_some() {
                    self.contact_profile = None;
                    return Task::none();
                }
                if self.compose_open {
                    self.compose_open = false;
                    return Task::none();
                }
                self.selected = None;
                self.selected_id = None;
                self.messages.clear();
                Task::none()
            }
            Message::WindowResized(size) => {
                self.width = size.width;
                self.height = size.height;
                refresh_scale_factor()
            }
            Message::ScaleFactor(factor) => {
                if factor.is_finite() && factor > 0.0 {
                    self.scale_factor = factor;
                }
                Task::none()
            }
            Message::Connect => Task::none(),
            Message::Connected(Ok(name)) => {
                self.connecting = false;
                self.core_connect_launched = true;
                self.auth = None;
                if let Ok(mut guard) = self.pending_session.lock() {
                    self.session = guard.take();
                }
                self.status = format!("Connected as {name}.");
                if let Some(session) = self.session.clone() {
                    if let Some(user) = session.active_profile() {
                        self.own_profile = OwnProfileDraft {
                            fields: user.profile,
                            dirty: false,
                        };
                    }
                    Task::batch([
                        Task::perform(
                            activate_network(session.clone()),
                            Message::NetworkActivated,
                        ),
                        Task::perform(load_chats(session.clone()), Message::ChatsLoaded),
                        Task::perform(
                            sync_address_auto_accept(session),
                            Message::AddressAutoAcceptSynced,
                        ),
                    ])
                } else {
                    self.status = "Connected, but session handoff failed.".into();
                    Task::none()
                }
            }
            Message::Connected(Err(err)) => {
                self.connecting = false;
                // Stay on unlock/welcome so the user can retry.
                self.status = if err.to_lowercase().contains("migrat")
                    || err.to_lowercase().contains("password")
                    || err.to_lowercase().contains("key")
                {
                    format!("Could not open database (wrong password?). {err}")
                } else {
                    err
                };
                tracing::warn!(status = %self.status, "unlock failed");
                Task::none()
            }
            Message::SelectChat(index) => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.clear_pointer_gesture();
                self.selected = Some(index);
                self.selected_id = self.chats.get(index).map(|c| c.id.without_scope());
                self.support = None;
                self.group_settings = None;
                self.contact_profile = None;
                // List/message hover must not stick into the conversation.
                self.hover.clear_all();
                self.cancel_press_timers();
                // Optimistic: clear badge immediately while core marks items read.
                if let Some(chat) = self.chats.get_mut(index) {
                    chat.unread = 0;
                }
                self.messages.clear();
                self.pin_history_end = true;
                self.reply_to = None;
                self.editing = None;
                self.message_menu = None;
                self.chat_menu = None;
                self.selection_mode = false;
                self.selected_items.clear();
                self.revealed_deleted.clear();
                self.revealed_secrets.clear();
                self.reaction_members = None;
                self.focus_item = None;
                self.history_away_from_end = false;
                HISTORY_AWAY_GATE.reset();
                Task::batch([
                    self.mark_selected_read(),
                    self.reload_messages(),
                    iced::widget::operation::focus(iced::widget::Id::new("composer-input")),
                ])
            }
            Message::ComposerEdit(action) => {
                let before = cursus_core::pending_shortcode_query(&self.composer_text())
                    .map(str::to_owned);
                self.composer.perform(action);
                let raw = self.composer_text();
                let expanded = cursus_core::expand_emoji_shortcodes(&raw);
                if expanded != raw {
                    self.set_composer_text(expanded);
                }
                // Only reset highlight when the `:prefix` changes — never on
                // cursor motion / every keystroke (that broke Tab cycling).
                self.sync_emoji_suggest_index(before.as_deref());
                Task::none()
            }
            Message::ComposerInsert(text) => {
                // Paste full Unicode strings from OSK / composed accents.
                // `Binding::Insert` only takes one char and ignores Key::Character
                // when the event has no `text` payload (common on mobile VK).
                let before = cursus_core::pending_shortcode_query(&self.composer_text())
                    .map(str::to_owned);
                self.composer
                    .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                        std::sync::Arc::new(text),
                    )));
                let raw = self.composer_text();
                let expanded = cursus_core::expand_emoji_shortcodes(&raw);
                if expanded != raw {
                    self.set_composer_text(expanded);
                }
                self.sync_emoji_suggest_index(before.as_deref());
                Task::none()
            }
            Message::EmojiSuggestNext => {
                let n = self.emoji_suggestions().len();
                if n > 0 {
                    self.emoji_suggest_index = (self.emoji_suggest_index + 1) % n;
                }
                Task::none()
            }
            Message::EmojiSuggestPrev => {
                let n = self.emoji_suggestions().len();
                if n > 0 {
                    self.emoji_suggest_index = (self.emoji_suggest_index + n - 1) % n;
                }
                Task::none()
            }
            Message::EmojiSuggestSelect(index) => {
                self.emoji_suggest_index = index;
                self.accept_emoji_suggestion();
                Task::none()
            }
            Message::EmojiSuggestAccept => {
                if self.emoji_suggestions().is_empty() {
                    return Task::none();
                }
                self.accept_emoji_suggestion();
                Task::none()
            }
            Message::Send => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if !self.emoji_suggestions().is_empty() {
                    self.accept_emoji_suggestion();
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected_chat_for_api() else {
                    return Task::none();
                };
                if chat.pending_request_id.is_some() {
                    self.status =
                        "Accept the contact request before sending messages.".into();
                    return Task::none();
                }
                let text = cursus_core::expand_emoji_shortcodes(self.composer_text().trim());
                if text.is_empty() {
                    return Task::none();
                }
                if let Some(edit) = self.editing.take() {
                    self.clear_composer();
                    self.reply_to = None;
                    return Task::batch([
                        Task::perform(
                            update_text(session, chat.id, edit.item_id, text),
                            Message::Edited,
                        ),
                        iced::widget::operation::focus(iced::widget::Id::new("composer-input")),
                    ]);
                }
                let quoted = self.reply_to.as_ref().map(|r| r.item_id);
                self.clear_composer();
                self.reply_to = None;
                // Keep OSK up on touch: re-focus after send (release-only hit
                // already avoids the first unfocus on many devices).
                Task::batch([
                    Task::perform(send_text(session, chat, text, quoted), Message::Sent),
                    iced::widget::operation::focus(iced::widget::Id::new("composer-input")),
                ])
            }
            Message::Sent(Ok(())) => {
                self.status = "Message accepted by SimpleX core.".into();
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::Sent(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::Edited(Ok(())) => {
                self.status = "Message updated.".into();
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::Edited(Err(err)) => {
                self.status = friendly_edit_error(&err);
                Task::none()
            }
            Message::EditLastOutgoing => self.begin_edit_last_outgoing(),
            Message::AttachFile => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected_chat_for_api() else {
                    self.status = "Select a chat before attaching a file.".into();
                    return Task::none();
                };
                if chat.pending_request_id.is_some() {
                    self.status = "Accept the contact request before sending files.".into();
                    return Task::none();
                }
                self.status = "Choose a file…".into();
                Task::perform(attach_and_send_file(session, chat), Message::FileAttached)
            }
            Message::FileAttached(Ok(())) => {
                self.status = "File sent — uploading via XFTP…".into();
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::FileAttached(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ReceiveFile { file_id, file_name } => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.enqueue_receive(file_id, file_name)
            }
            Message::FileReceived(Ok(summary)) => {
                self.status = summary;
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::FileReceived(Err(err)) => {
                // Allow retry after failure (`file_id:<id>:…` from receive_file).
                if let Some(rest) = err.strip_prefix("file_id:") {
                    if let Some((id, _)) = rest.split_once(':') {
                        if let Ok(id) = id.parse::<i64>() {
                            self.receiving_files.remove(&id);
                        }
                    }
                }
                self.status = err;
                Task::none()
            }
            Message::OpenLocalFile(path) => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.status = format!("Opening {}…", path.display());
                Task::perform(open_local_file(path), Message::FileReceived)
            }
            Message::OpenUrl(url) => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.pending_url = Some(url);
                Task::none()
            }
            Message::ConfirmOpenUrl => {
                let Some(url) = self.pending_url.take() else {
                    return Task::none();
                };
                self.status = format!("Opening {url}…");
                Task::perform(open_url(url), Message::FileReceived)
            }
            Message::CancelOpenUrl => {
                self.pending_url = None;
                Task::none()
            }
            Message::ToggleReaction { item_id, emoji } => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected_chat_for_api() else {
                    return Task::none();
                };
                let add = self
                    .messages
                    .iter()
                    .find(|m| m.item_id == item_id)
                    .and_then(|m| {
                        m.reactions.iter().find(|r| reaction_emoji_eq(&r.emoji, &emoji))
                    })
                    .map(|r| !r.user_reacted)
                    .unwrap_or(true);
                self.message_menu = None;
                Task::perform(
                    set_reaction(session, chat.id, item_id, emoji, add),
                    Message::ReactionDone,
                )
            }
            Message::ShowReactionMembers { item_id, emoji } => {
                self.fetch_reaction_members(item_id, emoji)
            }
            Message::ReactionChipLongPress {
                item_id,
                emoji,
                generation,
            } => {
                let matched = self.reaction_chip_press_generation == generation
                    && self
                        .reaction_chip_press
                        .as_ref()
                        .is_some_and(|(id, e)| *id == item_id && e == &emoji)
                    && !self.press_moved;
                if matched {
                    self.reaction_chip_press = None;
                    self.press_moved = true; // ignore the following lift as a toggle tap
                    self.hover_ok = false;
                    return self.fetch_reaction_members(item_id, emoji);
                }
                Task::none()
            }
            Message::ReactionMembersLoaded {
                item_id,
                emoji,
                result,
            } => {
                match result {
                    Ok(names) => {
                        self.reaction_members = Some(ReactionMembersOverlay {
                            item_id,
                            emoji,
                            names,
                            loading: false,
                            error: None,
                        });
                    }
                    Err(err) => {
                        self.reaction_members = Some(ReactionMembersOverlay {
                            item_id,
                            emoji,
                            names: Vec::new(),
                            loading: false,
                            error: Some(err),
                        });
                    }
                }
                Task::none()
            }
            Message::CloseReactionMembers => {
                self.reaction_members = None;
                Task::none()
            }
            Message::ReactionDone(Ok(())) => {
                self.status = "Reaction updated.".into();
                self.reload_messages()
            }
            Message::ReactionDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenMessageMenu(item_id) => {
                // Touch double-tap / desktop right-click — don't require
                // tap_allowed (press_moved may still be set).
                self.open_message_menu(item_id);
                Task::none()
            }
            Message::AcceptGroupInvitation(group_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.status = "Joining group…".into();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            session
                                .join_group(group_id)
                                .map(|_| "Joined group.".to_string())
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    Message::GroupInvitationHandled,
                )
            }
            Message::RejectGroupInvitation(group_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.status = "Rejecting invitation…".into();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            session
                                .leave_group(group_id)
                                .map(|_| "Group invitation rejected.".to_string())
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    Message::GroupInvitationHandled,
                )
            }
            Message::GroupInvitationHandled(Ok(msg)) => {
                self.status = msg;
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::GroupInvitationHandled(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::CloseMessageMenu => {
                self.message_menu = None;
                Task::none()
            }
            Message::MsgMenuAction(action) => self.handle_msg_menu_action(action),
            Message::MsgMenuConfirmDelete => self.confirm_msg_delete(),
            Message::MsgMenuCancelConfirm => {
                if let Some(menu) = self.message_menu.as_mut() {
                    menu.pane = MsgMenuPane::Actions;
                }
                Task::none()
            }
            Message::MsgBubbleReleased(item_id) => {
                if self.selection_mode && !self.press_moved {
                    if self.selected_items.contains(&item_id) {
                        self.selected_items.remove(&item_id);
                    } else {
                        self.selected_items.insert(item_id);
                    }
                    if self.selected_items.is_empty() {
                        self.selection_mode = false;
                    }
                }
                self.msg_long_press_item = None;
                self.msg_long_press_generation = self.msg_long_press_generation.wrapping_add(1);
                Task::none()
            }
            Message::MsgBubbleLongPress { item_id, generation } => {
                if self.msg_long_press_generation == generation
                    && self.msg_long_press_item == Some(item_id)
                    && !self.press_moved
                {
                    self.msg_long_press_item = None;
                    self.press_moved = true; // ignore lift as a media/quote tap
                    self.hover_ok = false;
                    self.open_message_menu(item_id);
                }
                Task::none()
            }
            Message::CancelReply => {
                self.reply_to = None;
                Task::none()
            }
            Message::CancelEdit => {
                self.editing = None;
                self.clear_composer();
                Task::none()
            }
            Message::JumpToQuoted(item_id) => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.jump_to_item(item_id)
            }
            Message::ClearFocusItem { generation } => {
                if self.focus_item_generation == generation {
                    self.focus_item = None;
                }
                Task::none()
            }
            Message::ScrollToLatest => {
                self.history_away_from_end = false;
                HISTORY_AWAY_GATE.reset();
                iced::widget::operation::snap_to(
                    iced::widget::Id::new("chat-history"),
                    RelativeOffset { x: 0.0, y: 0.0 },
                )
            }
            Message::HistoryAwayChanged(away) => {
                // Ignore on empty/short threads — wheel still fires globally.
                if away && (self.selected.is_none() || self.messages.len() < 8) {
                    HISTORY_AWAY_GATE.reset();
                    return Task::none();
                }
                if self.history_away_from_end != away {
                    self.history_away_from_end = away;
                    HISTORY_AWAY_GATE.set(away);
                }
                Task::none()
            }
            Message::ToggleDesktopNotifications => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.desktop_prefs.notifications = !self.desktop_prefs.notifications;
                desktop::save_prefs(&self.desktop_prefs);
                self.status = if self.desktop_prefs.notifications {
                    "Desktop notifications on.".into()
                } else {
                    "Desktop notifications off.".into()
                };
                Task::none()
            }
            Message::ToggleTray => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if self.desktop_prefs.tray {
                    self.tray = None;
                    self.desktop_prefs.tray = false;
                    self.desktop_prefs.close_to_tray = false;
                    desktop::save_prefs(&self.desktop_prefs);
                    self.status = "System tray disabled.".into();
                } else if let Some(tray) = desktop::AppTray::try_new() {
                    self.tray = Some(tray);
                    self.desktop_prefs.tray = true;
                    desktop::save_prefs(&self.desktop_prefs);
                    self.status = "System tray enabled.".into();
                } else {
                    self.status =
                        "System tray unavailable (StatusNotifier / AppIndicator).".into();
                }
                Task::none()
            }
            Message::ToggleCloseToTray => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if !self.desktop_prefs.tray {
                    self.status = "Enable the tray first.".into();
                    return Task::none();
                }
                self.desktop_prefs.close_to_tray = !self.desktop_prefs.close_to_tray;
                desktop::save_prefs(&self.desktop_prefs);
                self.status = if self.desktop_prefs.close_to_tray {
                    "Close hides to tray.".into()
                } else {
                    "Close quits Cursus.".into()
                };
                Task::none()
            }
            Message::CycleThemePreference => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                theme::invalidate_system_theme_cache();
                self.desktop_prefs.theme = self.desktop_prefs.theme.cycle();
                desktop::save_prefs(&self.desktop_prefs);
                self.apply_appearance();
                self.status = format!(
                    "Theme: {}.",
                    self.desktop_prefs.theme.label()
                );
                Task::none()
            }
            Message::CycleTextSize => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.desktop_prefs.text_size = self.desktop_prefs.text_size.cycle();
                desktop::save_prefs(&self.desktop_prefs);
                self.status = format!(
                    "Text size: {}.",
                    self.desktop_prefs.text_size.label()
                );
                Task::none()
            }
            Message::ToggleAddressAutoAccept => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    self.status = "Connect first to change address settings.".into();
                    return Task::none();
                };
                let enable = !self.desktop_prefs.auto_accept_address;
                // Optimistic UI; core result reconciles.
                self.desktop_prefs.auto_accept_address = enable;
                self.status = if enable {
                    "Enabling auto-accept…".into()
                } else {
                    "Disabling auto-accept…".into()
                };
                Task::perform(
                    set_address_auto_accept(session, enable),
                    Message::AddressAutoAcceptSynced,
                )
            }
            Message::AddressAutoAcceptSynced(Ok(enabled)) => {
                self.desktop_prefs.auto_accept_address = enabled;
                desktop::save_prefs(&self.desktop_prefs);
                self.status = if enabled {
                    "Address auto-accept on.".into()
                } else {
                    "Address auto-accept off.".into()
                };
                Task::none()
            }
            Message::AddressAutoAcceptSynced(Err(err)) => {
                self.status = err;
                // Re-sync from core if possible.
                if let Some(session) = self.session.clone() {
                    Task::perform(
                        sync_address_auto_accept(session),
                        Message::AddressAutoAcceptSynced,
                    )
                } else {
                    Task::none()
                }
            }
            Message::NetworkActivated(Ok(())) => Task::none(),
            Message::NetworkActivated(Err(err)) => {
                tracing::warn!(%err, "activate/resubscribe failed");
                Task::none()
            }
            Message::OwnProfileDisplayName(v) => {
                self.own_profile.fields.display_name = v;
                self.own_profile.dirty = true;
                Task::none()
            }
            Message::OwnProfileBio(v) => {
                self.own_profile.fields.short_descr = media::truncate_bio_bytes(&v);
                self.own_profile.dirty = true;
                Task::none()
            }
            Message::OwnProfileDescription(v) => {
                self.own_profile.fields.description = v;
                self.own_profile.dirty = true;
                Task::none()
            }
            Message::PickOwnProfileImage => Task::perform(
                pick_and_encode_profile_image(),
                Message::OwnProfileImagePicked,
            ),
            Message::OwnProfileImagePicked(Ok(data_url)) => {
                self.own_profile.fields.image = Some(data_url);
                self.own_profile.dirty = true;
                self.status = "Profile picture ready — Save to apply.".into();
                Task::none()
            }
            Message::OwnProfileImagePicked(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ClearOwnProfileImage => {
                self.own_profile.fields.image = None;
                self.own_profile.dirty = true;
                Task::none()
            }
            Message::SaveOwnProfile => {
                let Some(session) = self.session.clone() else {
                    self.status = "Connect first.".into();
                    return Task::none();
                };
                let name = self.own_profile.fields.display_name.trim().to_string();
                if name.is_empty() {
                    self.status = "Display name is required.".into();
                    return Task::none();
                }
                self.own_profile.fields.display_name = name;
                let fields = self.own_profile.fields.clone();
                self.status = "Saving profile…".into();
                Task::perform(save_own_profile(session, fields), Message::OwnProfileSaved)
            }
            Message::OwnProfileSaved(Ok(user)) => {
                self.own_profile = OwnProfileDraft {
                    fields: user.profile,
                    dirty: false,
                };
                self.status = "Profile saved (contacts notified).".into();
                Task::none()
            }
            Message::OwnProfileSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ToggleProfileAddress => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    self.status = "Connect first.".into();
                    return Task::none();
                };
                let enable = !self.own_profile.fields.contact_link.as_ref().is_some_and(|s| !s.is_empty());
                self.status = if enable {
                    "Sharing address on profile…".into()
                } else {
                    "Removing address from profile…".into()
                };
                Task::perform(
                    set_profile_address(session, enable),
                    Message::ProfileAddressToggled,
                )
            }
            Message::ProfileAddressToggled(Ok(user)) => {
                self.own_profile.fields.contact_link = user.profile.contact_link.clone();
                self.own_profile.fields.preferences = user.profile.preferences.clone();
                // Keep draft edits for other fields; only address share changed.
                self.status = if user.shares_address() {
                    "Address shared on profile.".into()
                } else {
                    "Address removed from profile.".into()
                };
                Task::none()
            }
            Message::ProfileAddressToggled(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenContactProfile => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.open_contact_profile()
            }
            Message::CloseContactProfile => {
                let dirty = self
                    .contact_profile
                    .as_ref()
                    .is_some_and(|p| p.alias_dirty);
                if dirty {
                    // Save first (needs overlay), then dismiss.
                    return Task::batch([
                        self.update(Message::SaveContactAlias),
                        Task::done(Message::CloseContactProfileAfterSave),
                    ]);
                }
                self.contact_profile = None;
                Task::none()
            }
            Message::CloseContactProfileAfterSave => {
                self.contact_profile = None;
                Task::none()
            }
            Message::ContactAliasChanged(v) => {
                if let Some(p) = self.contact_profile.as_mut() {
                    p.alias = v;
                    p.alias_dirty = true;
                }
                Task::none()
            }
            Message::SaveContactAlias => {
                let Some(overlay) = self.contact_profile.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let contact_id = overlay.contact_id;
                let alias = overlay.alias.clone();
                Task::perform(
                    save_contact_alias(session, contact_id, alias),
                    Message::ContactAliasSaved,
                )
            }
            Message::ContactAliasSaved(Ok(())) => {
                if let Some(p) = self.contact_profile.as_mut() {
                    p.alias_dirty = false;
                    let idx = p.chat_index;
                    let alias = p.alias.clone();
                    let peer = p.fields.display_name.clone();
                    if let Some(chat) = self.chats.get_mut(idx) {
                        chat.local_alias = alias.clone();
                        chat.title = if alias.is_empty() {
                            if peer.is_empty() {
                                chat.title.clone()
                            } else {
                                peer
                            }
                        } else {
                            alias
                        };
                    }
                }
                self.status = "Alias saved.".into();
                self.reload_chats()
            }
            Message::ContactAliasSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ContactProfileMute => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected.and_then(|i| self.chats.get(i).cloned()) else {
                    return Task::none();
                };
                let mut settings = chat.settings.clone();
                settings.enable_ntfs = settings.enable_ntfs.next_direct();
                let label = if settings.enable_ntfs.is_muted() {
                    "Chat muted."
                } else {
                    "Chat unmuted."
                };
                if let Some(p) = self.contact_profile.as_mut() {
                    p.muted = settings.enable_ntfs.is_muted();
                }
                Task::perform(
                    set_chat_settings(session, chat.id, settings, label),
                    Message::ChatMenuDone,
                )
            }
            Message::ContactProfileFavorite => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected.and_then(|i| self.chats.get(i).cloned()) else {
                    return Task::none();
                };
                let mut settings = chat.settings.clone();
                settings.favorite = !settings.favorite;
                let label = if settings.favorite {
                    "Added to favorites."
                } else {
                    "Removed from favorites."
                };
                if let Some(p) = self.contact_profile.as_mut() {
                    p.favorite = settings.favorite;
                }
                Task::perform(
                    set_chat_settings(session, chat.id, settings, label),
                    Message::ChatMenuDone,
                )
            }
            Message::ContactProfileClear => {
                // Reuse confirm flow via a synthetic chat menu.
                if let Some(i) = self.selected {
                    self.open_chat_menu(i);
                    if let Some(menu) = self.chat_menu.as_mut() {
                        menu.pane = ChatMenuPane::ConfirmClear;
                    }
                    self.contact_profile = None;
                }
                Task::none()
            }
            Message::CycleContactTimedMessages => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    p.timed_allow = p.timed_allow.cycle();
                    if p.timed_allow.is_on() && p.timed_ttl_secs <= 0 {
                        p.timed_ttl_secs = DEFAULT_TIMED_TTL_SECS;
                    }
                }
                self.save_contact_feature_prefs()
            }
            Message::SetContactTimedTtl(secs) => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    if !p.timed_allow.is_on() {
                        return Task::none();
                    }
                    p.timed_ttl_secs = secs.max(1);
                }
                self.save_contact_feature_prefs()
            }
            Message::CycleContactFullDelete => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    p.full_delete_allow = p.full_delete_allow.cycle();
                }
                self.save_contact_feature_prefs()
            }
            Message::CycleContactReactions => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    p.reactions_allow = p.reactions_allow.cycle();
                }
                self.save_contact_feature_prefs()
            }
            Message::CycleContactVoice => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    p.voice_allow = p.voice_allow.cycle();
                }
                self.save_contact_feature_prefs()
            }
            Message::CycleContactCalls => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                if let Some(p) = self.contact_profile.as_mut() {
                    p.calls_allow = p.calls_allow.cycle();
                }
                self.save_contact_feature_prefs()
            }
            Message::ContactPrefsSaved(Ok(())) => {
                self.status = "Chat features updated.".into();
                // Reload chats so mergedPreferences / contactPreference refresh.
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::ContactPrefsSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ContactProfileDelete => {
                if let Some(i) = self.selected {
                    self.open_chat_menu(i);
                    if let Some(menu) = self.chat_menu.as_mut() {
                        menu.pane = ChatMenuPane::ConfirmDelete;
                    }
                    self.contact_profile = None;
                }
                Task::none()
            }
            Message::OpenCompose => {
                self.compose_open = true;
                Task::none()
            }
            Message::CloseCompose => {
                self.compose_open = false;
                Task::none()
            }
            Message::OpenCreateGroup => {
                self.create_chat = Some(CreateChatOverlay {
                    kind: CreateKind::Group,
                    name: String::new(),
                });
                Task::none()
            }
            Message::OpenCreateChannel => {
                self.create_chat = Some(CreateChatOverlay {
                    kind: CreateKind::Channel,
                    name: String::new(),
                });
                Task::none()
            }
            Message::CloseCreateChat => {
                self.create_chat = None;
                Task::none()
            }
            Message::CreateChatNameChanged(v) => {
                if let Some(c) = self.create_chat.as_mut() {
                    c.name = v;
                }
                Task::none()
            }
            Message::ConfirmCreateChat => {
                let Some(draft) = self.create_chat.clone() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let name = draft.name.trim().to_string();
                if name.is_empty() {
                    self.status = match draft.kind {
                        CreateKind::Group => "Enter a group name.".into(),
                        CreateKind::Channel => "Enter a channel name.".into(),
                    };
                    return Task::none();
                }
                self.status = match draft.kind {
                    CreateKind::Group => "Creating group…".into(),
                    CreateKind::Channel => "Creating channel…".into(),
                };
                Task::perform(create_chat_entity(session, draft.kind, name), Message::CreateChatDone)
            }
            Message::CreateChatDone(Ok((kind, chat_id, link, title))) => {
                self.create_chat = None;
                self.compose_open = false;
                self.nav = match kind {
                    CreateKind::Group => NavSection::Chats,
                    CreateKind::Channel => NavSection::Channels,
                };
                self.selected_id = Some(chat_id.clone());
                match kind {
                    CreateKind::Group => {
                        self.status = "Group created.".into();
                        self.reload_chats()
                    }
                    CreateKind::Channel => {
                        // Core keeps channels `creating_in_progress=1` until relays
                        // publish short-link data, so `/_get chats` omits them.
                        // Official apps seed the list from the create response —
                        // do the same so the channel shows immediately.
                        self.seeded_channel_ids.insert(chat_id.api_id);
                        if !self.chats.iter().any(|c| c.id == chat_id) {
                            let mut peer = ProfileFields::default();
                            peer.display_name = title.clone();
                            self.chats.insert(
                                0,
                                ChatPreview {
                                    id: chat_id.clone(),
                                    title: title.clone(),
                                    preview: "Channel created".into(),
                                    unread: 0,
                                    unread_chat: false,
                                    settings: ChatSettings::default(),
                                    tag_ids: Vec::new(),
                                    pending_request_id: None,
                                    peer_profile: peer,
                                    local_alias: String::new(),
                                    is_channel: true,
                                    membership_role: Some("owner".into()),
                                    group_preferences: None,
                                    member_admission: None,
                                    public_group: None,
                                    contact_preferences: None,
                                    contact_merged_preferences: None,
                                    members_require_attention: 0,
                                    membership_support_unread: 0,
                                    membership_has_support_chat: false,
                                },
                            );
                        }
                        self.rebind_selection();
                        if let Some(link) = link {
                            self.status = "Channel created — invite link copied.".into();
                            Task::batch([
                                iced::clipboard::write(link),
                                self.reload_chats(),
                            ])
                        } else {
                            self.status = "Channel created.".into();
                            self.reload_chats()
                        }
                    }
                }
            }
            Message::CreateChatDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenGroupSettings => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.open_group_settings()
            }
            Message::CloseGroupSettings => {
                let dirty = self
                    .group_settings
                    .as_ref()
                    .is_some_and(|g| g.alias_dirty);
                if dirty {
                    return Task::batch([
                        self.update(Message::SaveGroupAlias),
                        Task::done(Message::CloseGroupSettingsAfterSave),
                    ]);
                }
                // Also commit dirty group profile fields on leave.
                let profile_dirty = self.group_settings.as_ref().is_some_and(|g| g.dirty);
                if profile_dirty {
                    return Task::batch([
                        self.update(Message::SaveGroupProfile),
                        Task::done(Message::CloseGroupSettingsAfterSave),
                    ]);
                }
                self.group_settings = None;
                Task::none()
            }
            Message::CloseGroupSettingsAfterSave => {
                self.group_settings = None;
                Task::none()
            }
            Message::OpenSupportChatList => {
                let Some(chat) = self.selected.and_then(|i| self.chats.get(i)) else {
                    return Task::none();
                };
                if !matches!(chat.id.chat_type, ChatType::Group) || chat.is_channel {
                    return Task::none();
                }
                self.open_support_list(chat.id.api_id)
            }
            Message::OpenUserSupportChat => {
                let Some(chat) = self.selected.and_then(|i| self.chats.get(i).cloned()) else {
                    return Task::none();
                };
                if !matches!(chat.id.chat_type, ChatType::Group) || chat.is_channel {
                    return Task::none();
                }
                self.open_support_thread(chat.id.api_id, None, "Chat with admins".into(), false)
            }
            Message::OpenMemberSupportChat { member_id, title } => {
                let Some(chat) = self.selected.and_then(|i| self.chats.get(i)) else {
                    return Task::none();
                };
                if !matches!(chat.id.chat_type, ChatType::Group) {
                    return Task::none();
                }
                let from_list = self
                    .support
                    .as_ref()
                    .is_some_and(|s| matches!(s.pane, SupportPane::List));
                self.open_support_thread(chat.id.api_id, Some(member_id), title, from_list)
            }
            Message::CloseSupportOverlay => self.close_support_overlay(),
            Message::SupportMembersLoaded(Ok(members)) => {
                if let Some(s) = self.support.as_mut() {
                    s.members = members
                        .into_iter()
                        .filter(|m| m.support_chat.is_some())
                        .collect();
                    s.members_loading = false;
                }
                Task::none()
            }
            Message::SupportMembersLoaded(Err(err)) => {
                if let Some(s) = self.support.as_mut() {
                    s.members_loading = false;
                }
                self.status = err;
                Task::none()
            }
            Message::DeleteMemberSupportChat(member_id) => {
                let Some(s) = self.support.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = s.group_id;
                Task::perform(
                    delete_member_support_chat(session, group_id, member_id),
                    Message::MemberSupportChatDeleted,
                )
            }
            Message::MemberSupportChatDeleted(Ok(())) => {
                self.status = "Support chat deleted.".into();
                if let Some(s) = self.support.as_ref() {
                    let gid = s.group_id;
                    return self.open_support_list(gid);
                }
                Task::none()
            }
            Message::MemberSupportChatDeleted(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupSettingsNameChanged(v) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.name = v;
                    g.dirty = true;
                }
                Task::none()
            }
            Message::GroupSettingsShortDescrChanged(v) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.short_descr = media::truncate_bio_bytes(&v);
                    g.dirty = true;
                }
                Task::none()
            }
            Message::GroupSettingsWelcomeChanged(v) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.welcome_message = v;
                    g.dirty = true;
                }
                Task::none()
            }
            Message::GroupSettingsAliasChanged(v) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.alias = v;
                    g.alias_dirty = true;
                }
                Task::none()
            }
            Message::SaveGroupAlias => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = g.group_id;
                let alias = g.alias.clone();
                Task::perform(
                    save_group_alias(session, group_id, alias),
                    Message::GroupAliasSaved,
                )
            }
            Message::GroupAliasSaved(Ok(())) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.alias_dirty = false;
                    let gid = g.group_id;
                    let alias = g.alias.clone();
                    let peer = g.name.clone();
                    if let Some(chat) = self.chats.iter_mut().find(|c| {
                        matches!(c.id.chat_type, ChatType::Group) && c.id.api_id == gid
                    }) {
                        chat.local_alias = alias.clone();
                        chat.title = if alias.is_empty() {
                            if peer.is_empty() {
                                chat.title.clone()
                            } else {
                                peer
                            }
                        } else {
                            alias
                        };
                    }
                }
                self.status = "Local alias saved.".into();
                self.reload_chats()
            }
            Message::GroupAliasSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupSettingsMute => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let chat_id = ChatId {
                    chat_type: ChatType::Group,
                    api_id: g.group_id,
                    scope: None,
                };
                let Some(chat) = self
                    .chats
                    .iter()
                    .find(|c| c.id == chat_id)
                    .cloned()
                else {
                    return Task::none();
                };
                let mut settings = chat.settings.clone();
                settings.enable_ntfs = settings.enable_ntfs.next_direct();
                let muted = settings.enable_ntfs.is_muted();
                if let Some(g) = self.group_settings.as_mut() {
                    g.muted = muted;
                }
                if let Some(c) = self.chats.iter_mut().find(|c| c.id == chat_id) {
                    c.settings = settings.clone();
                }
                self.status = if muted {
                    "Muted.".into()
                } else {
                    "Unmuted.".into()
                };
                Task::perform(
                    set_chat_settings(session, chat_id, settings, "Notifications updated."),
                    Message::ChatMenuDone,
                )
            }
            Message::GroupSettingsFavorite => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let chat_id = ChatId {
                    chat_type: ChatType::Group,
                    api_id: g.group_id,
                    scope: None,
                };
                let Some(chat) = self
                    .chats
                    .iter()
                    .find(|c| c.id == chat_id)
                    .cloned()
                else {
                    return Task::none();
                };
                let mut settings = chat.settings.clone();
                settings.favorite = !settings.favorite;
                let favorite = settings.favorite;
                if let Some(g) = self.group_settings.as_mut() {
                    g.favorite = favorite;
                }
                if let Some(c) = self.chats.iter_mut().find(|c| c.id == chat_id) {
                    c.settings = settings.clone();
                }
                self.status = if favorite {
                    "Favorited.".into()
                } else {
                    "Unfavorited.".into()
                };
                Task::perform(
                    set_chat_settings(session, chat_id, settings, "Favorite updated."),
                    Message::ChatMenuDone,
                )
            }
            Message::PickGroupProfileImage => Task::perform(
                pick_and_encode_profile_image(),
                Message::GroupProfileImagePicked,
            ),
            Message::GroupProfileImagePicked(Ok(data_url)) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.image = Some(data_url);
                    g.dirty = true;
                }
                self.status = "Picture ready — Save profile to apply.".into();
                Task::none()
            }
            Message::GroupProfileImagePicked(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ClearGroupProfileImage => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.image = None;
                    g.dirty = true;
                }
                Task::none()
            }
            Message::SaveGroupProfile => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let name = g.name.trim().to_string();
                if name.is_empty() {
                    self.status = "Group name required.".into();
                    return Task::none();
                }
                let profile = GroupProfileUpdate {
                    display_name: name,
                    full_name: g.full_name.clone(),
                    short_descr: g.short_descr.clone(),
                    welcome_message: g.welcome_message.clone(),
                    image: g.image.clone(),
                    group_preferences: g.group_preferences.clone(),
                    member_admission: g.member_admission.clone(),
                    public_group: g.public_group.clone(),
                };
                let group_id = g.group_id;
                Task::perform(
                    save_group_profile(session, group_id, profile),
                    Message::GroupProfileSaved,
                )
            }
            Message::GroupProfileSaved(Ok(())) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.dirty = false;
                    g.title = g.name.clone();
                    let gid = g.group_id;
                    let title = g.title.clone();
                    let image = g.image.clone();
                    let short_descr = g.short_descr.clone();
                    let welcome = g.welcome_message.clone();
                    if let Some(chat) = self.chats.iter_mut().find(|c| {
                        matches!(c.id.chat_type, ChatType::Group) && c.id.api_id == gid
                    }) {
                        chat.title = title;
                        chat.peer_profile.image = image;
                        chat.peer_profile.short_descr = short_descr;
                        chat.peer_profile.description = welcome;
                    }
                }
                self.status = "Group profile saved.".into();
                self.reload_chats()
            }
            Message::GroupProfileSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupMembersLoaded(Ok(members)) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.members = members;
                    g.members_loading = false;
                }
                Task::none()
            }
            Message::GroupMembersLoaded(Err(err)) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.members_loading = false;
                }
                self.status = err;
                Task::none()
            }
            Message::OpenAddGroupMember => {
                if let Some(g) = self.group_settings.as_mut() {
                    if g.is_channel {
                        // Core: "can't invite contact to channel" — link only.
                        g.pane = GroupSettingsPane::Overview;
                        self.status =
                            "Channels use an invite link — create/copy it below. Contacts can’t be added directly."
                                .into();
                        return Task::none();
                    }
                    g.pane = GroupSettingsPane::AddMember;
                }
                Task::none()
            }
            Message::CloseAddGroupMember => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::Overview;
                }
                Task::none()
            }
            Message::GroupInviteRoleCycle => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.invite_role = cycle_role_below(&g.invite_role, &g.my_role);
                }
                Task::none()
            }
            Message::AddGroupMember(contact_id) => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                if g.is_channel {
                    self.status =
                        "Channels can’t invite contacts — share the channel link instead.".into();
                    if let Some(g) = self.group_settings.as_mut() {
                        g.pane = GroupSettingsPane::Overview;
                    }
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = g.group_id;
                let role = g.invite_role.clone();
                self.status = "Inviting contact…".into();
                Task::perform(
                    add_group_member(session, group_id, contact_id, role),
                    Message::GroupMemberAdded,
                )
            }
            Message::GroupMemberAdded(Ok(())) => {
                self.status = "Invitation sent.".into();
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::Overview;
                    g.members_loading = true;
                    let session = self.session.clone();
                    let group_id = g.group_id;
                    if let Some(session) = session {
                        return Task::perform(
                            load_group_members(session, group_id),
                            Message::GroupMembersLoaded,
                        );
                    }
                }
                Task::none()
            }
            Message::GroupMemberAdded(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenGroupMember(member_id) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.selected_member_id = Some(member_id);
                    g.pane = GroupSettingsPane::MemberInfo;
                }
                Task::none()
            }
            Message::CloseGroupMember => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.selected_member_id = None;
                    g.pane = GroupSettingsPane::Overview;
                }
                Task::none()
            }
            Message::GroupMemberSetRole(role) => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = g.group_id;
                self.status = format!("Setting role to {role}…");
                Task::perform(
                    set_group_member_role(session, group_id, member_id, role),
                    Message::GroupMemberRoleDone,
                )
            }
            Message::GroupMemberRoleDone(Ok(())) => {
                self.status = "Member role updated.".into();
                self.reload_group_members()
            }
            Message::GroupMemberRoleDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupMemberRemove { with_messages } => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = g.group_id;
                self.status = "Removing member…".into();
                Task::perform(
                    remove_group_member(session, group_id, member_id, with_messages),
                    Message::GroupMemberRemoved,
                )
            }
            Message::GroupMemberRemoved(Ok(())) => {
                self.status = "Member removed.".into();
                if let Some(g) = self.group_settings.as_mut() {
                    g.selected_member_id = None;
                    g.pane = GroupSettingsPane::Overview;
                }
                self.reload_group_members()
            }
            Message::GroupMemberRemoved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupMemberToggleBlockSelf => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let show = g
                    .members
                    .iter()
                    .find(|m| m.group_member_id == member_id)
                    .map(|m| !m.show_messages)
                    .unwrap_or(false);
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(
                    set_member_show_messages(session, g.group_id, member_id, show),
                    Message::GroupMemberBlockSelfDone,
                )
            }
            Message::GroupMemberBlockSelfDone(Ok(())) => {
                self.status = "Member visibility updated.".into();
                self.reload_group_members()
            }
            Message::GroupMemberBlockSelfDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupMemberToggleBlockAll => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let blocked = g
                    .members
                    .iter()
                    .find(|m| m.group_member_id == member_id)
                    .map(|m| !m.blocked_by_admin)
                    .unwrap_or(true);
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(
                    block_group_member(session, g.group_id, member_id, blocked),
                    Message::GroupMemberBlockAllDone,
                )
            }
            Message::GroupMemberBlockAllDone(Ok(())) => {
                self.status = "Member block updated.".into();
                self.reload_group_members()
            }
            Message::GroupMemberBlockAllDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::GroupMemberAccept => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(
                    accept_group_member(session, g.group_id, member_id, "member".into()),
                    Message::GroupMemberAccepted,
                )
            }
            Message::GroupMemberAccepted(Ok(())) => {
                self.status = "Member accepted.".into();
                self.reload_group_members()
            }
            Message::GroupMemberAccepted(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenGroupPreferences => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::Preferences;
                }
                Task::none()
            }
            Message::CloseGroupPreferences => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::Overview;
                }
                Task::none()
            }
            Message::GroupPrefToggle(key) => {
                if let Some(g) = self.group_settings.as_mut() {
                    toggle_group_pref(&mut g.group_preferences, key);
                    g.dirty = true;
                }
                Task::none()
            }
            Message::SetGroupTimedTtl(secs) => {
                if let Some(g) = self.group_settings.as_mut() {
                    if !group_pref_on(g.group_preferences.as_ref(), "timedMessages") {
                        return Task::none();
                    }
                    let obj = g
                        .group_preferences
                        .get_or_insert_with(|| serde_json::json!({}))
                        .as_object_mut();
                    if let Some(obj) = obj {
                        let entry = obj
                            .entry("timedMessages".to_string())
                            .or_insert_with(|| serde_json::json!({ "enable": "on" }));
                        if let Some(map) = entry.as_object_mut() {
                            map.insert("enable".into(), serde_json::Value::String("on".into()));
                            map.insert(
                                "ttl".into(),
                                serde_json::Value::Number(secs.max(1).into()),
                            );
                        }
                    }
                    g.dirty = true;
                }
                Task::none()
            }
            Message::GroupAdmissionToggle => {
                if let Some(g) = self.group_settings.as_mut() {
                    let on = g
                        .member_admission
                        .as_ref()
                        .and_then(|v| v.get("review"))
                        .is_some();
                    g.member_admission = if on {
                        None
                    } else {
                        Some(serde_json::json!({ "review": "all" }))
                    };
                    g.dirty = true;
                }
                Task::none()
            }
            Message::SaveGroupPreferences => self.update(Message::SaveGroupProfile),
            Message::EnsureGroupLink => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let group_id = g.group_id;
                // Link joiners may only be observer/member (core rejects higher).
                let role = match g.link_member_role.as_str() {
                    "observer" => "observer".into(),
                    _ => "member".into(),
                };
                let channel = g.is_channel;
                self.status = if channel {
                    "Preparing channel link…".into()
                } else {
                    "Preparing group link…".into()
                };
                Task::perform(
                    ensure_group_link(session, group_id, role),
                    Message::GroupLinkReady,
                )
            }
            Message::GroupLinkReady(Ok(link)) => {
                let channel = self
                    .group_settings
                    .as_ref()
                    .is_some_and(|g| g.is_channel);
                if let Some(g) = self.group_settings.as_mut() {
                    g.link = Some(link);
                }
                // Quiet for channels (link is always created with the channel).
                if !channel {
                    self.status = "Group link ready — tap Copy.".into();
                }
                Task::none()
            }
            Message::GroupLinkReady(Err(err)) => {
                let channel = self
                    .group_settings
                    .as_ref()
                    .is_some_and(|g| g.is_channel);
                if let Some(g) = self.group_settings.as_mut() {
                    g.link = None;
                }
                // Don’t dump raw agent errors into the chrome for channels.
                self.status = if channel {
                    "No channel invite link available — create a new channel to invite people."
                        .into()
                } else {
                    err
                };
                Task::none()
            }
            Message::CopyGroupLink => {
                let channel = self
                    .group_settings
                    .as_ref()
                    .is_some_and(|g| g.is_channel);
                if let Some(link) = self
                    .group_settings
                    .as_ref()
                    .and_then(|g| g.link.clone())
                {
                    self.status = if channel && is_channel_short_link(&link) {
                        "Channel link copied — share it with people to invite."
                            .into()
                    } else if channel {
                        "This isn’t a /c# channel link — create a new channel to invite people."
                            .into()
                    } else {
                        "Group link copied.".into()
                    };
                    iced::clipboard::write(link)
                } else if channel {
                    self.status =
                        "No channel link to copy — create a new channel for a fresh invite."
                            .into();
                    Task::none()
                } else {
                    self.update(Message::EnsureGroupLink)
                }
            }
            Message::GroupLinkRoleCycle => {
                let Some(g) = self.group_settings.as_mut() else {
                    return Task::none();
                };
                // Core rejects roles above `member` for link joiners
                // (`CEGroupMemberInitialRole` — only observer/member).
                g.link_member_role = cycle_link_joiner_role(&g.link_member_role);
                let role = g.link_member_role.clone();
                let group_id = g.group_id;
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                if g.link.is_none() {
                    return Task::none();
                }
                Task::perform(
                    set_group_link_role(session, group_id, role),
                    Message::GroupLinkRoleSet,
                )
            }
            Message::GroupLinkRoleSet(Ok(())) => {
                self.status = "Link joiner role updated.".into();
                Task::none()
            }
            Message::GroupLinkRoleSet(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::DeleteGroupLink => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                // Official: channel links cannot be deleted / rotated.
                if g.is_channel {
                    self.status =
                        "Channel links can’t be deleted — share the existing /c# link."
                            .into();
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(
                    delete_group_link(session, g.group_id),
                    Message::GroupLinkDeleted,
                )
            }
            Message::GroupLinkDeleted(Ok(())) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.link = None;
                }
                self.status = "Group link deleted.".into();
                Task::none()
            }
            Message::GroupLinkDeleted(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::AddGroupShortLink => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                // Official: `/_short link` is private-group only (PROHIBITED on channels).
                if g.is_channel {
                    self.status =
                        "Channel links already include the short form — use Copy."
                            .into();
                    return Task::none();
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.status = "Adding short / profile link…".into();
                Task::perform(
                    add_group_short_link(session, g.group_id),
                    Message::GroupShortLinkReady,
                )
            }
            Message::GroupShortLinkReady(Ok(link)) => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.link = Some(link.clone());
                }
                self.status = "Short link ready — copied.".into();
                iced::clipboard::write(link)
            }
            Message::GroupShortLinkReady(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenMemberDirectChat => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(member_id) = g.selected_member_id else {
                    return Task::none();
                };
                let Some(contact_id) = g
                    .members
                    .iter()
                    .find(|m| m.group_member_id == member_id)
                    .and_then(|m| m.contact_id)
                else {
                    self.status = "No direct chat with this member yet.".into();
                    return Task::none();
                };
                let Some((index, _)) = self.chats.iter().enumerate().find(|(_, c)| {
                    matches!(c.id.chat_type, ChatType::Direct) && c.id.api_id == contact_id
                }) else {
                    self.status =
                        "Direct chat not found — enable Direct messages in preferences, then reconnect."
                            .into();
                    return Task::none();
                };
                self.group_settings = None;
                self.nav = NavSection::Chats;
                self.update(Message::SelectChat(index))
            }
            Message::OpenLeaveGroupConfirm => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::ConfirmLeave;
                }
                Task::none()
            }
            Message::ConfirmLeaveGroup => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(leave_group(session, g.group_id), Message::GroupLeft)
            }
            Message::GroupLeft(Ok(())) => {
                self.group_settings = None;
                self.selected = None;
                self.selected_id = None;
                self.messages.clear();
                self.status = "Left group.".into();
                self.reload_chats()
            }
            Message::GroupLeft(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::OpenDeleteGroupConfirm => {
                if let Some(g) = self.group_settings.as_mut() {
                    g.pane = GroupSettingsPane::ConfirmDelete;
                }
                Task::none()
            }
            Message::ConfirmDeleteGroup => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let chat = ChatId {
                    chat_type: ChatType::Group,
                    api_id: g.group_id,
                    scope: None,
                };
                self.pending_delete_id = Some(chat.clone());
                // Drop seed + row immediately so reload cannot resurrect a ghost.
                self.forget_chat_locally(&chat);
                self.group_settings = None;
                self.status = "Deleting…".into();
                Task::perform(delete_chat(session, chat), Message::GroupDeleted)
            }
            Message::GroupDeleted(Ok(_)) => {
                self.pending_delete_id = None;
                self.group_settings = None;
                self.status = "Deleted.".into();
                self.reload_chats()
            }
            Message::GroupDeleted(Err(err)) => {
                if store_already_gone(&err) {
                    if let Some(id) = self.pending_delete_id.take() {
                        self.forget_chat_locally(&id);
                    }
                    self.group_settings = None;
                    self.status = "Deleted.".into();
                    return self.reload_chats();
                }
                self.pending_delete_id = None;
                self.status = err;
                self.reload_chats()
            }
            Message::ClearGroupChat => {
                let Some(g) = self.group_settings.as_ref() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let chat = ChatId {
                    chat_type: ChatType::Group,
                    api_id: g.group_id,
                    scope: None,
                };
                Task::perform(clear_chat(session, chat), Message::GroupChatCleared)
            }
            Message::GroupChatCleared(Ok(_)) => {
                self.messages.clear();
                self.status = "Chat cleared.".into();
                self.reload_messages()
            }
            Message::GroupChatCleared(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::StartAudioCall(contact_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let peer_name = self
                    .chats
                    .iter()
                    .find(|c| {
                        matches!(c.id.chat_type, ChatType::Direct) && c.id.api_id == contact_id
                    })
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| "Contact".into());
                self.active_call = Some(CallInfo {
                    contact_id,
                    peer_name: peer_name.clone(),
                    outgoing: true,
                    media: CallMedia::Audio,
                });
                self.incoming_call = None;
                self.status = format!("Calling {peer_name}…");
                self.nav = NavSection::Calls;
                Task::perform(
                    call_invite(session, contact_id, CallMedia::Audio),
                    Message::CallActionDone,
                )
            }
            Message::AcceptIncomingCall => {
                let Some(call) = self.incoming_call.take() else {
                    return Task::none();
                };
                // Accept path: keep call active; peer's WebRTC offer will follow.
                // Full media plane (browser call.html bridge) is still thin here.
                self.active_call = Some(CallInfo {
                    outgoing: false,
                    ..call
                });
                self.status = format!("In call with {}.", self.active_call.as_ref().unwrap().peer_name);
                Task::none()
            }
            Message::RejectIncomingCall => {
                let Some(call) = self.incoming_call.take() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                Task::perform(call_reject(session, call.contact_id), Message::CallActionDone)
            }
            Message::EndActiveCall => {
                let Some(call) = self.active_call.take() else {
                    return Task::none();
                };
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.status = "Call ended.".into();
                Task::perform(call_end(session, call.contact_id), Message::CallActionDone)
            }
            Message::CallActionDone(Ok(msg)) => {
                self.status = msg;
                Task::none()
            }
            Message::CallActionDone(Err(err)) => {
                self.status = err;
                self.active_call = None;
                Task::none()
            }
            Message::WindowFocused(focused) => {
                self.window_focused = focused;
                Task::none()
            }
            Message::WindowCloseRequested => {
                // Never tray-hide during welcome/unlock — that left a mid-migrate
                // process holding the DB while a new window looked “stuck”.
                if self.auth.is_some() || self.connecting {
                    return self.update(Message::QuitApp);
                }
                if self.desktop_prefs.close_to_tray && self.tray.is_some() {
                    self.update(Message::HideToTray)
                } else {
                    self.update(Message::QuitApp)
                }
            }
            Message::MainWindowOpened(id) => {
                self.main_window = Some(id);
                self.window_focused = true;
                // Do not open libsimplex until welcome/unlock succeeds (SQLCipher key).
                Task::batch([
                    iced::window::gain_focus(id),
                    refresh_window_metrics_for(id),
                ])
            }
            Message::WelcomeNameChanged(name) => {
                if let Some(AuthScreen::WelcomeName { name: n }) = self.auth.as_mut() {
                    *n = name;
                }
                Task::none()
            }
            Message::WelcomeNameNext => {
                let Some(AuthScreen::WelcomeName { name }) = self.auth.clone() else {
                    return Task::none();
                };
                let name = name.trim().to_string();
                if name.is_empty() {
                    self.status = "Enter a display name.".into();
                    return Task::none();
                }
                self.auth = Some(AuthScreen::WelcomePassword {
                    name,
                    password: String::new(),
                    confirm: String::new(),
                });
                self.status = "Choose a password that unlocks your encrypted database.".into();
                Task::none()
            }
            Message::WelcomePasswordChanged(password) => {
                if let Some(AuthScreen::WelcomePassword { password: p, .. }) = self.auth.as_mut() {
                    *p = password;
                }
                Task::none()
            }
            Message::WelcomeConfirmChanged(confirm) => {
                if let Some(AuthScreen::WelcomePassword { confirm: c, .. }) = self.auth.as_mut() {
                    *c = confirm;
                }
                Task::none()
            }
            Message::WelcomeBack => {
                if let Some(AuthScreen::WelcomePassword { name, .. }) = self.auth.clone() {
                    self.auth = Some(AuthScreen::WelcomeName { name });
                    self.status = "Welcome — set your name to begin.".into();
                }
                Task::none()
            }
            Message::WelcomeSubmit => {
                let Some(AuthScreen::WelcomePassword {
                    name,
                    password,
                    confirm,
                }) = self.auth.clone()
                else {
                    return Task::none();
                };
                if password.len() < 8 {
                    self.status = "Password must be at least 8 characters.".into();
                    return Task::none();
                }
                if password != confirm {
                    self.status = "Passwords do not match.".into();
                    return Task::none();
                }
                self.begin_connect(ConnectRequest {
                    password,
                    display_name: name,
                    encrypt_local_files: true,
                    save_account: true,
                })
            }
            Message::UnlockPasswordChanged(password) => {
                if let Some(AuthScreen::Unlock { password: p, .. }) = self.auth.as_mut() {
                    *p = password;
                }
                Task::none()
            }
            Message::UnlockSubmit => {
                let Some(AuthScreen::Unlock {
                    display_name,
                    password,
                }) = self.auth.clone()
                else {
                    return Task::none();
                };
                if password.is_empty() {
                    self.status = "Enter your password.".into();
                    return Task::none();
                }
                let encrypt = load_account()
                    .map(|a| a.encrypt_local_files)
                    .unwrap_or(true);
                self.begin_connect(ConnectRequest {
                    password,
                    display_name,
                    encrypt_local_files: encrypt,
                    save_account: false,
                })
            }
            Message::DecryptFileDone { encrypted, result } => {
                let mut tasks = Vec::new();
                match result {
                    Ok(plain) => {
                        if plain.is_file() && !self.waveforms.contains_key(&plain) {
                            // Voice / audio may need peaks from the plaintext cache.
                            let looks_audio = plain
                                .extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| {
                                    matches!(
                                        e.to_ascii_lowercase().as_str(),
                                        "m4a" | "aac" | "mp3" | "ogg" | "opus" | "wav" | "caf"
                                    )
                                });
                            if looks_audio {
                                tasks.push(self.load_waveform(plain.clone()));
                            }
                        }
                        self.plaintext_files.insert(encrypted, plain);
                    }
                    Err(err) => {
                        tracing::warn!(%err, "decrypt attachment failed");
                    }
                }
                // Keep the decrypt queue moving without blasting the thread pool.
                tasks.push(self.enqueue_decrypts());
                tasks.push(self.enqueue_missing_video_thumbs());
                Task::batch(tasks)
            }
            Message::ShowFromTray => {
                if let Some(id) = self.main_window {
                    return Task::batch([
                        iced::window::gain_focus(id),
                        iced::window::set_mode(id, iced::window::Mode::Windowed),
                    ]);
                }
                let (id, open) = open_main_window();
                self.main_window = Some(id);
                open
            }
            Message::HideToTray => {
                let Some(id) = self.main_window.take() else {
                    return Task::none();
                };
                self.window_focused = false;
                // Close (don't Mode::Hidden) — Wayland ignores set_visible(false).
                iced::window::close(id)
            }
            Message::QuitApp => {
                self.tray = None;
                let close = self
                    .main_window
                    .take()
                    .map(iced::window::close)
                    .unwrap_or_else(Task::none);
                Task::batch([close, iced::exit()])
            }
            Message::ToggleRevealDeleted(item_id) => {
                self.toggle_reveal_deleted(item_id);
                Task::none()
            }
            Message::ToggleSecretReveal { item_id, index } => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let entry = self.revealed_secrets.entry(item_id).or_default();
                if entry.contains(&index) {
                    entry.remove(&index);
                    if entry.is_empty() {
                        self.revealed_secrets.remove(&item_id);
                    }
                } else {
                    entry.insert(index);
                }
                Task::none()
            }
            Message::ForwardToChat(index) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(from) = self.selected_chat_for_api() else {
                    return Task::none();
                };
                let Some(to) = self.chats.get(index).cloned() else {
                    return Task::none();
                };
                let Some(menu) = self.message_menu.as_ref() else {
                    return Task::none();
                };
                let item_id = menu.item_id;
                self.message_menu = None;
                Task::perform(
                    forward_item(session, from.id, to.id, item_id),
                    Message::ForwardDone,
                )
            }
            Message::ForwardDone(Ok(())) => {
                self.status = "Message forwarded.".into();
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::ForwardDone(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::DeleteItemsDone(Ok(())) => {
                self.status = if self.selected_full_delete_for_user() {
                    "Message deleted for everyone.".into()
                } else {
                    "Message deleted here — peer needs Full deletion allowed to wipe it.".into()
                };
                self.selected_items.clear();
                self.selection_mode = false;
                self.message_menu = None;
                Task::batch([self.reload_messages(), self.reload_chats()])
            }
            Message::DeleteItemsDone(Err(err)) => {
                // Already-marked items reject broadcast; refresh so UI shows the mark.
                if err.contains("invalidChatItemDelete") {
                    self.message_menu = None;
                    self.status = "Message already deleted.".into();
                    return self.reload_messages();
                }
                self.status = err;
                Task::none()
            }
            Message::ItemInfoLoaded(Ok(info)) => {
                if let Some(menu) = self.message_menu.as_mut() {
                    let file_detail = match &menu.pane {
                        MsgMenuPane::Info { file_detail, .. } => file_detail.clone(),
                        _ => None,
                    };
                    menu.pane = MsgMenuPane::Info {
                        item_id: info.item_id,
                        time: info.time,
                        status: info.status,
                        text: info.text,
                        file_detail,
                    };
                }
                Task::none()
            }
            Message::ItemInfoLoaded(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ClearSelection => {
                self.selected_items.clear();
                self.selection_mode = false;
                Task::none()
            }
            Message::DeleteSelected => {
                let ids: Vec<i64> = self.selected_items.iter().copied().collect();
                let mut live = Vec::new();
                let mut marked = Vec::new();
                for id in ids {
                    if self
                        .messages
                        .iter()
                        .find(|m| m.item_id == id)
                        .map(|m| m.is_deleted())
                        .unwrap_or(false)
                    {
                        marked.push(id);
                    } else {
                        live.push(id);
                    }
                }
                let mut tasks = Vec::new();
                if !live.is_empty() {
                    tasks.push(self.delete_items(live, true));
                }
                if !marked.is_empty() {
                    tasks.push(self.delete_items(marked, false));
                }
                Task::batch(tasks)
            }
            Message::ToggleVoiceRecord => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                let task = self.toggle_voice_record();
                // Keep OSK / composer focus when possible after mic press.
                Task::batch([
                    task,
                    iced::widget::operation::focus(iced::widget::Id::new("composer-input")),
                ])
            }
            Message::VoiceDraftReady(Ok(draft)) => {
                self.waveforms
                    .insert(draft.path.clone(), draft.peaks.clone());
                self.voice_draft = Some(draft);
                self.status = "Voice ready — send or cancel.".into();
                Task::none()
            }
            Message::VoiceDraftReady(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::SendVoiceDraft => self.send_voice_draft(),
            Message::CancelVoiceDraft => {
                if let Some(draft) = self.voice_draft.take() {
                    let _ = std::fs::remove_file(&draft.path);
                }
                self.status = "Voice discarded.".into();
                Task::none()
            }
            Message::VoiceSent(Ok(())) => {
                self.voice_draft = None;
                self.status = "Voice message sent.".into();
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::VoiceSent(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::PlayVoice {
                file_id,
                path,
                duration_secs,
            } => {
                if !self.tap_allowed() {
                    return Task::none();
                }
                self.play_voice_bubble(file_id, path, duration_secs)
            }
            Message::StopVoicePlayback => {
                self.stop_voice_playback();
                Task::none()
            }
            Message::ScrollFlingStart { id, velocity_y } => {
                self.scroll_fling = Some(ScrollFling { id, velocity_y });
                Task::none()
            }
            Message::ScrollFlingCancel { id } => {
                if self.scroll_fling.as_ref().is_some_and(|f| f.id == id) {
                    self.scroll_fling = None;
                }
                Task::none()
            }
            Message::ScrollFlingTick => {
                let Some(fling) = self.scroll_fling.as_mut() else {
                    return Task::none();
                };
                const DT: f32 = 1.0 / 60.0;
                // Slightly snappier stop than GTK default — avoids runaway coasts.
                const FRICTION: f32 = 0.86;
                let dy = fling.velocity_y * DT;
                fling.velocity_y *= FRICTION;
                if fling.velocity_y.abs() < 40.0 {
                    self.scroll_fling = None;
                    return Task::none();
                }
                let id = iced::widget::Id::from(fling.id.clone());
                iced::widget::operation::scroll_by(
                    id,
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy },
                )
            }
            Message::VoiceTick => {
                if let Some(play) = self.voice_playback.as_mut() {
                    let elapsed = play.started.elapsed().as_secs_f32();
                    if elapsed >= play.duration_secs.max(1) as f32 {
                        self.stop_voice_playback();
                    } else if let Ok(Some(_)) = play.child.try_wait() {
                        self.stop_voice_playback();
                    }
                }
                Task::none()
            }
            Message::WaveformReady { path, peaks } => {
                if let Ok(peaks) = peaks {
                    self.waveforms.insert(path, peaks);
                }
                Task::none()
            }
            Message::VideoThumbReady { path, thumb } => {
                self.video_thumbs_pending.remove(&path);
                let mut tasks = Vec::new();
                if let Some(thumb) = thumb {
                    // Round the poster off the UI thread; stash under the video path
                    // so chat bubbles reuse image_previews like stills.
                    let thumb_for_round = thumb.clone();
                    let video_path = path.clone();
                    self.video_thumbs.insert(path.clone(), thumb);
                    if !self.image_previews.contains_key(&path)
                        && !self.image_previews_pending.contains(&path)
                    {
                        self.image_previews_pending.insert(path);
                        tasks.push(Task::perform(
                            async move {
                                let preview = tokio::task::spawn_blocking(move || {
                                    media::rounded_fitted_preview(
                                        &thumb_for_round,
                                        220,
                                        280,
                                        12.0,
                                    )
                                    .ok()
                                })
                                .await
                                .ok()
                                .flatten();
                                Message::ImagePreviewReady {
                                    path: video_path,
                                    preview,
                                }
                            },
                            |m| m,
                        ));
                    }
                } else {
                    tracing::warn!(
                        path = %path.display(),
                        "video poster extraction failed"
                    );
                }
                tasks.push(self.enqueue_missing_video_thumbs());
                Task::batch(tasks)
            }
            Message::MediaSaved(Ok(summary)) => {
                self.status = summary;
                Task::none()
            }
            Message::MediaSaved(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::RefreshChats => self.reload_chats(),
            Message::RefreshChatsDebounced(gen) => {
                if gen != self.chats_refresh_gen {
                    return Task::none();
                }
                self.reload_chats()
            }
            Message::ChatsLoaded(Ok(mut chats)) => {
                for chat in &mut chats {
                    if let Some(req_id) = chat.pending_request_id {
                        if self.handled_contact_requests.contains(&req_id) {
                            chat.pending_request_id = None;
                        }
                    }
                }
                // Keep seeded channels that core still hides (`creating_in_progress`)
                // until `groupLinkDataUpdated` — but only ids still in `seeded_channel_ids`
                // (deleted channels are removed from that set so they cannot ghost).
                prune_resolved_seeds(&mut self.seeded_channel_ids, &chats);
                self.chats =
                    merge_seeded_channels(chats, &self.chats, &self.seeded_channel_ids);
                self.rebind_selection();
                if let Some(overlay) = self.contact_profile.as_mut() {
                    if let Some((idx, chat)) = self.chats.iter().enumerate().find(|(_, c)| {
                        matches!(c.id.chat_type, ChatType::Direct)
                            && c.id.api_id == overlay.contact_id
                    }) {
                        overlay.chat_index = idx;
                        overlay.title = chat.title.clone();
                        overlay.fields = chat.peer_profile.clone();
                        if !overlay.alias_dirty {
                            overlay.alias = chat.local_alias.clone();
                        }
                        overlay.muted = chat.settings.enable_ntfs.is_muted();
                        overlay.favorite = chat.settings.favorite;
                        fill_contact_feature_overlay(overlay, chat);
                    }
                }
                Task::none()
            }
            Message::ChatsLoaded(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::MessagesLoaded { gen, result } => {
                if gen != self.messages_load_gen {
                    return Task::none();
                }
                match result {
                    Ok(messages) => {
                        // Defer auto-receive until WarmChatMedia so first paint /
                        // scroll aren't competing with a burst of XFTP starts.
                        self.pending_auto_receives = auto_receive_from_messages(&messages);
                        self.messages = messages;
                        // With Anchor::End, relative y=0 is the newest (visual bottom).
                        let pin = if self.pin_history_end {
                            self.pin_history_end = false;
                            iced::widget::operation::snap_to(
                                iced::widget::Id::new("chat-history"),
                                RelativeOffset { x: 0.0, y: 0.0 },
                            )
                        } else {
                            Task::none()
                        };
                        // Paint first; media warm starts after a frame so open-chat
                        // never competes with the first scrollable layout.
                        let defer_media = Task::perform(
                            async {
                                tokio::time::sleep(std::time::Duration::from_millis(32)).await;
                                Message::WarmChatMedia
                            },
                            |m| m,
                        );
                        Task::batch([pin, defer_media])
                    }
                    Err(err) => {
                        self.status = err;
                        Task::none()
                    }
                }
            }
            Message::WarmChatMedia => {
                // Decrypt + waveforms first (unlocks plaintext paths). Posters
                // follow on WarmChatVisuals so ffmpeg does not fight decrypt.
                let auto = std::mem::take(&mut self.pending_auto_receives);
                let defer_visuals = Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                        Message::WarmChatVisuals
                    },
                    |m| m,
                );
                Task::batch([
                    self.enqueue_receives(auto),
                    self.enqueue_decrypts(),
                    self.enqueue_missing_waveforms(),
                    defer_visuals,
                ])
            }
            Message::WarmChatVisuals => Task::batch([
                self.enqueue_missing_video_thumbs(),
                self.enqueue_missing_image_previews(),
            ]),
            Message::ImagePreviewReady { path, preview } => {
                self.image_previews_pending.remove(&path);
                if let Some(preview) = preview {
                    self.image_previews.insert(path, preview);
                }
                self.enqueue_missing_image_previews()
            }
            Message::ChatMarkedRead(Ok(chat_id)) => {
                if let Some(chat) = self.chats.iter_mut().find(|c| c.id == chat_id) {
                    chat.unread = 0;
                }
                Task::none()
            }
            Message::ChatMarkedRead(Err(err)) => {
                tracing::warn!(%err, "mark chat read failed");
                Task::none()
            }
            Message::AcceptContactRequest(req_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                if self.contact_request_busy {
                    return Task::none();
                }
                // Hide Accept/Reject immediately so a slow/failed-looking reply
                // can't be double-pressed into SEInvitationNotFound.
                self.set_pending_request_visible(req_id, false);
                self.contact_request_busy = true;
                self.status = "Accepting contact request…".into();
                Task::perform(
                    accept_contact_request(session, req_id),
                    move |result| Message::ContactRequestHandled { req_id, result },
                )
            }
            Message::RejectContactRequest(req_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                if self.contact_request_busy {
                    return Task::none();
                }
                self.set_pending_request_visible(req_id, false);
                self.contact_request_busy = true;
                self.status = "Rejecting contact request…".into();
                Task::perform(
                    reject_contact_request(session, req_id),
                    move |result| Message::ContactRequestHandled { req_id, result },
                )
            }
            Message::ContactRequestHandled { req_id, result: Ok(summary) } => {
                self.contact_request_busy = false;
                self.handled_contact_requests.insert(req_id);
                self.status = summary;
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::ContactRequestHandled { req_id, result: Err(err) } => {
                self.contact_request_busy = false;
                // First accept may have consumed the invitation while the
                // command still errored — don't leave the bar stuck.
                if err.contains("SEInvitationNotFound") || err.contains("contactRequestNotFound")
                {
                    self.handled_contact_requests.insert(req_id);
                    self.status = "Contact request already handled — refreshing…".into();
                    Task::batch([self.reload_chats(), self.reload_messages()])
                } else {
                    // Real failure — put the Accept/Reject bar back.
                    self.set_pending_request_visible(req_id, true);
                    self.status = err;
                    Task::none()
                }
            }
            Message::InviteInputChanged(value) => {
                self.invite_input = value;
                Task::none()
            }
            Message::CreateInvitation => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.compose_open = true;
                self.status = "Creating one-time invite…".into();
                Task::perform(create_invitation(session), Message::InviteReady)
            }
            Message::EnsureAddress => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                self.compose_open = true;
                self.status = "Preparing contact address…".into();
                Task::perform(ensure_address(session), Message::InviteReady)
            }
            Message::AcceptInvite => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let link = self.invite_input.trim().to_string();
                if link.is_empty() {
                    self.status = "Paste an invite or address link first.".into();
                    return Task::none();
                }
                self.status = "Accepting link…".into();
                Task::perform(accept_invite(session, link), Message::InviteAccepted)
            }
            Message::InviteReady(Ok(link)) => {
                self.invite_link = Some(link.display.clone());
                self.compose_open = true;
                self.status = "Invite ready — copy and share the link.".into();
                iced::clipboard::write(link.display)
            }
            Message::InviteReady(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::InviteAccepted(Ok(summary)) => {
                self.invite_input.clear();
                self.compose_open = false;
                self.status = summary;
                self.reload_chats()
            }
            Message::InviteAccepted(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::CopyInviteLink => {
                if let Some(link) = self.invite_link.clone() {
                    self.status = "Invite link copied.".into();
                    iced::clipboard::write(link)
                } else {
                    Task::none()
                }
            }
            Message::CoreEvent(event) => self.handle_core_event(event),
            Message::PointerMoved { x, y } => {
                // Only arrives when PointerGate decided App must react.
                if !self.hover_ok {
                    self.hover_ok = true;
                }
                if x.is_finite() && y.is_finite() {
                    let dy = y - self.last_pointer.1;
                    self.last_pointer = (x, y);
                    if POINTER_GATE.moved.load(Ordering::Relaxed) {
                        self.press_moved = true;
                        self.cancel_press_timers();
                        // Touch/drag: finger down → older history; finger up → toward latest.
                        if self.selected.is_some() && self.messages.len() >= 8 {

                            if dy > 2.0 {
                                if let Some(away) = HISTORY_AWAY_GATE.mark_away() {
                                    self.history_away_from_end = away;
                                }
                            } else if dy < -2.0 {
                                if let Some(away) = HISTORY_AWAY_GATE.mark_at_end() {
                                    self.history_away_from_end = away;
                                }
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::PointerPressed { x, y } => {
                let (px, py) = if x.is_finite() && y.is_finite() {
                    (x, y)
                } else {
                    POINTER_GATE.last_xy()
                };
                self.press_anchor = Some((px, py));
                self.press_moved = false;
                self.last_pointer = (px, py);
                // Gate already began the press in the subscription.

                // Do not capture press in mouse_area — scrollable needs FingerPressed.
                // Long-press is armed against one target only (same as right-click).
                let mut tasks = Vec::new();

                // Prefer the deepest hover: reaction → message → chat row.
                if self.selected_is_group() {
                    if let Some((item_id, emoji)) = self.hover.reaction.borrow().clone() {
                        self.reaction_chip_press_generation =
                            self.reaction_chip_press_generation.wrapping_add(1);
                        self.reaction_chip_press = Some((item_id, emoji.clone()));
                        let generation = self.reaction_chip_press_generation;
                        tasks.push(Task::perform(
                            async move {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    LONG_PRESS_MS,
                                ))
                                .await;
                                Message::ReactionChipLongPress {
                                    item_id,
                                    emoji,
                                    generation,
                                }
                            },
                            |msg| msg,
                        ));
                        return Task::batch(tasks);
                    }
                }

                // Message menus on touch use HoverArea double-tap (long-press
                // was too clunky). Desktop still opens via right-click.
                if !self.selection_mode && self.hover.msg.get().is_some() {
                    // Swallow chat-row long-press while a bubble is hot.
                    return Task::batch(tasks);
                }

                // Chat-row menu only when the list is the active surface.
                let hovered_chat = self.hover.chat.get();
                let list_armed = match resolve(self.width, self.height) {
                    LayoutMode::Desktop => hovered_chat.is_some(),
                    LayoutMode::Compact | LayoutMode::Mobile => {
                        self.selected.is_none() && hovered_chat.is_some()
                    }
                };
                if list_armed {
                    if let Some(index) = hovered_chat {
                        self.long_press_generation =
                            self.long_press_generation.wrapping_add(1);
                        self.long_press_index = Some(index);
                        let generation = self.long_press_generation;
                        tasks.push(Task::perform(
                            async move {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    LONG_PRESS_MS,
                                ))
                                .await;
                                Message::ChatRowLongPress { index, generation }
                            },
                            |msg| msg,
                        ));
                    }
                }
                Task::batch(tasks)
            }
            Message::PointerTouchEnded => {
                self.hover_ok = false;
                POINTER_GATE.touch_ended();
                self.cancel_press_timers();
                self.press_anchor = None;
                // Do not leave App.press_moved stuck after a scroll: the next
                // list tap often skips PointerPressed (hover not hot yet).
                self.press_moved = false;
                Task::none()
            }
            Message::TrayActions(actions) => {
                let mut tasks = Vec::new();
                for action in actions {
                    match action {
                        desktop::TrayAction::Show => tasks.push(self.update(Message::ShowFromTray)),
                        desktop::TrayAction::Hide => tasks.push(self.update(Message::HideToTray)),
                        desktop::TrayAction::Quit => tasks.push(self.update(Message::QuitApp)),
                    }
                }
                Task::batch(tasks)
            }
            Message::ChatRowReleased(index) => {
                // Use the gate (not sticky App.press_moved): scroll sets
                // press_moved true, and the next row tap used to never clear it
                // when PointerPressed was skipped — chats became unopenable.
                let open = POINTER_GATE.tap_ok() && self.chat_menu.is_none();
                self.cancel_press_timers();
                self.press_anchor = None;
                self.press_moved = false;
                if open {
                    // Short tap — open the chat (ignored after a scroll gesture).
                    return self.update(Message::SelectChat(index));
                }
                POINTER_GATE.reset_gesture();
                Task::none()
            }
            Message::ChatRowLongPress { index, generation } => {
                if self.long_press_generation == generation
                    && self.long_press_index == Some(index)
                    && !self.press_moved
                {
                    self.long_press_index = None;
                    self.hover_ok = false;
                    self.open_chat_menu(index);
                }
                Task::none()
            }
            Message::OpenChatMenu(index) => {
                self.open_chat_menu(index);
                Task::none()
            }
            Message::Escape => {
                if self.create_chat.is_some() {
                    self.create_chat = None;
                    Task::none()
                } else if self
                    .group_settings
                    .as_ref()
                    .is_some_and(|g| !matches!(g.pane, GroupSettingsPane::Overview))
                {
                    if let Some(g) = self.group_settings.as_mut() {
                        g.pane = GroupSettingsPane::Overview;
                        g.selected_member_id = None;
                    }
                    Task::none()
                } else if self.group_settings.is_some() {
                    // Esc: discard dirty group profile / alias, don't save.
                    self.group_settings = None;
                    Task::none()
                } else if self.contact_profile.is_some() {
                    // Esc: leave without committing alias.
                    self.contact_profile = None;
                    Task::none()
                } else if self.nav == NavSection::Settings && self.own_profile.dirty {
                    // Esc: discard settings edits (do not submit).
                    if let Some(user) = self.session.as_ref().and_then(|s| s.active_profile()) {
                        self.own_profile = OwnProfileDraft {
                            fields: user.profile,
                            dirty: false,
                        };
                    } else {
                        self.own_profile.dirty = false;
                    }
                    Task::none()
                } else if self.compose_open {
                    self.compose_open = false;
                    Task::none()
                } else if self.reaction_members.is_some() {
                    self.reaction_members = None;
                    Task::none()
                } else if self.message_menu.is_some() {
                    self.message_menu = None;
                    Task::none()
                } else if self.pending_url.is_some() {
                    self.pending_url = None;
                    Task::none()
                } else if self.editing.is_some() {
                    self.editing = None;
                    self.clear_composer();
                    Task::none()
                } else if self.reply_to.is_some() {
                    self.reply_to = None;
                    Task::none()
                } else if self.selection_mode {
                    self.selection_mode = false;
                    self.selected_items.clear();
                    Task::none()
                } else if cursus_core::pending_shortcode_query(&self.composer_text()).is_some() {
                    // Drop the unfinished `:prefix` so suggestions close.
                    let mut text = self.composer_text();
                    if let Some(start) = text.rfind(':') {
                        text.truncate(start);
                        self.set_composer_text(text);
                    }
                    self.emoji_suggest_index = 0;
                    Task::none()
                } else {
                    self.chat_menu = None;
                    Task::none()
                }
            }
            Message::CloseChatMenu => {
                // Esc / backdrop tap — no-op when the menu is already closed.
                self.chat_menu = None;
                Task::none()
            }
            Message::ChatMenuAction(action) => self.handle_chat_menu_action(action),
            Message::ChatMenuDoClear => self.confirm_clear_chat(),
            Message::ChatMenuDoDelete => self.confirm_delete_chat(),
            Message::ChatMenuCancelConfirm => {
                if let Some(menu) = self.chat_menu.as_mut() {
                    menu.pane = ChatMenuPane::Actions;
                }
                Task::none()
            }
            Message::ChatMenuOpenLists(Ok(tags)) => {
                if let Some(menu) = self.chat_menu.as_mut() {
                    menu.pane = ChatMenuPane::Lists {
                        tags,
                        new_name: String::new(),
                    };
                }
                Task::none()
            }
            Message::ChatMenuOpenLists(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ChatMenuNewListChanged(value) => {
                if let Some(ChatContextMenu {
                    pane: ChatMenuPane::Lists { new_name, .. },
                    ..
                }) = self.chat_menu.as_mut()
                {
                    *new_name = value;
                }
                Task::none()
            }
            Message::ChatMenuCreateList => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(ChatContextMenu {
                    pane: ChatMenuPane::Lists { new_name, .. },
                    ..
                }) = self.chat_menu.as_ref()
                else {
                    return Task::none();
                };
                let name = new_name.trim().to_string();
                if name.is_empty() {
                    self.status = "Enter a list name first.".into();
                    return Task::none();
                }
                self.status = "Creating list…".into();
                Task::perform(create_chat_tag(session, name), Message::ChatMenuOpenLists)
            }
            Message::ChatMenuToggleTag(tag_id) => {
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(menu) = self.chat_menu.as_ref() else {
                    return Task::none();
                };
                let Some(chat) = self.chats.iter().find(|c| c.id == menu.chat_id).cloned() else {
                    return Task::none();
                };
                let mut tag_ids = chat.tag_ids.clone();
                if let Some(pos) = tag_ids.iter().position(|id| *id == tag_id) {
                    tag_ids.remove(pos);
                } else {
                    tag_ids.push(tag_id);
                }
                let chat_id = chat.id.clone();
                self.status = "Updating lists…".into();
                // Keep the lists pane open; only refresh chats so checkmarks update.
                Task::perform(set_chat_tags(session, chat_id, tag_ids), |result| {
                    match result {
                        Ok(summary) => Message::ChatMenuTagsUpdated(Ok(summary)),
                        Err(err) => Message::ChatMenuTagsUpdated(Err(err)),
                    }
                })
            }
            Message::ChatMenuTagsUpdated(Ok(summary)) => {
                self.status = summary;
                self.reload_chats()
            }
            Message::ChatMenuTagsUpdated(Err(err)) => {
                self.status = err;
                Task::none()
            }
            Message::ChatMenuDone(Ok(summary)) => {
                self.pending_delete_id = None;
                self.status = summary;
                self.chat_menu = None;
                Task::batch([self.reload_chats(), self.reload_messages()])
            }
            Message::ChatMenuDone(Err(err)) => {
                if store_already_gone(&err) {
                    if let Some(id) = self.pending_delete_id.take() {
                        self.forget_chat_locally(&id);
                    }
                    self.chat_menu = None;
                    self.status = "Deleted.".into();
                    return self.reload_chats();
                }
                self.pending_delete_id = None;
                self.status = err;
                self.reload_chats()
            }
        }
    }

    /// Remove a chat from local UI state (seed set + list + selection).
    fn forget_chat_locally(&mut self, id: &ChatId) {
        self.seeded_channel_ids.remove(&id.api_id);
        self.chats.retain(|c| &c.id != id);
        if self.selected_id.as_ref() == Some(id) {
            self.selected = None;
            self.selected_id = None;
            self.messages.clear();
        }
        self.rebind_selection();
    }

    fn open_chat_menu(&mut self, index: usize) {
        let Some(chat) = self.chats.get(index) else {
            return;
        };
        self.message_menu = None;
        self.chat_menu = Some(ChatContextMenu {
            index,
            chat_id: chat.id.clone(),
            title: chat.title.clone(),
            pane: ChatMenuPane::Actions,
        });
    }

    fn handle_chat_menu_action(&mut self, action: ChatMenuAction) -> Task<Message> {
        let Some(menu) = self.chat_menu.clone() else {
            return Task::none();
        };
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(chat) = self.chats.iter().find(|c| c.id == menu.chat_id).cloned() else {
            self.chat_menu = None;
            return Task::none();
        };

        match action {
            ChatMenuAction::ViewContact => {
                self.chat_menu = None;
                if let Some(index) = self.chats.iter().position(|c| c.id == menu.chat_id) {
                    self.selected = Some(index);
                    self.selected_id = Some(menu.chat_id);
                }
                self.open_contact_profile()
            }
            ChatMenuAction::ViewGroup => {
                self.chat_menu = None;
                if let Some(index) = self.chats.iter().position(|c| c.id == menu.chat_id) {
                    self.selected = Some(index);
                    self.selected_id = Some(menu.chat_id);
                }
                self.open_group_settings()
            }
            ChatMenuAction::MarkRead => {
                self.status = "Marking as read…".into();
                // Optimistic badge clear.
                if let Some(row) = self.chats.iter_mut().find(|c| c.id == chat.id) {
                    row.unread = 0;
                    row.unread_chat = false;
                }
                Task::perform(mark_chat_read(session, chat.id), |result| {
                    Message::ChatMenuDone(match result {
                        Ok(_) => Ok("Marked as read.".into()),
                        Err(err) => Err(err),
                    })
                })
            }
            ChatMenuAction::MarkUnread => {
                self.status = "Marking as unread…".into();
                if let Some(row) = self.chats.iter_mut().find(|c| c.id == chat.id) {
                    row.unread_chat = true;
                    if row.unread == 0 {
                        row.unread = 1;
                    }
                }
                Task::perform(mark_chat_unread(session, chat.id), Message::ChatMenuDone)
            }
            ChatMenuAction::ToggleFavorite => {
                let mut settings = chat.settings.clone();
                settings.favorite = !settings.favorite;
                let label = if settings.favorite {
                    "Added to favorites."
                } else {
                    "Removed from favorites."
                };
                self.status = "Updating favorite…".into();
                Task::perform(
                    set_chat_settings(session, chat.id, settings, label),
                    Message::ChatMenuDone,
                )
            }
            ChatMenuAction::ToggleMute => {
                let mut settings = chat.settings.clone();
                settings.enable_ntfs = settings.enable_ntfs.next_direct();
                let label = if settings.enable_ntfs.is_muted() {
                    "Chat muted."
                } else {
                    "Chat unmuted."
                };
                self.status = "Updating notifications…".into();
                Task::perform(
                    set_chat_settings(session, chat.id, settings, label),
                    Message::ChatMenuDone,
                )
            }
            ChatMenuAction::AddToList => {
                self.status = "Loading lists…".into();
                Task::perform(load_chat_tags(session), Message::ChatMenuOpenLists)
            }
            ChatMenuAction::Clear => {
                if let Some(menu) = self.chat_menu.as_mut() {
                    menu.pane = ChatMenuPane::ConfirmClear;
                }
                Task::none()
            }
            ChatMenuAction::Delete => {
                if let Some(menu) = self.chat_menu.as_mut() {
                    menu.pane = ChatMenuPane::ConfirmDelete;
                }
                Task::none()
            }
        }
    }

    pub(crate) fn confirm_clear_chat(&mut self) -> Task<Message> {
        let Some(menu) = self.chat_menu.clone() else {
            return Task::none();
        };
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        self.status = "Clearing chat…".into();
        Task::perform(clear_chat(session, menu.chat_id), Message::ChatMenuDone)
    }

    pub(crate) fn confirm_delete_chat(&mut self) -> Task<Message> {
        let Some(menu) = self.chat_menu.clone() else {
            return Task::none();
        };
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        self.pending_delete_id = Some(menu.chat_id.clone());
        self.forget_chat_locally(&menu.chat_id);
        self.chat_menu = None;
        self.status = "Deleting chat…".into();
        Task::perform(delete_chat(session, menu.chat_id), Message::ChatMenuDone)
    }

    fn handle_core_event(&mut self, event: Event) -> Task<Message> {
        let mut tasks = Vec::new();
        // Avoid double `reload_messages` when NewChatItems also matches
        // `refreshes_messages()` (busy groups used to schedule 2× get_chat).
        let mut messages_reload_queued = false;

        // Official clients auto-accept image/voice/video offers on newChatItems.
        let auto = auto_receive_candidates(&event);
        if !auto.is_empty() {
            tasks.push(self.enqueue_receives(auto));
        }

        if let Event::NewChatItems(_) = &event {
            self.maybe_notify_new_items(&event);
            if let Some(hint) = event.chat_id_hint() {
                if self.selected_id.as_ref() == Some(&hint) {
                    tasks.push(self.reload_messages());
                    messages_reload_queued = true;
                    // Viewing this chat — don't leave an unread badge hanging.
                    tasks.push(self.mark_selected_read());
                    if let Some(chat) = self.chats.iter_mut().find(|c| c.id == hint) {
                        chat.unread = 0;
                    }
                }
            } else if self.selected_id.is_some() {
                // Unknown chat targeting — refresh open conversation just in case.
                tasks.push(self.reload_messages());
                messages_reload_queued = true;
                tasks.push(self.mark_selected_read());
            }
        }

        // Apply delivery/read receipt ticks without a full history reload when possible.
        let status_updates = event.status_updates();
        if !status_updates.is_empty() {
            let selected = self.selected_id.clone();
            let mut touched_open = false;
            for (chat_id, item_id, status) in &status_updates {
                if selected.as_ref() == Some(chat_id) {
                    if let Some(msg) = self.messages.iter_mut().find(|m| m.item_id == *item_id) {
                        msg.delivery = *status;
                        // Outgoing file rows often lag on XFTP status; receipt
                        // progress means the transfer finished from our POV.
                        if msg.outgoing {
                            if let Some(file) = msg.file.as_mut() {
                                if matches!(
                                    status,
                                    cursus_core::DeliveryStatus::Sent
                                        | cursus_core::DeliveryStatus::Delivered
                                ) {
                                    file.state = cursus_core::FileTransferState::Complete;
                                }
                            }
                        }
                        touched_open = true;
                    } else if !messages_reload_queued {
                        touched_open = false;
                        tasks.push(self.reload_messages());
                        break;
                    }
                }
            }
            if touched_open {
                tracing::debug!("applied delivery status update(s)");
            }
        } else if let Some((file_id, transferred, total)) = event.file_progress_updates() {
            // Patch progress in place — avoids scroll jumps on every XFTP tick.
            let mut touched = false;
            for msg in self.messages.iter_mut() {
                if let Some(file) = msg.file.as_mut() {
                    if file.file_id == file_id {
                        file.state = cursus_core::FileTransferState::InProgress;
                        file.transferred = Some(transferred);
                        file.transfer_total = Some(total);
                        if file.file_size <= 0 {
                            file.file_size = total;
                        }
                        touched = true;
                    }
                }
            }
            if !touched && self.selected_id.is_some() && !messages_reload_queued {
                tasks.push(self.reload_messages());
            }
        } else if event.refreshes_messages()
            && self.selected_id.is_some()
            && !messages_reload_queued
        {
            // File complete / delete / reaction events carry updated rows.
            tasks.push(self.reload_messages());
        }

        if event.refreshes_chats() {
            // Debounce list_chats — NewChatItems / reactions can arrive in bursts
            // and each full reload blocks a worker + rebuilds the chat list UI.
            tasks.push(self.schedule_reload_chats());
            match &event {
                Event::ContactConnected(body) => {
                    self.status = "Contact connected.".into();
                    // Hide Accept/Reject immediately; list refresh confirms via
                    // connStatus (tagged JSON object — see pending_accept_request_id).
                    if let Some(req_id) = body
                        .pointer("/contact/contactRequestId")
                        .or_else(|| body.get("contactRequestId"))
                        .and_then(|v| v.as_i64())
                    {
                        self.set_pending_request_visible(req_id, false);
                    } else {
                        // Fallback: drop pending on the open chat.
                        if let Some(i) = self.selected {
                            if let Some(chat) = self.chats.get_mut(i) {
                                chat.pending_request_id = None;
                            }
                        }
                    }
                }
                Event::ReceivedContactRequest(_) => {
                    self.status =
                        "Incoming contact request — open the chat and Accept.".into();
                }
                Event::NewChatItems(_) => {
                    // Keep status quiet for high-frequency message events.
                }
                Event::Other { type_name, body } => {
                    tracing::debug!(%type_name, "core event");
                    match type_name.as_str() {
                        "callInvitation" => {
                            if let Some(call) = parse_call_invitation(body, false) {
                                self.incoming_call = Some(call.clone());
                                self.status = format!("Incoming call from {}.", call.peer_name);
                                self.nav = NavSection::Calls;
                            }
                        }
                        "callEnded" => {
                            self.active_call = None;
                            self.incoming_call = None;
                            self.status = "Call ended.".into();
                        }
                        "rcvFileComplete" => {
                            let name = body
                                .pointer("/chatItem/chatItem/file/fileName")
                                .or_else(|| body.pointer("/chatItem/file/fileName"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("file");
                            if let Some(id) = body
                                .pointer("/chatItem/chatItem/file/fileId")
                                .or_else(|| body.pointer("/chatItem/file/fileId"))
                                .and_then(|v| v.as_i64())
                            {
                                self.receiving_files.remove(&id);
                            }
                            self.status = format!("Saved {name} to {}", files_dir().display());
                            // Accept only starts the transfer — refresh when bytes land
                            // so previews / open / decrypt see Complete + local_path.
                            tasks.push(self.reload_messages());
                        }
                        "rcvFileError" | "rcvFileWarning" => {
                            if let Some(id) = body
                                .pointer("/rcvFileTransfer/fileId")
                                .or_else(|| body.pointer("/chatItem/file/fileId"))
                                .and_then(|v| v.as_i64())
                            {
                                self.receiving_files.remove(&id);
                            }
                            self.status = "File download failed — try Download again.".into();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        Task::batch(tasks)
    }

    fn enqueue_receives(&mut self, files: Vec<(i64, String)>) -> Task<Message> {
        let mut tasks = Vec::new();
        for (file_id, file_name) in files {
            tasks.push(self.enqueue_receive(file_id, file_name));
        }
        Task::batch(tasks)
    }

    fn enqueue_receive(&mut self, file_id: i64, file_name: String) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        if !self.receiving_files.insert(file_id) {
            return Task::none();
        }
        self.status = format!("Downloading {file_name}…");
        Task::perform(
            receive_file(session, file_id, file_name),
            Message::FileReceived,
        )
    }

    fn set_pending_request_visible(&mut self, req_id: i64, visible: bool) {
        if visible {
            if let Some(selected) = self.selected_id.clone() {
                if let Some(chat) = self.chats.iter_mut().find(|c| c.id == selected) {
                    chat.pending_request_id = Some(req_id);
                }
            }
            return;
        }
        for chat in &mut self.chats {
            if chat.pending_request_id == Some(req_id) {
                chat.pending_request_id = None;
            }
        }
    }

    fn reload_chats(&self) -> Task<Message> {
        match self.session.clone() {
            Some(session) => Task::perform(load_chats(session), Message::ChatsLoaded),
            None => Task::none(),
        }
    }

    /// Coalesce rapid `list_chats` requests from the event stream.
    fn schedule_reload_chats(&mut self) -> Task<Message> {
        self.chats_refresh_gen = self.chats_refresh_gen.wrapping_add(1);
        let gen = self.chats_refresh_gen;
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(180)).await;
                gen
            },
            Message::RefreshChatsDebounced,
        )
    }

    fn reload_messages(&mut self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(id) = self.selected_id.clone() else {
            return Task::none();
        };
        self.messages_load_gen = self.messages_load_gen.wrapping_add(1);
        let gen = self.messages_load_gen;
        Task::perform(
            async move {
                let result = load_messages(session, id).await;
                Message::MessagesLoaded { gen, result }
            },
            |m| m,
        )
    }

    fn mark_selected_read(&self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(id) = self.selected_id.clone() else {
            return Task::none();
        };
        Task::perform(mark_chat_read(session, id), Message::ChatMarkedRead)
    }

    fn rebind_selection(&mut self) {
        if let Some(id) = self.selected_id.clone() {
            self.selected = self.chats.iter().position(|c| c.id.same_base(&id));
            if self.selected.is_none() {
                self.selected_id = None;
                self.support = None;
                self.messages.clear();
            }
        }
    }

    /// Chat list row for the open conversation, with `selected_id` scope applied
    /// (support threads use `#gid(_support)` / `#gid(_support:gmId)`).
    fn selected_chat_for_api(&self) -> Option<ChatPreview> {
        let mut chat = self.selected.and_then(|i| self.chats.get(i).cloned())?;
        if let Some(id) = self.selected_id.clone() {
            chat.id = id;
        }
        Some(chat)
    }

    fn apply_appearance(&self) {
        theme::set_light_active(self.desktop_prefs.theme.resolve_light());
    }

    fn iced_theme(&self) -> Theme {
        self.apply_appearance();
        let tokens = theme::t();
        Theme::custom(tokens.iced_theme_name(), tokens.iced_palette())
    }

    fn view(&self, _window: iced::window::Id) -> Element<'_, Message> {
        self.apply_appearance();
        container(shell(self))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().bg_app)),
                ..container::Style::default()
            })
            .into()
    }

    pub(crate) fn nav(&self) -> NavSection {
        self.nav
    }
    pub(crate) fn chats(&self) -> &[ChatPreview] {
        &self.chats
    }
    pub(crate) fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub(crate) fn composer(&self) -> String {
        self.composer_text()
    }

    pub(crate) fn composer_content(&self) -> &text_editor::Content {
        &self.composer
    }

    fn composer_text(&self) -> String {
        // `Content::text` always joins with `\n` and forces a trailing newline.
        self.composer.text().trim_end_matches('\n').to_string()
    }

    fn set_composer_text(&mut self, text: impl AsRef<str>) {
        self.composer = text_editor::Content::with_text(text.as_ref());
        self.move_composer_cursor_to_end();
    }

    fn move_composer_cursor_to_end(&mut self) {
        let line = self.composer.line_count().saturating_sub(1);
        // cosmic_text cursor index is a byte offset into the line.
        let column = self
            .composer
            .line(line)
            .map(|l| l.text.len())
            .unwrap_or(0);
        self.composer.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column },
            selection: None,
        });
    }

    fn clear_composer(&mut self) {
        self.composer = text_editor::Content::new();
    }

    /// Reset suggestion highlight when the pending `:prefix` changes; otherwise
    /// keep the index and clamp it to the current list.
    fn sync_emoji_suggest_index(&mut self, before_prefix: Option<&str>) {
        let text = self.composer_text();
        let after = cursus_core::pending_shortcode_query(&text);
        if before_prefix != after {
            self.emoji_suggest_index = 0;
            return;
        }
        let n = self.emoji_suggestions().len();
        if n == 0 {
            self.emoji_suggest_index = 0;
        } else {
            self.emoji_suggest_index = self.emoji_suggest_index.min(n - 1);
        }
    }

    pub(crate) fn voice_recording(&self) -> bool {
        self.voice_rec.is_some()
    }

    pub(crate) fn voice_draft(&self) -> Option<&VoiceDraft> {
        self.voice_draft.as_ref()
    }

    pub(crate) fn voice_playback_progress(&self, file_id: i64) -> Option<f32> {
        let play = self.voice_playback.as_ref()?;
        if play.file_id != file_id {
            return None;
        }
        let dur = play.duration_secs.max(1) as f32;
        Some((play.started.elapsed().as_secs_f32() / dur).clamp(0.0, 1.0))
    }

    pub(crate) fn waveform_for(&self, path: &PathBuf) -> Option<&[f32]> {
        self.waveforms.get(path).map(|v| v.as_slice())
    }

    /// Poster for a video path — **memory only** on the view path.
    /// Disk thumbs are loaded asynchronously via [`Self::enqueue_missing_video_thumbs`].
    pub(crate) fn video_thumb_for(&self, path: &std::path::Path) -> Option<PathBuf> {
        self.video_thumbs.get(path).cloned()
    }

    /// Rounded still preview — **memory only** (never probes disk / image headers
    /// during `view()`, which ran on every fling tick).
    pub(crate) fn image_preview_for(
        &self,
        path: &std::path::Path,
        _max_w: u32,
        _max_h: u32,
    ) -> Option<(PathBuf, u32, u32)> {
        self.image_previews.get(path).cloned()
    }

    pub(crate) fn auth(&self) -> Option<&AuthScreen> {
        self.auth.as_ref()
    }

    pub(crate) fn status_text(&self) -> &str {
        self.status.as_str()
    }

    /// Plaintext path for previews / open (decrypt cache or original).
    pub(crate) fn plaintext_path_for(&self, file: &cursus_core::FileInfo) -> Option<PathBuf> {
        let path = file.local_path.clone()?;
        if !file.encrypted {
            return Some(path);
        }
        self.plaintext_files.get(&path).cloned()
    }

    fn begin_connect(&mut self, req: ConnectRequest) -> Task<Message> {
        if self.connecting || self.session.is_some() {
            return Task::none();
        }
        self.connecting = true;
        self.status = "Unlocking encrypted database…".into();
        let slot = Arc::clone(&self.pending_session);
        Task::perform(connect_session(slot, req), Message::Connected)
    }

    fn enqueue_decrypts(&self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        // Newest media first, limited parallelism — decrypting every CryptoFile
        // in a 100-message history blocked the UI when opening a chat.
        const MAX_DECRYPTS: usize = 8;
        let mut jobs = Vec::new();
        for msg in self.messages.iter().rev() {
            if jobs.len() >= MAX_DECRYPTS {
                break;
            }
            let Some(file) = msg.file.as_ref() else {
                continue;
            };
            if !file.encrypted {
                continue;
            }
            let Some(path) = file.local_path.clone() else {
                continue;
            };
            if self.plaintext_files.contains_key(&path) {
                continue;
            }
            let Some(key) = file.crypto_key.clone() else {
                continue;
            };
            let Some(nonce) = file.crypto_nonce.clone() else {
                continue;
            };
            jobs.push((path, key, nonce));
        }
        let tasks = jobs
            .into_iter()
            .map(|(path, key, nonce)| {
                let session = session.clone();
                Task::perform(
                    async move {
                        let encrypted = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            session
                                .ensure_plaintext_file(&path, &key, &nonce)
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r);
                        Message::DecryptFileDone { encrypted, result }
                    },
                    |msg| msg,
                )
            })
            .collect::<Vec<_>>();
        Task::batch(tasks)
    }

    fn toggle_voice_record(&mut self) -> Task<Message> {
        if self.voice_draft.is_some() {
            self.status = "Send or cancel the current voice draft first.".into();
            return Task::none();
        }
        if self.voice_rec.is_some() {
            return self.stop_voice_to_draft();
        }
        if self.session.is_none() {
            self.status = "Connect before recording.".into();
            return Task::none();
        }
        let Some(chat) = self.selected_chat_for_api() else {
            self.status = "Select a chat before recording.".into();
            return Task::none();
        };
        if chat.pending_request_id.is_some() {
            self.status = "Accept the contact request before sending voice.".into();
            return Task::none();
        }
        match start_voice_recording() {
            Ok(rec) => {
                self.status = "Recording… tap stop when done.".into();
                self.voice_rec = Some(rec);
                Task::none()
            }
            Err(err) => {
                self.status = err;
                Task::none()
            }
        }
    }

    fn stop_voice_to_draft(&mut self) -> Task<Message> {
        let Some(mut rec) = self.voice_rec.take() else {
            return Task::none();
        };
        self.status = "Preparing voice…".into();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    stop_ffmpeg(&mut rec.child);
                    let secs = rec.started.elapsed().as_secs().max(1).min(300) as u32;
                    let meta = std::fs::metadata(&rec.path).map_err(|e| e.to_string())?;
                    if meta.len() < 64 {
                        let _ = std::fs::remove_file(&rec.path);
                        return Err("Recording was empty — check microphone / PulseAudio.".into());
                    }
                    let peaks = media::extract_waveform_peaks(&rec.path, media::WAVEFORM_BARS)
                        .unwrap_or_else(|_| vec![0.35; media::WAVEFORM_BARS]);
                    Ok(VoiceDraft {
                        path: rec.path,
                        duration_secs: secs,
                        peaks,
                    })
                })
                .await
                .map_err(|e| e.to_string())?
            },
            Message::VoiceDraftReady,
        )
    }

    fn send_voice_draft(&mut self) -> Task<Message> {
        let Some(draft) = self.voice_draft.clone() else {
            return Task::none();
        };
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(chat) = self.selected_chat_for_api() else {
            self.status = "Select a chat before sending voice.".into();
            return Task::none();
        };
        self.status = "Sending voice…".into();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    session
                        .send_voice(&chat.id, &draft.path.to_string_lossy(), draft.duration_secs)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())?
            },
            Message::VoiceSent,
        )
    }

    fn play_voice_bubble(
        &mut self,
        file_id: i64,
        path: PathBuf,
        duration_secs: u32,
    ) -> Task<Message> {
        if self
            .voice_playback
            .as_ref()
            .is_some_and(|p| p.file_id == file_id)
        {
            self.stop_voice_playback();
            return Task::none();
        }
        self.stop_voice_playback();
        match spawn_audio_player(&path) {
            Ok(child) => {
                self.voice_playback = Some(VoicePlayback {
                    file_id,
                    path: path.clone(),
                    duration_secs: duration_secs.max(1),
                    started: Instant::now(),
                    child,
                });
                self.status = format!("Playing {}…", path.display());
                if !self.waveforms.contains_key(&path) {
                    return self.load_waveform(path);
                }
            }
            Err(err) => {
                self.status = err;
            }
        }
        Task::none()
    }

    fn stop_voice_playback(&mut self) {
        if let Some(mut play) = self.voice_playback.take() {
            let _ = play.child.kill();
            let _ = play.child.wait();
        }
    }

    fn load_waveform(&self, path: PathBuf) -> Task<Message> {
        Task::perform(
            async move {
                let peaks = tokio::task::spawn_blocking(move || {
                    let path2 = path.clone();
                    (
                        path2,
                        media::extract_waveform_peaks(&path, media::WAVEFORM_BARS),
                    )
                })
                .await
                .map_err(|e| e.to_string());
                match peaks {
                    Ok((path, peaks)) => Message::WaveformReady { path, peaks },
                    Err(e) => Message::WaveformReady {
                        path: PathBuf::new(),
                        peaks: Err(e),
                    },
                }
            },
            |m| m,
        )
    }

    fn enqueue_missing_image_previews(&mut self) -> Task<Message> {
        const MAX: usize = 4;
        const PREVIEW_MAX_W: u32 = 220;
        const PREVIEW_MAX_H: u32 = 280;
        const RADIUS: f32 = 12.0;
        // Snapshot paths first — avoids cloning ChatMessage rows and borrow fights.
        let mut candidates = Vec::new();
        for msg in self.messages.iter().rev() {
            if candidates.len() >= MAX * 2 {
                break;
            }
            let Some(file) = msg.file.as_ref() else {
                continue;
            };
            if !matches!(
                effective_complete(file.state, msg.outgoing, msg.delivery),
                FileTransferState::Complete
            ) {
                continue;
            }
            let is_image = matches!(file.kind, cursus_core::FileKind::Image)
                || media::is_image_name(&file.file_name);
            let is_video = matches!(file.kind, cursus_core::FileKind::Video)
                || media::is_video_name(&file.file_name);
            if !is_image || is_video {
                continue;
            }
            let Some(path) = self.plaintext_path_for(file) else {
                continue;
            };
            candidates.push(path);
        }
        let mut tasks = Vec::new();
        for path in candidates {
            if tasks.len() >= MAX {
                break;
            }
            if !path.is_file() {
                continue;
            }
            if let Some(cached) =
                media::cached_rounded_fitted_preview(&path, PREVIEW_MAX_W, PREVIEW_MAX_H)
            {
                self.image_previews.insert(path, cached);
                continue;
            }
            if self.image_previews.contains_key(&path)
                || self.image_previews_pending.contains(&path)
            {
                continue;
            }
            self.image_previews_pending.insert(path.clone());
            tasks.push(Task::perform(
                async move {
                    let path_for_work = path.clone();
                    let preview = tokio::task::spawn_blocking(move || {
                        media::rounded_fitted_preview(
                            &path_for_work,
                            PREVIEW_MAX_W,
                            PREVIEW_MAX_H,
                            RADIUS,
                        )
                        .ok()
                    })
                    .await
                    .ok()
                    .flatten();
                    Message::ImagePreviewReady { path, preview }
                },
                |msg| msg,
            ));
        }
        Task::batch(tasks)
    }

    fn enqueue_missing_video_thumbs(&mut self) -> Task<Message> {
        // Newest first, hard cap — spawning ffmpeg for every video in history
        // made opening a media-heavy chat hang for seconds.
        const MAX_THUMBS: usize = 4;
        let mut candidates = Vec::new();
        for msg in self.messages.iter().rev() {
            if candidates.len() >= MAX_THUMBS * 2 {
                break;
            }
            let Some(file) = msg.file.as_ref() else {
                continue;
            };
            if !matches!(
                effective_complete(file.state, msg.outgoing, msg.delivery),
                FileTransferState::Complete
            ) {
                continue;
            }
            let is_video = matches!(file.kind, cursus_core::FileKind::Video)
                || media::is_video_name(&file.file_name);
            if !is_video {
                continue;
            }
            let Some(path) = self.plaintext_path_for(file) else {
                continue;
            };
            candidates.push(path);
        }
        let mut tasks = Vec::new();
        for path in candidates {
            if tasks.len() >= MAX_THUMBS {
                break;
            }
            if !path.is_file() {
                continue;
            }
            if self.video_thumbs_pending.contains(&path) {
                continue;
            }
            if let Some(cached) = media::cached_video_thumbnail(&path) {
                self.video_thumbs.insert(path.clone(), cached.clone());
                if self.image_previews.contains_key(&path) {
                    continue;
                }
                if let Some(rounded) =
                    media::cached_rounded_fitted_preview(&cached, 220, 280)
                {
                    self.image_previews.insert(path, rounded);
                    continue;
                }
                if self.image_previews_pending.contains(&path) {
                    continue;
                }
                self.image_previews_pending.insert(path.clone());
                let video_path = path;
                tasks.push(Task::perform(
                    async move {
                        let preview = tokio::task::spawn_blocking(move || {
                            media::rounded_fitted_preview(&cached, 220, 280, 12.0).ok()
                        })
                        .await
                        .ok()
                        .flatten();
                        Message::ImagePreviewReady {
                            path: video_path,
                            preview,
                        }
                    },
                    |msg| msg,
                ));
                continue;
            }
            if self.video_thumbs.contains_key(&path) {
                continue;
            }
            self.video_thumbs_pending.insert(path.clone());
            tasks.push(Task::perform(
                async move {
                    let path_for_work = path.clone();
                    let thumb = tokio::task::spawn_blocking(move || {
                        media::cached_video_thumbnail(&path_for_work)
                            .or_else(|| media::video_thumbnail(&path_for_work).ok())
                    })
                    .await
                    .ok()
                    .flatten();
                    Message::VideoThumbReady { path, thumb }
                },
                |msg| msg,
            ));
        }
        Task::batch(tasks)
    }

    fn enqueue_missing_waveforms(&self) -> Task<Message> {
        // Cap like video thumbs — waveform extract is also spawn_blocking/ffmpeg.
        const MAX_WAVES: usize = 6;
        let mut paths = Vec::new();
        for msg in self.messages.iter().rev() {
            if paths.len() >= MAX_WAVES {
                break;
            }
            let Some(file) = msg.file.as_ref() else {
                continue;
            };
            if !matches!(file.kind, cursus_core::FileKind::Voice) {
                continue;
            }
            let Some(path) = self.plaintext_path_for(file) else {
                continue;
            };
            if !path.is_file() || self.waveforms.contains_key(&path) {
                continue;
            }
            paths.push(path);
        }
        Task::batch(paths.into_iter().map(|p| self.load_waveform(p)).collect::<Vec<_>>())
    }

    pub(crate) fn connected(&self) -> bool {
        self.session.is_some()
    }
    pub(crate) fn connecting(&self) -> bool {
        self.connecting
    }
    pub(crate) fn invite_input(&self) -> &str {
        &self.invite_input
    }
    pub(crate) fn invite_link(&self) -> Option<&str> {
        self.invite_link.as_deref()
    }
    pub(crate) fn compose_open(&self) -> bool {
        self.compose_open
    }
    pub(crate) fn layout(&self) -> LayoutMode {
        resolve(self.width, self.height)
    }
    pub(crate) fn density(&self) -> Density {
        Density::for_window(self.layout(), self.width, self.height)
            .with_text_scale(self.desktop_prefs.text_size.scale())
    }
    pub(crate) fn hover_ok(&self) -> bool {
        // Gate clears on finger-up without always publishing a Message.
        self.hover_ok && POINTER_GATE.hover_ok.load(Ordering::Relaxed)
    }
    pub(crate) fn window_width(&self) -> f32 {
        self.width
    }
    pub(crate) fn chat_menu(&self) -> Option<&ChatContextMenu> {
        self.chat_menu.as_ref()
    }
    pub(crate) fn pending_url(&self) -> Option<&str> {
        self.pending_url.as_deref()
    }

    pub(crate) fn emoji_suggestions(&self) -> Vec<cursus_core::EmojiSuggestion> {
        let text = self.composer_text();
        let Some(prefix) = cursus_core::pending_shortcode_query(&text) else {
            return Vec::new();
        };
        // Cap at 5 so the mobile suggest panel can keep a fixed height
        // (variable row counts were making the composer wiggle on ARM).
        cursus_core::suggest_emoji_shortcodes(prefix, 5)
    }

    pub(crate) fn emoji_suggest_index(&self) -> usize {
        self.emoji_suggest_index
    }

    pub(crate) fn message_menu(&self) -> Option<&MessageMenu> {
        self.message_menu.as_ref()
    }

    pub(crate) fn reply_to(&self) -> Option<&ReplyTarget> {
        self.reply_to.as_ref()
    }

    pub(crate) fn editing(&self) -> Option<&EditTarget> {
        self.editing.as_ref()
    }

    fn begin_edit_message(&mut self, msg: &ChatMessage) -> Task<Message> {
        if !msg.can_edit_text() {
            self.status = if !msg.outgoing {
                "Only your own messages can be edited.".into()
            } else if msg.is_deleted() {
                "Deleted messages can't be edited.".into()
            } else if msg.file.is_some() {
                "File and media messages can't be edited.".into()
            } else {
                "This message can no longer be edited.".into()
            };
            return Task::none();
        }
        self.reply_to = None;
        self.editing = Some(EditTarget {
            item_id: msg.item_id,
            preview: truncate_preview(&msg.text, 80),
        });
        self.set_composer_text(&msg.text);
        iced::widget::operation::focus(iced::widget::Id::new("composer-input"))
    }

    fn begin_edit_last_outgoing(&mut self) -> Task<Message> {
        if self.selected_chat_for_api().is_none() {
            return Task::none();
        }
        let last = self
            .messages
            .iter()
            .rev()
            .find(|m| m.outgoing && !m.is_announcement && m.group_invitation.is_none())
            .cloned();
        let Some(msg) = last else {
            self.status = "No sent messages to edit.".into();
            return Task::none();
        };
        if !msg.can_edit_text() {
            self.status = if msg.file.is_some() {
                "Latest sent message is a file and can't be edited.".into()
            } else if msg.is_deleted() {
                "Latest sent message was deleted.".into()
            } else {
                "Latest sent message can no longer be edited.".into()
            };
            return Task::none();
        }
        self.begin_edit_message(&msg)
    }

    pub(crate) fn selection_mode(&self) -> bool {
        self.selection_mode
    }

    pub(crate) fn selected_items(&self) -> &HashSet<i64> {
        &self.selected_items
    }

    pub(crate) fn is_deleted_revealed(&self, item_id: i64) -> bool {
        self.revealed_deleted.contains(&item_id)
    }

    pub(crate) fn is_secret_revealed(&self, item_id: i64, index: usize) -> bool {
        self.revealed_secrets
            .get(&item_id)
            .is_some_and(|set| set.contains(&index))
    }

    pub(crate) fn reaction_members(&self) -> Option<&ReactionMembersOverlay> {
        self.reaction_members.as_ref()
    }

    pub(crate) fn own_profile(&self) -> &OwnProfileDraft {
        &self.own_profile
    }

    pub(crate) fn contact_profile(&self) -> Option<&ContactProfileOverlay> {
        self.contact_profile.as_ref()
    }

    pub(crate) fn create_chat(&self) -> Option<&CreateChatOverlay> {
        self.create_chat.as_ref()
    }

    pub(crate) fn group_settings(&self) -> Option<&GroupSettingsOverlay> {
        self.group_settings.as_ref()
    }

    pub(crate) fn active_call(&self) -> Option<&CallInfo> {
        self.active_call.as_ref()
    }

    pub(crate) fn incoming_call(&self) -> Option<&CallInfo> {
        self.incoming_call.as_ref()
    }

    /// Chats for the current nav: inbox (non-channel) or channels only.
    pub(crate) fn chats_for_nav(&self) -> Vec<(usize, &ChatPreview)> {
        self.chats
            .iter()
            .enumerate()
            .filter(|(_, c)| match self.nav {
                NavSection::Channels => c.is_channel,
                NavSection::Chats => !c.is_channel,
                _ => false,
            })
            .collect()
    }

    pub(crate) fn direct_contacts(&self) -> Vec<(usize, &ChatPreview)> {
        self.chats
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                matches!(c.id.chat_type, ChatType::Direct) && c.pending_request_id.is_none()
            })
            .collect()
    }

    /// Direct contacts not already linked as group members (for Add member).
    pub(crate) fn contacts_to_add_to_group(&self) -> Vec<&ChatPreview> {
        let Some(g) = self.group_settings.as_ref() else {
            return Vec::new();
        };
        let existing: HashSet<i64> = g
            .members
            .iter()
            .filter_map(|m| m.contact_id)
            .collect();
        self.chats
            .iter()
            .filter(|c| {
                matches!(c.id.chat_type, ChatType::Direct)
                    && c.pending_request_id.is_none()
                    && !existing.contains(&c.id.api_id)
            })
            .collect()
    }

    pub(crate) fn selected_is_group(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .is_some_and(|c| matches!(c.id.chat_type, ChatType::Group))
    }

    pub(crate) fn selected_is_group_or_channel(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .is_some_and(|c| matches!(c.id.chat_type, ChatType::Group))
    }

    /// Relay-backed public group (Channels tab).
    pub(crate) fn selected_is_channel(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .is_some_and(|c| c.is_channel)
    }

    /// True for private groups (not channels) — show per-member avatars/names.
    pub(crate) fn selected_shows_member_headers(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .is_some_and(|c| matches!(c.id.chat_type, ChatType::Group) && !c.is_channel)
    }

    pub(crate) fn selected_is_direct(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .is_some_and(|c| {
                matches!(c.id.chat_type, ChatType::Direct) && c.pending_request_id.is_none()
            })
    }

    /// True when a release should count as a tap (not a scroll gesture).
    ///
    /// Driven only by [`POINTER_GATE`]: `App.press_moved` can stick after a
    /// scroll if `PointerPressed` is skipped on the next tap (hover not hot
    /// yet), which previously made every chat open no-op.
    pub(crate) fn tap_allowed(&self) -> bool {
        POINTER_GATE.tap_ok()
    }

    fn clear_pointer_gesture(&mut self) {
        self.press_moved = false;
        self.press_anchor = None;
        POINTER_GATE.reset_gesture();
        self.cancel_press_timers();
    }

    fn cancel_press_timers(&mut self) {
        self.long_press_index = None;
        self.long_press_generation = self.long_press_generation.wrapping_add(1);
        self.msg_long_press_item = None;
        self.msg_long_press_generation = self.msg_long_press_generation.wrapping_add(1);
        self.reaction_chip_press = None;
        self.reaction_chip_press_generation =
            self.reaction_chip_press_generation.wrapping_add(1);
    }

    fn open_support_list(&mut self, group_id: i64) -> Task<Message> {
        self.group_settings = None;
        self.support = Some(SupportOverlay {
            group_id,
            pane: SupportPane::List,
            members: Vec::new(),
            members_loading: true,
        });
        // Keep main group selected for list context, unscoped.
        if let Some(id) = self.selected_id.as_mut() {
            *id = id.without_scope();
        }
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        Task::perform(load_group_members(session, group_id), Message::SupportMembersLoaded)
    }

    fn open_support_thread(
        &mut self,
        group_id: i64,
        member_id: Option<i64>,
        title: String,
        from_list: bool,
    ) -> Task<Message> {
        self.group_settings = None;
        let members = self
            .support
            .as_ref()
            .map(|s| s.members.clone())
            .unwrap_or_default();
        self.support = Some(SupportOverlay {
            group_id,
            pane: SupportPane::Thread {
                member_id,
                title,
                from_list,
            },
            members,
            members_loading: false,
        });
        self.selected_id = Some(ChatId::with_support(group_id, member_id));
        self.messages.clear();
        self.pin_history_end = true;
        self.reply_to = None;
        self.editing = None;
        self.selection_mode = false;
        self.selected_items.clear();
        Task::batch([self.mark_selected_read(), self.reload_messages()])
    }

    fn close_support_overlay(&mut self) -> Task<Message> {
        let group_id = self.support.as_ref().map(|s| s.group_id);
        self.support = None;
        if let Some(gid) = group_id {
            self.selected_id = Some(ChatId::new(ChatType::Group, gid));
            self.messages.clear();
            self.pin_history_end = true;
            return Task::batch([self.mark_selected_read(), self.reload_messages()]);
        }
        Task::none()
    }

    pub(crate) fn support(&self) -> Option<&SupportOverlay> {
        self.support.as_ref()
    }

    pub(crate) fn support_badge_count(&self, chat: &ChatPreview) -> i64 {
        if can_moderate(chat.membership_role.as_deref().unwrap_or("member")) {
            chat.members_require_attention
        } else {
            chat.membership_support_unread
        }
    }

    fn open_group_settings(&mut self) -> Task<Message> {
        let Some(index) = self.selected else {
            return Task::none();
        };
        let Some(chat) = self.chats.get(index).cloned() else {
            return Task::none();
        };
        if !matches!(chat.id.chat_type, ChatType::Group) {
            self.status = "Open a group or channel for settings.".into();
            return Task::none();
        }
        let group_id = chat.id.api_id;
        let name = if chat.peer_profile.display_name.is_empty() {
            chat.title.clone()
        } else {
            chat.peer_profile.display_name.clone()
        };
        let my_role = chat
            .membership_role
            .clone()
            .unwrap_or_else(|| "member".into());
        self.group_settings = Some(GroupSettingsOverlay {
            group_id,
            title: chat.title.clone(),
            name,
            full_name: chat.peer_profile.full_name.clone(),
            short_descr: chat.peer_profile.short_descr.clone(),
            welcome_message: chat.peer_profile.description.clone(),
            image: chat.peer_profile.image.clone(),
            is_channel: chat.is_channel,
            members: Vec::new(),
            members_loading: true,
            link: None,
            link_member_role: "member".into(),
            invite_role: "member".into(),
            my_role: my_role.clone(),
            alias: chat.local_alias.clone(),
            alias_dirty: false,
            muted: chat.settings.enable_ntfs.is_muted(),
            favorite: chat.settings.favorite,
            group_preferences: chat.group_preferences.clone(),
            member_admission: chat.member_admission.clone(),
            public_group: chat.public_group.clone(),
            selected_member_id: None,
            pane: GroupSettingsPane::Overview,
            dirty: false,
        });
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let members = load_group_members(Arc::clone(&session), group_id);
        // Only owners/admins can get or create invite links — members hit
        // groupUserRole / groupLinkNotFound and dumped huge WARN payloads.
        if can_manage_members(&my_role) {
            let link = ensure_group_link(session, group_id, "member".into());
            Task::batch([
                Task::perform(members, Message::GroupMembersLoaded),
                Task::perform(link, Message::GroupLinkReady),
            ])
        } else {
            Task::perform(members, Message::GroupMembersLoaded)
        }
    }

    fn reload_group_members(&mut self) -> Task<Message> {
        let Some(g) = self.group_settings.as_mut() else {
            return Task::none();
        };
        g.members_loading = true;
        let group_id = g.group_id;
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        Task::perform(load_group_members(session, group_id), Message::GroupMembersLoaded)
    }

    fn open_contact_profile(&mut self) -> Task<Message> {
        let Some(index) = self.selected else {
            return Task::none();
        };
        let Some(chat) = self.chats.get(index).cloned() else {
            return Task::none();
        };
        if !matches!(chat.id.chat_type, ChatType::Direct) || chat.pending_request_id.is_some() {
            self.status = "Profile is available for contacts.".into();
            return Task::none();
        }
        let mut overlay = ContactProfileOverlay {
            chat_index: index,
            contact_id: chat.id.api_id,
            title: chat.title.clone(),
            fields: chat.peer_profile.clone(),
            alias: chat.local_alias.clone(),
            alias_dirty: false,
            muted: chat.settings.enable_ntfs.is_muted(),
            favorite: chat.settings.favorite,
            timed_allow: FeatureAllow::No,
            timed_contact_allows: FeatureAllow::No,
            timed_ttl_secs: DEFAULT_TIMED_TTL_SECS,
            full_delete_allow: FeatureAllow::No,
            full_delete_contact_allows: FeatureAllow::No,
            full_delete_for_user: false,
            full_delete_for_contact: false,
            reactions_allow: FeatureAllow::Yes,
            reactions_contact_allows: FeatureAllow::Yes,
            voice_allow: FeatureAllow::Yes,
            voice_contact_allows: FeatureAllow::Yes,
            calls_allow: FeatureAllow::Yes,
            calls_contact_allows: FeatureAllow::Yes,
            contact_preferences: chat.contact_preferences.clone(),
        };
        fill_contact_feature_overlay(&mut overlay, &chat);
        self.contact_profile = Some(overlay);
        Task::none()
    }

    fn save_contact_feature_prefs(&mut self) -> Task<Message> {
        let Some(overlay) = self.contact_profile.as_ref() else {
            return Task::none();
        };
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let contact_id = overlay.contact_id;
        let prefs = build_contact_prefs_json(
            overlay.contact_preferences.as_ref(),
            overlay.timed_allow,
            overlay.timed_ttl_secs,
            overlay.full_delete_allow,
            overlay.reactions_allow,
            overlay.voice_allow,
            overlay.calls_allow,
        );
        // Keep overlay in sync for next merge.
        if let Some(p) = self.contact_profile.as_mut() {
            p.contact_preferences = Some(prefs.clone());
        }
        self.status = "Updating chat features…".into();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    session
                        .set_contact_prefs(contact_id, &prefs)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())?
            },
            Message::ContactPrefsSaved,
        )
    }

    /// True when Full deletion is negotiated so broadcast deletes wipe for us.
    pub(crate) fn selected_full_delete_for_user(&self) -> bool {
        self.selected
            .and_then(|i| self.chats.get(i))
            .and_then(|c| c.contact_merged_preferences.as_ref())
            .map(|m| merged_enabled(Some(m), "fullDelete", true))
            .unwrap_or(false)
    }

    fn fetch_reaction_members(&mut self, item_id: i64, emoji: String) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(chat) = self.selected_chat_for_api() else {
            return Task::none();
        };
        if !matches!(chat.id.chat_type, cursus_core::ChatType::Group) {
            self.status = "Who reacted is only available in groups.".into();
            return Task::none();
        }
        self.reaction_members = Some(ReactionMembersOverlay {
            item_id,
            emoji: emoji.clone(),
            names: Vec::new(),
            loading: true,
            error: None,
        });
        let chat_id = chat.id;
        Task::perform(
            load_reaction_members(session, chat_id, item_id, emoji.clone()),
            move |result| Message::ReactionMembersLoaded {
                item_id,
                emoji: emoji.clone(),
                result,
            },
        )
    }

    pub(crate) fn focus_item(&self) -> Option<i64> {
        self.focus_item
    }

    pub(crate) fn show_jump_to_latest(&self) -> bool {
        // Only when the user has scrolled away from the newest messages.
        // (Always-on for len>=12 covered the last bubble with the chevron FAB.)
        self.history_away_from_end
    }

    pub(crate) fn desktop_prefs(&self) -> &desktop::DesktopPrefs {
        &self.desktop_prefs
    }

    pub(crate) fn message_by_id(&self, item_id: i64) -> Option<&ChatMessage> {
        self.messages.iter().find(|m| m.item_id == item_id)
    }

    /// Desktop notify for inbound `newChatItems` when the user is not already
    /// focused on that conversation (and the chat is not muted).
    fn maybe_notify_new_items(&self, event: &Event) {
        if !self.desktop_prefs.notifications {
            return;
        }
        let Event::NewChatItems(value) = event else {
            return;
        };
        let Some(items) = value.get("chatItems").and_then(|v| v.as_array()) else {
            return;
        };
        for aci in items {
            let Some(info) = aci.get("chatInfo") else {
                continue;
            };
            let Some(chat_id) = chat_id_from_info(info) else {
                continue;
            };
            let item = aci.get("chatItem").unwrap_or(aci);
            let dir = item
                .pointer("/chatDir/type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let outgoing = dir.contains("Snd");
            if outgoing {
                continue;
            }
            if self.window_focused && self.selected_id.as_ref() == Some(&chat_id) {
                continue;
            }
            if self
                .chats
                .iter()
                .find(|c| c.id == chat_id)
                .map(|c| c.settings.enable_ntfs.is_muted())
                .unwrap_or(false)
            {
                continue;
            }
            let title = chat_title_from_info(info).unwrap_or_else(|| "Cursus".into());
            let body = item
                .pointer("/meta/itemText")
                .and_then(|v| v.as_str())
                .or_else(|| item.pointer("/content/msgContent/text").and_then(|v| v.as_str()))
                .unwrap_or("New message")
                .to_string();
            desktop::notify_message(&title, &body);
        }
    }

    fn open_message_menu(&mut self, item_id: i64) {
        self.chat_menu = None;
        self.message_menu = Some(MessageMenu {
            item_id,
            pane: MsgMenuPane::Actions,
        });
    }

    fn handle_msg_menu_action(&mut self, action: MsgMenuAction) -> Task<Message> {
        let Some(menu) = self.message_menu.clone() else {
            return Task::none();
        };
        let item_id = menu.item_id;
        let Some(msg) = self.messages.iter().find(|m| m.item_id == item_id).cloned() else {
            self.message_menu = None;
            return Task::none();
        };
        match action {
            MsgMenuAction::Reply => {
                self.editing = None;
                self.reply_to = Some(ReplyTarget {
                    item_id,
                    preview: truncate_preview(&msg.text, 80),
                });
                self.message_menu = None;
                Task::none()
            }
            MsgMenuAction::Copy => {
                self.message_menu = None;
                self.status = "Copied.".into();
                iced::clipboard::write(msg.text)
            }
            MsgMenuAction::Edit => {
                self.message_menu = None;
                self.begin_edit_message(&msg)
            }
            MsgMenuAction::Forward => {
                if let Some(menu) = self.message_menu.as_mut() {
                    menu.pane = MsgMenuPane::ForwardPick;
                }
                Task::none()
            }
            MsgMenuAction::Info => {
                let file_detail = msg.file.as_ref().map(|f| {
                    let state = effective_complete(f.state, msg.outgoing, msg.delivery);
                    file_detail_line(f, state, msg.outgoing)
                });
                if let Some(menu) = self.message_menu.as_mut() {
                    menu.pane = MsgMenuPane::Info {
                        item_id,
                        time: None,
                        status: None,
                        text: msg.text.clone(),
                        file_detail,
                    };
                }
                let Some(session) = self.session.clone() else {
                    return Task::none();
                };
                let Some(chat) = self.selected_chat_for_api() else {
                    return Task::none();
                };
                Task::perform(load_item_info(session, chat.id, item_id), Message::ItemInfoLoaded)
            }
            MsgMenuAction::Delete => {
                if msg.is_deleted() || !msg.outgoing {
                    // Local-only: already marked, or incoming (cannot broadcast).
                    return self.delete_items(vec![item_id], false);
                }
                if let Some(menu) = self.message_menu.as_mut() {
                    menu.pane = MsgMenuPane::ConfirmDelete;
                }
                Task::none()
            }
            MsgMenuAction::Select => {
                self.selection_mode = true;
                self.selected_items.clear();
                self.selected_items.insert(item_id);
                self.message_menu = None;
                Task::none()
            }
            MsgMenuAction::ToggleReveal => {
                self.toggle_reveal_deleted(item_id);
                self.message_menu = None;
                Task::none()
            }
            MsgMenuAction::Download => {
                self.message_menu = None;
                let Some(file) = msg.file.as_ref() else {
                    self.status = "No file to download.".into();
                    return Task::none();
                };
                let Some(src) = self.plaintext_path_for(file).filter(|p| p.is_file()) else {
                    self.status = if file.encrypted {
                        "Decrypting file… try again in a moment.".into()
                    } else {
                        "File is not available locally yet.".into()
                    };
                    return Task::none();
                };
                let name = file.file_name.clone();
                Task::perform(save_media_as(src, name), Message::MediaSaved)
            }
        }
    }

    fn toggle_reveal_deleted(&mut self, item_id: i64) {
        if self.revealed_deleted.contains(&item_id) {
            self.revealed_deleted.remove(&item_id);
        } else {
            self.revealed_deleted.insert(item_id);
        }
    }

    fn confirm_msg_delete(&mut self) -> Task<Message> {
        let Some(menu) = self.message_menu.as_ref() else {
            return Task::none();
        };
        let item_id = menu.item_id;
        let Some(msg) = self.messages.iter().find(|m| m.item_id == item_id) else {
            return Task::none();
        };
        // Already marked → local remove only. Incoming → local only (core rejects
        // broadcast for rcv). Outgoing → broadcast so the peer gets XMsgDel.
        if msg.is_deleted() || !msg.outgoing {
            return self.delete_items(vec![item_id], false);
        }
        self.delete_items(vec![item_id], true)
    }

    fn delete_items(&mut self, ids: Vec<i64>, broadcast: bool) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let Some(chat) = self.selected_chat_for_api() else {
            return Task::none();
        };
        Task::perform(
            delete_items(session, chat.id, ids, broadcast),
            Message::DeleteItemsDone,
        )
    }

    fn jump_to_item(&mut self, item_id: i64) -> Task<Message> {
        let Some(index) = self.messages.iter().position(|m| m.item_id == item_id) else {
            self.status = "Original message is no longer available.".into();
            return Task::none();
        };
        self.focus_item = Some(item_id);
        self.focus_item_generation = self.focus_item_generation.wrapping_add(1);
        let generation = self.focus_item_generation;
        let n = self.messages.len();
        // Anchor::End: relative y=0 is newest (bottom). Oldest (index 0) → y≈1.
        let y = if n <= 1 {
            0.0
        } else {
            (n - 1 - index) as f32 / (n - 1) as f32
        };
        Task::batch([
            iced::widget::operation::snap_to(
                iced::widget::Id::new("chat-history"),
                RelativeOffset { x: 0.0, y },
            ),
            Task::perform(
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
                    Message::ClearFocusItem { generation }
                },
                |msg| msg,
            ),
        ])
    }

    fn accept_emoji_suggestion(&mut self) {
        let suggestions = self.emoji_suggestions();
        if suggestions.is_empty() {
            return;
        }
        let index = self.emoji_suggest_index.min(suggestions.len() - 1);
        let glyph = suggestions[index].glyph;
        let mut text = self.composer_text();
        let Some(start) = text.rfind(':') else {
            return;
        };
        text.truncate(start);
        text.push_str(glyph);
        // Recreates content and parks the caret after the inserted emoji.
        self.set_composer_text(text);
        self.emoji_suggest_index = 0;
    }
    pub(crate) fn chat_by_id(&self, id: &ChatId) -> Option<&ChatPreview> {
        self.chats.iter().find(|c| &c.id == id)
    }
    pub(crate) fn hover_slots(&self) -> &hover::HoverSlots {
        &self.hover
    }
}

fn main_window_settings() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1180.0, 720.0),
        exit_on_close_request: false,
        ..Default::default()
    }
}

fn open_main_window() -> (iced::window::Id, Task<Message>) {
    let (id, open) = iced::window::open(main_window_settings());
    (id, open.map(Message::MainWindowOpened))
}

fn refresh_window_metrics_for(id: iced::window::Id) -> Task<Message> {
    Task::batch([
        iced::window::size(id).map(Message::WindowResized),
        iced::window::scale_factor(id).map(Message::ScaleFactor),
    ])
}

fn refresh_scale_factor() -> Task<Message> {
    iced::window::latest().then(|id| match id {
        Some(id) => iced::window::scale_factor(id).map(Message::ScaleFactor),
        None => Task::none(),
    })
}

/// Hash/Eq by Arc pointer so `Subscription::run_with` restarts only on reconnect.
#[derive(Clone)]
struct SessionSub(Arc<Session>);

impl PartialEq for SessionSub {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionSub {}

impl std::hash::Hash for SessionSub {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as usize).hash(state);
    }
}

fn core_event_stream(
    session: &SessionSub,
) -> impl stream::Stream<Item = Message> + Send + 'static {
    let session = session.0.clone();
    iced::stream::channel(64, move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
        let mut rx = session.spawn_receiver();
        while let Some(event) = rx.recv().await {
            if output.send(Message::CoreEvent(event)).await.is_err() {
                break;
            }
        }
    })
}

/// Poll tray/menu channels; only emit when there is work (avoids a 5 Hz full UI rebuild).
fn tray_action_stream(
    ids: &(tray_icon::menu::MenuId, tray_icon::menu::MenuId, tray_icon::menu::MenuId),
) -> impl stream::Stream<Item = Message> + Send + 'static {
    let (show_id, hide_id, quit_id) = ids.clone();
    iced::stream::channel(8, move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let mut actions = Vec::new();
            while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
                if matches!(
                    event,
                    tray_icon::TrayIconEvent::DoubleClick { .. }
                        | tray_icon::TrayIconEvent::Click { .. }
                ) {
                    actions.push(desktop::TrayAction::Show);
                }
            }
            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if event.id == show_id {
                    actions.push(desktop::TrayAction::Show);
                } else if event.id == hide_id {
                    actions.push(desktop::TrayAction::Hide);
                } else if event.id == quit_id {
                    actions.push(desktop::TrayAction::Quit);
                }
            }
            if actions.is_empty() {
                continue;
            }
            if output.send(Message::TrayActions(actions)).await.is_err() {
                break;
            }
        }
    })
}

async fn connect_session(
    slot: Arc<Mutex<Option<Arc<Session>>>>,
    req: ConnectRequest,
) -> Result<String, String> {
    const UNLOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
    let work = tokio::task::spawn_blocking(move || {
        tracing::info!("unlock: Session::open");
        let cfg = SessionConfig {
            db_key: req.password.clone(),
            initial_display_name: req.display_name.clone(),
            encrypt_local_files: req.encrypt_local_files,
            ..SessionConfig::default()
        };
        let mut session = Session::open(cfg).map_err(|e| e.to_string())?;
        tracing::info!("unlock: start_chat");
        session
            .start_chat_with_options(req.encrypt_local_files)
            .map_err(|e| e.to_string())?;
        // Align active profile display name with welcome name when core created a default.
        if let Some(user) = session.active_profile() {
            if user.display_name() != req.display_name.trim() && !req.display_name.trim().is_empty()
            {
                let mut fields = user.profile.clone();
                fields.display_name = req.display_name.trim().to_string();
                let _ = session.update_profile(fields);
            }
        }
        if req.save_account {
            let meta = AccountMeta {
                display_name: req.display_name.trim().to_string(),
                version: 1,
                encrypt_local_files: req.encrypt_local_files,
            };
            save_account(&meta).map_err(|e| e.to_string())?;
        }
        let name = session
            .active_profile()
            .map(|u| u.display_name().to_string())
            .unwrap_or_else(|| req.display_name.clone());
        let session = Arc::new(session);
        *slot.lock().map_err(|e| e.to_string())? = Some(session);
        tracing::info!(%name, "unlock: done");
        Ok(name)
    });
    match tokio::time::timeout(UNLOCK_TIMEOUT, work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_) => Err(
            "Unlock timed out after 90s. Quit any other Cursus window/tray instance and try again."
                .into(),
        ),
    }
}

async fn activate_network(session: Arc<Session>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || session.activate_network().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn load_chats(session: Arc<Session>) -> Result<Vec<ChatPreview>, String> {
    tokio::task::spawn_blocking(move || session.list_chats().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn load_messages(
    session: Arc<Session>,
    chat: ChatId,
) -> Result<Vec<ChatMessage>, String> {
    tokio::task::spawn_blocking(move || {
        session
            .list_messages(&chat, 100)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn send_text(
    session: Arc<Session>,
    chat: ChatPreview,
    text: String,
    quoted_item_id: Option<i64>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .send_text_quoted(&chat.id, &text, quoted_item_id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn update_text(
    session: Arc<Session>,
    chat: ChatId,
    item_id: i64,
    text: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .update_text(&chat, item_id, &text)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn friendly_edit_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("invalidchatitemupdate")
        || lower.contains("invalid chat item update")
        || lower.contains("not editable")
        || lower.contains("ceinvalidchatitemupdate")
    {
        "This message can no longer be edited.".into()
    } else {
        err.to_string()
    }
}

async fn delete_items(
    session: Arc<Session>,
    chat_id: ChatId,
    item_ids: Vec<i64>,
    broadcast: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .delete_items(&chat_id, &item_ids, broadcast)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn forward_item(
    session: Arc<Session>,
    from: ChatId,
    to: ChatId,
    item_id: i64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .forward_items(&from, &to, &[item_id])
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn load_item_info(
    session: Arc<Session>,
    chat_id: ChatId,
    item_id: i64,
) -> Result<ItemInfoData, String> {
    tokio::task::spawn_blocking(move || {
        let value = session
            .item_info(&chat_id, item_id)
            .map_err(|e| e.to_string())?;
        let time = value
            .pointer("/chatItem/chatItem/meta/itemTs")
            .or_else(|| value.pointer("/chatItem/meta/itemTs"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let status = value
            .pointer("/chatItem/chatItem/meta/itemStatus/type")
            .or_else(|| value.pointer("/chatItem/meta/itemStatus/type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let text = value
            .pointer("/chatItem/chatItem/content/msgContent/text")
            .or_else(|| value.pointer("/chatItem/content/msgContent/text"))
            .or_else(|| value.pointer("/chatItem/chatItem/meta/itemText"))
            .or_else(|| value.pointer("/chatItem/meta/itemText"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text = cursus_core::expand_emoji_shortcodes(&text);
        Ok(ItemInfoData {
            item_id,
            time,
            status,
            text,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn chat_id_from_info(info: &serde_json::Value) -> Option<ChatId> {
    let type_name = info.get("type")?.as_str()?;
    match type_name {
        "direct" => Some(ChatId {
                    chat_type: cursus_core::ChatType::Direct,
                    api_id: info.pointer("/contact/contactId")?.as_i64()?,
                    scope: None,
                }),
        "group" => {
            let api_id = info.pointer("/groupInfo/groupId")?.as_i64()?;
            let scope = info.get("groupChatScope").and_then(|scope| {
                if scope.is_null() {
                    return None;
                }
                match scope.get("type").and_then(|v| v.as_str())? {
                    "memberSupport" => {
                        let group_member_id = scope
                            .pointer("/groupMember/groupMemberId")
                            .and_then(|v| v.as_i64());
                        Some(cursus_core::GroupChatScope::MemberSupport { group_member_id })
                    }
                    _ => None,
                }
            });
            Some(ChatId {
                chat_type: cursus_core::ChatType::Group,
                api_id,
                scope,
            })
        }
        "local" => Some(ChatId {
                    chat_type: cursus_core::ChatType::Local,
                    api_id: info.pointer("/noteFolder/noteFolderId")?.as_i64()?,
                    scope: None,
                }),
        _ => None,
    }
}

fn chat_title_from_info(info: &serde_json::Value) -> Option<String> {
    match info.get("type")?.as_str()? {
        "direct" => info
            .pointer("/contact/localDisplayName")
            .or_else(|| info.pointer("/contact/profile/displayName"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "group" => info
            .pointer("/groupInfo/localDisplayName")
            .or_else(|| info.pointer("/groupInfo/groupProfile/displayName"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "local" => Some("Notes".into()),
        _ => None,
    }
}

fn truncate_preview(s: &str, max: usize) -> String {
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

async fn attach_and_send_file(session: Arc<Session>, chat: ChatPreview) -> Result<(), String> {
    let path = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Attach file")
            .pick_file()
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Attachment cancelled.".to_string())?;

    let path_str = path.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        session
            .send_file(&chat.id, &path_str)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn receive_file(
    session: Arc<Session>,
    file_id: i64,
    file_name: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .receive_file(file_id)
            .map(|_| {
                format!(
                    "Downloading {file_name}… (saving under {})",
                    files_dir().display()
                )
            })
            .map_err(|e| format!("file_id:{file_id}:{e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn auto_receive_from_messages(messages: &[ChatMessage]) -> Vec<(i64, String)> {
    messages
        .iter()
        .filter(|m| !m.outgoing)
        .filter_map(|m| {
            let file = m.file.as_ref()?;
            if file.should_auto_receive() {
                Some((file.file_id, file.file_name.clone()))
            } else {
                None
            }
        })
        .collect()
}

async fn open_local_file(path: PathBuf) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        if !path.is_file() {
            return Err(format!("File missing: {}", path.display()));
        }
        // Prefer mpv for video — xdg-open often picks a sluggish default player.
        if media::is_video_name(
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
        ) {
            if Command::new("mpv")
                .args([
                    "--force-window=immediate",
                    "--keep-open=no",
                    "--really-quiet",
                    "--",
                ])
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(format!("Opened {}", path.display()));
            }
        }
        Command::new("xdg-open")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(format!("Opened {}", path.display()))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn open_url(url: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        Command::new("xdg-open")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(format!("Opened {url}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_reaction(
    session: Arc<Session>,
    chat_id: ChatId,
    item_id: i64,
    emoji: String,
    add: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_reaction(&chat_id, item_id, &emoji, add)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn sync_address_auto_accept(session: Arc<Session>) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        session
            .address_auto_accept()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_address_auto_accept(session: Arc<Session>, enable: bool) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_address_auto_accept(enable)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn load_reaction_members(
    session: Arc<Session>,
    chat_id: ChatId,
    item_id: i64,
    emoji: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        session
            .reaction_members(&chat_id, item_id, &emoji)
            .map(|members| {
                members
                    .into_iter()
                    .map(|m| m.display_name)
                    .collect::<Vec<_>>()
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn load_group_members(
    session: Arc<Session>,
    group_id: i64,
) -> Result<Vec<GroupMemberInfo>, String> {
    tokio::task::spawn_blocking(move || {
        session
            .list_group_members(group_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn add_group_member(
    session: Arc<Session>,
    group_id: i64,
    contact_id: i64,
    role: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .add_group_member(group_id, contact_id, &role)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn save_group_profile(
    session: Arc<Session>,
    group_id: i64,
    profile: GroupProfileUpdate,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .update_group_profile(group_id, &profile)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn save_group_alias(
    session: Arc<Session>,
    group_id: i64,
    alias: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_group_alias(group_id, &alias)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn ensure_group_link(
    session: Arc<Session>,
    group_id: i64,
    role: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .group_invite_link(group_id, &role)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_group_link_role(
    session: Arc<Session>,
    group_id: i64,
    role: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_group_link_role(group_id, &role)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn delete_group_link(session: Arc<Session>, group_id: i64) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .delete_group_link(group_id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn delete_member_support_chat(
    session: Arc<Session>,
    group_id: i64,
    group_member_id: i64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .delete_member_support_chat(group_id, group_member_id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn add_group_short_link(session: Arc<Session>, group_id: i64) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .short_group_link(group_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_group_member_role(
    session: Arc<Session>,
    group_id: i64,
    member_id: i64,
    role: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_group_member_role(group_id, member_id, &role)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn remove_group_member(
    session: Arc<Session>,
    group_id: i64,
    member_id: i64,
    with_messages: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .remove_group_member(group_id, member_id, with_messages)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_member_show_messages(
    session: Arc<Session>,
    group_id: i64,
    member_id: i64,
    show: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_member_show_messages(group_id, member_id, show)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn block_group_member(
    session: Arc<Session>,
    group_id: i64,
    member_id: i64,
    blocked: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .block_group_member(group_id, member_id, blocked)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn accept_group_member(
    session: Arc<Session>,
    group_id: i64,
    member_id: i64,
    role: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .accept_group_member(group_id, member_id, &role)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn leave_group(session: Arc<Session>, group_id: i64) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .leave_group(group_id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn cycle_role_below(current: &str, my_role: &str) -> String {
    let max = group_role_rank(my_role).saturating_sub(1).max(1);
    let allowed: Vec<&str> = GROUP_ROLES
        .iter()
        .copied()
        .filter(|r| group_role_rank(r) <= max)
        .collect();
    if allowed.is_empty() {
        return "member".into();
    }
    let idx = allowed
        .iter()
        .position(|r| *r == current)
        .map(|i| (i + 1) % allowed.len())
        .unwrap_or(0);
    allowed[idx].to_string()
}

/// Group invite-link joiner role — SimpleX allows only `observer` / `member`.
fn cycle_link_joiner_role(current: &str) -> String {
    const LINK_ROLES: &[&str] = &["observer", "member"];
    let idx = LINK_ROLES
        .iter()
        .position(|r| *r == current)
        .map(|i| (i + 1) % LINK_ROLES.len())
        .unwrap_or(0);
    LINK_ROLES[idx].to_string()
}

fn toggle_group_pref(prefs: &mut Option<serde_json::Value>, key: &str) {
    let obj = prefs
        .get_or_insert_with(|| serde_json::json!({}))
        .as_object_mut();
    let Some(obj) = obj else {
        return;
    };
    // Support defaults to on in SimpleX when omitted — seed that so the first
    // toggle turns it off instead of writing a redundant "on".
    let default_enable = if key == "support" { "on" } else { "off" };
    let entry = obj.entry(key.to_string()).or_insert_with(|| {
        serde_json::json!({ "enable": default_enable })
    });
    let on = entry
        .get("enable")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("on"))
        .unwrap_or(key == "support");
    if let Some(map) = entry.as_object_mut() {
        let turn_on = !on;
        map.insert(
            "enable".into(),
            serde_json::Value::String(if turn_on { "on" } else { "off" }.into()),
        );
        // Disappearing messages need a TTL when enabled (core uses `ttl`).
        if key == "timedMessages" {
            if turn_on {
                if map.get("ttl").and_then(|v| v.as_i64()).unwrap_or(0) <= 0 {
                    map.insert(
                        "ttl".into(),
                        serde_json::Value::Number(DEFAULT_TIMED_TTL_SECS.into()),
                    );
                }
            } else {
                map.insert("ttl".into(), serde_json::Value::Null);
            }
        }
    }
}

const DEFAULT_TIMED_TTL_SECS: i64 = 86_400; // 1 day
pub(crate) const TIMED_TTL_STEPS: &[i64] = &[
    30,        // 30 sec
    300,       // 5 min
    3_600,     // 1 hour
    86_400,    // 1 day
    604_800,   // 1 week
    2_592_000, // 30 days
];

pub(crate) fn format_timed_ttl(secs: i64) -> String {
    match secs {
        30 => "30 seconds".into(),
        300 => "5 minutes".into(),
        3_600 => "1 hour".into(),
        86_400 => "1 day".into(),
        604_800 => "1 week".into(),
        2_592_000 => "30 days".into(),
        s if s < 60 => format!("{s} sec"),
        s if s < 3_600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3_600),
        s if s < 604_800 => format!("{} d", s / 86_400),
        s => format!("{} d", s / 86_400),
    }
}

fn contact_pref_allow(prefs: Option<&serde_json::Value>, key: &str) -> FeatureAllow {
    prefs
        .and_then(|p| p.get(key))
        .and_then(|v| v.get("allow"))
        .and_then(|v| v.as_str())
        .map(FeatureAllow::from_str)
        .unwrap_or(FeatureAllow::No)
}

fn merged_user_allow(merged: Option<&serde_json::Value>, key: &str) -> Option<FeatureAllow> {
    let up = merged?.get(key)?.get("userPreference")?;
    up.get("preference")
        .and_then(|v| v.get("allow"))
        .and_then(|v| v.as_str())
        .map(FeatureAllow::from_str)
}

fn feature_you_allow(
    prefs: Option<&serde_json::Value>,
    merged: Option<&serde_json::Value>,
    key: &str,
    default: FeatureAllow,
) -> FeatureAllow {
    if prefs.and_then(|p| p.get(key)).is_some() {
        return contact_pref_allow(prefs, key);
    }
    merged_user_allow(merged, key).unwrap_or(default)
}

fn merged_contact_allow(merged: Option<&serde_json::Value>, key: &str) -> FeatureAllow {
    merged
        .and_then(|p| p.get(key))
        .and_then(|v| v.get("contactPreference"))
        .and_then(|v| v.get("allow"))
        .and_then(|v| v.as_str())
        .map(FeatureAllow::from_str)
        .unwrap_or(FeatureAllow::No)
}

fn merged_enabled(merged: Option<&serde_json::Value>, key: &str, for_user: bool) -> bool {
    let field = if for_user { "forUser" } else { "forContact" };
    merged
        .and_then(|p| p.get(key))
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn contact_pref_ttl(
    prefs: Option<&serde_json::Value>,
    merged: Option<&serde_json::Value>,
) -> Option<i64> {
    prefs
        .and_then(|p| p.get("timedMessages"))
        .and_then(|v| v.get("ttl"))
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .or_else(|| {
            merged
                .and_then(|p| p.get("timedMessages"))
                .and_then(|v| v.get("userPreference"))
                .and_then(|v| v.get("preference"))
                .and_then(|v| v.get("ttl"))
                .and_then(|v| v.as_i64())
                .filter(|n| *n > 0)
        })
}

fn fill_contact_feature_overlay(overlay: &mut ContactProfileOverlay, chat: &ChatPreview) {
    let prefs = chat.contact_preferences.as_ref();
    let merged = chat.contact_merged_preferences.as_ref();
    // Defaults when neither contact nor merged prefs specify a value.
    overlay.timed_allow =
        feature_you_allow(prefs, merged, "timedMessages", FeatureAllow::No);
    overlay.timed_contact_allows = merged_contact_allow(merged, "timedMessages");
    overlay.timed_ttl_secs = contact_pref_ttl(prefs, merged).unwrap_or(DEFAULT_TIMED_TTL_SECS);
    overlay.full_delete_allow =
        feature_you_allow(prefs, merged, "fullDelete", FeatureAllow::No);
    overlay.full_delete_contact_allows = merged_contact_allow(merged, "fullDelete");
    overlay.full_delete_for_user = merged_enabled(merged, "fullDelete", true);
    overlay.full_delete_for_contact = merged_enabled(merged, "fullDelete", false);
    overlay.reactions_allow =
        feature_you_allow(prefs, merged, "reactions", FeatureAllow::Yes);
    overlay.reactions_contact_allows = merged_contact_allow(merged, "reactions");
    overlay.voice_allow = feature_you_allow(prefs, merged, "voice", FeatureAllow::Yes);
    overlay.voice_contact_allows = merged_contact_allow(merged, "voice");
    overlay.calls_allow = feature_you_allow(prefs, merged, "calls", FeatureAllow::Yes);
    overlay.calls_contact_allows = merged_contact_allow(merged, "calls");
    overlay.contact_preferences = chat.contact_preferences.clone();
}

fn build_contact_prefs_json(
    existing: Option<&serde_json::Value>,
    timed_allow: FeatureAllow,
    timed_ttl_secs: i64,
    full_delete: FeatureAllow,
    reactions: FeatureAllow,
    voice: FeatureAllow,
    calls: FeatureAllow,
) -> serde_json::Value {
    let mut prefs = existing.cloned().unwrap_or_else(|| serde_json::json!({}));
    let timed = if timed_allow.is_on() {
        serde_json::json!({
            "allow": timed_allow.as_api(),
            "ttl": timed_ttl_secs.max(1),
        })
    } else {
        serde_json::json!({ "allow": "no" })
    };
    if let Some(obj) = prefs.as_object_mut() {
        obj.insert("timedMessages".into(), timed);
        obj.insert(
            "fullDelete".into(),
            serde_json::json!({ "allow": full_delete.as_api() }),
        );
        obj.insert(
            "reactions".into(),
            serde_json::json!({ "allow": reactions.as_api() }),
        );
        obj.insert(
            "voice".into(),
            serde_json::json!({ "allow": voice.as_api() }),
        );
        obj.insert(
            "calls".into(),
            serde_json::json!({ "allow": calls.as_api() }),
        );
    }
    prefs
}

pub(crate) fn group_pref_on(prefs: Option<&serde_json::Value>, key: &str) -> bool {
    prefs
        .and_then(|p| p.get(key))
        .and_then(|v| v.get("enable"))
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("on"))
        .unwrap_or(false)
}

/// Member support defaults to on in SimpleX when the pref is omitted.
pub(crate) fn group_support_allowed(prefs: Option<&serde_json::Value>) -> bool {
    prefs
        .and_then(|p| p.get("support"))
        .and_then(|v| v.get("enable"))
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("on"))
        .unwrap_or(true)
}

pub(crate) fn can_manage_members(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("admin")
}

pub(crate) fn can_moderate(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("moderator")
}

pub(crate) fn can_edit_group_profile(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("owner")
}

async fn create_chat_entity(
    session: Arc<Session>,
    kind: CreateKind,
    name: String,
) -> Result<(CreateKind, ChatId, Option<String>, String), String> {
    tokio::task::spawn_blocking(move || {
        let title = name.trim().to_string();
        match kind {
            CreateKind::Group => session
                .create_group(&name)
                .map(|id| (kind, id, None, title))
                .map_err(|e| e.to_string()),
            CreateKind::Channel => session
                .create_channel(&name)
                .map(|(id, link)| (kind, id, link, title))
                .map_err(|e| e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn call_invite(
    session: Arc<Session>,
    contact_id: i64,
    media: CallMedia,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .call_invite(contact_id, media)
            .map(|_| "Call invitation sent.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn call_reject(session: Arc<Session>, contact_id: i64) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .call_reject(contact_id)
            .map(|_| "Call declined.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn call_end(session: Arc<Session>, contact_id: i64) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .call_end(contact_id)
            .map(|_| "Call ended.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn parse_call_invitation(body: &serde_json::Value, outgoing: bool) -> Option<CallInfo> {
    let contact = body.get("contact")?;
    let contact_id = contact.get("contactId")?.as_i64()?;
    let peer_name = contact
        .get("localDisplayName")
        .or_else(|| contact.pointer("/profile/displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("Contact")
        .to_string();
    let media = match body
        .pointer("/callType/media")
        .or_else(|| body.pointer("/callInvitation/callType/media"))
        .and_then(|v| v.as_str())
    {
        Some("video") => CallMedia::Video,
        _ => CallMedia::Audio,
    };
    Some(CallInfo {
        contact_id,
        peer_name,
        outgoing,
        media,
    })
}

async fn pick_and_encode_profile_image() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["jpg", "jpeg", "png", "webp", "gif", "bmp"])
            .pick_file()
            .ok_or_else(|| "No image selected.".to_string())?;
        media::encode_profile_image(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn save_own_profile(
    session: Arc<Session>,
    fields: ProfileFields,
) -> Result<UserProfile, String> {
    tokio::task::spawn_blocking(move || session.update_profile(fields).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn set_profile_address(session: Arc<Session>, on: bool) -> Result<UserProfile, String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_profile_address(on)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn save_contact_alias(
    session: Arc<Session>,
    contact_id: i64,
    alias: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_contact_alias(contact_id, &alias)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn reaction_emoji_eq(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> &str {
        if s == "❤" || s == "❤️" || s == "\u{2764}\u{FE0F}" {
            "❤"
        } else {
            s
        }
    }
    norm(a) == norm(b)
}

fn start_voice_recording() -> Result<VoiceRec, String> {
    // Record under tmp/ — send path encrypts into files/ (CryptoFile) then deletes this.
    let dir = cursus_core::temp_dir().join("rec");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("voice-{stamp}.m4a"));
    let mut last_err = String::from("no audio capture backend worked");

    // On musl/ARM, prefer arecord first — ffmpeg ALSA/Pulse often opens a device
    // that stays silent (UI thinks it records, file is near-empty header only).
    #[cfg(target_env = "musl")]
    {
        if let Ok(rec) = try_arecord_voice(path.with_extension("wav"), &mut last_err) {
            return Ok(rec);
        }
    }

    // Prefer AAC when available; fall back to PCM WAV.
    #[cfg(target_env = "musl")]
    let backends: &[&[&str]] = &[
        &["-f", "alsa", "-i", "default"],
        &["-f", "alsa", "-i", "plughw:0,0"],
        &["-f", "alsa", "-i", "hw:0,0"],
        &["-f", "alsa", "-i", "sysdefault"],
        &["-f", "pulse", "-i", "default"],
    ];
    #[cfg(not(target_env = "musl"))]
    let backends: &[&[&str]] = &[
        &["-f", "pulse", "-i", "default"],
        &["-f", "pipewire", "-i", "default"],
        &["-f", "alsa", "-i", "default"],
        &["-f", "alsa", "-i", "plughw:0,0"],
        &["-f", "alsa", "-i", "hw:0,0"],
        &["-f", "alsa", "-i", "sysdefault"],
    ];
    let encodes: &[&[&str]] = &[
        &["-ac", "1", "-ar", "16000", "-c:a", "aac", "-b:a", "32k", "-y"],
        &["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-f", "wav", "-y"],
    ];

    for input in backends {
        for encode in encodes {
            let out = if encode.iter().any(|s| *s == "wav") {
                path.with_extension("wav")
            } else {
                path.clone()
            };
            let mut args = vec!["-hide_banner", "-loglevel", "error", "-nostdin"];
            args.extend_from_slice(input);
            args.extend_from_slice(encode);
            match Command::new("ffmpeg")
                .args(&args)
                .arg(&out)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    match child.try_wait() {
                        Ok(None) => {
                            let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                            // WAV header alone is ~44 bytes; require real PCM growth.
                            if len > 256 {
                                return Ok(VoiceRec {
                                    path: out,
                                    child,
                                    started: Instant::now(),
                                });
                            }
                            let _ = child.kill();
                            let _ = child.wait();
                            last_err = format!(
                                "ffmpeg {:?} wrote only {len} bytes (silent/hung device?)",
                                input
                            );
                            let _ = std::fs::remove_file(&out);
                        }
                        Ok(Some(status)) => {
                            let stderr = child.stderr.as_mut().and_then(|s| {
                                let mut buf = String::new();
                                use std::io::Read;
                                let _ = s.read_to_string(&mut buf);
                                Some(buf)
                            });
                            last_err = format!(
                                "ffmpeg {:?} {:?} exited ({status}): {}",
                                input,
                                encode,
                                stderr.unwrap_or_default().trim()
                            );
                            let _ = std::fs::remove_file(&out);
                        }
                        Err(e) => {
                            last_err = format!("ffmpeg {:?} wait failed: {e}", input);
                            let _ = child.kill();
                            let _ = std::fs::remove_file(&out);
                        }
                    }
                }
                Err(e) => {
                    last_err = format!("ffmpeg {:?} spawn failed: {e}", input);
                }
            }
        }
    }

    #[cfg(not(target_env = "musl"))]
    {
        if let Ok(rec) = try_arecord_voice(path.with_extension("wav"), &mut last_err) {
            return Ok(rec);
        }
    }

    Err(format!(
        "{last_err}. Install ffmpeg (with ALSA/Pulse) or arecord for mic capture."
    ))
}

fn try_arecord_voice(wav: PathBuf, last_err: &mut String) -> Result<VoiceRec, ()> {
    match Command::new("arecord")
        .args([
            "-q",
            "-f",
            "S16_LE",
            "-c",
            "1",
            "-r",
            "16000",
            "-t",
            "wav",
            wav.to_str().unwrap_or("voice.wav"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::sleep(std::time::Duration::from_millis(250));
            match child.try_wait() {
                Ok(None) => {
                    let len = std::fs::metadata(&wav).map(|m| m.len()).unwrap_or(0);
                    if len > 44 {
                        return Ok(VoiceRec {
                            path: wav,
                            child,
                            started: Instant::now(),
                        });
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    *last_err = format!("arecord wrote only {len} bytes");
                    let _ = std::fs::remove_file(&wav);
                }
                Ok(Some(status)) => {
                    *last_err = format!("arecord exited ({status})");
                    let _ = std::fs::remove_file(&wav);
                }
                Err(e) => {
                    *last_err = format!("arecord wait failed: {e}");
                    let _ = child.kill();
                    let _ = std::fs::remove_file(&wav);
                }
            }
        }
        Err(e) => {
            *last_err = format!("arecord spawn failed: {e}");
        }
    }
    Err(())
}

fn stop_ffmpeg(child: &mut Child) {
    // Prefer SIGINT so arecord / ffmpeg finalize the WAV/m4a header.
    // (stdin 'q' only works when ffmpeg stdin is a pipe — we use -nostdin now.)
    #[cfg(unix)]
    {
        let pid = child.id();
        let _ = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status();
    }
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(b"q");
        let _ = stdin.flush();
    }
    let _ = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    // Give the recorder a moment to finalize; then force-kill if needed.
    for _ in 0..50 {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_audio_player(path: &PathBuf) -> Result<Child, String> {
    if !path.is_file() {
        return Err(format!("Voice file missing: {}", path.display()));
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() < 64 {
        return Err(format!(
            "Voice file empty ({} bytes): {}",
            meta.len(),
            path.display()
        ));
    }
    let is_wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));

    let mut attempts: Vec<(&str, Result<Child, std::io::Error>)> = Vec::new();
    attempts.push((
        "mpv",
        Command::new("mpv")
            .args(["--no-video", "--really-quiet", "--"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn(),
    ));
    attempts.push((
        "ffplay",
        Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "--"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn(),
    ));
    if is_wav {
        attempts.push((
            "aplay",
            Command::new("aplay")
                .args(["-q", "--"])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn(),
        ));
        attempts.push((
            "paplay",
            Command::new("paplay")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn(),
        ));
    }
    // Decode via ffmpeg → aplay pipe for odd containers on ARM.
    attempts.push((
        "ffmpeg|aplay",
        Command::new("sh")
            .args([
                "-c",
                "ffmpeg -hide_banner -loglevel error -i \"$1\" -f wav -ac 1 -ar 16000 - | aplay -q -t wav -",
                "sh",
                path.to_str().unwrap_or(""),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn(),
    ));

    let mut last = String::from("no audio player worked");
    for (name, result) in attempts {
        match result {
            Ok(child) => return Ok(child),
            Err(e) => last = format!("{name}: {e}"),
        }
    }
    Err(format!(
        "{last}. Install mpv, ffplay, or aplay to play voice messages."
    ))
}

async fn save_media_as(src: PathBuf, file_name: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        if !src.is_file() {
            return Err(format!("File missing: {}", src.display()));
        }
        let dest = rfd::FileDialog::new()
            .set_title("Save as")
            .set_file_name(&file_name)
            .save_file()
            .ok_or_else(|| "Save cancelled.".to_string())?;
        std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        Ok(format!("Saved to {}", dest.display()))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn effective_complete(
    state: FileTransferState,
    outgoing: bool,
    delivery: DeliveryStatus,
) -> FileTransferState {
    if outgoing
        && matches!(state, FileTransferState::InProgress | FileTransferState::Other)
        && matches!(delivery, DeliveryStatus::Sent | DeliveryStatus::Delivered)
    {
        return FileTransferState::Complete;
    }
    state
}

fn file_detail_line(
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    outgoing: bool,
) -> String {
    let ext = std::path::Path::new(&file.file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .unwrap_or_else(|| {
            match file.kind {
                cursus_core::FileKind::Image => ".img".into(),
                cursus_core::FileKind::Video => ".vid".into(),
                cursus_core::FileKind::Voice => ".audio".into(),
                _ => "file".into(),
            }
        });
    let size = if file.file_size > 0 {
        let n = file.file_size as f64;
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        if n >= MB {
            format!("{:.1} MB", n / MB)
        } else if n >= KB {
            format!("{:.0} KB", n / KB)
        } else {
            format!("{} B", file.file_size)
        }
    } else {
        String::new()
    };
    let label = match state {
        FileTransferState::Complete => {
            if outgoing {
                "sent"
            } else {
                "saved"
            }
        }
        FileTransferState::InProgress => {
            if outgoing {
                "sending"
            } else {
                "receiving"
            }
        }
        FileTransferState::Failed => "failed",
        FileTransferState::Offer => "offer",
        FileTransferState::Other => "file",
    };
    if size.is_empty() {
        format!("{ext} · {label}")
    } else {
        format!("{ext} · {size} · {label}")
    }
}

async fn create_invitation(session: Arc<Session>) -> Result<ConnLink, String> {
    tokio::task::spawn_blocking(move || session.create_invitation_link().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn ensure_address(session: Arc<Session>) -> Result<ConnLink, String> {
    tokio::task::spawn_blocking(move || session.ensure_address_link().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn mark_chat_read(
    session: Arc<Session>,
    chat: ChatId,
) -> Result<ChatId, String> {
    tokio::task::spawn_blocking(move || {
        session.mark_chat_read(&chat).map(|_| chat).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn mark_chat_unread(session: Arc<Session>, chat: ChatId) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .mark_chat_unread(&chat)
            .map(|_| "Marked as unread.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn set_chat_settings(
    session: Arc<Session>,
    chat: ChatId,
    settings: ChatSettings,
    ok_label: &'static str,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_chat_settings(&chat, &settings)
            .map(|_| ok_label.into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn clear_chat(session: Arc<Session>, chat: ChatId) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .clear_chat(&chat)
            .map(|_| "Chat cleared.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn delete_chat(session: Arc<Session>, chat: ChatId) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .delete_chat(&chat)
            .map(|_| "Chat deleted.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Core already removed the group/chat (`errorStore` / `groupNotFound`).
fn store_already_gone(err: &str) -> bool {
    err.contains("groupNotFound") || err.contains("chatNotFound")
}

async fn load_chat_tags(session: Arc<Session>) -> Result<Vec<ChatTag>, String> {
    tokio::task::spawn_blocking(move || session.list_chat_tags().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn create_chat_tag(session: Arc<Session>, name: String) -> Result<Vec<ChatTag>, String> {
    tokio::task::spawn_blocking(move || session.create_chat_tag(&name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

async fn set_chat_tags(
    session: Arc<Session>,
    chat: ChatId,
    tag_ids: Vec<i64>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .set_chat_tags(&chat, &tag_ids)
            .map(|_| "Lists updated.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn accept_contact_request(
    session: Arc<Session>,
    req_id: i64,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let value = session
            .accept_contact_request(req_id)
            .map_err(|e| e.to_string())?;
        let name = value
            .pointer("/contact/localDisplayName")
            .or_else(|| value.pointer("/contact/profile/displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or("contact");
        Ok(format!("Accepted {name} — you can message them once connected."))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn reject_contact_request(
    session: Arc<Session>,
    req_id: i64,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        session
            .reject_contact_request(req_id)
            .map(|_| "Contact request rejected.".into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn accept_invite(session: Arc<Session>, link: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let value = session.connect_link(&link).map_err(|e| e.to_string())?;
        let ty = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("ok");
        Ok(match ty {
            "sentConfirmation" => "Invite accepted — waiting for the other side to confirm.".into(),
            "sentInvitation" => "Request sent to their address — waiting for acceptance.".into(),
            "contactAlreadyExists" => "That contact is already in your chats.".into(),
            "connectionPlan" => "Link recognized — finish connecting from the plan UI (next).".into(),
            other => format!("Link accepted ({other})."),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
