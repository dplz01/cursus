//! Color tokens from `docs/DESIGN_SYSTEM.md` (light + dark).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use iced::theme::Palette;
use iced::widget::{button, container, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Vector};

/// Process-wide active palette — set from `App::view` / preference changes so
/// style closures and view helpers stay token-driven without threading `App`
/// through every icon helper.
static LIGHT_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_light_active(light: bool) {
    LIGHT_ACTIVE.store(light, Ordering::Relaxed);
}

pub fn is_light_active() -> bool {
    LIGHT_ACTIVE.load(Ordering::Relaxed)
}

/// Resolved design tokens for the active appearance.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub bg_app: Color,
    pub bg_sidebar: Color,
    pub bg_list: Color,
    pub bg_panel: Color,
    pub bg_elevated: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_on: Color,
    pub accent_soft: Color,
    pub accent_soft_hover: Color,
    pub accent_soft_press: Color,
    pub border: Color,
    pub danger: Color,
    pub success: Color,
    /// Peer allows `always` (asymmetric) — warm gold, readable on dark/light.
    pub always: Color,
    /// Bright yellow fill for things that need attention (pending links).
    pub attention: Color,
    /// Unplayed waveform bars (played ones are `accent`); `_out` on the
    /// outgoing bubble, which is already tinted.
    pub voice_rest: Color,
    pub voice_rest_out: Color,
    pub accent_hover: Color,
    pub accent_press: Color,
    pub danger_hover: Color,
    pub danger_press: Color,
    pub disabled_fill: Color,
    /// Quiet chrome hover (secondary buttons, idle nav).
    pub bg_hover: Color,
    /// Text fields, composer pill and quiet buttons.
    pub bg_field: Color,
    /// Incoming / outgoing message bubbles.
    pub bubble_in: Color,
    pub bubble_out: Color,
    /// Time + secondary text inside outgoing bubbles.
    pub bubble_out_meta: Color,
    /// Delivered tick inside outgoing bubbles (accent may vanish on them).
    pub bubble_out_tick: Color,
}

impl Tokens {
    pub fn dark() -> Self {
        let accent = Color::from_rgb(0.357, 0.549, 1.0); // #5B8CFF
        Self {
            bg_app: rgb(0x0E1015),
            // Rail + list read as one sidebar; conversation sits a step darker
            // so bubbles and cards carry the hierarchy (no borders needed).
            bg_sidebar: rgb(0x15181E),
            bg_list: rgb(0x15181E),
            bg_panel: rgb(0x0E1015),
            bg_elevated: rgb(0x1C2028),
            text_primary: rgb(0xF2F3F5),
            text_secondary: rgb(0xA0A6B3),
            text_muted: rgb(0x6E7585),
            accent,
            accent_on: rgb(0x0B1020),
            accent_soft: Color::from_rgba(0.357, 0.549, 1.0, 0.16),
            accent_soft_hover: Color::from_rgba(0.357, 0.549, 1.0, 0.26),
            accent_soft_press: Color::from_rgba(0.357, 0.549, 1.0, 0.36),
            border: rgb(0x252A33),
            danger: rgb(0xFF6B6B),
            success: rgb(0x6BCB77),
            always: Color::from_rgb(0.925, 0.757, 0.310), // gold
            attention: rgb(0xF5C518),
            voice_rest: rgb(0x6E7585),
            voice_rest_out: rgb(0x7F93BC),
            accent_hover: Color::from_rgb(0.42, 0.60, 1.0),
            accent_press: Color::from_rgb(0.30, 0.48, 0.92),
            danger_hover: Color::from_rgb(0.85, 0.28, 0.28),
            danger_press: Color::from_rgb(0.70, 0.20, 0.20),
            disabled_fill: Color::from_rgb(0.25, 0.30, 0.40),
            bg_hover: rgb(0x20242D),
            bg_field: rgb(0x111318),
            bubble_in: rgb(0x1C2028),
            bubble_out: rgb(0x243A63),
            bubble_out_meta: rgb(0xA7B6D6),
            bubble_out_tick: rgb(0x9CBBFF),
        }
    }

    pub fn light() -> Self {
        let accent = Color::from_rgb(0.184, 0.435, 0.929); // #2F6FED
        Self {
            bg_app: rgb(0xEEF0F4),
            bg_sidebar: Color::WHITE,
            bg_list: Color::WHITE,
            bg_panel: rgb(0xEEF0F4),
            bg_elevated: Color::WHITE,
            text_primary: rgb(0x12141A),
            text_secondary: rgb(0x5C6370),
            text_muted: rgb(0x8B919C),
            accent,
            accent_on: Color::WHITE,
            accent_soft: Color::from_rgba(0.184, 0.435, 0.929, 0.12),
            accent_soft_hover: Color::from_rgba(0.184, 0.435, 0.929, 0.20),
            accent_soft_press: Color::from_rgba(0.184, 0.435, 0.929, 0.30),
            border: rgb(0xDDE1E7),
            danger: rgb(0xC62828),
            success: rgb(0x2E7D32),
            always: Color::from_rgb(0.710, 0.545, 0.060), // gold
            attention: rgb(0xF5B800),
            // Soft accent instead of grey: calmer next to the blue play button.
            voice_rest: Color::from_rgba(0.184, 0.435, 0.929, 0.32),
            voice_rest_out: Color::from_rgba(0.184, 0.435, 0.929, 0.45),
            accent_hover: Color::from_rgb(0.145, 0.365, 0.820),
            accent_press: Color::from_rgb(0.110, 0.300, 0.720),
            danger_hover: Color::from_rgb(0.690, 0.110, 0.110),
            danger_press: Color::from_rgb(0.580, 0.080, 0.080),
            disabled_fill: Color::from_rgb(0.820, 0.835, 0.855),
            bg_hover: rgb(0xEDEFF3),
            bg_field: rgb(0xF2F4F7),
            bubble_in: Color::WHITE,
            bubble_out: rgb(0xD8E5FF),
            bubble_out_meta: rgb(0x5A6C8F),
            bubble_out_tick: accent,
        }
    }

    pub fn for_light(light: bool) -> Self {
        if light {
            Self::light()
        } else {
            Self::dark()
        }
    }

    pub fn iced_palette(self) -> Palette {
        Palette {
            background: self.bg_app,
            text: self.text_primary,
            primary: self.accent,
            success: self.success,
            warning: Color::from_rgb(0.90, 0.70, 0.20),
            danger: self.danger,
        }
    }

    pub fn iced_theme_name(self) -> String {
        if is_light_active() {
            "Cursus Light".into()
        } else {
            "Cursus Dark".into()
        }
    }
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb(
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    )
}

/// Active tokens (follows [`set_light_active`]).
pub fn t() -> Tokens {
    Tokens::for_light(is_light_active())
}

const SYSTEM_THEME_TTL: Duration = Duration::from_secs(30);
static SYSTEM_THEME_CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Detect whether the desktop prefers a dark color scheme.
///
/// Order: xdg-desktop-portal (Hyprland / non-GNOME) → gsettings color-scheme →
/// gtk-theme name → dark fallback only when nothing reports a preference.
///
/// `view()` / `iced_theme()` call this on every rebuild. Detection spawns
/// `gdbus` / `gsettings` (~10–20 ms, longer if D-Bus has to wait for a portal),
/// so only the very first call — or the first after the user switches theme —
/// runs it inline. Afterwards the cached answer is returned immediately and a
/// background thread refreshes it once the TTL has passed; the next frame
/// picks the new value up.
pub fn system_prefers_dark() -> bool {
    let cached = SYSTEM_THEME_CACHE.lock().ok().and_then(|g| *g);
    match cached {
        Some((at, dark)) => {
            if at.elapsed() >= SYSTEM_THEME_TTL
                && !SYSTEM_THEME_REFRESHING.swap(true, std::sync::atomic::Ordering::AcqRel)
            {
                std::thread::spawn(|| {
                    let dark = system_prefers_dark_uncached();
                    if let Ok(mut guard) = SYSTEM_THEME_CACHE.lock() {
                        *guard = Some((Instant::now(), dark));
                    }
                    SYSTEM_THEME_REFRESHING.store(false, std::sync::atomic::Ordering::Release);
                });
            }
            dark
        }
        None => {
            let dark = system_prefers_dark_uncached();
            if let Ok(mut guard) = SYSTEM_THEME_CACHE.lock() {
                *guard = Some((Instant::now(), dark));
            }
            dark
        }
    }
}

/// Set while a background refresh is running (at most one at a time).
static SYSTEM_THEME_REFRESHING: AtomicBool = AtomicBool::new(false);

/// Drop the cached system preference (call after the user cycles theme).
pub fn invalidate_system_theme_cache() {
    if let Ok(mut guard) = SYSTEM_THEME_CACHE.lock() {
        *guard = None;
    }
}

fn system_prefers_dark_uncached() -> bool {
    if let Some(dark) = portal_prefers_dark() {
        return dark;
    }
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if s.contains("prefer-dark") {
                return true;
            }
            if s.contains("prefer-light") || s.contains("default") {
                // GNOME "default" follows light chrome on modern desktops.
                return false;
            }
        }
    }
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if s.contains("dark") {
                return true;
            }
            if !s.trim().is_empty() && s.contains('\'') {
                // Named light theme (no "dark" in the name).
                return false;
            }
        }
    }
    // Last resort when no desktop setting is readable.
    true
}

/// Read `org.freedesktop.appearance color-scheme` via the settings portal.
/// `0` = no preference, `1` = prefer dark, `2` = prefer light.
fn portal_prefers_dark() -> Option<bool> {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.ReadOne",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    // Typical: `(<uint32 1>,)` or `(uint32 2,)`.
    if s.contains("uint32 1") || s.contains("uint32 1>") {
        return Some(true);
    }
    if s.contains("uint32 2") || s.contains("uint32 2>") {
        return Some(false);
    }
    None
}

pub fn panel_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_| {
        let c = t();
        container::Style {
            background: Some(Background::Color(c.bg_panel)),
            ..container::Style::default()
        }
    }
}

pub fn sidebar_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_| {
        let c = t();
        container::Style {
            background: Some(Background::Color(c.bg_sidebar)),
            ..container::Style::default()
        }
    }
}

/// Rounded floating section nav that sits over the list page.
pub fn floating_nav_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_| {
        let c = t();
        let shadow_a = if is_light_active() { 0.16 } else { 0.55 };
        container::Style {
            background: Some(Background::Color(c.bg_elevated)),
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 18.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, shadow_a),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 18.0,
            },
            ..container::Style::default()
        }
    }
}

/// `hover_ok` is false after a touch lift: iced leaves the cursor on the last
/// touch point, which would otherwise keep buttons stuck in `Hovered`.
pub fn nav_active(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Pressed => Some(Background::Color(c.accent_soft_press)),
            button::Status::Hovered if hover_ok => {
                Some(Background::Color(c.accent_soft_hover))
            }
            _ => Some(Background::Color(c.accent_soft)),
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn nav_idle(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Pressed => Some(Background::Color(c.accent_soft)),
            button::Status::Hovered if hover_ok => {
                Some(Background::Color(c.bg_hover))
            }
            _ => Some(Background::Color(Color::TRANSPARENT)),
        };
        button::Style {
            background,
            text_color: c.text_secondary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn primary_action(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => c.accent_hover,
            button::Status::Pressed => c.accent_press,
            button::Status::Disabled => c.disabled_fill,
            _ => c.accent,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: c.accent_on,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 10.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn secondary_action(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => c.bg_hover,
            button::Status::Pressed => c.bg_hover,
            _ => c.bg_field,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: c.text_primary,
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Yellow square beside "+" (pending one-time links): same shape as the
/// "+" button, filled with the `attention` yellow — something to look at.
pub fn attention_action(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let mut fill = c.attention;
        if matches!(status, button::Status::Pressed) || (hover_ok && matches!(status, button::Status::Hovered)) {
            fill = Color { a: 0.85, ..fill };
        }
        button::Style {
            background: Some(Background::Color(fill)),
            text_color: on_attention(),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 10.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Icon / text colour on the `attention` yellow (dark in both themes).
pub fn on_attention() -> Color {
    rgb(0x1F1A05)
}

/// Selectable list row (settings categories): `accent.soft` when selected.
pub fn list_row(selected: bool, hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = if selected {
            Some(Background::Color(c.accent_soft))
        } else {
            match status {
                button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
                button::Status::Pressed => Some(Background::Color(c.accent_soft)),
                _ => None,
            }
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn menu_item(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
            button::Status::Pressed => Some(Background::Color(c.accent_soft)),
            _ => Some(Background::Color(Color::TRANSPARENT)),
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn reaction_chip(
    selected: bool,
    hover_ok: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = if selected {
            Some(Background::Color(match status {
                button::Status::Pressed => c.accent_press,
                button::Status::Hovered if hover_ok => c.accent_hover,
                _ => c.accent,
            }))
        } else {
            match status {
                button::Status::Hovered if hover_ok => {
                    Some(Background::Color(c.accent_soft))
                }
                button::Status::Pressed => Some(Background::Color(c.accent_soft_press)),
                _ => Some(Background::Color(c.bg_elevated)),
            }
        };
        button::Style {
            background,
            text_color: if selected {
                c.accent_on
            } else {
                c.text_primary
            },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn emoji_suggest_row(
    selected: bool,
    hover_ok: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = if selected {
            Some(Background::Color(match status {
                button::Status::Pressed => c.accent_press,
                _ => c.accent,
            }))
        } else {
            match status {
                button::Status::Hovered if hover_ok => {
                    Some(Background::Color(c.accent_soft))
                }
                button::Status::Pressed => Some(Background::Color(c.accent_soft_press)),
                _ => Some(Background::Color(Color::TRANSPARENT)),
            }
        };
        button::Style {
            background,
            text_color: if selected {
                c.accent_on
            } else {
                c.text_primary
            },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn danger_action(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => c.danger_hover,
            button::Status::Pressed => c.danger_press,
            _ => c.danger,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: Color::WHITE,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 10.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Quiet danger tile — outlined / soft fill for destructive chrome (delete).
pub fn danger_quiet(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let (background, border_color) = match status {
            button::Status::Hovered if hover_ok => (
                Some(Background::Color(Color {
                    a: 0.18,
                    ..c.danger
                })),
                c.danger,
            ),
            button::Status::Pressed => (
                Some(Background::Color(Color {
                    a: 0.28,
                    ..c.danger
                })),
                c.danger,
            ),
            _ => (
                Some(Background::Color(c.bg_elevated)),
                Color {
                    a: 0.55,
                    ..c.danger
                },
            ),
        };
        button::Style {
            background,
            text_color: c.danger,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Chat-action tile (clear / leave / delete / support / invite): same 12px
/// rounded shape as the old bordered tiles but no border or fill at rest — the shape
/// only appears on hover / press.
pub fn ghost_tile(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    ghost_tile_r(hover_ok, 12.0)
}

/// [`ghost_tile`] with its own corner radius (16 = flush with a card edge).
pub fn ghost_tile_r(hover_ok: bool, radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
            button::Status::Pressed => Some(Background::Color(c.accent_soft)),
            _ => None,
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Destructive variant of [`ghost_tile`] — red label/icon, soft red on hover.
pub fn ghost_danger(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    ghost_danger_r(hover_ok, 12.0)
}

/// [`ghost_danger`] with its own corner radius.
pub fn ghost_danger_r(hover_ok: bool, radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => Some(Background::Color(Color { a: 0.14, ..c.danger })),
            button::Status::Pressed => Some(Background::Color(Color { a: 0.24, ..c.danger })),
            _ => None,
        };
        button::Style {
            background,
            text_color: c.danger,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Inline text field with the action-tile radius (12). Pair with ≥10px
/// horizontal padding so the rounded corners never clip the placeholder.
pub fn rounded_field() -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    |_, status| {
        let c = t();
        let border_color = match status {
            text_input::Status::Focused { .. } => c.accent,
            _ => c.border,
        };
        text_input::Style {
            background: Background::Color(c.bg_field),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            icon: c.text_muted,
            placeholder: c.text_secondary,
            value: c.text_primary,
            selection: c.accent_soft,
        }
    }
}

/// Multi-line twin of [`rounded_field`] (editor sheet).
pub fn rounded_editor() -> impl Fn(&iced::Theme, text_editor::Status) -> text_editor::Style {
    |_, status| {
        let c = t();
        let border_color = match status {
            text_editor::Status::Focused { .. } => c.accent,
            _ => c.border,
        };
        text_editor::Style {
            background: Background::Color(c.bg_field),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            placeholder: c.text_secondary,
            value: c.text_primary,
            selection: c.accent_soft,
        }
    }
}

/// Blue link box (accent-soft fill, accent hairline) — shared links / call cards.
pub fn link_box(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => c.accent_soft_press,
            button::Status::Pressed => c.accent_soft_press,
            _ => c.accent_soft,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: c.accent,
            border: Border {
                color: c.accent,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn composer_input() -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    |_, status| {
        let c = t();
        let border_color = match status {
            text_input::Status::Focused { .. } => c.accent,
            _ => c.border,
        };
        text_input::Style {
            background: Background::Color(c.bg_field),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 10.0.into(),
            },
            icon: c.text_muted,
            placeholder: c.text_muted,
            value: c.text_primary,
            selection: c.accent_soft,
        }
    }
}

pub fn composer_editor() -> impl Fn(&iced::Theme, text_editor::Status) -> text_editor::Style {
    |_, _| {
        let c = t();
        text_editor::Style {
            // The surrounding pill paints the field.
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 10.0.into(),
            },
            placeholder: c.text_muted,
            value: c.text_primary,
            selection: c.accent_soft,
        }
    }
}
