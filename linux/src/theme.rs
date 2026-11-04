//! Color tokens from `docs/DESIGN_SYSTEM.md` (light + dark).

use std::sync::atomic::{AtomicBool, Ordering};

use iced::theme::Palette;
use iced::widget::{button, container, text_editor, text_input};
use iced::{Background, Border, Color};

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
    pub accent_hover: Color,
    pub accent_press: Color,
    pub danger_hover: Color,
    pub danger_press: Color,
    pub disabled_fill: Color,
    /// Quiet chrome hover (secondary buttons, idle nav).
    pub bg_hover: Color,
}

impl Tokens {
    pub fn dark() -> Self {
        let accent = Color::from_rgb(0.357, 0.549, 1.0); // #5B8CFF
        Self {
            bg_app: Color::from_rgb(0.071, 0.078, 0.102),       // #12141A
            bg_sidebar: Color::from_rgb(0.094, 0.106, 0.133),   // #181B22
            bg_list: Color::from_rgb(0.110, 0.125, 0.157),      // #1C2028
            bg_panel: Color::from_rgb(0.110, 0.125, 0.157),
            bg_elevated: Color::from_rgb(0.149, 0.169, 0.212),  // #262B36
            text_primary: Color::from_rgb(0.949, 0.953, 0.961), // #F2F3F5
            text_secondary: Color::from_rgb(0.627, 0.651, 0.702), // #A0A6B3
            text_muted: Color::from_rgb(0.431, 0.459, 0.522),   // #6E7585
            accent,
            accent_on: Color::from_rgb(0.043, 0.063, 0.125), // #0B1020
            accent_soft: Color::from_rgba(0.357, 0.549, 1.0, 0.18),
            accent_soft_hover: Color::from_rgba(0.357, 0.549, 1.0, 0.30),
            accent_soft_press: Color::from_rgba(0.357, 0.549, 1.0, 0.40),
            border: Color::from_rgb(0.180, 0.204, 0.251), // #2E3440
            danger: Color::from_rgb(1.0, 0.420, 0.420),   // #FF6B6B
            success: Color::from_rgb(0.420, 0.796, 0.467), // #6BCB77
            always: Color::from_rgb(0.925, 0.757, 0.310), // #ECD04F gold
            accent_hover: Color::from_rgb(0.42, 0.60, 1.0),
            accent_press: Color::from_rgb(0.30, 0.48, 0.92),
            danger_hover: Color::from_rgb(0.85, 0.28, 0.28),
            danger_press: Color::from_rgb(0.70, 0.20, 0.20),
            disabled_fill: Color::from_rgb(0.25, 0.30, 0.40),
            bg_hover: Color::from_rgb(0.149, 0.169, 0.212), // elevated
        }
    }

    pub fn light() -> Self {
        let accent = Color::from_rgb(0.184, 0.435, 0.929); // #2F6FED
        Self {
            bg_app: Color::from_rgb(0.957, 0.961, 0.969),       // #F4F5F7
            bg_sidebar: Color::from_rgb(0.922, 0.929, 0.941),   // #EBEDF0
            bg_list: Color::WHITE,                              // #FFFFFF
            bg_panel: Color::WHITE,
            bg_elevated: Color::WHITE,
            text_primary: Color::from_rgb(0.071, 0.078, 0.102), // #12141A
            text_secondary: Color::from_rgb(0.361, 0.388, 0.439), // #5C6370
            text_muted: Color::from_rgb(0.545, 0.569, 0.612),   // #8B919C
            accent,
            accent_on: Color::WHITE,
            accent_soft: Color::from_rgba(0.184, 0.435, 0.929, 0.16),
            accent_soft_hover: Color::from_rgba(0.184, 0.435, 0.929, 0.26),
            accent_soft_press: Color::from_rgba(0.184, 0.435, 0.929, 0.36),
            border: Color::from_rgb(0.843, 0.859, 0.886), // #D7DBE2
            danger: Color::from_rgb(0.776, 0.157, 0.157), // #C62828
            success: Color::from_rgb(0.180, 0.490, 0.196), // #2E7D32
            always: Color::from_rgb(0.710, 0.545, 0.060), // #B58B0F gold
            accent_hover: Color::from_rgb(0.145, 0.365, 0.820),
            accent_press: Color::from_rgb(0.110, 0.300, 0.720),
            danger_hover: Color::from_rgb(0.690, 0.110, 0.110),
            danger_press: Color::from_rgb(0.580, 0.080, 0.080),
            disabled_fill: Color::from_rgb(0.820, 0.835, 0.855),
            bg_hover: Color::from_rgb(0.886, 0.898, 0.918), // #E2E5EA
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

/// Active tokens (follows [`set_light_active`]).
pub fn t() -> Tokens {
    Tokens::for_light(is_light_active())
}

/// Detect whether the desktop prefers a dark color scheme.
pub fn system_prefers_dark() -> bool {
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if s.contains("prefer-dark") {
                return true;
            }
            if s.contains("prefer-light") {
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
        }
    }
    // Prefer dark when unset — matches the original Cursus default.
    true
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

pub fn list_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_| {
        let c = t();
        container::Style {
            background: Some(Background::Color(c.bg_list)),
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        }
    }
}

pub fn sidebar_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_| {
        let c = t();
        container::Style {
            background: Some(Background::Color(c.bg_sidebar)),
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 0.0.into(),
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
            _ => c.bg_sidebar,
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

/// Round icon-only control (mute / favorite in contact header).
pub fn round_icon(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
            button::Status::Pressed => Some(Background::Color(c.accent_soft)),
            _ => Some(Background::Color(c.bg_elevated)),
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Selectable TTL / preference chip.
pub fn choice_chip(
    selected: bool,
    hover_ok: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = if selected {
            Some(Background::Color(match status {
                button::Status::Pressed => c.accent_soft_press,
                _ => c.accent_soft,
            }))
        } else {
            match status {
                button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
                button::Status::Pressed => Some(Background::Color(c.accent_soft)),
                _ => Some(Background::Color(c.bg_elevated)),
            }
        };
        button::Style {
            background,
            text_color: if selected { c.accent } else { c.text_primary },
            border: Border {
                color: if selected { c.accent } else { c.border },
                width: 1.0,
                radius: 10.0.into(),
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
                color: c.border,
                width: 1.0,
                radius: 14.0.into(),
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

/// Soft elevated tile for secondary chrome actions (clear / leave).
pub fn quiet_tile(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let c = t();
        let background = match status {
            button::Status::Hovered if hover_ok => Some(Background::Color(c.bg_hover)),
            button::Status::Pressed => Some(Background::Color(c.accent_soft)),
            _ => Some(Background::Color(c.bg_elevated)),
        };
        button::Style {
            background,
            text_color: c.text_primary,
            border: Border {
                color: c.border,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn icon_action(hover_ok: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
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
                radius: 12.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// Borderless composer tool (+ / mic) — fill stays clear; SVG carries the color.
pub fn composer_tool() -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    |_, _| {
        let c = t();
        button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
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

/// Send control: accent when the composer has text, otherwise a quiet tool.
pub fn composer_send(
    active: bool,
    hover_ok: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |theme, status| {
        if active {
            icon_action(hover_ok)(theme, status)
        } else {
            composer_tool()(theme, status)
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
            background: Background::Color(c.bg_sidebar),
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
            background: Background::Color(c.bg_sidebar),
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
