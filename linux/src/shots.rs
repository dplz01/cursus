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
        }
        if want("new") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.compose_open = true;
            render(&app, DESKTOP, &format!("new-{t}"));
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
