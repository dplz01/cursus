//! Headless screenshots of real Cursus screens (design review harness).
//!
//! `cargo test -p cursus-linux shots -- --ignored --nocapture` renders a set of
//! screens with demo data through iced's headless renderer (tiny-skia) and
//! writes PNGs to `$CURSUS_SHOTS_DIR` (default `target/shots`). No session,
//! no tray, nothing touches the user's data or desktop.

use std::path::PathBuf;

use cursus_core::{
    ChatId, ChatMessage, ChatPreview, ChatSettings, ChatType, DeliveryStatus, GroupMemberInfo,
    MessageSender, ProfileFields, ReactionCount,
};
use iced::advanced::renderer::Headless;
use iced::Size;
use iced_runtime::user_interface::{Cache, UserInterface};

use crate::desktop::{DesktopPrefs, ThemePreference};
use crate::{App, GroupSettingsPane, Message, NavSection};

fn load_fonts() {
    crate::emoji_font::prefer_color_emoji();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    let mut fs = iced::advanced::graphics::text::font_system()
        .write()
        .expect("font system");
    for f in [
        "NotoSans-Regular.ttf",
        "NotoSans-Bold.ttf",
        "NotoSans-Italic.ttf",
        "NotoColorEmoji.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(dir.join(f)) {
            fs.load_font(std::borrow::Cow::Owned(bytes));
        }
    }
}

fn profile(name: &str, bio: &str) -> ProfileFields {
    ProfileFields {
        display_name: name.into(),
        short_descr: bio.into(),
        ..ProfileFields::default()
    }
}

fn chat(id: i64, kind: ChatType, title: &str, preview: &str, unread: i64) -> ChatPreview {
    ChatPreview {
        id: ChatId::new(kind, id),
        title: title.into(),
        preview: preview.into(),
        unread,
        unread_chat: false,
        settings: ChatSettings::default(),
        tag_ids: Vec::new(),
        chat_item_ttl: None,
        pending_request_id: None,
        peer_profile: profile(title, ""),
        local_alias: String::new(),
        is_channel: false,
        membership_role: None,
        group_preferences: None,
        member_admission: None,
        public_group: None,
        contact_preferences: None,
        contact_merged_preferences: None,
        members_require_attention: 0,
        membership_support_unread: 0,
        membership_has_support_chat: false,
        last_ts: None,
    }
}

fn msg(id: i64, text: &str, outgoing: bool, time: &str) -> ChatMessage {
    ChatMessage {
        item_id: id,
        text: text.into(),
        caption: String::new(),
        segments: Vec::new(),
        outgoing,
        created_at: Some(format!("2026-11-06T{time}:00Z")),
        delivery: DeliveryStatus::Delivered,
        file: None,
        reactions: Vec::new(),
        quote: None,
        deleted: None,
        sender: None,
        is_announcement: false,
        group_invitation: None,
        edited: false,
        editable: outgoing,
    }
}

fn member(id: i64, name: &str, role: &str, status: &str, bio: Option<&str>, me: bool) -> GroupMemberInfo {
    GroupMemberInfo {
        group_member_id: id,
        contact_id: if me { None } else { Some(100 + id) },
        display_name: name.into(),
        role: role.into(),
        status: status.into(),
        blocked_by_admin: false,
        show_messages: true,
        support_chat: None,
        image: None,
        public_name: name.into(),
        short_descr: bio.map(str::to_string),
        is_me: me,
    }
}

/// App populated with demo chats; `dark` picks the palette.
fn demo_app(dark: bool, width: f32, height: f32) -> App {
    let prefs = DesktopPrefs {
        tray: false,
        close_to_tray: false,
        theme: if dark { ThemePreference::Dark } else { ThemePreference::Light },
        ..DesktopPrefs::default()
    };
    let (mut app, _) = App::boot(prefs, None);
    app.auth = None;
    app.demo = true;
    app.width = width;
    app.height = height;

    let mut alice = chat(1, ChatType::Direct, "Alice Marchetti", "See you at the station at 9 — bring the tickets!", 3);
    alice.peer_profile = profile("Alice Marchetti", "Climbing, coffee, Rust.");
    let mut team = chat(2, ChatType::Group, "Cursus devs", "Bob: the invite page looks great now", 12);
    team.membership_role = Some("owner".into());
    team.group_preferences = Some(serde_json::json!({
        "directMessages": {"enable": "on"},
        "reactions": {"enable": "on"},
        "voice": {"enable": "on"},
        "support": {"enable": "on"}
    }));
    let mut fav = chat(3, ChatType::Direct, "Marco", "https://smp9.simplex.im/i#3iqO73eh1l09QmQfRwMyM2MplnycyJT7/HzD4YSRIOBK_Bvi1gZiXI1BY", 0);
    fav.settings.favorite = true;
    let mut muted = chat(4, ChatType::Group, "Climbing club", "Sunday: Arco, meet 8:00 at the parking lot near the crag", 0);
    muted.settings.enable_ntfs = cursus_core::NtfFilter::None;
    let mut notes = chat(5, ChatType::Local, "Notes", "Buy oat milk", 0);
    let mut chan = chat(6, ChatType::Group, "SimpleX news", "v6.5 is out: channels, relays and more", 1);
    chan.is_channel = true;
    let stamp = |c: &mut ChatPreview, ts: &str| c.last_ts = Some(ts.into());
    stamp(&mut alice, "2026-11-06T09:06:00Z");
    stamp(&mut team, "2026-11-06T08:41:00Z");
    stamp(&mut fav, "2026-11-05T18:20:00Z");
    stamp(&mut muted, "2026-11-03T07:15:00Z");
    stamp(&mut notes, "2026-10-12T12:00:00Z");
    stamp(&mut chan, "2026-11-06T07:30:00Z");
    app.chats = vec![alice, team, fav, muted, notes, chan];

    app.messages = vec![
        {
            let mut m = msg(1, "Hey! Are we still on for tomorrow?", false, "21:58");
            m.created_at = Some("2026-11-05T21:58:00Z".into());
            m
        },
        msg(2, "Yes 🙂 train at 9:12", true, "09:01"),
        {
            let mut m = msg(3, "Perfect. I'll grab coffee for both of us ☕", false, "09:02");
            m.reactions = vec![ReactionCount { emoji: "❤".into(), user_reacted: true, total: 1 }];
            m
        },
        msg(7, "Also bringing the rope", true, "09:03"),
        msg(4, "You're the best. Also: did you see the new Cursus build? The group invite page is really nice now.", true, "09:05"),
        msg(5, "See you at the station at 9 — bring the tickets!", false, "09:06"),
    ];
    app
}

fn select(app: &mut App, index: usize) {
    app.selected = Some(index);
    app.selected_id = Some(app.chats[index].id.clone());
}

fn group_page(app: &mut App, pane: GroupSettingsPane) {
    select(app, 1);
    app.messages = vec![{
        let mut m = msg(10, "the invite page looks great now", false, "10:00");
        m.sender = Some(MessageSender {
            group_member_id: 2,
            contact_id: Some(102),
            display_name: "Bob".into(),
            image: None,
            role: "admin".into(),
        });
        m
    }];
    let _ = app.open_group_settings();
    let g = app.group_settings.as_mut().expect("group settings");
    g.members_loading = false;
    g.members = vec![
        member(1, "Davide", "owner", "connected", Some("Building Cursus"), true),
        member(2, "Bob Wilkinson", "admin", "connected", Some("Haskell & hiking"), false),
        member(3, "Carla", "member", "connected", None, false),
        member(4, "Dimitri Alexandropoulos-Konstantinidis the Third", "member", "invited", Some("A very long bio that should never wrap onto a second line in the list"), false),
        member(5, "Eve", "observer", "connected", None, false),
    ];
    g.link = Some("https://smp9.simplex.im/g#Xk2pQ7vN1sLm9aR4tY8uW3eZ6cB0dF5hJ".into());
    g.pane = pane;
}

fn render(app: &App, size: Size, name: &str) {
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("headless renderer");
    let theme = app.iced_theme();
    let mut ui = UserInterface::build(
        app.view(iced::window::Id::unique()),
        size,
        Cache::default(),
        &mut renderer,
    );
    let mut messages: Vec<Message> = Vec::new();
    let _ = ui.update(
        &[iced::Event::Window(iced::window::Event::RedrawRequested(
            iced::time::Instant::now(),
        ))],
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    let base = iced::theme::Base::base(&theme);
    ui.draw(
        &mut renderer,
        &theme,
        &iced::advanced::renderer::Style {
            text_color: base.text_color,
        },
        iced::mouse::Cursor::Unavailable,
    );
    let scale = 1.0;
    let px = iced::Size::new(size.width as u32, size.height as u32);
    let rgba = renderer.screenshot(px, scale, base.background_color);
    let dir = std::env::var("CURSUS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/shots"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    image::save_buffer(&path, &rgba, px.width, px.height, image::ColorType::Rgba8).unwrap();
    eprintln!("wrote {}", path.display());
}

const DESKTOP: Size = Size::new(1200.0, 760.0);
const MOBILE: Size = Size::new(400.0, 820.0);

#[test]
#[ignore]
fn shots() {
    let only = std::env::var("CURSUS_SHOTS").ok();
    let want = |n: &str| only.as_deref().is_none_or(|o| o.split(',').any(|x| n.starts_with(x)));
    for dark in [true, false] {
        let t = if dark { "dark" } else { "light" };

        if want("chat") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            select(&mut app, 0);
            render(&app, DESKTOP, &format!("chat-{t}"));
        }
        if want("group") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            group_page(&mut app, GroupSettingsPane::Overview);
            render(&app, DESKTOP, &format!("group-{t}"));
        }
        if want("invite") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            group_page(&mut app, GroupSettingsPane::AddMember);
            render(&app, DESKTOP, &format!("invite-{t}"));
        }
        if want("prefs") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            group_page(&mut app, GroupSettingsPane::Preferences);
            render(&app, DESKTOP, &format!("prefs-{t}"));
        }
        if want("settings") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.nav = NavSection::Settings;
            app.own_profile.fields = profile("Davide", "Building Cursus");
            render(&app, DESKTOP, &format!("settings-{t}"));
        }
        if want("contact") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            select(&mut app, 0);
            let _ = app.open_contact_profile();
            render(&app, DESKTOP, &format!("contact-{t}"));
        }
        if want("calls") {
            for (size, name) in [(DESKTOP, "calls"), (MOBILE, "mcalls")] {
                let mut app = demo_app(dark, size.width, size.height);
                app.nav = NavSection::Calls;
                let rec = |id: i64, contact: i64, name: &str, outgoing: bool, status: &str, secs: i64, ts: &str| {
                    cursus_core::CallRecord {
                        item_id: id,
                        contact_id: contact,
                        peer_name: name.into(),
                        outgoing,
                        status: status.into(),
                        duration_secs: secs,
                        ts: Some(ts.into()),
                    }
                };
                app.call.set_history_for_test(vec![
                    rec(9, 1, "Alice Marchetti", true, "ended", 754, "2026-11-06T09:10:00Z"),
                    rec(8, 3, "Marco", false, "missed", 0, "2026-11-06T07:02:00Z"),
                    rec(7, 1, "Alice Marchetti", false, "ended", 65, "2026-11-05T21:40:00Z"),
                    rec(6, 3, "Marco", true, "pending", 0, "2026-11-04T12:00:00Z"),
                    rec(5, 1, "Alice Marchetti", false, "rejected", 0, "2026-10-28T18:30:00Z"),
                ]);
                render(&app, size, &format!("{name}-{t}"));
            }
        }
        if want("mobile") {
            let app = demo_app(dark, MOBILE.width, MOBILE.height);
            render(&app, MOBILE, &format!("mobile-list-{t}"));
        }
        if want("msettings") {
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            app.nav = NavSection::Settings;
            app.own_profile.fields = profile("Davide", "Building Cursus");
            render(&app, MOBILE, &format!("msettings-{t}"));
        }
        if want("links") {
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            select(&mut app, 2);
            let v = serde_json::json!([
                {"format": null, "text": "Join us: "},
                {"format": {"type": "simplexLink", "linkType": "group",
                            "simplexUri": "https://smp9.simplex.im/g#Xk2pQ7vN1sLm9aR4tY8uW3eZ6cB0dF5hJ",
                            "smpHosts": ["smp9.simplex.im"]},
                 "text": "https://smp9.simplex.im/g#Xk2pQ7vN1sLm9aR4tY8uW3eZ6cB0dF5hJ"},
                {"format": null, "text": " and docs at "},
                {"format": "uri", "text": "simplex.chat/docs"}
            ]);
            let mut m = msg(6, "", false, "09:10");
            m.segments = cursus_core::parse_formatted_text(Some(&v));
            app.messages.push(m);
            render(&app, MOBILE, &format!("links-{t}"));
            app.pending_url = Some(crate::PendingLink {
                url: "https://smp9.simplex.im/g#Xk2pQ7vN1sLm9aR4tY8uW3eZ6cB0dF5hJ".into(),
                simplex: Some(cursus_core::SimplexLinkKind::Group),
            });
            render(&app, MOBILE, &format!("links-join-{t}"));
            app.pending_url = Some(crate::PendingLink {
                url: "https://simplex.chat/docs".into(),
                simplex: None,
            });
            render(&app, MOBILE, &format!("links-web-{t}"));
        }
        if want("emoji") {
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            select(&mut app, 0);
            app.messages.push(msg(8, "I don't like these emojis \u{2764}\u{1F604}", true, "14:27"));
            app.messages.push(msg(9, "Text-style ones: \u{263A} \u{2764}\u{FE0F} \u{2600} \u{270C} \u{2B50} 👍🏽 🇮🇹 👨‍👩‍👧 ok → ✓", false, "14:28"));
            app.set_composer_text("\u{2764}\u{1F604} hi");
            render(&app, MOBILE, &format!("emoji-{t}"));
            app.set_composer_text("love you :he");
            render(&app, MOBILE, &format!("emoji-suggest-{t}"));
        }
        if want("gate") {
            for (size, name) in [(DESKTOP, "gate"), (MOBILE, "mgate")] {
                let mut app = demo_app(dark, size.width, size.height);
                app.auth = Some(crate::AuthScreen::WelcomeName { name: "Davide".into() });
                render(&app, size, &format!("{name}-welcome-{t}"));
                app.auth = Some(crate::AuthScreen::WelcomePassword {
                    name: "Davide".into(),
                    password: crate::Secret::new("hunter22".into()),
                    confirm: crate::Secret::default(),
                });
                render(&app, size, &format!("{name}-password-{t}"));
                app.auth = Some(crate::AuthScreen::Unlock {
                    display_name: "Davide".into(),
                    password: crate::Secret::default(),
                });
                app.status = "Could not open database (wrong password?).".into();
                render(&app, size, &format!("{name}-unlock-{t}"));
                app.auto_unlocking = true;
                render(&app, size, &format!("{name}-auto-{t}"));
            }
        }
        if want("voice") {
            for (size, name) in [(DESKTOP, "voice"), (MOBILE, "mvoice")] {
                let mut app = demo_app(dark, size.width, size.height);
                select(&mut app, 0);
                app.voice_draft = Some(crate::VoiceDraft {
                    path: std::env::temp_dir().join("cursus-shot-voice.wav"),
                    duration_secs: 7,
                    peaks: (0..96).map(|i| (i as f32 * 0.37).sin().abs() * 0.9 + 0.1).collect(),
                });
                render(&app, size, &format!("{name}-draft-{t}"));
                // Recording: a short-lived dummy process stands in for the recorder.
                app.voice_draft = None;
                let child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
                app.voice_rec = Some(crate::VoiceRec {
                    path: std::env::temp_dir().join("cursus-shot-rec.wav"),
                    child,
                    started: std::time::Instant::now(),
                });
                render(&app, size, &format!("{name}-rec-{t}"));
                if let Some(mut rec) = app.voice_rec.take() {
                    let _ = rec.child.kill();
                    let _ = rec.child.wait();
                }
            }
        }
        if want("lock") {
            for (size, name) in [(DESKTOP, "lock"), (MOBILE, "mlock")] {
                let mut app = demo_app(dark, size.width, size.height);
                app.locked = Some(crate::LockState::default());
                render(&app, size, &format!("{name}-{t}"));
                app.locked = Some(crate::LockState {
                    password: crate::Secret::new("wrong".into()),
                    failures: 3,
                    error: Some("Wrong password. Try again in 2 s.".into()),
                    ..Default::default()
                });
                render(&app, size, &format!("{name}-error-{t}"));
            }
        }
        if want("msettings-privacy") {
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            app.nav = NavSection::Settings;
            app.desktop_prefs.app_lock = true;
            render(&app, Size::new(MOBILE.width, 2200.0), &format!("msettings-privacy-{t}"));
        }
        if want("privacy") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.desktop_prefs.app_lock = true;
            app.nav = NavSection::Settings;
            app.settings_section = crate::SettingsSection::Privacy;
            render(&app, DESKTOP, &format!("privacy-{t}"));
            app.privacy_confirm = Some(crate::PrivacyConfirm::UnknownFileServers {
                files: vec![(1, "photo.jpg".into())],
                servers: vec!["files.example.org".into()],
            });
            render(&app, DESKTOP, &format!("privacy-servers-{t}"));
            app.privacy_confirm = Some(crate::PrivacyConfirm::EnableKeyringUnlock {
                password: crate::Secret::default(),
                checking: false,
                error: None,
            });
            render(&app, DESKTOP, &format!("privacy-keyring-{t}"));
            app.privacy_confirm = None;
            app.setting_info = Some(crate::SettingInfo::ProtectIp);
            render(&app, MOBILE, &format!("privacy-info-{t}"));
        }
        if want("new") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.compose_open = true;
            app.invitation = Some(cursus_core::Invitation {
                link: cursus_core::ConnLink {
                    display: "https://smp12.simplex.im/i#OzM8bkTdaVhHVWJbNe5tUa1Ru3TnHlHsTMx4jjOwTSE".into(),
                    full: String::new(),
                    short: None,
                },
                conn_id: 1,
                incognito: false,
            });
            render(&app, DESKTOP, &format!("new-{t}"));
            let mut phone = demo_app(dark, MOBILE.width, MOBILE.height);
            phone.compose_open = true;
            phone.invitation = app.invitation.clone();
            phone.desktop_prefs.incognito = true;
            if let Some(inv) = phone.invitation.as_mut() {
                inv.incognito = true;
            }
            render(&phone, MOBILE, &format!("new-mobile-incognito-{t}"));
            phone.setting_info = Some(crate::SettingInfo::Connect);
            render(&phone, MOBILE, &format!("new-info-{t}"));
            app.compose_open = false;
            app.nav = NavSection::Settings;
            app.settings_section = crate::SettingsSection::Address;
            render(&app, DESKTOP, &format!("settings-address-{t}"));
            app.address_link = Some("https://smp12.simplex.im/a#lrdvu2d8A1GumSmoKb2krQmtKhWXq-tyGpHuM7aMwsw".into());
            render(&app, DESKTOP, &format!("settings-address-link-{t}"));
        }
    }
}

#[test]
fn composer_long_press_restores_selection() {
    use crate::edit_menu::ComposerMenuAction;
    use iced::widget::text_editor::{Action, Content};
    let (mut app, _) = App::boot(
        DesktopPrefs {
            tray: false,
            close_to_tray: false,
            ..DesktopPrefs::default()
        },
        None,
    );
    app.composer = Content::with_text("hello world");
    let _ = app.update(Message::ComposerEdit(Action::SelectAll));
    assert_eq!(app.composer.selection().as_deref(), Some("hello world"));
    // Finger-down on the text = a click, which clears the selection…
    let _ = app.update(Message::ComposerEdit(Action::Click(iced::Point::new(5.0, 5.0))));
    let _ = app.update(Message::ComposerEdit(Action::Drag(iced::Point::new(6.0, 5.0))));
    assert_eq!(app.composer.selection(), None, "tap clears the selection");
    // …and the long-press menu brings it back.
    let _ = app.update(Message::ComposerMenu(ComposerMenuAction::Opened { by_touch: true }));
    assert_eq!(app.composer.selection().as_deref(), Some("hello world"));
    let _ = app.update(Message::ComposerMenu(ComposerMenuAction::Cut));
    assert_eq!(app.composer.text().trim_end(), "");
}

#[test]
fn support_back_returns_to_group_page() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    group_page(&mut app, GroupSettingsPane::Overview);
    let _ = app.update(Message::OpenSupportChatList);
    assert!(app.support.is_some() && app.group_settings.is_none());
    // List → a member thread → Back (list) → Back (group page).
    let _ = app.update(Message::OpenMemberSupportChat { member_id: 2, title: "Bob".into() });
    let _ = app.update(Message::Back);
    assert!(matches!(app.support.as_ref().map(|s| &s.pane), Some(crate::SupportPane::List)));
    let _ = app.update(Message::Back);
    assert!(app.support.is_none());
    assert!(app.group_settings.is_some(), "Back must return to the group page");

    // Opened from the chat header instead: Back returns to the chat.
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    select(&mut app, 1);
    let _ = app.update(Message::OpenSupportChatList);
    let _ = app.update(Message::Back);
    assert!(app.support.is_none() && app.group_settings.is_none());
}

/// Frame-cost benchmark: what one update (fling tick, voice tick, core event)
/// costs with a long conversation open. `cargo test --release -p cursus-linux
/// frame_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn frame_bench() {
    load_fonts();
    for (label, size) in [("desktop", DESKTOP), ("mobile", MOBILE)] {
        let mut app = demo_app(true, size.width, size.height);
        select(&mut app, 0);
        let base = app.messages.clone();
        app.messages.clear();
        let n = std::env::var("CURSUS_BENCH_MSGS").ok().and_then(|v| v.parse().ok()).unwrap_or(300usize);
        for i in 0..n {
            let mut m = base[i % base.len()].clone();
            m.item_id = 1000 + i as i64;
            m.created_at = Some(format!("2026-11-{:02}T{:02}:{:02}:00Z", 1 + (i / 100).min(5), 8 + (i / 10) % 12, i % 60));
            app.messages.push(m);
        }
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::with_name("Noto Sans"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let theme = app.iced_theme();
        let style = iced::advanced::renderer::Style { text_color: iced::Color::WHITE };
        let mut cache = Cache::default();
        let (mut t_view, mut t_build, mut t_update, mut t_draw) = (0.0, 0.0, 0.0, 0.0);
        let frames = 30;
        for f in 0..frames + 3 {
            let t0 = std::time::Instant::now();
            let el = app.view(iced::window::Id::unique());
            let t1 = std::time::Instant::now();
            let mut ui = UserInterface::build(el, size, cache, &mut renderer);
            let t2 = std::time::Instant::now();
            let mut msgs: Vec<Message> = Vec::new();
            let _ = ui.update(
                &[iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
                iced::mouse::Cursor::Unavailable,
                &mut renderer,
                &mut iced::advanced::clipboard::Null,
                &mut msgs,
            );
            let t3 = std::time::Instant::now();
            ui.draw(&mut renderer, &theme, &style, iced::mouse::Cursor::Unavailable);
            let t4 = std::time::Instant::now();
            cache = ui.into_cache();
            if f >= 3 {
                t_view += (t1 - t0).as_secs_f64();
                t_build += (t2 - t1).as_secs_f64();
                t_update += (t3 - t2).as_secs_f64();
                t_draw += (t4 - t3).as_secs_f64();
            }
        }
        let ms = |t: f64| t * 1000.0 / frames as f64;
        println!(
            "BENCH {label} {n} msgs: view {:.2}ms  build+layout {:.2}ms  update {:.2}ms  draw {:.2}ms  total {:.2}ms",
            ms(t_view), ms(t_build), ms(t_update), ms(t_draw), ms(t_view + t_build + t_update + t_draw)
        );
    }
}

/// One headless frame through a persistent UI cache (like the real event
/// loop), returning the pixels and the cache for the next frame.
fn frame_pixels(
    app: &App,
    size: Size,
    renderer: &mut iced::Renderer,
    cache: Cache,
) -> (Vec<u8>, Cache) {
    let theme = app.iced_theme();
    let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, renderer);
    let mut msgs: Vec<Message> = Vec::new();
    let _ = ui.update(
        &[iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
        iced::mouse::Cursor::Unavailable,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut msgs,
    );
    let base = iced::theme::Base::base(&theme);
    ui.draw(
        renderer,
        &theme,
        &iced::advanced::renderer::Style { text_color: base.text_color },
        iced::mouse::Cursor::Unavailable,
    );
    let cache = ui.into_cache();
    let px = iced::Size::new(size.width as u32, size.height as u32);
    (renderer.screenshot(px, 1.0, base.background_color), cache)
}

/// The cached message column must never show stale content: after each kind
/// of change, a frame rendered through the warm cache has to match a frame
/// rendered from scratch.
#[test]
fn history_cache_never_goes_stale() {
    load_fonts();
    let new_renderer = || {
        iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::with_name("Noto Sans"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap()
    };
    type Mutation = (&'static str, fn(&mut App));
    let mutations: &[Mutation] = &[
        ("delivery tick", |a| a.messages[1].delivery = DeliveryStatus::Sent),
        ("edited text", |a| {
            a.messages[2].text = "Edited: make it two coffees".into();
            a.messages[2].edited = true;
        }),
        ("new reaction", |a| {
            a.messages[4].reactions.push(ReactionCount { emoji: "👍".into(), user_reacted: false, total: 2 })
        }),
        ("new message", |a| a.messages.push(msg(50, "Just arrived 🚆", false, "09:30"))),
        ("selection mode", |a| {
            a.selection_mode = true;
            a.selected_items.insert(3);
        }),
        ("focus", |a| a.focus_item = Some(4)),
        ("reveal deleted", |a| {
            a.messages[0].deleted = Some(cursus_core::DeletedState::Marked);
            a.revealed_deleted.insert(a.messages[0].item_id);
        }),
        ("theme switch", |a| {
            a.desktop_prefs.theme = ThemePreference::Light;
        }),
        ("narrower window", |a| a.width = 900.0),
        ("blur media on", |a| a.desktop_prefs.blur_media = true),
        ("chat previews off", |a| a.desktop_prefs.chat_previews = false),
        ("epoch timestamp", |a| {
            let mut m = msg(51, "Group event without a real date", false, "00:00");
            m.created_at = Some("1970-01-01T00:00:00Z".into());
            a.messages.insert(0, m);
        }),
    ];
    for (label, mutate) in mutations {
        let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
        select(&mut app, 0);
        let mut warm = new_renderer();
        let (_, cache) = frame_pixels(&app, DESKTOP, &mut warm, Cache::default());
        let (_, cache) = frame_pixels(&app, DESKTOP, &mut warm, cache);
        mutate(&mut app);
        let (cached, _) = frame_pixels(&app, DESKTOP, &mut warm, cache);
        let mut cold = new_renderer();
        let (fresh, _) = frame_pixels(&app, DESKTOP, &mut cold, Cache::default());
        assert!(cached == fresh, "stale history after: {label}");
    }
}

/// Wrong passwords back off; the right one unlocks and clears the state.
#[test]
fn lock_screen_backs_off_then_unlocks() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    app.locked = Some(crate::LockState::default());
    for _ in 0..3 {
        let _ = app.update(Message::LockVerified(false));
    }
    let lock = app.locked.as_ref().expect("still locked");
    assert_eq!(lock.failures, 3);
    assert!(lock.retry_at.is_some(), "third failure must start a delay");
    assert!(lock.password.as_str().is_empty(), "field cleared after a miss");
    // Submitting during the delay does not start a check.
    app.locked.as_mut().unwrap().password = crate::Secret::new("x".into());
    let _ = app.update(Message::LockSubmit);
    assert!(!app.locked.as_ref().unwrap().checking);
    let _ = app.update(Message::LockVerified(true));
    assert!(app.locked.is_none());
}

/// Run a widget operation to completion, like the iced runtime does
/// (`focus_next` first counts focusables, then chains the actual move).
fn run_operation(
    ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
    op: Box<dyn iced::advanced::widget::Operation<()>>,
) {
    use iced::advanced::widget::operation::Outcome;
    let mut op = op;
    loop {
        ui.operate(renderer, op.as_mut());
        match op.finish() {
            Outcome::Chain(next) => op = next,
            _ => break,
        }
    }
}

/// Tab moves between the two welcome password fields: the focused field
/// leaves Tab uncaptured (so the app's listener issues FocusNext), and
/// focus_next lands on the confirmation field.
#[test]
fn tab_moves_between_password_fields() {
    use iced::advanced::widget::operation::focusable;
    load_fonts();
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    app.auth = Some(crate::AuthScreen::WelcomePassword {
        name: "D".into(),
        password: crate::Secret::default(),
        confirm: crate::Secret::default(),
    });
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), MOBILE, Cache::default(), &mut renderer);
    // Focus the first field, then press Tab.
    run_operation(&mut ui, &renderer, Box::new(focusable::focus_next::<()>()));
    let tab = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Tab),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: Some("\t".into()),
        repeat: false,
    });
    let mut msgs: Vec<Message> = Vec::new();
    let (_, statuses) = ui.update(
        std::slice::from_ref(&tab),
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut msgs,
    );
    assert_eq!(statuses[0], iced::event::Status::Ignored, "field must not swallow Tab");
    // What FocusNext does: the second (confirm) field gets focus — typing a
    // letter now edits the confirmation, not the password.
    run_operation(&mut ui, &renderer, Box::new(focusable::focus_next::<()>()));
    let key_x = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Character("x".into()),
        modified_key: iced::keyboard::Key::Character("x".into()),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyX),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: Some("x".into()),
        repeat: false,
    });
    let mut msgs: Vec<Message> = Vec::new();
    let _ = ui.update(
        std::slice::from_ref(&key_x),
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut msgs,
    );
    assert!(
        matches!(msgs.as_slice(), [Message::WelcomeConfirmChanged(_)]),
        "typing after Tab should edit the confirm field, got {msgs:?}"
    );
}


/// Profile texts are plain text now; the pencil opens the editor sheet,
/// whose ✓ writes the value through the same draft/save messages.
#[test]
fn field_editor_edits_and_saves_profile_texts() {
    use crate::EditField;
    load_fonts();
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    app.nav = NavSection::Settings;
    app.own_profile.fields = profile("Davide", "Building Cursus");

    let _ = app.update(Message::OpenFieldEditor(EditField::OwnBio));
    assert_eq!(app.field_editor().map(|e| e.text()), Some("Building Cursus".into()));
    // The sheet renders on top of Settings.
    let _ = app.view(iced::window::Id::unique());
    let _ = app.update(Message::FieldEditorInput("  Shipping Cursus 🚀 ".into()));
    let _ = app.update(Message::FieldEditorSave);
    assert!(app.field_editor().is_none(), "✓ closes the sheet");
    assert_eq!(app.own_profile.fields.short_descr, "Shipping Cursus 🚀");

    // A name can't be emptied: the sheet stays open with an error.
    let _ = app.update(Message::OpenFieldEditor(EditField::OwnName));
    let _ = app.update(Message::FieldEditorInput("   ".into()));
    let _ = app.update(Message::FieldEditorSave);
    assert!(app.field_editor().is_some_and(|e| e.error.is_some()));
    assert_eq!(app.own_profile.fields.display_name, "Davide");

    // Cancel / Escape drop the edit.
    let _ = app.update(Message::FieldEditorInput("Someone else".into()));
    let _ = app.update(Message::Escape);
    assert!(app.field_editor().is_none());
    assert_eq!(app.own_profile.fields.display_name, "Davide");

    // Multi-line description keeps its line breaks.
    let _ = app.update(Message::OpenFieldEditor(EditField::OwnDescription));
    let _ = app.update(Message::FieldEditorEdit(iced::widget::text_editor::Action::Edit(
        iced::widget::text_editor::Edit::Paste(std::sync::Arc::new("line one\nline two\n".into())),
    )));
    let _ = app.update(Message::FieldEditorSave);
    assert_eq!(app.own_profile.fields.description, "line one\nline two");
}

/// Group pages: the welcome-message tile opens the multi-line editor.
#[test]
fn welcome_message_tile_opens_editor() {
    use crate::EditField;
    load_fonts();
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    select(&mut app, 1);
    group_page(&mut app, GroupSettingsPane::Overview);
    let Some(g) = app.group_settings.as_mut() else {
        panic!("group page open");
    };
    g.my_role = "owner".into();
    g.welcome_message = "Ciao!".into();
    let _ = app.update(Message::OpenFieldEditor(EditField::GroupWelcome));
    let editor = app.field_editor().expect("sheet open");
    assert!(editor.field.multiline());
    assert_eq!(editor.text(), "Ciao!");
    let _ = app.view(iced::window::Id::unique());
    let _ = app.update(Message::FieldEditorCancel);
    assert!(app.field_editor().is_none());
}

/// Screens touched by the Linux UI fixes (bubble stamps with emoji, group
/// reactions, plain profile texts + editor sheet):
/// `cargo test -p cursus-linux shots_ui_fixes -- --ignored --nocapture`.
#[test]
#[ignore]
fn shots_ui_fixes() {
    use crate::EditField;
    let sender = |id: i64, name: &str, role: &str| MessageSender {
        group_member_id: id,
        contact_id: Some(100 + id),
        display_name: name.into(),
        image: None,
        role: role.into(),
    };
    for dark in [true, false] {
        let t = if dark { "dark" } else { "light" };

        // Group chat from the bug report: emoji lines + reactions.
        let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
        select(&mut app, 1);
        let incoming = |id: i64, text: &str, time: &str, who: (i64, &str, &str)| {
            let mut m = msg(id, text, false, time);
            m.sender = Some(sender(who.0, who.1, who.2));
            m
        };
        let g = (2, "giubina", "owner");
        let dav = (3, "Davide ANDROID", "member");
        app.messages = vec![
            incoming(20, "😝😝😝", "12:44", dav),
            incoming(21, "Tu hai dormito bene??", "12:45", dav),
            incoming(22, "E allora che me ne faccio?!", "13:22", g),
            incoming(23, "Così così 😔", "13:23", g),
            {
                let mut m = incoming(24, "Infatti non sto tanto bene 😔😔😔", "13:23", g);
                m.reactions = vec![ReactionCount { emoji: "❤".into(), user_reacted: true, total: 1 }];
                m
            },
            incoming(25, "Ma tu oggi vai dal dottore? 🤓", "13:23", g),
            incoming(26, "Dopo ci sentiamo 🥰🥰🥰🥰🥰🥰🥰🥰", "13:23", g),
            {
                let mut m = incoming(27, "Nono", "16:36", dav);
                m.reactions = vec![ReactionCount { emoji: "👍".into(), user_reacted: false, total: 2 }];
                m
            },
            {
                let mut m = msg(28, "stasera alle 19/20? 😊😊😊😊😊😊", true, "17:51");
                m.edited = true;
                m
            },
        ];
        render(&app, MOBILE, &format!("fix-group-chat-{t}"));

        // Settings → Profile: plain texts with pencils.
        let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
        app.nav = NavSection::Settings;
        app.own_profile.fields = profile("Davide", "Building Cursus");
        render(&app, DESKTOP, &format!("fix-profile-{t}"));
        let _ = app.update(Message::OpenFieldEditor(EditField::OwnBio));
        render(&app, DESKTOP, &format!("fix-profile-edit-{t}"));

        // Group page (owner): welcome-message tile + its editor.
        let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
        group_page(&mut app, GroupSettingsPane::Overview);
        if let Some(g) = app.group_settings.as_mut() {
            g.my_role = "owner".into();
            g.short_descr = "Where Cursus gets built".into();
            g.welcome_message = "Welcome to Cursus devs! Read the roadmap first, then say hi 👋".into();
        }
        render(&app, DESKTOP, &format!("fix-group-{t}"));
        let _ = app.update(Message::OpenFieldEditor(EditField::GroupWelcome));
        render(&app, DESKTOP, &format!("fix-group-welcome-{t}"));
        let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
        group_page(&mut app, GroupSettingsPane::Overview);
        if let Some(g) = app.group_settings.as_mut() {
            g.my_role = "owner".into();
        }
        render(&app, MOBILE, &format!("fix-mgroup-{t}"));
        let _ = app.update(Message::OpenFieldEditor(EditField::GroupAlias));
        render(&app, MOBILE, &format!("fix-mgroup-alias-{t}"));
    }
}

/// A refresh that appends your own bubble scrolls to it; new incoming ones
/// only get a "New messages" divider that goes away on its timer. Opening a chat or a
/// refresh with nothing new shows none.
#[test]
fn new_messages_follow_and_label() {
    load_fonts();
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    select(&mut app, 0);
    app.pin_history_end = false;
    let mut away = app.messages.clone();
    app.history_away_from_end = true;
    let load = |app: &mut App, messages: Vec<ChatMessage>| {
        let gen = app.messages_load_gen;
        let _ = app.update(Message::MessagesLoaded { gen, result: Ok(messages) });
    };

    // Same list: nothing happens.
    load(&mut app, away.clone());
    assert_eq!(app.new_messages_from(), None);
    assert!(app.history_away_from_end);

    // Own message: follow it, no label.
    away.push(msg(30, "on my way 🚆", true, "09:10"));
    load(&mut app, away.clone());
    assert!(!app.history_away_from_end, "scrolled to the new bubble");
    assert_eq!(app.new_messages_from(), None);

    // Two incoming: label above the first, but no jump — reading older
    // messages stays where it is. A third keeps the label there.
    app.history_away_from_end = true;
    away.push(msg(31, "great!", false, "09:11"));
    away.push(msg(32, "I'm at platform 4", false, "09:11"));
    load(&mut app, away.clone());
    assert!(app.history_away_from_end, "incoming messages don't scroll");
    assert_eq!(app.new_messages_from(), Some(31));
    render(&app, MOBILE, "fix-new-messages-dark");
    away.push(msg(33, "😄", false, "09:12"));
    load(&mut app, away.clone());
    assert_eq!(app.new_messages_from(), Some(31));

    // Only the latest timer clears it.
    let _ = app.update(Message::ClearNewMessagesMarker(0));
    assert_eq!(app.new_messages_from(), Some(31), "stale timer ignored");
    let _ = app.update(Message::ClearNewMessagesMarker(1));
    assert_eq!(app.new_messages_from(), None);

    // Opening a chat never labels its history.
    app.pin_history_end = true;
    away.push(msg(34, "new on open", false, "09:13"));
    load(&mut app, away);
    assert_eq!(app.new_messages_from(), None);
}

/// Full group / contact pages (cards with buttons):
/// `cargo test -p cursus-linux shots_profile_cards -- --ignored --nocapture`.
#[test]
#[ignore]
fn shots_profile_cards() {
    for dark in [true, false] {
        let t = if dark { "dark" } else { "light" };
        for (size, name) in [(Size::new(1200.0, 1500.0), "desk"), (Size::new(400.0, 1900.0), "mob")] {
            let mut app = demo_app(dark, size.width, size.height);
            group_page(&mut app, GroupSettingsPane::Overview);
            if let Some(g) = app.group_settings.as_mut() {
                g.my_role = "owner".into();
                g.welcome_message = "Welcome to Cursus devs!\nRead the roadmap first, then say hi 👋".into();
            }
            render(&app, size, &format!("cards-group-{name}-{t}"));
            let mut app = demo_app(dark, size.width, size.height);
            select(&mut app, 0);
            let _ = app.update(Message::OpenContactProfile);
            render(&app, size, &format!("cards-contact-{name}-{t}"));
        }
    }
}

/// Opening a chat with unread messages labels the first unread one (counting
/// incoming messages from the end) and scrolls to it; none → no label.
#[test]
fn opening_a_chat_labels_unread_messages() {
    load_fonts();
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    select(&mut app, 0);
    let history = app.messages.clone(); // incoming: 1, 3, 5 (5 is newest)
    let open = |app: &mut App, unread: i64| {
        app.messages.clear();
        app.pin_history_end = true;
        app.open_unread = unread;
        let gen = app.messages_load_gen;
        let _ = app.update(Message::MessagesLoaded { gen, result: Ok(history.clone()) });
    };

    open(&mut app, 2);
    assert_eq!(app.new_messages_from(), Some(3), "two unread: 3 and 5");
    assert_eq!(app.open_unread, 0, "used once");
    render(&app, MOBILE, "fix-open-unread-dark");

    open(&mut app, 1);
    assert_eq!(app.new_messages_from(), Some(5));

    open(&mut app, 50);
    assert_eq!(app.new_messages_from(), Some(1), "more unread than loaded: oldest incoming");

    open(&mut app, 0);
    assert_eq!(app.new_messages_from(), None);
}
