use iced::font::{self, Font};
use iced::widget::canvas::{self, Canvas, Fill, Frame, Geometry};
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::text::{Shaping, Wrapping};
use iced::keyboard;
use iced::keyboard::key;
use iced::widget::text_editor::Binding as EditorBinding;
use iced::widget::space::{horizontal, vertical};
use iced::widget::{
    button, column, container, image, mouse_area, progress_bar, rich_text, row, scrollable, span,
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

use crate::hover;
use crate::icons::{self, NavIcon};
use crate::layout::{Density, LayoutMode};
use crate::media;
use crate::theme;
use crate::{
    can_edit_group_profile, can_manage_members, can_moderate, group_pref_on, group_support_allowed,
    App, AuthScreen, ChatMenuAction, ChatMenuPane, CreateKind, GroupSettingsPane, Message,
    MsgMenuAction, MsgMenuPane, SupportPane,
};

/// Color-emoji family — use for glyph chips so ❤ becomes the red ❤️.
const EMOJI_FONT: Font = Font::with_name("Noto Color Emoji");

/// Invisible vertical scrollbar (still scrollable via wheel / touch).
fn no_bar() -> Scrollbar {
    Scrollbar::new().width(0).scroller_width(0).margin(0)
}

fn scroll_y<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content).direction(Direction::Vertical(no_bar()))
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
    let base: Element<'_, Message> = match app.layout() {
        LayoutMode::Desktop => desktop_shell(app),
        LayoutMode::Compact | LayoutMode::Mobile => stacked_shell(app),
    };
    let mut stack = Stack::new().width(Length::Fill).height(Length::Fill).push(base);
    if app.chat_menu().is_some() {
        stack = stack.push(chat_menu_overlay(app));
    }
    if app.media_viewer().is_some() {
        stack = stack.push(media_viewer_overlay(app));
    }
    if app.pending_url().is_some() {
        stack = stack.push(open_url_confirm_overlay(app));
    }
    if app.message_menu().is_some() {
        stack = stack.push(message_menu_overlay(app));
    }
    if app.reaction_members().is_some() {
        stack = stack.push(reaction_members_overlay(app));
    }
    if app.create_chat().is_some() {
        stack = stack.push(create_chat_overlay(app));
    }
    stack.into()
}

fn auth_gate(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let busy = app.connecting();
    let Some(auth) = app.auth() else {
        return vertical().height(0).into();
    };
    let card: Element<'_, Message> = match auth {
        AuthScreen::WelcomeName { name } => column![
            text("Cursus")
                .size(d.title)
                .color(theme::t().accent),
            text("Choose a display name for this profile.")
                .size(d.body)
                .color(theme::t().text_secondary),
            text_input("Your name", name)
                .on_input(Message::WelcomeNameChanged)
                .on_submit(Message::WelcomeNameNext)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            button(text(if busy { "Working…" } else { "Next" }).size(d.body))
                .on_press_maybe(if busy {
                    None
                } else {
                    Some(Message::WelcomeNameNext)
                })
                .padding(Padding::from([12, 18]))
                .style(theme::primary_action(hover)),
        ]
        .spacing(14)
        .into(),
        AuthScreen::WelcomePassword {
            name,
            password,
            confirm,
        } => column![
            text("Set database password")
                .size(d.title)
                .color(theme::t().text_primary),
            text(format!(
                "Hi {name}. This password encrypts your local chat database (SQLCipher). It is never stored — you will enter it each time you open Cursus."
            ))
            .size(d.meta)
            .color(theme::t().text_secondary),
            text_input("Password (min 8 characters)", password)
                .secure(true)
                .on_input(Message::WelcomePasswordChanged)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            text_input("Confirm password", confirm)
                .secure(true)
                .on_input(Message::WelcomeConfirmChanged)
                .on_submit(Message::WelcomeSubmit)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            row![
                button(text("Back").size(d.meta))
                    .on_press_maybe(if busy {
                        None
                    } else {
                        Some(Message::WelcomeBack)
                    })
                    .padding(Padding::from([10, 14]))
                    .style(theme::secondary_action(hover)),
                button(text(if busy { "Creating…" } else { "Create profile" }).size(d.meta))
                    .on_press_maybe(if busy {
                        None
                    } else {
                        Some(Message::WelcomeSubmit)
                    })
                    .padding(Padding::from([10, 14]))
                    .style(theme::primary_action(hover)),
            ]
            .spacing(8),
        ]
        .spacing(14)
        .into(),
        AuthScreen::Unlock {
            display_name,
            password,
        } => column![
            text("Unlock Cursus")
                .size(d.title)
                .color(theme::t().text_primary),
            text(format!("Welcome back, {display_name}."))
                .size(d.body)
                .color(theme::t().text_secondary),
            text_input("Database password", password)
                .secure(true)
                .on_input(Message::UnlockPasswordChanged)
                .on_submit(Message::UnlockSubmit)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            button(text(if busy { "Unlocking…" } else { "Unlock" }).size(d.body))
                .on_press_maybe(if busy {
                    None
                } else {
                    Some(Message::UnlockSubmit)
                })
                .padding(Padding::from([12, 18]))
                .style(theme::primary_action(hover)),
        ]
        .spacing(14)
        .into(),
    };
    let status = app.status_text().trim();
    let card: Element<'_, Message> = if !status.is_empty()
        && (status.to_lowercase().contains("error")
            || status.to_lowercase().contains("fail")
            || status.to_lowercase().contains("password")
            || status.to_lowercase().contains("wrong")
            || status.to_lowercase().contains("could not")
            || status.to_lowercase().contains("timed out"))
    {
        column![
            card,
            text(status.to_string())
                .size(d.meta)
                .color(theme::t().danger),
        ]
        .spacing(10)
        .into()
    } else {
        card
    };
    let panel = container(card)
        .width(Length::Fixed(420.0))
        .padding(Padding::from([28, 28]))
        .style(|_| {
            let c = theme::t();
            container::Style {
                background: Some(iced::Background::Color(c.bg_elevated)),
                border: iced::Border {
                    color: c.border,
                    width: 1.0,
                    radius: 16.0.into(),
                },
                ..container::Style::default()
            }
        });
    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_app)),
            ..container::Style::default()
        })
        .into()
}

fn desktop_shell(app: &App) -> Element<'_, Message> {
    let d = app.density();
    row![
        nav_rail(app, d),
        list_pane(app, Length::Fixed(d.list_width)),
        content_pane(app, false),
    ]
    .height(Length::Fill)
    .into()
}

fn stacked_shell(app: &App) -> Element<'_, Message> {
    let in_chat_nav = matches!(app.nav(), NavSection::Chats | NavSection::Channels);
    let show_detail = in_chat_nav
        && (app.selected().is_some()
            || app.contact_profile().is_some()
            || app.group_settings().is_some()
            || app.support().is_some()
            || app.compose_open());

    // Always keep list + bottom nav mounted (iOS-style). An open chat is an
    // opaque full-screen layer stacked on top — the tab bar stays underneath
    // rather than disappearing.
    let shell_base = column![
        container(list_pane(app, Length::Fill)).height(Length::Fill),
        bottom_nav(app),
    ]
    .height(Length::Fill);

    if show_detail {
        Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(shell_base)
            .push(
                container(content_pane(app, true))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::panel_style()),
            )
            .into()
    } else {
        shell_base.into()
    }
}

fn nav_rail(app: &App, d: Density) -> Element<'_, Message> {
    let mut col = Column::new()
        .spacing(8)
        .padding(Padding::from([12, 8]))
        .align_x(Alignment::Center)
        .width(Length::Fill);

    for section in NavSection::all() {
        col = col.push(nav_destination(app, section, true, d));
    }

    container(col)
        .width(d.rail_width)
        .height(Length::Fill)
        .style(theme::sidebar_style())
        .into()
}

fn bottom_nav(app: &App) -> Element<'_, Message> {
    let d = app.density();
    // Equal vertical inset so the bar height hugs the destination buttons.
    let pad_v = 6.0;
    let pad_h = 8.0;
    let mut r = Row::new()
        .spacing(6)
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
            container(nav_destination(app, section, false, d))
                .width(Length::Fill)
                .height(Length::Fill),
        );
    }

    container(r)
        .width(Length::Fill)
        .height(d.nav_bar_height)
        .style(theme::sidebar_style())
        .into()
}

fn nav_destination(
    app: &App,
    section: NavSection,
    vertical: bool,
    d: Density,
) -> Element<'_, Message> {
    let active = app.nav() == section;
    let icon = icons::nav_icon(section.icon(), active, d.icon);
    let hover_ok = app.hover_ok();

    let (width, height) = if vertical {
        // Fill the rail width with a tall square-ish hit target.
        (Length::Fill, Length::Fixed(d.nav_hit))
    } else {
        // Fill each bottom-nav cell.
        (Length::Fill, Length::Fill)
    };

    let content = container(icon)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let btn = button(content)
        .on_press(Message::Navigate(section))
        .padding(0)
        .width(width)
        .height(height);

    let styled = if active {
        btn.style(theme::nav_active(hover_ok))
    } else {
        btn.style(theme::nav_idle(hover_ok))
    };

    container(styled)
        .width(Length::Fill)
        .height(if vertical {
            Length::Shrink
        } else {
            Length::Fill
        })
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn list_pane(app: &App, width: Length) -> Element<'_, Message> {
    let d = app.density();
    let show_compose = matches!(app.nav(), NavSection::Chats | NavSection::Channels)
        && app.connected();
    let header: Element<'_, Message> = if show_compose {
        row![
            text(app.nav().label())
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            button(icons::centered_hit(
                icons::plus_icon(20.0, theme::t().accent),
                36.0,
            ))
            .on_press(Message::OpenCompose)
            .padding(0)
            .style(theme::secondary_action(app.hover_ok())),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(app.nav().label())
            .size(d.title)
            .color(theme::t().text_primary)
            .into()
    };

    let body: Element<'_, Message> = match app.nav() {
        NavSection::Chats => chat_list(app),
        NavSection::Channels => channel_list(app),
        NavSection::Calls => calls_panel(app),
        NavSection::Settings => settings_panel(app),
    };

    container(
        column![
            container(header).padding(Padding {
                top: d.pad,
                right: d.pad,
                bottom: d.pad_tight,
                left: d.pad,
            }),
            body
        ]
        .height(Length::Fill),
    )
    .width(width)
    .height(Length::Fill)
    .style(theme::list_style())
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

    let mut col = Column::new().spacing(8).padding(Padding {
        top: 0.0,
        right: d.pad_tight,
        bottom: d.pad_tight,
        left: d.pad_tight,
    });

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
        return scroll_y(col).height(Length::Fill).into();
    }

    for (index, chat) in rows {
        let selected = app.selected() == Some(index);
        let title = txt(&chat.title)
            .size(d.body)
            .color(theme::t().text_primary);
        let preview = txt(truncate(&chat.preview, 72))
            .size(d.meta)
            .color(theme::t().text_secondary);

        let mut badges = Row::new().spacing(6).align_y(Alignment::Center);
        if chat.settings.favorite {
            badges = badges.push(icons::star_icon(14.0, theme::t().accent));
        }
        if chat.settings.enable_ntfs.is_muted() {
            badges = badges.push(text("muted").size(d.meta).color(theme::t().text_muted));
        }
        let show_unread = chat.unread > 0 || chat.unread_chat;
        if show_unread {
            let label = if chat.unread > 0 {
                format!("{}", chat.unread)
            } else {
                "·".into()
            };
            badges = badges.push(text(label).size(d.meta).color(theme::t().accent));
        }
        let support_badge = app.support_badge_count(chat);
        if support_badge > 0 {
            badges = badges.push(
                text(format!("⚑{support_badge}"))
                    .size(d.meta)
                    .color(theme::t().always),
            );
        }

        let content = container(
            row![
                profile_avatar(chat.peer_profile.image.as_deref(), &chat.title, 36.0),
                column![title, preview].spacing(4).width(Length::Fill),
                badges,
            ]
            .align_y(Alignment::Center)
            .spacing(10),
        )
        .width(Length::Fill)
        .padding(Padding::from([d.list_row + 2.0, d.pad_tight + 4.0]));

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

        col = col.push(row);
    }
    scroll_y(col).height(Length::Fill).into()
}

fn content_pane(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    // Compose (new chat / group / channel) replaces the conversation pane.
    if app.compose_open() {
        return container(compose_page(app, with_back))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(if with_back { 0.0 } else { d.pad })
            .style(theme::panel_style())
            .into();
    }
    // Group / channel settings replace the conversation pane (same as contact profile).
    if app.group_settings().is_some() {
        return container(group_settings_page(app, with_back))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(if with_back { 0.0 } else { d.pad })
            .style(theme::panel_style())
            .into();
    }
    // Moderator support-chat list (member threads).
    if app
        .support()
        .is_some_and(|s| matches!(s.pane, SupportPane::List))
    {
        return container(support_list_page(app, with_back))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(if with_back { 0.0 } else { d.pad })
            .style(theme::panel_style())
            .into();
    }
    // Contact profile replaces the conversation pane (desktop) / is a full page (mobile).
    if app.contact_profile().is_some() {
        return container(contact_profile_page(app, with_back))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(if with_back { 0.0 } else { d.pad })
            .style(theme::panel_style())
            .into();
    }
    let content: Element<'_, Message> = match app.nav() {
        NavSection::Chats | NavSection::Channels => conversation_pane(app, with_back),
        NavSection::Calls => calls_detail(app),
        _ => placeholder(
            app.nav().label(),
            "Select an item from the list when this section is implemented.",
            d,
        ),
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(if with_back { 0.0 } else { d.pad })
        .style(theme::panel_style())
        .into()
}

fn conversation_pane(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let Some(index) = app.selected() else {
        return placeholder(
            "Select a chat",
            "Pick a conversation from the list.",
            d,
        );
    };
    let Some(chat) = app.chats().get(index) else {
        return placeholder("Select a chat", "Pick a conversation from the list.", d);
    };

    let support_thread = app.support().and_then(|s| match &s.pane {
        SupportPane::Thread { title, .. } => Some(title.as_str()),
        SupportPane::List => None,
    });
    let in_support = support_thread.is_some();

    let avatar = profile_avatar(
        chat.peer_profile.image.as_deref(),
        &chat.title,
        if with_back { 32.0 } else { 36.0 },
    );
    let title_text = support_thread.unwrap_or(chat.title.as_str());
    let subtitle = if in_support {
        "Support chat"
    } else {
        chat_type_label(chat)
    };
    let identity = column![
        text(title_text)
            .size(d.title)
            .color(theme::t().text_primary),
        text(subtitle)
            .size(d.meta)
            .color(theme::t().text_muted),
    ]
    .spacing(2);
    let identity: Element<'_, Message> = if in_support {
        row![avatar, identity.width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    } else if app.selected_is_direct() {
        mouse_area(
            row![avatar, identity.width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .on_release(Message::OpenContactProfile)
        .into()
    } else if app.selected_is_group_or_channel() {
        mouse_area(
            row![avatar, identity.width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .on_release(Message::OpenGroupSettings)
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
        let tint = if badge > 0 {
            theme::t().always
        } else {
            theme::t().text_secondary
        };
        let open = if can_moderate(chat.membership_role.as_deref().unwrap_or("member")) {
            Message::OpenSupportChatList
        } else {
            Message::OpenUserSupportChat
        };
        button(icons::centered_hit(icons::admin_icon(18.0, tint), 36.0))
            .on_press(open)
            .padding(0)
            .style(theme::round_icon(app.hover_ok()))
            .into()
    } else {
        horizontal().width(0).into()
    };

    let title_row: Element<'_, Message> = if with_back || in_support {
        row![
            button(icons::back_icon(18.0))
                .on_press(Message::Back)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(app.hover_ok())),
            identity,
            support_action,
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: d.pad_tight,
        })
        .into()
    } else {
        row![
            container(identity).width(Length::Fill),
            support_action,
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
    } else {
        composer_bar(app, d)
    };

    column![title_row, history, footer]
        .height(Length::Fill)
        .into()
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

fn composer_bar(app: &App, d: Density) -> Element<'_, Message> {
    let send_size = if app.layout() == LayoutMode::Mobile {
        44.0
    } else {
        40.0
    };
    let icon_size = if app.layout() == LayoutMode::Mobile {
        20.0
    } else {
        18.0
    };
    let hover_ok = app.hover_ok();

    let attach = button(icons::plus_tool(send_size, hover_ok))
        .on_press(Message::AttachFile)
        .width(send_size)
        .height(send_size)
        .padding(0)
        .style(theme::composer_tool());

    if let Some(draft) = app.voice_draft() {
        return voice_draft_bar(draft, send_size, icon_size, d, hover_ok);
    }

    let recording = app.voice_recording();
    let mic = if recording {
        button(icons::centered_hit(
            icons::stop_icon(icon_size, Color::from_rgb(0.95, 0.35, 0.35)),
            send_size,
        ))
        .on_press(Message::ToggleVoiceRecord)
        .width(send_size)
        .height(send_size)
        .padding(0)
        .style(theme::composer_tool())
    } else {
        button(icons::mic_tool(send_size, hover_ok))
            .on_press(Message::ToggleVoiceRecord)
            .width(send_size)
            .height(send_size)
            .padding(0)
            .style(theme::composer_tool())
    };

    let can_send = !app.composer().trim().is_empty();
    let send = button(icons::send_tool(send_size, can_send, hover_ok))
        .on_press(Message::Send)
        .width(send_size)
        .height(send_size)
        .padding(0)
        .style(theme::composer_send(can_send, hover_ok));

    let placeholder = if recording {
        "Recording… tap stop when done"
    } else {
        "Message"
    };

    let suggestions = app.emoji_suggestions();
    let suggest_idx = app.emoji_suggest_index();

    // Auto-growing field: Shrink uses the editor's real wrapped layout
    // (no char-count heuristics that wrap too early/late). WordOrGlyph
    // breaks long tokens like Telegram/WhatsApp.
    let line_px = d.body * 1.3;
    let h_pad = 10.0;
    let v_pad = ((send_size - line_px) / 2.0).max(6.0);
    let editor_padding = Padding {
        top: v_pad,
        right: h_pad,
        bottom: v_pad,
        left: h_pad,
    };
    let min_content = (send_size - editor_padding.y()).max(line_px);
    let max_content = line_px * 6.0;

    use crate::touch_mouse::touch_as_mouse;

    let editor = touch_as_mouse(
        text_editor(app.composer_content())
            .id(iced::widget::Id::new("composer-input"))
            .placeholder(placeholder)
            .on_action(Message::ComposerEdit)
            .padding(editor_padding)
            .size(d.body)
            .line_height(1.3)
            .height(Length::Shrink)
            .min_height(min_content)
            .max_height(max_content)
            .wrapping(Wrapping::WordOrGlyph)
            .style(theme::composer_editor())
            .key_binding({
                let has_suggestions = !suggestions.is_empty();
                move |key_press| {
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

                    // Enter sends (Shift+Enter = newline via default Binding::Enter).
                    if matches!(
                        key_press.key.as_ref(),
                        keyboard::Key::Named(key::Named::Enter)
                    ) && !key_press.modifiers.shift()
                    {
                        return Some(EditorBinding::Custom(Message::Send));
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
                            let cleaned: String =
                                raw.chars().filter(|c| !c.is_control()).collect();
                            if cleaned.chars().count() > 1 {
                                return Some(EditorBinding::Custom(Message::ComposerInsert(
                                    cleaned,
                                )));
                            }
                        }
                    }

                    EditorBinding::from_key_press(key_press)
                }
            }),
    );

    let input_row = row![attach, mic, editor, send]
        .spacing(0)
        .align_y(Alignment::End);

    // Match top/bottom so the stack isn't visually top-heavy.
    let bar_pad = if app.layout() == LayoutMode::Mobile {
        6.0
    } else {
        8.0
    };

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
                Color::from_rgb(0.043, 0.063, 0.125)
            } else {
                theme::t().text_primary
            };
            let label = row![
                emoji_glyph(hit.glyph)
                    .size(d.body)
                    .color(Color::WHITE),
                txt(format!(":{}:", hit.shortcode))
                    .size(d.meta)
                    .color(fg),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            let row = button(label)
                .on_press(Message::EmojiSuggestSelect(i))
                .padding(Padding::from([8, 10]))
                .width(Length::Fill)
                .height(row_h)
                .style(theme::emoji_suggest_row(selected, app.hover_ok()));
            list = list.push(row);
        }
        let suggest_panel = container(list)
            .padding(6)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().bg_elevated)),
                border: iced::Border {
                    color: theme::t().border,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..container::Style::default()
            });
        column![suggest_panel, vertical().height(6)]
            .width(Length::Fill)
            .into()
    };

    let reply_slot: Element<'_, Message> = if let Some(reply) = app.reply_to() {
        let preview: Element<'_, Message> = if cursus_core::is_short_emoji(&reply.preview) {
            emoji_glyph(reply.preview.clone())
                .size(d.body)
                .color(Color::WHITE)
                .into()
        } else {
            txt(reply.preview.clone())
                .size(d.meta)
                .color(theme::t().text_secondary)
                .into()
        };
        let reply_bar = container(
            row![
                column![
                    text("Replying")
                        .size(d.meta)
                        .color(theme::t().accent),
                    preview,
                ]
                .spacing(2)
                .width(Length::Fill),
                button(txt("✕").size(d.meta).color(theme::t().text_muted))
                    .on_press(Message::CancelReply)
                    .padding(Padding::from([4, 8]))
                    .style(theme::secondary_action(app.hover_ok())),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([8, 10]))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().accent_soft)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        });
        column![reply_bar, vertical().height(6)]
            .width(Length::Fill)
            .into()
    } else {
        Column::new().width(Length::Fill).into()
    };

    let body = column![suggest_slot, reply_slot, input_row]
        .spacing(0)
        .width(Length::Fill);

    container(body)
        .padding(Padding {
            top: bar_pad,
            right: d.pad,
            bottom: bar_pad,
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

fn contact_request_bar(app: &App, req_id: i64, d: Density) -> Element<'_, Message> {
    container(
        column![
            text("Incoming contact request")
                .size(d.body)
                .color(theme::t().text_primary),
            text("Someone connected via your address. Accept to chat, or reject.")
                .size(d.meta)
                .color(theme::t().text_secondary),
            row![
                button(text("Accept").size(d.body))
                    .on_press(Message::AcceptContactRequest(req_id))
                    .padding(Padding::from([12, 16]))
                    .style(theme::primary_action(app.hover_ok())),
                button(text("Reject").size(d.body))
                    .on_press(Message::RejectContactRequest(req_id))
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

    let show_members = app.selected_shows_member_headers();
    let show_channel = app.selected_is_channel();
    let jump_visible = app.show_jump_to_latest();
    let mut col = Column::new().spacing(8).padding(d.pad).width(Length::Fill);
    let mut prev_sender: Option<i64> = None;
    let mut prev_channel_block = false;
    for msg in messages {
        if msg.is_announcement {
            prev_sender = None;
            prev_channel_block = false;
            col = col.push(announcement_row(msg, d));
            continue;
        }
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
        col = col.push(message_bubble(app, msg, d, show_header));
    }
    // Do NOT append spacer when the jump FAB appears — that grows content at
    // Anchor::End and leaves an empty gap under the last bubble while scrolling.
    // The FAB is a Stack overlay; slight overlap with the last row is fine.
    //
    // Anchor::End must live on the Scrollbar inside direction() — calling
    // anchor_bottom() before direction() is overwritten by Scrollbar::default().
    // No on_scroll: every wheel tick was publishing a Message and rebuilding
    // the whole chat view (major scroll lag).
    let history = scrollable(col)
        .id(iced::widget::Id::new("chat-history"))
        .height(Length::Fill)
        .direction(Direction::Vertical(no_bar().anchor(Anchor::End)));

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
            .style(theme::secondary_action(app.hover_ok())),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(Padding {
            top: 0.0,
            right: d.pad,
            bottom: d.pad,
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

/// Telegram / SimpleX-style centered protocol line (left group, profile update, …).
fn announcement_row<'a>(msg: &'a ChatMessage, d: Density) -> Element<'a, Message> {
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

fn message_bubble<'a>(
    app: &'a App,
    msg: &'a ChatMessage,
    d: Density,
    show_member_header: bool,
) -> Element<'a, Message> {
    let align = if msg.outgoing {
        Alignment::End
    } else {
        Alignment::Start
    };
    let revealed = app.is_deleted_revealed(msg.item_id);
    let can_reveal = msg.deleted == Some(cursus_core::DeletedState::Marked);
    let show_as_deleted = msg.is_deleted() && !(can_reveal && revealed);
    let emoji_only = !show_as_deleted && msg.is_emoji_only();
    let bg = if emoji_only {
        Color::TRANSPARENT
    } else if msg.outgoing {
        theme::t().accent_soft
    } else {
        theme::t().bg_elevated
    };

    let mut body = Column::new().spacing(4);
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
            if let Some(preview) = visual_media_preview(app, msg, file, state) {
                media_flush = true;
                body = body.push(preview);
            } else {
                body = body.push(file_title_row(app, msg, file, state, d));
            }
            body = body.push(attachment_status_footer(msg, file, state, d, media_flush));
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
        body = body.push(bubble_content_with_time(
            emoji_message_text(&msg.text),
            msg,
            d,
            BUBBLE_TEXT_MAX,
        ));
        voice_owns_meta = true;
    } else {
        body = body.push(bubble_content_with_time(
            message_text_content(app, msg, d),
            msg,
            d,
            BUBBLE_TEXT_MAX,
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
        // Image sits flush; keep a little room under for size + time.
        Padding {
            top: 4.0,
            right: 4.0,
            bottom: 8.0,
            left: 4.0,
        }
    } else {
        Padding::from([8, 12])
    };
    let focused = app.focus_item() == Some(msg.item_id);
    let selected = app.selection_mode() && app.selected_items().contains(&msg.item_id);
    let border_w = if focused || selected {
        2.0
    } else if emoji_only {
        0.0
    } else {
        1.0
    };
    let border_color = if focused || selected {
        theme::t().accent
    } else {
        theme::t().border
    };
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
                color: border_color,
                width: border_w,
                radius: 16.0.into(),
            },
            ..container::Style::default()
        });

    const MEMBER_AVATAR: f32 = 32.0;
    let group_incoming =
        !msg.outgoing && msg.sender.is_some() && app.selected_shows_member_headers();
    let channel_incoming = !msg.outgoing && app.selected_is_channel();
    let bubble_with_sender: Element<'_, Message> = if group_incoming {
        let sender = msg.sender.as_ref().unwrap();
        let avatar: Element<'_, Message> = if show_member_header {
            profile_avatar(sender.image.as_deref(), &sender.display_name, MEMBER_AVATAR)
        } else {
            horizontal().width(MEMBER_AVATAR).height(MEMBER_AVATAR).into()
        };
        let mut bubble_col = Column::new().spacing(2);
        if show_member_header {
            let mut name_row = Row::new().spacing(6).align_y(Alignment::Center);
            name_row = name_row.push(
                text(sender.display_name.clone())
                    .size(d.meta)
                    .color(theme::t().accent),
            );
            if group_role_rank(&sender.role) > group_role_rank("member") {
                name_row = name_row.push(
                    text(sender.role.clone())
                        .size(d.meta)
                        .color(theme::t().text_muted),
                );
            }
            bubble_col = bubble_col.push(name_row);
        }
        bubble_col = bubble_col.push(bubble);
        row![avatar, bubble_col]
            .spacing(8)
            .align_y(Alignment::End)
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
        let mut bubble_col = Column::new().spacing(2);
        if show_member_header {
            bubble_col = bubble_col.push(
                row![
                    text(name.to_string())
                        .size(d.meta)
                        .color(theme::t().accent),
                    text("channel")
                        .size(d.meta)
                        .color(theme::t().text_muted),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        bubble_col = bubble_col.push(bubble);
        row![avatar, bubble_col]
            .spacing(8)
            .align_y(Alignment::End)
            .into()
    } else {
        bubble.into()
    };

    let stack = Column::new()
        .spacing(4)
        .width(Length::Fill)
        .align_x(align)
        .push(bubble_with_sender);

    // No on_press — keeps history scrollable under touch. Long-press is armed
    // from PointerPressed via hover slots (no Message on enter/exit).
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
        col = col.push(reaction_chips(app, msg, d));
    }

    col.into()
}

fn deleted_mark_chip<'a>(msg: &'a ChatMessage, d: Density) -> Element<'a, Message> {
    let label = match msg.deleted {
        Some(cursus_core::DeletedState::Content) => "deleted",
        _ => "marked deleted",
    };
    row![
        icons::trash_icon(14.0, theme::t().text_muted),
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

fn quote_preview<'a>(quote: &'a cursus_core::MessageQuote, d: Density) -> Element<'a, Message> {
    let preview = truncate_ui(&quote.text, 120);
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
            .into()
    } else {
        bar.into()
    }
}

fn truncate_ui(s: &str, max_chars: usize) -> String {
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

fn reaction_chips<'a>(app: &'a App, msg: &'a ChatMessage, d: Density) -> Element<'a, Message> {
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
                    .color(if r.user_reacted {
                        theme::t().accent_on
                    } else {
                        theme::t().text_secondary
                    }),
            );
        }
        let selected = r.user_reacted;
        let chip_face = container(chip_body).padding(Padding::from([4, 8])).style(
            move |_| container::Style {
                background: Some(iced::Background::Color(if selected {
                    theme::t().accent
                } else {
                    theme::t().bg_elevated
                })),
                border: iced::Border {
                    color: theme::t().border,
                    width: 1.0,
                    radius: 14.0.into(),
                },
                ..container::Style::default()
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
                .into()
        };
        row = row.push(chip);
    }
    container(row).into()
}

fn reaction_picker_row(item_id: i64, d: Density, hover: bool) -> Element<'static, Message> {
    let mut row = Row::new().spacing(4).align_y(Alignment::Center);
    for emoji in KNOWN_REACTIONS {
        let glyph = if *emoji == "❤" { "❤️" } else { *emoji };
        row = row.push(
            button(emoji_glyph(glyph.to_string()).size(d.body).color(Color::WHITE))
                .on_press(Message::ToggleReaction {
                    item_id,
                    emoji: (*emoji).to_string(),
                })
                .padding(Padding::from([6, 8]))
                .style(theme::reaction_chip(false, hover)),
        );
    }
    row.into()
}

fn emoji_message_text(raw: &str) -> Element<'static, Message> {
    let s = raw.trim().to_string();
    let n = s.graphemes(true).count();
    let size: f32 = if n <= LARGE_EMOJI_MAX { 48.0 } else { 36.0 };
    emoji_glyph(s)
        .size(size)
        .color(Color::WHITE)
        .into()
}

fn message_text_content(app: &App, msg: &ChatMessage, d: Density) -> Element<'static, Message> {
    if msg.segments.is_empty() {
        return txt(msg.text.clone())
            .size(d.body)
            .color(theme::t().text_primary)
            .wrapping(Wrapping::WordOrGlyph)
            .into();
    }
    let item_id = msg.item_id;
    let spans: Vec<_> = msg
        .segments
        .iter()
        .enumerate()
        .map(|(index, seg)| {
            let revealed = app.is_secret_revealed(item_id, index);
            styled_span(seg, d.body, item_id, index, revealed)
        })
        .collect();
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
        TextStyle::Secret if !secret_revealed => {
            "•".repeat(seg.text.chars().count().max(1))
        }
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
        s = s.link(Message::OpenUrl(link.clone()));
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
) -> Element<'static, Message> {
    let wave = Canvas::new(Waveform {
        peaks: draft.peaks.clone(),
        progress: 1.0,
        played: theme::t().accent,
        rest: theme::t().text_muted,
    })
    .width(Length::Fill)
    .height(36.0);

    container(
        row![
            button(icons::centered_hit(
                icons::cancel_icon(icon_size, theme::t().text_secondary),
                send_size,
            ))
            .on_press(Message::CancelVoiceDraft)
            .width(send_size)
            .height(send_size)
            .padding(0)
            .style(theme::secondary_action(hover)),
            column![
                text(format!("Voice · {}", format_duration(draft.duration_secs)))
                    .size(d.meta)
                    .color(theme::t().text_secondary),
                wave,
            ]
            .spacing(4)
            .width(Length::Fill),
            button(icons::centered_hit(
                icons::send_icon(icon_size, Color::from_rgb(0.043, 0.063, 0.125)),
                send_size,
            ))
            .on_press(Message::SendVoiceDraft)
            .width(send_size)
            .height(send_size)
            .padding(0)
            .style(theme::icon_action(hover)),
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

/// Max width for bubble body (including reserved trailing time slot).
const BUBBLE_TEXT_MAX: f32 = 420.0;

/// Compact height for time + ticks (fits inside the last text line box).
fn bubble_time_height(d: Density) -> f32 {
    f32::from(d.meta) + 2.0
}

/// Trailing gutter so glyphs never run under the floating time / receipts.
fn bubble_time_slot(d: Density, outgoing: bool) -> f32 {
    let text_w = f32::from(d.meta) * 5.2;
    let ticks = if outgoing {
        (f32::from(d.meta) * 0.85).clamp(10.0, 13.0) + 4.0
    } else {
        0.0
    };
    (text_w + ticks + 6.0).clamp(56.0, 120.0)
}

/// Body + Telegram-style time: text keeps a right gutter; time sits in that
/// gutter on the last line (no cover-up, no extra footer row).
fn bubble_content_with_time<'a>(
    content: Element<'a, Message>,
    msg: &'a ChatMessage,
    d: Density,
    content_max: f32,
) -> Element<'a, Message> {
    let slot = bubble_time_slot(d, msg.outgoing);
    container(
        Stack::new()
            .push(
                container(content)
                    .padding(Padding {
                        top: 0.0,
                        right: slot,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .max_width(content_max),
            )
            .push(
                container(bubble_time_meta(msg, d))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End),
            ),
    )
    // Keep time/receipts inside the bubble bounds (no floating green checks).
    .clip(true)
    .into()
}

/// Telegram-style trailing time (+ receipt for outgoing).
fn bubble_time_meta(msg: &ChatMessage, d: Density) -> Element<'static, Message> {
    let time_h = bubble_time_height(d);
    let icon_size = (f32::from(d.meta) * 0.85).clamp(10.0, 13.0);
    let mut row = Row::new()
        .spacing(3)
        .height(Length::Fixed(time_h))
        .align_y(Alignment::Center);
    if let Some(label) = format_bubble_time(msg.created_at.as_deref()) {
        row = row.push(
            txt(label)
                .size(d.meta)
                .line_height(iced::widget::text::LineHeight::Absolute(time_h.into()))
                .color(theme::t().text_muted)
                .wrapping(Wrapping::None),
        );
    }
    if msg.outgoing {
        row = row.push(receipt_icon(msg.delivery, icon_size));
    }
    row.into()
}

/// Format SimpleX `itemTs` (`2026-11-03T03:23:21Z`) as a local wall-clock like Telegram.
fn format_bubble_time(iso: Option<&str>) -> Option<String> {
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
                .or_else(|| chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.fZ").ok())
                .map(|ndt| {
                    ndt.and_utc()
                        .with_timezone(&chrono::Local)
                })
        })?;
    // Prefer 12h like Telegram; strip a leading zero manually for portability.
    let stamped = parsed.format("%I:%M %p").to_string();
    Some(
        stamped
            .strip_prefix('0')
            .unwrap_or(stamped.as_str())
            .to_string(),
    )
}

fn receipt_icon(status: DeliveryStatus, size: f32) -> Element<'static, Message> {
    match status {
        DeliveryStatus::Pending => text("...")
            .size((size - 2.0).max(10.0))
            .color(theme::t().text_muted)
            .into(),
        DeliveryStatus::Sent => icons::check_sent(size, theme::t().text_secondary),
        DeliveryStatus::Delivered => icons::check_read(size, theme::t().accent),
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
) -> Element<'a, Message> {
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

fn file_title_row<'a>(
    app: &'a App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Element<'a, Message> {
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
    let name: Element<'a, Message> =
        if matches!(state, FileTransferState::Complete) {
            if let Some(path) = open_path.filter(|p| p.is_file()) {
                mouse_area(name)
                    .on_release(Message::OpenLocalFile(path))
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

fn voice_waveform_row<'a>(
    app: &'a App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
    d: Density,
) -> Element<'a, Message> {
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
        .or_else(|| {
            file.local_path
                .as_ref()
                .and_then(|p| app.waveform_for(p))
        })
        .map(|p| p.to_vec())
        .unwrap_or_else(|| vec![0.35; media::WAVEFORM_BARS]);

    let wave = Canvas::new(Waveform {
        peaks,
        progress,
        played: theme::t().accent,
        rest: theme::t().text_muted,
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
                icons::stop_icon(20.0, Color::from_rgb(0.043, 0.063, 0.125))
            } else {
                icons::play_icon(20.0, Color::from_rgb(0.043, 0.063, 0.125))
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
                            border: iced::Border {
                                color: c.accent,
                                width: 0.0,
                                radius: 10.0.into(),
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
            icons::download_icon(16.0, Color::from_rgb(0.043, 0.063, 0.125)),
            hit,
        ))
        .width(hit)
        .height(hit)
        .style(|_| {
            let c = theme::t();
            container::Style {
                background: Some(iced::Background::Color(c.accent)),
                border: iced::Border {
                    radius: 8.0.into(),
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
    .into()
}

fn format_duration(secs: u32) -> String {
    let mins = secs / 60;
    let rem = secs % 60;
    format!("{mins}:{rem:02}")
}

fn visual_media_preview<'a>(
    app: &'a App,
    msg: &ChatMessage,
    file: &cursus_core::FileInfo,
    state: FileTransferState,
) -> Option<Element<'a, Message>> {
    if !matches!(state, FileTransferState::Complete) {
        return None;
    }
    // Prefer decrypted plaintext; skip preview until CryptoFile decrypt finishes.
    let path = app.plaintext_path_for(file)?;
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() == 0 {
        return None;
    }
    let is_video = matches!(file.kind, FileKind::Video) || media::is_video_name(&file.file_name);
    let source = if is_video {
        media::video_thumbnail(&path).unwrap_or_else(|_| path.clone())
    } else {
        path.clone()
    };
    // Cap for mobile chat panes — keep native aspect (no fixed 360×220 stretch).
    const PREVIEW_MAX_W: u32 = 300;
    const PREVIEW_MAX_H: u32 = 400;
    const RADIUS: f32 = 14.0;
    let (display_path, pw, ph) =
        match media::rounded_fitted_preview(&source, PREVIEW_MAX_W, PREVIEW_MAX_H, RADIUS) {
            Ok(v) => v,
            Err(_) => {
                let (pw, ph) = (PREVIEW_MAX_W, PREVIEW_MAX_H * 2 / 3);
                (source, pw, ph)
            }
        };
    let w = pw as f32;
    let h = ph as f32;
    let thumb = image(image::Handle::from_path(display_path))
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .content_fit(ContentFit::Contain);
    let mut stack = Stack::new()
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .push(thumb);
    if is_video {
        stack = stack.push(
            container(icons::centered_hit(
                icons::play_icon(26.0, Color::WHITE),
                48.0,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
    }
    let framed = container(stack)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .clip(true);
    Some(
        mouse_area(framed)
            .on_release(Message::OpenMedia {
                item_id: msg.item_id,
            })
            .into(),
    )
}

#[derive(Debug, Clone)]
struct Waveform {
    peaks: Vec<f32>,
    progress: f32,
    played: Color,
    rest: Color,
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
        let n = self.peaks.len().max(1) as f32;
        let gap = 0.75;
        let bar_w = ((bounds.width - gap * (n - 1.0)) / n).max(1.0);
        for (i, peak) in self.peaks.iter().enumerate() {
            let x = i as f32 * (bar_w + gap);
            let h = (peak.clamp(0.08, 1.0) * bounds.height * 0.92).max(1.5);
            let y = (bounds.height - h) * 0.5;
            let filled = (i as f32 + 1.0) / n <= self.progress + 0.001;
            frame.fill_rectangle(
                Point::new(x, y),
                Size::new(bar_w, h),
                Fill::from(if filled { self.played } else { self.rest }),
            );
        }
        vec![frame.into_geometry()]
    }
}

fn media_viewer_overlay(app: &App) -> Element<'_, Message> {
    let Some(viewer) = app.media_viewer() else {
        return horizontal().width(0).into();
    };
    let Some(item) = viewer.items.get(viewer.index) else {
        return horizontal().width(0).into();
    };
    // `thumb` is prepared in `ensure_media_thumb` (poster / downscale) — never decode here.
    let show_path = item
        .thumb
        .clone()
        .unwrap_or_else(|| item.path.clone());
    let media = image(image::Handle::from_path(show_path))
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Contain);

    let mut stage: Element<'_, Message> = container(media)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();

    if item.is_video {
        stage = Stack::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(stage)
            .push(
                container(
                    button(icons::centered_hit(
                        icons::play_icon(36.0, Color::from_rgb(0.043, 0.063, 0.125)),
                        72.0,
                    ))
                    .on_press(Message::MediaPlayVideo)
                    .padding(0)
                    .style(theme::primary_action(true)),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .into();
    }

    let counter = text(format!(
        "{} / {}  ·  {}",
        viewer.index + 1,
        viewer.items.len(),
        item.file_name
    ))
    .size(14)
    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.85));

    // Drag to swipe; a short tap opens the file outside (second press after the bubble).
    let stage = mouse_area(stage)
        .on_press(Message::MediaDragStart(0.0))
        .on_move(|p| Message::MediaDragMove(p.x))
        .on_release(Message::MediaDragEnd);

    let hit = 40.0;
    let panel = column![
        row![
            button(icons::centered_hit(
                icons::cancel_icon(20.0, Color::WHITE),
                hit,
            ))
            .on_press(Message::CloseMediaViewer)
            .padding(0)
            .style(theme::secondary_action(true)),
            horizontal().width(Length::Fill),
            counter,
            horizontal().width(8),
            button(icons::centered_hit(
                icons::download_icon(20.0, Color::WHITE),
                hit,
            ))
            .on_press(Message::MediaSaveAs)
            .padding(0)
            .style(theme::secondary_action(true)),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([12, 16])),
        container(stage)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true),
        row![
            button(icons::centered_hit(
                icons::previous_icon(22.0, Color::WHITE),
                hit,
            ))
            .on_press(Message::MediaPrev)
            .padding(0)
            .style(theme::secondary_action(true)),
            horizontal().width(Length::Fill),
            button(icons::centered_hit(
                icons::next_icon(22.0, Color::WHITE),
                hit,
            ))
            .on_press(Message::MediaNext)
            .padding(0)
            .style(theme::secondary_action(true)),
        ]
        .padding(Padding::from([12, 16])),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    iced::widget::opaque(
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.02, 0.02, 0.04, 0.94,
                ))),
                ..container::Style::default()
            }),
    )
}

fn compose_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let narrow = matches!(app.layout(), LayoutMode::Mobile);
    let side_by_side =
        matches!(app.layout(), LayoutMode::Desktop) && app.window_width() >= 1100.0;
    let hover = app.hover_ok();

    let header: Element<'_, Message> = if with_back {
        row![
            button(icons::back_icon(18.0))
                .on_press(Message::CloseCompose)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
            text("New")
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: d.pad_tight,
        })
        .into()
    } else {
        row![
            text("New")
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            button(text("Close").size(d.meta))
                .on_press(Message::CloseCompose)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding::from([d.pad_tight, 0.0]))
        .into()
    };

    let mut body = Column::new().spacing(12).width(Length::Fill);
    body = body.push(
        text("Connection")
            .size(d.body)
            .color(theme::t().text_primary),
    );
    body = body.push(
        text("Share a link, or paste one you received.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );

    let share_one = share_tile(
        "One-time invite",
        "Single-use link",
        Message::CreateInvitation,
        hover,
        d,
    );
    let share_addr = share_tile(
        "My address",
        "Reusable contact link",
        Message::EnsureAddress,
        hover,
        d,
    );
    let share_row: Element<'_, Message> = if side_by_side {
        row![share_one, share_addr].spacing(8).into()
    } else {
        column![share_one, share_addr].spacing(8).into()
    };
    body = body.push(share_row);

    let link_field = text_input("Paste invite or address link…", app.invite_input())
        .on_input(Message::InviteInputChanged)
        .on_submit(Message::AcceptInvite)
        .padding(d.composer)
        .size(d.meta)
        .width(Length::Fill)
        .style(theme::composer_input());
    let accept = centered_action(
        "Connect",
        Message::AcceptInvite,
        theme::primary_action(hover),
        d,
        narrow,
    );
    let paste_row: Element<'_, Message> = if narrow {
        column![
            text("Or paste a link")
                .size(d.meta)
                .color(theme::t().text_muted),
            link_field,
            accept,
        ]
        .spacing(8)
        .into()
    } else {
        column![
            text("Or paste a link")
                .size(d.meta)
                .color(theme::t().text_muted),
            row![link_field, accept]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    };
    body = body.push(paste_row);

    if let Some(link) = app.invite_link() {
        let link_preview = truncate(link, if narrow { 36 } else { 56 });
        body = body.push(
            container(
                column![
                    text("Ready to share")
                        .size(d.meta)
                        .color(theme::t().text_muted),
                    text(link_preview)
                        .size(d.meta)
                        .color(theme::t().text_primary),
                    centered_action(
                        "Copy link",
                        Message::CopyInviteLink,
                        theme::primary_action(hover),
                        d,
                        true,
                    ),
                ]
                .spacing(8),
            )
            .padding(12)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().bg_sidebar)),
                border: iced::Border {
                    color: theme::t().border,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    body = body.push(vertical().height(8));
    body = body.push(
        text("Create")
            .size(d.body)
            .color(theme::t().text_primary),
    );
    body = body.push(
        share_tile(
            "New group",
            "Private group — invite contacts after creating",
            Message::OpenCreateGroup,
            hover,
            d,
        ),
    );
    body = body.push(
        share_tile(
            "New channel",
            "Public channel using enabled chat relays",
            Message::OpenCreateChannel,
            hover,
            d,
        ),
    );

    column![
        header,
        scroll_y(body.padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad,
            left: d.pad,
        }))
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn share_tile<'a>(
    title: &'a str,
    subtitle: &'a str,
    on_press: Message,
    hover_ok: bool,
    d: Density,
) -> Element<'a, Message> {
    let body = column![
        text(title)
            .size(d.body)
            .color(theme::t().text_primary),
        text(subtitle)
            .size(d.meta)
            .color(theme::t().text_muted),
    ]
    .spacing(3)
    .align_x(Alignment::Center);

    button(
        container(body)
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .on_press(on_press)
    .padding(Padding::from([14, 12]))
    .width(Length::Fill)
    .style(theme::secondary_action(hover_ok))
    .into()
}

fn centered_action<'a>(
    label: &'a str,
    on_press: Message,
    style: impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style + 'a,
    d: Density,
    fill: bool,
) -> Element<'a, Message> {
    let label = container(text(label).size(d.meta))
        .width(Length::Fill)
        .center_x(Length::Fill);

    let mut btn = button(label)
        .on_press(on_press)
        .padding(Padding::from([12, 16]))
        .style(style);

    if fill {
        btn = btn.width(Length::Fill);
    }

    btn.into()
}

fn settings_panel(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let draft = app.own_profile();
    let shares = draft
        .fields
        .contact_link
        .as_ref()
        .is_some_and(|s| !s.is_empty());

    scroll_y(
        column![
            text("My profile")
                .size(d.meta)
                .color(theme::t().text_muted),
            row![
                profile_avatar(
                    draft.fields.image.as_deref(),
                    &draft.fields.display_name,
                    64.0,
                ),
                profile_photo_actions(
                    draft.fields.image.is_some(),
                    Message::PickOwnProfileImage,
                    Message::ClearOwnProfileImage,
                    app.hover_ok(),
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text("Display name")
                .size(d.meta)
                .color(theme::t().text_muted),
            text_input("Display name", &draft.fields.display_name)
                .on_input(Message::OwnProfileDisplayName)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            text("Bio")
                .size(d.meta)
                .color(theme::t().text_muted),
            text_input("Short bio", &draft.fields.short_descr)
                .on_input(Message::OwnProfileBio)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            text("Description")
                .size(d.meta)
                .color(theme::t().text_muted),
            text_input("Longer description", &draft.fields.description)
                .on_input(Message::OwnProfileDescription)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
            button(text("Save profile").size(d.meta))
                .on_press_maybe(if draft.dirty {
                    Some(Message::SaveOwnProfile)
                } else {
                    None
                })
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(app.hover_ok())),
            vertical().height(6),
            text("Address")
                .size(d.meta)
                .color(theme::t().text_muted),
            settings_toggle(
                "Share address on profile",
                shares,
                Message::ToggleProfileAddress,
                d,
                app.hover_ok(),
            ),
            settings_toggle(
                "Auto-accept address requests",
                app.desktop_prefs().auto_accept_address,
                Message::ToggleAddressAutoAccept,
                d,
                app.hover_ok(),
            ),
            vertical().height(6),
            text("Appearance")
                .size(d.meta)
                .color(theme::t().text_muted),
            settings_theme_cycle(app, d),
            settings_text_size_cycle(app, d),
            vertical().height(6),
            text("Desktop")
                .size(d.meta)
                .color(theme::t().text_muted),
            settings_toggle(
                "Desktop notifications",
                app.desktop_prefs().notifications,
                Message::ToggleDesktopNotifications,
                d,
                app.hover_ok(),
            ),
            settings_toggle(
                "System tray icon",
                app.desktop_prefs().tray,
                Message::ToggleTray,
                d,
                app.hover_ok(),
            ),
            settings_toggle(
                "Close window hides to tray",
                app.desktop_prefs().close_to_tray,
                Message::ToggleCloseToTray,
                d,
                app.hover_ok(),
            ),
        ]
        .spacing(12)
        .padding(d.pad),
    )
    .height(Length::Fill)
    .into()
}

fn settings_theme_cycle(app: &App, d: Density) -> Element<'_, Message> {
    let label = format!("Theme: {}", app.desktop_prefs().theme.label());
    let light = app.desktop_prefs().theme.resolve_light();
    button(
        row![
            text(label)
                .size(d.body)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            text(if light { "Light" } else { "Dark" })
                .size(d.meta)
                .color(theme::t().text_muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Message::CycleThemePreference)
    .padding(Padding::from([10, 8]))
    .width(Length::Fill)
    .style(theme::menu_item(app.hover_ok()))
    .into()
}

fn settings_text_size_cycle(app: &App, d: Density) -> Element<'_, Message> {
    let pref = app.desktop_prefs().text_size;
    button(
        row![
            text(format!("Text size: {}", pref.label()))
                .size(d.body)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            text(pref.label())
                .size(d.meta)
                .color(theme::t().text_muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Message::CycleTextSize)
    .padding(Padding::from([10, 8]))
    .width(Length::Fill)
    .style(theme::menu_item(app.hover_ok()))
    .into()
}

fn settings_toggle<'a>(
    label: &'a str,
    on: bool,
    msg: Message,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    button(
        row![
            text(label)
                .size(d.body)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            text(if on { "yes" } else { "no" })
                .size(d.meta)
                .color(theme::t().text_muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(msg)
    .padding(Padding::from([10, 8]))
    .width(Length::Fill)
    .style(theme::menu_item(hover))
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
                text_input("New list name…", new_name)
                    .on_input(Message::ChatMenuNewListChanged)
                    .on_submit(Message::ChatMenuCreateList)
                    .padding(d.composer)
                    .size(d.meta)
                    .width(Length::Fill)
                    .style(theme::composer_input()),
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

    let sheet = container(card)
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

fn menu_card<'a>(body: Element<'a, Message>, d: Density) -> Element<'a, Message> {
    container(body)
        .padding(d.pad)
        .width(Length::Fill)
        .max_width(400)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::t().bg_panel)),
            border: iced::Border {
                color: theme::t().border,
                width: 1.0,
                radius: 16.0.into(),
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
                .map(|m| m.deleted == Some(cursus_core::DeletedState::Marked))
                .unwrap_or(false);
            if can_reveal {
                body = body.push(menu_action(
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
                body = body.push(menu_action(
                    "Download",
                    Message::MsgMenuAction(MsgMenuAction::Download),
                    hover,
                    d,
                    false,
                ));
            }
            let outgoing = msg.map(|m| m.outgoing).unwrap_or(false);
            let actions: &[(&str, MsgMenuAction, bool)] = if is_deleted {
                &[
                    ("Info", MsgMenuAction::Info, false),
                    ("Delete for me", MsgMenuAction::Delete, true),
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
                body = body.push(menu_action(
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
        } => {
            let mut info_col = Column::new().spacing(6).width(Length::Fill);
            info_col = info_col.push(
                text("Message info")
                    .size(d.body)
                    .color(theme::t().text_primary),
            );
            info_col = info_col.push(
                txt(format!("Item #{item_id}"))
                    .size(d.meta)
                    .color(theme::t().text_secondary),
            );
            if let Some(ts) = time {
                info_col = info_col.push(
                    txt(format!("Time: {ts}"))
                        .size(d.meta)
                        .color(theme::t().text_secondary),
                );
            }
            if let Some(st) = status {
                info_col = info_col.push(
                    txt(format!("Status: {st}"))
                        .size(d.meta)
                        .color(theme::t().text_secondary),
                );
            }
            if !info_text.is_empty() {
                let preview = truncate_ui(info_text, 160);
                let value: Element<'_, Message> = if cursus_core::is_short_emoji(info_text) {
                    emoji_glyph(preview)
                        .size(d.body)
                        .color(Color::WHITE)
                        .into()
                } else {
                    txt(preview)
                        .size(d.meta)
                        .color(theme::t().text_secondary)
                        .into()
                };
                info_col = info_col.push(
                    row![
                        txt("Text: ")
                            .size(d.meta)
                            .color(theme::t().text_secondary),
                        value,
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
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

    let sheet = container(card)
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

fn profile_avatar<'a>(
    image_data_url: Option<&str>,
    label: &str,
    size: f32,
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
    icons::avatar_initial(label, size)
}

fn round_header_icon_btn(
    icon: Element<'static, Message>,
    on_press: Message,
    hover: bool,
) -> Element<'static, Message> {
    button(icons::centered_hit(icon, 36.0))
        .on_press(on_press)
        .padding(0)
        .style(theme::round_icon(hover))
        .into()
}

/// Title on the left (~75%), round actions at the 75% mark, empty trailing quarter.
fn profile_header_bar<'a>(
    leading: Element<'a, Message>,
    actions: Element<'a, Message>,
    d: Density,
    with_back: bool,
) -> Element<'a, Message> {
    let pad = if with_back {
        Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad_tight,
            left: d.pad_tight,
        }
    } else {
        Padding::from([d.pad_tight, 0.0])
    };
    row![
        container(leading)
            .width(Length::FillPortion(3))
            .align_y(Alignment::Center),
        actions,
        horizontal().width(Length::FillPortion(1)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding(pad)
    .into()
}

fn contact_profile_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(p) = app.contact_profile() else {
        return vertical().height(0).into();
    };

    let display = if p.fields.display_name.is_empty() {
        p.title.clone()
    } else {
        p.fields.display_name.clone()
    };

    let mute_tint = if p.muted {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    let fav_tint = if p.favorite {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    let header_actions = row![
        round_header_icon_btn(
            icons::star_icon(18.0, fav_tint),
            Message::ContactProfileFavorite,
            hover,
        ),
        round_header_icon_btn(
            icons::mute_icon(18.0, mute_tint),
            Message::ContactProfileMute,
            hover,
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let leading: Element<'_, Message> = if with_back {
        row![
            button(icons::back_icon(18.0))
                .on_press(Message::CloseContactProfile)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
            text("Contact")
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("Contact")
            .size(d.title)
            .color(theme::t().text_primary)
            .into()
    };
    let header = profile_header_bar(leading, header_actions.into(), d, with_back);

    let mut body = Column::new().spacing(12).width(Length::Fill);
    body = body.push(
        row![
            profile_avatar(p.fields.image.as_deref(), &display, 72.0),
            {
                let mut names = Column::new().spacing(4).width(Length::Fill);
                names = names.push(
                    text(display)
                        .size(d.title)
                        .color(theme::t().text_primary),
                );
                if !p.fields.full_name.is_empty() {
                    names = names.push(
                        text(p.fields.full_name.clone())
                            .size(d.meta)
                            .color(theme::t().text_secondary),
                    );
                }
                names
            },
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    );
    if !p.fields.short_descr.is_empty() {
        body = body.push(
            text(p.fields.short_descr.clone())
                .size(d.body)
                .color(theme::t().text_secondary),
        );
    }
    if !p.fields.description.is_empty() {
        body = body.push(
            text(p.fields.description.clone())
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    }
    let narrow = matches!(app.layout(), LayoutMode::Mobile) || app.window_width() < 720.0;
    body = body.push(
        text("Local name")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    body = body.push(alias_save_row(
        &p.alias,
        "Name shown only to you",
        Message::ContactAliasChanged,
        Message::SaveContactAlias,
        p.alias_dirty,
        d,
        hover,
    ));
    body = body.push(vertical().height(6));
    body = body.push(
        text("Chat features")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    body = body.push(
        text("Tap to cycle no → yes → always. Peer icon: red / green / gold.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );
    body = body.push(contact_feature_prefs(p, d, hover));
    body = body.push(vertical().height(6));
    body = body.push(
        text("Chat actions")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    body = body.push(contact_danger_actions(d, hover, narrow));

    column![
        header,
        scroll_y(body.padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad,
            left: d.pad,
        }))
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
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

    let sheet = container(card)
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
    let Some(url) = app.pending_url() else {
        return vertical().height(0).into();
    };
    let card = menu_card(
        column![
            text("Open this link?")
                .size(d.body)
                .color(theme::t().text_primary),
            text(url.to_string())
                .size(d.meta)
                .color(theme::t().accent),
            text("Only continue if you trust the sender and the destination.")
                .size(d.meta)
                .color(theme::t().text_secondary),
            row![
                button(text("Cancel").size(d.meta))
                    .on_press(Message::CancelOpenUrl)
                    .padding(Padding::from([10, 14]))
                    .style(theme::secondary_action(hover)),
                button(text("Open").size(d.meta))
                    .on_press(Message::ConfirmOpenUrl)
                    .padding(Padding::from([10, 14]))
                    .style(theme::primary_action(hover)),
            ]
            .spacing(8),
        ]
        .spacing(12)
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

    let sheet = container(card)
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

fn placeholder<'a>(title: &'a str, body: &'a str, d: Density) -> Element<'a, Message> {
    container(
        column![
            text(title).size(d.title).color(theme::t().text_primary),
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

fn channel_list(app: &App) -> Element<'_, Message> {
    // Same row chrome / hover / long-press as the chats list.
    chat_list(app)
}

fn calls_panel(app: &App) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let mut col = Column::new().spacing(8).padding(Padding {
        top: 0.0,
        right: d.pad_tight,
        bottom: d.pad_tight,
        left: d.pad_tight,
    });

    if let Some(call) = app.incoming_call() {
        col = col.push(
            container(
                column![
                    text(format!("Incoming call — {}", call.peer_name))
                        .size(d.body)
                        .color(theme::t().text_primary),
                    text(match call.media {
                        cursus_core::CallMedia::Audio => "Audio",
                        cursus_core::CallMedia::Video => "Video",
                    })
                    .size(d.meta)
                    .color(theme::t().text_muted),
                    row![
                        button(text("Decline").size(d.meta))
                            .on_press(Message::RejectIncomingCall)
                            .padding(Padding::from([8, 12]))
                            .style(theme::secondary_action(hover)),
                        button(text("Accept").size(d.meta))
                            .on_press(Message::AcceptIncomingCall)
                            .padding(Padding::from([8, 12]))
                            .style(theme::primary_action(hover)),
                    ]
                    .spacing(8),
                ]
                .spacing(8)
                .padding(d.pad_tight),
            )
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().accent_soft)),
                border: iced::Border {
                    color: theme::t().accent,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    if let Some(call) = app.active_call() {
        col = col.push(
            container(
                column![
                    text(format!(
                        "{} — {}",
                        if call.outgoing { "Calling" } else { "In call" },
                        call.peer_name
                    ))
                    .size(d.body)
                    .color(theme::t().text_primary),
                    text("Signaling via SimpleX. Media uses the peer’s WebRTC client when connected.")
                        .size(d.meta)
                        .color(theme::t().text_muted),
                    button(text("End call").size(d.meta))
                        .on_press(Message::EndActiveCall)
                        .padding(Padding::from([8, 12]))
                        .style(theme::primary_action(hover)),
                ]
                .spacing(8)
                .padding(d.pad_tight),
            )
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(theme::t().bg_elevated)),
                border: iced::Border {
                    color: theme::t().border,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    col = col.push(
        text("Start a call")
            .size(d.body)
            .color(theme::t().text_primary),
    );
    col = col.push(
        text("Audio call invite via /_call. Full in-app WebRTC media bridge is next.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );

    let contacts = app.direct_contacts();
    if contacts.is_empty() {
        col = col.push(placeholder(
            "No contacts",
            "Connect with someone in Chats first, then call them here.",
            d,
        ));
    } else {
        for (_index, chat) in contacts {
            let contact_id = chat.id.api_id;
            col = col.push(
                row![
                    profile_avatar(chat.peer_profile.image.as_deref(), &chat.title, 36.0),
                    text(chat.title.clone())
                        .size(d.body)
                        .color(theme::t().text_primary)
                        .width(Length::Fill),
                    button(text("Call").size(d.meta))
                        .on_press(Message::StartAudioCall(contact_id))
                        .padding(Padding::from([8, 12]))
                        .style(theme::secondary_action(hover)),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .padding(Padding::from([6, 4])),
            );
        }
    }
    scroll_y(col).height(Length::Fill).into()
}

fn calls_detail(app: &App) -> Element<'_, Message> {
    let d = app.density();
    if let Some(call) = app.active_call().or(app.incoming_call()) {
        return container(
            column![
                text(call.peer_name.clone())
                    .size(d.title)
                    .color(theme::t().text_primary),
                text(if app.incoming_call().is_some() {
                    "Incoming call"
                } else if call.outgoing {
                    "Outgoing call — waiting for answer"
                } else {
                    "Call in progress (signaling)"
                })
                .size(d.body)
                .color(theme::t().text_secondary),
                text("Cursus sends SimpleX call signaling. Audio/video media needs the WebRTC call page (official desktop opens it in the browser).")
                    .size(d.meta)
                    .color(theme::t().text_muted),
            ]
            .spacing(12)
            .padding(d.pad),
        )
        .width(Length::Fill)
        .into();
    }
    placeholder(
        "Calls",
        "Pick a contact on the left to start an audio call, or wait for an incoming invite.",
        d,
    )
}

/// Image-pen (set) + cancel/X (clear) for own / group / channel profile photos.
fn profile_photo_actions(
    has_image: bool,
    pick: Message,
    clear: Message,
    hover: bool,
) -> Element<'static, Message> {
    let tint = theme::t().text_secondary;
    row![
        button(icons::centered_hit(
            icons::image_pen_icon(20.0, tint),
            36.0,
        ))
        .on_press(pick)
        .padding(0)
        .style(theme::secondary_action(hover)),
        button(icons::centered_hit(
            icons::cancel_icon(18.0, tint),
            36.0,
        ))
        .on_press_maybe(if has_image { Some(clear) } else { None })
        .padding(0)
        .style(theme::secondary_action(hover)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn group_settings_page(app: &App, with_back: bool) -> Element<'_, Message> {
    let d = app.density();
    let hover = app.hover_ok();
    let Some(g) = app.group_settings() else {
        return vertical().height(0).into();
    };

    let (heading, close_msg): (&str, Message) = match g.pane {
        GroupSettingsPane::AddMember => ("Add members", Message::CloseAddGroupMember),
        GroupSettingsPane::Preferences => ("Group preferences", Message::CloseGroupPreferences),
        GroupSettingsPane::MemberInfo => ("Member", Message::CloseGroupMember),
        GroupSettingsPane::ConfirmLeave => ("Leave group", Message::CloseGroupSettings),
        GroupSettingsPane::ConfirmDelete => ("Delete group", Message::CloseGroupSettings),
        GroupSettingsPane::Overview if g.is_channel => ("Channel", Message::CloseGroupSettings),
        GroupSettingsPane::Overview => ("Group", Message::CloseGroupSettings),
    };
    let nested = !matches!(g.pane, GroupSettingsPane::Overview);
    let overview = matches!(g.pane, GroupSettingsPane::Overview);

    let mute_tint = if g.muted {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    let fav_tint = if g.favorite {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    let overview_actions: Element<'_, Message> = row![
        round_header_icon_btn(
            icons::star_icon(18.0, fav_tint),
            Message::GroupSettingsFavorite,
            hover,
        ),
        round_header_icon_btn(
            icons::mute_icon(18.0, mute_tint),
            Message::GroupSettingsMute,
            hover,
        ),
        round_header_icon_btn(
            icons::preferences_icon(18.0, theme::t().text_secondary),
            Message::OpenGroupPreferences,
            hover,
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into();

    let leading: Element<'_, Message> = if with_back {
        row![
            button(icons::back_icon(18.0))
                .on_press(if nested {
                    Message::Back
                } else {
                    close_msg.clone()
                })
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
            text(heading)
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    } else if nested {
        row![
            button(icons::back_icon(18.0))
                .on_press(Message::Back)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
            text(heading)
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(heading)
            .size(d.title)
            .color(theme::t().text_primary)
            .into()
    };

    let header = if overview {
        profile_header_bar(leading, overview_actions, d, with_back)
    } else if with_back || nested {
        let pad = if with_back {
            Padding {
                top: d.pad_tight,
                right: d.pad,
                bottom: d.pad_tight,
                left: d.pad_tight,
            }
        } else {
            Padding::from([d.pad_tight, 0.0])
        };
        row![leading]
            .align_y(Alignment::Center)
            .padding(pad)
            .into()
    } else {
        profile_header_bar(leading, horizontal().width(0).into(), d, with_back)
    };

    let body: Element<'_, Message> = match g.pane {
        GroupSettingsPane::AddMember => group_add_member_pane(app, g, d, hover),
        GroupSettingsPane::Preferences => group_preferences_pane(g, d, hover),
        GroupSettingsPane::MemberInfo => group_member_info_pane(g, d, hover),
        GroupSettingsPane::ConfirmLeave => column![
            icons::centered_hit(
                icons::leave_icon(28.0, theme::t().text_secondary),
                48.0,
            ),
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
            icons::centered_hit(
                icons::trash_icon(28.0, theme::t().danger),
                48.0,
            ),
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
            let narrow = matches!(app.layout(), LayoutMode::Mobile)
                || app.window_width() < 720.0;
            group_overview_pane(g, d, hover, narrow)
        }
    };

    column![
        header,
        scroll_y(
            container(body).padding(Padding {
                top: d.pad_tight,
                right: d.pad,
                bottom: d.pad,
                left: d.pad,
            }),
        )
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn group_overview_pane(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
    narrow: bool,
) -> Element<'_, Message> {
    let owner = can_edit_group_profile(&g.my_role);
    let admin = can_manage_members(&g.my_role);
    let photo_label = if g.is_channel {
        "Channel photo"
    } else {
        "Group photo"
    };
    let mut col = Column::new().spacing(10).width(Length::Fill);
    col = col.push(
        row![
            profile_avatar(g.image.as_deref(), &g.name, 64.0),
            column![
                text(photo_label)
                    .size(d.meta)
                    .color(theme::t().text_muted),
                if owner {
                    profile_photo_actions(
                        g.image.is_some(),
                        Message::PickGroupProfileImage,
                        Message::ClearGroupProfileImage,
                        hover,
                    )
                } else {
                    vertical().height(0).into()
                },
            ]
            .spacing(6),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );

    col = col.push(text("Name").size(d.meta).color(theme::t().text_muted));
    if owner {
        col = col.push(
            text_input("Display name", &g.name)
                .on_input(Message::GroupSettingsNameChanged)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
        );
    } else {
        col = col.push(
            text(g.name.clone())
                .size(d.body)
                .color(theme::t().text_primary),
        );
    }

    col = col.push(
        text("Short description")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    if owner {
        col = col.push(
            text_input("Optional short description", &g.short_descr)
                .on_input(Message::GroupSettingsShortDescrChanged)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
        );
    } else if !g.short_descr.is_empty() {
        col = col.push(
            text(g.short_descr.clone())
                .size(d.body)
                .color(theme::t().text_secondary),
        );
    }

    col = col.push(
        text("Welcome message")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    if owner {
        col = col.push(
            text_input("Message shown to new members", &g.welcome_message)
                .on_input(Message::GroupSettingsWelcomeChanged)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
        );
    } else if !g.welcome_message.is_empty() {
        col = col.push(
            text(g.welcome_message.clone())
                .size(d.body)
                .color(theme::t().text_secondary),
        );
    }

    if owner {
        col = col.push(
            button(text("Save profile").size(d.meta))
                .on_press_maybe(if g.dirty {
                    Some(Message::SaveGroupProfile)
                } else {
                    None
                })
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(hover)),
        );
    }

    col = col.push(vertical().height(4));
    col = col.push(
        text("Local name")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    col = col.push(alias_save_row(
        &g.alias,
        "Name shown only to you",
        Message::GroupSettingsAliasChanged,
        Message::SaveGroupAlias,
        g.alias_dirty,
        d,
        hover,
    ));

    if !g.is_channel {
        let support_ok = group_support_allowed(g.group_preferences.as_ref());
        let moderate = can_moderate(&g.my_role);
        if support_ok || moderate {
            col = col.push(vertical().height(6));
            col = col.push(
                text("Support")
                    .size(d.meta)
                    .color(theme::t().text_muted),
            );
            if moderate {
                col = col.push(
                    button(
                        row![
                            icons::admin_icon(20.0, theme::t().text_secondary),
                            text("Chats with members")
                                .size(d.body)
                                .color(theme::t().text_primary)
                                .width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenSupportChatList)
                    .padding(Padding::from([10, 8]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            } else {
                col = col.push(
                    button(
                        row![
                            icons::admin_icon(20.0, theme::t().text_secondary),
                            text("Chat with admins")
                                .size(d.body)
                                .color(theme::t().text_primary)
                                .width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenUserSupportChat)
                    .padding(Padding::from([10, 8]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
                );
            }
        }
    }

    col = col.push(vertical().height(6));
    col = col.push(
        text(format!("Members ({})", g.members.len()))
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    if g.members_loading {
        col = col.push(
            text("Loading…")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else if g.members.is_empty() {
        col = col.push(
            text("No members listed.")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else {
        for m in &g.members {
            let mid = m.group_member_id;
            let status = if m.status.is_empty() {
                String::new()
            } else {
                format!(" · {}", m.status)
            };
            col = col.push(
                button(
                    row![
                        text(m.display_name.clone())
                            .size(d.body)
                            .color(theme::t().text_primary)
                            .width(Length::Fill),
                        text(format!("{}{status}", m.role))
                            .size(d.meta)
                            .color(theme::t().text_muted),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenGroupMember(mid))
                .padding(Padding::from([8, 4]))
                .width(Length::Fill)
                .style(theme::menu_item(hover)),
            );
        }
    }
    // Private groups: invite contacts directly. Channels (useRelays): core
    // rejects /_add — share the invite link instead (same as official apps).
    if admin && !g.is_channel {
        col = col.push(
            button(text("Add members").size(d.meta))
                .on_press(Message::OpenAddGroupMember)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
        );
    }

    col = col.push(vertical().height(4));
    col = col.push(
        text(if g.is_channel {
            "Channel link"
        } else {
            "Invite link"
        })
        .size(d.meta)
        .color(theme::t().text_muted),
    );
    // Official SimpleX: channel links are share-only (no delete / short-upgrade /
    // role). Private groups get short upgrade + delete + joiner role.
    if g.is_channel {
        col = col.push(
            text(
                "You can share a link to this channel — anyone with it can join \
via chat relays. Copy the link below (it must contain /c#).",
            )
            .size(d.meta)
            .color(theme::t().text_secondary),
        );
    }
    if let Some(link) = g.link.as_ref() {
        let mut link_row = Row::new()
            .spacing(4)
            .align_y(Alignment::Center)
            .push(
                text(truncate_middle(link, if g.is_channel { 52 } else { 44 }))
                    .size(d.meta)
                    .color(theme::t().accent)
                    .width(Length::Fill),
            )
            .push(
                button(icons::centered_hit(
                    icons::copy_icon(18.0, theme::t().text_secondary),
                    36.0,
                ))
                .on_press(Message::CopyGroupLink)
                .width(36.0)
                .height(36.0)
                .padding(0)
                .style(theme::icon_action(hover)),
            );
        if !g.is_channel && admin {
            link_row = link_row.push(
                button(icons::centered_hit(
                    icons::trash_icon(18.0, theme::t().danger),
                    36.0,
                ))
                .on_press(Message::DeleteGroupLink)
                .width(36.0)
                .height(36.0)
                .padding(0)
                .style(theme::danger_quiet(hover)),
            );
        }
        col = col.push(link_row);
        if !g.is_channel {
            col = col.push(
                button(text("Short / profile link").size(d.meta))
                    .on_press(Message::AddGroupShortLink)
                    .padding(Padding::from([8, 12]))
                    .style(theme::secondary_action(hover)),
            );
            if admin {
                col = col.push(
                    button(
                        text(format!(
                            "Joiner role: {} (observer/member)",
                            g.link_member_role
                        ))
                        .size(d.meta),
                    )
                    .on_press(Message::GroupLinkRoleCycle)
                    .padding(Padding::from([8, 12]))
                    .style(theme::secondary_action(hover)),
                );
            }
        }
    } else if g.is_channel {
        col = col.push(
            text(
                if admin {
                    "No channel invite link on this channel. SimpleX doesn’t recreate \
channel links after they’re gone — create a new channel to invite people."
                } else {
                    "Only the owner can share the channel link."
                },
            )
            .size(d.meta)
            .color(theme::t().text_muted),
        );
    } else if admin {
        col = col.push(
            button(text("Create / show link").size(d.meta))
                .on_press(Message::EnsureGroupLink)
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(hover)),
        );
    } else {
        col = col.push(
            text("Only admins can manage the invite link.")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    }

    col = col.push(vertical().height(10));
    col = col.push(
        text("Chat actions")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    col = col.push(group_danger_actions(g.is_channel, d, hover, narrow));
    col.into()
}

/// Alias field + inline check save (contact / group / channel).
fn alias_save_row<'a>(
    value: &'a str,
    placeholder: &'a str,
    on_changed: impl Fn(String) -> Message + 'a,
    save: Message,
    dirty: bool,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    let check_color = if dirty {
        theme::t().accent
    } else {
        theme::t().text_muted
    };
    row![
        text_input(placeholder, value)
            .on_input(on_changed)
            .padding(d.composer)
            .size(d.body)
            .width(Length::Fill)
            .style(theme::composer_input()),
        button(icons::centered_hit(
            icons::check_sent(20.0, check_color),
            40.0,
        ))
        .on_press_maybe(if dirty { Some(save) } else { None })
        .padding(0)
        .style(theme::quiet_tile(hover)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn action_tile_row(
    tiles: Vec<iced::widget::Button<'static, Message>>,
    narrow: bool,
) -> Element<'static, Message> {
    if narrow {
        let mut col = Column::new().spacing(8).width(Length::Fill);
        for tile in tiles {
            col = col.push(tile);
        }
        col.into()
    } else {
        let mut row = Row::new().spacing(8).width(Length::Fill);
        for tile in tiles {
            row = row.push(tile.width(Length::Fill));
        }
        row.into()
    }
}

/// Mute / favorite / preferences — same tile language as clear/leave/delete.
fn peer_allow_icon(allow: crate::FeatureAllow) -> Element<'static, Message> {
    let color = match allow {
        crate::FeatureAllow::No => theme::t().danger,
        crate::FeatureAllow::Yes => theme::t().success,
        crate::FeatureAllow::Always => theme::t().always,
    };
    icons::user_alt_icon(16.0, color)
}

fn contact_feature_row(
    icon: Element<'static, Message>,
    label: &'static str,
    you: crate::FeatureAllow,
    peer: crate::FeatureAllow,
    on_press: Message,
    d: Density,
    hover: bool,
) -> Element<'static, Message> {
    button(
        row![
            icon,
            text(label)
                .size(d.body)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            text(you.label())
                .size(d.meta)
                .color(theme::t().text_muted),
            peer_allow_icon(peer),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(on_press)
    .padding(Padding::from([10, 8]))
    .width(Length::Fill)
    .style(theme::menu_item(hover))
    .into()
}

fn contact_feature_prefs(
    p: &crate::ContactProfileOverlay,
    d: Density,
    hover: bool,
) -> Element<'static, Message> {
    let icon_tint = theme::t().text_secondary;
    let mut col = Column::new().spacing(4).width(Length::Fill);

    col = col.push(contact_feature_row(
        icons::timer_icon(20.0, icon_tint),
        "Disappearing messages",
        p.timed_allow,
        p.timed_contact_allows,
        Message::CycleContactTimedMessages,
        d,
        hover,
    ));

    if p.timed_allow.is_on() {
        col = col.push(
            text("Delete after")
                .size(d.meta)
                .color(theme::t().text_muted)
                .width(Length::Fill),
        );
        // Two rows so every TTL is visible at once (no cycle).
        let mut ttl_col = Column::new().spacing(6).width(Length::Fill);
        for chunk in crate::TIMED_TTL_STEPS.chunks(3) {
            let mut ttl_row = Row::new().spacing(6).width(Length::Fill);
            for &secs in chunk {
                let selected = p.timed_ttl_secs == secs;
                ttl_row = ttl_row.push(
                    button(
                        text(crate::format_timed_ttl(secs))
                            .size(d.meta)
                            .color(if selected {
                                theme::t().accent
                            } else {
                                theme::t().text_primary
                            }),
                    )
                    .on_press(Message::SetContactTimedTtl(secs))
                    .padding(Padding::from([8, 10]))
                    .style(theme::choice_chip(selected, hover)),
                );
            }
            ttl_col = ttl_col.push(ttl_row);
        }
        col = col.push(ttl_col.padding(Padding::from([0, 4])));
    }

    col = col.push(contact_feature_row(
        icons::trash_icon(20.0, icon_tint),
        "Delete for everyone",
        p.full_delete_allow,
        p.full_delete_contact_allows,
        Message::CycleContactFullDelete,
        d,
        hover,
    ));
    col = col.push(contact_feature_row(
        icons::reaction_icon(20.0, icon_tint),
        "Message reactions",
        p.reactions_allow,
        p.reactions_contact_allows,
        Message::CycleContactReactions,
        d,
        hover,
    ));
    col = col.push(contact_feature_row(
        icons::mic_icon(20.0, icon_tint),
        "Voice messages",
        p.voice_allow,
        p.voice_contact_allows,
        Message::CycleContactVoice,
        d,
        hover,
    ));
    col = col.push(contact_feature_row(
        icons::phone_icon(20.0, icon_tint),
        "Audio/video calls",
        p.calls_allow,
        p.calls_contact_allows,
        Message::CycleContactCalls,
        d,
        hover,
    ));

    col.into()
}

fn contact_danger_actions(
    d: Density,
    hover: bool,
    narrow: bool,
) -> Element<'static, Message> {
    action_tile_row(
        vec![
            group_action_tile(
                icons::clear_all_icon(18.0, theme::t().text_secondary),
                "Clear chat",
                Message::ContactProfileClear,
                theme::quiet_tile(hover),
                d,
                narrow,
                false,
            ),
            group_action_tile(
                icons::trash_icon(18.0, theme::t().danger),
                "Delete contact",
                Message::ContactProfileDelete,
                theme::danger_quiet(hover),
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
    action_tile_row(
        vec![
            group_action_tile(
                icons::clear_all_icon(18.0, theme::t().text_secondary),
                "Clear chat",
                Message::ClearGroupChat,
                theme::quiet_tile(hover),
                d,
                narrow,
                false,
            ),
            group_action_tile(
                icons::leave_icon(18.0, theme::t().text_secondary),
                leave_label,
                Message::OpenLeaveGroupConfirm,
                theme::quiet_tile(hover),
                d,
                narrow,
                false,
            ),
            group_action_tile(
                icons::trash_icon(18.0, theme::t().danger),
                delete_label,
                Message::OpenDeleteGroupConfirm,
                theme::danger_quiet(hover),
                d,
                narrow,
                true,
            ),
        ],
        narrow,
    )
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
    let label_color = if danger {
        theme::t().danger
    } else {
        theme::t().text_primary
    };
    let body: Element<'_, Message> = if narrow {
        row![
            icons::centered_hit(icon, 28.0),
            text(label)
                .size(d.meta)
                .color(label_color)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    } else {
        column![
            icons::centered_hit(icon, 28.0),
            text(label)
                .size(d.meta)
                .color(label_color),
        ]
        .spacing(6)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .into()
    };

    button(
        container(body)
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .on_press(on_press)
    .padding(if narrow {
        Padding::from([8, 10])
    } else {
        Padding::from([10, 8])
    })
    .width(Length::Fill)
    .style(style)
}

fn group_add_member_pane<'a>(
    app: &'a App,
    g: &'a crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'a, Message> {
    if g.is_channel {
        return column![
            text("Channels don’t support adding contacts.")
                .size(d.body)
                .color(theme::t().text_primary),
            text("Go back and create/copy the channel link to invite people.")
                .size(d.meta)
                .color(theme::t().text_secondary),
            button(text("Back to channel settings").size(d.meta))
                .on_press(Message::CloseAddGroupMember)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
        ]
        .spacing(10)
        .into();
    }
    let mut list = Column::new().spacing(6).width(Length::Fill);
    list = list.push(
        text("Invite a contact into this group.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );
    list = list.push(
        button(text(format!("Invite as: {}", g.invite_role)).size(d.meta))
            .on_press(Message::GroupInviteRoleCycle)
            .padding(Padding::from([8, 12]))
            .style(theme::secondary_action(hover)),
    );
    let contacts = app.contacts_to_add_to_group();
    if contacts.is_empty() {
        list = list.push(
            text("No contacts left to add (connect someone in Chats first).")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else {
        for chat in contacts {
            let contact_id = chat.id.api_id;
            list = list.push(
                row![
                    profile_avatar(chat.peer_profile.image.as_deref(), &chat.title, 32.0),
                    text(chat.title.clone())
                        .size(d.body)
                        .color(theme::t().text_primary)
                        .width(Length::Fill),
                    button(text("Add").size(d.meta))
                        .on_press(Message::AddGroupMember(contact_id))
                        .padding(Padding::from([8, 12]))
                        .style(theme::primary_action(hover)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
    }
    list.into()
}

fn group_pref_icon(key: &str) -> Element<'static, Message> {
    let tint = theme::t().text_secondary;
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

    let leading: Element<'_, Message> = if with_back {
        row![
            button(icons::back_icon(18.0))
                .on_press(Message::CloseSupportOverlay)
                .padding(Padding::from([8, 12]))
                .style(theme::secondary_action(hover)),
            text("Chats with members")
                .size(d.title)
                .color(theme::t().text_primary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("Chats with members")
            .size(d.title)
            .color(theme::t().text_primary)
            .into()
    };
    let header = profile_header_bar(leading, horizontal().width(0).into(), d, with_back);

    let mut body = Column::new().spacing(8).width(Length::Fill);
    body = body.push(
        text("Support threads members opened with admins.")
            .size(d.meta)
            .color(theme::t().text_secondary),
    );
    if s.members_loading {
        body = body.push(
            text("Loading…")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    } else if s.members.is_empty() {
        body = body.push(
            text("No support chats yet.")
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
            body = body.push(
                row![
                    button(
                        row![
                            text(m.display_name.clone())
                                .size(d.body)
                                .color(theme::t().text_primary)
                                .width(Length::Fill),
                            if unread > 0 {
                                text(format!("{unread}"))
                                    .size(d.meta)
                                    .color(theme::t().always)
                            } else {
                                text("").size(d.meta)
                            },
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenMemberSupportChat {
                        member_id: mid,
                        title,
                    })
                    .padding(Padding::from([10, 8]))
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

    column![
        header,
        scroll_y(body.padding(Padding {
            top: d.pad_tight,
            right: d.pad,
            bottom: d.pad,
            left: d.pad,
        }))
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn group_preferences_pane(
    g: &crate::GroupSettingsOverlay,
    d: Density,
    hover: bool,
) -> Element<'_, Message> {
    let owner = can_edit_group_profile(&g.my_role);
    let prefs = g.group_preferences.as_ref();
    let mut col = Column::new().spacing(4).width(Length::Fill);
    col = col.push(
        text("Chat features")
            .size(d.meta)
            .color(theme::t().text_muted),
    );
    col = col.push(
        text(if owner {
            "Owners can change these features for everyone."
        } else {
            "View only — only owners can change preferences."
        })
        .size(d.meta)
        .color(theme::t().text_secondary),
    );
    for (key, label) in [
        ("timedMessages", "Disappearing messages"),
        ("directMessages", "Direct messages"),
        ("fullDelete", "Full deletion"),
        ("reactions", "Message reactions"),
        ("voice", "Voice messages"),
        ("files", "Files and media"),
        ("simplexLinks", "SimpleX links"),
        ("history", "Recent history"),
        ("support", "Chat with admins"),
    ] {
        let on = if key == "support" {
            group_support_allowed(prefs)
        } else {
            group_pref_on(prefs, key)
        };
        let row = row![
            group_pref_icon(key),
            text(label)
                .size(d.body)
                .color(theme::t().text_primary)
                .width(Length::Fill),
            text(if on { "yes" } else { "no" })
                .size(d.meta)
                .color(theme::t().text_muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if owner {
            col = col.push(
                button(row)
                    .on_press(Message::GroupPrefToggle(key))
                    .padding(Padding::from([10, 8]))
                    .width(Length::Fill)
                    .style(theme::menu_item(hover)),
            );
        } else {
            col = col.push(row.padding(Padding::from([10, 8])));
        }
        if key == "timedMessages" && on {
            let current = prefs
                .and_then(|p| p.get("timedMessages"))
                .and_then(|v| v.get("ttl"))
                .and_then(|v| v.as_i64())
                .unwrap_or(86_400);
            col = col.push(
                text("Delete after")
                    .size(d.meta)
                    .color(theme::t().text_muted)
                    .width(Length::Fill),
            );
            let mut ttl_col = Column::new().spacing(6).width(Length::Fill);
            for chunk in crate::TIMED_TTL_STEPS.chunks(3) {
                let mut ttl_row = Row::new().spacing(6).width(Length::Fill);
                for &secs in chunk {
                    let selected = current == secs;
                    let chip = button(
                        text(crate::format_timed_ttl(secs))
                            .size(d.meta)
                            .color(if selected {
                                theme::t().accent
                            } else {
                                theme::t().text_primary
                            }),
                    )
                    .padding(Padding::from([8, 10]))
                    .style(theme::choice_chip(selected, hover));
                    ttl_row = ttl_row.push(if owner {
                        chip.on_press(Message::SetGroupTimedTtl(secs))
                    } else {
                        chip
                    });
                }
                ttl_col = ttl_col.push(ttl_row);
            }
            col = col.push(ttl_col.padding(Padding::from([0, 4])));
        }
    }
    let review_on = g
        .member_admission
        .as_ref()
        .and_then(|v| v.get("review"))
        .is_some();
    let admission = row![
        icons::review_file_icon(20.0, theme::t().text_secondary),
        text("Review members before join")
            .size(d.body)
            .color(theme::t().text_primary)
            .width(Length::Fill),
        text(if review_on { "yes" } else { "no" })
            .size(d.meta)
            .color(theme::t().text_muted),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if owner {
        col = col.push(
            button(admission)
                .on_press(Message::GroupAdmissionToggle)
                .padding(Padding::from([10, 8]))
                .width(Length::Fill)
                .style(theme::menu_item(hover)),
        );
        col = col.push(
            button(text("Save preferences").size(d.meta))
                .on_press_maybe(if g.dirty {
                    Some(Message::SaveGroupPreferences)
                } else {
                    None
                })
                .padding(Padding::from([10, 14]))
                .style(theme::primary_action(hover)),
        );
    } else {
        col = col.push(admission.padding(Padding::from([10, 8])));
    }
    col.into()
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
    let mut col = Column::new().spacing(10).width(Length::Fill);
    col = col.push(
        text(m.display_name.clone())
            .size(d.title)
            .color(theme::t().text_primary),
    );
    col = col.push(
        text(format!("Role: {}", m.role))
            .size(d.body)
            .color(theme::t().text_secondary),
    );
    if !m.status.is_empty() {
        col = col.push(
            text(format!("Status: {}", m.status))
                .size(d.meta)
                .color(theme::t().text_muted),
        );
    }
    if m.contact_id.is_some() {
        col = col.push(
            button(text("Open direct chat").size(d.meta))
                .on_press(Message::OpenMemberDirectChat)
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(hover)),
        );
    }
    if mod_plus
        && !g.is_channel
        && (m.support_chat.is_some()
            || (group_support_allowed(g.group_preferences.as_ref())
                && group_role_rank(&m.role) < group_role_rank("moderator")))
    {
        col = col.push(
            button(
                row![
                    icons::admin_icon(18.0, theme::t().text_secondary),
                    text("Support chat").size(d.meta),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenMemberSupportChat {
                member_id: m.group_member_id,
                title: m.display_name.clone(),
            })
            .padding(Padding::from([8, 12]))
            .style(theme::secondary_action(hover)),
        );
    }
    if m.status.contains("pending") || m.status.contains("review") {
        col = col.push(
            button(text("Accept member").size(d.meta))
                .on_press(Message::GroupMemberAccept)
                .padding(Padding::from([8, 12]))
                .style(theme::primary_action(hover)),
        );
    }
    if admin {
        col = col.push(
            text("Change role")
                .size(d.meta)
                .color(theme::t().text_muted),
        );
        for role in ["observer", "member", "moderator", "admin"] {
            if role == m.role {
                continue;
            }
            col = col.push(
                button(text(format!("Set {role}")).size(d.meta))
                    .on_press(Message::GroupMemberSetRole(role.into()))
                    .padding(Padding::from([8, 12]))
                    .style(theme::secondary_action(hover)),
            );
        }
        col = col.push(
            button(text("Remove member").size(d.meta))
                .on_press(Message::GroupMemberRemove {
                    with_messages: false,
                })
                .padding(Padding::from([8, 12]))
                .style(theme::danger_action(hover)),
        );
        col = col.push(
            button(text("Remove + delete messages").size(d.meta))
                .on_press(Message::GroupMemberRemove {
                    with_messages: true,
                })
                .padding(Padding::from([8, 12]))
                .style(theme::danger_action(hover)),
        );
    }
    col = col.push(
        button(
            text(if m.show_messages {
                "Hide messages (for me)"
            } else {
                "Show messages (for me)"
            })
            .size(d.meta),
        )
        .on_press(Message::GroupMemberToggleBlockSelf)
        .padding(Padding::from([8, 12]))
        .style(theme::secondary_action(hover)),
    );
    if mod_plus {
        col = col.push(
            button(
                text(if m.blocked_by_admin {
                    "Unblock for all"
                } else {
                    "Block for all"
                })
                .size(d.meta),
            )
            .on_press(Message::GroupMemberToggleBlockAll)
            .padding(Padding::from([8, 12]))
            .style(theme::secondary_action(hover)),
        );
    }
    col.into()
}

fn create_chat_overlay(app: &App) -> Element<'_, Message> {
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
            text(title)
                .size(d.body)
                .color(theme::t().text_primary),
            text(hint)
                .size(d.meta)
                .color(theme::t().text_secondary),
            text_input(placeholder_name, &draft.name)
                .on_input(Message::CreateChatNameChanged)
                .on_submit(Message::ConfirmCreateChat)
                .padding(d.composer)
                .size(d.body)
                .width(Length::Fill)
                .style(theme::composer_input()),
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
    .on_press(Message::CloseCreateChat);
    let sheet = container(card)
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

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

/// Middle ellipsis so a selected preview is obviously incomplete (use the copy button).
fn truncate_middle(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    if max_chars < 5 {
        return truncate(s, max_chars);
    }
    let keep = (max_chars - 1) / 2;
    let head: String = chars.iter().take(keep).collect();
    let tail: String = chars.iter().rev().take(keep).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{head}…{tail}")
}
