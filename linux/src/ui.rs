use iced::font::{self, Font};
use iced::keyboard;
use iced::keyboard::key;
use iced::widget::canvas::{self, Canvas, Fill, Frame, Geometry};
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::space::{horizontal, vertical};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::text_editor::Binding as EditorBinding;
use iced::widget::{
    button, column, container, float, image, mouse_area, progress_bar, rich_text, row, scrollable, span,
    text, text_editor, text_input, Column, Row, Space, Stack,
};
use iced::{
    mouse, Alignment, Color, ContentFit, Element, Length, Padding, Point, Rectangle, Renderer, Size,
    Theme,
};
use unicode_segmentation::UnicodeSegmentation;

use cursus_core::{
    group_role_rank, ChatMessage, ChatType, DeliveryStatus, FileKind, FileTransferState,
    FormatColor, TextSegment, TextStyle, KNOWN_REACTIONS, LARGE_EMOJI_MAX,
};

use crate::ellipsis::ellipsis_text;
use crate::hover;
use crate::icons::{self, NavIcon};
use crate::layout::{Density, LayoutMode};
use crate::media;
use crate::preference_ui::{group as preference_group, Sections as PreferenceSections};
use crate::theme;
use crate::FeatureAllowLabel as _;
use crate::{
    can_edit_group_profile, can_manage_members, can_moderate, group_pref_on, group_support_allowed,
    member_status_is_pending, member_status_shows_pending_icon, App, AuthScreen, ChatMenuAction,
    ChatMenuPane, CreateKind, FeatureAllow, GroupSettingsPane, Message, MsgMenuAction, MsgMenuPane,
    PrefMenu, SupportPane,
};

/// Color-emoji family — use for glyph chips so ❤ becomes the red ❤️.
const EMOJI_FONT: Font = Font::with_name("Noto Color Emoji");
/// Names, titles and section headers (the only other bundled weight).
pub const BOLD: Font = Font {
    family: font::Family::Name("Noto Sans"),
    weight: font::Weight::Bold,
    stretch: font::Stretch::Normal,
    style: font::Style::Normal,
};

/// Invisible vertical scrollbar (still scrollable via wheel / touch).
fn no_bar() -> Scrollbar {
    Scrollbar::new().width(0).scroller_width(0).margin(0)
}

/// Header back — same hit language as composer file/rec/send (squarish, icon glow).
fn mobile_back_btn(on_press: Message, hover: bool) -> Element<'static, Message> {
    // Navigation finishes on release: rebuilding on press lets the release
    // land on the conversation header below and reopen the profile.
    button(icons::back_tool(32.0, hover))
        .on_press(on_press)
        .padding(0)
        .style(theme::icon_action(hover))
        .into()
}

/// Bold page heading, with a Back button in front when `back` is set.
fn page_title<'a>(
    title: impl iced::widget::text::IntoFragment<'a>,
    back: Option<Message>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let title = text(title)
        .size(d.title)
        .font(BOLD)
        .color(theme::t().text_primary);
    match back {
        Some(msg) => row![mobile_back_btn(msg, hover), title.width(Length::Fill)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        None => title.into(),
    }
}

/// Composer tool hit — press fires immediately (before OSK dismiss steals the
/// gesture on some ARM compositors). mouse_area avoids `button`'s focus steal.
fn composer_hit<'a>(
    icon: Element<'a, Message>,
    size: f32,
    on_press: Message,
) -> Element<'a, Message> {
    mouse_area(
        container(icon)
            .width(size)
            .height(size)
            .center_x(size)
            .center_y(size),
    )
    .on_press(on_press)
    .interaction(mouse::Interaction::Pointer)
    .into()
}

fn composer_inert_hit<'a>(icon: Element<'a, Message>, size: f32) -> Element<'a, Message> {
    container(icon)
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .into()
}

/// Send control with accent fill when the composer has text.
fn composer_send_hit<'a>(
    icon: Element<'a, Message>,
    size: f32,
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let face = if active {
        container(icon)
            .width(size)
            .height(size)
            .center_x(size)
            .center_y(size)
            .style(|_| {
                let c = theme::t();
                container::Style {
                    background: Some(iced::Background::Color(c.accent)),
                    border: iced::Border {
                        radius: theme::SMALL_CONTROL_RADIUS.into(),
                        ..Default::default()
                    },
                    shadow: floating_shadow(),
                    ..container::Style::default()
                }
            })
    } else {
        container(icon)
            .width(size)
            .height(size)
            .center_x(size)
            .center_y(size)
    };
    // on_press (not release): Wayland/OSK often eats the first outside-tap as
    // “dismiss keyboard”, so waiting for release never sees a send gesture.
    mouse_area(face).on_press(on_press).interaction(mouse::Interaction::Pointer).into()
}

/// Max width of page content so rows don't stretch on wide windows.
const PAGE_MAX: f32 = crate::layout::CONTENT_MAX_WIDTH;

/// Scrolling page body: standard insets, centred column capped at [`PAGE_MAX`].
fn page_scroll<'a>(body: impl Into<Element<'a, Message>>, d: Density) -> Element<'a, Message> {
    page_scroll_y(
        container(container(body).max_width(PAGE_MAX).width(Length::Fill))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(Padding {
                top: d.pad_tight,
                right: d.pad,
                bottom: d.pad,
                left: d.pad,
            }),
    )
}

fn scroll_y<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    // Default list/settings scroll — kinetic fling on touch (GTK-like).
    kinetic_scroll("page-scroll", content)
}

/// Scroll for pages full of buttons/toggles: a swipe that starts on a control
/// still scrolls (touch presses are deferred until they prove to be taps).
fn page_scroll_y<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    kinetic_scroll("page-scroll", crate::tap::defer_touch(content))
}

fn kinetic_scroll<'a>(
    id: &'static str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let scroller = scrollable(crate::touch_content::content(content))
        .id(iced::widget::Id::new(id))
        .height(Length::Fill)
        .direction(Direction::Vertical(no_bar()));
    crate::kinetic::track(id, crate::tap::guard(scroller))
}

/// Text with Advanced shaping so emoji can fall back to Noto Color Emoji.
fn txt<'a>(
    content: impl iced::widget::text::IntoFragment<'a>,
) -> iced::widget::Text<'a, Theme, Renderer> {
    text(content).shaping(Shaping::Advanced)
}

/// Emoji glyph forced onto the color-emoji font (avoids dingbat black hearts).
fn emoji_glyph<'a>(
    glyph: impl iced::widget::text::IntoFragment<'a>,
) -> iced::widget::Text<'a, Theme, Renderer> {
    text(glyph)
        .font(EMOJI_FONT)
        .shaping(Shaping::Advanced)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    Chats,
    Channels,
    Calls,
    Settings,
}

impl NavSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chats => "Chats",
            Self::Channels => "Channels",
            Self::Calls => "Calls",
            Self::Settings => "Settings",
        }
    }

    pub fn all() -> [NavSection; 4] {
        [Self::Chats, Self::Channels, Self::Calls, Self::Settings]
    }

    fn icon(self) -> NavIcon {
        match self {
            Self::Chats => NavIcon::Chats,
            Self::Channels => NavIcon::Channels,
            Self::Calls => NavIcon::Calls,
            Self::Settings => NavIcon::Settings,
        }
    }
}

/// Single entry point — layout mode picks the shell; state lives in [`App`].
pub fn shell(app: &App) -> Element<'_, Message> {
    if app.auth().is_some() {
        return auth_gate(app);
    }
    // App lock: nothing of the chats is even built while locked.
    if let Some(lock) = app.lock_state() {
        return lock_screen(app, lock);
    }
    // The image viewer is opaque. Keep the page's widget tree for Back, but
    // do not construct, lay out or paint thousands of hidden chat rows per photo.
    let base = crate::retained::view(if app.image_viewer.is_some() { None } else { Some(match app.layout() {
        LayoutMode::Desktop => desktop_shell(app),
        LayoutMode::Mobile => stacked_shell(app),
    }) });
    let modal = app.compose_open() || app.call_contacts_open || app.chat_menu().is_some()
        || app.pending_url().is_some()
        || app.privacy_confirm().is_some()
        || app.pending_links_open()
        || app.photo_menu().is_some()
        || app.link_confirm().is_some()
        || app.setting_info().is_some()
        || app.message_menu().is_some()
        || app.reaction_members().is_some()
        || app.create_chat().is_some()
        || app.pref_menu().is_some()
        || app.field_editor().is_some()
        || app.image_viewer.is_some();
    let mut stack = Stack::new().width(Length::Fill).height(Length::Fill).push(crate::input_gate::freeze(base, modal));
    if app.compose_open() {
        let height = (app.height - 80.0).clamp(180.0, 560.0);
        let card = container(compose_page(app, true)).width(Length::Fill)
            .height((height - 2.0 * app.density().pad).max(120.0));
        stack = stack.push(sheet_overlay(menu_card(card.into(), app.density()), Message::CloseCompose));
    }
    if app.call_contacts_open {
        stack = stack.push(call_contacts_overlay(app));
    }
    if app.image_viewer.is_some() {
        stack = stack.push(image_gallery(app));
    }
    if app.chat_menu().is_some() {
        stack = stack.push(chat_menu_overlay(app));
    }
    if app.pending_url().is_some() {
        stack = stack.push(open_url_confirm_overlay(app));
    }
    if app.privacy_confirm().is_some() {
        stack = stack.push(privacy_confirm_overlay(app));
    }
    if app.pending_links_open() {
        stack = stack.push(pending_links_overlay(app));
    }
    if let Some(target) = app.photo_menu() {
        stack = stack.push(photo_menu_overlay(app, target));
    }
    if let Some(confirm) = app.link_confirm() {
        stack = stack.push(link_confirm_overlay(app, confirm));
    }
    if let Some(info) = app.setting_info() {
        stack = stack.push(setting_info_overlay(app, info));
    }
    if app.message_menu().is_some() {
        stack = stack.push(message_menu_overlay(app));
    }
    if app.reaction_members().is_some() {
        stack = stack.push(reaction_members_overlay(app));
    }
    if app.create_chat().is_some() && !app.compose_open() {
        stack = stack.push(create_chat_overlay(app));
    }
    if app.pref_menu().is_some() {
        stack = stack.push(pref_menu_overlay(app));
    }
    if app.field_editor().is_some() {
        stack = stack.push(field_editor_overlay(app));
    }
    stack.into()
}

/// Lock screen (same card as unlock): works with a mouse + keyboard (field
/// focused, Enter unlocks) and on phones (wide field, large Unlock button
/// above the on-screen keyboard, Show/Hide for typing long passwords).
fn lock_screen<'a>(app: &'a App, lock: &'a crate::LockState) -> Element<'a, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let narrow = !matches!(app.layout(), LayoutMode::Desktop);
    let waiting = lock.retry_at.is_some_and(|t| std::time::Instant::now() < t);
    let mut body = Column::new().push(
        row![
            gate_field(
                "Database password",
                lock.password.as_str(),
                !lock.reveal,
                |s| Message::LockPasswordChanged(crate::Secret::new(s)),
                Message::LockSubmit,
                d,
            )
            .id(iced::widget::Id::new("lock-password")),
            reveal_toggle(lock.reveal, Message::LockToggleReveal, d, hover),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    if let Some(err) = lock.error.as_ref() {
        body = body.push(text(err.clone()).size(d.meta).color(theme::t().danger));
    } else if waiting {
        body = body.push(text("Too many attempts — wait a moment.").size(d.meta).color(theme::t().danger));
    }
    let can_submit = !lock.checking && !lock.password.as_str().is_empty();
    body = body.push(gate_button(
        if lock.checking { "Checking…" } else { "Unlock" },
        can_submit.then_some(Message::LockSubmit),
        d,
        hover,
    ));
    if !narrow {
        body = body.push(
            container(
                text("Press Enter to unlock · Ctrl+L locks again")
                    .size(d.meta)
                    .color(theme::t().text_muted),
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
        );
    }
    gate_card(
        app,
        icons::lock_icon(40.0, theme::t().accent),
        None,
        "Cursus is locked".into(),
        "Enter your database password to continue.".into(),
        body,
    )
}

/// Shared look of the welcome / unlock / lock cards: icon, centred title and
/// subtitle, then the body. Phones keep the card high so the on-screen
/// keyboard never covers the button; desktops centre it.
fn gate_card<'a>(
    app: &'a App,
    icon: Element<'a, Message>,
    eyebrow: Option<String>,
    title: String,
    subtitle: String,
    body: Column<'a, Message>,
) -> Element<'a, Message> {
    let d = app.density();
    let narrow = !matches!(app.layout(), LayoutMode::Desktop);
    let centred = |e: Element<'a, Message>| -> Element<'a, Message> {
        container(e).width(Length::Fill).center_x(Length::Fill).into()
    };
    let mut col = Column::new().spacing(12).width(Length::Fill);
    col = col.push(centred(icon));
    if let Some(eyebrow) = eyebrow {
        col = col.push(centred(
            text(eyebrow).size(d.meta).font(BOLD).color(theme::t().accent).into(),
        ));
    }
    col = col.push(centred(
        text(title)
            .size(d.title + 4.0)
            .font(BOLD)
            .color(theme::t().text_primary)
            .align_x(Alignment::Center)
            .into(),
    ));
    col = col.push(centred(
        text(subtitle)
            .size(d.meta)
            .color(theme::t().text_secondary)
            .align_x(Alignment::Center)
            .into(),
    ));
    col = col.push(vertical().height(4));
    col = col.push(body.spacing(12).width(Length::Fill));
    let panel = container(col)
        .width(Length::Fill)
        .max_width(400)
        .padding(if narrow { Padding::from([24, 18]) } else { Padding::from([32, 32]) })
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_elevated)),
            border: iced::Border {
                radius: theme::CONTROL_RADIUS.into(),
                width: 1.0,
                color: theme::t().border,
            },
            ..container::Style::default()
        });
    // Phones keep the card in the upper part (room for the keyboard), but
    // not glued to the top of a tall screen.
    let top = if narrow { (app.height * 0.14).clamp(32.0, 140.0) } else { 0.0 };
    let outer = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(Padding { top, right: 16.0, bottom: 0.0, left: 16.0 });
    let outer = if narrow { outer.align_y(Alignment::Start) } else { outer.center_y(Length::Fill) };
    outer
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            ..container::Style::default()
        })
        .into()
}

/// Wide rounded field used on the gate cards.
fn gate_field<'a>(
    placeholder: &'a str,
    value: &'a str,
    secure: bool,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    d: Density,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .secure(secure)
        .on_input(on_input)
        .on_submit(on_submit)
        .padding(Padding::from([14, 16]))
        .size(d.body + 1.0)
        .width(Length::Fill)
        .style(theme::rounded_field())
}

/// Full-width main action of a gate card (disabled while `busy`).
fn gate_button<'a>(label: &'a str, msg: Option<Message>, d: Density, hover: bool) -> Element<'a, Message> {
    button(
        container(text(label).size(d.body).font(BOLD))
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .on_press_maybe(msg)
    .padding(Padding::from([14, 18]))
    .width(Length::Fill)
    .style(theme::primary_action(hover))
    .into()
}

/// "Show" / "Hide" next to a password field.
fn reveal_toggle<'a>(revealed: bool, msg: Message, d: Density, hover: bool) -> Element<'a, Message> {
    button(
        text(if revealed { "Hide" } else { "Show" })
            .size(d.meta)
            .color(theme::t().accent),
    )
    .on_press(msg)
    .padding(Padding::from([14, 12]))
    .style(theme::ghost_tile(hover))
    .into()
}

fn auth_gate(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let busy = app.connecting();
    let reveal = app.auth_reveal();
    let Some(auth) = app.auth() else {
        return vertical().height(0).into();
    };
    let lock = || icons::lock_icon(40.0, theme::t().accent);
    // Errors from the last attempt, shown inline in the card.
    let status = app.status_text().trim();
    let lower = status.to_lowercase();
    let error = (!status.is_empty()
        && ["error", "fail", "wrong", "could not", "timed out", "keyring", "do not match", "at least", "enter a display name"]
            .iter()
            .any(|k| lower.contains(k)))
    .then(|| {
        text(status.to_string()).size(d.meta).color(theme::t().danger)
    });

    // Keyring auto-unlock: a calm splash instead of a form that flashes away.
    if app.auto_unlocking() {
        return gate_card(
            app,
            lock(),
            None,
            "Unlocking…".into(),
            "Using the password in your system keyring.".into(),
            Column::new(),
        );
    }

    match auth {
        AuthScreen::WelcomeName { name } => {
            let mut body = Column::new().push(crate::edit_menu::edit_field(
                gate_field("Your name", name, false, Message::WelcomeNameChanged, Message::WelcomeNameNext, d)
                    .id("edit-field-WelcomeNameChanged"),
                iced::widget::Id::new("edit-field-WelcomeNameChanged"),
                name,
                Message::WelcomeNameChanged,
                true,
            ));
            if let Some(e) = error {
                body = body.push(e);
            }
            body = body.push(gate_button(
                if busy { "Working…" } else { "Continue" },
                (!busy).then_some(Message::WelcomeNameNext),
                d,
                hover,
            ));
            gate_card(
                app,
                icons::user_alt_icon(40.0, theme::t().accent),
                Some("Step 1 of 2".into()),
                "Welcome to Cursus".into(),
                "Pick the name your contacts will see. You can change it later.".into(),
                body,
            )
        }
        AuthScreen::WelcomePassword { name, password, confirm } => {
            let mut body = Column::new()
                .push(
                    row![
                        gate_field(
                            "Password (at least 8 characters)",
                            password.as_str(),
                            !reveal,
                            |s| Message::WelcomePasswordChanged(crate::Secret::new(s)),
                            Message::WelcomeSubmit,
                            d,
                        ),
                        reveal_toggle(reveal, Message::AuthToggleReveal, d, hover),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .push(gate_field(
                    "Repeat password",
                    confirm.as_str(),
                    !reveal,
                    |s| Message::WelcomeConfirmChanged(crate::Secret::new(s)),
                    Message::WelcomeSubmit,
                    d,
                ));
            if let Some(e) = error {
                body = body.push(e);
            }
            body = body.push(gate_button(
                if busy { "Creating…" } else { "Create profile" },
                (!busy).then_some(Message::WelcomeSubmit),
                d,
                hover,
            ));
            body = body.push(
                container(
                    button(text("Back").size(d.meta).color(theme::t().text_secondary))
                        .on_press_maybe((!busy).then_some(Message::WelcomeBack))
                        .padding(Padding::from([8, 14]))
                        .style(theme::ghost_tile(hover)),
                )
                .width(Length::Fill)
                .center_x(Length::Fill),
            );
            gate_card(
                app,
                lock(),
                Some("Step 2 of 2".into()),
                format!("Protect your chats, {name}"),
                "This password encrypts everything Cursus stores. It is never saved — you'll enter it when Cursus starts, so keep it somewhere safe.".into(),
                body,
            )
        }
        AuthScreen::Unlock { display_name, password } => {
            let mut body = Column::new().push(
                row![
                    gate_field(
                        "Database password",
                        password.as_str(),
                        !reveal,
                        |s| Message::UnlockPasswordChanged(crate::Secret::new(s)),
                        Message::UnlockSubmit,
                        d,
                    )
                    .id(iced::widget::Id::new("unlock-password")),
                    reveal_toggle(reveal, Message::AuthToggleReveal, d, hover),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
            if let Some(e) = error {
                body = body.push(e);
            }
            body = body.push(gate_button(
                if busy { "Unlocking…" } else { "Unlock" },
                (!busy && !password.as_str().is_empty()).then_some(Message::UnlockSubmit),
                d,
                hover,
            ));
            gate_card(
                app,
                lock(),
                None,
                format!("Welcome back, {display_name}"),
                "Enter your password to open your chats.".into(),
                body,
            )
        }
    }
}

fn desktop_shell<'a>(app: &'a App) -> Element<'a, Message> {
    let d = app.density();
    let mut columns = Row::new().height(Length::Fill);
    // Destinations: a rail when there is room, else a bar under the list.
    if d.rail_width > 0.0 {
        columns = columns.push(nav_rail(app, d)).push(list_pane(app, Length::Fixed(d.list_width)));
    } else {
        columns = columns.push(container(list_with_bottom_nav(app)).width(d.list_width).height(Length::Fill));
    }
    columns = columns.push(pane_divider());
    // Wide windows: an open profile sits beside its chat instead of over it.
    use crate::page_motion::{slide, Slot};
    let moving = |content: Element<'a, Message>, slot: Slot| {
        let (remaining, _) = app.page_slide(slot);
        slide(content, remaining, false, theme::t().bg_panel)
    };
    match info_column(app) {
        Some(info) => columns
            .push(moving(
                container(conversation_pane(app, false)).width(Length::Fill).height(Length::Fill).style(theme::panel_style()).into(),
                Slot::Detail,
            ))
            .push(pane_divider())
            .push(moving(
                container(info).width(d.info_width).height(Length::Fill).style(theme::panel_style()).into(),
                Slot::Info,
            )),
        None => columns.push(moving(content_pane(app, false), Slot::Detail)),
    }
    .into()
}

/// The open contact / group page as a third column, when the window has
/// room for it next to the conversation it belongs to.
fn info_column(app: &App) -> Option<Element<'_, Message>> {
    let in_chats = matches!(app.nav(), NavSection::Chats | NavSection::Channels);
    if app.density().info_width <= 0.0 || !in_chats || app.selected().is_none() || app.support().is_some() {
        return None;
    }
    if app.group_settings().is_some() {
        Some(group_settings_page(app, true))
    } else if app.contact_profile().is_some() {
        Some(contact_profile_page(app, true))
    } else {
        None
    }
}

/// Room a list keeps at the END of its scrolled content for the floating
/// destinations bar (bar, its bottom inset and the fade above it). It goes
/// inside the scrollable, not under it, so rows keep scrolling beneath the
/// bar and only the last one stops above it.
fn nav_clearance(d: Density) -> f32 {
    if d.nav_bar_height > 0.0 { d.nav_bar_height + 12.0 + GLASS_FADE } else { 0.0 }
}

/// The list with the destinations bar floating over its bottom edge (one
/// column, and two columns without a rail).
fn list_with_bottom_nav(app: &App) -> Element<'_, Message> {
    let float_inset = if app.layout() == LayoutMode::Mobile { 12.0 } else { 8.0 };
    // On a wide one-column window the list and the bar keep a readable
    // width in the middle instead of stretching edge to edge.
    // Like the composer: only the bar itself is glass; nothing is drawn
    // around it, and the list scrolls underneath.
    let bar = container(container(bottom_nav(app)).max_width(PHONE_LIST_MAX))
        .padding(Padding { top: 0.0, right: float_inset, bottom: float_inset, left: float_inset })
        .width(Length::Fill)
        .align_x(Alignment::Center);
    let nav = container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
    Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(
            container(container(list_pane(app, Length::Fill)).max_width(PHONE_LIST_MAX).height(Length::Fill))
                .center_x(Length::Fill)
                .height(Length::Fill)
                .style(theme::panel_style()),
        )
        .push(nav)
        .into()
}

/// 1px hairline between the list and the main pane.
fn pane_divider<'a>() -> Element<'a, Message> {
    container(Space::new().width(1).height(Length::Fill))
        .width(1)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().border)),
            ..container::Style::default()
        })
        .into()
}

fn stacked_shell(app: &App) -> Element<'_, Message> {
    let in_chat_nav = matches!(app.nav(), NavSection::Chats | NavSection::Channels);
    let show_detail = (app.nav() == NavSection::Settings && app.settings_open())
        || in_chat_nav
            && (app.selected().is_some()
                || app.contact_profile().is_some()
                || app.group_settings().is_some()
                || app.support().is_some());

    // Exclusive pages: a chat / settings page fully replaces the list and
    // its bar.
    if show_detail {
        let page = container(content_pane(app, true))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::panel_style());
        // Opened from the list, the page slides over it while it dims.
        let (remaining, full) = app.page_slide(crate::page_motion::Slot::Detail);
        let under: Element<'_, Message> = if full && remaining > 0.0 {
            Stack::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .push(list_with_bottom_nav(app))
                .push(
                    container(Space::new().width(Length::Fill).height(Length::Fill))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(Color { a: 0.18 * (1.0 - remaining), ..Color::BLACK })),
                            ..container::Style::default()
                        }),
                )
                .into()
        } else {
            Space::new().width(Length::Fill).height(Length::Fill).into()
        };
        // Same tree at rest and in motion, so the page keeps its state.
        return Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(under)
            .push(crate::page_motion::slide(page, remaining, full, theme::t().bg_panel))
            .into();
    }
    list_with_bottom_nav(app)
}

/// Widest the phone layout's list (and its bottom bar) gets.
const PHONE_LIST_MAX: f32 = 720.0;

fn nav_rail(app: &App, d: Density) -> Element<'_, Message> {
    // Primary destinations stay stacked from the top (where Chats already
    // sat). Settings matches their hit size and sits at the bottom of the rail.
    let pad_x = 8.0;
    let primary = column![
        nav_destination(app, NavSection::Chats, true, d.nav_hit, d),
        nav_destination(app, NavSection::Channels, true, d.nav_hit, d),
        nav_destination(app, NavSection::Calls, true, d.nav_hit, d),
    ]
    .spacing(8)
    .padding(Padding {
        top: 12.0,
        right: pad_x,
        bottom: 0.0,
        left: pad_x,
    })
    .align_x(Alignment::Center)
    .width(Length::Fill);

    let settings = container(nav_destination(
        app,
        NavSection::Settings,
        true,
        d.nav_hit,
        d,
    ))
    .padding(Padding {
        top: 0.0,
        right: pad_x,
        bottom: pad_x,
        left: pad_x,
    })
    .width(Length::Fill);

    let col = column![primary, vertical().height(Length::Fill), settings]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);

    container(col)
        .width(d.rail_width)
        .height(Length::Fill)
        .style(theme::sidebar_style())
        .into()
}

fn bottom_nav(app: &App) -> Element<'_, Message> {
    let d = app.density();
    // Equal vertical inset so the bar height hugs the destination buttons.
    let pad_v = theme::DOCK_INSET;
    let pad_h = theme::DOCK_INSET;
    let mut r = Row::new()
        .spacing(4)
        .padding(Padding {
            top: pad_v,
            right: pad_h,
            bottom: pad_v,
            left: pad_h,
        })
        .width(Length::Fill)
        .height(Length::Fill);

    for section in NavSection::all() {
        r = r.push(
            container(nav_destination(app, section, false, d.nav_hit, d))
                .width(Length::Fill)
                .height(Length::Fill),
        );
    }

    crate::glass::surface(
        container(r)
            .width(Length::Fill)
            .height(d.nav_bar_height)
            .style(theme::floating_nav_style()),
        theme::CONTROL_RADIUS,
    )
}

fn nav_destination(
    app: &App,
    section: NavSection,
    vertical: bool,
    hit: f32,
    d: Density,
) -> Element<'_, Message> {
    let active = app.nav() == section;
    let icon = icons::nav_icon(section.icon(), active, if vertical { d.icon } else { 22.0 });
    let hover_ok = app.hover_ok();

    let (width, height) = if vertical {
        // Fill the rail width; `hit` sets the square-ish target height.
        (Length::Fill, Length::Fixed(hit))
    } else {
        // Fill each bottom-nav cell.
        (Length::Fill, Length::Fill)
    };

    let content = container(
        column![
            icon,
            text(section.label())
                .size(if vertical { 10 } else { 11 })
                .color(if active { theme::t().accent } else { theme::t().text_secondary }),
        ]
        .spacing(3)
        .align_x(Alignment::Center),
    )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let btn = button(content)
        .on_press(Message::Navigate(section))
        .padding(0)
        .width(width)
        .height(height);

    let radius = if vertical { theme::CONTROL_RADIUS } else { theme::DOCK_SELECTION_RADIUS };
    let styled = if active {
        btn.style(theme::nav_active(hover_ok, radius))
    } else {
        btn.style(theme::nav_idle(hover_ok, radius))
    };

    // Rail items must stay Shrink. `center_y(Fill)` also sets height to Fill,
    // which made Settings share leftover space with the spacer and float the
    // gear in the middle of that band instead of sitting in the corner.
    if vertical {
        container(styled)
            .width(Length::Fill)
            .height(Length::Shrink)
            .center_x(Length::Fill)
            .into()
    } else {
        container(styled)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

fn list_pane(app: &App, width: Length) -> Element<'_, Message> {
    let d = app.density();
    let show_compose = matches!(app.nav(), NavSection::Chats | NavSection::Channels | NavSection::Calls)
        && app.connected();
    // Same header chrome for every section: title + trailing 36px slot so
    // Calls/Settings match the Chats/Channels "title row with button" box.
    let trailing: Element<'_, Message> = if show_compose {
        let plus = button(icons::centered_hit(
            if app.nav() == NavSection::Calls { icons::phone_icon(20.0, theme::t().accent) }
            else { icons::plus_icon(20.0, theme::t().accent) },
            36.0,
        ))
        .on_press(if app.nav() == NavSection::Calls { Message::CallContacts(true) } else { Message::OpenCompose })
        .padding(0)
        .style(theme::soft_action(app.hover_ok()));
        let pending = app.pending_links().len();
        if pending > 0 && app.nav() != NavSection::Calls {
            row![pending_links_button(pending, app.hover_ok()), plus]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
        } else {
            plus.into()
        }
    } else {
        horizontal().width(36.0).height(36.0).into()
    };
    let header: Element<'_, Message> = row![
        text(app.nav().label())
            .size(d.title + 4.0)
            .font(BOLD)
            .color(theme::t().text_primary)
            .width(Length::Fill),
        trailing,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into();

    let body: Element<'_, Message> = match app.nav() {
        NavSection::Chats | NavSection::Channels => chat_list(app),
        NavSection::Calls => calls_panel(app),
        NavSection::Settings => settings_panel(app),
    };

    let col = Column::new()
        .push(container(header).padding(Padding {
            top: d.pad,
            right: d.pad,
            bottom: d.pad_tight,
            left: d.pad,
        }))
        .push(body)
        .height(Length::Fill);

    // Settings is made of floating panels: it sits on the panel background
    // like its pages, so the panels read the same at every level.
    let settings = app.nav() == NavSection::Settings;
    container(col)
        .width(width)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(if settings { theme::t().bg_panel } else { theme::t().bg_list })),
            ..container::Style::default()
        })
        .into()
}

fn chat_list(app: &App) -> Element<'_, Message> {
    let d = app.density();
    if !app.connected() {
        return placeholder(
            "No connection",
            "Waiting for SimpleX core… If this sticks, run ./scripts/fetch-libs.sh and restart.",
            d,
        );
    }

    let mut col = Column::new().spacing(0).padding(Padding::ZERO);

    let rows = app.chats_for_nav();
    if rows.is_empty() {
        let (title, body) = match app.nav() {
            NavSection::Channels => (
                "No channels yet",
                "Tap + to create a channel (needs chat relays), or join via invite link.",
            ),
            _ => (
                "No chats yet",
                "Tap + to invite someone, create a group or channel, or paste a link.",
            ),
        };
        col = col.push(placeholder(title, body, d));
        return scroll_y(col);
    }

    for (index, chat) in rows {
        let selected = app.selected() == Some(index);
        let show_unread = chat.unread > 0 || chat.unread_chat;
        let muted = chat.settings.enable_ntfs.is_muted();
        let support_badge = app.support_badge_count(chat);

        // Line 1: name · (muted) · time.
        let mut top = Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .push(
                container(crate::ellipsis::ellipsis_text_weight(
                    chat.title.clone(),
                    d.body,
                    theme::t().text_primary,
                    1,
                    true,
                ))
                .width(Length::Fill)
                .clip(true),
            );
        if muted {
            top = top.push(icons::mute_icon(14.0, theme::t().text_muted));
        }
        if let Some(when) = format_list_time(chat.last_ts.as_deref()) {
            top = top.push(
                txt(when)
                    .size(d.meta)
                    .wrapping(Wrapping::None)
                    .color(if show_unread && !muted {
                        theme::t().accent
                    } else {
                        theme::t().text_muted
                    }),
            );
        }

        // Line 2: preview (fitted to two lines with "…") · badges.
        let preview: Element<'_, Message> = if chat.preview.is_empty()
            || !app.desktop_prefs().chat_previews
        {
            Space::new().height(0).width(Length::Fill).into()
        } else {
            container(ellipsis_text(
                cursus_core::humanize_simplex_links(&chat.preview).into_owned(),
                d.meta,
                theme::t().text_secondary,
                2,
            ))
            .width(Length::Fill)
            .clip(true)
            .into()
        };
        let mut badges = Row::new().spacing(6).align_y(Alignment::Center);
        if chat.settings.favorite {
            badges = badges.push(icons::star_icon(14.0, theme::t().text_muted));
        }
        if support_badge > 0 {
            badges = badges.push(
                text(format!("⚑{support_badge}"))
                    .size(d.meta)
                    .color(theme::t().always),
            );
        }
        if show_unread {
            badges = badges.push(unread_count_badge(chat.unread, muted, d));
        }
        let bottom = row![preview, badges].spacing(8).align_y(Alignment::Start);

        let content = container(
            row![
                chat_list_avatar(chat, 48.0),
                container(column![top, bottom].spacing(2))
                    .width(Length::Fill)
                    .clip(true),
            ]
            .align_y(Alignment::Center)
            .spacing(12),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: d.list_row + 2.0,
            right: 10.0,
            bottom: d.list_row + 2.0,
            left: 10.0,
        });

        // Hover paint + long-press slots without App Messages (avoids full view rebuild).
        let row = hover::chat_row(
            content,
            index,
            selected,
            app.hover_ok(),
            app.hover_slots(),
            Message::ChatRowReleased(index),
            Message::OpenChatMenu(index),
        );

        col = col.push(container(row).padding(Padding::from([0, 8])));
    }
    scroll_y(col.push(vertical().height(nav_clearance(d))))
}

fn content_pane(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    if matches!(app.nav(), NavSection::Chats | NavSection::Channels) {
        // Group / channel settings replace the conversation pane (same as contact profile).
        if app.group_settings().is_some() {
            return container(group_settings_page(app, true))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(theme::panel_style())
                .into();
        }
        // Moderator support-chat list (member threads).
        if app
            .support()
            .is_some_and(|s| matches!(s.pane, SupportPane::List))
        {
            return container(support_list_page(app, true))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(theme::panel_style())
                .into();
        }
        // Contact profile replaces the conversation pane (desktop) / is a full page (mobile).
        if app.contact_profile().is_some() {
            return container(contact_profile_page(app, true))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(theme::panel_style())
                .into();
        }
    }
    let content: Element<'_, Message> = match app.nav() {
        NavSection::Chats | NavSection::Channels => conversation_pane(app, with_back),
        NavSection::Calls => calls_detail(app),
        NavSection::Settings => settings_detail(app, with_back),
    };

    // Desktop chat: no outer inset so the composer can span the full pane
    // width and sit flush on the bottom. Title/history keep their own padding.
    let edge_to_edge = !with_back
        && matches!(app.nav(), NavSection::Chats | NavSection::Channels);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(if with_back || edge_to_edge {
            0.0
        } else {
            d.pad
        })
        .style(theme::panel_style())
        .into()
}

fn conversation_pane(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let Some(chat) = app.selected().and_then(|index| app.chats().get(index)) else {
        let channels = app.nav() == NavSection::Channels;
        return empty_detail(
            icons::nav_icon(if channels { NavIcon::Channels } else { NavIcon::Chats }, false, 36.0),
            if channels { "Your channels" } else { "Your conversations" },
            "Choose one from the list, or start something new.",
            Some((if channels { "New channel" } else { "New chat" }, Message::OpenCompose)),
            d, app.hover_ok(),
        );
    };

    let support_thread = app.support().and_then(|s| match &s.pane {
        SupportPane::Thread { title, .. } => Some(title.as_str()),
        SupportPane::List => None,
    });
    let in_support = support_thread.is_some();

    let avatar = profile_avatar_fallback(
        chat.peer_profile.image.as_deref(),
        &chat.title,
        if with_back { 32.0 } else { 36.0 },
        chat_avatar_fallback(chat),
    );
    let title_text = support_thread.unwrap_or(chat.title.as_str());
    let subtitle = if in_support {
        "Support chat"
    } else {
        chat_type_label(chat)
    };
    let identity = column![
        txt(title_text)
            .size(d.body + 1.0)
            .font(BOLD)
            .wrapping(Wrapping::None)
            .color(theme::t().text_primary),
        text(subtitle)
            .size(d.meta)
            .color(theme::t().text_muted),
    ]
    .spacing(1);
    let identity: Element<'_, Message> = if in_support {
        row![avatar, identity.width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    } else if app.selected_is_direct() {
        // Tapping the title opens the profile; no hover tile (it is a
        // header, not a row).
        mouse_area(
            row![avatar, identity.width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::OpenContactProfile)
        .into()
    } else if app.selected_is_group_or_channel() {
        // Tapping the title opens the profile; no hover tile (it is a
        // header, not a row).
        mouse_area(
            row![avatar, identity.width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::OpenGroupSettings)
        .into()
    } else {
        row![avatar, identity.width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    };

    let support_action: Element<'_, Message> = if !in_support
        && app.selected_is_group_or_channel()
        && !chat.is_channel
        && (group_support_allowed(chat.group_preferences.as_ref())
            || chat.membership_has_support_chat
            || app.support_badge_count(chat) > 0)
    {
        let badge = app.support_badge_count(chat);
        let open = if can_moderate(chat.membership_role.as_deref().unwrap_or("member")) {
            Message::OpenSupportChatList
        } else {
            Message::OpenUserSupportChat
        };
        // Only unread support messages earn a header button (the yellow
        // counter); otherwise these chats live in the group page.
        if badge > 0 {
            attention_button(icons::flag_icon(20.0, theme::on_attention()), badge, open, app.hover_ok())
        } else {
            horizontal().width(0).into()
        }
    } else {
        horizontal().width(0).into()
    };

    // Audio call, top right of a direct chat (hidden when calls are off).
    let call_action: Element<'_, Message> = if !in_support
        && app.selected_is_direct()
        && chat.pending_request_id.is_none()
        && contact_calls_allowed(chat)
    {
        composer_hit(
            icons::phone_tool(36.0, app.hover_ok()),
            36.0,
            Message::StartAudioCall(chat.id.api_id),
        )
    } else {
        horizontal().width(0).into()
    };

    let title_row: Element<'_, Message> = if with_back || in_support {
        row![
            mobile_back_btn(Message::Back, app.hover_ok()),
            identity,
            support_action,
            call_action,
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: 8.0,
        })
        .into()
    } else {
        row![
            container(identity).width(Length::Fill),
            support_action,
            call_action,
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: d.pad,
        })
        .into()
    };

    let history = message_history(app, d);

    let footer: Element<'_, Message> = if let Some(req_id) = chat.pending_request_id {
        contact_request_bar(app, req_id, d)
    } else if app.selection_mode() {
        selection_bar(app, d)
    } else if let Some(m) = app.pending_support_member() {
        column![
            pending_member_actions_bar(m.group_member_id, d, app.hover_ok()),
            composer_bar(app, d),
        ]
        .into()
    } else {
        composer_bar(app, d)
    };

    // The title bar floats over the top of the history like the composer
    // and the destinations bar: a glass card inset from the edges, the
    // history scrolling underneath it (it keeps CHAT_HEADER_ROOM at its top).
    let side = if app.layout() == LayoutMode::Mobile { 6.0 } else { 10.0 };
    let header = container(crate::glass::surface(
        container(title_row)
            .height(CHAT_HEADER_HEIGHT)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .style(theme::floating_nav_style()),
        theme::CONTROL_RADIUS,
    ))
    .padding(Padding { top: CHAT_HEADER_INSET, right: side, bottom: 0.0, left: side })
    .width(Length::Fill);
    // Taps anywhere below the header keep the composer (and the on-screen
    // keyboard) up; only the header and other panes dismiss it.
    // The composer floats over the history (which scrolls underneath it);
    // other footers (request, selection) stay plain bars below it.
    let content: Element<'_, Message> = if app.composer_floats() {
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(history)
            .push(
                container(crate::measure::height_probe(footer, Message::ComposerHeight))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::End),
            )
            .into()
    } else {
        column![history, footer].height(Length::Fill).into()
    };
    let body = crate::touch_mouse::keep_focus_zone(content);
    Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(body)
        .push(header)
        .into()
}

/// Floating chat title bar: its height and its inset from the pane's top.
const CHAT_HEADER_HEIGHT: f32 = 56.0;
const CHAT_HEADER_INSET: f32 = 8.0;
/// Room the history keeps at its top so the first message can scroll clear
/// of the floating title bar (bar + inset above + 8 px gap below, as above
/// the composer).
const CHAT_HEADER_ROOM: f32 = CHAT_HEADER_INSET + CHAT_HEADER_HEIGHT + 8.0;

/// Whether both sides allow calls (`mergedPreferences.calls`); unknown → yes.
fn contact_calls_allowed(chat: &cursus_core::ChatPreview) -> bool {
    chat.contact_merged_preferences
        .as_ref()
        .and_then(|p| p.pointer("/calls/enabled/forUser"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn selection_bar(app: &App, d: Density) -> Element<'_, Message> {
    let n = app.selected_items().len();
    container(
        row![
            text(format!("{n} selected"))
                .size(d.body)
                .color(theme::t().text_primary),
            horizontal().width(Length::Fill),
            button(text("Cancel").size(d.meta))
                .on_press(Message::ClearSelection)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(app.hover_ok())),
            button(text("Delete").size(d.meta))
                .on_press(Message::DeleteSelected)
                .padding(Padding::from([8, 12]))
                .style(theme::danger_action(app.hover_ok())),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: d.pad_tight,
        right: d.pad,
        bottom: d.pad,
        left: d.pad,
    })
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().bg_sidebar)),
        border: iced::Border {
            color: theme::t().border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn pending_member_actions_bar(
    member_id: i64,
    d: Density,
    hover: bool,
) -> Element<'static, Message> {
    container(
        row![
            icons::pending_status_icon(16.0, theme::t().text_muted),
            text("Pending review")
                .size(d.meta)
                .color(theme::t().text_muted)
                .width(Length::Fill),
            button(text("Accept").size(d.meta))
                .on_press(Message::GroupMemberAccept {
                    member_id,
                    role: "member".into(),
                })
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(hover)),
            button(text("Reject").size(d.meta).color(theme::t().danger))
                .on_press(Message::GroupMemberRemove {
                    member_id,
                    with_messages: false,
                })
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: d.pad_tight,
        right: d.pad,
        bottom: 0.0,
        left: d.pad,
    })
    .width(Length::Fill)
    .into()
}

fn composer_key_binding(
    has_suggestions: bool,
    composer_empty: bool,
    enter_sends: bool,
) -> impl Fn(iced::widget::text_editor::KeyPress) -> Option<EditorBinding<Message>> {
    move |key_press| {
        if (key_press.modifiers.control()
            && key_press.key.as_ref() == keyboard::Key::Character("v"))
            || (key_press.modifiers.shift()
                && key_press.key == keyboard::Key::Named(key::Named::Insert))
        {
            return Some(EditorBinding::Custom(Message::PasteComposer));
        }
        // `:shortcode` list: Tab / Shift+Tab cycle, Enter accepts.
        if has_suggestions {
            if matches!(
                key_press.key.as_ref(),
                keyboard::Key::Named(key::Named::Tab)
            ) {
                return Some(EditorBinding::Custom(if key_press.modifiers.shift() {
                    Message::EmojiSuggestPrev
                } else {
                    Message::EmojiSuggestNext
                }));
            }
            if matches!(
                key_press.key.as_ref(),
                keyboard::Key::Named(key::Named::Enter)
            ) && !key_press.modifiers.shift()
            {
                return Some(EditorBinding::Custom(Message::EmojiSuggestAccept));
            }
        }

        // Enter sends (Shift+Enter = newline via default Binding::Enter);
        // with "Enter sends" off, Enter is a newline and Ctrl+Enter sends.
        if matches!(
            key_press.key.as_ref(),
            keyboard::Key::Named(key::Named::Enter)
        ) {
            let m = key_press.modifiers;
            let send = if enter_sends {
                !m.shift()
            } else {
                m.control() || m.command()
            };
            if send {
                return Some(EditorBinding::Custom(Message::SendKey));
            }
            if !enter_sends {
                return Some(EditorBinding::Enter);
            }
        }

        // ArrowUp with an empty field edits the latest editable outgoing
        // message (Telegram-style). Skip when there's text so multi-line
        // caret motion still works.
        if composer_empty
            && matches!(
                key_press.key.as_ref(),
                keyboard::Key::Named(key::Named::ArrowUp)
            )
        {
            return Some(EditorBinding::Custom(Message::EditLastOutgoing));
        }

        // Keep Ctrl/Cmd chords (SelectAll, Copy, Cut, Paste) on the
        // default path — never intercept Character while modifiers are set.
        // iced 0.14 IME handles composed accents / preedit natively.
        if key_press.modifiers.command() || key_press.modifiers.control() {
            return EditorBinding::from_key_press(key_press);
        }

        // OSK: multi-codepoint `text` (emoji / composed graphemes) when
        // unmodified. Single chars fall through to defaults + IME.
        if matches!(
            key_press.status,
            iced::widget::text_editor::Status::Focused { .. }
        ) {
            if let Some(raw) = key_press.text.as_ref() {
                let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
                if cleaned.chars().count() > 1 {
                    return Some(EditorBinding::Custom(Message::ComposerInsert(cleaned)));
                }
            }
        }

        EditorBinding::from_key_press(key_press)
    }
}

fn composer_bar(app: &App, d: Density) -> Element<'_, Message> {
    // Hit target / field height — a bit tighter than before. Glyphs stay at
    // `composer_glyph` (same visual size as the previous larger buttons).
    // Radius 12 like the list header's "+"; phones get a roomier 42 px
    // field (between the 36 px "+" and other messengers' 48), desktop 36.
    let send_size = if app.layout() == LayoutMode::Mobile { 42.0 } else { 36.0 };
    let icon_size = 20.0;
    let hover_ok = app.hover_ok();

    // mouse_area + on_press: fires before OSK dismiss consumes the gesture;
    // avoids `button`'s focus-steal on the editor.
    // Paperclip inside the field, at its trailing end (as on the Mac).
    let attach = composer_hit(
        icons::centered_hit(icons::attach_tool(icon_size, hover_ok), send_size),
        send_size,
        Message::AttachFile,
    );

    if let Some(draft) = app.voice_draft() {
        return voice_draft_bar(draft, send_size, icon_size, d, hover_ok, app.scale_factor());
    }

    let recording = app.voice_recording();
    // Starting the mic, or turning a finished take into a draft: the mic is
    // inert and the field says what's happening.
    let arming = app.voice_arming() || app.voice_preparing();
    let mic = if recording {
        composer_hit(
            icons::centered_hit(
                icons::stop_icon(icon_size, Color::from_rgb(0.95, 0.35, 0.35)),
                send_size,
            ),
            send_size,
            Message::ToggleVoiceRecord,
        )
    } else if arming {
        // Capture is still probing — keep the mic, ignore presses so the
        // start tap cannot also stop a recorder that has not attached yet.
        composer_inert_hit(soft_square(icons::mic_icon(icon_size, theme::t().accent), send_size), send_size)
    } else {
        // Same soft rounded square as the list header's "+".
        composer_hit(soft_square(icons::mic_icon(icon_size, theme::t().accent), send_size), send_size, Message::ToggleVoiceRecord)
    };

    let can_send = app.pasted_image.is_some() || !app.composer().trim().is_empty();
    let send = composer_send_hit(
        icons::send_tool(send_size, can_send, hover_ok),
        send_size,
        can_send,
        Message::Send,
    );

    let placeholder = if recording {
        "Recording…"
    } else if app.voice_preparing() {
        "Preparing voice…"
    } else if arming {
        "Starting microphone…"
    } else {
        "Message"
    };

    let suggestions = app.emoji_suggestions();
    let suggest_idx = app.emoji_suggest_index();

    // Fractal/GTK: wrap + natural height; side buttons always bottom-aligned
    // (fractal#802). Always Shrink so soft-wrap cannot scroll line 1 away.
    // Keep content min_height + vertical padding ≈ send_size for the
    // single-line look. Do NOT flip padding/alignment from a wrap estimate —
    // that Center→End (and v_pad) switch mid-typing caused a visible bump
    // on both arches when the estimate crossed 1 line before/at wrap.
    let line_px = d.body * 1.3;
    let h_pad = 14.0;
    let max_content = line_px * 6.0 + 16.0;
    let line_height = iced::widget::text::LineHeight::Absolute(line_px.into());
    let v_pad = ((send_size - line_px) / 2.0).max(0.0);
    let content_min = (send_size - 2.0 * v_pad).max(line_px);

    use crate::touch_mouse::touch_as_mouse_keeping_focus;

    let editor = touch_as_mouse_keeping_focus(
        text_editor(app.composer_content())
            .id(iced::widget::Id::new("composer-input"))
            .placeholder(placeholder)
            .on_action(Message::ComposerEdit)
            .padding(Padding {
                top: v_pad,
                right: h_pad,
                bottom: v_pad,
                left: h_pad,
            })
            .size(d.body)
            .line_height(line_height)
            .height(Length::Shrink)
            .min_height(content_min)
            .max_height(max_content)
            .wrapping(Wrapping::WordOrGlyph)
            .style(theme::composer_editor())
            .key_binding(composer_key_binding(
                !suggestions.is_empty(),
                app.composer().trim().is_empty(),
                app.desktop_prefs().enter_sends,
            )),
    );
    // Right-click / long-press: Select all · Cut · Copy · Paste.
    let editor = crate::edit_menu::composer_menu(
        editor,
        app.composer_content().selection().is_some(),
        app.composer().is_empty(),
        app.hover_ok(),
    );

    // Recording: ✕ discards, ■ stops to a draft you can review, and Send
    // stops + sends in one tap (the send button must not vanish here).
    let (inner, trailing): (Element<'_, Message>, Element<'_, Message>) = if recording {
        (
            composer_hit(
                icons::centered_hit(icons::cancel_icon(icon_size, theme::t().text_secondary), send_size),
                send_size,
                Message::DiscardVoiceRecording,
            ),
            row![
                mic,
                composer_send_hit(
                    icons::send_tool(send_size, true, hover_ok),
                    send_size,
                    true,
                    Message::SendVoiceNow,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        )
    } else if can_send {
        (attach, send)
    } else {
        (attach, mic)
    };
    // A floating field with the control radius of every other button; the
    // paperclip (or ✕ while recording) sits inside it at the end. The tree
    // shape is the same in every state, so the editor keeps its focus.
    let pill = crate::glass::surface(
        container(row![editor, inner].align_y(Alignment::End))
            .width(Length::Fill)
            .style(|_| floating_card()),
        theme::SMALL_CONTROL_RADIUS,
    );
    // Always End: equal-height single-line looks centered; multi-line grows up.
    let input_row = row![pill, trailing]
        .spacing(8)
        .align_y(Alignment::End);

    // Even inset above and below the text field on every layout. The top
    // one is also the fade strip (GLASS_FADE).
    let (bar_pad_top, bar_pad_bottom) = (GLASS_FADE, 8.0);

    // Keep a fixed 3-slot column ([suggest][reply][input]) so showing the
    // `:shortcode` list doesn't shift the editor's tree index and steal focus.
    // Empty slots stay typed as Column (same tag) rather than Space.
    let suggest_slot: Element<'_, Message> = if suggestions.is_empty() {
        Column::new().width(Length::Fill).into()
    } else {
        // Fixed-height panel (always 5 rows). Growing/shrinking with the
        // filter used to shove the composer up/down on every keystroke —
        // very noticeable above the OSK on ARM.
        const EMOJI_SUGGEST_ROWS: usize = 5;
        let row_h = (d.body + 16.0).max(36.0);
        let list_h =
            row_h * EMOJI_SUGGEST_ROWS as f32 + 2.0 * (EMOJI_SUGGEST_ROWS.saturating_sub(1) as f32);
        let mut list = Column::new()
            .spacing(2)
            .width(Length::Fill)
            .height(Length::Fixed(list_h));
        if suggestions.len() < EMOJI_SUGGEST_ROWS {
            // Keep hits glued to the input; empty space sits above.
            list = list.push(vertical().height(Length::Fill));
        }
        for (i, hit) in suggestions.iter().enumerate() {
            let selected = i == suggest_idx;
            let fg = if selected {
                theme::t().accent_on
            } else {
                theme::t().text_primary
            };
            // Both glyph and shortcode fill the row height and centre their
            // own line inside it: centring the two text boxes alone left the
            // smaller shortcode riding high next to the taller emoji line.
            let label = row![
                emoji_glyph(hit.glyph)
                    .size(d.body)
                    .color(Color::WHITE)
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Center),
                txt(format!(":{}:", hit.shortcode))
                    .size(d.meta)
                    .color(fg)
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Center),
            ]
            .spacing(10)
            .height(Length::Fill)
            .align_y(Alignment::Center);
            let row = button(label)
                .on_press(Message::EmojiSuggestSelect(i))
                .padding(Padding::from([0, 10]))
                .width(Length::Fill)
                .height(row_h)
                .style(theme::emoji_suggest_row(selected, app.hover_ok()));
            list = list.push(row);
        }
        // Same surface and width as the text field it completes: the
        // field's floating card, ending where the field ends (the send /
        // mic button sits beside the field, not under the list).
        let suggest_panel = crate::glass::surface(
            container(list)
                .padding(theme::SUGGEST_INSET)
                .width(Length::Fill)
                .style(|_| floating_card()),
            theme::SMALL_CONTROL_RADIUS,
        );
        column![
            row![suggest_panel, Space::new().width(send_size + 8.0)],
            vertical().height(6)
        ]
        .width(Length::Fill)
        .into()
    };

    // Editing wins over replying; both share one context bar.
    let pasted_slot: Element<'_, Message> = if let Some(bytes) = &app.pasted_image {
        let card = row![
            iced::widget::image(iced::widget::image::Handle::from_bytes(bytes.clone()))
                .width(72)
                .height(72)
                .content_fit(ContentFit::Contain),
            text("Image ready to send")
                .size(d.meta)
                .color(theme::t().text_secondary)
                .width(Length::Fill),
            button(icons::cancel_icon(20.0, theme::t().text_secondary))
                .padding(8)
                .style(theme::icon_action(app.hover_ok()))
                .on_press(Message::DiscardComposerImage)
        ]
        .spacing(12)
        .padding(12)
        .align_y(Alignment::Center);
        column![
            crate::glass::surface(
                container(card).width(Length::Fill).style(|_| floating_card()),
                theme::SMALL_CONTROL_RADIUS,
            ),
            vertical().height(8)
        ]
        .into()
    } else {
        Column::new().into()
    };
    let reply_slot: Element<'_, Message> = if let Some(edit) = app.editing() {
        composer_context_bar(
            icons::edit_icon(16.0, theme::t().accent),
            "Editing",
            &edit.preview,
            Message::CancelEdit,
            d,
            app.hover_ok(),
        )
    } else if let Some(reply) = app.reply_to() {
        composer_context_bar(
            icons::reply_icon(16.0, theme::t().accent),
            "Replying",
            &reply.preview,
            Message::CancelReply,
            d,
            app.hover_ok(),
        )
    } else {
        Column::new().width(Length::Fill).into()
    };

    // A recording that couldn't become a draft says why (the status line
    // isn't shown in chats).
    let voice_error_slot: Element<'_, Message> = match app.voice_error() {
        Some(err) => column![crate::glass::surface(container(row![
            icons::mic_icon(16.0, theme::t().danger),
            text(err.to_string())
                .size(d.meta)
                .color(theme::t().danger)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            composer_hit(icons::cancel_icon(14.0, theme::t().text_secondary), 28.0, Message::DismissVoiceError),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding::from([6, 10])))
        .width(Length::Fill)
        .style(|_| floating_card()), theme::SMALL_CONTROL_RADIUS), vertical().height(8)]
        .into(),
        None => Column::new().width(Length::Fill).into(),
    };

    let body = column![voice_error_slot, suggest_slot, reply_slot,
        pasted_slot, input_row]
        .spacing(0)
        .width(Length::Fill);

    let bar_pad_x = if app.layout() == LayoutMode::Mobile {
        6.0
    } else {
        10.0
    };
    // No bar behind it: only the field, its button and the cards above
    // them float over the chat, each on its own glass.
    container(body)
        .padding(Padding {
            top: bar_pad_top,
            right: bar_pad_x,
            bottom: bar_pad_bottom,
            left: bar_pad_x,
        })
        .width(Length::Fill)
        .into()
}

/// A card floating over the chat above the composer field (reply / edit,
/// pasted image, voice error, emoji suggestions): opaque, shadowed.
fn floating_card() -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(theme::glass_fill())),
        border: iced::Border {
            color: theme::glass_edge(),
            width: 1.0,
            radius: theme::SMALL_CONTROL_RADIUS.into(),
        },
        shadow: floating_shadow(),
        ..container::Style::default()
    }
}

/// Height of the fade above the floating composer.
/// Empty inset above floating bars (composer, destinations bar): the whole
/// gap between them and the content resting above. Nothing is painted in
/// it; only the bars themselves are glass.
pub(crate) const GLASS_FADE: f32 = 8.0;

/// Soft shadow under floating controls (composer field and its button).
fn floating_shadow() -> iced::Shadow {
    theme::glass_shadow()
}

/// A glyph on the soft-accent rounded square (list header "+", composer mic).
fn soft_square<'a>(icon: Element<'a, Message>, size: f32) -> Element<'a, Message> {
    let radius = theme::SMALL_CONTROL_RADIUS;
    // Glass like the field beside it (blurred backdrop + tint), with the
    // soft accent over the tint.
    let accent = container(icon)
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(theme::t().accent_soft)),
            border: iced::Border { radius: radius.into(), ..Default::default() },
            ..container::Style::default()
        });
    let glass = container(accent).style(move |_| container::Style {
        background: Some(iced::Background::Color(theme::glass_fill())),
        border: iced::Border { radius: radius.into(), width: 1.0, color: theme::glass_edge() },
        shadow: floating_shadow(),
        ..container::Style::default()
    });
    crate::glass::surface(glass, radius)
}

/// "Editing" / "Replying" strip above the composer: icon, label, preview and ✕.
/// Returned as a Column so the composer's slot keeps the same widget type.
fn composer_context_bar(
    icon: Element<'static, Message>,
    label: &'static str,
    preview: &str,
    on_cancel: Message,
    d: Density,
    hover: bool,
) -> Element<'static, Message> {
    let preview: Element<'static, Message> = if cursus_core::is_short_emoji(preview) {
        emoji_glyph(preview.to_string())
            .size(d.body)
            .color(Color::WHITE)
            .into()
    } else {
        txt(preview.to_string())
            .size(d.meta)
            .color(theme::t().text_secondary)
            .into()
    };
    let bar = container(
        row![
            icon,
            column![text(label).size(d.meta).color(theme::t().accent), preview]
                .spacing(2)
                .width(Length::Fill),
            button(txt("✕").size(d.meta).color(theme::t().text_muted))
                .on_press(on_cancel)
                .padding(Padding::from([4, 8]))
                .style(theme::secondary_action(hover)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 10]))
    .width(Length::Fill)
    .style(|_| floating_card());
    let bar = crate::glass::surface(bar, theme::SMALL_CONTROL_RADIUS);
    column![bar, vertical().height(8)].width(Length::Fill).into()
}

fn contact_request_bar(app: &App, req_id: i64, d: Density) -> Element<'_, Message> {
    let member = app.selected().and_then(|i| app.chats().get(i)).is_some_and(|chat| chat.member_request);
    container(
        column![
            text(if member { "Direct message request" } else { "Incoming contact request" })
                .size(d.body)
                .color(theme::t().text_primary),
            text(if member { "A group member wants to message you directly. Accept or decline." } else { "Someone connected via your address. Accept to chat, or reject." })
                .size(d.meta)
                .color(theme::t().text_secondary),
            row![
                button(text("Accept").size(d.body))
                    .on_press_maybe((!app.contact_request_busy).then_some(Message::AcceptContactRequest { req_id, member }))
                    .padding(Padding::from([12, 16]))
                    .style(theme::primary_action(app.hover_ok())),
                button(text("Reject").size(d.body))
                    .on_press_maybe((!app.contact_request_busy).then_some(Message::RejectContactRequest { req_id, member }))
                    .padding(Padding::from([12, 16]))
                    .style(theme::secondary_action(app.hover_ok())),
            ]
            .spacing(8),
        ]
        .spacing(8),
    )
    .padding(Padding {
        top: d.pad_tight,
        right: d.pad,
        bottom: d.pad,
        left: d.pad,
    })
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().bg_elevated)),
        border: iced::Border {
            color: theme::t().border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn message_history<'a>(app: &'a App, d: Density) -> Element<'a, Message> {
    let messages = app.messages();
    if messages.is_empty() {
        return container(
            text("No messages yet. Say hello.")
                .size(d.meta)
                .color(theme::t().text_muted),
        )
        .padding(d.pad)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let jump_visible = app.show_jump_to_latest();
    // The bubble column is rebuilt only when something it shows changes
    // (see `history_key`): scroll-fling ticks, pointer/hover messages, typing
    // in the composer, chat-list refreshes… reuse the cached widgets.
    let col = iced::widget::lazy(history_key(app, d), move |_| history_column(app, d));
    message_history_shell(col, app, d, jump_visible)
}

/// Fingerprint of everything [`history_column`] reads. When it is unchanged
/// the cached column is reused; anything that alters what a bubble shows must
/// be hashed here. Messages hash through their derived `Hash` (every field),
/// and per-file media state is hashed by the same lookups the bubbles make,
/// so the key cannot drift from the view.
fn history_key(app: &App, d: Density) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let f = |h: &mut std::collections::hash_map::DefaultHasher, v: f32| v.to_bits().hash(h);

    // Layout / appearance inputs.
    for v in [d.body, d.meta, d.title, d.pad, d.pad_tight, app.window_width()] {
        f(&mut h, v);
    }
    app.layout().hash(&mut h);
    // Bottom room for the floating composer.
    f(&mut h, if app.composer_floats() { app.composer_inset() } else { 0.0 });
    theme::is_light_active().hash(&mut h);
    app.hover_ok().hash(&mut h);
    // "Today" / "Yesterday" dividers roll over at midnight.
    chrono::Local::now().date_naive().hash(&mut h);
    clock().hash(&mut h);

    // Open chat (channel header name/avatar, member-header mode).
    if let Some(chat) = app.selected().and_then(|i| app.chats().get(i)) {
        chat.id.hash(&mut h);
        chat.title.hash(&mut h);
        chat.peer_profile.image.hash(&mut h);
        chat.is_channel.hash(&mut h);
    }

    // Media blur.
    app.desktop_prefs().blur_media.hash(&mut h);

    // Selection, focus and reveal state.
    app.new_messages_from().hash(&mut h);
    app.selection_mode().hash(&mut h);
    app.focus_item().hash(&mut h);
    let mut picked: Vec<i64> = app.selected_items().iter().copied().collect();
    picked.sort_unstable();
    picked.hash(&mut h);

    // The messages themselves, hashed once per change (see `History`).
    app.history_fingerprint().hash(&mut h);
    app.history_has_earlier().hash(&mut h);
    for msg in app.messages() {
        app.is_deleted_revealed(msg.item_id).hash(&mut h);
        for i in 0..msg.segments.len() {
            app.is_secret_revealed(msg.item_id, i).hash(&mut h);
        }
        if let Some(sender) = msg.sender.as_ref() {
            app.member_display_name(sender).hash(&mut h);
        }
        if let Some(file) = msg.file.as_ref() {
            let plain = app.plaintext_path_for(file);
            plain.hash(&mut h);
            if let Some(p) = plain.as_ref() {
                p.is_file().hash(&mut h);
                app.image_preview_for(p, 0, 0).hash(&mut h);
                app.blurred_preview_for(p).hash(&mut h);
                app.media_revealed(msg.item_id).hash(&mut h);
                if let Some(peaks) = app.waveform_for(p) {
                    for v in peaks {
                        f(&mut h, *v);
                    }
                }
            }
            if let Some(local) = file.local_path.as_ref() {
                local.is_file().hash(&mut h);
                if let Some(peaks) = app.waveform_for(local) {
                    peaks.len().hash(&mut h);
                }
            }
            app.voice_playback_progress(file.file_id)
                .map(f32::to_bits)
                .hash(&mut h);
        }
    }
    h.finish()
}

/// Every message bubble, day divider and event row of the open chat.
/// Owns all its data (`'static`) so [`iced::widget::lazy`] can cache it.
fn history_column(app: &App, d: Density) -> Element<'static, Message> {
    let messages = app.messages();
    let show_members = app.selected_shows_member_headers();
    let show_channel = app.selected_is_channel();
    // Room at the end for the composer floating over the history.
    let inset = if app.composer_floats() { app.composer_inset() } else { 0.0 };
    // No extra bottom padding: the composer's 8 px top inset is the whole
    // gap under the last message, as in WhatsApp / Telegram.
    let mut col = Column::new().spacing(0).padding(Padding {
        top: CHAT_HEADER_ROOM,
        right: d.pad,
        bottom: if inset > 0.0 { inset } else { 10.0 },
        left: d.pad,
    });
    col = col.width(Length::Fill);
    let mut prev_sender: Option<i64> = None;
    let mut prev_channel_block = false;
    let mut prev_day: Option<chrono::NaiveDate> = None;
    if app.history_has_earlier() {
        col = col.push(
            container(
                button(text("Load earlier messages").size(d.meta).color(theme::t().accent))
                    .padding(Padding::from([6, 14]))
                    .style(theme::secondary_action(app.hover_ok()))
                    .on_press(Message::LoadEarlierMessages),
            )
            .width(Length::Fill)
            .padding(Padding::from([6, 0]))
            .center_x(Length::Fill),
        );
    }
    let msgs: Vec<&ChatMessage> = messages.iter().collect();
    for (i, msg) in msgs.iter().copied().enumerate() {
        // Day divider whenever the local date changes.
        let at = divider_time(msg);
        if let Some(at) = at.as_ref() {
            if prev_day != Some(at.date_naive()) {
                prev_day = Some(at.date_naive());
                col = col.push(day_divider(at, d));
                prev_sender = None;
                prev_channel_block = false;
            }
        }
        if app.new_messages_from() == Some(msg.item_id) {
            col = col.push(new_messages_divider(d));
            prev_sender = None;
            prev_channel_block = false;
        }
        if msg.group_invitation.is_some() {
            prev_sender = None;
            prev_channel_block = false;
            col = col.push(vertical().height(10));
            col = col.push(group_invitation_card(app, msg, d));
            continue;
        }
        if msg.is_announcement {
            prev_sender = None;
            prev_channel_block = false;
            col = col.push(vertical().height(6));
            col = col.push(announcement_row(msg, d));
            continue;
        }
        let joined_prev = i > 0 && cursus_core::runs::continues_run(msgs[i - 1], msg);
        let joined_next = i + 1 < msgs.len() && cursus_core::runs::continues_run(msg, msgs[i + 1]);
        let show_header = if show_members {
            !msg.outgoing
                && msg.sender.as_ref().is_some_and(|s| {
                    prev_sender
                        .map(|id| id != s.group_member_id)
                        .unwrap_or(true)
                })
        } else if show_channel {
            // Official: channelRcv shows channel avatar/name; collapse consecutive.
            !msg.outgoing && !prev_channel_block
        } else {
            false
        };
        if msg.outgoing {
            prev_sender = None;
            prev_channel_block = false;
        } else if show_members {
            if let Some(s) = msg.sender.as_ref() {
                prev_sender = Some(s.group_member_id);
            }
            prev_channel_block = false;
        } else if show_channel {
            prev_channel_block = true;
            prev_sender = None;
        }
        // Tight inside a run from one sender, airy between runs.
        col = col.push(vertical().height(if joined_prev { 2 } else { 10 }));
        col = col.push(message_bubble(
            app,
            msg,
            d,
            show_header,
            BubbleRun { joined_next },
        ));
    }
    col.into()
}

/// Scrollable + kinetic fling + jump-to-latest FAB around the (cached) column.
fn message_history_shell<'a>(
    col: impl Into<Element<'a, Message>>,
    app: &'a App,
    d: Density,
    jump_visible: bool,
) -> Element<'a, Message> {
    // Do NOT append spacer when the jump FAB appears — that grows content at
    // Anchor::End and leaves an empty gap under the last bubble while scrolling.
    // The FAB is a Stack overlay; slight overlap with the last row is fine.
    //
    // Anchor::End must live on the Scrollbar inside direction() — calling
    // anchor_bottom() before direction() is overwritten by Scrollbar::default().
    // No on_scroll: every wheel tick was publishing a Message and rebuilding
    // the whole chat view (major scroll lag).
    let history = crate::kinetic::track_end_anchored(
        "chat-history",
        scrollable(crate::touch_content::content(col))
            .id(iced::widget::Id::new("chat-history"))
            .height(Length::Fill)
            .direction(Direction::Vertical(no_bar().anchor(Anchor::End))),

    );

    // Telegram-style “jump to latest” FAB when scrolled up into history.
    let jump: Element<'_, Message> = if jump_visible {
        let hit = 44.0;
        container(
            button(icons::centered_hit(
                icons::chevron_down(18.0),
                hit,
            ))
            .on_press(Message::ScrollToLatest)
            .width(hit)
            .height(hit)
            .padding(0)
            .style(theme::icon_action(app.hover_ok())),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(Padding {
            top: 0.0,
            right: d.pad,
            bottom: d.pad + if app.composer_floats() { app.composer_inset() } else { 0.0 },
            left: 0.0,
        })
        .into()
    } else {
        vertical().height(0).into()
    };

    container(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(history)
            .push(jump),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .clip(true)
    .into()
}

/// Group invitation in a direct chat — log-style copy + Accept / Reject when pending.
fn group_invitation_card<'a>(
    app: &'a App,
    msg: &'a ChatMessage,
    d: Density,
) -> Element<'static, Message> {
    let Some(inv) = msg.group_invitation.as_ref() else {
        return announcement_row(msg, d);
    };
    let hover = app.hover_ok();
    let status = inv.status.to_ascii_lowercase();
    let detail = match status.as_str() {
        "accepted" => "Joined".to_string(),
        "rejected" => "Rejected".to_string(),
        "expired" => "Expired".to_string(),
        _ if msg.outgoing => format!("Invited as {}", inv.member_role),
        _ => format!("Invited as {}", inv.member_role),
    };
    let mut body = Column::new()
        .spacing(6)
        .width(Length::Fill)
        .push(
            text(inv.group_name.clone())
                .size(d.body)
                .color(theme::t().text_primary),
        )
        .push(
            text(detail)
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    if msg.group_invite_pending() {
        body = body.push(
            row![
                button(text("Accept").size(d.meta))
                    .on_press(Message::AcceptGroupInvitation(inv.group_id))
                    .padding(Padding::from([8, 12]))
                    .style(theme::primary_action(hover)),
                button(text("Reject").size(d.meta))
                    .on_press(Message::RejectGroupInvitation(inv.group_id))
                    .padding(Padding::from([8, 12]))
                    .style(theme::secondary_action(hover)),
            ]
            .spacing(8),
        );
    }
    container(body)
        .padding(Padding::from([10, 12]))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_elevated)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: theme::CONTROL_RADIUS.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// Telegram / SimpleX-style centered protocol line (left group, profile update, …).
fn announcement_row(msg: &ChatMessage, d: Density) -> Element<'static, Message> {
    let time = format_bubble_time(msg.created_at.as_deref()).unwrap_or_default();
    let line = if time.is_empty() {
        msg.text.clone()
    } else {
        format!("{}  {time}", msg.text)
    };
    container(
        text(line)
            .size(d.meta)
            .color(theme::t().text_muted)
            .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([6, 16]))
    .center_x(Length::Fill)
    .into()
}

/// Position of a bubble inside a run of consecutive messages from one sender.
#[derive(Debug, Clone, Copy, Default)]
struct BubbleRun {
    /// The next message continues this run (same sender, close in time).
    joined_next: bool,
}

/// Temporary accent line + pill above the first message that just arrived.
fn new_messages_divider(d: Density) -> Element<'static, Message> {
    let line = || {
        container(Space::new().width(Length::Fill).height(1))
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color {
                    a: 0.45,
                    ..theme::t().accent
                })),
                ..container::Style::default()
            })
    };
    let pill = container(
        txt("New messages".to_string())
            .size(d.meta)
            .font(BOLD)
            .color(theme::t().accent),
    )
    .padding(Padding::from([3, 10]))
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().accent_soft)),
        border: iced::Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..container::Style::default()
    });
    container(
        row![line(), pill, line()]
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 12.0,
        right: 0.0,
        bottom: 2.0,
        left: 0.0,
    })
    .into()
}

/// Centered "Today" / "Yesterday" / date pill between days.
fn day_divider(at: &chrono::DateTime<chrono::Local>, d: Density) -> Element<'static, Message> {
    container(
        container(
            txt(format_day_label(at))
                .size(d.meta)
                .font(BOLD)
                .color(theme::t().text_secondary),
        )
        .padding(Padding::from([3, 10]))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_elevated)),
            border: iced::Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..container::Style::default()
        }),
    )
    .width(Length::Fill)
    .center_x(Length::Fill)
    .padding(Padding {
        top: 12.0,
        right: 0.0,
        bottom: 2.0,
        left: 0.0,
    })
    .into()
}

fn message_bubble<'a>(
    app: &'a App,
    msg: &'a ChatMessage,
    d: Density,
    show_member_header: bool,
    run: BubbleRun,
) -> Element<'static, Message> {
    let align = if msg.outgoing {
        Alignment::End
    } else {
        Alignment::Start
    };
    let revealed = app.is_deleted_revealed(msg.item_id);
    let can_reveal = msg.deleted.is_some_and(cursus_core::DeletedState::can_reveal);
    let show_as_deleted = msg.is_deleted() && !(can_reveal && revealed);
    let emoji_only = !show_as_deleted && msg.is_emoji_only();
    let bg = if emoji_only {
        Color::TRANSPARENT
    } else if msg.outgoing {
        theme::t().bubble_out
    } else {
        theme::t().bubble_in
    };

    let group_incoming =
        !msg.outgoing && msg.sender.is_some() && app.selected_shows_member_headers();
    let channel_incoming = !msg.outgoing && app.selected_is_channel();

    // Text with its stamp comes last and spans the bubble (see bubble_body).
    let mut body = crate::bubble_body::BubbleBody::new(4.0);
    // Sender name (and role) as the bubble's first line, like the macOS app.
    if show_member_header && (group_incoming || channel_incoming) {
        body = body.push(sender_line(app, msg, channel_incoming, d));
    }
    let mut voice_owns_meta = false;
    let mut media_flush = false;

    if show_as_deleted {
        // Shrink-wrap: chip + time/receipt on one row (no Fill — that stretched bubbles).
        body = body.push(
            row![deleted_mark_chip(msg, d), bubble_time_meta(msg, d)]
                .spacing(10)
                .align_y(Alignment::Center),
        );
        voice_owns_meta = true;
    } else if msg.is_deleted() {
        body = body.push(deleted_mark_chip(msg, d));
    }

    if show_as_deleted {
        // Collapsed: chip + meta already pushed.
    } else if let Some(quote) = msg.quote.as_ref() {
        body = body.push(quote_preview(quote, d));
    }

    if show_as_deleted {
        // Collapsed placeholder — no original content.
    } else if let Some(file) = msg.file.as_ref() {
        let state = effective_file_state(file.state, msg.outgoing, msg.delivery);
        let is_visual = matches!(file.kind, FileKind::Image | FileKind::Video)
            || media::is_image_name(&file.file_name)
            || media::is_video_name(&file.file_name);
        if matches!(file.kind, FileKind::Voice) {
            voice_owns_meta = true;
            body = body.push(voice_waveform_row(app, msg, file, state, d));
        } else if is_visual && matches!(state, FileTransferState::Complete) {
            if let Some(preview) = visual_media_preview(app, msg, file, state, d) {
                media_flush = true;
                // Stamp floats on the preview; size/status live in Message info.
                body = body.push(preview);
            } else {
                body = body.push(file_title_row(app, msg, file, state, d));
                body = body.push(attachment_status_footer(msg, file, state, d, false));
            }
            voice_owns_meta = true;
        } else {
            body = body.push(file_title_row(app, msg, file, state, d));
            if matches!(state, FileTransferState::InProgress) {
                body = body.push(transfer_progress_bar(file));
            }
            body = body.push(attachment_status_footer(msg, file, state, d, false));
            voice_owns_meta = true;
        }
    } else if emoji_only {
        body = body.stretch_last(true);
        body = body.push(bubble_content_with_time(
            emoji_message_text(&msg.text, Some(stamp_reserve_span(msg, d, emoji_text_size(&msg.text)))),
            msg,
            d,
        ));
        voice_owns_meta = true;
    } else {
        body = body.stretch_last(true);
        body = body.push(bubble_content_with_time(
            message_text_content(app, msg, d, Some(stamp_reserve_span(msg, d, d.body))),
            msg,
            d,
        ));
        voice_owns_meta = true;
    }

    // Fallback footer — keep shrink-wrapped (no Fill).
    if !voice_owns_meta {
        body = body.push(bubble_time_meta(msg, d));
    }

    let pad = if emoji_only {
        Padding::from([6, 8])
    } else if media_flush {
        // Concentric 16pt bubble / 12pt preview with an even 4pt bezel.
        Padding::from(4.0)
    } else {
        Padding::from([8, 12])
    };
    let focused = app.focus_item() == Some(msg.item_id);
    let selected = app.selection_mode() && app.selected_items().contains(&msg.item_id);
    let border_w = if focused || selected { 2.0 } else { 0.0 };
    // Rounded everywhere except the sender's side (right for sent, left for
    // received): its top corner is always small, its bottom corner too on
    // every bubble of a run but the last (`cursus_core::runs`).
    let r: f32 = if media_flush { 16.0 } else { 18.0 };
    const JOIN: f32 = 6.0;
    let side_top = JOIN;
    let side_bottom = if run.joined_next { JOIN } else { r };
    let radius = if msg.outgoing {
        iced::border::Radius {
            top_left: r,
            top_right: side_top,
            bottom_right: side_bottom,
            bottom_left: r,
        }
    } else {
        iced::border::Radius {
            top_left: side_top,
            top_right: r,
            bottom_right: r,
            bottom_left: side_bottom,
        }
    };
    let light = theme::is_light_active();
    let bubble = container(body)
        .padding(pad)
        .max_width(520)
        .style(move |_| container::Style {
            background: if emoji_only {
                None
            } else {
                Some(iced::Background::Color(bg))
            },
            border: iced::Border {
                color: theme::t().accent,
                width: border_w,
                radius,
            },
            // Light mode: a whisper of shadow lifts white bubbles off the grey.
            shadow: if light && !emoji_only {
                iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
                    offset: iced::Vector::new(0.0, 1.0),
                    blur_radius: 2.0,
                }
            } else {
                iced::Shadow::default()
            },
            ..container::Style::default()
        });

    const MEMBER_AVATAR: f32 = 32.0;
    const MEMBER_AVATAR_GAP: f32 = 8.0;
    let bubble_with_sender: Element<'_, Message> = if group_incoming {
        let sender = msg.sender.as_ref().unwrap();
        let sender_name = app.member_display_name(sender);
        let avatar: Element<'_, Message> = if show_member_header {
            profile_avatar(sender.image.as_deref(), &sender_name, MEMBER_AVATAR)
        } else {
            horizontal().width(MEMBER_AVATAR).height(MEMBER_AVATAR).into()
        };
        // Avatar at the top of the sender's first message.
        row![avatar, bubble]
            .spacing(MEMBER_AVATAR_GAP)
            .align_y(Alignment::Start)
            .into()
    } else if channel_incoming {
        // Official: channel avatar + chatViewName + "channel" role label.
        let chat = app.selected().and_then(|i| app.chats().get(i));
        let name = chat.map(|c| c.title.as_str()).unwrap_or("Channel");
        let image = chat.and_then(|c| c.peer_profile.image.as_deref());
        let avatar: Element<'_, Message> = if show_member_header {
            profile_avatar(image, name, MEMBER_AVATAR)
        } else {
            horizontal().width(MEMBER_AVATAR).height(MEMBER_AVATAR).into()
        };
        row![avatar, bubble]
            .spacing(MEMBER_AVATAR_GAP)
            .align_y(Alignment::Start)
            .into()
    } else {
        bubble.into()
    };

    let stack = Column::new()
        .spacing(4)
        .width(Length::Fill)
        .align_x(align)
        .push(bubble_with_sender);

    // Touch: long-press opens the menu. Mouse: right-click.
    let bubble_area = hover::track_msg(
        stack,
        msg.item_id,
        app.hover_slots(),
        Message::MsgBubbleReleased(msg.item_id),
        Message::OpenMessageMenu(msg.item_id),
    );

    let mut col = Column::new()
        .spacing(4)
        .width(Length::Fill)
        .align_x(align)
        .push(bubble_area);

    if !show_as_deleted && !msg.reactions.is_empty() {
        // Group / channel bubbles sit right of the sender's avatar: indent the
        // chips by that column so they line up under the bubble, not the avatar.
        let gutter = if group_incoming || channel_incoming {
            MEMBER_AVATAR + MEMBER_AVATAR_GAP
        } else {
            0.0
        };
        col = col.push(container(reaction_chips(app, msg, d)).padding(Padding {
            left: gutter,
            ..Padding::ZERO
        }));
    }

    col.into()
}

/// First line inside an incoming group / channel bubble: the sender (or the
/// channel) in accent, then its role when above member ("channel" for channels).
fn sender_line(app: &App, msg: &ChatMessage, channel: bool, d: Density) -> Element<'static, Message> {
    let (name, role) = if channel {
        let chat = app.selected().and_then(|i| app.chats().get(i));
        (chat.map(|c| c.title.clone()).unwrap_or_else(|| "Channel".into()), Some("channel".to_string()))
    } else {
        let sender = msg.sender.as_ref();
        let name = sender.map(|s| app.member_display_name(s)).unwrap_or_default();
        let role = sender
            .filter(|s| group_role_rank(&s.role) > group_role_rank("member"))
            .map(|s| s.role.clone());
        (name, role)
    };
    let mut line = Row::new().spacing(5).align_y(Alignment::Center).push(
        text(name).size(d.meta).font(BOLD).color(theme::t().accent),
    );
    // The role as its icon only (owner / admin / moderator; channels: none).
    if let Some(role) = role.filter(|_| !channel) {
        line = line.push(icons::role_icon(&role, (d.meta + 2.0).round(), theme::t().text_muted));
    }
    line.into()
}

fn deleted_mark_chip(msg: &ChatMessage, d: Density) -> Element<'static, Message> {
    let label = msg.deleted.map_or("deleted", cursus_core::DeletedState::label);
    let glyph = match msg.deleted {
        Some(cursus_core::DeletedState::Hidden | cursus_core::DeletedState::BlockedByAdmin) => {
            icons::block_prohibited_icon(14.0, theme::t().text_muted)
        }
        _ => icons::trash_icon(14.0, theme::t().text_muted),
    };
    row![
        glyph,
        txt(label)
            .size(d.meta)
            .color(theme::t().text_muted)
            .font(Font {
                family: font::Family::Name("Noto Sans"),
                weight: font::Weight::Normal,
                stretch: font::Stretch::Normal,
                style: font::Style::Italic,
            }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn quote_preview(quote: &cursus_core::MessageQuote, d: Density) -> Element<'static, Message> {
    let preview = truncate_graphemes(&cursus_core::humanize_simplex_links(&quote.text), 120);
    let glyph = if cursus_core::is_short_emoji(&quote.text) {
        emoji_glyph(preview.clone())
            .size(d.meta)
            .color(theme::t().text_primary)
    } else {
        txt(preview)
            .size(d.meta)
            .color(theme::t().text_secondary)
    };
    let bar = container(
        row![
            container(Space::new().width(Length::Fixed(3.0)).height(Length::Fixed(28.0))).style(|_| {
                container::Style {
                    background: Some(iced::Background::Color(theme::t().accent)),
                    border: iced::Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                    ..container::Style::default()
                }
            }),
            column![
                text(if quote.outgoing { "You" } else { "Reply" })
                    .size(d.meta)
                    .color(theme::t().accent),
                glyph,
            ]
            .spacing(2),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([6, 8]))
    .style(|_| container::Style {
        background: Some(iced::Background::Color(Color::from_rgba(
            0.0, 0.0, 0.0, 0.22,
        ))),
        border: iced::Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..container::Style::default()
    });

    if let Some(item_id) = quote.item_id {
        // Release-only so swiping over a quote still scrolls the history.
        mouse_area(bar)
            .on_release(Message::JumpToQuoted(item_id))
            .interaction(mouse::Interaction::Pointer)
            .into()
    } else {
        bar.into()
    }
}

/// First `max_chars` user-visible characters (grapheme clusters) plus "…".
/// Never splits an emoji sequence, flag or combining mark.
pub(crate) fn truncate_graphemes(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, g) in s.graphemes(true).enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push_str(g);
    }
    out
}

fn reaction_chips<'a>(app: &'a App, msg: &'a ChatMessage, d: Density) -> Element<'static, Message> {
    let group = app.selected_is_group();
    let mut row = Row::new().spacing(4).align_y(Alignment::Center);
    for r in &msg.reactions {
        let mut chip_body = Row::new()
            .spacing(4)
            .align_y(Alignment::Center)
            .push(emoji_glyph(r.display_glyph()).size(d.body).color(Color::WHITE));
        if r.total > 1 {
            chip_body = chip_body.push(
                txt(format!("{}", r.total))
                    .size(d.meta)
                    .font(BOLD)
                    .color(if r.user_reacted {
                        theme::t().accent
                    } else {
                        theme::t().text_secondary
                    }),
            );
        }
        // Yours: soft accent with an accent ring; others: quiet pill.
        let selected = r.user_reacted;
        let chip_face = container(chip_body).padding(Padding::from([3, 8])).style(
            move |_| {
                let c = theme::t();
                container::Style {
                    background: Some(iced::Background::Color(if selected {
                        c.accent_soft
                    } else {
                        c.bg_elevated
                    })),
                    border: iced::Border {
                        color: if selected { c.accent } else { Color::TRANSPARENT },
                        width: if selected { 1.0 } else { 0.0 },
                        radius: 999.0.into(),
                    },
                    ..container::Style::default()
                }
            },
        );
        // Release-only: press would capture the finger and block history scroll.
        let chip: Element<'_, Message> = if group {
            hover::track_reaction(
                chip_face,
                msg.item_id,
                r.emoji.clone(),
                app.hover_slots(),
                Message::ToggleReaction {
                    item_id: msg.item_id,
                    emoji: r.emoji.clone(),
                },
                Message::ShowReactionMembers {
                    item_id: msg.item_id,
                    emoji: r.emoji.clone(),
                },
            )
        } else {
            mouse_area(chip_face)
                .on_release(Message::ToggleReaction {
                    item_id: msg.item_id,
                    emoji: r.emoji.clone(),
                })
                .interaction(mouse::Interaction::Pointer)
                .into()
        };
        row = row.push(chip);
    }
    container(row).into()
}

fn reaction_picker_row(item_id: i64, d: Density, hover: bool) -> Element<'static, Message> {
    // One line on every width: the reactions share the sheet's width
    // equally instead of wrapping onto a second row on phones.
    let mut row = Row::new().spacing(2).align_y(Alignment::Center).width(Length::Fill);
    for emoji in KNOWN_REACTIONS {
        let glyph = if *emoji == "❤" { "❤️" } else { *emoji };
        row = row.push(
            button(
                container(emoji_glyph(glyph.to_string()).size(d.body).color(Color::WHITE))
                    .center_x(Length::Fill),
            )
            .on_press(Message::ToggleReaction {
                item_id,
                emoji: (*emoji).to_string(),
            })
            .padding(Padding::from([8, 0]))
            .width(Length::Fill)
            .style(theme::reaction_chip(false, hover)),
        );
    }
    container(row).max_width(8.0 * 48.0).into()
}

/// Big emoji; `reserve` is the trailing stamp room (see [`stamp_reserve`]).
fn emoji_message_text(
    raw: &str,
    reserve: Option<iced::widget::text::Span<'static, Message>>,
) -> Element<'static, Message> {
    let s = raw.trim().to_string();
    let size = emoji_text_size(&s);
    let Some(reserve) = reserve else {
        return emoji_glyph(s).size(size).color(Color::WHITE).into();
    };
    rich_text(vec![span(s).font(EMOJI_FONT).size(size), reserve])
        .size(size)
        .color(Color::WHITE)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

/// Formatted message body; `reserve` is the trailing stamp room.
fn message_text_content(
    app: &App,
    msg: &ChatMessage,
    d: Density,
    reserve: Option<iced::widget::text::Span<'static, Message>>,
) -> Element<'static, Message> {
    if msg.segments.is_empty() && reserve.is_none() {
        return txt(msg.text.clone())
            .size(d.body)
            .color(theme::t().text_primary)
            .wrapping(Wrapping::WordOrGlyph)
            .into();
    }
    if msg.segments.is_empty() {
        return plain_text_body(&msg.text, d, reserve);
    }
    let item_id = msg.item_id;
    let mut spans: Vec<_> = msg
        .segments
        .iter()
        .enumerate()
        .map(|(index, seg)| {
            let revealed = app.is_secret_revealed(item_id, index);
            styled_span(seg, d.body, item_id, index, revealed)
        })
        .collect();
    spans.extend(reserve);
    let rich = rich_text(spans)
        .on_link_click(|msg: Message| msg)
        .size(d.body)
        .color(theme::t().text_primary)
        .wrapping(Wrapping::WordOrGlyph);
    if msg.segments.iter().any(|s| s.link.is_some()) {
        crate::tap::tap_links(rich)
    } else {
        rich.into()
    }
}

fn profile_rich<'a>(
    raw: &str,
    size: f32,
    color: Color,
    bold: bool,
) -> iced::widget::text::Rich<'a, Message, Message> {
    let spans = cursus_core::profile_text(raw)
        .into_iter()
        .map(|mut segment| {
            if segment.style == TextStyle::Secret {
                segment.style = TextStyle::Snippet;
            }
            let styled = styled_span(&segment, size, -1, 0, true);
            if matches!(
                segment.style,
                TextStyle::Plain | TextStyle::Bold | TextStyle::Italic | TextStyle::Strike
            ) {
                styled.color(color)
            } else {
                styled
            }
        })
        .collect::<Vec<_>>();
    rich_text(spans)
        .size(size)
        .color(color)
        .font(if bold { BOLD } else { Font::DEFAULT })
        .wrapping(Wrapping::WordOrGlyph)
}

/// Unformatted body followed by the stamp reservation.
fn plain_text_body(
    raw: &str,
    d: Density,
    reserve: Option<iced::widget::text::Span<'static, Message>>,
) -> Element<'static, Message> {
    let mut spans = vec![span(raw.to_string()).color(theme::t().text_primary)];
    spans.extend(reserve);
    rich_text(spans)
        .size(d.body)
        .color(theme::t().text_primary)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn styled_span(
    seg: &TextSegment,
    body: f32,
    item_id: i64,
    index: usize,
    secret_revealed: bool,
) -> iced::widget::text::Span<'static, Message> {
    let display = match seg.style {
        TextStyle::Secret if !secret_revealed => "•".repeat(seg.text.chars().count().max(1)),
        _ => seg.text.clone(),
    };
    let mut s = span(display).color(theme::t().text_primary);

    match seg.style {
        TextStyle::Plain => {}
        TextStyle::Bold => {
            s = s.font(Font {
                weight: font::Weight::Bold,
                ..Font::DEFAULT
            });
        }
        TextStyle::Italic => {
            s = s.font(Font {
                style: font::Style::Italic,
                ..Font::DEFAULT
            });
        }
        TextStyle::Strike => {
            s = s.strikethrough(true);
        }
        TextStyle::Snippet | TextStyle::Command => {
            s = s
                .font(Font::MONOSPACE)
                .color(theme::t().accent)
                .background(iced::Background::Color(theme::t().bg_app));
        }
        TextStyle::Secret => {
            s = s
                .color(if secret_revealed {
                    theme::t().text_primary
                } else {
                    theme::t().text_muted
                })
                .background(iced::Background::Color(theme::t().accent_soft))
                .link(Message::ToggleSecretReveal { item_id, index });
        }
        TextStyle::Small => {
            s = s
                .size((body * 0.85).round())
                .color(theme::t().text_secondary);
        }
        TextStyle::Colored { color } => {
            s = s.color(format_color(color));
        }
        TextStyle::Uri | TextStyle::Email | TextStyle::Phone => {
            s = s.color(theme::t().accent).underline(true);
        }
        TextStyle::Mention => {
            s = s.font(Font {
                weight: font::Weight::Medium,
                ..Font::DEFAULT
            });
        }
    }

    if let Some(link) = seg.link.as_ref() {
        // SimpleX links come as the description plus an italic "(via …)"
        // part: both look like the link they are.
        if seg.simplex.is_some() {
            s = s.color(theme::t().accent).underline(true);
        }
        s = s.link(match seg.simplex {
            Some(kind) => Message::OpenSimplexLink {
                uri: link.clone(),
                kind,
            },
            None => Message::OpenUrl(link.clone()),
        });
    }
    s
}

fn format_color(c: FormatColor) -> Color {
    match c {
        FormatColor::Red => Color::from_rgb(1.0, 0.35, 0.35),
        FormatColor::Green => theme::t().success,
        FormatColor::Blue => theme::t().accent,
        FormatColor::Yellow => Color::from_rgb(0.95, 0.80, 0.25),
        FormatColor::Cyan => Color::from_rgb(0.30, 0.85, 0.90),
        FormatColor::Magenta => Color::from_rgb(0.90, 0.40, 0.85),
        FormatColor::Black => Color::from_rgb(0.05, 0.05, 0.08),
        FormatColor::White => Color::WHITE,
    }
}

fn voice_draft_bar(
    draft: &crate::VoiceDraft,
    send_size: f32,
    icon_size: f32,
    d: Density,
    hover: bool,
    scale: f32,
) -> Element<'static, Message> {
    let wave = Canvas::new(Waveform {
        peaks: draft.peaks.clone(),
        progress: 1.0,
        played: theme::t().accent,
        rest: theme::t().voice_rest,
        scale,
    })
    .width(Length::Fill)
    .height(36.0);

    // Same floating field + rounded-square Send as the text composer.
    let field = container(
        row![
            composer_hit(
                icons::centered_hit(
                    icons::cancel_icon(icon_size, theme::t().text_secondary),
                    send_size,
                ),
                send_size,
                Message::CancelVoiceDraft,
            ),
            column![
                text(format!("Voice · {}", format_duration(draft.duration_secs)))
                    .size(d.meta)
                    .color(theme::t().text_secondary),
                wave,
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 0.0 })
    .width(Length::Fill)
    .style(|_| floating_card());
    let field = crate::glass::surface(field, theme::SMALL_CONTROL_RADIUS);
    container(
        row![
            field,
            composer_send_hit(
                icons::send_tool(send_size, true, hover),
                send_size,
                true,
                Message::SendVoiceDraft,
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: GLASS_FADE, right: 10.0, bottom: 8.0, left: 10.0 })
    .width(Length::Fill)
    .into()
}

/// Soft cap for bubble body text on wide desktops.
const BUBBLE_TEXT_MAX: f32 = 420.0;

/// Compact height for time + ticks.
fn bubble_time_height(d: Density) -> f32 {
    d.meta + 2.0
}

/// Matches `bubble_time_meta` row spacing.
const STAMP_INNER_GAP: f32 = 3.0;

/// Room for one stamp icon (pencil / receipt) plus its gap, in en spaces
/// (0.5em of the meta size each): ≥ `icon + STAMP_INNER_GAP` for every meta.
const STAMP_ICON_SPACES: &str = "\u{2002}\u{2002}\u{2002}";

/// Invisible tail appended to the message text that reserves the stamp's
/// room, Telegram-style: the text shaper measures it with the real fonts
/// (emoji included) and wraps it onto a new line when the last line is too
/// full — then the stamp, pinned bottom-right, lands on that empty line.
///
/// Width estimates used to decide this before: they ignored the group
/// avatar column and counted emoji as half-width letters, so the time and
/// receipt were painted past the bubble edge.
///
/// The leading normal space is the only break point; the rest is glued
/// with no-break spaces so the reservation never splits across lines.
fn stamp_reserve(msg: &ChatMessage) -> String {
    let mut out = String::from(" \u{00A0}\u{2009}");
    if msg.edited {
        out.push_str(STAMP_ICON_SPACES);
    }
    if let Some(label) = format_bubble_time(msg.created_at.as_deref()) {
        out.push_str(&label.replace(' ', "\u{00A0}"));
    }
    if msg.outgoing {
        out.push_str(STAMP_ICON_SPACES);
    }
    out
}

/// The reservation as a transparent span at the stamp's text size, keeping
/// the line height of the body (`body_size`) so the last line doesn't shrink.
fn stamp_reserve_span(
    msg: &ChatMessage,
    d: Density,
    body_size: f32,
) -> iced::widget::text::Span<'static, Message> {
    let line = iced::widget::text::LineHeight::default().to_absolute(body_size.into());
    span(stamp_reserve(msg))
        .size(d.meta)
        .line_height(line)
        .color(Color::TRANSPARENT)
}

/// Size `emoji_message_text` draws `raw` at.
fn emoji_text_size(raw: &str) -> f32 {
    if raw.trim().graphemes(true).count() <= LARGE_EMOJI_MAX {
        48.0
    } else {
        36.0
    }
}

/// Body (ending in [`stamp_reserve_span`]) with time/receipt pinned to its
/// bottom-right corner — no chip background, the stamp sits on the bubble.
/// The body wraps at the width the bubble really gets, so neither can stick
/// out of it.
fn bubble_content_with_time(
    content: Element<'static, Message>,
    msg: &ChatMessage,
    d: Density,
) -> Element<'static, Message> {
    Stack::new()
        .push(container(content).max_width(BUBBLE_TEXT_MAX))
        .push(
            container(bubble_time_meta(msg, d))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End),
        )
        .into()
}

/// Telegram-style trailing time (+ edited pencil + receipt for outgoing).
fn bubble_time_meta(msg: &ChatMessage, d: Density) -> Element<'static, Message> {
    let time_h = bubble_time_height(d);
    let icon_size = (d.meta * 0.85).clamp(10.0, 13.0);
    let mut row = Row::new()
        .spacing(STAMP_INNER_GAP)
        .height(Length::Fixed(time_h))
        .align_y(Alignment::Center);
    let meta = if msg.outgoing {
        theme::t().bubble_out_meta
    } else {
        theme::t().text_muted
    };
    if msg.edited {
        row = row.push(icons::edit_icon(icon_size, meta));
    }
    if let Some(label) = format_bubble_time(msg.created_at.as_deref()) {
        row = row.push(
            txt(label)
                .size(d.meta)
                .line_height(iced::widget::text::LineHeight::Absolute(time_h.into()))
                .color(meta)
                .wrapping(Wrapping::None),
        );
    }
    if msg.outgoing {
        row = row.push(receipt_icon(msg.delivery, icon_size));
    }
    row.into()
}

/// Parse SimpleX timestamps (`2026-11-03T03:23:21Z`, RFC 3339) as local time.
fn parse_local_ts(raw: &str) -> Option<chrono::DateTime<chrono::Local>> {
    let raw = raw.trim();
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.fZ")
                .ok()
                .map(|ndt| ndt.and_utc().with_timezone(&chrono::Local))
        })
}

/// Chat-list stamp: time today, "Yesterday", weekday this week, else date.
fn format_list_time(iso: Option<&str>) -> Option<String> {
    let at = parse_local_ts(iso?)?;
    let now = chrono::Local::now();
    let days = now.date_naive().signed_duration_since(at.date_naive()).num_days();
    Some(match days {
        i64::MIN..=0 => clock().time(&at),
        1 => "Yesterday".into(),
        2..=6 => at.format("%a").to_string(),
        _ if at.format("%Y").to_string() == now.format("%Y").to_string() => {
            at.format("%-d %b").to_string()
        }
        _ => at.format("%-d %b %Y").to_string(),
    })
}

/// Local time a message is filed under for day dividers. Placeholder
/// timestamps (Unix epoch, "1 January 1970") are not a real day → `None`.
fn divider_time(msg: &ChatMessage) -> Option<chrono::DateTime<chrono::Local>> {
    msg.created_at
        .as_deref()
        .and_then(parse_local_ts)
        .filter(|t| chrono::Datelike::year(t) >= 2000)
}

/// Day divider label for the conversation ("Today", "Yesterday", "Monday 3 November").
fn format_day_label(at: &chrono::DateTime<chrono::Local>) -> String {
    let now = chrono::Local::now();
    match now.date_naive().signed_duration_since(at.date_naive()).num_days() {
        i64::MIN..=0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => at.format("%A").to_string(),
        _ if at.format("%Y").to_string() == now.format("%Y").to_string() => {
            at.format("%A %-d %B").to_string()
        }
        _ => at.format("%-d %B %Y").to_string(),
    }
}

/// 12-hour / 24-hour clock (Settings → Appearance), read by every time
/// label; set at start and when the setting changes.
static CLOCK_24: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn set_clock(format: cursus_core::TimeFormat) {
    CLOCK_24.store(format == cursus_core::TimeFormat::H24, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn clock() -> cursus_core::TimeFormat {
    if CLOCK_24.load(std::sync::atomic::Ordering::Relaxed) {
        cursus_core::TimeFormat::H24
    } else {
        cursus_core::TimeFormat::H12
    }
}

/// Format SimpleX `itemTs` (`2026-11-03T03:23:21Z`) as a local wall-clock
/// in the chosen clock.
pub(crate) fn format_bubble_time(iso: Option<&str>) -> Option<String> {
    let raw = iso?.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed = chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%SZ")
                .ok()
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.fZ").ok()
                })
                .map(|ndt| ndt.and_utc()
                        .with_timezone(&chrono::Local))
        })?;
    Some(clock().time(&parsed))
}

fn receipt_icon(status: DeliveryStatus, size: f32) -> Element<'static, Message> {
    match status {
        DeliveryStatus::Pending => text("...")
            .size((size - 2.0).max(10.0))
            .color(theme::t().text_muted)
            .into(),
        DeliveryStatus::Sent => icons::check_sent(size, theme::t().bubble_out_meta),
        DeliveryStatus::Delivered => icons::check_read(size, theme::t().bubble_out_tick),
        DeliveryStatus::Error => text("!")
            .size(size.max(12.0))
            .color(theme::t().danger)
            .into(),
        DeliveryStatus::Warning => text("!")
            .size(size.max(12.0))
            .color(Color::from_rgb(0.95, 0.65, 0.25))
            .into(),
    }
}

/// Prefer delivery/receipt truth over a stale XFTP progress snapshot.
fn effective_file_state(
    state: FileTransferState,
    outgoing: bool,
    delivery: DeliveryStatus,
) -> FileTransferState {
    if outgoing
        && matches!(state, FileTransferState::InProgress | FileTransferState::Other)
        && matches!(
            delivery,
            DeliveryStatus::Sent | DeliveryStatus::Delivered
        )
    {
        return FileTransferState::Complete;
    }
    state
}

fn file_state_label(state: FileTransferState, outgoing: bool) -> &'static str {
    match state {
        FileTransferState::Offer => "available",
        FileTransferState::InProgress => "transferring…",
        FileTransferState::Complete => {
            if outgoing {
                "sent"
            } else {
                "saved"
            }
        }
        FileTransferState::Failed => "failed",
        FileTransferState::Other => "file",
    }
}

fn transfer_status_line(
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    outgoing: bool,
) -> String {
    let size = if file.file_size > 0 {
        format_file_size(file.file_size)
    } else {
        String::new()
    };
    match state {
        FileTransferState::InProgress => {
            let done = file.transferred.unwrap_or(0);
            let total = file
                .transfer_total
                .filter(|n| *n > 0)
                .unwrap_or(file.file_size);
            if total > 0 {
                format!(
                    "{} / {} · {}",
                    format_file_size(done),
                    format_file_size(total),
                    file_state_label(state, outgoing)
                )
            } else if size.is_empty() {
                file_state_label(state, outgoing).to_string()
            } else {
                format!("{} · {}", size, file_state_label(state, outgoing))
            }
        }
        FileTransferState::Failed => {
            let detail = file
                .error_detail
                .as_deref()
                .unwrap_or(file_state_label(state, outgoing));
            if size.is_empty() {
                detail.to_string()
            } else {
                format!("{size} · {detail}")
            }
        }
        _ => {
            if size.is_empty() {
                file_state_label(state, outgoing).to_string()
            } else {
                format!("{} · {}", size, file_state_label(state, outgoing))
            }
        }
    }
}

fn transfer_progress_bar(file: &cursus_core::FileInfo) -> Element<'static, Message> {
    let frac = file.progress_fraction().unwrap_or(0.04);
    progress_bar(0.0..=1.0, frac)
        .girth(Length::Fixed(4.0))
        .length(Length::Fill)
        .style(|_| iced::widget::progress_bar::Style {
            background: iced::Background::Color(theme::t().bg_app),
            bar: iced::Background::Color(theme::t().accent),
            border: iced::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
        })
        .into()
}

fn attachment_status_footer<'a>(
    msg: &'a ChatMessage,
    file: &'a cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
    _stretch: bool,
) -> Element<'static, Message> {
    let line = transfer_status_line(file, state, msg.outgoing);
    let color = if matches!(state, FileTransferState::Failed) {
        theme::t().danger
    } else {
        theme::t().text_muted
    };
    // Always Fill so time + ticks pin to the bubble's bottom-right — a shrink
    // row left them mid-bubble under a wider filename.
    row![
        text(line)
            .size(d.meta)
            .color(color)
            .width(Length::Fill),
        bubble_time_meta(msg, d),
    ]
    .spacing(8)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

fn format_file_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let n = bytes as f64;
    if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{bytes} B")
    }
}

fn is_image_file(name: &str) -> bool {
    matches!(
        std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp")
    )
}

fn file_title_row(
    app: &App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Element<'static, Message> {
    let kind_icon = if is_image_file(&file.file_name) {
        icons::image_icon(18.0, theme::t().text_secondary)
    } else if media::is_video_name(&file.file_name) {
        icons::play_icon(18.0, theme::t().text_secondary)
    } else {
        icons::file_icon(18.0, theme::t().text_secondary)
    };
    let name = text(file.file_name.clone())
        .size(d.body)
        .color(theme::t().text_primary);
    let open_path = app.plaintext_path_for(file);
    let name: Element<'static, Message> =
        if matches!(state, FileTransferState::Complete) {
            if let Some(path) = open_path.filter(|p| p.is_file()) {
                mouse_area(name)
                    .on_release(if is_image_file(&file.file_name) {
                    Message::OpenImage(path)
                } else {
                    Message::OpenLocalFile(path)
                })
                    .interaction(mouse::Interaction::Pointer)
                    .into()
            } else {
                name.into()
            }
        } else {
            name.into()
        };
    let mut row = Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(kind_icon)
        .push(name);
    row = row.push(offer_action(msg, file, state, d));
    row.into()
}

fn voice_waveform_row(
    app: &App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Element<'static, Message> {
    let play = 40.0;
    let wave_h = 28.0;
    let receipt_h = 14.0;
    let side_gap = 4.0;
    // Side column may be taller (receipt); keep play square and vertically centered.
    let row_h = f32::max(wave_h + side_gap + receipt_h, play);
    let label = file
        .duration_secs
        .map(format_duration)
        .unwrap_or_else(|| "voice".into());
    let progress = app.voice_playback_progress(file.file_id).unwrap_or(0.0);
    let plain = app.plaintext_path_for(file);
    let peaks = plain
        .as_ref()
        .and_then(|p| app.waveform_for(p))
        .or_else(|| file.local_path
                .as_ref()
                .and_then(|p| app.waveform_for(p)))
        .map(|p| p.to_vec())
        .unwrap_or_else(|| vec![0.35; media::WAVEFORM_BARS]);

    let wave = Canvas::new(Waveform {
        peaks,
        progress,
        played: theme::t().accent,
        rest: if msg.outgoing { theme::t().voice_rest_out } else { theme::t().voice_rest },
        scale: app.scale_factor(),
    })
    .width(Length::Fill)
    .height(wave_h);

    let wave_row = Row::new()
        .spacing(10)
        .height(wave_h)
        .align_y(Alignment::Center)
        .push(wave)
        .push(
            text(label)
                .size(d.meta)
                .color(theme::t().text_secondary),
        )
        .push(offer_action(msg, file, state, d));

    // Right column: waveform on top; time + receipt bottom-right (Telegram).
    let mut side = Column::new()
        .spacing(side_gap)
        .width(Length::Fill)
        .height(row_h)
        .push(wave_row);

    side = side.push(
        container(bubble_time_meta(msg, d))
            .width(Length::Fill)
            .height(receipt_h)
            .align_x(Alignment::End),
    );

    let play_btn: Element<'_, Message> = if matches!(state, FileTransferState::Complete) {
        if let Some(path) = plain.filter(|p| p.is_file()) {
            let playing = app.voice_playback_progress(file.file_id).is_some();
            let icon = if playing {
                icons::stop_icon(20.0, theme::t().accent_on)
            } else {
                icons::play_icon(20.0, theme::t().accent_on)
            };
            // mouse_area + release: button.on_press blocks swipe-scroll on voice rows.
            mouse_area(
                container(icons::centered_hit(icon, play))
                    .width(play)
                    .height(play)
                    .style(|_| {
                        let c = theme::t();
                        container::Style {
                            background: Some(iced::Background::Color(c.accent)),
                            // Same shape as the nav rail's destination buttons.
                            border: iced::Border {
                                color: c.accent,
                                width: 0.0,
                                radius: theme::SMALL_CONTROL_RADIUS.into(),
                            },
                            ..container::Style::default()
                        }
                    }),
            )
            .on_release(Message::PlayVoice {
                file_id: file.file_id,
                path,
                duration_secs: file.duration_secs.unwrap_or(1),
            })
            .interaction(mouse::Interaction::Pointer)
            .into()
        } else {
            horizontal().width(play).into()
        }
    } else {
        container(icons::mic_icon(20.0, theme::t().text_secondary))
            .width(play)
            .height(play)
            .center_x(play)
            .center_y(play)
            .into()
    };

    Row::new()
        .spacing(10)
        .height(row_h)
        .align_y(Alignment::Center)
        .push(play_btn)
        .push(side)
        .into()
}

fn offer_action(
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Element<'static, Message> {
    if msg.outgoing || !matches!(state, FileTransferState::Offer) {
        return horizontal().width(0).into();
    }
    if file.should_auto_receive() {
        return text("receiving…")
            .size(d.meta)
            .color(theme::t().text_muted)
            .into();
    }
    let hit = 28.0;
    mouse_area(
        container(icons::centered_hit(
            icons::download_icon(16.0, theme::t().accent_on),
            hit,
        ))
        .width(hit)
        .height(hit)
        .style(|_| {
            let c = theme::t();
            container::Style {
                background: Some(iced::Background::Color(c.accent)),
                border: iced::Border {
                    radius: theme::SMALL_CONTROL_RADIUS.into(),
                    ..Default::default()
                },
                ..container::Style::default()
            }
        }),
    )
    .on_release(Message::ReceiveFile {
        file_id: file.file_id,
        file_name: file.file_name.clone(),
    })
    .interaction(mouse::Interaction::Pointer)
    .into()
}

fn format_duration(secs: u32) -> String {
    let mins = secs / 60;
    let rem = secs % 60;
    format!("{mins}:{rem:02}")
}

fn visual_media_preview(
    app: &App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Option<Element<'static, Message>> {
    if !matches!(state, FileTransferState::Complete) {
        return None;
    }
    // Prefer decrypted plaintext; skip preview until CryptoFile decrypt finishes.
    // No fs::metadata / image_dimensions here — view runs on every fling tick.
    let path = app.plaintext_path_for(file)?;
    let is_video = matches!(file.kind, FileKind::Video) || media::is_video_name(&file.file_name);
    // Compact chat-pane previews — keep native aspect.
    const PREVIEW_MAX_W: u32 = 220;
    const PREVIEW_MAX_H: u32 = 280;
    const RADIUS: f32 = 12.0;
    // Videos: placeholder only (ffmpeg posters were unreliable on ARM — do not
    // regenerate). Stills still use the rounded image preview cache.
    let prepared = if is_video {
        None
    } else {
        app.image_preview_for(&path, PREVIEW_MAX_W, PREVIEW_MAX_H)
    };
    // "Blur photos": show the blurred copy until this item is tapped; never
    // the sharp preview while the blurred one is still being made.
    let blur = app.desktop_prefs().blur_media && !app.media_revealed(msg.item_id);
    let prepared = match prepared {
        // Placeholder until the blurred copy exists.
        Some((_, pw, ph)) if blur => app.blurred_preview_for(&path).map(|b| (b.clone(), pw, ph)),
        other => other,
    };
    let (w, h, face): (f32, f32, Element<'static, Message>) = if let Some((display_path, pw, ph)) =
        prepared
    {
        let w = pw as f32;
        let h = ph as f32;
        let thumb = image(image::Handle::from_path(display_path))
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .content_fit(ContentFit::Contain);
        (w, h, thumb.into())
    } else {
        // Still not ready / video placeholder.
        let w = 160.0;
        let h = 120.0;
        let glyph = if is_video {
            icons::play_icon(28.0, theme::t().text_secondary)
        } else {
            icons::image_icon(28.0, theme::t().text_secondary)
        };
        let face = container(icons::centered_hit(glyph, 48.0))
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .center_x(Length::Fixed(w))
            .center_y(Length::Fixed(h))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().bg_elevated)),
                border: iced::Border {
                    radius: RADIUS.into(),
                    ..Default::default()
                },
                ..container::Style::default()
            });
        (w, h, face.into())
    };
    let mut stack = Stack::new()
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .push(face);
    // Floating time + receipt on the image (bare — no chip).
    stack = stack.push(
        container(bubble_time_meta(msg, d))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(Padding {
                top: 0.0,
                right: 6.0,
                bottom: 4.0,
                left: 0.0,
            }),
    );
    let framed = container(stack)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .clip(true);
    // Images stay in the app; the outer bubble owns long-press menus.
    let tap = if blur && !is_video {
        Message::RevealMedia(msg.item_id)
    } else {
        if is_video {
            Message::OpenLocalFile(path)
        } else {
            Message::OpenImage(path)
        }
    };
    Some(
        mouse_area(framed)
            .on_release(tap)
            .interaction(mouse::Interaction::Pointer)
            .into(),
    )
}

#[derive(Debug, Clone)]
struct Waveform {
    peaks: Vec<f32>,
    progress: f32,
    played: Color,
    rest: Color,
    /// Window scale factor: bars are laid out on whole device pixels.
    scale: f32,
}

/// One waveform bar in device pixels: x, width, y, height.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WaveBar {
    x: f32,
    w: f32,
    y: f32,
    h: f32,
    peak: f32,
}

/// Lay voice-message bars out on the device-pixel grid so every bar and gap
/// has crisp edges (fractional gaps blur into grey smears). Bar and gap
/// widths are fixed; the bar count follows the available width and the
/// peaks are resampled to it (max per bucket so speech stays visible).
fn wave_layout(peaks: &[f32], width_px: f32, height_px: f32, scale: f32) -> Vec<WaveBar> {
    const BAR: f32 = 3.0; // logical px
    const GAP: f32 = 2.0;
    if peaks.is_empty() || width_px < 1.0 || height_px < 1.0 {
        return Vec::new();
    }
    let bar = (BAR * scale).round().max(1.0);
    let gap = (GAP * scale).round().max(1.0);
    let pitch = bar + gap;
    let n = (((width_px + gap) / pitch).floor() as usize).max(1);
    // Leftover (< one pitch) splits evenly on both sides, on whole pixels.
    let left = ((width_px - (n as f32 * pitch - gap)) / 2.0).max(0.0).floor();
    let src = peaks.len();
    (0..n)
        .map(|i| {
            let a = i * src / n;
            let b = ((i + 1) * src / n).max(a + 1).min(src);
            let peak = peaks[a.min(src - 1)..b]
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            // Never shorter than wide: the quietest bars are round dots.
            let mut h = (peak.clamp(0.08, 1.0) * height_px * 0.92).round().max(bar);
            // Same parity as the box so the bar centres on a whole pixel.
            if (height_px.round() - h) as i32 % 2 != 0 {
                h += 1.0;
            }
            let y = ((height_px - h) / 2.0).round();
            WaveBar {
                x: left + i as f32 * pitch,
                w: bar,
                y,
                h,
                peak,
            }
        })
        .collect()
}

impl canvas::Program<Message> for Waveform {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let s = if self.scale.is_finite() && self.scale > 0.0 { self.scale } else { 1.0 };
        // The canvas may sit at a fractional offset: shift so bar edges land
        // on device pixels.
        let ox = (bounds.x * s).round() / s - bounds.x;
        let oy = (bounds.y * s).round() / s - bounds.y;
        let bars = wave_layout(&self.peaks, (bounds.width * s).floor(), (bounds.height * s).floor(), s);
        let n = bars.len().max(1) as f32;
        for (i, b) in bars.iter().enumerate() {
            let filled = (i as f32 + 1.0) / n <= self.progress + 0.001;
            let color = if filled { self.played } else { self.rest };
            let top_left = Point::new(ox + b.x / s, oy + b.y / s);
            let size = Size::new(b.w / s, b.h / s);
            // Short bars are dots / capsules (radius = half the width; they
            // are never shorter than wide, so no eye shapes); tall bars get
            // slightly rounded ends.
            let r = if b.h <= b.w * 1.5 { b.w / 2.0 } else { b.w * 0.4 } / s;
            frame.fill(
                &canvas::Path::rounded_rectangle(top_left, size, r.into()),
                Fill::from(color),
            );
        }
        vec![frame.into_geometry()]
    }
}

fn compose_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let narrow = matches!(app.layout(), LayoutMode::Mobile);
    let hover = app.hover_ok();

    let header: Element<'_, Message> = if with_back {
        row![
            mobile_back_btn(Message::CloseCompose, hover),
            text("New conversation")
                .size(d.title).font(BOLD)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: 4.0,
        })
        .into()
    } else {
        // Desktop: no Close button — Esc (or picking another section) closes.
        row![text("New conversation")
            .size(d.title).font(BOLD)
            .color(theme::t().text_primary)
            .width(Length::Fill),]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding::from([d.pad_tight, 0.0]))
        .into()
    };

    // Connect: the one-time link (shown at once, kept until used), its
    // incognito switch and a fresh-link button, then joining with a link.
    let mut connect = Column::new().spacing(4).width(Length::Fill);
    connect = connect.push(
        row![
            icon_disc(icons::link_icon(18.0, theme::t().accent), 36.0),
            text("Connect")
                .size(d.body)
                .font(BOLD)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            button(icons::centered_hit(icons::info_icon(18.0, theme::t().text_muted), 32.0))
                .on_press(Message::ShowSettingInfo(crate::SettingInfo::Connect))
                .padding(0)
                .style(theme::icon_action(hover)),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(Padding::from([4, 4])),
    );
    connect = connect.push(pref_section("Copy one-time link", false, d));
    connect = connect.push(crate::preference_ui::note("Share privately with one person. The link expires after they connect.", d));
    let link_box: Element<'_, Message> = match app.invitation() {
        Some(inv) => link_copy_box(&inv.link.display, Message::CopyInviteLink, d, hover),
        None => container(
            text(if app.invitation_busy() { "Creating link…" } else { "No link yet" })
                .size(d.meta)
                .color(theme::t().text_muted),
        )
        .padding(Padding::from([10, 12]))
        .width(Length::Fill)
        .into(),
    };
    connect = connect.push(container(link_box).padding(Padding::from([0, 8])));
    connect = connect.push(preference_switch(
        None, "Use a random profile", Some("Use a random name instead of your profile."),
        app.incognito(), (!app.invitation_busy()).then_some(Message::ToggleIncognito), d, hover,
    ));
    // Same look as the Connect button below: accent, full width on phones.
    let regenerate = button(
        container(
            row![
                icons::repeat_once_icon(16.0, theme::t().accent),
                text("Replace link").size(d.meta),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(if narrow { Length::Fill } else { Length::Shrink })
        .center_x(if narrow { Length::Fill } else { Length::Shrink }),
    )
    .on_press_maybe((!app.invitation_busy()).then_some(Message::RegenerateInvitation))
    .padding(Padding::from([9, 18]))
    .width(if narrow { Length::Fill } else { Length::Shrink })
    .style(theme::secondary_action(hover));
    connect = connect.push(
        container(regenerate)
            .width(Length::Fill)
            .align_x(Alignment::End)
            .padding(Padding::from([4, 8])),
    );

    let link_field = crate::edit_menu::edit_field(
        text_input("Invite or address link…", app.invite_input())
            .id("edit-field-InviteInputChanged")
            .on_input(Message::InviteInputChanged)
            .on_submit(Message::AcceptInvite)
            .padding(Padding {
                top: 8.0,
                right: 12.0,
                bottom: 8.0,
                left: 12.0,
            })
            .size(d.body)
            .width(Length::Fill)
            .style(theme::rounded_field()),
        iced::widget::Id::new("edit-field-InviteInputChanged"),
        app.invite_input(),
        Message::InviteInputChanged,
        true,
    );
    // Icon-only paste (fills the field from the clipboard; never connects).
    let paste_btn = composer_hit(
        icons::paste_tool(PROFILE_HEADER_ICON, hover),
        PROFILE_HEADER_ICON,
        Message::PasteInviteLink,
    );
    let field_row = row![link_field, paste_btn]
        .spacing(6)
        .align_y(Alignment::Center);
    let label = if app.invite_connecting() { "Connecting…" } else { "Connect" };
    let connect_btn = button(
        container(text(label).size(d.meta))
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .on_press_maybe(
        (!app.invite_connecting() && !app.invite_input().trim().is_empty())
            .then_some(Message::AcceptInvite),
    )
    .padding(Padding::from([9, 18]))
    .width(if narrow { Length::Fill } else { Length::Shrink })
    .style(theme::primary_action(hover));
    connect = connect.push(pref_section("Join with a link", false, d));
    connect = connect.push(crate::preference_ui::note("Paste a contact, group or channel link to connect.", d));
    let join: Element<'_, Message> = if narrow {
        column![field_row, connect_btn].spacing(8).into()
    } else {
        row![field_row.width(Length::Fill), connect_btn]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
    };
    connect = connect.push(container(join).padding(Padding::from([0, 8])));
    if let Some(error) = app.invite_error() {
        connect = connect.push(container(text(error).size(d.meta).color(theme::t().danger)).padding([0, 8]));
    }

    // Same tiles as a profile's quick actions: glyph over a centred label.
    let choice = |icon, title: &'static str, message| button(column![
            icons::centered_hit(icon, 24.0),
            ellipsis_text(title.to_string(), d.meta, theme::t().text_secondary, 1),
        ]
        .spacing(6).align_x(Alignment::Center).width(Length::Fill))
        .on_press(message).padding(Padding::from([12, 4])).width(Length::Fill).style(theme::profile_action(hover));
    let choices = row![
        choice(icons::link_icon(22.0, theme::t().accent), "Connect", Message::CloseCreateChat),
        choice(icons::user_group_icon(22.0, theme::t().accent), "New group", Message::OpenCreateGroup),
        choice(icons::nav_icon(icons::NavIcon::Channels, true, 22.0), "New channel", Message::OpenCreateChannel),
    ].spacing(8);
    let form = if app.create_chat().is_some() { create_chat_form(app) } else { card(connect) };
    let body = column![choices, form].spacing(PAGE_GAP);
    column![header, page_scroll(body, d)]
        .height(Length::Fill)
        .into()
}

/// Settings in the list column. Desktop: profile summary + category rows
/// (the section opens in the main pane). Mobile / compact: every section as
/// cards on one scrolling page.
/// The settings list: your profile on top, then one row per category with
/// a line saying what is inside. Each opens its page — beside the list when
/// there are two columns, over it when there is one.
fn settings_panel(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let two_columns = matches!(app.layout(), LayoutMode::Desktop);
    let draft = app.own_profile();
    let row_button = |content: Element<'static, Message>, section: crate::SettingsSection| {
        button(content)
            .on_press(Message::OpenSettingsSection(section))
            .padding(Padding::from([if two_columns { 12.0 } else { 20.0 }, 16.0]))
            .width(Length::Fill)
            .style(theme::list_row(two_columns && app.settings_open() && app.settings_section() == section, hover))
    };
    let bio = if draft.fields.short_descr.is_empty() { "Add a bio".to_string() } else { draft.fields.short_descr.clone() };
    let mut col = Column::new().spacing(2).width(Length::Fill);
    col = col.push(row_button(
        row![
            profile_avatar(draft.fields.image.as_deref(), &draft.fields.display_name, 56.0),
            column![
                text(draft.fields.display_name.clone()).size(d.title).font(BOLD).color(theme::t().text_primary),
                container(profile_rich(&bio, d.meta, theme::t().text_secondary, false).wrapping(Wrapping::None))
                    .width(Length::Fill).height(d.meta * 1.5).clip(true),
            ].spacing(2).width(Length::Fill),
        ]
        .spacing(14)
        .align_y(Alignment::Center)
        .into(),
        crate::SettingsSection::Profile,
    ));
    col = col.push(vertical().height(12));
    // Categories in named groups, the same on Android / macOS:
    // you · the app · about.
    for (heading, group) in SETTINGS_GROUPS {
        let mut rows = Column::new().width(Length::Fill);
        for &section in group {
            let (icon, title) = settings_section_meta(section);
            rows = rows.push(
                button(
                    row![
                        icon_disc(icon, ROW_ICON_BOX),
                        text(title).size(d.body).color(theme::t().text_primary).width(Length::Fill),
                        icons::chevron_right(16.0, theme::t().text_muted),
                    ]
                    .spacing(ROW_GAP)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenSettingsSection(section))
                .padding(Padding::from([if two_columns { 12.0 } else { 18.0 }, 16.0]))
                .width(Length::Fill)
                .style(theme::list_row_r(
                    two_columns && app.settings_open() && app.settings_section() == section,
                    hover,
                    theme::INSET_CONTROL_RADIUS,
                )),
            );
        }
        col = col.push(crate::preference_ui::group(Some(heading), None, rows, d));
        col = col.push(vertical().height(4));
    }
    let bottom = nav_clearance(d).max(24.0);
    page_scroll_y(col.spacing(8).padding(Padding { top: 0.0, right: 12.0, bottom, left: 12.0 }))
}

/// The settings list's groups, in order, with their headings.
const SETTINGS_GROUPS: [(&str, &[crate::SettingsSection]); 3] = {
    use crate::SettingsSection as S;
    [
        ("Account", &[S::Profile, S::Address]),
        ("Preferences", &[S::Privacy, S::Notifications, S::Appearance]),
        ("Cursus", &[S::About]),
    ]
};

fn settings_section_meta(section: crate::SettingsSection) -> (Element<'static, Message>, &'static str) {
    use crate::SettingsSection as S;
    let a = theme::t().accent;
    match section {
        S::Profile => (icons::user_alt_icon(20.0, a), "Profile"),
        S::Address => (icons::address_card_icon(20.0, a), "Address"),
        S::Privacy => (icons::shield_icon(20.0, a), "Privacy & Security"),
        S::Notifications => (icons::notifications_icon(20.0, a), "Notifications"),
        S::Appearance => (icons::image_icon(20.0, a), "Appearance"),
        S::About => (icons::info_icon(20.0, a), "About"),
    }
}

/// A settings page: its name (with Back when it covers the list) and its rows.
fn settings_detail(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    if !app.settings_open() {
        return empty_detail(icons::nav_icon(NavIcon::Settings, false, 36.0),
            "Your settings", "Choose a category to make Cursus your own.", None, d, app.hover_ok());
    }
    let (_, title) = settings_section_meta(app.settings_section());
    let pad = if with_back {
        Padding { top: d.pad_tight, right: d.pad, bottom: d.pad_tight, left: 8.0 }
    } else {
        Padding::from([d.pad_tight, 0.0])
    };
    column![
        container(page_title(title, Some(Message::Back), d, app.hover_ok())).padding(pad),
        page_scroll(settings_section_card(app, app.settings_section()), d),
    ]
    .height(Length::Fill)
    .into()
}

fn settings_section_card(app: &App, section: crate::SettingsSection) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let prefs = app.desktop_prefs();
    let grouped = !matches!(section, crate::SettingsSection::Profile | crate::SettingsSection::About);
    let mut col = PreferenceSections::new(grouped, d);
    match section {
        crate::SettingsSection::Profile => {
            let draft = app.own_profile();
            col = col.push(vertical().height(6));
            col = col.push(profile_identity(
                tappable_avatar(
                    profile_avatar(
                        draft.fields.image.as_deref(),
                        &draft.fields.display_name,
                        PROFILE_AVATAR,
                    ),
                    Message::OpenPhotoMenu(crate::PhotoTarget::Own),
                ),
                None,
                vec![
                    editable_line(
                        &draft.fields.display_name,
                        "Display name",
                        LineStyle::Name,
                        crate::EditField::OwnName,
                        d,
                        hover,
                    ),
                    editable_line(
                        &draft.fields.short_descr,
                        "Add a bio",
                        LineStyle::Sub,
                        crate::EditField::OwnBio,
                        d,
                        hover,
                    ),
                ],
            ));
            col = col.section("Description");
            col = col.push(editable_line(
                &draft.fields.description,
                "Add a longer description",
                LineStyle::Body,
                crate::EditField::OwnDescription,
                d,
                hover,
            ));
        }
        crate::SettingsSection::Address => {
            // The reusable address, as in the official apps: create it,
            // show / hide it (hidden by default), delete it. Sharing options
            // only exist while there is one.
            match app.address_link() {
                None => {
                    col = col.push(
                        container(text("Loading…").size(d.meta).color(theme::t().text_muted))
                            .padding(Padding::from([6, 12])),
                    );
                }
                Some(None) => {
                    col = col.push(nav_row(
                        icons::address_card_icon(18.0, theme::t().accent),
                        "Create address",
                        None,
                        Message::EnsureAddress,
                        None,
                        d,
                        hover,
                    ));
                }
                Some(Some(link)) => {
                    if app.address_revealed() {
                        col = col.push(
                            container(
                                row![
                                    link_copy_box(link, Message::CopyAddressLink, d, hover),
                                    composer_hit(
                                        icons::eye_icon(20.0, theme::t().accent),
                                        PROFILE_HEADER_ICON,
                                        Message::ToggleAddressReveal,
                                    ),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .padding(Padding::from([4, 12])),
                        );
                    } else {
                        col = col.push(nav_row(
                            icons::eye_icon(18.0, theme::t().accent),
                            "Show address",
                            None,
                            Message::ToggleAddressReveal,
                            None,
                            d,
                            hover,
                        ));
                    }
                    let shares = app
                        .own_profile()
                        .fields
                        .contact_link
                        .as_ref()
                        .is_some_and(|s| !s.is_empty());
                    col = col.push(settings_toggle_row("Share address on profile", shares, Message::SetProfileAddress, d));
                    col = col.push(settings_toggle_row(
                        "Auto-accept address requests",
                        prefs.auto_accept_address,
                        Message::SetAddressAutoAccept,
                        d,
                    ));
                    col = col.push(group_action_tile(
                        icons::trash_icon(18.0, theme::t().danger),
                        "Delete address",
                        Message::AskDeleteAddress,
                        theme::ghost_danger(hover),
                        d,
                        true,
                        true,
                    ));
                }
            }
        }
        crate::SettingsSection::Appearance => {
            col = col.section("Display");
            col = col.push(settings_choice_row(
                "Theme",
                prefs.theme.label(),
                Message::OpenPrefMenu(PrefMenu::Theme),
                d,
                hover,
            ));
            col = col.push(settings_choice_row(
                "Text size",
                prefs.text_size.label(),
                Message::OpenPrefMenu(PrefMenu::TextSize),
                d,
                hover,
            ));
            col = col.push(settings_choice_row(
                "Time format",
                prefs.time_format.label(),
                Message::OpenPrefMenu(PrefMenu::TimeFormat),
                d,
                hover,
            ));
            col = col.section("Typing");
            col = col.push(info_toggle(
                "Enter sends messages",
                crate::SettingInfo::EnterSends,
                prefs.enter_sends,
                Message::SetEnterSends,
                d,
                hover,
            ));
        }
        crate::SettingsSection::Privacy => {
            use crate::SettingInfo as I;
            let (rcpt_contacts, rcpt_groups) = app.receipts();
            // What someone looking at the screen sees.
            col = col.section("On screen");
            col = col.push(info_toggle("Message previews in chat list", I::ChatPreviews, prefs.chat_previews, Message::SetChatPreviews, d, hover));
            col = col.push(info_toggle("Blur photos and videos", I::BlurMedia, prefs.blur_media, Message::SetBlurMedia, d, hover));
            // What your contacts learn.
            col = col.section("What contacts see");
            col = col.push(info_toggle(
                "Delivery receipts to contacts",
                I::ReceiptsContacts,
                rcpt_contacts,
                |on| Message::SetReceipts { groups: false, on },
                d,
                hover,
            ));
            col = col.push(info_toggle(
                "Delivery receipts in small groups",
                I::ReceiptsGroups,
                rcpt_groups,
                |on| Message::SetReceipts { groups: true, on },
                d,
                hover,
            ));
            col = col.section("Links");
            col = col.push(info_toggle("Remove tracking from links", I::CleanLinks, prefs.clean_links, Message::SetCleanLinks, d, hover));
            col = col.section("App access");
            // Who can open Cursus.
            col = col.push(info_toggle("App lock", I::AppLock, prefs.app_lock, Message::SetAppLock, d, hover));
            if prefs.app_lock {
                col = col.push(info_row(
                    "Lock",
                    I::LockAfter,
                    choice_chip(prefs.lock_after.label(), Message::OpenPrefMenu(PrefMenu::LockAfter), d, hover),
                    d,
                    hover,
                ));
                col = col.push(group_action_tile(
                    icons::lock_icon(18.0, theme::t().accent),
                    "Lock now",
                    Message::LockNow,
                    theme::ghost_tile_r(hover, ROW_RADIUS),
                    d,
                    true,
                    false,
                ));
            }
            col = col.section("Starting");
            col = col.push(info_toggle(
                "Unlock automatically at start",
                I::KeyringUnlock,
                prefs.keyring_unlock,
                Message::SetKeyringUnlock,
                d,
                hover,
            ));

            // What reaches other servers and the disk.
            col = col.section("Downloads");
            col = col.push(info_toggle(
                "Download images and voice automatically",
                I::AutoDownload,
                prefs.auto_download,
                Message::SetAutoDownload,
                d,
                hover,
            ));
            col = col.push(info_toggle("Protect IP address", I::ProtectIp, prefs.protect_ip, Message::SetProtectIp, d, hover));
            col = col.section("On this device");
            col = col.push(info_toggle("Encrypt stored files", I::EncryptFiles, app.encrypt_files(), Message::SetEncryptFiles, d, hover));
        }
        crate::SettingsSection::Notifications => {
            col = col.section("Alerts");
            use crate::SettingInfo as I;
            col = col.push(info_toggle(
                "Desktop notifications",
                I::DesktopNotifications,
                prefs.notifications,
                Message::SetDesktopNotifications,
                d,
                hover,
            ));
            col = col.push(info_toggle(
                "Hide message text in notifications",
                I::HideNotificationText,
                prefs.hide_notification_text,
                Message::SetHideNotificationText,
                d,
                hover,
            ));
            col = col.section("Background");
            col = col.push(info_toggle("System tray icon", I::Tray, prefs.tray, Message::SetTray, d, hover));
            col = col.push(info_toggle(
                "Close window hides to tray",
                I::CloseToTray,
                prefs.close_to_tray,
                Message::SetCloseToTray,
                d,
                hover,
            ));
        }
        crate::SettingsSection::About => {
            col = col.push(
                container(
                    column![
                        text(concat!("Cursus ", env!("CARGO_PKG_VERSION"))).size(d.body).color(theme::t().text_primary),
                        text("Free software under AGPL-3.0-or-later, built on the official SimpleX Chat core.")
                            .size(d.meta)
                            .color(theme::t().text_secondary),
                    ]
                    .spacing(4),
                )
                .padding(Padding::from([6, 12])),
            );
            col = col.push(nav_row(
                icons::link_icon(18.0, theme::t().accent),
                "Source code",
                Some("github.com/dplz01/cursus".to_string()),
                Message::OpenUrl("https://github.com/dplz01/cursus".into()),
                None,
                d,
                hover,
            ));
        }
    }
    col.finish()
}

/// Trailing slot of a preference row (kept for future per-row status).
enum PrefTrail {
    None,
}

/// The one settings / preferences row used by app Settings, chat (contact),
/// group and channel preferences: [icon] label ……… control [peer glyph].
fn pref_row<'a>(
    icon: Option<Element<'a, Message>>,
    label: &'a str,
    control: Element<'a, Message>,
    trail: PrefTrail,
    d: Density,
) -> Element<'a, Message> {
    preference_row_content(icon, label, None, control, trail, d)
}

fn preference_row_content<'a>(
    icon: Option<Element<'a, Message>>,
    label: &'a str,
    note: Option<&'a str>,
    control: Element<'a, Message>,
    trail: PrefTrail,
    d: Density,
) -> Element<'a, Message> {
    let support = note.map(|note| {
        text(note)
            .size(d.meta)
            .color(theme::t().text_secondary)
            .width(Length::Fill)
            .into()
    });
    preference_row_with_support(icon, label, support, control, trail, d)
}

fn preference_row_with_support<'a>(
    icon: Option<Element<'a, Message>>,
    label: &'a str,
    support: Option<Element<'a, Message>>,
    control: Element<'a, Message>,
    trail: PrefTrail,
    d: Density,
) -> Element<'a, Message> {
    // Bare glyph in the shared 24px slot, like navigation rows.
    let icon = icon.map(|icon| icons::centered_hit(icon, ROW_ICON_BOX));
    let trail = match trail {
        PrefTrail::None => Space::new().width(0).height(0).into(),
    };
    crate::preference_row::PreferenceRow::row(
        icon,
        text(label).size(d.body).color(theme::t().text_primary)
            .width(Length::Fill)
            .into(),
        support,
        control,
        trail,
        d.body,
        d.list_width == 0.0,
    )
}

/// Explanation under a setting's name (plain; the (i) sits by the name).
fn setting_support(info: crate::SettingInfo, d: Density) -> Element<'static, Message> {
    text(setting_summary(info)).size(d.meta).color(theme::t().text_secondary)
        .width(Length::Fill)
        .into()
}

/// A setting's name with its (i) right after it: the help belongs to the
/// name, not to the switch. Its own button, so it never toggles the row.
fn info_title<'a>(label: &'a str, info: crate::SettingInfo, d: Density, hover: bool) -> Element<'a, Message> {
    row![
        text(label).size(d.body).color(theme::t().text_primary),
        button(icons::centered_hit(icons::info_icon(16.0, theme::t().text_muted), 24.0))
            .on_press(Message::ShowSettingInfo(info))
            .padding(0)
            .style(theme::icon_action(hover)),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// [`preference_row_with_support`] with a ready-made title element.
fn info_pref_row<'a>(
    label: &'a str,
    info: crate::SettingInfo,
    control: Element<'a, Message>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    crate::preference_row::PreferenceRow::row(
        None,
        info_title(label, info, d, hover),
        Some(setting_support(info, d)),
        control,
        Space::new().width(0).height(0).into(),
        d.body,
        d.list_width == 0.0,
    )
}

fn preference_switch<'a>(
    icon: Option<Element<'a, Message>>,
    label: &'a str,
    note: Option<&'a str>,
    on: bool,
    message: Option<Message>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let row = preference_row_content(
        icon,
        label,
        note,
        pref_toggle(on, message.is_some()),
        PrefTrail::None,
        d,
    );
    button(row)
        .on_press_maybe(message)
        .padding(0)
        .width(Length::Fill)
        .style(theme::ghost_tile_r(hover, theme::INSET_CONTROL_RADIUS))
        .into()
}

/// Section label above a group of preference rows (same on every page).
fn pref_section<'a>(label: &'a str, first: bool, d: Density) -> Element<'a, Message> {
    container(crate::preference_ui::heading(label, d))
        .padding(Padding { top: if first { 0.0 } else { 14.0 }, bottom: 6.0, ..Padding::ZERO })
        .into()
}

fn pref_toggle<'a>(on: bool, enabled: bool) -> Element<'a, Message> {
    let c = theme::t();
    let knob = container(Space::new().width(20).height(20)).style(move |_| container::Style {
        background: Some(
            Color {
                a: if enabled { 1.0 } else { 0.45 },
                ..Color::WHITE
            }
            .into(),
        ),
        border: iced::Border {
            radius: theme::CONTROL_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let track = container(knob)
        .width(46)
        .height(28)
        .padding(Padding::from([4, 4]))
        .align_x(if on { Alignment::End } else { Alignment::Start })
        .style(move |_| container::Style {
            background: Some(
                if on {
                    Color {
                        a: if enabled { 1.0 } else { 0.45 },
                        ..c.accent
                    }
                } else {
                    Color {
                        a: if enabled { 0.24 } else { 0.11 },
                        ..c.text_secondary
                    }
                }
                .into(),
            ),
            border: iced::Border {
                radius: theme::CONTROL_RADIUS.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    track.into()
}

/// Setting name with an explanation button.
fn info_row<'a>(
    label: &'a str,
    info: crate::SettingInfo,
    control: Element<'a, Message>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    info_pref_row(label, info, control, d, hover)
}

fn info_toggle<'a>(
    label: &'a str,
    info: crate::SettingInfo,
    on: bool,
    to_msg: fn(bool) -> Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let label = match info {
        crate::SettingInfo::ChatPreviews => "Message previews",
        crate::SettingInfo::ReceiptsContacts => "Contact delivery receipts",
        crate::SettingInfo::ReceiptsGroups => "Group delivery receipts",
        crate::SettingInfo::AutoDownload => "Automatic media downloads",
        crate::SettingInfo::HideNotificationText => "Private notifications",
        crate::SettingInfo::KeyringUnlock => "Automatic unlock",
        _ => label,
    };
    let content = info_pref_row(label, info, pref_toggle(on, true), d, hover);
    button(content)
        .on_press(to_msg(!on))
        .padding(0)
        .width(Length::Fill)
        .style(theme::ghost_tile(hover))
        .into()
}

fn setting_summary(info: crate::SettingInfo) -> &'static str {
    use crate::SettingInfo as I;
    match info {
        I::ChatPreviews => "Show the latest message below each name.",
        I::BlurMedia => "Reveal media only when you tap it.",
        I::ReceiptsContacts => "Let contacts know their messages arrived.",
        I::ReceiptsGroups => "Share delivery status in groups of up to 20.",
        I::CleanLinks => "Clean the links you send and open.",
        I::AppLock => "Require your password when you return.",
        I::LockAfter => "Choose when Cursus asks you to unlock.",
        I::KeyringUnlock => "Use the password saved in your system keyring.",
        I::AutoDownload => "Keep small media ready to view and play.",
        I::ProtectIp => "Ask before using unfamiliar file servers.",
        I::EncryptFiles => "Protect new attachments on this device.",
        I::DesktopNotifications => "Get an alert when a new message arrives.",
        I::HideNotificationText => "Show the sender, keep the message private.",
        I::Tray => "Keep Cursus within reach in your system tray.",
        I::CloseToTray => "Keep receiving messages after closing the window.",
        I::EnterSends => "Use Shift+Enter to start a new line.",
        I::GroupReceipts => "Let members know their messages arrived.",
        I::Connect => "Invite someone or connect with their link.",
    }
}

/// Explanation sheet for one setting (opened from its ⓘ button).
fn setting_info_overlay(app: &App, info: crate::SettingInfo) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let (title, body) = info.text();
    let mut content = column![row![
        icons::info_icon(20.0, theme::t().accent),
        text(title).size(d.body).font(BOLD).color(theme::t().text_primary),
    ]
    .spacing(8)
    .align_y(Alignment::Center)]
    .spacing(12)
    .max_width(440);
    for para in body.split("\n\n") {
        content = content.push(
            text(para)
                .size(d.meta + 1.0)
                .color(theme::t().text_secondary)
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    content = content.push(
        container(
            button(text("Got it").size(d.meta).font(BOLD))
                .on_press(Message::CloseSettingInfo)
                .padding(Padding::from([10, 18]))
                .style(theme::primary_action(hover)),
        )
        .width(Length::Fill)
        .align_x(Alignment::End),
    );
    let card = menu_card(content.into(), d);
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseSettingInfo);
    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));
    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

fn settings_toggle_row<'a>(
    label: &'a str,
    on: bool,
    to_msg: fn(bool) -> Message,
    d: Density,
) -> Element<'a, Message> {
    preference_switch(None, label, None, on, Some(to_msg(!on)), d, true)
}

fn settings_choice_row<'a>(
    label: &'a str,
    value: &'a str,
    open: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    pref_row(None, label, choice_chip(value, open, d, hover), PrefTrail::None, d)
}

fn choice_chip<'a>(
    value: impl Into<String>,
    open: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    // Quiet value + chevron (the same look as rows that open a choice);
    // the tint appears on hover / press.
    button(row_value(&value.into(), d))
        .on_press(open)
        .padding(Padding::from([6, 8]))
        .style(theme::ghost_tile_r(hover, theme::INSET_CONTROL_RADIUS - 4.0))
        .into()
}

fn chat_menu_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let Some(menu) = app.chat_menu() else {
        return vertical().height(0).into();
    };
    let chat = app.chat_by_id(&menu.chat_id);
    let hover = app.hover_ok();

    let card: Element<'_, Message> = match &menu.pane {
        ChatMenuPane::Actions => {
            let mut items = Column::new().spacing(4).width(Length::Fill);
            items = items.push(
                text(&menu.title)
                    .size(d.body)
                    .color(theme::t().text_primary),
            );
            items = items.push(
                text("Chat options")
                    .size(d.meta)
                    .color(theme::t().text_muted),
            );
            items = items.push(vertical().height(8));

            if matches!(menu.chat_id.chat_type, ChatType::Direct)
                && chat.map(|c| c.pending_request_id.is_none()).unwrap_or(false)
            {
                items = items.push(menu_action(
                    "View contact",
                    Message::ChatMenuAction(ChatMenuAction::ViewContact),
                    hover,
                    d,
                    false,
                ));
            }
            if matches!(menu.chat_id.chat_type, ChatType::Group) {
                items = items.push(menu_action(
                    if chat.map(|c| c.is_channel).unwrap_or(false) {
                        "Channel settings"
                    } else {
                        "Group settings"
                    },
                    Message::ChatMenuAction(ChatMenuAction::ViewGroup),
                    hover,
                    d,
                    false,
                ));
            }

            let show_mark_read = chat
                .map(|c| c.unread > 0 || c.unread_chat)
                .unwrap_or(false);
            if show_mark_read {
                items = items.push(menu_action(
                    "Mark as read",
                    Message::ChatMenuAction(ChatMenuAction::MarkRead),
                    hover,
                    d,
                    false,
                ));
            } else {
                items = items.push(menu_action(
                    "Mark as unread",
                    Message::ChatMenuAction(ChatMenuAction::MarkUnread),
                    hover,
                    d,
                    false,
                ));
            }

            let fav_label = if chat.map(|c| c.settings.favorite).unwrap_or(false) {
                "Unfavorite"
            } else {
                "Favorite"
            };
            items = items.push(menu_action(
                fav_label,
                Message::ChatMenuAction(ChatMenuAction::ToggleFavorite),
                hover,
                d,
                false,
            ));

            let mute_label = if chat
                .map(|c| c.settings.enable_ntfs.is_muted())
                .unwrap_or(false)
            {
                "Unmute"
            } else {
                "Mute"
            };
            // Notes folder has no mute/favorite/delete in official app for some of these —
            // still show mute/favorite for direct/group; hide delete destructive extras for Local.
            let is_local = matches!(menu.chat_id.chat_type, ChatType::Local);
            if !is_local {
                items = items.push(menu_action(
                    mute_label,
                    Message::ChatMenuAction(ChatMenuAction::ToggleMute),
                    hover,
                    d,
                    false,
                ));
                items = items.push(menu_action(
                    "Add to list",
                    Message::ChatMenuAction(ChatMenuAction::AddToList),
                    hover,
                    d,
                    false,
                ));
            }

            items = items.push(menu_action(
                "Clear chat",
                Message::ChatMenuAction(ChatMenuAction::Clear),
                hover,
                d,
                false,
            ));
            if !is_local {
                items = items.push(menu_action(
                    "Delete",
                    Message::ChatMenuAction(ChatMenuAction::Delete),
                    hover,
                    d,
                    true,
                ));
            }

            menu_card(items.into(), d)
        }
        ChatMenuPane::ConfirmClear => menu_card(
            column![
                text("Clear chat?")
                    .size(d.body)
                    .color(theme::t().text_primary),
                text("Message history will be removed on this device.")
                    .size(d.meta)
                    .color(theme::t().text_secondary),
                row![
                    button(text("Cancel").size(d.meta))
                        .on_press(Message::ChatMenuCancelConfirm)
                        .padding(Padding::from([10, 14]))
                        .style(theme::secondary_action(hover)),
                    button(text("Clear").size(d.meta))
                        .on_press(Message::ChatMenuDoClear)
                        .padding(Padding::from([10, 14]))
                        .style(theme::danger_action(hover)),
                ]
                .spacing(8),
            ]
            .spacing(12)
            .into(),
            d,
        ),
        ChatMenuPane::ConfirmDelete => menu_card(
            column![
                text("Delete chat?")
                    .size(d.body)
                    .color(theme::t().text_primary),
                text("This removes the contact/chat and cannot be undone.")
                    .size(d.meta)
                    .color(theme::t().text_secondary),
                row![
                    button(text("Cancel").size(d.meta))
                        .on_press(Message::ChatMenuCancelConfirm)
                        .padding(Padding::from([10, 14]))
                        .style(theme::secondary_action(hover)),
                    button(text("Delete").size(d.meta))
                        .on_press(Message::ChatMenuDoDelete)
                        .padding(Padding::from([10, 14]))
                        .style(theme::danger_action(hover)),
                ]
                .spacing(8),
            ]
            .spacing(12)
            .into(),
            d,
        ),
        ChatMenuPane::Lists { tags, new_name } => {
            let mut list = Column::new().spacing(4).width(Length::Fill);
            list = list.push(
                text("Add to list")
                    .size(d.body)
                    .color(theme::t().text_primary),
            );
            list = list.push(
                text("Tap a list to add or remove this chat.")
                    .size(d.meta)
                    .color(theme::t().text_secondary),
            );
            if tags.is_empty() {
                list = list.push(
                    text("No lists yet — create one below.")
                        .size(d.meta)
                        .color(theme::t().text_muted),
                );
            }
            for tag in tags {
                let assigned = chat
                    .map(|c| c.tag_ids.contains(&tag.id))
                    .unwrap_or(false);
                let label = if assigned {
                    format!("✓ {}", tag.text)
                } else {
                    tag.text.clone()
                };
                list = list.push(
                    button(text(label).size(d.body))
                        .on_press(Message::ChatMenuToggleTag(tag.id))
                        .padding(Padding::from([12, 12]))
                        .width(Length::Fill)
                        .style(theme::menu_item(hover)),
                );
            }
            list = list.push(vertical().height(8));
            list = list.push(
                crate::edit_menu::edit_field(
                    text_input("New list name…", new_name)
                    .id("edit-field-ChatMenuNewListChanged")
                    .on_input(Message::ChatMenuNewListChanged)
                    .on_submit(Message::ChatMenuCreateList)
                    .padding(d.composer)
                    .size(d.meta)
                    .width(Length::Fill)
                    .style(theme::composer_input()),
                    iced::widget::Id::new("edit-field-ChatMenuNewListChanged"),
                    new_name,
                    Message::ChatMenuNewListChanged,
                    true,
                ),
            );
            list = list.push(
                row![
                    button(text("Create list").size(d.meta))
                        .on_press(Message::ChatMenuCreateList)
                        .padding(Padding::from([10, 14]))
                        .style(theme::primary_action(hover)),
                    button(text("Done").size(d.meta))
                        .on_press(Message::CloseChatMenu)
                        .padding(Padding::from([10, 14]))
                        .style(theme::secondary_action(hover)),
                ]
                .spacing(8),
            );
            menu_card(list.into(), d)
        }
    };

    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseChatMenu);

    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));

    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

fn pref_menu_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(kind) = app.pref_menu() else {
        return vertical().height(0).into();
    };
    // Optional role glyph per row (menus with “svg + name” entries).
    let mut item_icons: Vec<Option<&'static str>> = Vec::new();
    let (title, items): (&str, Vec<(String, bool, Message)>) = match kind {
        PrefMenu::GroupLinkDelete => ("Delete the group link? Members stay in the group.", vec![
            ("Cancel".into(), false, Message::ClosePrefMenu),
            ("Delete link".into(), false, Message::DeleteGroupLink),
        ]),
        PrefMenu::GroupLinkKind => {
            let short = app.group_settings().map(|g| g.link_prefer_short);
            (
                "Link format",
                vec![
                    ("Short link".to_string(), short == Some(true), Message::SetGroupLinkShort(true)),
                    ("Full link".to_string(), short == Some(false), Message::SetGroupLinkShort(false)),
                ],
            )
        }
        PrefMenu::GroupLinkRole | PrefMenu::GroupInviteRole => {
            let link = matches!(kind, PrefMenu::GroupLinkRole);
            let (cur, roles): (String, Vec<&'static str>) = match app.group_settings() {
                Some(g) if link => (g.link_member_role.clone(), crate::LINK_JOINER_ROLES.to_vec()),
                Some(g) => (g.invite_role.clone(), crate::invite_roles_for(&g.my_role)),
                None => (String::new(), Vec::new()),
            };
            item_icons = roles.iter().map(|r| Some(*r)).collect();
            (
                if link { "Join as" } else { "Invite as" },
                roles
                    .into_iter()
                    .map(|r| {
                        let msg = if link {
                            Message::SetGroupLinkRole(r.into())
                        } else {
                            Message::SetGroupInviteRole(r.into())
                        };
                        (role_label(r).to_string(), cur.eq_ignore_ascii_case(r), msg)
                    })
                    .collect(),
            )
        }
        PrefMenu::Theme => {
            let cur = app.desktop_prefs().theme;
            (
                "Theme",
                crate::desktop::ThemePreference::ALL
                    .into_iter()
                    .map(|t| {
                        (t.label().to_string(), t == cur, Message::SetThemePreference(t),
                        )
                    })
                    .collect(),
            )
        }
        PrefMenu::LockAfter => {
            let cur = app.desktop_prefs().lock_after;
            (
                "Lock",
                crate::desktop::LockAfter::ALL
                    .into_iter()
                    .map(|t| (t.label().to_string(), t == cur, Message::SetLockAfter(t)))
                    .collect(),
            )
        }
        PrefMenu::TextSize => {
            let cur = app.desktop_prefs().text_size;
            (
                "Text size",
                crate::desktop::TextSizePreference::ALL
                    .into_iter()
                    .map(|t| (t.label().to_string(), t == cur, Message::SetTextSize(t)))
                    .collect(),
            )
        }
        PrefMenu::TimeFormat => {
            let cur = app.desktop_prefs().time_format;
            (
                "Time format",
                cursus_core::TimeFormat::ALL
                    .into_iter()
                    .map(|t| (t.label().to_string(), t == cur, Message::SetTimeFormat(t)))
                    .collect(),
            )
        }
        PrefMenu::ContactTimed => {
            let cur = app
                .contact_profile()
                .map(|p| TimedAllowChoice::from_allow(p.timed_allow));
            (
                "Disappearing messages",
                TimedAllowChoice::OPTIONS
                    .into_iter()
                    .map(|c| {
                        (
                            c.to_string(),
                            Some(c) == cur,
                            Message::SetContactTimedAllow(c.to_allow()),
                        )
                    })
                    .collect(),
            )
        }
        PrefMenu::ContactTtl | PrefMenu::ContactChatItemTtl | PrefMenu::GroupTtl => {
            let current = match kind {
                PrefMenu::ContactChatItemTtl => app
                    .contact_profile()
                    .map(|p| TtlChoice::from_secs(p.chat_item_ttl)),
                PrefMenu::ContactTtl => app.contact_profile().map(|p| TtlChoice::from_secs(Some(p.timed_ttl_secs).filter(|s| *s > 0))),
                _ => app.group_settings().map(|g| {
                    let secs = g
                        .group_preferences
                        .as_ref()
                        .and_then(|p| p.get("timedMessages"))
                        .and_then(|v| v.get("ttl"))
                        .and_then(|v| v.as_i64());
                    TtlChoice::from_secs(secs)
                }),
            };
            let title = match kind {
                PrefMenu::ContactChatItemTtl => "Delete after (this device only)",
                _ => "Delete after",
            };
            (
                title,
                TtlChoice::OPTIONS
                    .into_iter()
                    .map(|c| {
                        let msg = match kind {
                            PrefMenu::ContactChatItemTtl => {
                                Message::SetContactChatItemTtl(c.secs())
                            }
                            PrefMenu::ContactTtl => Message::SetContactTimedTtl(c.secs()),
                            _ => Message::SetGroupTimedTtl(c.secs()),
                        };
                        (c.to_string(), Some(c) == current, msg)
                    })
                    .collect(),
            )
        }
        PrefMenu::ContactRcpts => {
            let cur = app
                .contact_profile()
                .map(|p| RcptChoice::from_opt(p.send_rcpts));
            let default_on = app.receipts().0;
            (
                "Delivery receipts",
                RcptChoice::OPTIONS
                    .into_iter()
                    .map(|c| {
                        (
                            c.label(default_on),
                            Some(c) == cur,
                            Message::SetContactSendRcpts(c.to_opt()),
                        )
                    })
                    .collect(),
            )
        }
        PrefMenu::GroupRcpts => {
            let cur = RcptChoice::from_opt(app.group_send_rcpts());
            let default_on = app.receipts().1;
            (
                "Delivery receipts",
                RcptChoice::OPTIONS
                    .into_iter()
                    .map(|c| {
                        (c.label(default_on), c == cur, Message::SetGroupSendRcpts(c.to_opt()),
                        )
                    })
                    .collect(),
            )
        }
        PrefMenu::ContactFullDelete
        | PrefMenu::ContactReactions
        | PrefMenu::ContactVoice
        | PrefMenu::ContactCalls => {
            let (title, cur, to_msg): (&str, Option<FeatureAllow>, fn(FeatureAllow) -> Message) =
                match kind {
                    PrefMenu::ContactFullDelete => (
                        "Delete for everyone",
                        app.contact_profile().map(|p| p.full_delete_allow),
                        Message::SetContactFullDelete,
                    ),
                    PrefMenu::ContactReactions => (
                        "Message reactions",
                        app.contact_profile().map(|p| p.reactions_allow),
                        Message::SetContactReactions,
                    ),
                    PrefMenu::ContactVoice => (
                        "Voice messages",
                        app.contact_profile().map(|p| p.voice_allow),
                        Message::SetContactVoice,
                    ),
                    _ => (
                        "Audio and video calls",
                        app.contact_profile().map(|p| p.calls_allow),
                        Message::SetContactCalls,
                    ),
                };
            (
                title,
                FeatureAllow::FULL_OPTIONS
                    .into_iter()
                    .map(|a| (a.label().to_string(), Some(a) == cur, to_msg(a)))
                    .collect(),
            )
        }
    };

    // Title, options and the optional note share one text edge (the
    // options' own padding), so nothing in the sheet sits off by a few px.
    let mut body = Column::new().spacing(0).width(Length::Fill);
    body = body.push(
        container(text(title).size(d.meta).font(BOLD).color(theme::t().text_secondary))
            .padding(Padding { top: 12.0, right: SHEET_ITEM_PAD_X, bottom: 6.0, left: SHEET_ITEM_PAD_X }),
    );
    for (i, (label, selected, msg)) in items.into_iter().enumerate() {
        let color = if selected {
            theme::t().accent
        } else {
            theme::t().text_primary
        };
        let face: Element<'_, Message> = match item_icons.get(i).copied().flatten() {
            Some(role) => row![
                icons::role_icon(role, 18.0, color),
                text(label).size(d.body).color(color),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            None => text(label).size(d.body).color(color).into(),
        };
        let check: Element<'_, Message> = if selected {
            icons::check_icon(18.0, theme::t().accent)
        } else {
            Space::new().width(18).height(18).into()
        };
        body = body.push(
            button(
                row![container(face).width(Length::Fill), check]
                    .spacing(12)
                    .align_y(Alignment::Center),
            )
            .on_press(msg)
            .padding(Padding::from([12.0, SHEET_ITEM_PAD_X]))
            .width(Length::Fill)
            .style(theme::menu_item_r(hover, theme::INSET_CONTROL_RADIUS)),
        );
    }
    if let Some(note) = pref_menu_note(app, kind) {
        body = body.push(
            container(text(note).size(d.meta).color(theme::t().text_secondary).width(Length::Fill))
                .padding(Padding { top: 6.0, right: SHEET_ITEM_PAD_X, bottom: 12.0, left: SHEET_ITEM_PAD_X }),
        );
    }
    let card = sheet_card(body.into());
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::ClosePrefMenu);
    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));
    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

/// Capitalised role name for menus / buttons.
fn role_label(role: &str) -> &'static str {
    match role.to_ascii_lowercase().as_str() {
        "observer" => "Observer",
        "moderator" => "Moderator",
        "admin" => "Admin",
        "owner" => "Owner",
        _ => "Member",
    }
}

/// Group receipts in the message info sheet: a heading per state
/// ("Received by 3") over avatar + name rows; scrolls when long.
fn receipt_list<'a>(receipts: &'a [cursus_core::MemberReceipt], d: Density) -> Element<'a, Message> {
    let mut col = Column::new().spacing(2).width(Length::Fill);
    let mut i = 0;
    while i < receipts.len() {
        let state = receipts[i].state;
        let group: Vec<_> = receipts[i..].iter().take_while(|r| r.state == state).collect();
        i += group.len();
        col = col.push(
            container(
                row![
                    text(state.heading()).size(d.meta).font(BOLD).color(theme::t().text_secondary).width(Length::Fill),
                    text(group.len().to_string()).size(d.meta).color(theme::t().text_muted),
                ]
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 10.0, right: 0.0, bottom: 4.0, left: 0.0 }),
        );
        for r in group {
            col = col.push(
                row![
                    profile_avatar(r.image.as_deref(), &r.name, 28.0),
                    crate::ellipsis::ellipsis_text(r.name.clone(), d.body, theme::t().text_primary, 1),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .padding(Padding::from([4, 0])),
            );
        }
    }
    container(scrollable(col).height(Length::Shrink))
        .max_height(280)
        .width(Length::Fill)
        .into()
}

/// Horizontal text inset inside a choice sheet (title, options, note).
const SHEET_ITEM_PAD_X: f32 = 16.0;

/// The explanation that belongs to a choice: shown under the options in
/// the sheet instead of under the row on the page.
fn pref_menu_note(app: &App, kind: PrefMenu) -> Option<String> {
    let p = app.contact_profile();
    let peer = |f: fn(&crate::ContactProfileOverlay) -> FeatureAllow| {
        p.map(|p| f(p).peer_note(&p.title))
    };
    match kind {
        PrefMenu::ContactRcpts => Some("Let this contact know their messages arrived.".into()),
        PrefMenu::GroupRcpts => Some("Let members know their messages arrived.".into()),
        PrefMenu::ContactTimed => peer(|p| p.timed_contact_allows),
        PrefMenu::ContactFullDelete => peer(|p| p.full_delete_contact_allows),
        PrefMenu::ContactReactions => peer(|p| p.reactions_contact_allows),
        PrefMenu::ContactVoice => peer(|p| p.voice_contact_allows),
        PrefMenu::ContactCalls => peer(|p| p.calls_contact_allows),
        _ => None,
    }
}

/// Card of a choice sheet: the panel's 4 px inset, so options highlight
/// as inset rounded rows exactly like rows in a page panel.
fn sheet_card<'a>(body: Element<'a, Message>) -> Element<'a, Message> {
    container(crate::tap::guard(body))
        .padding(crate::preference_ui::PANEL_PAD)
        .width(Length::Fill)
        .max_width(400)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: theme::CONTROL_RADIUS.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn menu_card<'a>(body: Element<'a, Message>, d: Density) -> Element<'a, Message> {
    container(crate::tap::guard(body))
        .padding(d.pad)
        .width(Length::Fill)
        .max_width(400)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: theme::CONTROL_RADIUS.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn message_menu_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(menu) = app.message_menu() else {
        return vertical().height(0).into();
    };
    let item_id = menu.item_id;

    let msg = app.message_by_id(item_id);
    let is_deleted = msg.map(|m| m.is_deleted()).unwrap_or(false);

    let card: Element<'_, Message> = match &menu.pane {
        MsgMenuPane::Actions => {
            let mut body = Column::new().spacing(8).width(Length::Fill);
            if !is_deleted {
                body = body.push(reaction_picker_row(item_id, d, hover));
            }
            body = body.push(
                text("Message")
                    .size(d.meta)
                    .color(theme::t().text_muted),
            );
            let revealed = app.is_deleted_revealed(item_id);
            let can_reveal = msg
                .and_then(|m| m.deleted)
                .is_some_and(cursus_core::DeletedState::can_reveal);
            if can_reveal {
                body = body.push(icon_menu_action(
                    icons::history_icon(18.0, theme::t().text_primary),
                    if revealed { "Hide" } else { "Reveal" },
                    Message::MsgMenuAction(MsgMenuAction::ToggleReveal),
                    hover,
                    d,
                    false,
                ));
            }
            let can_download = msg
                .and_then(|m| m.file.as_ref())
                .map(|f| {
                    matches!(
                        f.kind,
                        FileKind::Image | FileKind::Video | FileKind::Voice | FileKind::File
                    ) && f
                        .local_path
                        .as_ref()
                        .map(|p| p.is_file())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if can_download && !is_deleted {
                body = body.push(icon_menu_action(
                    icons::download_icon(18.0, theme::t().text_primary),
                    "Download",
                    Message::MsgMenuAction(MsgMenuAction::Download),
                    hover,
                    d,
                    false,
                ));
            }
            let outgoing = msg.map(|m| m.outgoing).unwrap_or(false);
            let can_edit = msg.map(|m| m.can_edit_text()).unwrap_or(false);
            let actions: &[(&str, MsgMenuAction, bool)] = if is_deleted {
                &[
                    ("Info", MsgMenuAction::Info, false),
                    ("Delete for me", MsgMenuAction::Delete, true),
                    ("Select", MsgMenuAction::Select, false),
                ]
            } else if outgoing && can_edit {
                &[
                    ("Reply", MsgMenuAction::Reply, false),
                    ("Copy", MsgMenuAction::Copy, false),
                    ("Edit", MsgMenuAction::Edit, false),
                    ("Forward", MsgMenuAction::Forward, false),
                    ("Info", MsgMenuAction::Info, false),
                    ("Delete for everyone", MsgMenuAction::Delete, true),
                    ("Select", MsgMenuAction::Select, false),
                ]
            } else if outgoing {
                &[
                    ("Reply", MsgMenuAction::Reply, false),
                    ("Copy", MsgMenuAction::Copy, false),
                    ("Forward", MsgMenuAction::Forward, false),
                    ("Info", MsgMenuAction::Info, false),
                    ("Delete for everyone", MsgMenuAction::Delete, true),
                    ("Select", MsgMenuAction::Select, false),
                ]
            } else {
                &[
                    ("Reply", MsgMenuAction::Reply, false),
                    ("Copy", MsgMenuAction::Copy, false),
                    ("Forward", MsgMenuAction::Forward, false),
                    ("Info", MsgMenuAction::Info, false),
                    ("Delete for me", MsgMenuAction::Delete, true),
                    ("Select", MsgMenuAction::Select, false),
                ]
            };
            for (label, action, danger) in actions {
                let icon_color = if *danger {
                    theme::t().danger
                } else {
                    theme::t().text_primary
                };
                let icon = msg_menu_icon(*action, 18.0, icon_color);
                body = body.push(icon_menu_action(
                    icon,
                    label,
                    Message::MsgMenuAction(*action),
                    hover,
                    d,
                    *danger,
                ));
            }
            menu_card(body.into(), d)
        }
        MsgMenuPane::ConfirmDelete => {
            let outgoing = msg.map(|m| m.outgoing).unwrap_or(false);
            let full = app.selected_full_delete_for_user();
            let (title, blurb) = if !outgoing {
                (
                    "Delete for me?",
                    "Removes the message only on this device. The sender still has it.",
                )
            } else if full {
                (
                    "Delete for everyone?",
                    "Full deletion is on — the message will be removed for you and the contact.",
                )
            } else {
                (
                    "Delete for everyone?",
                    "Contact has not allowed Full deletion — they will only see it marked deleted (content may remain). Enable Full deletion on both sides for a real wipe.",
                )
            };
            menu_card(
                column![
                    text(title)
                        .size(d.body)
                        .color(theme::t().text_primary),
                    text(blurb)
                        .size(d.meta)
                        .color(theme::t().text_secondary),
                    row![
                        button(text("Cancel").size(d.meta))
                            .on_press(Message::MsgMenuCancelConfirm)
                            .padding(Padding::from([10, 14]))
                            .style(theme::secondary_action(hover)),
                        button(text("Delete").size(d.meta))
                            .on_press(Message::MsgMenuConfirmDelete)
                            .padding(Padding::from([10, 14]))
                            .style(theme::danger_action(hover)),
                    ]
                    .spacing(8),
                ]
                .spacing(12)
                .into(),
                d,
            )
        }
        MsgMenuPane::ForwardPick => {
            let mut list = Column::new().spacing(4).width(Length::Fill);
            list = list.push(
                text("Forward to…")
                    .size(d.body)
                    .color(theme::t().text_primary),
            );
            for (i, chat) in app.chats().iter().enumerate() {
                if app.selected() == Some(i) {
                    continue;
                }
                list = list.push(menu_action(
                    chat.title.as_str(),
                    Message::ForwardToChat(i),
                    hover,
                    d,
                    false,
                ));
            }
            list = list.push(
                button(text("Cancel").size(d.meta))
                    .on_press(Message::CloseMessageMenu)
                    .padding(Padding::from([10, 14]))
                    .style(theme::secondary_action(hover)),
            );
            menu_card(list.into(), d)
        }
        MsgMenuPane::Info {
            item_id,
            time,
            status,
            text: info_text,
            file_detail,
            receipts,
        } => {
            // Label · value rows like the Mac sheet; words, not core codes.
            let _ = item_id;
            let outgoing = msg.is_some_and(|m| m.outgoing);
            let mut info_col = Column::new().spacing(8).width(Length::Fill);
            info_col = info_col.push(
                text("Message info").size(d.body).font(BOLD).color(theme::t().text_primary),
            );
            let info_row = |label: &'static str, value: Element<'static, Message>| -> Element<'static, Message> {
                row![
                    text(label).size(d.meta).color(theme::t().text_secondary).width(Length::Fixed(72.0)),
                    container(value).width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Start)
                .into()
            };
            if let Some(when) = time.as_deref().and_then(parse_local_ts) {
                info_col = info_col.push(info_row(
                    if outgoing { "Sent" } else { "Received" },
                    txt(clock().date_time(&when)).size(d.meta).color(theme::t().text_primary).into(),
                ));
            }
            if let Some(st) = status {
                info_col = info_col.push(info_row("Status", txt(st.clone()).size(d.meta).color(theme::t().text_primary).into()));
            }
            if let Some(file) = file_detail {
                info_col = info_col.push(info_row("File", txt(file.clone()).size(d.meta).color(theme::t().text_primary).into()));
            }
            if !info_text.is_empty() {
                let preview = truncate_graphemes(info_text, 160);
                let value: Element<'_, Message> = if cursus_core::is_short_emoji(info_text) {
                    emoji_glyph(preview).size(d.body).color(Color::WHITE).into()
                } else {
                    txt(preview).size(d.meta).color(theme::t().text_primary).into()
                };
                info_col = info_col.push(info_row("Text", value));
            }
            if !receipts.is_empty() {
                info_col = info_col.push(receipt_list(receipts, d));
            }
            info_col = info_col.push(
                button(text("Close").size(d.meta))
                    .on_press(Message::CloseMessageMenu)
                    .padding(Padding::from([10, 14]))
                    .style(theme::secondary_action(hover)),
            );
            menu_card(info_col.into(), d)
        }
    };

    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseMessageMenu);

    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));

    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AvatarFallback {
    /// First letter of the label.
    Initial,
    /// User-group glyph (default private-group picture).
    GroupIcon,
    /// Antenna glyph (default channel picture).
    ChannelIcon,
}

fn profile_avatar<'a>(
    image_data_url: Option<&str>,
    label: &str,
    size: f32,
) -> Element<'a, Message> {
    profile_avatar_fallback(image_data_url, label, size, AvatarFallback::Initial)
}

fn profile_avatar_fallback<'a>(
    image_data_url: Option<&str>,
    label: &str,
    size: f32,
    fallback: AvatarFallback,
) -> Element<'a, Message> {
    if let Some(url) = image_data_url.filter(|s| !s.is_empty()) {
        // Render at 2× for sharper circles on HiDPI; iced clip is rectangular.
        let px = (size * 2.0).round().max(32.0) as u32;
        // Cached Handle — group chats re-view on pointer moves; re-decoding
        // every member avatar each time made groups feel uniquely laggy.
        if let Some(handle) = crate::avatar_cache::handle(url, px) {
            return container(
                image(handle)
                    .width(size)
                    .height(size)
                    .content_fit(ContentFit::Fill),
            )
            .width(size)
            .height(size)
            .center_x(size)
            .center_y(size)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: theme::t().border,
                    width: 1.0,
                    radius: (size * 0.5).into(),
                },
                ..container::Style::default()
            })
            .into();
        }
    }
    match fallback {
        AvatarFallback::Initial => icons::avatar_initial(label, size),
        AvatarFallback::GroupIcon => icons::group_avatar_disk(size),
        AvatarFallback::ChannelIcon => icons::channel_avatar_disk(size),
    }
}

fn chat_avatar_fallback(chat: &cursus_core::ChatPreview) -> AvatarFallback {
    if chat.is_channel {
        AvatarFallback::ChannelIcon
    } else if matches!(chat.id.chat_type, ChatType::Group) {
        AvatarFallback::GroupIcon
    } else {
        AvatarFallback::Initial
    }
}

/// Badge diameter/width shared by [`unread_count_badge`] and the preview fit.
fn unread_badge_size(count: i64, d: Density) -> (String, f32, f32) {
    let label = if count <= 0 {
        "·".to_string()
    } else if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let diameter = (d.meta + 10.0).clamp(20.0, 24.0);
    let width = if label.len() > 1 {
        (diameter + d.meta * 0.55 * (label.len().saturating_sub(1) as f32)).max(diameter)
    } else {
        diameter
    };
    (label, diameter, width)
}

/// Rough advance of one grapheme in Noto Sans, in em.
pub(crate) fn grapheme_em(g: &str) -> f32 {
    let mut chars = g.chars();
    let Some(c) = chars.next() else { return 0.0 };
    if chars.next().is_some() && !c.is_ascii() {
        return 1.2; // emoji sequences / combining clusters
    }
    match c {
        ' ' => 0.27,
        '\t' => 1.0,
        'i' | 'l' | 'j' | 'I' | '.' | ',' | '\'' | '!' | '|' | ':' | ';' | '`' => 0.27,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '/' | '-' => 0.36,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.86,
        'A'..='Z' => 0.66,
        '0'..='9' => 0.57,
        c if c.is_ascii() => 0.54,
        // CJK, emoji and other wide scripts.
        c if (c as u32) >= 0x1100 => 1.15,
        _ => 0.6,
    }
}

/// Unread count pill — solid accent; grey when the chat is muted.
fn unread_count_badge<'a>(count: i64, muted: bool, d: Density) -> Element<'a, Message> {
    let (label, diameter, width) = unread_badge_size(count, d);
    container(
        txt(label)
            .size(d.meta)
            .font(BOLD)
            .color(if muted {
                theme::t().bg_list
            } else {
                theme::t().accent_on
            })
            .line_height(iced::widget::text::LineHeight::Absolute(diameter.into())),
    )
    .width(width)
    .height(diameter)
    .center_x(width)
    .center_y(diameter)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(if muted {
            theme::t().text_muted
        } else {
            theme::t().accent
        })),
        border: iced::Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: (diameter * 0.5).into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// Chat / channel list avatar: larger disk; request bubble only for inbound
/// contact requests (unknown senders).
fn chat_list_avatar<'a>(chat: &cursus_core::ChatPreview, size: f32) -> Element<'a, Message> {
    let avatar = profile_avatar_fallback(
        chat.peer_profile.image.as_deref(),
        &chat.title,
        size,
        chat_avatar_fallback(chat),
    );

    let badge_icon: Option<Element<'_, Message>> = if chat.pending_request_id.is_some() {
        Some(icons::user_question_icon(11.0, theme::t().text_primary))
    } else if matches!(chat.id.chat_type, ChatType::Group) && !chat.is_channel {
        Some(icons::user_group_icon(11.0, theme::t().text_primary))
    } else {
        None
    };

    let badge_size = (size * 0.38).clamp(16.0, 22.0);
    // Same slot width with or without a badge so names line up in the list.
    let frame = size + badge_size * 0.22;
    let Some(badge_icon) = badge_icon else {
        return container(avatar)
            .width(frame)
            .height(frame)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start)
            .into();
    };

    let badge = container(badge_icon)
        .width(badge_size)
        .height(badge_size)
        .center_x(badge_size)
        .center_y(badge_size)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.5,
                radius: (badge_size * 0.5).into(),
            },
            ..container::Style::default()
        });

    // Slight overhang so the bubble sits on the disk edge.
    Stack::new()
        .width(frame)
        .height(frame)
        .push(
            container(avatar)
                .width(frame)
                .height(frame)
                .align_x(Alignment::Start)
                .align_y(Alignment::Start),
        )
        .push(
            container(badge)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End)
                .align_y(Alignment::End),
        )
        .into()
}

/// Squarish header action — same 32px hit as mobile back / member menus.
fn round_header_icon_btn(
    icon: Element<'static, Message>,
    on_press: Message,
    _hover: bool,
) -> Element<'static, Message> {
    composer_hit(icon, 32.0, on_press)
}

const PROFILE_HEADER_ICON: f32 = 22.0;

/// One of a profile's quick actions (call, star, mute, settings).
#[derive(Clone, Copy)]
enum Quick {
    Call,
    Star(bool),
    Mute(bool),
    Settings,
}

impl Quick {
    fn icon(self, hover: bool) -> Element<'static, Message> {
        match self {
            Quick::Call => icons::phone_tool(PROFILE_HEADER_ICON, hover),
            Quick::Star(on) => icons::star_tool(PROFILE_HEADER_ICON, on, hover),
            Quick::Mute(on) => icons::mute_tool(PROFILE_HEADER_ICON, on, hover),
            Quick::Settings => icons::settings_tool(PROFILE_HEADER_ICON, hover),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quick::Call => "Call",
            Quick::Star(true) => "Starred",
            Quick::Star(false) => "Star",
            Quick::Mute(true) => "Muted",
            Quick::Mute(false) => "Mute",
            Quick::Settings => "Settings",
        }
    }
}

/// Phones (no list pane): the quick actions are big tiles under the
/// profile, spanning the page like the panels below them. Wider windows:
/// small icon tools in the page's header bar, like a chat header.
/// Returns (header tools, tiles for the page body).
fn profile_quick_actions(
    actions: Vec<(Quick, Message)>,
    d: Density,
    hover: bool,
) -> (Element<'static, Message>, Option<Element<'static, Message>>) {
    if d.list_width != 0.0 {
        let mut tools = Row::new().spacing(4).align_y(Alignment::Center);
        for (kind, on_press) in actions {
            tools = tools.push(composer_hit(kind.icon(hover), 36.0, on_press));
        }
        return (tools.into(), None);
    }
    let mut row = Row::new().spacing(8);
    for (kind, on_press) in actions {
        let face = column![
            icons::centered_hit(kind.icon(hover), 24.0),
            ellipsis_text(kind.label().to_string(), d.meta, theme::t().text_secondary, 1),
        ]
        .spacing(6)
        .align_x(Alignment::Center)
        .width(Length::Fill);
        row = row.push(crate::hover::action_glow(
            button(face)
                .on_press(on_press)
                .padding(Padding::from([12, 4]))
                .width(Length::Fill)
                .style(theme::profile_action(hover)),
            24.0,
        ));
    }
    (Space::new().width(0).into(), Some(row.width(Length::Fill).into()))
}

/// A long or multi-line short description reads as a paragraph: it goes in
/// its own block under the actions instead of being centred in the hero.
fn is_paragraph(value: &str) -> bool {
    value.contains('\n') || value.chars().count() > 64
}

/// A centred secondary line of a profile header (bio, full name).
fn hero_line<'a>(value: String, d: Density) -> Element<'a, Message> {
    container(
        profile_rich(&value, d.body, theme::t().text_secondary, false).align_x(Alignment::Center),
    )
    .padding(Padding::from([0, 12]))
    .center_x(Length::Fill)
    .into()
}

/// A paragraph of a profile under its section label.
fn about_block<'a>(label: &'a str, value: String, d: Density) -> Element<'a, Message> {
    column![
        card_title(vertical().height(0).into(), label, "", d),
        container(profile_rich(&value, d.body, theme::t().text_primary, false))
            .padding(Padding::from([2, 12]))
            .width(Length::Fill),
    ]
    .into()
}

/// Title on the left; round actions flush to the right edge.
/// A profile page whose top bar floats like the chat's: the bar on a glass
/// card inset from the top and sides, the page scrolling underneath it and
/// keeping CHAT_HEADER_ROOM at its top.
fn floating_header_page<'a>(
    header: Element<'a, Message>,
    body: impl Into<Element<'a, Message>>,
    d: Density,
) -> Element<'a, Message> {
    let side = if d.list_width == 0.0 { 6.0 } else { 10.0 };
    let bar = container(crate::glass::surface(
        container(header)
            .height(CHAT_HEADER_HEIGHT)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .style(theme::floating_nav_style()),
        theme::CONTROL_RADIUS,
    ))
    .padding(Padding { top: CHAT_HEADER_INSET, right: side, bottom: 0.0, left: side })
    .width(Length::Fill);
    let scroll = page_scroll_y(
        container(container(body).max_width(PAGE_MAX).width(Length::Fill))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(Padding { top: CHAT_HEADER_ROOM, right: d.pad, bottom: d.pad, left: d.pad }),
    );
    Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(scroll)
        .push(bar)
        .into()
}

fn profile_header_bar<'a>(
    leading: Element<'a, Message>,
    actions: Element<'a, Message>,
    d: Density,
    with_back: bool,
) -> Element<'a, Message> {
    // Inside its floating card: Back (or the title) 8 px from the edge.
    let _ = with_back;
    let pad = Padding { top: d.pad_tight, right: d.pad, bottom: d.pad_tight, left: 8.0 };
    row![
        container(leading)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        actions,
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .height(56)
    .padding(Padding {
        top: 0.0,
        bottom: 0.0,
        ..pad
    })
    .into()
}

fn contact_profile_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(p) = app.contact_profile() else {
        return vertical().height(0).into();
    };

    if matches!(p.pane, crate::ContactProfilePane::Preferences) {
        return contact_preferences_page(app, p);
    }

    let display = if p.fields.display_name.is_empty() {
        p.title.clone()
    } else {
        p.fields.display_name.clone()
    };

    let (tools, tiles) = profile_quick_actions(
        vec![
            (Quick::Call, Message::StartAudioCall(p.contact_id)),
            (Quick::Star(p.favorite), Message::ContactProfileFavorite),
            (Quick::Mute(p.muted), Message::ContactProfileMute),
            (Quick::Settings, Message::OpenContactPreferences),
        ],
        d,
        hover,
    );

    let leading = page_title("Contact", with_back.then_some(Message::CloseContactProfile), d, hover);
    let header = profile_header_bar(leading, tools, d, with_back);

    let mut body = Column::new().spacing(PAGE_GAP).width(Length::Fill);
    {
        let mut lines = vec![
            profile_name_text(display.clone(), d),
            editable_line(
                &p.alias,
                "Add a local name",
                LineStyle::Sub,
                crate::EditField::ContactAlias,
                d,
                hover,
            ),
        ];
        if !p.fields.full_name.is_empty() {
            lines.push(container(text(p.fields.full_name.clone()).size(d.body).color(theme::t().text_secondary))
                .padding(Padding::from([0, 12])).center_x(Length::Fill).into());
        }
        if !p.fields.short_descr.is_empty() { lines.push(hero_line(p.fields.short_descr.clone(), d)); }
        body = body.push(profile_identity(
            profile_avatar(p.fields.image.as_deref(), &display, PROFILE_AVATAR),
            None,
            lines,
        ));
        if let Some(tiles) = tiles {
            body = body.push(tiles);
        }
        if !p.fields.description.is_empty() {
            body = body.push(about_block("About", p.fields.description.clone(), d));
        }
    }
    let narrow = matches!(app.layout(), LayoutMode::Mobile) || app.window_width() < 720.0;
    body = body.push(button_card(contact_danger_actions(d, hover, narrow)));

    floating_header_page(header, body, d)
}

fn contact_preferences_page<'a>(
    app: &'a App,
    p: &'a crate::ContactProfileOverlay,
) -> Element<'a, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    // Same header with or without a parent pane: the page always has Back.
    let leading = page_title("Settings", Some(Message::CloseContactPreferences), d, hover);
    let header = profile_header_bar(leading, Space::new().width(0).into(), d, true);

    let receipts = choice_row(
        Some(icons::check_read(20.0, theme::t().accent)), "Delivery receipts",
        RcptChoice::from_opt(p.send_rcpts).label(app.receipts().0),
        None,
        Message::OpenPrefMenu(PrefMenu::ContactRcpts), d, hover,
    );
    let body = column![
        crate::preference_ui::group(Some("Privacy"), None, receipts, d),
        crate::preference_ui::group(Some("Chat permissions"), None, contact_feature_prefs(p, d, hover), d),
    ].spacing(20);

    floating_header_page(header, body, d)
}

fn reaction_members_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(overlay) = app.reaction_members() else {
        return vertical().height(0).into();
    };
    let glyph = if overlay.emoji == "❤" || overlay.emoji == "❤️" {
        "❤️".to_string()
    } else {
        overlay.emoji.clone()
    };
    let mut body = Column::new().spacing(6).width(Length::Fill);
    body = body.push(
        row![
            emoji_glyph(glyph).size(d.body).color(Color::WHITE),
            text("Reacted")
                .size(d.body)
                .color(theme::t().text_primary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    if overlay.loading {
        body = body.push(
            text("Loading…")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else if let Some(err) = overlay.error.as_ref() {
        body = body.push(text(err.clone()).size(d.meta).color(theme::t().danger));
    } else if overlay.names.is_empty() {
        body = body.push(
            text("No members listed.")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else {
        for name in &overlay.names {
            body = body.push(
                text(name.clone())
                    .size(d.body)
                    .color(theme::t().text_primary),
            );
        }
    }
    body = body.push(
        button(text("Close").size(d.meta))
            .on_press(Message::CloseReactionMembers)
            .padding(Padding::from([10, 14]))
            .style(theme::secondary_action(hover)),
    );
    let card = menu_card(body.into(), d);
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseReactionMembers);

    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));

    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

fn open_url_confirm_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(pending) = app.pending_url() else {
        return vertical().height(0).into();
    };
    let url = pending.url.clone();
    let (title, headline, note, action) = match pending.simplex {
        Some(kind) => {
            let hosts: Vec<String> = cursus_core::link_host(&url).into_iter().collect();
            (
                kind.join_prompt(),
                cursus_core::simplex_link_label(kind, &hosts),
                "You will connect via this SimpleX link. Only continue if you trust the sender.",
                kind.join_action(),
            )
        }
        None => (
            "Open this link?",
            url.clone(),
            "It will open in your web browser. Only continue if you trust the sender and the destination.",
            "Open",
        ),
    };
    let mut body = column![
        text(title).size(d.body).color(theme::t().text_primary),
        text(headline)
            .size(d.meta)
            .color(theme::t().accent)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(12);
    if pending.simplex.is_some() {
        // Full link, muted, so the user can still inspect what they join.
        let shown: String = if url.chars().count() > 120 {
            url.chars().take(117).chain("…".chars()).collect()
        } else {
            url.clone()
        };
        body = body.push(
            text(shown)
                .size(d.meta)
                .color(theme::t().text_muted)
                .wrapping(Wrapping::Glyph),
        );
    }
    let card = menu_card(
        body.push(text(note).size(d.meta).color(theme::t().text_secondary))
            .push(
                row![
                    button(text("Cancel").size(d.meta))
                        .on_press(Message::CancelOpenUrl)
                        .padding(Padding::from([10, 14]))
                        .style(theme::secondary_action(hover)),
                    button(text(action).size(d.meta))
                        .on_press(Message::ConfirmOpenUrl)
                        .padding(Padding::from([10, 14]))
                        .style(theme::primary_action(hover)),
                ]
                .spacing(8),
            )
            .into(),
        d,
    );

    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CancelOpenUrl);

    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));

    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

/// Warning sheet for settings / downloads that lower privacy.
fn privacy_confirm_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(confirm) = app.privacy_confirm() else {
        return vertical().height(0).into();
    };
    let (title, body, action): (&str, String, &str) = match confirm {
        crate::PrivacyConfirm::ShowNotificationText => (
            "Show message text in notifications?",
            "Anyone who can see your screen will be able to read incoming messages, \
             and many desktops keep notifications in a history that stays after Cursus closes."
                .into(),
            "Show text",
        ),
        crate::PrivacyConfirm::CloseToTray => (
            "Keep Cursus running in the tray?",
            "Closing the window will hide Cursus instead of quitting. It stays unlocked, and \
             decrypted photos, videos and voice messages you opened stay in its private cache \
             until you quit from the tray."
                .into(),
            "Hide to tray",
        ),
        crate::PrivacyConfirm::StopProtectingIp => (
            "Stop protecting your IP address?",
            "Files will download from any server the sender chooses, including automatic \
             downloads. Without Tor or VPN, those servers see your IP address."
                .into(),
            "Turn off",
        ),
        crate::PrivacyConfirm::DisableAppLock => (
            "Turn off app lock?",
            "Anyone with access to this computer while Cursus is running can read your chats, \
             including when it's hidden in the tray."
                .into(),
            "Turn off",
        ),
        crate::PrivacyConfirm::ShowChatPreviews => (
            "Show message previews?",
            "The last message of every chat will be visible in the chat list to anyone who can \
             see your screen."
                .into(),
            "Show previews",
        ),
        crate::PrivacyConfirm::UnblurMedia => (
            "Stop blurring media?",
            "Photos and videos will be shown in full in your chats, visible to anyone who can see \
             your screen."
                .into(),
            "Show media",
        ),
        crate::PrivacyConfirm::AutoDownload => (
            "Download media automatically?",
            "Images, voice messages and short videos from anyone who writes to you will download \
             without asking — including from strangers in groups."
                .into(),
            "Download automatically",
        ),
        crate::PrivacyConfirm::KeepTrackingLinks => (
            "Keep tracking codes in links?",
            "Links you send and open will keep codes like utm_source or fbclid, which let \
             websites track who shared them and where you came from."
                .into(),
            "Keep codes",
        ),
        crate::PrivacyConfirm::StopEncryptingFiles => (
            "Store new files unencrypted?",
            "Photos, videos, voice messages and files you send or receive from now on will be saved \
             readable on disk. Files already stored stay encrypted."
                .into(),
            "Store unencrypted",
        ),
        crate::PrivacyConfirm::SendReceipts { groups } => (
            "Send delivery receipts?",
            if *groups {
                "Members of small groups will see when their messages reach this device.".into()
            } else {
                "Your contacts will see when their messages reach this device.".into()
            },
            "Send receipts",
        ),
        crate::PrivacyConfirm::EnableKeyringUnlock { .. } => (
            "Unlock automatically?",
            "Your database password will be kept in your desktop's keyring (GNOME Keyring, KWallet…), \
             which is unlocked when you log in. Cursus will then open without asking — anyone \
             using your logged-in session can read your chats. Enter your password to confirm."
                .into(),
            "Store in keyring",
        ),
        crate::PrivacyConfirm::UnknownFileServers { servers, .. } => (
            "Download from unknown servers?",
            format!(
                "This file is on servers you don't use. Without Tor or VPN, your IP address will be \
                 visible to: {}.",
                if servers.is_empty() { "unknown".to_string() } else { servers.join(", ") }
            ),
            "Download",
        ),
    };
    let mut content = column![
        text(title)
            .size(d.body)
            .font(BOLD)
            .color(theme::t().text_primary),
        text(body)
            .size(d.meta)
            .color(theme::t().text_secondary)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(12)
    .max_width(420);
    let mut busy = false;
    if let crate::PrivacyConfirm::EnableKeyringUnlock {
        password,
        checking,
        error,
    } = confirm
    {
        busy = *checking;
        content = content.push(
            text_input("Database password", password.as_str())
                .id(iced::widget::Id::new("keyring-password"))
                .on_input(|s| Message::KeyringPasswordChanged(crate::Secret::new(s)))
                .on_submit(Message::ConfirmPrivacy)
                .secure(true)
                .padding(Padding::from([12, 14]))
                .size(d.body)
                .style(theme::rounded_field()),
        );
        if let Some(err) = error {
            content = content.push(text(err.clone()).size(d.meta).color(theme::t().danger));
        }
    }
    let confirm_btn = button(text(if busy { "Checking…" } else { action }).size(d.meta))
        .padding(Padding::from([10, 14]))
        .style(theme::danger_action(hover));
    let confirm_btn = if busy {
        confirm_btn
    } else {
        confirm_btn.on_press(Message::ConfirmPrivacy)
    };
    content = content.push(
        row![
            button(text("Cancel").size(d.meta))
                .on_press(Message::CancelPrivacy)
                .padding(Padding::from([10, 14]))
                .style(theme::secondary_action(hover)),
            confirm_btn,
        ]
        .spacing(8),
    );
    let card = menu_card(content.into(), d);
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CancelPrivacy);
    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));
    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

fn msg_menu_icon<'a>(action: MsgMenuAction, size: f32, color: Color) -> Element<'a, Message> {
    match action {
        MsgMenuAction::Reply => icons::reply_icon(size, color),
        MsgMenuAction::Copy => icons::copy_icon(size, color),
        MsgMenuAction::Edit => icons::edit_icon(size, color),
        MsgMenuAction::Forward => icons::forward_icon(size, color),
        MsgMenuAction::Info => icons::info_icon(size, color),
        MsgMenuAction::Delete => icons::trash_icon(size, color),
        MsgMenuAction::Select => icons::select_icon(size, color),
        MsgMenuAction::Download => icons::download_icon(size, color),
        MsgMenuAction::ToggleReveal => icons::history_icon(size, color),
    }
}

fn menu_action<'a>(
    label: &'a str,
    on_press: Message,
    hover_ok: bool,
    d: Density,
    danger: bool,
) -> Element<'a, Message> {
    let color = if danger {
        theme::t().danger
    } else {
        theme::t().text_primary
    };
    button(text(label).size(d.body).color(color))
        .on_press(on_press)
        .padding(Padding::from([12, 12]))
        .width(Length::Fill)
        .style(theme::menu_item(hover_ok))
        .into()
}

fn icon_menu_action<'a>(
    icon: Element<'a, Message>,
    label: &'a str,
    on_press: Message,
    hover_ok: bool,
    d: Density,
    danger: bool,
) -> Element<'a, Message> {
    let color = if danger {
        theme::t().danger
    } else {
        theme::t().text_primary
    };
    let label_row = row![icon, text(label).size(d.body).color(color)]
        .spacing(10)
        .align_y(Alignment::Center);
    button(label_row)
        .on_press(on_press)
        .padding(Padding::from([12, 12]))
        .width(Length::Fill)
        .style(theme::menu_item(hover_ok))
        .into()
}

/// Shared empty detail for the four destinations: one focal point and one action.
fn empty_detail<'a>(
    icon: Element<'a, Message>,
    title: &'a str,
    body: &'a str,
    action: Option<(&'a str, Message)>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let mut content = column![
        icon,
        text(title)
            .size(d.title + 2.0)
            .font(BOLD)
            .color(theme::t().text_primary),
        text(body)
            .size(d.body)
            .color(theme::t().text_secondary)
            .align_x(Alignment::Center),
    ]
    .spacing(14)
    .align_x(Alignment::Center);
    if let Some((label, message)) = action {
        content = content.push(
            button(text(label).size(d.body))
                .on_press(message)
                .padding(Padding::from([12, 20]))
                .style(theme::primary_action(hover)),
        );
    }
    container(container(content).max_width(360).padding(d.pad))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn placeholder<'a>(title: &'a str, body: &'a str, d: Density) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .size(d.title)
                .font(BOLD)
                .color(theme::t().text_primary),
            text(body).size(d.meta).color(theme::t().text_secondary),
        ]
        .spacing(8)
        .padding(d.pad),
    )
    .width(Length::Fill)
    .into()
}

fn chat_type_label(chat: &cursus_core::ChatPreview) -> &'static str {
    if chat.pending_request_id.is_some() {
        return "Contact request";
    }
    if chat.is_channel {
        return "Channel";
    }
    match chat.id.chat_type {
        cursus_core::ChatType::Direct => "Direct",
        cursus_core::ChatType::Group => "Group",
        cursus_core::ChatType::Local => "Notes",
        cursus_core::ChatType::Other => "Chat",
    }
}

fn calls_panel(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let mut col = Column::new().spacing(0).padding(Padding {
        top: 0.0,
        right: d.pad_tight,
        bottom: d.pad_tight,
        left: d.pad_tight,
    });

    if let Some(card) = call_card(app, false) {
        col = col.push(card);
    }

    let contacts_by_id: std::collections::HashMap<_, _> = app
        .chats()
        .iter()
        .filter(|chat| matches!(chat.id.chat_type, ChatType::Direct))
        .map(|chat| (chat.id.api_id, chat))
        .collect();
    let history = app.call_history();
    if app.call.history_loading() {
        col = col.push(
            container(
                text(if history.is_empty() {
                    "Loading recent calls…"
                } else {
                    "Loading earlier calls…"
                })
                .size(d.meta)
                .color(theme::t().text_muted),
            )
            .padding(Padding::from([10, 4])),
        );
    }
    if !history.is_empty() {
        col = col.push(calls_section_label("Recent", d));
        for rec in history {
            col = col.push(call_history_row(
                contacts_by_id.get(&rec.contact_id).copied(),
                rec,
                d,
                hover,
            ));
        }
    }

    if let Some(error) = app.call.history_error() {
        col = col.push(row![
            text(error).size(d.meta).color(theme::t().danger),
            button(text("Retry").size(d.meta)).on_press(Message::Call(crate::call_flow::CallMsg::ReloadHistory)),
        ].spacing(8).align_y(Alignment::Center));
    } else if history.is_empty() && !app.call.history_loading() {
        col = col.push(placeholder("No recent calls", "Your recent calls will appear here.", d));
    }
    page_scroll_y(col.push(vertical().height(nav_clearance(d))))
}

fn call_contacts_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let mut contacts = Column::new().spacing(4).width(Length::Fill);
    let query = app.call_contact_search.to_lowercase();
    let matches: Vec<_> = app.direct_contacts().into_iter()
        .filter(|(_, chat)| chat.title.to_lowercase().contains(&query)).collect();
    let count = matches.len();
    for (_, chat) in matches {
        contacts = contacts.push(button(row![
            profile_avatar(chat.peer_profile.image.as_deref(), &chat.title, 40.0),
            text(chat.title.clone()).size(d.body).color(theme::t().text_primary),
        ].spacing(12).align_y(Alignment::Center))
            .on_press(Message::StartAudioCall(chat.id.api_id)).padding(10).width(Length::Fill)
            .style(theme::ghost_tile(hover)));
    }
    if count == 0 {
        contacts = contacts.push(text(if query.is_empty() { "Add a contact to start a call." } else { "No matching contacts." }).size(d.meta));
    }
    let list_height = (count.max(1) as f32 * 64.0).min(360.0).min((app.height - 240.0).max(64.0));
    let content = column![
        text("New audio call").size(d.title).color(theme::t().text_primary),
        text_input("Search contacts", &app.call_contact_search).on_input(Message::CallContactSearch).padding(10).style(theme::rounded_field()),
        container(scroll_y(contacts)).height(list_height),
        button(text("Cancel")).on_press(Message::CallContacts(false)).padding(10).style(theme::secondary_action(hover)),
    ].spacing(12);
    sheet_overlay(menu_card(content.into(), d), Message::CallContacts(false))
}

fn calls_section_label(label: &str, d: Density) -> Element<'static, Message> {
    // List headings sit at the list's row inset (12), not a panel's.
    container(text(label.to_string()).size(d.meta).font(BOLD).color(theme::t().text_secondary))
        .padding(Padding { top: 14.0, right: 12.0, bottom: 6.0, left: 12.0 })
        .into()
}

/// `1:05`, `12:40`, `1:02:03`.
fn format_call_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Status line for one history entry ("Outgoing · 12:34", "Missed", …).
fn call_record_label(rec: &cursus_core::CallRecord) -> String {
    let direction = if rec.outgoing { "Outgoing" } else { "Incoming" };
    let label = match (rec.outgoing, rec.status.as_str()) {
        (_, "ended") if rec.duration_secs > 0 => {
            format!("{direction} · {}", format_call_duration(rec.duration_secs))
        }
        (true, "missed" | "pending" | "rejected") => "No answer".into(),
        (false, "missed" | "pending") => "Missed".into(),
        (false, "rejected") => "Declined".into(),
        (_, "error") => "Failed".into(),
        (_, "accepted" | "negotiated" | "progress") => "In progress".into(),
        _ => direction.into(),
    };
    label
}

/// Recent-call row: tap opens the chat, the phone button calls back.
fn call_history_row<'a>(
    chat: Option<&'a cursus_core::ChatPreview>,
    rec: &'a cursus_core::CallRecord,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let t = theme::t();
    let name = chat
        .map(|c| c.title.clone())
        .unwrap_or_else(|| rec.peer_name.clone());
    let image = chat.and_then(|c| c.peer_profile.image.as_deref());
    let missed = rec.is_missed();
    let tint = if missed || matches!(rec.status.as_str(), "rejected" | "error") {
        t.danger
    } else if rec.outgoing {
        t.accent
    } else {
        t.success
    };
    let when = format_list_time(rec.ts.as_deref()).unwrap_or_default();
    let info = row![
        profile_avatar(image, &name, 40.0),
        column![
            crate::ellipsis::ellipsis_text_weight(
                name.clone(),
                d.body,
                if missed { t.danger } else { t.text_primary },
                1,
                true
            ),
            row![
                icons::call_history_icon(rec.outgoing, &rec.status, d.body * 0.9, tint),
                text(call_record_label(rec))
                    .size(d.body * 0.85)
                    .wrapping(Wrapping::None)
                    .color(tint),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        ]
        .spacing(2)
        .width(Length::Fill),
        text(when).size(d.body * 0.78).color(t.text_muted),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let open = mouse_area(
        container(info)
            .width(Length::Fill)
            .padding(Padding::from([6, 4])),
    )
    .on_release(Message::Call(crate::call_flow::CallMsg::OpenChat(
        rec.contact_id,
    )))
    .interaction(mouse::Interaction::Pointer);
    row![
        container(open).width(Length::Fill).clip(true),
        round_header_icon_btn(
            icons::phone_tool(PROFILE_HEADER_ICON, hover),
            Message::StartAudioCall(rec.contact_id),
            hover,
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn calls_detail(app: &App) -> Element<'_, Message> {
    let d = app.density();
    if let Some(card) = call_card(app, true) {
        return container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(d.pad)
            .into();
    }
    empty_detail(
        icons::nav_icon(NavIcon::Calls, false, 36.0),
        "Stay in touch",
        "Choose a contact to start an encrypted audio call.",
        None,
        d,
        app.hover_ok(),
    )
}

/// Ringing / active call: avatar, name, state, and its actions.
/// `large` = the detail pane; otherwise a card on top of the contact list.
fn call_card(app: &App, large: bool) -> Option<Element<'_, Message>> {
    use crate::call_flow::CallPhase;
    let d = app.density();
    let hover = app.hover_ok();
    let t = theme::t();

    let (contact_id, name, state, actions): (i64, String, String, Element<'_, Message>) =
        if let Some(inv) = app.incoming_call() {
            let kind = match inv.media {
                cursus_core::CallMedia::Audio => "Incoming call",
                // We answer video invitations with audio only.
                cursus_core::CallMedia::Video => "Incoming video call (audio only)",
            };
            let actions = row![
                call_button(
                    "Decline",
                    icons::phone_icon(18.0, t.text_primary),
                    Message::RejectIncomingCall,
                    theme::danger_action(hover)
                ),
                call_button(
                    "Accept",
                    icons::phone_icon(18.0, t.accent_on),
                    Message::AcceptIncomingCall,
                    theme::primary_action(hover)
                ),
            ]
            .spacing(12)
            .into();
            (
                inv.contact_id,
                inv.peer_name.clone(),
                kind.to_string(),
                actions,
            )
        } else {
            let call = app.active_call()?;
            let state = match call.phase {
                CallPhase::Ringing => "Calling…".to_string(),
                CallPhase::Connecting => "Connecting…".to_string(),
                CallPhase::Connected => {
                    let secs = call
                        .connected_at
                        .map(|t| t.elapsed().as_secs())
                        .unwrap_or(0);
                    let clock = if secs >= 3600 {
                        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
                    } else {
                        format!("{:02}:{:02}", secs / 60, secs % 60)
                    };
                    if call.muted {
                        format!("{clock} · muted")
                    } else {
                        clock
                    }
                }
            };
            let mute_label = if call.muted { "Unmute" } else { "Mute" };
            let mute_tint = if call.muted { t.danger } else { t.text_primary };
            let actions = row![
                call_button(
                    mute_label,
                    icons::mic_icon(18.0, mute_tint),
                    Message::Call(crate::call_flow::CallMsg::ToggleMute),
                    theme::secondary_action(hover)
                ),
                call_button(
                    "End",
                    icons::phone_icon(18.0, t.text_primary),
                    Message::EndActiveCall,
                    theme::danger_action(hover)
                ),
            ]
            .spacing(12)
            .into();
            (call.contact_id, call.peer_name.clone(), state, actions)
        };

    let image = app
        .chats()
        .iter()
        .find(|c| {
            matches!(c.id.chat_type, cursus_core::ChatType::Direct) && c.id.api_id == contact_id
        })
        .and_then(|c| c.peer_profile.image.clone());
    let avatar_size = if large { 112.0 } else { 44.0 };
    let avatar = profile_avatar(image.as_deref(), &name, avatar_size);
    let name_text = text(name)
        .size(if large { d.title } else { d.body })
        .font(BOLD)
        .color(t.text_primary);
    let state_text = text(state)
        .size(if large { d.body } else { d.meta })
        .color(t.text_secondary);

    if large {
        return Some(
            column![
                avatar,
                name_text,
                state_text,
                vertical().height(24),
                actions,
            ]
            .spacing(10)
            .align_x(Alignment::Center)
            .into(),
        );
    }
    Some(
        container(
            column![
                row![avatar, column![name_text, state_text].spacing(2)]
                    .spacing(12)
                    .align_y(Alignment::Center),
                actions,
            ]
            .spacing(12)
            .padding(d.pad_tight),
        )
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().accent_soft)),
            border: iced::Border {
                color: theme::t().accent,
                width: 1.0,
                radius: theme::CONTROL_RADIUS.into(),
            },
            ..container::Style::default()
        })
        .into(),
    )
}

fn call_button<'a>(
    label: &'a str,
    icon: Element<'a, Message>,
    on_press: Message,
    style: impl Fn(&iced::Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    button(
        row![icon, text(label).size(14)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .on_press(on_press)
    .padding(Padding::from([10, 16]))
    .style(style)
    .into()
}

/// Image-pen (set) + cancel/X (clear) for own / group / channel profile photos.
/// A profile picture you can change: tap opens the Change / Remove sheet.
fn tappable_avatar<'a>(avatar: Element<'a, Message>, on_press: Message) -> Element<'a, Message> {
    mouse_area(avatar)
        .on_release(on_press)
        .interaction(mouse::Interaction::Pointer)
        .into()
}

/// Change / Remove picture sheet for your profile or a group you own.
fn photo_menu_overlay(app: &App, target: crate::PhotoTarget) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let (has_image, pick, clear) = match target {
        crate::PhotoTarget::Own => (
            app.own_profile().fields.image.is_some(),
            Message::PickOwnProfileImage,
            Message::ClearOwnProfileImage,
        ),
        crate::PhotoTarget::Group => (
            app.group_settings().is_some_and(|g| g.image.is_some()),
            Message::PickGroupProfileImage,
            Message::ClearGroupProfileImage,
        ),
    };
    let item =
        |icon: Element<'static, Message>, label: &'static str, msg: Message, danger: bool| {
            let fg = if danger {
                theme::t().danger
            } else {
                theme::t().text_primary
            };
            button(
                row![icon, text(label).size(d.body).color(fg)]
                    .spacing(12)
                    .align_y(Alignment::Center),
            )
            .on_press(msg)
            .padding(Padding::from([10, 12]))
            .width(Length::Fill)
            .style(theme::menu_item(hover))
        };
    let mut col = column![
        text("Profile picture")
            .size(d.body)
            .font(BOLD)
            .color(theme::t().text_primary),
        item(
            icons::image_pen_icon(18.0, theme::t().accent),
            "Change picture",
            pick,
            false
        ),
    ]
    .spacing(4)
    .max_width(360);
    if has_image {
        col = col.push(item(
            icons::trash_icon(18.0, theme::t().danger),
            "Remove picture",
            clear,
            true,
        ));
    }
    col = col.push(item(
        icons::cancel_icon(18.0, theme::t().text_secondary),
        "Cancel",
        Message::ClosePhotoMenu,
        false,
    ));
    sheet_overlay(menu_card(col.into(), d), Message::ClosePhotoMenu)
}

fn group_settings_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(g) = app.group_settings() else {
        return vertical().height(0).into();
    };

    let (heading, close_msg): (&str, Message) = match g.pane {
        GroupSettingsPane::Link if g.is_channel => ("Share this channel", Message::Back),
        GroupSettingsPane::Link => ("Share this group", Message::Back),
        GroupSettingsPane::AddMember => ("Invite a contact", Message::CloseAddGroupMember),
        GroupSettingsPane::Preferences => ("Group settings", Message::CloseGroupPreferences),
        GroupSettingsPane::MemberInfo => ("Member", Message::CloseGroupMember),
        GroupSettingsPane::ConfirmLeave => ("Leave group", Message::CloseGroupSettings),
        GroupSettingsPane::ConfirmDelete => ("Delete group", Message::CloseGroupSettings),
        GroupSettingsPane::ChannelMembers if can_edit_group_profile(&g.my_role) => {
            ("Subscribers", Message::Back)
        }
        GroupSettingsPane::ChannelMembers => ("Owners", Message::Back),
        GroupSettingsPane::Overview if g.is_channel => ("Channel", Message::CloseGroupSettings),
        GroupSettingsPane::Overview => ("Group", Message::CloseGroupSettings),
    };
    let nested = !matches!(g.pane, GroupSettingsPane::Overview);

    let overview_actions: Element<'_, Message> = if nested {
        horizontal().width(0).into()
    } else {
        group_quick_actions(g, d, hover).0
    };

    // Nested panes always go Back; top-level ones close (phones only).
    let back = if nested {
        Some(Message::Back)
    } else {
        with_back.then(|| close_msg.clone())
    };
    let leading = page_title(heading, back, d, hover);

    // Keep the same Back hit target and header geometry in every profile pane.
    let header = profile_header_bar(leading, overview_actions, d, with_back);

    let body: Element<'_, Message> = match g.pane {
        GroupSettingsPane::Link | GroupSettingsPane::AddMember => group_add_member_pane(app, g, d, hover),
        GroupSettingsPane::Preferences => group_preferences_pane(app, g, d, hover),
        GroupSettingsPane::MemberInfo => group_member_info_pane(g, d, hover),
        GroupSettingsPane::ChannelMembers => {
            button_card(group_members_list(g, d, hover, !can_edit_group_profile(&g.my_role)))
        }
        GroupSettingsPane::ConfirmLeave => column![
            icons::centered_hit(icons::leave_icon(28.0, theme::t().text_secondary), 48.0,),
            text(if g.is_channel {
                "Leave this channel? You will need a new invite to rejoin."
            } else {
                "Leave this group? You will need a new invite to rejoin."
            })
            .size(d.body)
            .color(theme::t().text_secondary),
            row![
                button(text("Cancel").size(d.meta))
                    .on_press(Message::Back)
                    .padding(Padding::from([12, 16]))
                    .width(Length::Fill)
                    .style(theme::secondary_action(hover)),
                button(
                    row![
                        icons::leave_icon(16.0, Color::WHITE),
                        text("Leave").size(d.meta).color(Color::WHITE),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ConfirmLeaveGroup)
                .padding(Padding::from([12, 16]))
                .width(Length::Fill)
                .style(theme::danger_action(hover)),
            ]
            .spacing(8),
        ]
        .spacing(14)
        .align_x(Alignment::Center)
        .into(),
        GroupSettingsPane::ConfirmDelete => column![
            icons::centered_hit(icons::trash_icon(28.0, theme::t().danger), 48.0,),
            text(if g.is_channel {
                "Delete this channel for yourself? This cannot be undone."
            } else {
                "Delete this group for yourself? This cannot be undone."
            })
            .size(d.body)
            .color(theme::t().text_secondary),
            row![
                button(text("Cancel").size(d.meta))
                    .on_press(Message::Back)
                    .padding(Padding::from([12, 16]))
                    .width(Length::Fill)
                    .style(theme::secondary_action(hover)),
                button(
                    row![
                        icons::trash_icon(16.0, Color::WHITE),
                        text("Delete").size(d.meta).color(Color::WHITE),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ConfirmDeleteGroup)
                .padding(Padding::from([12, 16]))
                .width(Length::Fill)
                .style(theme::danger_action(hover)),
            ]
            .spacing(8),
        ]
        .spacing(14)
        .align_x(Alignment::Center)
        .into(),
        GroupSettingsPane::Overview => {
            let narrow = matches!(app.layout(), LayoutMode::Mobile) || app.window_width() < 720.0;
            let support_unread = app
                .chats()
                .iter()
                .find(|c| {
                    c.id.chat_type == cursus_core::ChatType::Group && c.id.api_id == g.group_id
                })
                .map(|c| app.support_badge_count(c))
                .unwrap_or(0);
            group_overview_pane(g, d, hover, narrow, support_unread)
        }
    };

    let page: Element<'_, Message> = floating_header_page(header, body, d);

    let mut layers = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(crate::input_gate::freeze(page, g.member_dropdown.is_some()));
    if g.member_dropdown.is_some() && g.selected_member_id.is_some() {
        layers = layers.push(member_dropdown_overlay(g, d, hover));
    }
    iced::widget::opaque(layers)
}

fn member_dropdown_overlay(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'_, Message> {
    let Some(member_id) = g.selected_member_id else {
        return Space::new().into();
    };
    let Some(m) = g.members.iter().find(|x| x.group_member_id == member_id) else {
        return Space::new().into();
    };
    let Some(kind) = g.member_dropdown else {
        return Space::new().into();
    };
    let tint = theme::t().text_secondary;
    let danger = theme::t().danger;
    let mut items = Column::new().spacing(4).width(Length::Fill);
    items = items.push(
        text(m.display_name.clone())
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    match kind {
        crate::MemberDropdown::Role => {
            let roles: &[&str] = if g.my_role.eq_ignore_ascii_case("owner") {
                &["observer", "member", "moderator", "admin", "owner"]
            } else {
                &["observer", "member", "moderator", "admin"]
            };
            for role in roles {
                if m.role.eq_ignore_ascii_case(role) {
                    continue;
                }
                items = items.push(
                    button(
                        row![
                            icons::role_icon(role, 18.0, tint),
                            text(format!("make {role}"))
                                .size(d.body)
                                .color(theme::t().text_primary),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberSetRole((*role).into()))
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
        }
        crate::MemberDropdown::Actions => {
            let mod_plus = can_moderate(&g.my_role);
            let admin = can_manage_members(&g.my_role);
            let pending = member_status_is_pending(&m.status);
            if pending && mod_plus {
                items = items.push(
                    button(
                        row![
                            icons::member_role_icon(18.0, theme::t().success),
                            text("Accept as member")
                                .size(d.body)
                                .color(theme::t().text_primary),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberAccept {
                        member_id,
                        role: "member".into(),
                    })
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
                items = items.push(
                    button(
                        row![
                            icons::observer_role_icon(18.0, tint),
                            text("Accept as observer")
                                .size(d.body)
                                .color(theme::t().text_primary),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberAccept {
                        member_id,
                        role: "observer".into(),
                    })
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
                items = items.push(
                    button(
                        row![
                            icons::remove_profile_icon(18.0, danger),
                            text("Reject").size(d.body).color(danger),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberRemove {
                        member_id,
                        with_messages: false,
                    })
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
            // Direct message — only when the group allows direct messages.
            if !g.is_channel
                && !m.is_me
                && !pending
                && group_pref_on(g.group_preferences.as_ref(), "directMessages")
            {
                items = items.push(
                    button(
                        row![
                            icons::paperplane_icon(18.0, tint),
                            text("Direct message")
                                .size(d.body)
                                .color(theme::t().text_primary),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenMemberDirectChat)
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
            if mod_plus
                && (m.support_chat.is_some()
                    || (group_support_allowed(g.group_preferences.as_ref())
                        && group_role_rank(&m.role) < group_role_rank("moderator")))
            {
                items = items.push(
                    button(
                        row![
                            icons::flag_icon(18.0, tint),
                            text(if group_role_rank(&m.role) >= group_role_rank("moderator") { "Chat with admin" } else { "Chat with member" })
                                .size(d.body)
                                .color(theme::t().text_primary),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenMemberSupportChat {
                        member_id: m.group_member_id,
                        title: m.display_name.clone(),
                    })
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
            if admin {
                let block_label = if m.blocked_by_admin {
                    "Unblock for all"
                } else {
                    "Block for all"
                };
                items = items.push(
                    button(
                        row![
                            icons::block_prohibited_icon(18.0, danger),
                            text(block_label).size(d.body).color(danger),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberToggleBlockAll)
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
                items = items.push(
                    button(
                        row![
                            icons::remove_profile_icon(18.0, danger),
                            text("Remove member").size(d.body).color(danger),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::GroupMemberRemove {
                        member_id,
                        with_messages: false,
                    })
                    .padding(Padding::from([10, 12]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
        }
    }

    let card = menu_card(items.into(), d);
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::CloseMemberDropdown);

    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));

    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

fn group_quick_actions(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> (Element<'static, Message>, Option<Element<'static, Message>>) {
    profile_quick_actions(
        vec![
            (Quick::Star(g.favorite), Message::GroupSettingsFavorite),
            (Quick::Mute(g.muted), Message::GroupSettingsMute),
            (Quick::Settings, Message::OpenGroupPreferences),
        ],
        d,
        hover,
    )
}

fn group_overview_pane(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
    narrow: bool,
    support_unread: i64,
) -> Element<'_, Message> {
    let owner = can_edit_group_profile(&g.my_role);
    let admin = can_manage_members(&g.my_role);
    let mut page = Column::new().spacing(PAGE_GAP).width(Length::Fill);

    // ── Profile card ────────────────────────────────────────────────────
    let mut profile = Column::new().spacing(6).width(Length::Fill);
    {
        let name: Element<'_, Message> = if owner {
            editable_line(
                &g.name,
                "Name",
                LineStyle::Name,
                crate::EditField::GroupName,
                d,
                hover,
            )
        } else {
            profile_name_text(g.name.clone(), d)
        };
        let alias = editable_line(
            &g.alias,
            "Add a local name",
            LineStyle::Sub,
            crate::EditField::GroupAlias,
            d,
            hover,
        );
        let avatar = profile_avatar_fallback(
            g.image.as_deref(),
            &g.name,
            PROFILE_AVATAR,
            if g.is_channel {
                AvatarFallback::ChannelIcon
            } else {
                AvatarFallback::GroupIcon
            },
        );
        let mut lines = vec![name, alias];
        // The short description is the line under the name, for everyone
        // (a paragraph-long one gets its own block below the actions).
        let paragraph = is_paragraph(&g.short_descr);
        if owner && !paragraph {
            lines.push(editable_line(
                &g.short_descr,
                "Add a short description",
                LineStyle::Sub,
                crate::EditField::GroupShortDescr,
                d,
                hover,
            ));
        } else if !owner && !paragraph && !g.short_descr.is_empty() {
            lines.push(hero_line(g.short_descr.clone(), d));
        }
        profile = profile.push(profile_identity(
            if owner {
                tappable_avatar(avatar, Message::OpenPhotoMenu(crate::PhotoTarget::Group))
            } else {
                avatar
            },
            None,
            lines,
        ));
    }
    page = page.push(card(profile));
    if let Some(tiles) = group_quick_actions(g, d, hover).1 {
        page = page.push(tiles);
    }
    // A paragraph-long description: its own block under the actions.
    if is_paragraph(&g.short_descr) {
        page = page.push(if owner {
            column![
                card_title(vertical().height(0).into(), "Description", "", d),
                button_card(editable_line(&g.short_descr, "Add a short description", LineStyle::Body,
                    crate::EditField::GroupShortDescr, d, hover)),
            ]
            .spacing(8)
            .into()
        } else {
            about_block("Description", g.short_descr.clone(), d)
        });
    }
    if !owner && !g.welcome_message.is_empty() {
        page = page.push(about_block("Welcome message", g.welcome_message.clone(), d));
    }

    // ── Channel link (share-only) ───────────────────────────────────────

    // ── Support / invite / subscribers ──────────────────────────────────
    let support_ok = group_support_allowed(g.group_preferences.as_ref());
    let moderate = can_moderate(&g.my_role);
    let mut people = Column::new().spacing(0).width(Length::Fill);
    if owner {
        // Same row as "Chats with members" / "Invite"; opens the editor.
        let preview = cursus_core::profile_text(&g.welcome_message)
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let sub = if preview.is_empty() {
            String::new()
        } else {
            preview
        };
        people = people.push(card_row(
            icons::scroll_icon(18.0, theme::t().accent),
            "Welcome message",
            Some(sub),
            Message::OpenFieldEditor(crate::EditField::GroupWelcome),
            None,
            d,
            hover,
        ));
    }
    if support_ok || moderate {
        let (label, sub, msg) = if moderate {
            ("Chats with members", "", Message::OpenSupportChatList)
        } else {
            ("Chat with admins", "", Message::OpenUserSupportChat)
        };
        people = people.push(card_row(
            icons::flag_icon(18.0, theme::t().accent),
            label,
            None::<String>.filter(|_| !sub.is_empty()),
            msg,
            (support_unread > 0).then(|| attention_count(support_unread)),
            d,
            hover,
        ));
    }
    if g.is_channel {
        // Official channels: no inline member list. Owners open “Subscribers”,
        // everyone else only sees the channel’s owners.
        let owner_view = can_edit_group_profile(&g.my_role);
        let (label, sub) = if owner_view {
            ("Subscribers", format!("{} people", g.members.len()))
        } else {
            ("Owners", String::new())
        };
        people = people.push(card_row(
            icons::user_group_icon(18.0, theme::t().accent),
            label,
            Some(sub),
            Message::OpenChannelMembers,
            None,
            d,
            hover,
        ));
    }
    if admin || g.is_channel {
        people = people.push(card_row(icons::share_icon(18.0, theme::t().accent),
            if g.is_channel { "Share this channel" } else { "Share this group" }, None,
            Message::OpenGroupLink, None, d, hover));
    }
    if owner || support_ok || moderate || admin || g.is_channel {
        page = page.push(button_card(people));
    }

    // ── Members (private groups) ────────────────────────────────────────
    if !g.is_channel {
        let count = g.members.len();
        let mut rows = Column::new().spacing(0).width(Length::Fill);
        if admin {
            rows = rows.push(card_row(icons::user_plus_icon(18.0, theme::t().accent),
                "Invite a contact", None, Message::OpenAddGroupMember, None, d, hover));
        }
        rows = rows.push(group_members_list(g, d, hover, false));
        page = page.push(
            column![
                card_title(
                    vertical().height(0).into(),
                    if count == 1 { "1 member".to_string() } else { format!("{count} members") },
                    "",
                    d,
                ),
                button_card(rows),
            ]
            .spacing(8),
        );
    }

    // ── Chat actions ────────────────────────────────────────────────────
    // Official: a channel’s only owner can delete it but not leave it.
    let sole_channel_owner = g.is_channel
        && g.my_role.eq_ignore_ascii_case("owner")
        && !g
            .members
            .iter()
            .any(|m| !m.is_me && m.role.eq_ignore_ascii_case("owner"));
    page = page.push(button_card(group_danger_actions(
        g.is_channel,
        !sole_channel_owner,
        d,
        hover,
        narrow,
    )));
    page.into()
}

/// Vertical gap between cards on a page.
const PAGE_GAP: f32 = 22.0;

/// Navigation row inside a card: accent icon disc · title / subtitle · chevron
/// (or a custom trailing glyph). Ghost tile, so the shape shows on hover.
fn nav_row<'a>(
    icon: Element<'a, Message>,
    title: &'a str,
    subtitle: Option<String>,
    on_press: Message,
    trailing: Option<Element<'a, Message>>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    nav_row_styled(icon, title, subtitle, on_press, trailing, d, hover, false)
}

/// [`nav_row`] for a [`button_card`]: fills the card's width and corners,
/// content stays where the inset card put it.
fn card_row<'a>(
    icon: Element<'a, Message>,
    title: &'a str,
    subtitle: Option<String>,
    on_press: Message,
    trailing: Option<Element<'a, Message>>,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    nav_row_styled(icon, title, subtitle, on_press, trailing, d, hover, true)
}

#[allow(clippy::too_many_arguments)]
fn nav_row_styled<'a>(
    icon: Element<'a, Message>,
    title: &'a str,
    subtitle: Option<String>,
    on_press: Message,
    trailing: Option<Element<'a, Message>>,
    d: Density,
    hover: bool,
    flush: bool,
) -> Element<'a, Message> {
    let mut labels = Column::new()
        .spacing(1)
        .width(Length::Fill)
        .push(text(title).size(d.body).color(theme::t().text_primary));
    let two_lines = subtitle.as_deref().is_some_and(|s| !s.is_empty());
    if let Some(sub) = subtitle.filter(|s| !s.is_empty()) {
        // Card rows keep one line each (the welcome preview can be long).
        labels = labels.push(if flush {
            crate::ellipsis::ellipsis_text(sub, d.meta, theme::t().text_secondary, 1)
        } else {
            text(sub)
                .size(d.meta)
                .color(theme::t().text_secondary)
                .into()
        });
    }
    let _ = flush;
    button(
        container(row![
            icon_disc(icon, ROW_ICON_BOX),
            labels,
            trailing.unwrap_or_else(|| icons::chevron_right(16.0, theme::t().text_muted)),
        ]
        .spacing(ROW_GAP)
        .align_y(Alignment::Center))
        .width(Length::Fill)
        .center_y(if d.list_width == 0.0 { Length::Fill } else { Length::Shrink }),
    )
    .on_press(on_press)
    .padding(if d.list_width == 0.0 { Padding::from([12, 16]) } else { ROW_PAD })
    .width(Length::Fill)
    .height(if d.list_width == 0.0 { Length::Fixed(profile_phone_row_height(d, two_lines)) } else { Length::Shrink })
    .style(theme::ghost_tile_r(hover, ROW_RADIUS))
    .into()
}

/// Phone row height: one line (56, the Material list height) or room for a
/// title and one preview line, at the current text scale.
fn profile_phone_row_height(d: Density, two_lines: bool) -> f32 {
    if two_lines {
        ((d.body + d.meta) * 1.3 + 28.0).max(68.0).ceil()
    } else {
        (d.body * 1.3 + 28.0).max(56.0).ceil()
    }
}

/// Every row of a page — navigation, preference, action — has the same
/// inset, icon box and gap, so labels line up down the whole page.
const ROW_PAD: Padding = Padding {
    top: 10.0,
    right: 16.0,
    bottom: 10.0,
    left: 16.0,
};
const ROW_ICON_BOX: f32 = 24.0;
const ROW_GAP: f32 = 14.0;
/// Rows live inside panels: concentric with the panel's corner.
const ROW_RADIUS: f32 = theme::INSET_CONTROL_RADIUS;

/// The glyph in front of a row: a fixed box, no backdrop.
fn icon_disc<'a>(icon: Element<'a, Message>, size: f32) -> Element<'a, Message> {
    let size = size.min(ROW_ICON_BOX + 4.0);
    icons::centered_hit(icon, size)
}

/// Avatar size in member / contact lists (group page, Invite, support list).
const MEMBER_AVATAR: f32 = 40.0;

/// Gold “pending” clock in a fixed 16×16 box — never shrinks with the window.
fn pending_glyph<'a>() -> Element<'a, Message> {
    container(icons::pending_status_icon(16.0, theme::t().always))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .into()
}

/// Member rows (avatar · name · gold pending clock · role/action menus).
/// `owners_only`: channel subscribers see just the owners (official).
fn group_members_list<'a>(
    g: &'a crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
    owners_only: bool,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(0).width(Length::Fill);
    if g.members_loading {
        col = col.push(container(text("Loading…").size(d.meta).color(theme::t().text_muted)).padding(ROW_PAD));
    } else if g.members.is_empty() {
        col = col.push(
            container(text("No members listed.").size(d.meta).color(theme::t().text_muted)).padding(ROW_PAD),
        );
    } else {
        // Rows sit flush (no gap) — the list reads as one block.
        let mut list = Column::new().spacing(0).width(Length::Fill);
        for m in g
            .members
            .iter()
            .filter(|m| !owners_only || m.role.eq_ignore_ascii_case("owner"))
        {
            let mid = m.group_member_id;
            let name = if m.is_me {
                format!("You ({})", m.public_name)
            } else {
                m.display_name.clone()
            };
            // One line, "…" when too long (never wraps).
            let name_text = ellipsis_text(name, d.body, theme::t().text_primary, 1);
            // Invited / connecting: gold clock right after the name; its size
            // is reserved first so a narrow window never squeezes it away.
            let name_line: Element<'_, Message> = if member_status_shows_pending_icon(&m.status) {
                crate::trail_row::trail_row(name_text, pending_glyph(), 6.0)
            } else {
                name_text
            };
            let mut names = Column::new().spacing(if d.list_width == 0.0 { 4 } else { 1 }).width(Length::Fill).push(name_line);
            if let Some(bio) = m.short_descr.as_deref().filter(|bio| !bio.trim().is_empty()) {
                let rich = profile_rich(bio, d.meta, theme::t().text_secondary, false);
                if d.list_width == 0.0 {
                    let segments = cursus_core::profile_text(bio);
                    if segments.iter().all(|segment| segment.style == TextStyle::Plain) {
                        let preview = segments.iter().map(|segment| segment.text.as_str()).collect::<String>();
                        names = names.push(ellipsis_text(preview, d.meta, theme::t().text_secondary, 1));
                    } else {
                        names = names.push(container(rich.wrapping(Wrapping::None))
                            .height(d.meta * 1.5).width(Length::Fill).clip(true));
                    }
                } else { names = names.push(rich); }
            }
            let leading = Row::new()
                .spacing(10)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .push(profile_avatar(
                    m.image.as_deref(),
                    &m.public_name,
                    if d.list_width == 0.0 { 48.0 } else { MEMBER_AVATAR },
                ))
                .push(names);
            // One hit target per member; the role is a quiet sign, not a
            // control. Role and moderation live on the member page.
            let pad = if d.list_width == 0.0 {
                Padding::from([12, 16])
            } else {
                Padding::from([6, 16])
            };
            list = list.push(
                button(
                    row![leading, icons::role_icon(&m.role, 18.0, theme::t().text_secondary)]
                        .spacing(12)
                        .align_y(Alignment::Center),
                )
                .on_press(Message::OpenGroupMember(mid))
                .padding(pad)
                .width(Length::Fill)
                .style(theme::ghost_tile_r(hover, CARD_RADIUS)),
            );
        }
        col = col.push(list);
    }
    col.into()
}

/// Profile size of the avatar in every profile header.
const PROFILE_AVATAR: f32 = 96.0;

/// Read-only name line, inset to line up with the text inside the fields.
fn profile_name_text<'a>(name: String, d: Density) -> Element<'a, Message> {
    container(
        text(name).size(d.title + 4.0).font(BOLD).color(theme::t().text_primary)
            .align_x(Alignment::Center)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding(Padding::from([0, 12]))
    .center_x(Length::Fill)
    .into()
}

/// How an [`editable_line`] is set.
#[derive(Debug, Clone, Copy)]
enum LineStyle {
    /// Big bold name in a profile header.
    Name,
    /// Secondary line under the name (bio, local name).
    Sub,
    /// Paragraph under a section label (descriptions).
    Body,
}

/// Formatted profile text, with a pencil right after it that opens
/// the editor sheet. Empty values show a muted hint instead.
fn editable_line<'a>(
    value: &str,
    placeholder: &str,
    style: LineStyle,
    field: crate::EditField,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let empty = value.trim().is_empty();
    let shown = if empty {
        placeholder.to_string()
    } else {
        value.to_string()
    };
    let (size, color) = match style {
        LineStyle::Name => (d.title + 4.0, theme::t().text_primary),
        LineStyle::Sub => (d.body, theme::t().text_secondary),
        LineStyle::Body => (d.body, theme::t().text_secondary),
    };
    let plain_name = matches!(field, crate::EditField::OwnName | crate::EditField::GroupName |
        crate::EditField::GroupAlias | crate::EditField::ContactAlias);
    let label: Element<'a, Message> = if plain_name {
        text(shown.clone()).size(size).color(if empty { theme::t().text_muted } else { color })
            .font(if matches!(style, LineStyle::Name) { BOLD } else { Font::DEFAULT }).into()
    } else {
        profile_rich(&shown, size, if empty { theme::t().text_muted } else { color }, false).into()
    };
    let pencil = button(icons::edit_icon(16.0, theme::t().text_muted))
        .on_press(Message::OpenFieldEditor(field))
        .padding(6)
        .style(theme::icon_action(hover));
    let paragraph = matches!(style, LineStyle::Body) ||
        (matches!(field, crate::EditField::GroupShortDescr | crate::EditField::OwnBio) &&
         (value.contains('\n') || value.chars().count() > 64));
    if paragraph {
        return button(row![container(label).width(Length::Fill), icons::edit_icon(16.0, theme::t().text_muted)]
            .spacing(12).align_y(Alignment::Start))
            .on_press(Message::OpenFieldEditor(field)).padding(Padding::from([12, 16])).width(Length::Fill)
            .style(theme::ghost_tile_r(hover, ROW_RADIUS)).into();
    }
    let line = crate::trail_row::trail_row(label, pencil, 4.0);
    match style {
        // Paragraphs start at the row inset, under their section label.
        LineStyle::Body => container(line)
            .padding(Padding::from([0, 12]))
            .width(Length::Fill)
            .into(),
        // Header lines are centred under the picture. The pencil sits to the
        // right of the text, so the same space is left free on the left.
        _ => container(line)
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 28.0,
            })
            .center_x(Length::Fill)
            .into(),
    }
}

fn field_editor_meta(field: crate::EditField) -> (&'static str, &'static str, &'static str) {
    use crate::EditField as F;
    match field {
        F::OwnName => ("Display name", "How your contacts see you.", "Your name"),
        F::OwnBio => ("Bio", "A short line under your name.", "Bio"),
        F::OwnDescription => (
            "Description",
            "Shown on your profile.",
            "Tell people about yourself",
        ),
        F::GroupName => ("Name", "Every member sees this name.", "Name"),
        F::GroupAlias | F::ContactAlias => ("Local name", "Only you see this name.", "Local name"),
        F::GroupShortDescr => (
            "Short description",
            "Shown under the name.",
            "Short description",
        ),
        F::GroupWelcome => (
            "Welcome message",
            "Sent to every new member when they join.",
            "Welcome to the group! Please…",
        ),
    }
}

/// Editor sheet for one profile text: title, hint, field, cancel ✕ / save ✓.
fn field_editor_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(editor) = app.field_editor() else {
        return vertical().height(0).into();
    };
    let (title, hint, placeholder) = field_editor_meta(editor.field);
    let id = iced::widget::Id::new(crate::FIELD_EDITOR_ID);

    let cancel = button(icons::cancel_icon(16.0, theme::t().text_secondary))
        .on_press(Message::FieldEditorCancel)
        .padding(10)
        .style(theme::icon_action(hover));
    let save = button(icons::check_icon(18.0, theme::t().accent_on))
        .on_press(Message::FieldEditorSave)
        .padding(9)
        .style(theme::primary_action(hover));
    let header = row![
        column![
            text(title)
                .size(d.body + 1.0)
                .font(BOLD)
                .color(theme::t().text_primary),
            text(hint).size(d.meta).color(theme::t().text_secondary),
        ]
        .spacing(2)
        .width(Length::Fill),
        cancel,
        save,
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let field: Element<'_, Message> = if editor.field.multiline() {
        text_editor(&editor.content)
            .id(id)
            .placeholder(placeholder)
            .on_action(Message::FieldEditorEdit)
            .padding(Padding::from([10, 14]))
            .size(d.body)
            .height(Length::Shrink)
            .min_height(140.0)
            .max_height(320.0)
            .wrapping(Wrapping::WordOrGlyph)
            .style(theme::rounded_editor())
            .into()
    } else {
        crate::edit_menu::edit_field(
            text_input(placeholder, &editor.value)
                .id(id.clone())
                .on_input(Message::FieldEditorInput)
                .on_submit(Message::FieldEditorSave)
                .padding(Padding::from([12, 14]))
                .size(d.body)
                .width(Length::Fill)
                .style(theme::rounded_field()),
            id,
            &editor.value,
            Message::FieldEditorInput,
            true,
        )
    };

    let mut content = column![header, field].spacing(14).width(Length::Fill);
    let raw = editor.text();
    if editor.field.required() {
        content = content.push(text("Names are plain text and must start with a letter, number or emoji.").size(d.meta).color(theme::t().text_secondary));
    }
    content = content.push(
        text(if editor.field.required() {
            "Names are displayed exactly as typed."
        } else {
            "Format: **bold** · *italic* · _italic_ · ~strike~ · `code`"
        })
        .size(d.meta)
        .color(theme::t().text_secondary),
    );
    if !raw.trim().is_empty() {
        content = content.push(text("Preview").size(d.meta).color(theme::t().text_muted));
        let preview: Element<'_, Message> = if matches!(editor.field,
            crate::EditField::OwnName | crate::EditField::GroupName |
            crate::EditField::GroupAlias | crate::EditField::ContactAlias) {
            text(raw.clone()).size(d.body).color(theme::t().text_primary).into()
        } else { profile_rich(&raw, d.body, theme::t().text_primary, false).into() };
        content = content.push(container(scrollable(preview).height(Length::Shrink)).max_height(120));
    }
    if let Some(err) = editor.error.as_ref() {
        content = content.push(text(err.clone()).size(d.meta).color(theme::t().danger));
    }
    let card = container(content)
        .padding(d.pad)
        .width(Length::Fill)
        .max_width(480)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: theme::CONTROL_RADIUS.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 24.0,
            },
            ..container::Style::default()
        });
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::FieldEditorCancel);
    // Phones: sit in the upper part so the on-screen keyboard doesn't hide it.
    let narrow = !matches!(app.layout(), LayoutMode::Desktop);
    // Opaque card: taps on its empty parts stay on the card (they neither
    // close the sheet through the backdrop nor reach anything underneath).
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(Padding::from([if narrow { 48 } else { 24 }, 16]));
    let sheet = if narrow {
        sheet.align_y(Alignment::Start)
    } else {
        sheet.center_y(Length::Fill)
    };
    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

/// The one profile header used by Contact, Group, Channel and Settings pages:
/// avatar (photo buttons centred under it when editable) on the left, the
/// name lines stacked on the right.
fn profile_identity<'a>(
    avatar: Element<'a, Message>,
    photo: Option<Element<'a, Message>>,
    lines: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    // Centred hero: picture, then name and the lines under it.
    let mut hero = Column::new()
        .spacing(6)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .push(avatar);
    if let Some(photo) = photo {
        hero = hero.push(photo);
    }
    hero = hero.push(vertical().height(4));
    for line in lines {
        hero = hero.push(line);
    }
    container(hero)
        .width(Length::Fill)
        .padding(Padding::from([8, 12]))
        .into()
}

fn action_tile_row(
    tiles: Vec<iced::widget::Button<'static, Message>>,
    narrow: bool,
) -> Element<'static, Message> {
    // Actions are rows like every other row, on any width.
    let _ = narrow;
    let mut col = Column::new().spacing(0).width(Length::Fill);
    for tile in tiles {
        col = col.push(tile);
    }
    col.into()
}

/// Mute / favorite / preferences — same tile language as clear/leave/delete.
/// A row that opens a choice menu: the whole row is the button, the
/// current value sits at the end with a chevron, and an optional sentence
/// underneath explains it (e.g. what the contact allows).
fn choice_row<'a>(
    icon: Option<Element<'a, Message>>,
    label: &'a str,
    value: String,
    support: Option<String>,
    open: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let support = support.map(|s| text(s).size(d.meta).color(theme::t().text_secondary).width(Length::Fill).into());
    button(preference_row_with_support(icon, label, support, row_value(&value, d), PrefTrail::None, d))
        .on_press(open)
        .padding(0)
        .width(Length::Fill)
        .style(theme::ghost_tile_r(hover, CARD_RADIUS))
        .into()
}

fn contact_feature_prefs(
    p: &crate::ContactProfileOverlay,
    d: Density,
    hover: bool,
) -> Element<'static, Message> {
    // What the contact allows lives in the choice sheet (`pref_menu_note`),
    // so every row here is one centred line: icon, label, value, chevron.
    let tint = theme::t().accent;
    let mut col = Column::new().spacing(0).width(Length::Fill);

    col = col.push(choice_row(
        Some(icons::timer_icon(20.0, tint)),
        "Disappearing messages",
        TimedAllowChoice::from_allow(p.timed_allow).to_string(),
        None,
        Message::OpenPrefMenu(PrefMenu::ContactTimed),
        d,
        hover,
    ));

    let show_negotiated_ttl = matches!(p.timed_allow, FeatureAllow::Yes);
    let show_for_me_ttl = matches!(p.timed_allow, FeatureAllow::Always);
    if show_negotiated_ttl || show_for_me_ttl {
        let current = if show_for_me_ttl {
            TtlChoice::from_secs(p.chat_item_ttl)
        } else {
            TtlChoice::from_secs(Some(p.timed_ttl_secs).filter(|s| *s > 0))
        };
        let open = if show_for_me_ttl {
            Message::OpenPrefMenu(PrefMenu::ContactChatItemTtl)
        } else {
            Message::OpenPrefMenu(PrefMenu::ContactTtl)
        };
        col = col.push(choice_row(
            Some(icons::pending_status_icon(20.0, tint)),
            if show_for_me_ttl { "Delete after (this device only)" } else { "Delete after" },
            current.to_string(),
            None,
            open,
            d,
            hover,
        ));
    }

    for (icon, label, mine, menu) in [
        (icons::trash_icon(20.0, tint), "Delete for everyone", p.full_delete_allow, PrefMenu::ContactFullDelete),
        (icons::reaction_icon(20.0, tint), "Message reactions", p.reactions_allow, PrefMenu::ContactReactions),
        (icons::mic_icon(20.0, tint), "Voice messages", p.voice_allow, PrefMenu::ContactVoice),
        (icons::phone_icon(20.0, tint), "Audio and video calls", p.calls_allow, PrefMenu::ContactCalls),
    ] {
        col = col.push(choice_row(Some(icon), label, mine.label().to_string(), None, Message::OpenPrefMenu(menu), d, hover));
    }

    col.into()
}

fn contact_danger_actions(d: Density, hover: bool, narrow: bool) -> Element<'static, Message> {
    action_tile_row(
        vec![
            group_action_tile(
                icons::clear_all_icon(18.0, theme::t().accent),
                "Clear chat",
                Message::ContactProfileClear,
                theme::ghost_tile_r(hover, CARD_RADIUS),
                d,
                narrow,
                false,
            ),
            group_action_tile(
                icons::trash_icon(18.0, theme::t().danger),
                "Delete contact",
                Message::ContactProfileDelete,
                theme::ghost_danger_r(hover, CARD_RADIUS),
                d,
                narrow,
                true,
            ),
        ],
        narrow,
    )
}

/// Clear / leave / delete — compact tiles matching the contact page.
fn group_danger_actions(
    is_channel: bool,
    can_leave: bool,
    d: Density,
    hover: bool,
    narrow: bool,
) -> Element<'static, Message> {
    let leave_label = if is_channel {
        "Leave channel"
    } else {
        "Leave group"
    };
    let delete_label = if is_channel {
        "Delete channel"
    } else {
        "Delete group"
    };
    let mut tiles = vec![group_action_tile(
        icons::clear_all_icon(18.0, theme::t().accent),
        "Clear chat",
        Message::ClearGroupChat,
        theme::ghost_tile_r(hover, CARD_RADIUS),
        d,
        narrow,
        false,
    )];
    if can_leave {
        tiles.push(group_action_tile(
            icons::leave_icon(18.0, theme::t().danger),
            leave_label,
            Message::OpenLeaveGroupConfirm,
            theme::ghost_danger_r(hover, CARD_RADIUS),
            d,
            narrow,
            true,
        ));
    }
    tiles.push(group_action_tile(
        icons::trash_icon(18.0, theme::t().danger),
        delete_label,
        Message::OpenDeleteGroupConfirm,
        theme::ghost_danger_r(hover, CARD_RADIUS),
        d,
        narrow,
        true,
    ));
    action_tile_row(tiles, narrow)
}

fn group_action_tile<'a>(
    icon: Element<'a, Message>,
    label: &'a str,
    on_press: Message,
    style: impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style + 'a,
    d: Density,
    narrow: bool,
    danger: bool,
) -> iced::widget::Button<'a, Message> {
    // The same row as navigation rows; destructive ones in red.
    let _ = narrow;
    let label_color = if danger {
        theme::t().danger
    } else {
        theme::t().text_primary
    };
    button(
        container(row![
            icon_disc(icon, ROW_ICON_BOX),
            text(label)
                .size(d.body)
                .color(label_color)
                .width(Length::Fill),
        ]
        .spacing(ROW_GAP)
        .align_y(Alignment::Center))
        .width(Length::Fill)
        .center_y(if d.list_width == 0.0 { Length::Fill } else { Length::Shrink }),
    )
    .on_press(on_press)
    .padding(if d.list_width == 0.0 { Padding::from([12, 16]) } else { ROW_PAD })
    .width(Length::Fill)
    .height(if d.list_width == 0.0 { Length::Fixed(profile_phone_row_height(d, false)) } else { Length::Shrink })
    .style(style)
}

/// Rounded blue box showing a link; the whole box copies, copy glyph at the end.
fn link_copy_box<'a>(
    link: &str,
    on_copy: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    button(
        row![
            container(ellipsis_text(link.to_string(), d.meta, theme::t().accent, 1)).width(Length::Fill),
            text("Copy").size(d.meta).color(theme::t().accent),
            icons::copy_icon(18.0, theme::t().accent),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(on_copy)
    .padding(Padding::from([12, 16]))
    .width(Length::Fill)
    .style(theme::link_box(hover))
    .into()
}

/// Corner of a row inside a panel (concentric with the panel).
const CARD_RADIUS: f32 = theme::INSET_CONTROL_RADIUS;

/// A section of a page: rows straight on the panel, no box around them
/// (the same flat language as the chat list).
fn card<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    card_padded(body, Padding::ZERO)
}

/// A group of rows on the shared floating panel (settings, profile
/// actions, member lists): the same surface as the preference pages.
fn button_card<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    crate::preference_ui::panel(body)
}

fn card_padded<'a>(
    body: impl Into<Element<'a, Message>>,
    padding: Padding,
) -> Element<'a, Message> {
    let _ = padding;
    container(body).width(Length::Fill).into()
}

/// Section heading (shared style) with an optional quiet note under it.
fn card_title<'a>(
    _icon: Element<'a, Message>,
    title: impl iced::widget::text::IntoFragment<'a>,
    note: impl Into<String>,
    d: Density,
) -> Element<'a, Message> {
    let note = note.into();
    let mut labels = column![crate::preference_ui::heading(title, d)].spacing(2);
    if !note.is_empty() {
        labels = labels.push(crate::preference_ui::note(note, d));
    }
    labels.into()
}

/// Shared with the Mac page: link first, sharing, join role, then deletion.
fn group_share_pane<'a>(g: &'a crate::GroupSettingsOverlay, d: Density, hover: bool) -> Element<'a, Message> {
    let admin = can_manage_members(&g.my_role);
    let mut content = Column::new().spacing(8).width(Length::Fill)
        .push(crate::preference_ui::note(
            if g.is_channel { "Anyone with this link can subscribe." } else { "Anyone with this link can join the group." },
            d));
    if let Some(link) = &g.link {
        // The box is the copy action (Linux shares through the clipboard).
        content = content.push(link_copy_box(link, Message::CopyGroupLink, d, hover));
        if admin && !g.is_channel {
            content = content.push(vertical().height(12));
            content = content.push(button_card(column![
                card_row(icons::role_icon(&g.link_member_role, 20.0, theme::t().accent), "People joining become", None,
                    Message::OpenPrefMenu(PrefMenu::GroupLinkRole), Some(row_value(role_label(&g.link_member_role), d)), d, hover),
                danger_row(icons::trash_icon(20.0, theme::t().danger), "Delete link",
                    Message::OpenPrefMenu(PrefMenu::GroupLinkDelete), d, hover),
            ]));
        }
    } else if g.is_channel {
        content = content.push(crate::preference_ui::note("No channel link available. Create a new channel to invite people.", d));
    } else if admin {
        content = content.push(button(text("Create link").size(d.body)).on_press(Message::EnsureGroupLink)
            .padding(Padding::from([12, 16])).style(theme::primary_action(hover)));
    } else {
        content = content.push(crate::preference_ui::note("No link yet — ask an admin.", d));
    }
    content.into()
}

/// Group Invite page: who to invite as, a search field and the contacts.
fn group_add_member_pane<'a>(
    app: &'a App,
    g: &'a crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    if matches!(g.pane, GroupSettingsPane::Link) || g.is_channel {
        return group_share_pane(g, d, hover);
    }
    let query = g.invite_search.to_lowercase();
    let contacts = app.contacts_to_add_to_group();
    let filtered: Vec<_> = contacts.iter().filter(|chat| chat.title.to_lowercase().contains(&query)).collect();
    let mut people = Column::new().width(Length::Fill);
    if filtered.is_empty() {
        people = people.push(container(text(if contacts.is_empty() { "Everyone you know is already here." } else { "No matching contacts." })
            .size(d.meta).color(theme::t().text_secondary)).padding(ROW_PAD));
    }
    for chat in filtered {
        people = people.push(button(row![
            profile_avatar(chat.peer_profile.image.as_deref(), &chat.title, MEMBER_AVATAR),
            container(ellipsis_text(chat.title.clone(), d.body, theme::t().text_primary, 1)).width(Length::Fill).clip(true),
            icons::user_plus_tool(24.0, hover),
        ].spacing(12).align_y(Alignment::Center))
            .on_press(Message::AddGroupMember(chat.id.api_id))
            .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 16.0 })
            .width(Length::Fill).style(theme::ghost_tile_r(hover, CARD_RADIUS)));
    }
    column![
        crate::preference_ui::note("They get the invitation in your direct chat.", d),
        button_card(card_row(icons::role_icon(&g.invite_role, 20.0, theme::t().accent), "Invite as", None,
            Message::OpenPrefMenu(PrefMenu::GroupInviteRole), Some(row_value(role_label(&g.invite_role), d)), d, hover)),
        vertical().height(4),
        text_input("Search contacts", &g.invite_search).on_input(Message::GroupInviteSearch)
            .padding(Padding::from([10, 16])).style(theme::rounded_field()),
        button_card(people),
    ].spacing(8).into()
}

fn group_pref_icon(key: &str) -> Element<'static, Message> {
    let tint = theme::t().accent;
    match key {
        "timedMessages" => icons::timer_icon(20.0, tint),
        "directMessages" => icons::paperplane_icon(20.0, tint),
        "fullDelete" => icons::trash_icon(20.0, tint),
        "reactions" => icons::reaction_icon(20.0, tint),
        "voice" => icons::mic_icon(20.0, tint),
        "files" => icons::image_icon(20.0, tint),
        "simplexLinks" => icons::link_icon(20.0, tint),
        "history" => icons::history_icon(20.0, tint),
        "support" => icons::admin_icon(20.0, tint),
        _ => icons::preferences_icon(20.0, tint),
    }
}

fn support_list_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(s) = app.support() else {
        return vertical().height(0).into();
    };

    let leading = page_title(
        "Chats with members",
        with_back.then_some(Message::CloseSupportOverlay),
        d,
        hover,
    );
    let header = profile_header_bar(leading, horizontal().width(0).into(), d, with_back);

    let mut body = Column::new().spacing(8).width(Length::Fill);
    body = body.push(
        text("Pending joiners and support threads with members.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );
    if s.members_loading {
        body = body.push(text("Loading…").size(d.meta).color(theme::t().text_muted));
    } else if s.members.is_empty() {
        body = body.push(
            text("No pending members or support chats.")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else {
        for m in &s.members {
            let unread = m
                .support_chat
                .as_ref()
                .map(|sc| sc.unread + sc.member_attention)
                .unwrap_or(0);
            let mid = m.group_member_id;
            let title = m.display_name.clone();
            let pending = member_status_is_pending(&m.status);
            let mut name = Row::new().spacing(6).align_y(Alignment::Center);
            name = name.push(profile_avatar(
                m.image.as_deref(),
                &m.public_name,
                MEMBER_AVATAR,
            ));
            let label = ellipsis_text(m.display_name.clone(), d.body, theme::t().text_primary, 1);
            name = name.push(
                container(if pending {
                    crate::trail_row::trail_row(label, pending_glyph(), 6.0)
                } else {
                    label
                })
                .width(Length::Fill),
            );
            if unread > 0 {
                name = name.push(
                    text(format!("{unread}"))
                        .size(d.meta)
                        .color(theme::t().always),
                );
            }
            let mut actions = Row::new().spacing(6).align_y(Alignment::Center);
            if pending {
                actions = actions.push(
                    button(text("Accept").size(d.meta))
                        .on_press(Message::GroupMemberAccept {
                            member_id: mid,
                            role: "member".into(),
                        })
                        .padding(Padding::from([8, 10]))
                        .style(theme::primary_action(hover)),
                );
                actions = actions.push(
                    button(text("Reject").size(d.meta).color(theme::t().danger))
                        .on_press(Message::GroupMemberRemove {
                            member_id: mid,
                            with_messages: false,
                        })
                        .padding(Padding::from([8, 10]))
                        .style(theme::secondary_action(hover)),
                );
                // Tap the name to read the join request (and its message) in
                // the member's support chat; Accept / Reject stay inline.
                body = body.push(
                    row![
                        button(name)
                            .on_press(Message::OpenMemberSupportChat {
                                member_id: mid,
                                title,
                            })
                            .padding(Padding::from([6, 8]))
                            .width(Length::Fill)
                            .style(theme::menu_item(hover)),
                        actions,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            } else {
                body = body.push(
                    row![
                        button(name)
                            .on_press(Message::OpenMemberSupportChat {
                                member_id: mid,
                                title,
                            })
                            .padding(Padding::from([6, 8]))
                            .width(Length::Fill)
                            .style(theme::menu_item(hover)),
                        button(icons::centered_hit(
                            icons::trash_icon(16.0, theme::t().danger),
                            32.0,
                        ))
                        .on_press(Message::DeleteMemberSupportChat(mid))
                        .padding(0)
                        .style(theme::danger_quiet(hover)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
        }
    }

    floating_header_page(header, body, d)
}

fn group_preferences_pane<'a>(
    app: &'a App,
    g: &'a crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let owner = can_edit_group_profile(&g.my_role);
    let prefs = g.group_preferences.as_ref();
    let mut groups = Column::new().spacing(20).width(Length::Fill);
    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Messages",
            &[
                ("timedMessages", "Disappearing messages"),
                ("directMessages", "Direct messages"),
                ("fullDelete", "Delete for everyone"),
                ("history", "Recent history"),
            ],
        ),
        (
            "Media & interaction",
            &[
                ("reactions", "Message reactions"),
                ("voice", "Voice messages"),
                ("files", "Files and media"),
                ("simplexLinks", "SimpleX links"),
            ],
        ),
        ("Members", &[("support", "Chat with admins")]),
    ];
    for &(title, features) in sections {
        let mut rows = Column::new().spacing(0).width(Length::Fill);
        for &(key, label) in features {
            let on = if key == "support" {
                group_support_allowed(prefs)
            } else {
                group_pref_on(prefs, key)
            };
            rows = rows.push(preference_switch(
                Some(group_pref_icon(key)),
                label,
                Some(group_feature_summary(key)),
                on,
                owner.then_some(Message::GroupPrefSet(key, !on)),
                d,
                hover,
            ));
            if key == "timedMessages" && on {
                let current = prefs
                    .and_then(|p| p.pointer("/timedMessages/ttl"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(86_400);
                let label = TtlChoice::from_secs(Some(current)).to_string();
                let control = if owner {
                    choice_chip(label, Message::OpenPrefMenu(PrefMenu::GroupTtl), d, hover)
                } else {
                    text(label)
                        .size(d.meta)
                        .color(theme::t().text_secondary)
                        .into()
                };
                rows = rows.push(pref_row(None, "Delete after", control, PrefTrail::None, d));
            }
        }
        if title == "Members" && owner && !g.is_channel {
            let on = g
                .member_admission
                .as_ref()
                .and_then(|v| v.get("review"))
                .is_some();
            rows = rows.push(preference_switch(
                Some(icons::review_file_icon(18.0, theme::t().accent)),
                "Review new members",
                Some("Approve requests before people join."),
                on,
                Some(Message::GroupAdmissionSet(!on)),
                d,
                hover,
            ));
        }
        let note = if title == "Messages" {
            Some(if !owner {
                "Only owners can change these preferences."
            } else if g.is_channel {
                "Changes apply to everyone in this channel."
            } else {
                "Changes apply to everyone in this group."
            })
        } else {
            None
        };
        groups = groups.push(preference_group(Some(title), note, rows, d));
    }
    if !g.is_channel {
        let row = info_row(
            "Send receipts",
            crate::SettingInfo::GroupReceipts,
            choice_chip(
                RcptChoice::from_opt(app.group_send_rcpts()).label(app.receipts().1),
                Message::OpenPrefMenu(PrefMenu::GroupRcpts),
                d,
                hover,
            ),
            d,
            hover,
        );
        groups = groups.push(preference_group(
            Some("Just for you"),
            Some("These choices only affect your account."),
            row,
            d,
        ));
    }
    groups.into()
}

fn group_feature_summary(key: &str) -> &'static str {
    match key {
        "timedMessages" => "Automatically remove messages after a set time.",
        "directMessages" => "Let members connect with each other.",
        "fullDelete" => "Allow senders to remove messages for everyone.",
        "history" => "Share recent messages with new members.",
        "reactions" => "Respond with a quick emoji.",
        "voice" => "Record and share audio messages.",
        "files" => "Share photos, videos and documents.",
        "simplexLinks" => "Share SimpleX invitations in messages.",
        _ => "Give members a private way to reach admins.",
    }
}

/// TTL pick_list entry — `Never` clears / disables disappearing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtlChoice {
    Never,
    Secs(i64),
}

impl TtlChoice {
    const OPTIONS: [Self; 7] = [
        Self::Never,
        Self::Secs(30),
        Self::Secs(300),
        Self::Secs(3_600),
        Self::Secs(86_400),
        Self::Secs(604_800),
        Self::Secs(2_592_000),
    ];

    fn from_secs(secs: Option<i64>) -> Self {
        match secs {
            Some(s) if s > 0 => Self::OPTIONS
                .iter()
                .copied()
                .find(|o| matches!(o, Self::Secs(v) if *v == s))
                .unwrap_or(Self::Secs(s)),
            _ => Self::Never,
        }
    }

    fn secs(self) -> Option<i64> {
        match self {
            Self::Never => None,
            Self::Secs(s) => Some(s),
        }
    }
}

impl std::fmt::Display for TtlChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => f.write_str("Never"),
            Self::Secs(s) => f.write_str(&crate::format_timed_ttl(*s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimedAllowChoice {
    Off,
    On,
    ForMe,
}

impl TimedAllowChoice {
    const OPTIONS: [Self; 3] = [Self::Off, Self::On, Self::ForMe];

    fn from_allow(a: FeatureAllow) -> Self {
        match a {
            FeatureAllow::No => Self::Off,
            FeatureAllow::Yes => Self::On,
            FeatureAllow::Always => Self::ForMe,
        }
    }

    fn to_allow(self) -> FeatureAllow {
        match self {
            Self::Off => FeatureAllow::No,
            Self::On => FeatureAllow::Yes,
            Self::ForMe => FeatureAllow::Always,
        }
    }
}

impl std::fmt::Display for TimedAllowChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "Off",
            Self::On => "On",
            Self::ForMe => "For me",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RcptChoice {
    Default,
    On,
    Off,
}

impl RcptChoice {
    const OPTIONS: [Self; 3] = [Self::Default, Self::On, Self::Off];

    fn from_opt(v: Option<bool>) -> Self {
        match v {
            None => Self::Default,
            Some(true) => Self::On,
            Some(false) => Self::Off,
        }
    }

    fn to_opt(self) -> Option<bool> {
        match self {
            Self::Default => None,
            Self::On => Some(true),
            Self::Off => Some(false),
        }
    }
}

impl RcptChoice {
    /// "Default (on)" shows what the account-wide setting resolves to.
    fn label(self, default_on: bool) -> String {
        match self {
            Self::Default => format!("Default ({})", if default_on { "on" } else { "off" }),
            Self::On => "On".into(),
            Self::Off => "Off".into(),
        }
    }
}

fn group_member_info_pane(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'_, Message> {
    let Some(member_id) = g.selected_member_id else {
        return text("Member not found.")
            .size(d.body)
            .color(theme::t().text_muted)
            .into();
    };
    let Some(m) = g.members.iter().find(|m| m.group_member_id == member_id) else {
        return text("Member not found.")
            .size(d.body)
            .color(theme::t().text_muted)
            .into();
    };
    let admin = can_manage_members(&g.my_role);
    let mod_plus = can_moderate(&g.my_role);
    let pending = member_status_is_pending(&m.status);
    let mut identity = column![
        text(if m.is_me { "You".to_owned() } else { m.display_name.clone() }).size(d.title).font(BOLD).color(theme::t().text_primary),
        row![icons::role_icon(&m.role, 16.0, theme::t().text_secondary),
            text(format!("{} · {}", role_label(&m.role), m.status)).size(d.meta).color(theme::t().text_secondary)]
            .spacing(6).align_y(Alignment::Center),
    ].spacing(4).width(Length::Fill);
    if !m.public_name.is_empty() && m.public_name != m.display_name {
        identity = identity.push(text(m.public_name.clone()).size(d.meta).color(theme::t().text_secondary));
    }
    let mut col = column![row![profile_avatar(m.image.as_deref(), &m.display_name, 64.0), identity]
        .spacing(14).align_y(Alignment::Center)].spacing(if d.list_width == 0.0 { 4 } else { 12 }).width(Length::Fill);
    if let Some(bio) = m.short_descr.as_deref().filter(|bio| !bio.trim().is_empty()) {
        col = col.push(pref_section("Bio", false, d));
        col = col.push(container(profile_rich(bio, d.body, theme::t().text_secondary, false))
            .padding(Padding::from([0.0, crate::preference_ui::SECTION_INSET])));
    }
    if m.is_me { return col.into(); }
    let mut main = Column::new().width(Length::Fill);
    if !pending && (m.contact_id.is_some() || (!g.is_channel && group_pref_on(g.group_preferences.as_ref(), "directMessages"))) {
        main = main.push(card_row(icons::paperplane_icon(20.0, theme::t().accent), "Direct message", None,
            Message::OpenMemberDirectChat, None, d, hover));
    }
    if mod_plus && !g.is_channel && (m.support_chat.is_some() ||
        (group_support_allowed(g.group_preferences.as_ref()) && group_role_rank(&m.role) < group_role_rank("moderator"))) {
        main = main.push(card_row(icons::flag_icon(20.0, theme::t().accent),
            if group_role_rank(&m.role) >= group_role_rank("moderator") { "Chat with admin" } else { "Chat with member" },
            None, Message::OpenMemberSupportChat { member_id, title: m.display_name.clone() }, None, d, hover));
    }
    if admin && !pending {
        main = main.push(card_row(icons::role_icon(&m.role, 20.0, theme::t().accent), "Role", None,
            Message::OpenMemberRoleMenu(member_id), Some(row_value(role_label(&m.role), d)), d, hover));
    }
    // Immediate actions: no chevron (chevrons mean "opens a page / menu").
    if mod_plus {
        main = main.push(action_row(icons::block_prohibited_icon(20.0, theme::t().accent),
            if m.blocked_by_admin { "Unblock for all" } else { "Block for all" },
            Message::GroupMemberToggleBlockAll, d, hover));
    }
    col = col.push(vertical().height(8));
    col = col.push(button_card(main));
    if pending && mod_plus {
        let mut review = Column::new().width(Length::Fill);
        for (label, role) in [("Accept as member", "member"), ("Accept as observer", "observer")] {
            review = review.push(card_row(icons::role_icon(role, 20.0, theme::t().accent), label, None,
                Message::GroupMemberAccept { member_id, role: role.into() }, None, d, hover));
        }
        review = review.push(danger_row(icons::remove_profile_icon(20.0, theme::t().danger), "Reject",
            Message::GroupMemberRemove { member_id, with_messages: false }, d, hover));
        col = col.push(pref_section("Waiting for review", false, d));
        col = col.push(button_card(review));
    }
    if admin && !pending {
        col = col.push(pref_section("Manage member", false, d));
        col = col.push(button_card(column![
            danger_row(icons::remove_profile_icon(20.0, theme::t().danger), "Remove member",
                Message::GroupMemberRemove { member_id, with_messages: false }, d, hover),
            danger_row(icons::trash_icon(20.0, theme::t().danger), "Remove and delete messages",
                Message::GroupMemberRemove { member_id, with_messages: true }, d, hover),
        ]));
    }
    col.into()
}

/// The current value of a row that opens a choice: quiet text + chevron
/// (the whole row is the button, so the value is not a second button).
fn row_value<'a>(value: &str, d: Density) -> Element<'a, Message> {
    row![
        text(value.to_string()).size(d.meta).color(theme::t().text_secondary),
        icons::chevron_right(16.0, theme::t().text_muted),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// An immediate action inside a panel: glyph and label, no chevron.
fn action_row<'a>(
    icon: Element<'a, Message>,
    label: &'a str,
    on_press: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    group_action_tile(icon, label, on_press, theme::ghost_tile_r(hover, CARD_RADIUS), d, false, false).into()
}

/// A destructive row inside a panel: red glyph and label, red hover.
fn danger_row<'a>(
    icon: Element<'a, Message>,
    label: &'a str,
    on_press: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    group_action_tile(icon, label, on_press, theme::ghost_danger_r(hover, CARD_RADIUS), d, false, true).into()
}

fn create_chat_form(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(draft) = app.create_chat() else {
        return vertical().height(0).into();
    };
    let (title, hint, placeholder_name) = match draft.kind {
        CreateKind::Group => (
            "New group",
            "Private group — invite contacts after creating.",
            "Group name",
        ),
        CreateKind::Channel => (
            "New channel",
            "Public channel using your enabled chat relays (from SimpleX network settings).",
            "Channel name",
        ),
    };
    let card = menu_card(
        column![
            text(title).size(d.body).color(theme::t().text_primary),
            text(hint).size(d.meta).color(theme::t().text_secondary),
            crate::edit_menu::edit_field(
                text_input(placeholder_name, &draft.name)
                    .id("edit-field-CreateChatNameChanged")
                    .on_input(Message::CreateChatNameChanged)
                    .on_submit(Message::ConfirmCreateChat)
                    .padding(d.composer)
                    .size(d.body)
                    .width(Length::Fill)
                    .style(theme::composer_input()),
                iced::widget::Id::new("edit-field-CreateChatNameChanged"),
                &draft.name,
                Message::CreateChatNameChanged,
                true,
            ),
            row![
                button(text("Cancel").size(d.meta))
                    .on_press(Message::CloseCreateChat)
                    .padding(Padding::from([10, 14]))
                    .style(theme::secondary_action(hover)),
                button(text("Create").size(d.meta))
                    .on_press(Message::ConfirmCreateChat)
                    .padding(Padding::from([10, 14]))
                    .style(theme::primary_action(hover)),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into(),
        d,
    );
    card
}

fn create_chat_overlay(app: &App) -> Element<'_, Message> {
    sheet_overlay(create_chat_form(app), Message::CloseCreateChat)
}


#[cfg(test)]
mod stamp_tests {
    //! The time + receipt stamp must stay inside its bubble at every width,
    //! with emoji (wide, colour glyphs) anywhere in the text.
    use super::*;
    use iced::advanced::layout::{Layout, Limits, Node};
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::Tree;

    fn renderer() -> iced::Renderer {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
        {
            let mut fs = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            for f in ["NotoSans-Regular.ttf", "NotoColorEmoji.ttf"] {
                if let Ok(bytes) = std::fs::read(dir.join(f)) {
                    fs.load_font(std::borrow::Cow::Owned(bytes));
                }
            }
        }
        iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            Font::with_name("Noto Sans"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer")
    }

    fn message(text: &str, outgoing: bool, edited: bool, at: &str) -> ChatMessage {
        ChatMessage {
            item_id: 1,
            text: text.into(),
            caption: String::new(),
            segments: Vec::new(),
            outgoing,
            created_at: Some(at.into()),
            delivery: cursus_core::DeliveryStatus::Delivered,
            file: None,
            reactions: Vec::new(),
            quote: None,
            deleted: None,
            sender: None,
            is_announcement: false,
            group_invitation: None,
            edited,
            editable: false,
        }
    }

    fn layout(r: &iced::Renderer, mut el: Element<'static, Message>, max_w: f32) -> Node {
        let mut tree = Tree::new(&el);
        el.as_widget_mut().layout(
            &mut tree,
            r,
            &Limits::new(Size::ZERO, Size::new(max_w, 10_000.0)),
        )
    }

    fn body(msg: &ChatMessage, d: Density) -> Element<'static, Message> {
        if msg.is_emoji_only() {
            let reserve = stamp_reserve_span(msg, d, emoji_text_size(&msg.text));
            emoji_message_text(&msg.text, Some(reserve))
        } else {
            plain_text_body(&msg.text, d, Some(stamp_reserve_span(msg, d, d.body)))
        }
    }

    const TEXTS: &[&str] = &[
        "Infatti non sto tanto bene 😔😔😔",
        "Ma tu oggi vai dal dottore? 🤓",
        "Dopo ci sentiamo 🥰",
        "Così così 😔",
        "😝😝😝",
        "🥰",
        "😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔😔",
        "ok 👨‍👩‍👧 🇮🇹 👍🏽",
        "Tu hai dormito bene??",
        "precipitevolissimevolmente😂😂😂",
        "a",
    ];
    const TIMES: &[&str] = &[
        "2026-11-07T12:44:00Z",
        "2026-11-07T01:23:00Z",
        "2026-11-07T23:59:00Z",
    ];

    fn densities() -> Vec<Density> {
        vec![
            Density::for_window(crate::layout::LayoutMode::Mobile, 390.0, 800.0),
            Density::for_window(crate::layout::LayoutMode::Mobile, 560.0, 1200.0),
            Density::for_window(crate::layout::LayoutMode::Desktop, 1400.0, 900.0),
        ]
    }

    /// The invisible reservation is at least as wide as the stamp drawn over
    /// it, so the stamp never covers the end of the text.
    #[test]
    fn reservation_covers_the_stamp() {
        let r = renderer();
        for d in densities() {
            for &at in TIMES {
                for (outgoing, edited) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let msg = message("x", outgoing, edited, at);
                    // Without the leading breakable space (that's the gap).
                    let reserved = stamp_reserve(&msg).trim_start_matches(' ').to_string();
                    let reserve_w = layout(
                        &r,
                        rich_text::<Message, _, _, _>(vec![span(reserved).size(d.meta)]).into(),
                        10_000.0,
                    )
                    .size()
                    .width;
                    let stamp_w = layout(&r, bubble_time_meta(&msg, d), 10_000.0).size().width;
                    assert!(
                        reserve_w >= stamp_w,
                        "reserve {reserve_w} < stamp {stamp_w} (meta {}, out {outgoing}, edited {edited}, {at})",
                        d.meta
                    );
                }
            }
        }
    }

    /// Screenshot bug: in a group chat the stamp of an emoji line was drawn
    /// past the bubble edge. At every width the stamp stays inside the body.
    #[test]
    fn stamp_stays_inside_the_bubble_with_emoji() {
        let r = renderer();
        for d in densities() {
            for &text in TEXTS {
                for &at in TIMES {
                    for outgoing in [false, true] {
                        let msg = message(text, outgoing, outgoing, at);
                        let mut w = 90.0;
                        while w <= 460.0 {
                            let node =
                                layout(&r, bubble_content_with_time(body(&msg, d), &msg, d), w);
                            let outer = node.bounds();
                            assert!(
                                outer.width <= w + 0.5,
                                "{text:?} at {w}: body {} wider than the bubble",
                                outer.width
                            );
                            let l = Layout::new(&node);
                            let mut kids = l.children();
                            let text_box = kids.next().unwrap().bounds();
                            let stamp = kids.next().unwrap().children().next().unwrap().bounds();
                            assert!(
                                stamp.x >= outer.x - 0.5
                                    && stamp.x + stamp.width <= outer.x + outer.width + 0.5
                                    && stamp.y + stamp.height <= outer.y + outer.height + 0.5
                                    && stamp.y >= outer.y - 0.5,
                                "{text:?} at {w}: stamp {stamp:?} outside {outer:?}"
                            );
                            assert_eq!(
                                text_box.height, outer.height,
                                "{text:?}: stamp must not grow the body"
                            );
                            w += 9.0;
                        }
                    }
                }
            }
        }
    }

    /// Short reply under a long quote: the stamp sits at the bubble's
    /// bottom-right, not right after the text; the bubble keeps the quote's
    /// width. A reply longer than its quote still sets the width itself.
    #[test]
    fn reply_stamp_sits_at_the_right_edge() {
        let r = renderer();
        let d = densities()[0];
        let quote = cursus_core::MessageQuote {
            item_id: None,
            text: "SimpleX group link (via smp15.simplex.im)".into(),
            outgoing: false,
        };
        for (text, outgoing) in [("hi!", true), ("ok 👍", false), ("😝", true)] {
            let msg = message(text, outgoing, false, TIMES[1]);
            let bubble = crate::bubble_body::BubbleBody::new(4.0)
                .push(quote_preview(&quote, d))
                .stretch_last(true)
                .push(bubble_content_with_time(body(&msg, d), &msg, d));
            let node = layout(&r, bubble.into(), 360.0);
            let l = Layout::new(&node);
            let mut kids = l.children();
            let quote_box = kids.next().unwrap().bounds();
            let stamped = kids.next().unwrap();
            let stamp = stamped
                .children()
                .nth(1)
                .unwrap()
                .children()
                .next()
                .unwrap()
                .bounds();
            assert_eq!(
                node.bounds().width,
                quote_box.width,
                "{text:?}: the quote sets the width"
            );
            assert!(
                (stamp.x + stamp.width - quote_box.width).abs() < 0.5,
                "{text:?}: stamp ends at {} but the bubble at {}",
                stamp.x + stamp.width,
                quote_box.width
            );
        }
        let long = message(
            "A much longer answer than the quoted line above, so it wraps",
            true,
            false,
            TIMES[1],
        );
        let bubble = crate::bubble_body::BubbleBody::new(4.0)
            .push(quote_preview(&quote, d))
            .stretch_last(true)
            .push(bubble_content_with_time(body(&long, d), &long, d));
        let node = layout(&r, bubble.into(), 360.0);
        let stamped = Layout::new(&node).children().nth(1).unwrap().bounds();
        assert_eq!(
            stamped.width,
            node.bounds().width,
            "the long reply sets the width"
        );
        assert!(node.bounds().width <= 360.0);
    }

    /// A short line keeps the stamp on the same line (no extra row).
    #[test]
    fn short_line_keeps_stamp_inline() {
        let r = renderer();
        let d = densities()[0];
        for text in ["Nono 🥰", "Nono", "Dopo ci sentiamo 🥰"] {
            let msg = message(text, false, false, TIMES[0]);
            let with = layout(&r, body(&msg, d), 300.0).size().height;
            let without = layout(&r, plain_text_body(&msg.text, d, None), 300.0)
                .size()
                .height;
            assert_eq!(with, without, "{text:?}");
        }
        let big = message("😝😝😝", false, false, TIMES[0]);
        let with = layout(&r, body(&big, d), 300.0).size().height;
        let without = layout(&r, emoji_message_text(&big.text, None), 300.0)
            .size()
            .height;
        assert_eq!(with, without, "big emoji line keeps its height");
    }
}

#[cfg(test)]
mod enter_tests {
    use super::*;
    use iced::widget::text_editor::{KeyPress, Status};

    fn enter(modifiers: keyboard::Modifiers) -> KeyPress {
        KeyPress {
            key: keyboard::Key::Named(key::Named::Enter),
            modified_key: keyboard::Key::Named(key::Named::Enter),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            modifiers,
            text: Some("\r".into()),
            status: Status::Focused { is_hovered: false },
        }
    }

    fn sends(b: Option<EditorBinding<Message>>) -> bool {
        matches!(b, Some(EditorBinding::Custom(Message::SendKey)))
    }

    fn newline(b: Option<EditorBinding<Message>>) -> bool {
        matches!(b, Some(EditorBinding::Enter))
    }

    /// Enter sends through `SendKey`, which skips the touch-gesture gate
    /// (`Send`, for the button, keeps it) — Enter used to be dropped after
    /// scrolling the history on a touchscreen.
    #[test]
    fn enter_sends_by_default() {
        let bind = composer_key_binding(false, false, true);
        assert!(sends(bind(enter(keyboard::Modifiers::empty()))));
        assert!(newline(bind(enter(keyboard::Modifiers::SHIFT))));
    }

    #[test]
    fn enter_adds_a_line_when_the_setting_is_off() {
        let bind = composer_key_binding(false, false, false);
        assert!(newline(bind(enter(keyboard::Modifiers::empty()))));
        assert!(newline(bind(enter(keyboard::Modifiers::SHIFT))));
        assert!(sends(bind(enter(keyboard::Modifiers::CTRL))));
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_graphemes;

    #[test]
    fn epoch_timestamps_get_no_day_divider() {
        let msg = |ts: &str| cursus_core::ChatMessage {
            item_id: 1,
            text: String::new(),
            caption: String::new(),
            segments: Vec::new(),
            outgoing: false,
            created_at: Some(ts.into()),
            delivery: cursus_core::DeliveryStatus::Delivered,
            file: None,
            reactions: Vec::new(),
            quote: None,
            deleted: None,
            sender: None,
            is_announcement: true,
            group_invitation: None,
            edited: false,
            editable: false,
        };
        assert!(super::divider_time(&msg("1970-01-01T00:00:00Z")).is_none());
        assert!(super::divider_time(&msg("2026-11-05T10:00:00Z")).is_some());
    }

    #[test]
    fn truncation_keeps_emoji_sequences_whole() {
        let family = "👨\u{200D}👩\u{200D}👧";
        let s = format!("{family}🇮🇹 ciao");
        // 2 visible characters, not 2 code points.
        assert_eq!(truncate_graphemes(&s, 2), format!("{family}🇮🇹…"));
        assert_eq!(truncate_graphemes("short", 10), "short");
    }
}

#[cfg(test)]
mod wave_layout_tests {
    use super::wave_layout;

    #[test]
    fn bars_and_gaps_sit_on_whole_pixels() {
        let peaks: Vec<f32> = (0..96).map(|i| i as f32 / 95.0).collect();
        for &(scale, width) in &[(1.0_f32, 620.0_f32), (1.25, 775.0), (2.0, 1240.0)] {
            let bars = wave_layout(&peaks, width, 36.0 * scale, scale);
            assert!(!bars.is_empty());
            for (i, b) in bars.iter().enumerate() {
                for v in [b.x, b.w, b.y, b.h] {
                    assert_eq!(v, v.round(), "scale {scale}: fractional {v}");
                }
                assert!(b.x + b.w <= width);
                assert!(b.h >= b.w, "bar shorter than wide: eye-shaped when rounded");
                if i > 0 {
                    let prev = bars[i - 1];
                    assert_eq!(b.x - (prev.x + prev.w), (2.0 * scale).round());
                }
            }
        }
    }

    #[test]
    fn resampling_keeps_the_loud_spot() {
        let mut peaks = vec![0.05; 96];
        peaks[40] = 1.0;
        let bars = wave_layout(&peaks, 200.0, 36.0, 1.0);
        assert!(bars.iter().any(|b| b.peak == 1.0));
    }
}

/// Gold square beside "+" with the number of unused one-time links; opens
/// their list.
fn pending_links_button<'a>(count: usize, hover: bool) -> Element<'a, Message> {
    attention_button(
        icons::link_icon(20.0, theme::on_attention()),
        count as i64,
        Message::OpenPendingLinks,
        hover,
    )
}

/// Yellow "needs attention" square (same box as "+") with a round count
/// on its top-right corner: pending links, unread support chats.
fn attention_button<'a>(
    icon: Element<'a, Message>,
    count: i64,
    on_press: Message,
    hover: bool,
) -> Element<'a, Message> {
    let btn = button(icons::centered_hit(icon, 36.0))
        .on_press(on_press)
        .padding(0)
        .style(theme::attention_action(hover));
    // Same 36×36 box as "+" (header height unchanged); the count is centred
    // on the top-right corner, drawn over the edge with `float`.
    Stack::new()
        .width(36.0)
        .height(36.0)
        .push(btn)
        .push(
            container(float(count_badge(count)).translate(|bounds, _| {
                iced::Vector::new(bounds.width / 2.0, -bounds.height / 2.0)
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Start),
        )
        .into()
}

/// Round accent count (a pill past 9), no border.
fn count_badge<'a>(count: i64) -> Element<'a, Message> {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let one = count < 10;
    container(
        text(label)
            .size(10.0)
            .font(BOLD)
            .color(theme::t().accent_on),
    )
    .height(16.0)
    .width(if one {
        Length::Fixed(16.0)
    } else {
        Length::Shrink
    })
    .padding(Padding::from([0, if one { 0 } else { 5 }]))
    .center_x(if one {
        Length::Fixed(16.0)
    } else {
        Length::Shrink
    })
    .center_y(16.0)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().accent)),
        border: iced::Border {
            radius: theme::CONTROL_RADIUS.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Count in the yellow attention style, for list rows (support chats).
fn attention_count<'a>(count: i64) -> Element<'a, Message> {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let one = count < 10;
    container(
        text(label)
            .size(11.0)
            .font(BOLD)
            .color(theme::on_attention()),
    )
    .height(20.0)
    .width(if one {
        Length::Fixed(20.0)
    } else {
        Length::Shrink
    })
    .padding(Padding::from([0, if one { 0 } else { 6 }]))
    .center_x(if one {
        Length::Fixed(20.0)
    } else {
        Length::Shrink
    })
    .center_y(20.0)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().attention)),
        border: iced::Border {
            radius: theme::CONTROL_RADIUS.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Dimmed backdrop (tap closes) with `card` centred — like the other sheets.
fn sheet_overlay<'a>(card: Element<'a, Message>, dismiss: Message) -> Element<'a, Message> {
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..container::Style::default()
            }),
    )
    .on_press(dismiss);
    // Opaque card: taps on its empty parts neither close the sheet nor
    // reach the backdrop; opaque stack: nothing underneath gets them either.
    let sheet = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::from([24, 20]));
    iced::widget::opaque(
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(backdrop)
            .push(sheet),
    )
}

/// Unused one-time links: copy (short / full), name, delete. Anyone holding
/// one can still connect, so they stay visible here until used or deleted.
fn pending_links_overlay(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let mut col = Column::new().spacing(12).max_width(480);
    col = col.push(
        row![
            icon_disc(icons::link_icon(18.0, theme::t().attention), 36.0),
            text("Unused invitations")
                .size(d.body)
                .font(BOLD)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            button(icons::centered_hit(
                icons::cancel_icon(18.0, theme::t().text_secondary),
                32.0
            ))
            .on_press(Message::ClosePendingLinks)
            .padding(0)
            .style(theme::icon_action(hover)),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );
    col = col.push(
        text("One-time links you created that nobody has used yet. Anyone who has one can still connect \u{2014} delete the ones you no longer need.")
            .size(d.meta)
            .color(theme::t().text_secondary)
            .wrapping(Wrapping::WordOrGlyph),
    );
    let links = app.pending_links();
    if links.is_empty() {
        col = col.push(
            text("No unused invitations.")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    }
    let small = |label: &'static str,
                 icon: Element<'static, Message>,
                 msg: Message|
     -> Element<'static, Message> {
        button(
            row![icon, text(label).size(d.meta)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(msg)
        .padding(Padding::from([6, 10]))
        .style(theme::secondary_action(hover))
        .into()
    };
    let mut list = Column::new().spacing(10);
    for inv in links {
        let id = inv.conn_id;
        let name = if inv.alias.trim().is_empty() {
            "Unnamed link".to_string()
        } else {
            inv.alias.clone()
        };
        let mut when = format_list_time(Some(&inv.created_at))
            .map(|t| format!("Created {t}"))
            .unwrap_or_default();
        if inv.incognito {
            when = if when.is_empty() {
                "Incognito".into()
            } else {
                format!("{when} \u{b7} incognito")
            };
        }
        let mut item = Column::new().spacing(8);
        match app.renaming_link().filter(|(rid, _)| *rid == id) {
            Some((_, draft)) => {
                item = item.push(
                    row![
                        text_input("Name, e.g. who it's for", draft)
                            .on_input(Message::PendingLinkNameChanged)
                            .on_submit(Message::SavePendingLinkName)
                            .padding(Padding::from([8, 12]))
                            .size(d.body)
                            .width(Length::Fill)
                            .style(theme::rounded_field()),
                        button(text("Save").size(d.meta))
                            .on_press(Message::SavePendingLinkName)
                            .padding(Padding::from([8, 14]))
                            .style(theme::primary_action(hover)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            None => {
                item = item.push(
                    row![
                        column![
                            text(name)
                                .size(d.body)
                                .color(if inv.alias.trim().is_empty() {
                                    theme::t().text_secondary
                                } else {
                                    theme::t().text_primary
                                }),
                            text(when).size(d.meta).color(theme::t().text_muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        button(icons::centered_hit(
                            icons::trash_icon(16.0, theme::t().danger),
                            32.0
                        ))
                        .on_press(Message::AskDeletePendingLink(id))
                        .padding(0)
                        .style(theme::icon_action(hover)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }
        let mut actions = Row::new().spacing(6).align_y(Alignment::Center);
        if inv.link.short.is_some() {
            actions = actions.push(small(
                "Copy",
                icons::copy_icon(14.0, theme::t().text_primary),
                Message::CopyPendingLink {
                    conn_id: id,
                    full: false,
                },
            ));
            actions = actions.push(small(
                "Copy full",
                icons::copy_icon(14.0, theme::t().text_primary),
                Message::CopyPendingLink {
                    conn_id: id,
                    full: true,
                },
            ));
        } else {
            actions = actions.push(small(
                "Copy",
                icons::copy_icon(14.0, theme::t().text_primary),
                Message::CopyPendingLink {
                    conn_id: id,
                    full: true,
                },
            ));
        }
        actions = actions.push(small(
            "Name",
            icons::edit_icon(14.0, theme::t().text_primary),
            Message::RenamePendingLink(id),
        ));
        item = item.push(actions);
        list = list.push(
            container(item)
                .padding(Padding::from([10, 12]))
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(theme::t().bg_field)),
                    border: iced::Border {
                        color: theme::t().border,
                        width: 1.0,
                        radius: theme::CONTROL_RADIUS.into(),
                    },
                    ..container::Style::default()
                }),
        );
    }
    col = col.push(crate::tap::guard(
        scrollable(crate::touch_content::content(crate::tap::defer_touch(list)))
            .height(Length::Shrink),
    ));
    sheet_overlay(menu_card(col.into(), d), Message::ClosePendingLinks)
}

/// "Replace link" / delete a pending link: the link stops working, so ask.
fn link_confirm_overlay(app: &App, confirm: crate::LinkConfirm) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let (title, body, action, msg) = match confirm {
        crate::LinkConfirm::Regenerate => (
            "Create a new link?",
            "The previous link will be deleted. Anyone you already sent it to won't be able to connect with it.",
            "Delete and create new",
            Message::ConfirmRegenerateInvitation,
        ),
        crate::LinkConfirm::Delete(_) => (
            "Delete this link?",
            "It will stop working. Anyone you sent it to won't be able to connect with it.",
            "Delete",
            Message::ConfirmDeletePendingLink,
        ),
        crate::LinkConfirm::DeleteAddress => (
            "Delete address?",
            "All your contacts will remain connected. Nobody new can use the address, and your profile is updated for your contacts.",
            "Delete address",
            Message::ConfirmDeleteAddress,
        ),
    };
    let card = column![
        text(title)
            .size(d.body)
            .font(BOLD)
            .color(theme::t().text_primary),
        text(body)
            .size(d.meta)
            .color(theme::t().text_secondary)
            .wrapping(Wrapping::WordOrGlyph),
        row![
            horizontal().width(Length::Fill),
            button(text("Cancel").size(d.meta))
                .on_press(Message::CancelLinkConfirm)
                .padding(Padding::from([10, 14]))
                .style(theme::secondary_action(hover)),
            button(text(action).size(d.meta))
                .on_press(msg)
                .padding(Padding::from([10, 14]))
                .style(theme::danger_action(hover)),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .max_width(420);
    sheet_overlay(menu_card(card.into(), d), Message::CancelLinkConfirm)
}

/// A modal viewer shares the chat's downloaded images and never launches an OS app.
fn image_gallery(app: &App) -> Element<'_, Message> {
    let Some((path, zoom)) = &app.image_viewer else {
        return Space::new().into();
    };
    let items = app.gallery_cache.items();
    let index = items.iter().position(|(item, _)| item == path).unwrap_or(0);
    let tool = |icon, message| {
        button(icons::centered_hit(icon, 36.0))
            .padding(0)
            .on_press(message)
            .style(theme::icon_action(app.hover_ok()))
    };
    let title = items.get(index).map_or_else(String::new, |(_, name)| name.clone());
    let header = row![
        tool(icons::cancel_icon(20.0, iced::Color::WHITE), Message::CloseImage),
        container(crate::ellipsis::ellipsis_text(title, 16.0, iced::Color::WHITE, 1)).width(Length::Fill).clip(true),
        tool(
            icons::minus_icon(20.0, iced::Color::WHITE),
            Message::ImageZoom(0.8)
        ),
        tool(
            icons::plus_icon(20.0, iced::Color::WHITE),
            Message::ImageZoom(1.25)
        ),
    ]
    .spacing(8)
    .padding(8)
    .align_y(Alignment::Center)
    .height(56);
    let previous = button(icons::centered_hit(icons::back_icon(20.0, iced::Color { a: if index > 0 { 1.0 } else { 0.3 }, ..iced::Color::WHITE }), 36.0))
        .on_press_maybe((index > 0).then_some(Message::ImageStep(-1))).padding(0).style(theme::icon_action(app.hover_ok()));
    let next = button(icons::centered_hit(icons::next_icon(20.0, iced::Color { a: if index + 1 < items.len() { 1.0 } else { 0.3 }, ..iced::Color::WHITE }), 36.0))
        .on_press_maybe((index + 1 < items.len()).then_some(Message::ImageStep(1))).padding(0).style(theme::icon_action(app.hover_ok()));
    let footer = row![previous, text(format!("{} / {}", index + 1, items.len().max(1))).color(iced::Color::WHITE), next]
    .spacing(16)
    .align_y(Alignment::Center);
    container(column![
        header,
        if let Some(handle) = app.gallery_cache.get(path) {
            crate::image_gestures::from_handle(path, *zoom, handle)
        } else if let Some((preview, _, _)) = app.image_preview_for(path, 220, 280) {
            image(iced::widget::image::Handle::from_path(preview))
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            container(
                text(if app.gallery_cache.failed(path) {
                    "Could not open this image."
                } else {
                    "Loading image…"
                })
                .color(iced::Color::WHITE),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        },
        container(footer).center_x(Length::Fill).padding(8)
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Color::BLACK.into()),
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod swipe_control_tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[test]
    fn slow_swipes_ending_on_the_original_control_never_activate_it() {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let d = Density::for_window(LayoutMode::Mobile, 400.0, 300.0);
        for kind in ["navigation", "danger", "switch", "choice", "link"] {
            for held in [false, true] {
                let mut cache = Cache::default();
                let mut messages = vec![];
                let origin = iced::Point::new(
                    if kind == "choice" { 40.0 } else { 120.0 },
                    if kind == "choice" { 164.0 } else { 180.0 },
                );
                let mut events = vec![(
                    iced::Event::Touch(iced::touch::Event::FingerPressed {
                        id: iced::touch::Finger(33),
                        position: origin,
                    }),
                    origin,
                )];
                if held {
                    events.push((
                        iced::Event::Window(iced::window::Event::RedrawRequested(
                            std::time::Instant::now() + crate::tap::HOLD * 2,
                        )),
                        origin,
                    ));
                }
                // Each step is below slop. Scrolling translates the content back
                // under the finger, so measuring only content positions misses it.
                for step in 1..=10 {
                    let point = origin + iced::Vector::new(0.0, -3.0 * step as f32);
                    events.push((
                        iced::Event::Touch(iced::touch::Event::FingerMoved {
                            id: iced::touch::Finger(33),
                            position: point,
                        }),
                        point,
                    ));
                }
                let end = origin + iced::Vector::new(0.0, -30.0);
                events.push((
                    iced::Event::Touch(iced::touch::Event::FingerLifted {
                        id: iced::touch::Finger(33),
                        position: end,
                    }),
                    end,
                ));
                events.push((
                    iced::Event::Touch(iced::touch::Event::FingerPressed {
                        id: iced::touch::Finger(34),
                        position: end,
                    }),
                    end,
                ));
                events.push((
                    iced::Event::Touch(iced::touch::Event::FingerLifted {
                        id: iced::touch::Finger(34),
                        position: end,
                    }),
                    end,
                ));
                for (event, point) in events {
                    let control: Element<'_, Message> = match kind {
                        "navigation" => card_row(
                            icons::link_icon(18.0, theme::t().accent),
                            "Invite",
                            None,
                            Message::OpenAddGroupMember,
                            None,
                            d,
                            false,
                        ),
                        "danger" => button(text("Delete group"))
                            .on_press(Message::OpenAddGroupMember)
                            .padding(16)
                            .width(Length::Fill)
                            .style(theme::danger_action(false))
                            .into(),
                        "switch" => preference_switch(
                            None,
                            "Notifications",
                            None,
                            true,
                            Some(Message::OpenAddGroupMember),
                            d,
                            false,
                        ),
                        "choice" => choice_chip("Default", Message::OpenAddGroupMember, d, false),
                        _ => button(text("Create invite link"))
                            .on_press(Message::OpenAddGroupMember)
                            .padding(16)
                            .width(Length::Fill)
                            .into(),
                    };
                    let body = page_scroll_y(iced::widget::column![
                        Space::new().height(150),
                        control,
                        Space::new().height(1000)
                    ]);
                    let mut ui = UserInterface::build(
                        body,
                        iced::Size::new(400.0, 300.0),
                        cache,
                        &mut renderer,
                    );
                    let event = if matches!(event, iced::Event::Window(_)) {
                        iced::Event::Window(iced::window::Event::RedrawRequested(
                            std::time::Instant::now() + crate::tap::HOLD * 2,
                        ))
                    } else {
                        event
                    };
                    let swipe_lift = matches!(
                        event,
                        iced::Event::Touch(iced::touch::Event::FingerLifted {
                            id: iced::touch::Finger(33),
                            ..
                        })
                    );
                    let _ = ui.update(
                        &[event],
                        mouse::Cursor::Available(point),
                        &mut renderer,
                        &mut iced::advanced::clipboard::Null,
                        &mut messages,
                    );
                    if swipe_lift {
                        assert!(
                            !messages
                                .iter()
                                .any(|m| matches!(m, Message::OpenAddGroupMember)),
                            "{kind}, held={held}: scrolling must never activate a control"
                        );
                    }
                    cache = ui.into_cache();
                }
                assert_eq!(
                    messages
                        .iter()
                        .filter(|m| matches!(m, Message::OpenAddGroupMember))
                        .count(),
                    1,
                    "{kind}, held={held}: a real tap must still work immediately after swiping"
                );
            }
        }
    }
}

#[cfg(test)]
mod reaction_layout_tests {
    use super::*;
    use iced::advanced::{renderer::Headless, widget::Operation};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[test]
    fn reaction_rows_fit_narrow_windows_and_large_text() {
        struct Probe {
            width: f32,
            glyphs: usize,
        }
        impl Operation for Probe {
            fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) {
                op(self);
            }
            fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, _: &str) {
                assert!(
                    bounds.x >= 0.0 && bounds.x + bounds.width <= self.width + 0.1,
                    "reaction overflow at {}px: {bounds:?}",
                    self.width
                );
                self.glyphs += 1;
            }
        }
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        for width in [160.0, 220.0, 280.0] {
            for scale in [1.0, 1.6] {
                let d =
                    Density::for_window(LayoutMode::Mobile, width, 600.0).with_text_scale(scale);
                let mut ui = UserInterface::build(
                    reaction_picker_row(1, d, false),
                    iced::Size::new(width, 600.0),
                    Cache::default(),
                    &mut renderer,
                );
                let mut probe = Probe { width, glyphs: 0 };
                ui.operate(&renderer, &mut probe);
                assert_eq!(probe.glyphs, 8);
            }
        }
    }
}
