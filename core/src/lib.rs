//! Shared SimpleX session layer for Cursus.
//!
//! Talks to the official `libsimplex` C ABI. Does not reimplement SMP.

mod account;
pub mod app_lock;
mod chat_list;
mod commands;
mod error;
mod ffi;
mod invitations;
mod json;
pub mod media;
mod models;
#[cfg(target_env = "musl")]
mod musl_compat;
mod parse;
mod paths;
pub mod preferences;
mod rich_text;
pub mod roles;
pub mod runs;
pub mod safety;
mod session;

pub use account::{
    database_exists, load_account, save_account, setup_complete, wipe_local_data, AccountMeta,
};
pub use chat_list::{merge_seeded_channels, prune_resolved_seeds};
pub use commands::Commands;
pub use error::{Error, Result};
pub use invitations::Invitation;
pub use models::{
    group_role_rank, parse_chat_info_id, CallMedia, CallRecord, ChatId, ChatMessage, ChatPreview,
    ChatRelayInfo, ChatSettings, ChatTag, ChatType, ConnLink, DeletedState, DeliveryStatus, Event,
    FileInfo, FileKind, FileTransferState, GroupChatScope, GroupInvitationInfo, GroupMemberInfo,
    GroupProfileUpdate, GroupSupportChat, MemberReceipt, MessageInfo, MessageQuote, ReceiptState, MessageSender, NtfFilter,
    ProfileFields, ReactionCount, ReactionMember, UserProfile, GROUP_ROLES, KNOWN_REACTIONS,
};
#[cfg(target_env = "musl")]
pub use musl_compat::{compat_status_line, ensure_ld_preload_reexec, is_musl_host};
pub use parse::{auto_receive_candidates, is_channel_short_link};
pub use paths::{
    data_dir, db_prefix, default_libs_dir, files_dir, init_process_env, private_cache_dir,
    set_data_dir, temp_dir, wipe_plaintext_cache,
};
pub use preferences::FeatureAllow;
pub use rich_text::{
    accept_shortcode, expand_emoji_shortcodes, humanize_simplex_links, is_short_emoji, link_host,
    outgoing_text, parse_formatted_text, pending_shortcode_query, profile_text, simplex_link_label,
    suggest_emoji_shortcodes, EmojiSuggestion, FormatColor, SimplexLinkKind, TextSegment,
    TextStyle, LARGE_EMOJI_MAX,
};
pub use session::{ReceiveOutcome, Session, SessionConfig};

/// Tests that start libsimplex hold this: Argon2 hashing on another thread
/// while the Haskell runtime starts up crashes the test process (the apps
/// only hash after the database is open — covered by the smoke test).
#[cfg(test)]
pub(crate) static LIBSIMPLEX_STARTUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

