//! Navigation icons from vendored SVG assets (see `assets/icons/ATTRIBUTION.md`).
//!
//! Drawn via iced’s SVG renderer and tinted with a color filter so dark/light
//! chrome stays consistent without editing paths at runtime.

use iced::widget::{container, svg, text, Svg};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length};

use crate::theme;
use crate::Message;

#[derive(Debug, Clone, Copy)]
pub enum NavIcon {
    Chats,
    Channels,
    Calls,
    Settings,
}

pub fn nav_icon<'a>(kind: NavIcon, active: bool, size: f32) -> Element<'a, Message> {
    let color = if active {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    tinted(handle_for(kind), color, size)
}

/// Chat bubbles — nav Chats glyph (also usable as a generic chats mark).
pub fn chats_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(handle_for(NavIcon::Chats), color, size)
}

/// Admin / “Chat with admins” — Instructure UI Line Icons (MIT via SVG Repo).
pub fn admin_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/admin.svg").as_slice()),
        color,
        size,
    )
}

pub fn back_icon<'a>(size: f32) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/back.svg").as_slice()),
        theme::t().text_primary,
        size,
    )
}

/// Paper-plane send glyph (Dazzle Line Icons / CC Attribution).
pub fn send_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/send.svg").as_slice()),
        color,
        size,
    )
}

/// Attach / plus glyph (Zest Interface Icons / MIT).
pub fn plus_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/plus.svg").as_slice()),
        color,
        size,
    )
}

/// Center an icon inside a square hit target (iced buttons lay out content top-left).
pub fn centered_hit<'a>(icon: Element<'a, Message>, hit: f32) -> Element<'a, Message> {
    container(icon)
        .width(hit)
        .height(hit)
        .center_x(hit)
        .center_y(hit)
        .into()
}

/// Collapse control (Zest Interface Icons / MIT).
pub fn chevron_up<'a>(size: f32) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/chevron-up.svg").as_slice()),
        theme::t().text_secondary,
        size,
    )
}

/// Expand control (Zest Interface Icons / MIT).
pub fn chevron_down<'a>(size: f32) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/chevron-down.svg").as_slice()),
        theme::t().text_secondary,
        size,
    )
}

/// Single check — sent (Coolicons / CC Attribution).
pub fn check_sent<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/check.svg").as_slice()),
        color,
        size,
    )
}

/// Double check — delivered/read (Solar Icons / CC Attribution).
pub fn check_read<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/check-read.svg").as_slice()),
        color,
        size,
    )
}

/// Download / save glyph (Solar Bold Icons / CC Attribution).
pub fn download_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/download.svg").as_slice()),
        color,
        size,
    )
}

/// Image / media glyph (Solar Icons / CC Attribution).
pub fn image_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/image-square.svg").as_slice()),
        color,
        size,
    )
}

/// Generic file glyph — reuses image-square outline at muted weight for non-images.
pub fn file_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    image_icon(size, color)
}

/// Microphone — Cursus original (CC0).
pub fn mic_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/mic.svg").as_slice()),
        color,
        size,
    )
}

/// Play triangle — Cursus original (CC0).
pub fn play_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/play.svg").as_slice()),
        color,
        size,
    )
}

/// Stop square — Cursus original (CC0).
pub fn stop_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/stop.svg").as_slice()),
        color,
        size,
    )
}

/// Cancel / close — Interface Line Icons / kreevstudio (CC Attribution via SVG Repo).
pub fn cancel_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/cancel.svg").as_slice()),
        color,
        size,
    )
}

/// Trash / marked-deleted — Wave Oval Interface Icons / Arthur Kazais (Public Domain via SVG Repo).
pub fn trash_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/trash-delete.svg").as_slice()),
        color,
        size,
    )
}

/// Copy to clipboard — Solar Linear Icons / Solar Icons (CC Attribution via SVG Repo).
pub fn copy_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/copy.svg").as_slice()),
        color,
        size,
    )
}

/// Refresh / rotate — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn refresh_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(
            include_bytes!("../assets/icons/refresh-ccw-alt.svg").as_slice(),
        ),
        color,
        size,
    )
}

/// Leave / exit session — Gitlab Interface Icons / Gitlab (MIT via SVG Repo).
pub fn leave_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/session-leave.svg").as_slice()),
        color,
        size,
    )
}

/// Clear all — Keyicons Interface Icons / Elnodos (CC Attribution via SVG Repo).
pub fn clear_all_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/clear-all.svg").as_slice()),
        color,
        size,
    )
}

/// Mute notifications — Element Plus Line Interface Icons / element-plus (MIT via SVG Repo).
pub fn mute_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(
            include_bytes!("../assets/icons/mute-notification.svg").as_slice(),
        ),
        color,
        size,
    )
}

/// Group preferences / tools — Element Plus Line Interface Icons / element-plus (MIT).
pub fn preferences_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/preferences.svg").as_slice()),
        color,
        size,
    )
}

/// Next media — Uxaspects Sharp Cornered Icons (Apache-2.0 via SVG Repo).
pub fn next_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/next.svg").as_slice()),
        color,
        size,
    )
}

/// Previous media — Uxaspects Sharp Cornered Icons (Apache-2.0 via SVG Repo).
pub fn previous_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/previous.svg").as_slice()),
        color,
        size,
    )
}

/// Favorite star — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn star_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/star.svg").as_slice()),
        color,
        size,
    )
}

/// Disappearing-messages timer — Gentlecons Interface Icons (CC Attribution via SVG Repo).
pub fn timer_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/timer.svg").as_slice()),
        color,
        size,
    )
}

/// Direct messages — Framework7 Line Icons / framework7io (MIT via SVG Repo).
pub fn paperplane_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/paperplane.svg").as_slice()),
        color,
        size,
    )
}

/// SimpleX links — Solar Broken Line Icons (CC Attribution via SVG Repo).
pub fn link_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(
            include_bytes!("../assets/icons/link-minimalistic-2.svg").as_slice(),
        ),
        color,
        size,
    )
}

/// Recent history — Solar Linear Icons (CC Attribution via SVG Repo).
pub fn history_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/history.svg").as_slice()),
        color,
        size,
    )
}

/// Review members before join — Iconship Interface Icons / Giovana (CC Attribution via SVG Repo).
pub fn review_file_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/review-file.svg").as_slice()),
        color,
        size,
    )
}

/// Message reactions — Classjob Ff Game Icons / xivapi (MIT via SVG Repo).
pub fn reaction_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(
            include_bytes!("../assets/icons/reaction-emoji-add.svg").as_slice(),
        ),
        color,
        size,
    )
}

/// Peer / contact allow indicator — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn user_alt_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/user-alt-1.svg").as_slice()),
        color,
        size,
    )
}

/// Phone / calls — Solar Linear Icons (CC BY via SVG Repo); same asset as nav Calls.
pub fn phone_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/calls.svg").as_slice()),
        color,
        size,
    )
}

/// Edit / set profile picture — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn image_pen_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg::Handle::from_memory(include_bytes!("../assets/icons/image-pen.svg").as_slice()),
        color,
        size,
    )
}

pub fn avatar_initial<'a>(label: &str, size: f32) -> Element<'a, Message> {
    let ch = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    container(text(ch).size(size * 0.42).color(theme::t().text_primary))
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(theme::t().bg_elevated)),
            border: Border {
                color: theme::t().border,
                width: 1.0,
                radius: (size * 0.5).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn handle_for(kind: NavIcon) -> svg::Handle {
    let bytes: &'static [u8] = match kind {
        NavIcon::Chats => include_bytes!("../assets/icons/chats.svg"),
        NavIcon::Channels => include_bytes!("../assets/icons/channels.svg"),
        NavIcon::Calls => include_bytes!("../assets/icons/calls.svg"),
        NavIcon::Settings => include_bytes!("../assets/icons/settings.svg"),
    };
    svg::Handle::from_memory(bytes)
}

fn tinted<'a>(handle: svg::Handle, color: Color, size: f32) -> Element<'a, Message> {
    let icon: Svg<'a, iced::Theme> = svg(handle)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style {
            color: Some(color),
        });

    container(icon)
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .into()
}

/// Composer chrome icon: fills `hit` for hover hit-testing, draws the SVG at
/// its intrinsic size (~24px) **centered** via [`ContentFit::ScaleDown`] (no
/// upscale), and tints via [`svg::Status`]. Shrinking `hit` does not change
/// the glyph as long as `hit` ≥ intrinsic size.
pub fn composer_tool_icon<'a>(
    handle: svg::Handle,
    hit: f32,
    idle: Color,
    hot: Color,
    hover_ok: bool,
) -> Element<'a, Message> {
    let icon: Svg<'a, iced::Theme> = svg(handle)
        .width(Length::Fixed(hit))
        .height(Length::Fixed(hit))
        .content_fit(ContentFit::ScaleDown)
        .style(move |_, status| svg::Style {
            color: Some(match status {
                svg::Status::Hovered if hover_ok => hot,
                _ => idle,
            }),
        });
    icon.into()
}

pub fn plus_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    composer_tool_icon(
        svg::Handle::from_memory(include_bytes!("../assets/icons/plus.svg").as_slice()),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn mic_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    composer_tool_icon(
        svg::Handle::from_memory(include_bytes!("../assets/icons/mic.svg").as_slice()),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn send_tool<'a>(hit: f32, active: bool, hover_ok: bool) -> Element<'a, Message> {
    let handle =
        svg::Handle::from_memory(include_bytes!("../assets/icons/send.svg").as_slice());
    if active {
        // Solid accent button — fixed on-accent glyph, no hover swap needed.
        composer_tool_icon(
            handle,
            hit,
            theme::t().accent_on,
            theme::t().accent_on,
            false,
        )
    } else {
        composer_tool_icon(
            handle,
            hit,
            theme::t().text_secondary,
            theme::t().text_primary,
            hover_ok,
        )
    }
}
