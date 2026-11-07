//! Navigation icons from vendored SVG assets (see `assets/icons/ATTRIBUTION.md`).
//!
//! Drawn via iced’s SVG renderer and tinted with a color filter so dark/light
//! chrome stays consistent without editing paths at runtime.

use iced::widget::{container, svg, text, Svg};
use iced::{Alignment, Background, Border, Color, ContentFit, Element, Length};

use crate::theme;
use crate::Message;

/// Static handle per icon: `from_memory` hashes the SVG bytes for its id,
/// and `view()` builds these for every visible row on every rebuild.
macro_rules! svg_handle {
    ($path:literal) => {{
        static HANDLE: std::sync::LazyLock<svg::Handle> =
            std::sync::LazyLock::new(|| svg::Handle::from_memory(include_bytes!($path).as_slice()));
        HANDLE.clone()
    }};
}

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

/// Admin / “Chat with admins” — Instructure UI Line Icons (MIT via SVG Repo).
pub fn admin_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/admin.svg"),
        color,
        size,
    )
}

/// Paper-plane send glyph (Dazzle Line Icons / CC Attribution).
/// Attach / plus glyph (Zest Interface Icons / MIT).
pub fn plus_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/plus.svg"),
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

/// Expand control (Zest Interface Icons / MIT).
pub fn chevron_down<'a>(size: f32) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/chevron-down.svg"),
        theme::t().text_secondary,
        size,
    )
}

/// Single check — sent (Coolicons / CC Attribution).
pub fn check_sent<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/check.svg"),
        color,
        size,
    )
}

/// Double check — delivered/read (Solar Icons / CC Attribution).
pub fn check_read<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/check-read.svg"),
        color,
        size,
    )
}

/// Download / save glyph (Solar Bold Icons / CC Attribution).
pub fn download_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/download.svg"),
        color,
        size,
    )
}

/// Image / media glyph (Solar Icons / CC Attribution).
pub fn image_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/image-square.svg"),
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
        svg_handle!("../assets/icons/mic.svg"),
        color,
        size,
    )
}

/// Play triangle — Cursus original (CC0).
pub fn play_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/play.svg"),
        color,
        size,
    )
}

/// Stop square — Cursus original (CC0).
pub fn stop_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/stop.svg"),
        color,
        size,
    )
}

/// Cancel / close — Interface Line Icons / kreevstudio (CC Attribution via SVG Repo).
pub fn cancel_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/cancel.svg"),
        color,
        size,
    )
}

/// Trash / marked-deleted — Wave Oval Interface Icons / Arthur Kazais (Public Domain via SVG Repo).
pub fn trash_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/trash-delete.svg"),
        color,
        size,
    )
}

/// Copy to clipboard — Solar Linear Icons / Solar Icons (CC Attribution via SVG Repo).
pub fn copy_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/copy.svg"),
        color,
        size,
    )
}

/// Edit / pencil — Generica Tiny Interface Icons / Vlad Cristea (CC Attribution via SVG Repo).
pub fn edit_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/edit-3.svg"),
        color,
        size,
    )
}

/// Parchment scroll for the group welcome message — Appic Marketing (MIT,
/// via allsvgicons.com).
pub fn scroll_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/scripture-scroll.svg"),
        color,
        size,
    )
}

/// Save check in the text editor sheet (Coolicons / CC Attribution).
pub fn check_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/check.svg"),
        color,
        size,
    )
}

/// Reply — Solar Outline Icons / Solar Icons (CC Attribution via SVG Repo).
pub fn reply_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/reply.svg"),
        color,
        size,
    )
}

/// Forward — Solar Outline Icons / Solar Icons (CC Attribution via SVG Repo).
pub fn forward_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/forward.svg"),
        color,
        size,
    )
}

/// Info circle — Solar Linear Icons / Solar Icons (CC Attribution via SVG Repo).
pub fn info_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/info-circle.svg"),
        color,
        size,
    )
}

/// Select multiple — Coolicons Line Oval Icons / krystonschwarze (CC Attribution via SVG Repo).
pub fn select_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/select-multiple.svg"),
        color,
        size,
    )
}

/// Leave / exit session — Gitlab Interface Icons / Gitlab (MIT via SVG Repo).
pub fn leave_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/session-leave.svg"),
        color,
        size,
    )
}

/// Clear all — Keyicons Interface Icons / Elnodos (CC Attribution via SVG Repo).
pub fn clear_all_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/clear-all.svg"),
        color,
        size,
    )
}

/// Mute notifications — Element Plus Line Interface Icons / element-plus (MIT via SVG Repo).
pub fn mute_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/mute-notification.svg"),
        color,
        size,
    )
}

/// Group preferences / tools — Element Plus Line Interface Icons / element-plus (MIT).
pub fn preferences_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/preferences.svg"),
        color,
        size,
    )
}

/// Favorite star — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn star_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/star.svg"),
        color,
        size,
    )
}

/// Disappearing-messages timer — Gentlecons Interface Icons (CC Attribution via SVG Repo).
pub fn timer_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/timer.svg"),
        color,
        size,
    )
}

/// Direct messages — Framework7 Line Icons / framework7io (MIT via SVG Repo).
pub fn paperplane_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/paperplane.svg"),
        color,
        size,
    )
}

/// SimpleX links — Solar Broken Line Icons (CC Attribution via SVG Repo).
/// Padlock — Orchid Line Interface Icons (MIT), app lock screen / Lock button.
pub fn lock_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/lock.svg"),
        color,
        size,
    )
}

pub fn shield_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/shield-check.svg"),
        color,
        size,
    )
}

pub fn link_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/link-minimalistic-2.svg"),
        color,
        size,
    )
}

/// Recent history — Solar Linear Icons (CC Attribution via SVG Repo).
pub fn history_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/history.svg"),
        color,
        size,
    )
}

/// Review members before join — Iconship Interface Icons / Giovana (CC Attribution via SVG Repo).
pub fn review_file_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/review-file.svg"),
        color,
        size,
    )
}

/// Message reactions — Classjob Ff Game Icons / xivapi (MIT via SVG Repo).
pub fn reaction_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/reaction-emoji-add.svg"),
        color,
        size,
    )
}

/// Peer / contact allow indicator — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn user_alt_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/user-alt-1.svg"),
        color,
        size,
    )
}

/// Phone / calls — Solar Linear Icons (CC BY via SVG Repo); same asset as nav Calls.
pub fn phone_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/calls.svg"),
        color,
        size,
    )
}

/// Edit / set profile picture — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn image_pen_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/image-pen.svg"),
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

/// Default group avatar disk — Jtb Variety Oval Icons / jtblabs (MIT via SVG Repo).
pub fn group_avatar_disk<'a>(size: f32) -> Element<'a, Message> {
    avatar_icon_disk(
        svg_handle!("../assets/icons/user-group.svg"),
        size,
        theme::t().text_secondary,
    )
}

/// Default contact avatar disk — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn contact_avatar_disk<'a>(size: f32) -> Element<'a, Message> {
    avatar_icon_disk(
        svg_handle!("../assets/icons/user-alt-1.svg"),
        size,
        theme::t().text_secondary,
    )
}

/// Default channel avatar disk — CFPB Design System (Public Domain via SVG Repo).
pub fn channel_avatar_disk<'a>(size: f32) -> Element<'a, Message> {
    avatar_icon_disk(
        svg_handle!("../assets/icons/channels.svg"),
        size,
        theme::t().text_secondary,
    )
}

/// Circular disk filled with a tinted SVG (default avatars / placeholders).
pub fn avatar_icon_disk<'a>(
    handle: svg::Handle,
    size: f32,
    color: Color,
) -> Element<'a, Message> {
    let glyph = (size * 0.52).clamp(14.0, 36.0);
    container(tinted(handle, color, glyph))
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
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

/// Group / people — Jtb Variety Oval Icons / jtblabs (MIT via SVG Repo).
pub fn user_group_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/user-group.svg"),
        color,
        size,
    )
}

/// Contact request — Dazzle Line Icons / Dazzle UI (CC Attribution via SVG Repo).
pub fn user_question_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/user-question-alt-1.svg"),
        color,
        size,
    )
}

fn handle_for(kind: NavIcon) -> svg::Handle {
    match kind {
        NavIcon::Chats => svg_handle!("../assets/icons/chats.svg"),
        NavIcon::Channels => svg_handle!("../assets/icons/channels.svg"),
        NavIcon::Calls => svg_handle!("../assets/icons/calls.svg"),
        NavIcon::Settings => svg_handle!("../assets/icons/settings.svg"),
    }
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

/// Profile / member chrome — every glyph is the same optical size as
/// `settings.svg` (24×24, stroke 1.5). `ContentFit::Contain` into a fixed
/// glyph box so 800px / 1024 viewBox assets cannot look bigger than settings.
pub fn profile_tool_icon<'a>(
    handle: svg::Handle,
    hit: f32,
    idle: Color,
    hot: Color,
    hover_ok: bool,
) -> Element<'a, Message> {
    const GLYPH: f32 = 20.0;
    let icon: Svg<'a, iced::Theme> = svg(handle)
        .width(Length::Fixed(GLYPH))
        .height(Length::Fixed(GLYPH))
        .content_fit(ContentFit::Contain)
        .style(move |_, status| svg::Style {
            color: Some(match status {
                svg::Status::Hovered if hover_ok => hot,
                _ => idle,
            }),
        });
    container(icon)
        .width(hit)
        .height(hit)
        .center_x(hit)
        .center_y(hit)
        .into()
}

pub fn plus_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    composer_tool_icon(
        svg_handle!("../assets/icons/plus.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn mic_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    composer_tool_icon(
        svg_handle!("../assets/icons/mic.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

/// Header chrome tools — same optical size / stroke as the main-page settings gear.
pub fn back_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/back.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn phone_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/calls.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn star_tool<'a>(hit: f32, active: bool, hover_ok: bool) -> Element<'a, Message> {
    let idle = if active {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    profile_tool_icon(
        svg_handle!("../assets/icons/star.svg"),
        hit,
        idle,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn mute_tool<'a>(hit: f32, active: bool, hover_ok: bool) -> Element<'a, Message> {
    let idle = if active {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    profile_tool_icon(
        svg_handle!("../assets/icons/mute-notification.svg"),
        hit,
        idle,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn settings_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/settings.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn admin_tool<'a>(hit: f32, active: bool, hover_ok: bool) -> Element<'a, Message> {
    let idle = if active {
        theme::t().accent
    } else {
        theme::t().text_secondary
    };
    profile_tool_icon(
        svg_handle!("../assets/icons/admin.svg"),
        hit,
        idle,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn user_member_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/user-member.svg"),
        color,
        size,
    )
}

/// Role-button glyph matching the member's current privilege.
pub fn role_tool<'a>(role: &str, hit: f32, hover_ok: bool) -> Element<'a, Message> {
    let handle = match role.to_ascii_lowercase().as_str() {
        "observer" => svg_handle!("../assets/icons/eye-observer.svg"),
        "moderator" => svg_handle!("../assets/icons/shield-check.svg"),
        "admin" => svg_handle!("../assets/icons/admin-role.svg"),
        "owner" => svg_handle!("../assets/icons/owner.svg"),
        _ => svg_handle!("../assets/icons/user-member.svg"),
    };
    profile_tool_icon(
        handle,
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn actions_menu_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/actions-new-window.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn user_plus_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/user-plus-alt-1.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn pending_status_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/system-pending-line.svg"),
        color,
        size,
    )
}

pub fn observer_role_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/eye-observer.svg"),
        color,
        size,
    )
}

pub fn member_role_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    user_member_icon(size, color)
}

pub fn moderator_role_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/shield-check.svg"),
        color,
        size,
    )
}

pub fn admin_role_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/admin-role.svg"),
        color,
        size,
    )
}

pub fn owner_role_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/owner.svg"),
        color,
        size,
    )
}

pub fn role_icon<'a>(role: &str, size: f32, color: Color) -> Element<'a, Message> {
    match role {
        "observer" => observer_role_icon(size, color),
        "moderator" => moderator_role_icon(size, color),
        "admin" => admin_role_icon(size, color),
        "owner" => owner_role_icon(size, color),
        _ => member_role_icon(size, color),
    }
}

pub fn block_prohibited_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/block-prohibited.svg"),
        color,
        size,
    )
}

pub fn remove_profile_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/remove-profile.svg"),
        color,
        size,
    )
}

pub fn send_tool<'a>(hit: f32, active: bool, hover_ok: bool) -> Element<'a, Message> {
    let handle =
        svg_handle!("../assets/icons/send.svg");
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

/// Paste-from-clipboard — Gentlecons (Konstantin Filatov, CC BY), same chrome as other icon-only tools.
pub fn paste_tool<'a>(hit: f32, hover_ok: bool) -> Element<'a, Message> {
    profile_tool_icon(
        svg_handle!("../assets/icons/paste.svg"),
        hit,
        theme::t().text_secondary,
        theme::t().text_primary,
        hover_ok,
    )
}

pub fn user_plus_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/user-plus-alt-1.svg"),
        color,
        size,
    )
}

/// Chevron pointing right (navigation rows) — chevron-down rotated −90°.
pub fn chevron_right<'a>(size: f32, color: Color) -> Element<'a, Message> {
    let icon: Svg<'a, iced::Theme> = svg(svg_handle!("../assets/icons/chevron-down.svg"))
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .rotation(iced::Radians(-std::f32::consts::FRAC_PI_2))
    .style(move |_, _| svg::Style { color: Some(color) });
    container(icon)
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .into()
}

/// Plain tinted gear (no hover states) for icon discs.
pub fn settings_icon_plain<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/settings.svg"),
        color,
        size,
    )
}

/// “My address” — Dazzle Line Icons address-card (CC BY).
pub fn address_card_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/address-card.svg"),
        color,
        size,
    )
}

/// “One-time invite” — Phosphor repeat-once-fill (MIT).
pub fn repeat_once_icon<'a>(size: f32, color: Color) -> Element<'a, Message> {
    tinted(
        svg_handle!("../assets/icons/repeat-once.svg"),
        color,
        size,
    )
}
