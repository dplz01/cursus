//! Shared SimpleX session layer for Cursus.
//!
//! Talks to the official `libsimplex` C ABI. Does not reimplement SMP.

mod account;
mod chat_list;
mod commands;
mod error;
mod ffi;
mod models;
#[cfg(target_env = "musl")]
mod musl_compat;
mod paths;
pub mod preferences;
pub mod roles;
pub mod safety;
mod rich_text;
mod session;

pub use account::{
    account_path, database_exists, load_account, save_account, setup_complete, wipe_local_data,
    AccountMeta,
};
pub use chat_list::{merge_seeded_channels, prune_resolved_seeds};
pub use commands::Commands;
pub use models::parse_chat_info_id;
pub use error::{Error, Result};
pub use models::{
    group_role_rank, CallInfo, CallMedia, ChatId, ChatMessage, ChatPreview, ChatRelayInfo,
    ChatSettings, ChatTag, ChatType, ConnLink, DeletedState, DeliveryStatus, Event, FileInfo,
    FileKind, FileTransferState, GroupChatScope, GroupMemberInfo, GroupProfileUpdate,
    GroupSupportChat, MessageContent, MessageQuote, MessageSender, NtfFilter, ProfileFields,
    MessageInfo, ReactionCount, ReactionMember, UserProfile, GROUP_ROLES, KNOWN_REACTIONS,
    MAX_IMAGE_SIZE_AUTO_RCV, MAX_VIDEO_SIZE_AUTO_RCV, MAX_VOICE_SIZE_AUTO_RCV,
};
pub use rich_text::{
    accept_shortcode, emoji_presentation, expand_emoji_shortcodes, is_short_emoji, outgoing_text,
    parse_formatted_text, pending_shortcode_query,
    suggest_emoji_shortcodes, humanize_simplex_links, link_host, simplex_link_label, EmojiSuggestion, FormatColor,
    SimplexLinkKind, TextSegment, TextStyle, LARGE_EMOJI_MAX,
};
#[cfg(target_env = "musl")]
pub use musl_compat::{compat_status_line, ensure_ld_preload_reexec, is_musl_host};
pub use preferences::FeatureAllow;
pub use paths::{
    data_dir, db_prefix, default_libs_dir, files_dir, plaintext_cache_path, private_cache_dir,
    init_process_env, resolve_local_file, runtime_cache_dir, set_data_dir,
    temp_dir,
};
pub use session::{
    auto_receive_candidates, auto_receive_from_chat_item, wipe_plaintext_cache, ReceiveOutcome, is_channel_short_link, is_short_conn_link,
    shareable_conn_uri, Session, SessionConfig,
};

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn open_session_against_downloaded_lib() {
        let dir = std::env::temp_dir().join(format!("cursus-smoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = SessionConfig {
            libs_dir: default_libs_dir(),
            db_prefix: dir.join("simplex_v1"),
            db_key: zeroize::Zeroizing::new("smoke".to_string()),
            migration_confirm: "yesUp".into(),
            initial_display_name: "Smoke".into(),
            encrypt_local_files: true,
        };
        let mut session = Session::open(cfg).expect("open session");
        session.start_chat().expect("start chat");
        let _ = session.list_chats().expect("list chats");
    }

    #[test]
    fn local_notes_use_create_not_send() {
        let chat = ChatId {
            chat_type: ChatType::Local,
            api_id: 1,
            scope: None,
        };
        let cmd = Commands::send_text(&chat, "hello");
        assert!(
            cmd.starts_with("/_create *1 json "),
            "local notes must use /_create, got: {cmd}"
        );
        assert!(!cmd.contains("/_send"), "must not use /_send for local notes");
    }

    #[test]
    fn get_chat_command_shape() {
        let chat = ChatId {
            chat_type: ChatType::Local,
            api_id: 1,
            scope: None,
        };
        assert_eq!(Commands::get_chat(&chat, 50), "/_get chat *1 count=50");
        let support = ChatId::with_support(42, None);
        assert_eq!(
            Commands::get_chat(&support, 30),
            "/_get chat #42(_support) count=30"
        );
        let member = ChatId::with_support(42, Some(7));
        assert_eq!(
            Commands::get_chat(&member, 30),
            "/_get chat #42(_support:7) count=30"
        );
        assert_eq!(
            Commands::delete_member_support_chat(42, 7),
            "/_delete member chat #42 7"
        );
        let send = Commands::send_text(&support, "help");
        assert!(
            send.starts_with("/_send #42(_support) "),
            "support sends must keep scope, got: {send}"
        );
    }

    #[test]
    fn invite_command_shapes() {
        assert_eq!(Commands::create_invitation(1), "/_connect 1");
        assert_eq!(
            Commands::connect_link(1, "https://smp14.simplex.im/i#abc/def"),
            "/connect https://smp14.simplex.im/i#abc/def"
        );
        assert_eq!(Commands::show_address(1), "/_show_address 1");
        let on = Commands::address_settings(1, false, Some(false));
        assert!(on.starts_with("/_address_settings 1 "));
        assert!(on.contains(r#""autoAccept":{"acceptIncognito":false}"#));
        let off = Commands::address_settings(1, false, None);
        assert!(off.contains(r#""autoAccept":null"#));
    }

    #[test]
    fn reaction_members_command_shape() {
        let cmd = Commands::reaction_members(1, 9, 42, "\u{1F44D}");
        assert!(cmd.starts_with("/_reaction members 1 #9 42 "));
        assert!(cmd.contains(r#""type":"emoji""#));
        assert!(cmd.contains("\u{1F44D}"));
    }

    #[test]
    fn profile_command_shapes() {
        let fields = ProfileFields {
            display_name: "Ada".into(),
            full_name: "".into(),
            short_descr: "bio".into(),
            description: "".into(),
            image: None,
            contact_link: None,
            preferences: None,
        };
        let cmd = Commands::update_profile(1, &fields);
        assert!(cmd.starts_with("/_profile 1 "));
        assert!(cmd.contains(r#""displayName":"Ada""#));
        assert!(cmd.contains(r#""shortDescr":"bio""#));
        assert_eq!(Commands::profile_address(1, true), "/_profile_address 1 on");
        assert_eq!(Commands::profile_address(1, false), "/_profile_address 1 off");
        assert_eq!(Commands::set_contact_alias(3, "Bob"), "/_set alias @3 Bob");
        assert_eq!(Commands::set_contact_alias(3, ""), "/_set alias @3");
        let prefs = serde_json::json!({
            "timedMessages": { "allow": "yes", "ttl": 86400 },
            "fullDelete": { "allow": "no" },
            "reactions": { "allow": "yes" },
            "voice": { "allow": "yes" },
            "calls": { "allow": "yes" }
        });
        let pref_cmd = Commands::set_contact_prefs(3, &prefs);
        assert!(pref_cmd.starts_with("/_set prefs @3 "));
        assert!(pref_cmd.contains(r#""timedMessages""#));
        assert!(pref_cmd.contains(r#""fullDelete""#));
        assert!(pref_cmd.contains(r#""reactions""#));
        assert!(pref_cmd.contains(r#""voice""#));
        assert!(pref_cmd.contains(r#""calls""#));
    }

    #[test]
    fn group_channel_call_command_shapes() {
        let g = Commands::new_group(1, "Friends");
        assert!(g.starts_with("/_group 1 incognito=off "));
        assert!(g.contains(r#""displayName":"Friends""#));
        let ch = Commands::new_public_group(1, &[10, 20], "News");
        assert!(ch.starts_with("/_public group 1 incognito=off 10,20 "));
        assert!(ch.contains(r#""displayName":"News""#));
        assert_eq!(Commands::get_user_servers(1), "/_servers 1");
        assert_eq!(Commands::activate_chat(), "/_app activate restore=on");
        assert_eq!(Commands::resubscribe_all(), "/_resubscribe all");
        assert!(is_channel_short_link("simplex:/c#abc?h=smp18.simplex.im"));
        assert!(is_channel_short_link("https://smp18.simplex.im/c#abc"));
        assert!(!is_channel_short_link("https://smp18.simplex.im/g#abc"));
        assert!(!is_channel_short_link("simplex:/contact#/?v=1"));
        assert_eq!(
            shareable_conn_uri("simplex:/c#abc?h=smp18.simplex.im"),
            "simplex:/c#abc?h=smp18.simplex.im"
        );
        assert_eq!(
            shareable_conn_uri("simplex:/contact#/?v=1&smp=x"),
            "https://simplex.chat/contact#/?v=1&smp=x"
        );
        let invite = Commands::call_invite(5, CallMedia::Audio);
        assert!(invite.starts_with("/_call invite @5 "));
        assert!(invite.contains(r#""media":"audio""#));
        assert_eq!(Commands::call_reject(5), "/_call reject @5");
        assert_eq!(Commands::call_end(5), "/_call end @5");
        assert_eq!(Commands::call_get(), "/_call get");
        assert_eq!(Commands::list_members(9), "/_members #9");
        assert_eq!(Commands::add_member(9, 3, "member"), "/_add #9 3 member");
        let profile = GroupProfileUpdate {
            display_name: "G".into(),
            welcome_message: "".into(),
            image: None,
            ..GroupProfileUpdate::default()
        };
        assert!(Commands::update_group_profile(9, &profile).starts_with("/_group_profile #9 "));
        let with_img = GroupProfileUpdate {
            display_name: "G".into(),
            image: Some("data:image/jpg;base64,xx".into()),
            ..GroupProfileUpdate::default()
        };
        assert!(Commands::update_group_profile(9, &with_img)
            .contains(r#""image":"data:image/jpg;base64,xx""#));
        assert_eq!(Commands::create_group_link(9, "member"), "/_create link #9 member");
        assert_eq!(Commands::get_group_link(9), "/_get link #9");
        assert_eq!(Commands::leave_group(9), "/_leave #9");
        assert_eq!(
            Commands::set_member_role(9, &[4], "admin"),
            "/_member role #9 4 admin"
        );
        assert_eq!(
            Commands::remove_members(9, &[4], false),
            "/_remove #9 4 messages=off"
        );
        assert_eq!(Commands::set_group_link_role(9, "observer"), "/_set link role #9 observer");
        assert_eq!(Commands::delete_group_link(9), "/_delete link #9");
        assert_eq!(Commands::set_group_alias(9, "Work"), "/_set alias #9 Work");
    }

    #[test]
    fn read_chat_command_shape() {
        let chat = ChatId {
            chat_type: ChatType::Direct,
            api_id: 2,
            scope: None,
        };
        assert_eq!(Commands::read_chat(&chat), "/_read chat @2");
    }

    #[test]
    fn contact_request_command_shapes() {
        assert_eq!(
            Commands::accept_contact_request(1),
            "/_accept incognito=off 1"
        );
        assert_eq!(Commands::reject_contact_request(1), "/_reject 1");
    }

    #[test]
    fn send_file_command_shape() {
        let chat = ChatId {
            chat_type: ChatType::Direct,
            api_id: 2,
            scope: None,
        };
        let cmd = Commands::send_file(&chat, "/tmp/photo.jpg", "photo.jpg", true, None);
        assert!(cmd.starts_with("/_send @2 "));
        assert!(cmd.contains(r#""type":"image""#));
        assert!(cmd.contains("/tmp/photo.jpg"));
        let file_cmd = Commands::send_file(&chat, "/tmp/doc.pdf", "doc.pdf", false, None);
        assert!(file_cmd.contains(r#""type":"file""#));
        let voice = Commands::send_voice(&chat, "voice.m4a", 12, Some(("k", "n")));
        assert!(voice.contains(r#""type":"voice""#));
        assert!(voice.contains(r#""duration":12"#));
        assert!(voice.contains(r#""fileKey":"k""#));
        assert!(voice.contains(r#""fileNonce":"n""#));
        assert!(voice.contains(r#""filePath":"voice.m4a""#));
        assert_eq!(
            Commands::receive_file(3, false),
            "/freceive 3 approved_relays=off encrypt=on"
        );
        assert_eq!(
            Commands::receive_file(3, true),
            "/freceive 3 approved_relays=on encrypt=on"
        );
        assert_eq!(Commands::set_encrypt_local_files(true), "/_files_encrypt on");
        assert_eq!(Commands::set_encrypt_local_files(false), "/_files_encrypt off");
        let react = Commands::reaction(
            &ChatId {
                chat_type: ChatType::Direct,
                api_id: 2,
            scope: None,
        },
            99,
            "👍",
            true,
        );
        assert_eq!(
            react,
            r#"/_reaction @2 99 on {"emoji":"👍","type":"emoji"}"#
        );
    }

    #[test]
    fn delivery_status_parse() {
        use serde_json::json;
        assert_eq!(
            DeliveryStatus::parse(&json!({"type": "sndNew"})),
            DeliveryStatus::Pending
        );
        assert_eq!(
            DeliveryStatus::parse(&json!({"type": "sndSent", "sndProgress": "complete"})),
            DeliveryStatus::Sent
        );
        assert_eq!(
            DeliveryStatus::parse(&json!({
                "type": "sndRcvd",
                "msgRcptStatus": "ok",
                "sndProgress": "complete"
            })),
            DeliveryStatus::Delivered
        );
        assert_eq!(
            DeliveryStatus::parse(&json!({"type": "sndErrorAuth"})),
            DeliveryStatus::Error
        );
    }

    #[test]
    fn chat_menu_command_shapes() {
        let chat = ChatId {
            chat_type: ChatType::Direct,
            api_id: 9,
            scope: None,
        };
        assert_eq!(Commands::unread_chat(&chat, true), "/_unread chat @9 on");
        assert_eq!(Commands::clear_chat(&chat), "/_clear chat @9");
        assert_eq!(Commands::delete_chat(&chat), "/_delete @9 full notify=on");
        let settings = ChatSettings {
            enable_ntfs: NtfFilter::None,
            favorite: true,
            send_rcpts: Some(true),
        };
        let settings_cmd = Commands::set_chat_settings(&chat, &settings);
        assert!(settings_cmd.starts_with("/_settings @9 "));
        assert!(settings_cmd.contains(r#""enableNtfs":"none""#));
        assert!(settings_cmd.contains(r#""favorite":true"#));
        assert!(settings_cmd.contains(r#""sendRcpts":true"#));
        assert_eq!(
            Commands::set_chat_ttl(1, &chat, Some(86400)),
            "/_ttl 1 @9 86400"
        );
        assert_eq!(
            Commands::set_chat_ttl(1, &chat, None),
            "/_ttl 1 @9 default"
        );
        assert_eq!(Commands::set_chat_tags(&chat, &[1, 2]), "/_tags @9 1,2");
        assert_eq!(Commands::get_chat_tags(1), "/_get tags 1");
    }
}

#[cfg(test)]
mod receipts_probe {
    use super::*;

    #[test]
    #[ignore]
    fn delivery_receipts_toggle_round_trip() {
        let dir = std::env::temp_dir().join(format!("cursus-rcpt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = SessionConfig {
            libs_dir: default_libs_dir(),
            db_prefix: dir.join("simplex_v1"),
            db_key: zeroize::Zeroizing::new("probe".to_string()),
            migration_confirm: "yesUp".into(),
            initial_display_name: "Probe".into(),
            encrypt_local_files: true,
        };
        let mut session = Session::open(cfg).expect("open");
        session.start_chat().expect("start");
        let before = session.refresh_user().expect("user");
        println!("before: contacts={} groups={}", before.send_rcpts_contacts, before.send_rcpts_small_groups);
        let r = session.set_delivery_receipts(false, false);
        println!("set contacts off -> {:?}", r.as_ref().map(|u| (u.send_rcpts_contacts, u.send_rcpts_small_groups)));
        assert!(!r.unwrap().send_rcpts_contacts);
        let g = session.set_delivery_receipts(true, false).unwrap();
        assert!(!g.send_rcpts_contacts && !g.send_rcpts_small_groups);
        drop(session);
        // Reopen: the core reloads the user from the DB.
        let cfg = SessionConfig {
            libs_dir: default_libs_dir(),
            db_prefix: dir.join("simplex_v1"),
            db_key: zeroize::Zeroizing::new("probe".to_string()),
            migration_confirm: "yesUp".into(),
            initial_display_name: "Probe".into(),
            encrypt_local_files: true,
        };
        let mut session = Session::open(cfg).expect("reopen");
        session.start_chat().expect("restart");
        let after = session.refresh_user().unwrap();
        assert!(!after.send_rcpts_contacts && !after.send_rcpts_small_groups, "persisted");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
