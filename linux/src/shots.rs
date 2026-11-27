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

// Full-shell input tests share the process-wide physical pointer mode.
// Keep their synthetic devices from changing another test's hover mid-frame.
static POINTER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        preview_kind: cursus_core::PreviewKind::Text,
        preview_segments: Vec::new(),
        unread,
        unread_chat: false,
        settings: ChatSettings::default(),
        tag_ids: Vec::new(),
        chat_item_ttl: None,
        pending_request_id: None,
        member_request: false,
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
    alice.preview_segments = cursus_core::profile_text("See you at the station at **9** — bring the _tickets_!");
    alice.peer_profile = profile("Alice Marchetti", "Climbing, coffee, Rust.");
    let mut team = chat(2, ChatType::Group, "Cursus devs", "Bob: the invite page looks great now", 12);
    team.membership_role = Some("owner".into());
    team.preview = "Bob: the invite page looks great now, and the new composer is round like a bubble — shipping it tomorrow after review".into();
    team.preview_segments = cursus_core::profile_text("Bob: the **invite page** looks great now, and the _new composer_ is round like a bubble — shipping it tomorrow after review");
    team.group_preferences = Some(serde_json::json!({
        "directMessages": {"enable": "on"},
        "reactions": {"enable": "on"},
        "voice": {"enable": "on"},
        "support": {"enable": "on"}
    }));
    let mut fav = chat(3, ChatType::Direct, "Marco", "https://smp9.simplex.im/i#3iqO73eh1l09QmQfRwMyM2MplnycyJT7/HzD4YSRIOBK_Bvi1gZiXI1BY", 0);
    fav.settings.favorite = true;
    let mut muted = chat(4, ChatType::Group, "Climbing club", "Sunday: Arco, meet 8:00 at the parking lot near the crag", 0);
    muted.preview_kind = cursus_core::PreviewKind::Image;
    muted.settings.enable_ntfs = cursus_core::NtfFilter::None;
    let mut notes = chat(5, ChatType::Local, "Notes", "Voice message · 0:12", 0);
    notes.preview_kind = cursus_core::PreviewKind::Voice;
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
    ].into();
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
    }].into();
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
    let want = |n: &str| {
        only.as_deref().is_none_or(|o| o.split(',').any(|x| n.starts_with(x)))
    };
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
        if want("clock24") {
            // 24-hour clock: Appearance, then the stamps in a chat.
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.desktop_prefs.time_format = cursus_core::TimeFormat::H24;
            super::ui::set_clock(cursus_core::TimeFormat::H24);
            app.nav = NavSection::Settings;
            app.settings_open = true;
            app.settings_section = crate::SettingsSection::Appearance;
            render(&app, DESKTOP, &format!("clock24-appearance-{t}"));
            app.nav = NavSection::Chats;
            select(&mut app, 0);
            render(&app, DESKTOP, &format!("clock24-chat-{t}"));
            super::ui::set_clock(cursus_core::TimeFormat::H12);
        }
        if want("motion") {
            // GTK-style phone transitions frozen mid-way (~half travelled).
            use super::page_motion::{Motion, Under};
            let half = std::time::Duration::from_millis(53);
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            select(&mut app, 0);
            let now = std::time::Instant::now();
            app.motion_now = now + half;
            app.page_motion = Some(Motion::push(Under::List, now));
            render(&app, MOBILE, &format!("motion-chat-push-{t}"));
            // A back swipe held 40% of the way across.
            let mut drag = Motion::drag(Under::List, now);
            drag.phase = super::page_motion::Phase::Drag(0.4);
            app.page_motion = Some(drag);
            render(&app, MOBILE, &format!("motion-chat-swipe-{t}"));
            // Chats → Settings, the lists half way past each other.
            let mut tabs = demo_app(dark, MOBILE.width, MOBILE.height);
            tabs.own_profile.fields = profile("Davide", "Building Cursus");
            tabs.nav = NavSection::Settings;
            tabs.section_motion = Some((NavSection::Chats, 1.0, Motion::push(Under::List, now)));
            tabs.motion_now = now + half;
            render(&tabs, MOBILE, &format!("motion-tabs-{t}"));
            let _ = app.open_contact_profile();
            app.page_motion = Some(Motion::push(Under::Conversation, now));
            render(&app, MOBILE, &format!("motion-profile-push-{t}"));
            if let Some(p) = app.contact_profile.as_mut() {
                p.pane = crate::ContactProfilePane::Preferences;
            }
            app.page_motion = Some(Motion::pop(Under::ContactOverview, now));
            render(&app, MOBILE, &format!("motion-prefs-pop-{t}"));
        }
        if want("contact") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            select(&mut app, 0);
            let _ = app.open_contact_profile();
            render(&app, DESKTOP, &format!("contact-{t}"));
        }
        if want("profiles") {
            // Profile pages: Back only on a phone (actions in the page), Back
            // and the actions on a wide window; About folded to two lines.
            let about = "Climber, coffee person and part-time map collector. Usually somewhere between Arco and the Dolomites, \
                planning the next trip. Ask me about routes, gear or where the good espresso is.";
            for (size, name) in [(MOBILE, "mprofile"), (DESKTOP, "profile")] {
                let mut app = demo_app(dark, size.width, size.height);
                select(&mut app, 0);
                let _ = app.open_contact_profile();
                if let Some(p) = app.contact_profile.as_mut() {
                    p.fields.description = about.into();
                }
                render(&app, size, &format!("{name}-contact-{t}"));
                app.about_open = true;
                render(&app, size, &format!("{name}-contact-open-{t}"));
                if let Some(p) = app.contact_profile.as_mut() {
                    p.pane = crate::ContactProfilePane::Preferences;
                }
                render(&app, size, &format!("{name}-contact-prefs-{t}"));
                let mut app = demo_app(dark, size.width, size.height);
                group_page(&mut app, GroupSettingsPane::Overview);
                render(&app, size, &format!("{name}-group-{t}"));
                // You own it: the editable description folds too.
                if let Some(g) = app.group_settings.as_mut() {
                    g.short_descr = about.into();
                }
                render(&app, size, &format!("{name}-group-about-{t}"));
                group_page(&mut app, GroupSettingsPane::Preferences);
                render(&app, size, &format!("{name}-group-prefs-{t}"));
            }
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
        if want("sizes") {
            // Every column arrangement, and the narrowest tile.
            for (w, h) in [(280.0, 560.0), (360.0, 720.0), (640.0, 720.0), (940.0, 720.0), (1280.0, 760.0)] {
                let size = Size::new(w, h);
                let mut app = demo_app(dark, w, h);
                render(&app, size, &format!("size{w}-list-{t}"));
                select(&mut app, 0);
                render(&app, size, &format!("size{w}-chat-{t}"));
                let _ = app.open_contact_profile();
                render(&app, size, &format!("size{w}-contact-{t}"));
                let mut app = demo_app(dark, w, h);
                group_page(&mut app, GroupSettingsPane::Overview);
                render(&app, size, &format!("size{w}-group-{t}"));
                let mut app = demo_app(dark, w, h);
                app.nav = NavSection::Settings;
                app.own_profile.fields = profile("Davide", "Building Cursus");
                render(&app, size, &format!("size{w}-settings-{t}"));
                let _ = app.update(Message::OpenSettingsSection(crate::SettingsSection::Privacy));
                render(&app, size, &format!("size{w}-settings-privacy-{t}"));
            }
        }
        if want("mobile") {
            let app = demo_app(dark, MOBILE.width, MOBILE.height);
            render(&app, MOBILE, &format!("mobile-list-{t}"));
            // A list longer than the screen: rows must show beneath the
            // floating bar (the list scrolls under it), not stop above it.
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            for i in 0..8 {
                let mut extra = chat(100 + i, ChatType::Direct, &format!("Contact {i}"), "Older conversation", 0);
                extra.last_ts = Some(format!("2026-10-0{}T10:00:00Z", 9 - i));
                app.chats.push(extra);
            }
            render(&app, MOBILE, &format!("mobile-list-long-{t}"));
        }
        if want("msettings") {
            let mut app = demo_app(dark, MOBILE.width, MOBILE.height);
            app.nav = NavSection::Settings;
            app.own_profile.fields = profile("Davide", "Building Cursus");
            render(&app, MOBILE, &format!("msettings-{t}"));
            app.photo_menu = Some(crate::PhotoTarget::Own);
            render(&app, MOBILE, &format!("msettings-photo-{t}"));
            app.photo_menu = None;
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
            // A one-time invitation on a server with an onion address.
            let inv = serde_json::json!([
                {"format": {"type": "simplexLink", "linkType": "invitation",
                            "simplexUri": "https://smp10.simplex.im/i#Xk2pQ7vN1sLm9aR4",
                            "smpHosts": ["smp10.simplex.im", "rb2pbttocvnbrngnwziclp2f4ckjq65kebafws6g4hy22cdaiv5dwjqd.onion"]},
                 "text": "https://smp10.simplex.im/i#Xk2pQ7vN1sLm9aR4"}
            ]);
            let mut m = msg(7, "", true, "16:51");
            m.segments = cursus_core::parse_formatted_text(Some(&inv));
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
            // History taller than the screen: the newest message must sit
            // just above the field, without a gap the size of the fade.
            for i in 0..6 {
                app.messages.push(msg(20 + i, "Filling the history so it scrolls", i % 2 == 0, "14:30"));
            }
            app.messages.push(msg(40, "unnatural spacing", true, "14:31"));
            app.set_composer_text("");
            render(&app, MOBILE, &format!("chat-end-{t}"));
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
                // Voice messages in the chat, one half played.
                let voice = |id: i64, outgoing: bool| {
                    let path = std::env::temp_dir().join(format!("cursus-shot-voice-{id}.m4a"));
                    let _ = std::fs::write(&path, b"shot");
                    let mut m = msg(id, "", outgoing, "16:51");
                    m.file = Some(cursus_core::FileInfo {
                        file_id: id,
                        file_name: "voice.m4a".into(),
                        file_size: 12_000,
                        state: cursus_core::FileTransferState::Complete,
                        kind: cursus_core::FileKind::Voice,
                        duration_secs: Some(7),
                        local_path: Some(path.clone()),
                        encrypted: false,
                        crypto_key: None,
                        crypto_nonce: None,
                        transferred: None,
                        transfer_total: None,
                        error_detail: None,
                        preview: None,
                    });
                    (m, path)
                };
                let peaks: Vec<f32> = (0..96).map(|i| (i as f32 * 0.29).sin().abs() * 0.85 + 0.08).collect();
                for (id, out) in [(41, false), (42, true)] {
                    let (m, path) = voice(id, out);
                    app.waveforms.insert(path.clone(), peaks.clone());
                    app.plaintext_files.insert(path.clone(), path);
                    app.messages.push(m);
                }
                app.voice_draft = None;
                app.voice_playback = Some(crate::VoicePlayback {
                    file_id: 41,
                    path: std::env::temp_dir().join("cursus-shot-voice-41.m4a"),
                    duration_secs: 7,
                    started: std::time::Instant::now() - std::time::Duration::from_secs(3),
                    child: std::process::Command::new("true").spawn().unwrap(),
                });
                render(&app, size, &format!("{name}-messages-{t}"));
                if let Some(mut play) = app.voice_playback.take() {
                    let _ = play.child.wait();
                }
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
            let _ = app.update(Message::OpenSettingsSection(crate::SettingsSection::Privacy));
            render(&app, Size::new(MOBILE.width, 2200.0), &format!("msettings-privacy-{t}"));
        }
        if want("privacy") {
            let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
            app.desktop_prefs.app_lock = true;
            app.nav = NavSection::Settings;
            let _ = app.update(Message::OpenSettingsSection(crate::SettingsSection::Privacy));
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
                alias: String::new(),
                created_at: "2026-11-10T14:05:00Z".into(),
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
            // Pending links: gold counter button beside "+", then its list.
            let mk = |id: i64, alias: &str, created: &str, incognito: bool| cursus_core::Invitation {
                link: cursus_core::ConnLink {
                    display: format!("https://smp12.simplex.im/i#{id}OzM8bkTdaVhHVWJbNe5tUa1Ru3TnHlHs"),
                    full: format!("simplex:/invitation#/?v=2-7&smp={id}"),
                    short: Some(format!("https://smp12.simplex.im/i#{id}OzM8bkTdaVhHVWJbNe5tUa1Ru3TnHlHs")),
                },
                conn_id: id,
                incognito,
                alias: alias.into(),
                created_at: created.into(),
            };
            app.pending_links = vec![
                mk(3, "For Anna (climbing)", "2026-11-10T14:05:00Z", false),
                mk(2, "", "2026-11-08T09:30:00Z", true),
            ];
            render(&app, DESKTOP, &format!("pending-button-{t}"));
            app.pending_links_open = true;
            render(&app, DESKTOP, &format!("pending-list-{t}"));
            app.renaming_link = Some((2, "Marco".into()));
            render(&app, MOBILE, &format!("pending-rename-{t}"));
            app.renaming_link = None;
            app.link_confirm = Some(crate::LinkConfirm::Regenerate);
            render(&app, MOBILE, &format!("pending-confirm-{t}"));
            app.link_confirm = None;
            app.pending_links_open = false;
            app.pending_links.clear();
            app.nav = NavSection::Settings;
            let _ = app.update(Message::OpenSettingsSection(crate::SettingsSection::Address));
            app.address_link = Some(None);
            render(&app, DESKTOP, &format!("settings-address-{t}"));
            app.address_link = Some(Some("https://smp12.simplex.im/a#lrdvu2d8A1GumSmoKb2krQmtKhWXq-tyGpHuM7aMwsw".into()));
            render(&app, DESKTOP, &format!("settings-address-hidden-{t}"));
            app.address_revealed = true;
            render(&app, DESKTOP, &format!("settings-address-link-{t}"));
            app.link_confirm = Some(crate::LinkConfirm::DeleteAddress);
            render(&app, MOBILE, &format!("settings-address-delete-{t}"));
            app.link_confirm = None;
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

/// Frame-cost benchmark: what one frame costs with a long conversation and a
/// long chat list, for the things that happen many times a second: an idle
/// redraw (fling / voice tick), a keystroke in the composer, and the pointer
/// moving over the history. `cargo test --release -p cursus-linux
/// frame_bench -- --ignored --nocapture` (`CURSUS_BENCH_MSGS`, `CURSUS_BENCH_CHATS`).
#[test]
#[ignore]
fn frame_bench() {
    load_fonts();
    let env = |k: &str, d: usize| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let n = env("CURSUS_BENCH_MSGS", 300);
    let chats = env("CURSUS_BENCH_CHATS", 300);
    let avatars = env("CURSUS_BENCH_AVATARS", 1) == 1;
    #[derive(Clone, Copy)]
    enum Step {
        Redraw,
        Type,
        Hover,
        Incoming,
    }
    for (label, size) in [("desktop", DESKTOP), ("mobile", MOBILE)] {
        for (step_label, step) in [("redraw", Step::Redraw), ("type", Step::Type), ("hover", Step::Hover), ("arrive", Step::Incoming)] {
            let mut app = demo_app(true, size.width, size.height);
            let base_chat = app.chats[0].clone();
            for i in 0..chats {
                let mut c = base_chat.clone();
                c.id = cursus_core::ChatId::new(ChatType::Direct, 1000 + i as i64);
                c.title = format!("Contact {i}");
                c.peer_profile = profile(&c.title, "");
                if avatars {
                    // A distinct small picture per chat, like real profiles.
                    let img = image::RgbImage::from_fn(128, 128, |x, y| {
                        image::Rgb([(i * 7 % 256) as u8, (x * 2) as u8, (y * 2) as u8])
                    });
                    let mut png = Vec::new();
                    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
                    use base64::Engine as _;
                    c.peer_profile.image = Some(format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&png)
                    ));
                }
                c.preview = format!("Message number {i} with a little bit of preview text");
                app.chats.push(c);
            }
            let member_avatars: Vec<String> = (0..7u8)
                .map(|k| {
                    let img = image::RgbImage::from_fn(96, 96, |x, y| {
                        image::Rgb([k.wrapping_mul(37), (x as u8) ^ (y as u8), (x * y) as u8])
                    });
                    let mut jpg = Vec::new();
                    img.write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Png).unwrap();
                    use base64::Engine as _;
                    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&jpg))
                })
                .collect();
            // Group chat: every message has a sender, every fourth a file.
            let group = env("CURSUS_BENCH_GROUP", 0) == 1;
            select(&mut app, if group { 1 } else { 0 });
            let base = app.messages.to_vec();
            app.messages.clear();
            for i in 0..n {
                let mut m = base[i % base.len()].clone();
                m.item_id = 1000 + i as i64;
                m.created_at = Some(format!("2026-11-{:02}T{:02}:{:02}:00Z", 1 + (i / 100).min(5), 8 + (i / 10) % 12, i % 60));
                if group && !m.outgoing {
                    m.sender = Some(MessageSender {
                        group_member_id: (i % 7) as i64,
                        contact_id: Some(5000 + (i % 7) as i64),
                        display_name: format!("Member {}", i % 7),
                        // Real members have ~12 KB profile pictures.
                        image: avatars.then(|| member_avatars[i % 7].clone()),
                        role: "member".into(),
                    });
                }
                if group && i % 4 == 0 {
                    m.file = Some(cursus_core::FileInfo {
                        file_id: 1000 + i as i64,
                        file_name: "notes.txt".into(),
                        file_size: 1234,
                        state: cursus_core::FileTransferState::Complete,
                        kind: cursus_core::FileKind::File,
                        duration_secs: None,
                        local_path: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")),
                        encrypted: false,
                        crypto_key: None,
                        crypto_nonce: None,
                        transferred: None,
                        transfer_total: None,
                        error_detail: None,
                        preview: None,
                    });
                }
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
            let (mut t_app, mut t_view, mut t_build, mut t_update, mut t_draw) = (0.0, 0.0, 0.0, 0.0, 0.0);
            let frames = 40;
            for f in 0..frames + 3 {
                let t0 = std::time::Instant::now();
                let (events, cursor) = match step {
                    Step::Redraw => (
                        vec![iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
                        iced::mouse::Cursor::Unavailable,
                    ),
                    Step::Type => {
                        let c = if f % 12 == 11 { ' ' } else { 'a' };
                        let _ = app.update(Message::ComposerEdit(iced::widget::text_editor::Action::Edit(
                            iced::widget::text_editor::Edit::Insert(c),
                        )));
                        (
                            vec![iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
                            iced::mouse::Cursor::Unavailable,
                        )
                    }
                    Step::Incoming => {
                        // A message arrives in the open chat: history and
                        // chat list both change.
                        let mut m = app.messages.to_vec().last().unwrap().clone();
                        m.item_id += 1;
                        app.messages.push(m);
                        app.chats[0].preview = format!("new message {f}");
                        (
                            vec![iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
                            iced::mouse::Cursor::Unavailable,
                        )
                    }
                    Step::Hover => {
                        let p = iced::Point::new(size.width * 0.7, 120.0 + (f % 20) as f32 * 25.0);
                        (
                            vec![iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: p })],
                            iced::mouse::Cursor::Available(p),
                        )
                    }
                };
                let t1 = std::time::Instant::now();
                let el = app.view(iced::window::Id::unique());
                let t2 = std::time::Instant::now();
                let mut ui = UserInterface::build(el, size, cache, &mut renderer);
                let t3 = std::time::Instant::now();
                let mut msgs: Vec<Message> = Vec::new();
                let _ = ui.update(&events, cursor, &mut renderer, &mut iced::advanced::clipboard::Null, &mut msgs);
                let t4 = std::time::Instant::now();
                ui.draw(&mut renderer, &theme, &style, cursor);
                let t5 = std::time::Instant::now();
                cache = ui.into_cache();
                for m in msgs {
                    let _ = app.update(m);
                }
                if f >= 3 {
                    t_app += (t1 - t0).as_secs_f64();
                    t_view += (t2 - t1).as_secs_f64();
                    t_build += (t3 - t2).as_secs_f64();
                    t_update += (t4 - t3).as_secs_f64();
                    t_draw += (t5 - t4).as_secs_f64();
                }
            }
            let ms = |t: f64| t * 1000.0 / frames as f64;
            println!(
                "BENCH {label:7} {step_label:6} {n} msgs {chats} chats: app {:.2}  view {:.2}  build+layout {:.2}  update {:.2}  draw {:.2}  total {:.2} ms",
                ms(t_app), ms(t_view), ms(t_build), ms(t_update), ms(t_draw),
                ms(t_app + t_view + t_build + t_update + t_draw)
            );
        }
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
        ("delivery tick", |a| {
            a.messages.edit()[1].delivery = DeliveryStatus::Sent
        }),
        ("edited text", |a| {
            a.messages.edit()[2].text = "Edited: make it two coffees".into();
            a.messages.edit()[2].edited = true;
        }),
        ("new reaction", |a| {
            a.messages.edit()[4].reactions.push(ReactionCount { emoji: "👍".into(), user_reacted: false, total: 2 })
        }),
        ("new message", |a| {
            a.messages.push(msg(50, "Just arrived 🚆", false, "09:30"))
        }),
        ("selection mode", |a| {
            a.selection_mode = true;
            a.selected_items.insert(3);
        }),
        ("focus", |a| a.focus_item = Some(4)),
        ("reveal deleted", |a| {
            a.messages.edit()[0].deleted = Some(cursus_core::DeletedState::Marked);
            a.revealed_deleted.insert(a.messages.edit()[0].item_id);
        }),
        ("theme switch", |a| {
            a.desktop_prefs.theme = ThemePreference::Light;
        }),
        ("narrower window", |a| a.width = 900.0),
        ("blur media on", |a| a.desktop_prefs.blur_media = true),
        ("chat previews off", |a| {
            a.desktop_prefs.chat_previews = false
        }),
        ("message removed", |a| {
            let mut messages = a.messages.to_vec();
            messages.remove(2);
            a.messages = messages.into();
        }),
        ("messages loaded above", |a| {
            let mut messages = vec![msg(60, "Older one", false, "07:00"), msg(61, "Older two", true, "07:01")];
            messages.extend(a.messages.to_vec());
            a.messages = messages.into();
        }),
        ("chat preview changed", |a| a.chats[2].preview = "A brand new last message".into()),
        ("chats reordered", |a| {
            a.chats.swap(1, 3);
        }),
        ("unread changed", |a| a.chats[1].unread = 40),
        ("epoch timestamp", |a| {
            let mut m = msg(51, "Group event without a real date", false, "00:00");
            m.created_at = Some("1970-01-01T00:00:00Z".into());
            let mut messages = a.messages.to_vec();
            messages.insert(0, m);
            a.messages = messages.into();
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

/// Profile text retains its editable source; the pencil opens the editor sheet,
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
        ].into();
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
    let mut away = app.messages.to_vec();
    app.history_away_from_end = true;
    let load = |app: &mut App, messages: Vec<ChatMessage>| {
        let gen = app.messages_load_gen;
        let _ = app.update(Message::MessagesLoaded { gen, chat: app.selected_id.clone().unwrap(), result: Ok(messages.into()) });
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
    let stale_timer = app.new_messages_marker.unwrap().1;
    render(&app, MOBILE, "fix-new-messages-dark");
    away.push(msg(33, "😄", false, "09:12"));
    load(&mut app, away.clone());
    assert_eq!(app.new_messages_from(), Some(31));

    // Only the latest timer clears it.
    let latest_timer = app.new_messages_marker.unwrap().1;
    let _ = app.update(Message::ClearNewMessagesMarker(stale_timer));
    assert_eq!(app.new_messages_from(), Some(31), "stale timer ignored");
    let _ = app.update(Message::ClearNewMessagesMarker(latest_timer));
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
                g.short_descr = "Build **Cursus** together with friends.\nA clear, readable profile on a small screen.".into();
            }
            render(&app, size, &format!("cards-group-{name}-{t}"));
            {
                let group = app.group_settings.as_mut().unwrap();
                group.pane = GroupSettingsPane::MemberInfo;
                group.selected_member_id = Some(2);
                group.members[1].short_descr = Some("Building **Cursus** with _friends_ 🌍".into());
            }
            render(&app, size, &format!("cards-member-{name}-{t}"));
            app.group_settings.as_mut().unwrap().pane = GroupSettingsPane::Link;
            render(&app, size, &format!("cards-share-{name}-{t}"));
            let mut app = demo_app(dark, size.width, size.height);
            select(&mut app, 0);
            let _ = app.update(Message::OpenContactProfile);
            render(&app, size, &format!("cards-contact-{name}-{t}"));
            let _ = app.update(Message::OpenContactPreferences);
            render(&app, size, &format!("cards-contact-settings-{name}-{t}"));
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
    let history = app.messages.to_vec(); // incoming: 1, 3, 5 (5 is newest)
    let open = |app: &mut App, unread: i64| {
        app.messages.clear();
        app.pin_history_end = true;
        app.open_unread = unread;
        let gen = app.messages_load_gen;
        let _ = app.update(Message::MessagesLoaded { gen, chat: app.selected_id.clone().unwrap(), result: Ok(history.clone().into()) });
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

#[test]
fn back_and_escape_pop_one_page_at_a_time() {
    for size in [MOBILE, DESKTOP] {
        for back in [Message::Back, Message::Escape] {
            let mut app = demo_app(true, size.width, size.height);
            select(&mut app, 0);
            let _ = app.update(Message::OpenContactProfile);
            let _ = app.update(Message::OpenContactPreferences);
            let _ = app.update(back.clone());
            assert!(matches!(app.contact_profile.as_ref().map(|p| p.pane), Some(crate::ContactProfilePane::Overview)));
            assert!(app.selected.is_some());
            let _ = app.update(back.clone());
            assert!(app.contact_profile.is_none());
            assert!(app.selected.is_some());
            let _ = app.update(back.clone());
            assert!(app.selected.is_none());

            for pane in [
                GroupSettingsPane::Preferences, GroupSettingsPane::AddMember, GroupSettingsPane::Link,
                GroupSettingsPane::MemberInfo, GroupSettingsPane::ChannelMembers,
                GroupSettingsPane::ConfirmLeave, GroupSettingsPane::ConfirmDelete,
            ] {
                group_page(&mut app, pane);
                let _ = app.update(back.clone());
                assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Overview);
                let _ = app.update(back.clone());
                assert!(app.group_settings.is_none());
                assert!(app.selected.is_some());
            }
            let _ = app.update(Message::Navigate(NavSection::Settings));
            let _ = app.update(Message::OpenSettingsSection(crate::SettingsSection::Privacy));
            assert!(app.settings_open);
            let _ = app.update(back);
            assert!(!app.settings_open);
            assert_eq!(app.nav, NavSection::Settings);
        }
    }
}

#[test]
fn section_switches_drop_foreign_pages_and_invalidate_history() {
    for destination in NavSection::all() {
        for group in [false, true] {
            let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
            if group {
                group_page(&mut app, GroupSettingsPane::Preferences);
            } else {
                select(&mut app, 0);
                let _ = app.update(Message::OpenContactProfile);
            }
            // Switch away from whichever section owns the profile.
            if destination == NavSection::Chats { app.nav = NavSection::Channels; }
            app.photo_menu = Some(crate::PhotoTarget::Own);
            app.pending_links_open = true;
            let old_gen = app.messages_load_gen;
            let old_chat = app.selected_id.clone().unwrap();
            let _ = app.update(Message::Navigate(destination));
            assert_eq!(app.nav, destination);
            assert!(app.contact_profile.is_none() && app.group_settings.is_none());
            assert!(app.photo_menu.is_none() && !app.pending_links_open);
            assert!(app.support.is_none() && !app.compose_open);
            if matches!(destination, NavSection::Calls | NavSection::Settings | NavSection::Channels) {
                assert!(app.selected.is_none());
                assert_ne!(app.messages_load_gen, old_gen);
                let _ = app.update(Message::MessagesLoaded {
                    gen: old_gen, chat: old_chat, result: Ok(vec![msg(999, "stale history", false, "09:00")].into()),
                });
                assert!(app.messages.is_empty(), "history from the previous section must stay hidden");
            }
        }
    }
}

#[test]
fn escape_closes_sheets_before_the_page_underneath() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    group_page(&mut app, GroupSettingsPane::Preferences);
    app.photo_menu = Some(crate::PhotoTarget::Own);
    let _ = app.update(Message::Escape);
    assert!(app.photo_menu.is_none());
    assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Preferences);
    app.pending_links_open = true;
    let _ = app.update(Message::Escape);
    assert!(!app.pending_links_open);
    assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Preferences);
}

/// A real click is split by view rebuilds: its release must never open the
/// conversation header exposed by closing the profile on its press.
#[test]
fn profile_back_click_does_not_reopen_the_profile() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, Event, Point};
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    for width in [400.0, 800.0, 1200.0] {
        for group in [false, true] {
            let size = Size::new(width, 760.0);
            let mut app = demo_app(true, width, size.height);
            if group {
                group_page(&mut app, GroupSettingsPane::Overview);
            } else {
                select(&mut app, 0);
                let _ = app.open_contact_profile();
            }
            let density = app.density();
            let left = if density.info_width > 0.0 {
                width - density.info_width
            } else if width >= 600.0 {
                density.rail_width + density.list_width + 1.0
            } else {
                0.0
            };
            let point = Point::new(left + 24.0, 24.0);
            let mut cache = Cache::default();
            for event in [
                mouse::Event::CursorMoved { position: point },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                let mut ui = UserInterface::build(
                    app.view(iced::window::Id::unique()),
                    size,
                    cache,
                    &mut renderer,
                );
                let mut messages = Vec::new();
                let _ = ui.update(
                    &[Event::Mouse(event)],
                    mouse::Cursor::Available(point),
                    &mut renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut messages,
                );
                cache = ui.into_cache();
                assert!(
                    !messages.iter().any(|m| matches!(
                        m,
                        Message::OpenContactProfile | Message::OpenGroupSettings
                    )),
                    "the Back click must not reach the profile opener ({width}, group={group})"
                );
                for message in messages {
                    let _ = app.update(message);
                }
                if matches!(event, mouse::Event::ButtonPressed(_)) {
                    assert!(
                        app.contact_profile.is_some() || app.group_settings.is_some(),
                        "Back waits for release"
                    );
                }
            }
            assert!(
                app.contact_profile.is_none() && app.group_settings.is_none(),
                "Back closes the page ({width}, group={group})"
            );
            assert!(app.selected.is_some(), "the conversation stays open");
        }
    }
}

#[test]
fn preference_help_does_not_toggle_the_setting() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, Event, Point};
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    let _ = app.update(Message::Navigate(NavSection::Settings));
    let _ = app.update(Message::OpenSettingsSection(
        crate::SettingsSection::Privacy,
    ));
    assert!(app.desktop_prefs.chat_previews);
    let mut cache = Cache::default();
    // The (i) sits right after the setting's name: 4 px gap, 24 px hit.
    for (target, help) in [
        ("Message previews", true),
        ("Message previews", false),
    ] {
        use iced::advanced::widget::Operation;
        struct Locate {
            text: &'static str,
            at: std::sync::Arc<std::sync::Mutex<Option<iced::Rectangle>>>,
        }
        impl Operation for Locate {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text(
                &mut self,
                _id: Option<&iced::widget::Id>,
                bounds: iced::Rectangle,
                text: &str,
            ) {
                if text == self.text {
                    *self.at.lock().unwrap() = Some(bounds);
                }
            }
        }
        let at = std::sync::Arc::new(std::sync::Mutex::new(None::<iced::Rectangle>));
        let mut ui = UserInterface::build(
            app.view(iced::window::Id::unique()),
            DESKTOP,
            cache,
            &mut renderer,
        );
        ui.operate(
            &renderer,
            &mut Locate {
                text: target,
                at: at.clone(),
            },
        );
        cache = ui.into_cache();
        let bounds = at.lock().unwrap().expect("the setting text is present");
        let point = if help {
            Point::new(bounds.x + bounds.width + 4.0 + 12.0, bounds.center_y())
        } else {
            bounds.center()
        };
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let mut ui = UserInterface::build(
                app.view(iced::window::Id::unique()),
                DESKTOP,
                cache,
                &mut renderer,
            );
            let mut messages = Vec::new();
            let _ = ui.update(
                &[Event::Mouse(event)],
                mouse::Cursor::Available(point),
                &mut renderer,
                &mut iced::advanced::clipboard::Null,
                &mut messages,
            );
            cache = ui.into_cache();
            for message in messages {
                let _ = app.update(message);
            }
        }
        if help {
            assert!(app.setting_info.is_some(), "help opens its explanation");
            assert!(
                app.desktop_prefs.chat_previews,
                "help must not toggle its parent row"
            );
            let _ = app.update(Message::Escape);
        } else {
            assert!(
                !app.desktop_prefs.chat_previews,
                "clicking the row toggles exactly once"
            );
        }
    }
}

#[test]
#[ignore]
fn shots_preferences_redesign() {
    for dark in [true, false] {
        let tone = if dark { "dark" } else { "light" };
        let size = Size::new(400.0, 900.0);
        let mut app = demo_app(dark, size.width, size.height);
        let _ = app.update(Message::Navigate(NavSection::Settings));
        let _ = app.update(Message::OpenSettingsSection(
            crate::SettingsSection::Privacy,
        ));
        render(&app, size, &format!("preferences-privacy-{tone}"));
        for channel in [false, true] {
            let mut app = demo_app(dark, size.width, size.height);
            app.chats[1].is_channel = channel;
            app.nav = if channel {
                NavSection::Channels
            } else {
                NavSection::Chats
            };
            group_page(&mut app, GroupSettingsPane::Preferences);
            let name = if channel { "channel" } else { "group" };
            render(&app, size, &format!("preferences-{name}-{tone}"));
            app.group_settings.as_mut().unwrap().my_role = "member".into();
            render(&app, size, &format!("preferences-{name}-readonly-{tone}"));
        }
    }
}

#[test]
fn member_popups_preserve_scroll_and_block_the_background() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::advanced::widget::{operation::Scrollable, Operation};
    use iced::{mouse, Event, Point, Rectangle, Vector};
    struct Probe {
        offset: std::sync::Arc<std::sync::Mutex<f32>>,
        set: Option<f32>,
    }
    impl Operation for Probe {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: Rectangle,
            _content: Rectangle,
            translation: Vector,
            state: &mut dyn Scrollable,
        ) {
            if id == Some(&iced::widget::Id::new("page-scroll")) && bounds.x > 200.0 {
                *self.offset.lock().unwrap() = translation.y;
                if let Some(y) = self.set {
                    state.scroll_to(iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    });
                }
            }
        }
    }
    load_fonts();
    let size = Size::new(800.0, 440.0);
    let mut app = demo_app(true, size.width, size.height);
    group_page(&mut app, GroupSettingsPane::Overview);
    // Enough members to exercise scrolling on any font installation.
    let g = app.group_settings.as_mut().unwrap();
    for id in 20..50 {
        g.members.push(member(
            id,
            "Another member",
            "member",
            "connected",
            None,
            false,
        ));
    }
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let mut cache = Cache::default();
    let offset = std::sync::Arc::new(std::sync::Mutex::new(0.0));
    let mut frame = |app: &App, cache: Cache, set: Option<f32>, wheel: bool| {
        let mut ui = UserInterface::build(
            app.view(iced::window::Id::unique()),
            size,
            cache,
            &mut renderer,
        );
        let event = if wheel {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -8.0 },
            })
        } else {
            Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            ))
        };
        let _ = ui.update(
            &[event],
            mouse::Cursor::Available(Point::new(300.0, 150.0)),
            &mut renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Vec::new(),
        );
        ui.operate(
            &renderer,
            &mut Probe {
                offset: offset.clone(),
                set,
            },
        );
        ui.into_cache()
    };
    cache = frame(&app, cache, Some(320.0), false);
    cache = frame(&app, cache, None, false);
    let before = *offset.lock().unwrap();
    assert!(
        before.abs() > 10.0,
        "the profile must be scrolled before opening a menu"
    );
    for open in [
        Message::OpenMemberRoleMenu(2),
        Message::OpenMemberActionMenu(2),
    ] {
        let _ = app.update(open);
        cache = frame(&app, cache, None, false);
        assert_eq!(
            *offset.lock().unwrap(),
            before,
            "opening the popup keeps the page in place"
        );
        cache = frame(&app, cache, None, true);
        assert_eq!(
            *offset.lock().unwrap(),
            before,
            "scrolling over the backdrop must not move the page"
        );
        let _ = app.update(Message::CloseMemberDropdown);
        cache = frame(&app, cache, None, false);
        assert_eq!(
            *offset.lock().unwrap(),
            before,
            "closing the popup also preserves the page"
        );
    }
}

#[test]
fn narrow_preferences_keep_supporting_text_wider_than_the_title() {
    use iced::advanced::widget::Operation;
    use iced::Rectangle;
    struct Measure(std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Rectangle>>>);
    impl Operation for Measure {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text(&mut self, _id: Option<&iced::widget::Id>, bounds: Rectangle, text: &str) {
            self.0.lock().unwrap().insert(text.into(), bounds);
        }
    }
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    for width in [280.0, 340.0, 400.0, 800.0, 1200.0] {
        let mut app = demo_app(true, width, 900.0);
        group_page(&mut app, GroupSettingsPane::Preferences);
        let mut ui = UserInterface::build(
            app.view(iced::window::Id::unique()),
            Size::new(width, 900.0),
            Cache::default(),
            &mut renderer,
        );
        let found = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        ui.operate(&renderer, &mut Measure(found.clone()));
        let theme = app.iced_theme();
        ui.draw(
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style {
                text_color: crate::theme::t().text_primary,
            },
            iced::mouse::Cursor::Unavailable,
        );
        let bounds = found.lock().unwrap();
        let title = bounds.get("Disappearing messages").unwrap();
        let support = bounds
            .get("Automatically remove messages after a set time.")
            .unwrap();
        assert!(
            support.width >= title.width + 40.0,
            "{width}: supporting text must use the width below the switch"
        );
        assert!(title.x + title.width <= width && support.x + support.width <= width);
    }
}

#[test]
fn holds_open_menus_in_the_complete_linux_shell() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::advanced::widget::Operation;
    use iced::{mouse, touch, Event, Point};
    struct Locate {
        needle: String,
        point: Option<Point>,
    }
    impl Operation for Locate {
        fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) {
            op(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, text: &str) {
            if text == self.needle {
                self.point = Some(bounds.center());
            }
        }
    }
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    for native_touch in [true, false] {
        for bubble in [false, true] {
            let size = Size::new(800.0, 760.0);
            let mut app = demo_app(true, size.width, size.height);
            if bubble {
                select(&mut app, 0);
                app.messages = vec![msg(31, "Hold this message", false, "09:06")].into();
            }
            let mut locate = Locate {
                needle: if bubble {
                    super::ui::format_bubble_time(app.messages[0].created_at.as_deref()).unwrap()
                } else {
                    "Alice Marchetti".into()
                },
                point: None,
            };
            let mut ui = UserInterface::build(
                app.view(iced::window::Id::unique()),
                size,
                Cache::default(),
                &mut renderer,
            );
            ui.operate(&renderer, &mut locate);
            let mut cache = ui.into_cache();
            let point = locate.point.unwrap_or_else(|| {
                panic!("hold target visible: bubble={bubble}, touch={native_touch}")
            });
            let finger = touch::Finger(81);
            let cursor = if native_touch {
                mouse::Cursor::Unavailable
            } else {
                mouse::Cursor::Available(point)
            };
            let press = if native_touch {
                Event::Touch(touch::Event::FingerPressed {
                    id: finger,
                    position: point,
                })
            } else {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            };
            let release = if native_touch {
                Event::Touch(touch::Event::FingerLifted {
                    id: finger,
                    position: point,
                })
            } else {
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            };
            for event in [
                press,
                Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(crate::LONG_PRESS_MS + 1),
                )),
                release,
            ] {
                let mut ui = UserInterface::build(
                    app.view(iced::window::Id::unique()),
                    size,
                    cache,
                    &mut renderer,
                );
                let mut messages = vec![];
                // Advance the fake clock after layout, so cold-font/layout time
                // cannot consume the intended hold interval before the press.
                let event = if matches!(event, Event::Window(iced::window::Event::RedrawRequested(_))) {
                    Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now() + std::time::Duration::from_millis(crate::LONG_PRESS_MS + 1)))
                } else { event };
                let _ = ui.update(
                    &[event],
                    cursor,
                    &mut renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut messages,
                );
                cache = ui.into_cache();
                for message in messages {
                    let _ = app.update(message);
                }
            }
            assert!(
                if bubble {
                    app.message_menu.is_some()
                } else {
                    app.chat_menu.is_some()
                },
                "menu for bubble={bubble}, native touch={native_touch}"
            );
        }
    }
}

#[test]
fn image_viewer_dismisses_before_the_chat_and_paste_stays_in_its_chat() {
    let mut app = demo_app(true, 800.0, 760.0);
    select(&mut app, 0);
    app.image_viewer = Some((PathBuf::from("test.png"), 2.0));
    let _ = app.update(Message::Escape);
    assert!(app.image_viewer.is_none());
    assert!(app.selected.is_some());
    let old = app.selected_id.clone();
    select(&mut app, 1);
    let _ = app.update(Message::ComposerImagePasted {
        chat: old.clone(),
        image: Ok(Some(vec![1])),
    });
    assert!(app.pasted_image.is_none());
    let _ = app.update(Message::ComposerTextPasted {
        chat: old,
        text: Some("wrong chat".into()),
    });
    assert!(app.composer().trim().is_empty());
    let _ = app.update(Message::ComposerImagePasted {
        chat: app.selected_id.clone(),
        image: Ok(Some(vec![1, 2])),
    });
    assert_eq!(app.pasted_image, Some(vec![1, 2]));
    let _ = app.update(Message::DiscardComposerImage);
    assert!(app.pasted_image.is_none());
}

#[test]
#[ignore = "writes design-review screenshots"]
fn shots_image_viewer_and_paste() {
    load_fonts();
    let path = std::env::temp_dir().join(format!("cursus-viewer-shot-{}.png", std::process::id()));
    image::RgbaImage::from_fn(640, 480, |x, y| image::Rgba([(x * 255 / 640) as u8, (y * 255 / 480) as u8, 160, 255])).save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for dark in [false, true] {
        let mut app = demo_app(dark, 800.0, 760.0);
        select(&mut app, 0);
        app.image_viewer = Some((path.clone(), 1.0));
        app.gallery_cache.begin(vec![(path.clone(), "example.png".into())]);
        let jobs = app.gallery_cache.plan(&path);
        for (file, generation) in jobs { app.gallery_cache.complete(file.clone(), generation, crate::gallery::decode(&file)); }
        render(&app, Size::new(800.0, 760.0), if dark { "viewer-dark" } else { "viewer-light" });
        app.image_viewer = None;
        app.pasted_image = Some(bytes.clone());
        render(&app, Size::new(800.0, 760.0), if dark { "pasted-image-dark" } else { "pasted-image-light" });
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn profile_action_labels_hover_and_click_as_part_of_the_button() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Start in mouse mode before constructing a view whose styles capture it.
    // Another input test may have ended in touch mode.
    crate::POINTER_GATE.touch_ended();
    let _ = crate::POINTER_GATE.filter_move(-100.0, -100.0);
    use iced::advanced::widget::Operation;
    use iced::{mouse, Event, Point, Rectangle};
    struct Locate(Option<Rectangle>);
    impl Operation for Locate {
        fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) {
            op(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: Rectangle, text: &str) {
            if text == "Settings" {
                self.0 = Some(bounds);
            }
        }
    }
    load_fonts();
    let size = Size::new(400.0, 900.0);
    let mut app = demo_app(true, size.width, size.height);
    select(&mut app, 0);
    let _ = app.open_contact_profile();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let mut ui = UserInterface::build(
        app.view(iced::window::Id::unique()),
        size,
        Cache::default(),
        &mut renderer,
    );
    let mut locate = Locate(None);
    ui.operate(&renderer, &mut locate);
    let bounds = locate.0.expect("profile Settings action");
    // The bottom padding is deliberately outside both the SVG and the label.
    let point = Point::new(bounds.center_x(), bounds.y + bounds.height + 6.0);
    let mut messages = vec![];
    let _ = ui.update(
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        ],
        mouse::Cursor::Available(point),
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    let theme = app.iced_theme();
    ui.draw(
        &mut renderer,
        &theme,
        &iced::advanced::renderer::Style {
            text_color: iced::Color::WHITE,
        },
        mouse::Cursor::Available(point),
    );
    let pixels = renderer.screenshot(Size::new(400, 900), 1.0, iced::Color::BLACK);
    let mut glowing = 0;
    for y in (bounds.y as usize).saturating_sub(32)..bounds.y as usize {
        for x in (bounds.center_x() as usize).saturating_sub(15)
            ..(bounds.center_x() as usize + 15).min(400)
        {
            let i = (y * 400 + x) * 4;
            if pixels[i + 2] > 220 && pixels[i + 1] > 120 && pixels[i + 1] < 185 && pixels[i] < 145
            {
                glowing += 1;
            }
        }
    }
    assert!(
        glowing > 3,
        "hovering the button padding must illuminate its icon"
    );
    crate::POINTER_GATE.begin_press(point.x, point.y);
    let touch_point = point + iced::Vector::new(0.0, 5.0);
    let _ = ui.update(
        &[
            Event::Touch(iced::touch::Event::FingerPressed {
                id: iced::touch::Finger(91),
                position: point,
            }),
            Event::Touch(iced::touch::Event::FingerMoved {
                id: iced::touch::Finger(91),
                position: touch_point,
            }),
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        ],
        mouse::Cursor::Available(touch_point),
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    ui.draw(
        &mut renderer,
        &theme,
        &iced::advanced::renderer::Style {
            text_color: iced::Color::WHITE,
        },
        mouse::Cursor::Available(touch_point),
    );
    let pixels = renderer.screenshot(Size::new(400, 900), 1.0, iced::Color::BLACK);
    let mut glowing = 0;
    for y in (bounds.y as usize).saturating_sub(32)..bounds.y as usize {
        for x in (bounds.center_x() as usize).saturating_sub(15)
            ..(bounds.center_x() as usize + 15).min(400)
        {
            let i = (y * 400 + x) * 4;
            if pixels[i + 2] > 220 && pixels[i + 1] > 120 && pixels[i + 1] < 185 && pixels[i] < 145
            {
                glowing += 1;
            }
        }
    }
    assert_eq!(
        glowing, 0,
        "touching or swiping over a profile action must never illuminate its icon"
    );
    let _ = ui.update(
        &[Event::Touch(iced::touch::Event::FingerLost {
            id: iced::touch::Finger(91),
            position: touch_point,
        })],
        mouse::Cursor::Available(touch_point),
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    crate::POINTER_GATE.touch_ended();
    let _ = crate::POINTER_GATE.filter_move(point.x + 20.0, point.y);

    let _ = ui.update(
        &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(point),
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    drop(ui);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(
        app.contact_profile.as_ref().unwrap().pane,
        crate::ContactProfilePane::Preferences
    );
}

#[test]
fn gallery_navigation_stops_at_both_ends() {
    let mut app = demo_app(true, 800.0, 760.0);
    select(&mut app, 0);
    let folder = std::env::temp_dir().join(format!("cursus-gallery-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let paths: Vec<_> = ["cachehash_first.png", "cachehash_second.png"]
        .iter()
        .map(|name| folder.join(name))
        .collect();
    app.messages = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            image::RgbaImage::new(2, 2).save(path).unwrap();
            let mut message = msg(index as i64 + 1, "", false, "09:06");
            message.file = Some(cursus_core::FileInfo {
                file_id: index as i64 + 1,
                file_name: ["first.png", "second.png"][index].into(),
                file_size: 100,
                state: cursus_core::FileTransferState::Complete,
                kind: cursus_core::FileKind::Image,
                duration_secs: None,
                local_path: Some(path.clone()),
                encrypted: false,
                crypto_key: None,
                crypto_nonce: None,
                transferred: None,
                transfer_total: None,
                error_detail: None,
                preview: None,
            });
            message
        })
        .collect::<Vec<_>>()
        .into();
    assert!(paths[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("cachehash_"));
    let items = app.chat_gallery_items();
    assert_eq!(items[0].1, "first.png");
    assert_eq!(items[1].1, "second.png");
    app.gallery_cache.begin(items);
    app.messages.clear(); // Incoming reloads must not replace the visible name or gallery order.
    assert_eq!(app.gallery_cache.items()[0].1, "first.png");
    app.image_viewer = Some((paths[0].clone(), 2.0));
    let _ = app.update(Message::ImageStep(-1));
    assert_eq!(
        app.image_viewer.as_ref().unwrap(),
        &(paths[0].clone(), 2.0),
        "edge navigation must preserve the current photo and zoom"
    );
    let _ = app.update(Message::ImageStep(1));
    assert_eq!(app.image_viewer.as_ref().unwrap().0, paths[1]);
    let _ = app.update(Message::ImageStep(1));
    assert_eq!(app.image_viewer.as_ref().unwrap().0, paths[1]);
    let _ = app.update(Message::ImageStep(-1));
    assert_eq!(app.image_viewer.as_ref().unwrap().0, paths[0]);
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(folder).unwrap();
}

#[test]
#[ignore = "writes audit design screenshots"]
fn shots_audit_design() {
    for dark in [false, true] {
        let tone = if dark { "dark" } else { "light" };
        let size = Size::new(400.0, 900.0);
        let mut app = demo_app(dark, size.width, size.height);
        select(&mut app, 0);
        let _ = app.open_contact_profile();
        render(&app, size, &format!("audit-profile-{tone}"));
        let _ = app.update(Message::OpenContactPreferences);
        render(&app, size, &format!("audit-chat-settings-{tone}"));
        app.pref_menu = Some(crate::PrefMenu::ContactRcpts);
        render(&app, size, &format!("audit-choice-sheet-{tone}"));
        app.pref_menu = Some(crate::PrefMenu::ContactReactions);
        render(&app, size, &format!("audit-feature-sheet-{tone}"));
        app.pref_menu = None;
        {
            use cursus_core::{MemberReceipt, ReceiptState};
            let r = |id, name: &str, state| MemberReceipt { group_member_id: id, name: name.into(), image: None, state };
            app.message_menu = Some(crate::MessageMenu {
                item_id: 4,
                pane: crate::MsgMenuPane::Info {
                    item_id: 4,
                    time: Some("2026-11-06T09:05:00Z".into()),
                    status: Some("Sent".into()),
                    text: "You're the best.".into(),
                    file_detail: None,
                    receipts: vec![
                        r(2, "Bob Rossi", ReceiptState::Received),
                        r(3, "Carla", ReceiptState::Received),
                        r(4, "Dario Bianchi", ReceiptState::Pending),
                    ],
                },
            });
            render(&app, size, &format!("audit-receipts-{tone}"));
            app.message_menu = None;
        }
        group_page(&mut app, GroupSettingsPane::AddMember);
        app.contact_profile = None;
        app.group_settings
            .as_mut()
            .unwrap()
            .apply_links(cursus_core::ConnLink {
                full: "https://simplex.chat/invitation-for-a-long-group-name".into(),
                short: Some("https://simplex.chat/g/example".into()),
                display: "https://simplex.chat/g/example".into(),
            });
        render(&app, size, &format!("audit-invite-{tone}"));
        let _ = app.update(Message::Navigate(NavSection::Calls));
        app.call.set_history_for_test(vec![
            cursus_core::CallRecord {
                item_id: 1,
                contact_id: 1,
                peer_name: "Alice".into(),
                outgoing: false,
                status: "missed".into(),
                duration_secs: 0,
                ts: Some("2026-11-17T12:00:00Z".into()),
            },
            cursus_core::CallRecord {
                item_id: 2,
                contact_id: 1,
                peer_name: "Alice".into(),
                outgoing: false,
                status: "ended".into(),
                duration_secs: 82,
                ts: Some("2026-11-17T11:00:00Z".into()),
            },
            cursus_core::CallRecord {
                item_id: 3,
                contact_id: 1,
                peer_name: "Alice".into(),
                outgoing: true,
                status: "ended".into(),
                duration_secs: 34,
                ts: Some("2026-11-17T10:00:00Z".into()),
            },
        ]);
        render(&app, size, &format!("audit-calls-{tone}"));
    }
}

#[test]
fn gallery_does_not_construct_or_traverse_unrelated_chat_rows() {
    use iced::advanced::widget::Operation;
    struct Count { hidden: usize, loading: bool }
    impl Operation for Count {
        fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) { op(self); }
        fn text(&mut self, _: Option<&iced::widget::Id>, _: iced::Rectangle, text: &str) {
            if text.starts_with("hidden-chat-") { self.hidden += 1; }
            self.loading |= text == "Loading image…";
        }
    }
    load_fonts();
    let mut app = demo_app(true, 800.0, 760.0);
    let template = app.chats[0].clone();
    app.chats = (0..10_000).map(|index| { let mut chat = template.clone(); chat.title = format!("hidden-chat-{index}"); chat }).collect();
    app.image_viewer = Some((PathBuf::from("current-photo.png"), 1.0));
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(iced::Font::DEFAULT, iced::Pixels(16.0), Some("tiny-skia"))).unwrap();
    let started = std::time::Instant::now();
    let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), Size::new(800.0, 760.0), Cache::default(), &mut renderer);
    let mut count = Count { hidden: 0, loading: false }; ui.operate(&renderer, &mut count);
    assert_eq!(count.hidden, 0);
    assert!(count.loading);
    eprintln!("gallery build/layout with 10,000 unrelated chats: {:?}", started.elapsed());
}

#[test]
fn swiping_profile_actions_does_not_block_other_rows() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::advanced::widget::Operation;
    use iced::{mouse, touch, Event, Point};
    struct Locate<'a>(&'a str, Option<Point>);
    impl Operation for Locate<'_> {
        fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) { op(self); }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, text: &str) {
            if text == self.0 { self.1 = Some(bounds.center()); }
        }
    }
    load_fonts();
    let size = Size::new(400.0, 1100.0);
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(iced::Font::with_name("Noto Sans"), iced::Pixels(16.0), Some("tiny-skia"))).unwrap();
    for action in ["Mute", "Settings", "Star"] {
        for target in ["Invite a contact", "Welcome message", "Chats with members"] {
            let mut app = demo_app(true, size.width, size.height);
            group_page(&mut app, GroupSettingsPane::Overview);
            let cache = Cache::default();
            let mut messages = vec![];
            let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, &mut renderer);
            let mut locate = Locate(action, None); ui.operate(&renderer, &mut locate);
            let origin = locate.1.expect(action);
            let end = origin + iced::Vector::new(35.0, -20.0);
            for (event, point) in [
                (Event::Touch(touch::Event::FingerPressed { id: touch::Finger(1), position: origin }), origin),
                (Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now() + crate::tap::HOLD * 2)), origin),
                (Event::Touch(touch::Event::FingerMoved { id: touch::Finger(1), position: origin + iced::Vector::new(0.0, -5.0) }), origin + iced::Vector::new(0.0, -5.0)),
                (Event::Touch(touch::Event::FingerMoved { id: touch::Finger(1), position: end }), end),
                (Event::Touch(touch::Event::FingerLifted { id: touch::Finger(1), position: end }), end),
            ] {
                let _ = ui.update(&[event], mouse::Cursor::Available(point), &mut renderer, &mut iced::advanced::clipboard::Null, &mut messages);
            }
            assert!(messages.is_empty(), "a swipe on {action} must not activate it");
            // No app message is emitted by a swipe: retain the live widget tree.
            let mut locate = Locate(target, None); ui.operate(&renderer, &mut locate);
            let point = locate.1.expect(target);
            for event in [touch::Event::FingerPressed { id: touch::Finger(2), position: point }, touch::Event::FingerLifted { id: touch::Finger(2), position: point }] {
                let _ = ui.update(&[Event::Touch(event)], mouse::Cursor::Available(point), &mut renderer, &mut iced::advanced::clipboard::Null, &mut messages);
            }
            assert!(messages.iter().any(|m| match target {
                "Invite a contact" => matches!(m, Message::OpenAddGroupMember),
                "Welcome message" => matches!(m, Message::OpenFieldEditor(crate::EditField::GroupWelcome)),
                _ => matches!(m, Message::OpenSupportChatList),
            }), "{target} must respond immediately after swiping {action}: {messages:?}");
        }
    }
}

#[test]
fn group_nested_back_keeps_its_position_and_returns_to_profile() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, touch, Event, Point};
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    // A tap on the one Back spot of the page (the pages have no titles now:
    // Back floats alone, at the same place on the profile and inside it).
    let tap = |app: &App, renderer: &mut iced::Renderer, cache: Cache, point: Point| {
        let size = Size::new(400.0, 900.0);
        let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, renderer);
        let mut messages = vec![];
        for event in [
            touch::Event::FingerPressed { id: touch::Finger(3), position: point },
            touch::Event::FingerLifted { id: touch::Finger(3), position: point },
        ] {
            let _ = ui.update(
                &[Event::Touch(event)],
                mouse::Cursor::Available(point),
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut messages,
            );
        }
        (messages, ui.into_cache())
    };
    // Centre of the Back square: side inset 6, top inset 8, 56 px square.
    let back = Point::new(6.0 + 28.0, 8.0 + 28.0);
    for channel in [false, true] {
        for pane in [GroupSettingsPane::Preferences, GroupSettingsPane::AddMember] {
            if channel && pane == GroupSettingsPane::AddMember {
                continue;
            }
            let size = Size::new(400.0, 900.0);
            let mut app = demo_app(true, size.width, size.height);
            group_page(&mut app, GroupSettingsPane::Overview);
            app.group_settings.as_mut().unwrap().is_channel = channel;
            let (on_profile, cache) = tap(&app, &mut renderer, Cache::default(), back);
            assert!(
                on_profile.iter().any(|m| matches!(m, Message::CloseGroupSettings)),
                "the profile's Back is at the Back spot"
            );
            let _ = app.update(if pane == GroupSettingsPane::Preferences {
                Message::OpenGroupPreferences
            } else {
                Message::OpenAddGroupMember
            });
            let (messages, _) = tap(&app, &mut renderer, cache, back);
            assert!(messages.iter().any(|m| matches!(m, Message::Back)), "the nested page's Back is at the same spot");
            for message in messages {
                let _ = app.update(message);
            }
            assert_eq!(
                app.group_settings.as_ref().unwrap().pane,
                GroupSettingsPane::Overview
            );
            assert!(
                app.selected.is_some(),
                "Back must retain the selected conversation"
            );
        }
    }
}
#[test]
fn calls_reuse_loaded_history_and_invalidate_only_for_relevant_events() {
    use crate::call_flow::CallMsg;
    let mut app = demo_app(true, 800.0, 900.0);
    let call = cursus_core::CallRecord {
        item_id: 9,
        contact_id: 1,
        peer_name: "Alice".into(),
        outgoing: false,
        status: "missed".into(),
        duration_secs: 0,
        ts: None,
    };
    let _ = app.update(Message::Call(CallMsg::HistoryProgress(vec![call.clone()])));
    assert_eq!(
        app.call_history(),
        std::slice::from_ref(&call),
        "cold-load results appear before the scan finishes"
    );
    let _ = app.update(Message::Call(CallMsg::HistoryLoaded(Ok(
        vec![call.clone()],
    ))));
    assert!(app.call.history_is_current());
    assert_eq!(
        app.open_call_history().units(),
        0,
        "cached Calls must not start a new scan"
    );
    let _ = app.update(Message::Navigate(NavSection::Settings));
    let _ = app.call_history_event(&cursus_core::Event::NewChatItems(
        serde_json::json!({"chatItems": [{"chatItem": {"content": {"type": "rcvMsgContent"}}}]}),
    ));
    assert!(
        app.call.history_is_current(),
        "ordinary messages don't invalidate call history"
    );
    let _ = app.call_history_event(&cursus_core::Event::Other {
        type_name: "callEnded".into(),
        body: serde_json::json!({}),
    });
    assert!(
        !app.call.history_is_current(),
        "a call change outside Calls must invalidate its cache"
    );
    let _ = app.update(Message::Call(CallMsg::HistoryProgress(vec![])));
    assert_eq!(
        app.call_history(),
        &[call],
        "refreshing must retain existing rows until the replacement is ready"
    );
}

#[test]
fn local_chat_mutations_invalidate_call_history() {
    use crate::call_flow::CallMsg;
    let mut app = demo_app(true, 800.0, 900.0);
    let _ = app.update(Message::Call(CallMsg::HistoryLoaded(Ok(vec![]))));
    assert!(app.call.history_is_current());
    let _ = app.update(Message::ChatMenuDone(Ok("Chat cleared.".into())));
    assert!(
        !app.call.history_is_current(),
        "local chat mutations must invalidate cached history too"
    );
}

#[test]
fn profile_editors_keep_rich_text_and_literal_shortcodes() {
    use crate::EditField;
    for field in [
        EditField::OwnName,
        EditField::OwnBio,
        EditField::OwnDescription,
        EditField::GroupName,
        EditField::GroupAlias,
        EditField::GroupShortDescr,
        EditField::GroupWelcome,
        EditField::ContactAlias,
    ] {
        let mut app = demo_app(true, 400.0, 900.0);
        group_page(&mut app, GroupSettingsPane::Overview);
        if field == EditField::ContactAlias {
            select(&mut app, 0);
            let _ = app.open_contact_profile();
        }
        let _ = app.update(Message::OpenFieldEditor(field));
        let source = if field.multiline() {
            "Team *Cursus*\n_description_ :rocket:"
        } else {
            "Team *Cursus* :rocket:"
        };
        if field.multiline() {
            app.field_editor.as_mut().unwrap().content = iced::widget::text_editor::Content::new();
            let _ = app.update(Message::FieldEditorEdit(
                iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
                    std::sync::Arc::new(source.into()),
                )),
            ));
        } else {
            let _ = app.update(Message::FieldEditorInput(source.into()));
        }
        let _ = app.update(Message::FieldEditorSave);
        assert_eq!(
            app.field_value(field).as_deref(),
            Some(source),
            "saving {field:?} must preserve markup, line breaks and literal shortcodes"
        );
    }
    assert!(
        EditField::GroupShortDescr.multiline(),
        "short group/channel descriptions use the welcome-message editor"
    );
}

#[test]
#[ignore = "writes reaction and rich-profile design screenshots"]
fn shots_reactions_and_rich_profiles() {
    for dark in [false, true] {
        let tone = if dark { "dark" } else { "light" };
        let size = Size::new(280.0, 850.0);
        let mut app = demo_app(dark, size.width, size.height);
        select(&mut app, 0);
        app.message_menu = Some(crate::MessageMenu {
            item_id: 1,
            pane: crate::MsgMenuPane::Actions,
        });
        render(&app, size, &format!("reactions-narrow-{tone}"));
        app.message_menu = None;
        group_page(&mut app, GroupSettingsPane::Overview);
        let g = app.group_settings.as_mut().unwrap();
        g.name = "Team *Cursus* :rocket:".into();
        g.alias = "_Local name_".into();
        g.short_descr = "*Build* something _nice_".into();
        g.welcome_message = "Welcome to *Cursus*! :wave:".into();
        render(&app, size, &format!("profile-markdown-{tone}"));
        let _ = app.update(Message::OpenFieldEditor(crate::EditField::GroupWelcome));
        render(&app, size, &format!("profile-editor-markdown-{tone}"));
        let _ = app.update(Message::FieldEditorCancel);
        app.group_settings.as_mut().unwrap().short_descr = "Build *nice* things\nTogether".into();
        let _ = app.update(Message::OpenFieldEditor(crate::EditField::GroupShortDescr));
        render(&app, size, &format!("short-description-editor-{tone}"));
    }
}

#[test]
fn creation_and_call_sheets_dismiss_without_changing_the_page_underneath() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    group_page(&mut app, GroupSettingsPane::Overview);
    let selected = app.selected;
    let _ = app.update(Message::OpenCompose);
    assert!(app.compose_open);
    assert_eq!(app.selected, selected);
    let _ = app.update(Message::OpenCreateGroup);
    assert!(app.create_chat.is_some());
    let _ = app.update(Message::Back);
    assert!(app.create_chat.is_none());
    assert!(app.compose_open);
    let _ = app.update(Message::Escape);
    assert!(!app.compose_open);
    assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Overview);
    assert_eq!(app.selected, selected);

    let _ = app.update(Message::OpenGroupLink);
    assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Link);
    let _ = app.update(Message::Back);
    assert_eq!(app.group_settings.as_ref().unwrap().pane, GroupSettingsPane::Overview);

    let _ = app.update(Message::Navigate(NavSection::Calls));
    let _ = app.update(Message::CallContacts(true));
    assert!(app.call_contacts_open);
    let _ = app.update(Message::Escape);
    assert!(!app.call_contacts_open);
    assert_eq!(app.nav, NavSection::Calls);
    let _ = app.update(Message::CallContacts(true));
    let _ = app.update(Message::Navigate(NavSection::Chats));
    assert!(!app.call_contacts_open);
}

#[test]
#[ignore = "writes navigation overlay design screenshots"]
fn shots_creation_and_call_overlays() {
    for dark in [true, false] {
        let tone = if dark { "dark" } else { "light" };
        let mut app = demo_app(dark, DESKTOP.width, DESKTOP.height);
        select(&mut app, 0);
        let _ = app.update(Message::OpenCompose);
        render(&app, DESKTOP, &format!("new-connect-{tone}"));
        let _ = app.update(Message::OpenCreateGroup);
        render(&app, DESKTOP, &format!("new-group-{tone}"));
        let _ = app.update(Message::CloseCompose);
        let _ = app.update(Message::Navigate(NavSection::Calls));
        let _ = app.update(Message::CallContacts(true));
        render(&app, DESKTOP, &format!("call-contacts-{tone}"));
        let _ = app.update(Message::CallContacts(false));
        let _ = app.update(Message::Navigate(NavSection::Chats));
        group_page(&mut app, GroupSettingsPane::Overview);
        render(&app, DESKTOP, &format!("group-actions-{tone}"));
    }
}

#[test]
fn group_dm_acceptance_keeps_address_requests_with_the_same_id() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    app.chats[0].pending_request_id = Some(42);
    app.chats[0].member_request = false;
    app.chats[2].pending_request_id = Some(42);
    app.chats[2].member_request = true;
    let _ = app.update(Message::ContactRequestHandled { req_id: 42, member: true, accepted: true, result: Ok("Accepted".into()) });
    let list = app.chats.clone();
    let _ = app.update(Message::ChatsLoaded(Ok(list)));
    assert_eq!(app.chats[0].pending_request_id, Some(42));
    assert!(app.chats.iter().find(|chat| chat.member_request).unwrap().pending_request_id.is_none());
}

#[test]
fn failed_request_action_does_not_replace_another_chats_request() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    app.chats[0].pending_request_id = Some(42);
    app.chats[2].pending_request_id = Some(77);
    app.chats[2].member_request = true;
    select(&mut app, 2);
    let _ = app.update(Message::ContactRequestHandled { req_id: 42, member: false, accepted: true, result: Err("temporary error".into()) });
    assert_eq!(app.chats[0].pending_request_id, Some(42));
    assert_eq!(app.chats[2].pending_request_id, Some(77));
    assert!(app.chats[2].member_request);
}

#[test]
fn rejected_dm_keeps_the_composer_blocked_until_the_chat_is_removed() {
    let mut app = demo_app(true, DESKTOP.width, DESKTOP.height);
    app.chats[0].pending_request_id = Some(42);
    app.chats[0].member_request = true;
    let _ = app.update(Message::ContactRequestHandled { req_id: 42, member: true, accepted: false, result: Ok("Rejected".into()) });
    assert_eq!(app.chats[0].pending_request_id, Some(42));
    assert!(app.handled_contact_requests.contains(&(true, 42)));
}


#[test]
fn failed_connect_preserves_the_form_for_retry() {
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    app.compose_open = true;
    app.invite_connecting = true;
    let link = "https://example.com/i#test";
    app.invite_input = link.into();

    let _ = app.update(Message::InviteInputChanged("different".into()));
    assert_eq!(app.invite_input(), link);

    let _ = app.update(Message::InviteAccepted {
        from_form: true,
        result: Err("Could not connect.".into()),
    });
    assert!(app.compose_open);
    assert!(!app.invite_connecting());
    assert_eq!(app.invite_input(), link);
    assert_eq!(app.invite_error(), Some("Could not connect."));

    let _ = app.update(Message::InviteInputChanged("corrected".into()));
    assert!(app.invite_error().is_none());
    let _ = app.update(Message::InviteAccepted { from_form: true, result: Ok("Request sent.".into()) });
    assert!(!app.compose_open);
    assert!(app.invite_input().is_empty());
}

/// A tap on the group chat's title bar opens the group page — and the same
/// tap's release must not then land on the page it opened (the owner's group
/// picture sat under the finger and opened its photo sheet).
#[test]
fn chat_header_tap_opens_the_profile_and_nothing_under_it() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, touch, Event, Point};
    load_fonts();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let size = Size::new(400.0, 900.0);
    for touch_input in [false, true] {
        let mut app = demo_app(true, size.width, size.height);
        select(&mut app, 1);
        assert!(app.group_settings.is_none());
        // Over the title, which is where the group picture is once the page opens.
        let at = Point::new(200.0, 36.0);
        let (press, release) = if touch_input {
            (
                Event::Touch(touch::Event::FingerPressed { id: touch::Finger(7), position: at }),
                Event::Touch(touch::Event::FingerLifted { id: touch::Finger(7), position: at }),
            )
        } else {
            (
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            )
        };
        let mut cache = Cache::default();
        let mut all = Vec::new();
        // Press and release in separate frames: the view is rebuilt from the
        // app after each, as when running.
        for event in [press, release] {
            let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, &mut renderer);
            let mut messages = Vec::new();
            let _ = ui.update(&[event], mouse::Cursor::Available(at), &mut renderer, &mut iced::advanced::clipboard::Null, &mut messages);
            cache = ui.into_cache();
            for m in messages {
                all.push(format!("{m:?}"));
                let _ = app.update(m);
            }
        }
        assert!(app.group_settings.is_some(), "the tap opened the group page ({all:?})");
        assert!(app.photo_menu.is_none(), "…and not its photo sheet ({all:?})");
    }
}

/// Escape on a phone's profile page slides it out once: the held-back
/// Escape, applied when the slide ends, used to start a second slide.
#[test]
fn escape_slides_a_profile_out_once() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    struct Slide;
    impl Drop for Slide {
        fn drop(&mut self) {
            crate::TEST_PAGES_SLIDE.with(|s| s.set(false));
        }
    }
    crate::TEST_PAGES_SLIDE.with(|s| s.set(true));
    let _slide = Slide;
    for group in [false, true] {
        let mut app = demo_app(true, MOBILE.width, MOBILE.height);
        if group {
            group_page(&mut app, GroupSettingsPane::Overview);
        } else {
            select(&mut app, 0);
            let _ = app.open_contact_profile();
        }
        app.page_key = app.current_page();
        let _ = app.update(Message::Escape);
        assert!(app.page_motion.is_some(), "Escape slides the page out (group={group})");
        // The slide ends.
        let later = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let _ = app.update(Message::MotionFrame(later));
        assert!(app.page_motion.is_none(), "no second slide (group={group})");
        assert!(app.group_settings.is_none() && app.contact_profile.is_none(), "the page closed (group={group})");
        assert!(app.selected.is_some(), "…back to the conversation (group={group})");
    }
}

/// "Show more" after the folded description unfolds it — and on your own
/// group, where the text is a button that edits it, it only unfolds.
#[test]
fn show_more_unfolds_and_does_not_edit() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, touch, Event, Point};
    load_fonts();
    let size = Size::new(400.0, 820.0);
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let long = "Climber, coffee person and part-time map collector. Usually somewhere between Arco and the \
                Dolomites, planning the next trip. Ask me about routes, gear or where the good espresso is.";
    for touch_input in [false, true] {
        let mut app = demo_app(false, size.width, size.height);
        group_page(&mut app, GroupSettingsPane::Overview);
        let g = app.group_settings.as_mut().unwrap();
        assert_eq!(g.my_role, "owner", "your own group: the text edits");
        g.short_descr = long.into();
        // Where "Show more" is drawn: the accent text right of the rows' icons.
        let theme = app.iced_theme();
        let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, Cache::default(), &mut renderer);
        let mut sink = Vec::new();
        let _ = ui.update(
            &[Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))],
            mouse::Cursor::Unavailable, &mut renderer, &mut iced::advanced::clipboard::Null, &mut sink,
        );
        let base = iced::theme::Base::base(&theme);
        ui.draw(&mut renderer, &theme, &iced::advanced::renderer::Style { text_color: base.text_color }, mouse::Cursor::Unavailable);
        let rgba = renderer.screenshot(iced::Size::new(size.width as u32, size.height as u32), 1.0, base.background_color);
        // The palette is process-wide and other tests switch it: accept the
        // accent of either appearance.
        let targets = [false, true].map(|light| {
            let a = crate::theme::Tokens::for_light(light).accent;
            [a.r, a.g, a.b].map(|c| (c * 255.0) as i32)
        });
        let mut hits = Vec::new();
        for y in 300..560 {
            for x in 120..390 {
                let i = ((y * size.width as usize) + x) * 4;
                let px = [rgba[i] as i32, rgba[i + 1] as i32, rgba[i + 2] as i32];
                if targets.iter().any(|t| (0..3).map(|k| (px[k] - t[k]).abs()).sum::<i32>() < 30) {
                    hits.push((x as f32, y as f32));
                }
            }
        }
        assert!(hits.len() > 20, "\"Show more\" is drawn");
        let at = Point::new(
            hits.iter().map(|h| h.0).sum::<f32>() / hits.len() as f32,
            hits.iter().map(|h| h.1).sum::<f32>() / hits.len() as f32,
        );
        let events = if touch_input {
            vec![
                Event::Touch(touch::Event::FingerPressed { id: touch::Finger(4), position: at }),
                Event::Touch(touch::Event::FingerLifted { id: touch::Finger(4), position: at }),
            ]
        } else {
            vec![
                Event::Mouse(mouse::Event::CursorMoved { position: at }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ]
        };
        let mut messages = Vec::new();
        for event in events {
            let _ = ui.update(&[event], mouse::Cursor::Available(at), &mut renderer, &mut iced::advanced::clipboard::Null, &mut messages);
        }
        let names: Vec<String> = messages.iter().map(|m| format!("{m:?}")).collect();
        assert!(messages.iter().any(|m| matches!(m, Message::ToggleAbout)), "it unfolds (touch={touch_input}): {names:?}");
        assert!(!messages.iter().any(|m| matches!(m, Message::OpenFieldEditor(_))), "and doesn't edit (touch={touch_input}): {names:?}");
    }
}

/// Phone: changing destination slides the lists sideways, in the bar's order,
/// and the slide ends at rest on the new list.
#[test]
fn destinations_slide_sideways() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    struct Slide;
    impl Drop for Slide {
        fn drop(&mut self) {
            crate::TEST_PAGES_SLIDE.with(|s| s.set(false));
        }
    }
    crate::TEST_PAGES_SLIDE.with(|s| s.set(true));
    let _slide = Slide;
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    let _ = app.update(Message::Navigate(NavSection::Settings));
    let (from, out, inc) = app.section_slide().expect("Chats → Settings slides");
    assert_eq!(from, NavSection::Chats);
    assert!(inc > 0.9 && out > -0.1, "Settings comes in from the right ({out}, {inc})");
    // Half way: both lists show, side by side.
    app.motion_now += std::time::Duration::from_millis(40);
    let (_, out, inc) = app.section_slide().unwrap();
    assert!((inc - out - 1.0).abs() < 1e-4 && inc > 0.0 && inc < 1.0, "side by side ({out}, {inc})");
    let _ = app.update(Message::MotionFrame(std::time::Instant::now() + std::time::Duration::from_secs(2)));
    assert!(app.section_slide().is_none(), "at rest after the slide");
    // Back to Chats: from the left.
    let _ = app.update(Message::Navigate(NavSection::Chats));
    let (_, _, inc) = app.section_slide().unwrap();
    assert!(inc < -0.9, "Chats comes in from the left ({inc})");
    // Tapping the destination you're on doesn't slide.
    let _ = app.update(Message::MotionFrame(std::time::Instant::now() + std::time::Duration::from_secs(2)));
    let _ = app.update(Message::Navigate(NavSection::Chats));
    assert!(app.section_slide().is_none());
}

#[test]
fn composer_mic_click_reaches_the_app() {
    // The mic is a glass square; glass shields its taps, so a press target
    // wrapped around the glass (instead of inside it) never fired.
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use iced::{mouse, Event, Point};
    load_fonts();
    for size in [MOBILE, DESKTOP] {
        let mut app = demo_app(true, size.width, size.height);
        select(&mut app, 0);
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::with_name("Noto Sans"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let mut cache = Cache::default();
        let mut hit = false;
        'scan: for dy in (4..140).step_by(6) {
            for dx in (4..140).step_by(6) {
                let point = Point::new(size.width - dx as f32, size.height - dy as f32);
                for event in [
                    mouse::Event::CursorMoved { position: point },
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                ] {
                    let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, &mut renderer);
                    let mut messages = Vec::new();
                    let _ = ui.update(
                        &[Event::Mouse(event)],
                        mouse::Cursor::Available(point),
                        &mut renderer,
                        &mut iced::advanced::clipboard::Null,
                        &mut messages,
                    );
                    cache = ui.into_cache();
                    if messages.iter().any(|m| matches!(m, Message::ToggleVoiceRecord)) {
                        hit = true;
                        break 'scan;
                    }
                }
            }
        }
        assert!(hit, "clicking the composer mic sends ToggleVoiceRecord ({size:?})");
    }
}

#[test]
fn chat_opened_from_calls_slides_over_calls_and_back_returns_there() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    struct Slide;
    impl Drop for Slide {
        fn drop(&mut self) {
            crate::TEST_PAGES_SLIDE.with(|s| s.set(false));
        }
    }
    crate::TEST_PAGES_SLIDE.with(|s| s.set(true));
    let _slide = Slide;
    let later = || std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    let _ = app.update(Message::Navigate(NavSection::Calls));
    let _ = app.update(Message::MotionFrame(later()));
    // Tap a call with Alice (contact 1): her chat opens over the Calls list.
    let _ = app.update(Message::Call(crate::call_flow::CallMsg::OpenChat(1)));
    assert_eq!(app.nav(), NavSection::Chats);
    assert_eq!(app.list_nav(), NavSection::Calls, "the Calls list stays under the page");
    assert!(app.page_slide().is_some_and(|(offset, under)| offset > 0.5
        && under == crate::page_motion::Under::List), "the chat slides in");
    let _ = app.update(Message::MotionFrame(later()));
    // Back slides the chat out, onto Calls.
    let _ = app.update(Message::Back);
    let _ = app.update(Message::MotionFrame(later()));
    assert!(app.selected().is_none());
    assert_eq!(app.nav(), NavSection::Calls, "Back returns to Calls");
    assert_eq!(app.list_nav(), NavSection::Calls);
}

/// One headless frame that also delivers `events` (a wheel scroll…).
fn frame_with_events(
    app: &mut App,
    size: Size,
    renderer: &mut iced::Renderer,
    cache: Cache,
    events: &[iced::Event],
    cursor: iced::mouse::Cursor,
) -> (Vec<u8>, Cache) {
    let theme = app.iced_theme();
    let mut msgs: Vec<Message> = Vec::new();
    let mut ui = UserInterface::build(app.view(iced::window::Id::unique()), size, cache, renderer);
    let _ = ui.update(events, cursor, renderer, &mut iced::advanced::clipboard::Null, &mut msgs);
    let base = iced::theme::Base::base(&theme);
    ui.draw(renderer, &theme, &iced::advanced::renderer::Style { text_color: base.text_color }, cursor);
    let cache = ui.into_cache();
    for m in msgs {
        let _ = app.update(m);
    }
    let px = iced::Size::new(size.width as u32, size.height as u32);
    (renderer.screenshot(px, 1.0, base.background_color), cache)
}

#[test]
fn phone_chat_list_keeps_its_scroll_after_a_chat() {
    let _pointer_test = POINTER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_fonts();
    let mut app = demo_app(true, MOBILE.width, MOBILE.height);
    let base = app.chats[0].clone();
    for i in 0..40 {
        let mut c = base.clone();
        c.id = cursus_core::ChatId::new(ChatType::Direct, 500 + i);
        c.title = format!("Contact {i}");
        app.chats.push(c);
    }
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Noto Sans"), iced::Pixels(16.0), Some("tiny-skia"))).unwrap();
    let redraw = [iced::Event::Window(iced::window::Event::RedrawRequested(iced::time::Instant::now()))];
    let at = iced::mouse::Cursor::Available(iced::Point::new(200.0, 400.0));
    let (_, mut cache) = frame_with_events(&mut app, MOBILE, &mut renderer, Cache::default(), &redraw, at);
    // Scroll the list well down.
    for _ in 0..6 {
        let wheel = [iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -200.0 },
        })];
        let (_, c) = frame_with_events(&mut app, MOBILE, &mut renderer, cache, &wheel, at);
        cache = c;
    }
    let (scrolled, c) = frame_with_events(&mut app, MOBILE, &mut renderer, cache, &redraw, at);
    cache = c;
    let (top, _) = frame_pixels(&app, MOBILE, &mut renderer, Cache::default());
    assert!(scrolled != top, "the wheel scrolled the list");
    // Open a chat, then come back.
    select(&mut app, 3);
    let (_, c) = frame_with_events(&mut app, MOBILE, &mut renderer, cache, &redraw, at);
    cache = c;
    let (_, c) = frame_with_events(&mut app, MOBILE, &mut renderer, cache, &redraw, at);
    cache = c;
    let _ = app.update(Message::Back);
    let _ = app.update(Message::MotionFrame(std::time::Instant::now() + std::time::Duration::from_secs(5)));
    assert!(app.selected().is_none());
    let (back, _) = frame_with_events(&mut app, MOBILE, &mut renderer, cache, &redraw, at);
    assert!(back == scrolled, "Back shows the list where it was scrolled");
}
