//! Linux desktop notifications + optional system tray (KSNI / StatusNotifier).

use std::fs;
use std::path::PathBuf;

use cursus_core::data_dir;
use serde::{Deserialize, Serialize};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the desktop color-scheme (GNOME `color-scheme` / gtk-theme).
    #[default]
    System,
    Light,
    Dark,
}

impl std::fmt::Display for ThemePreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    pub fn resolve_light(self) -> bool {
        match self {
            Self::Light => true,
            Self::Dark => false,
            Self::System => !crate::theme::system_prefers_dark(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextSizePreference {
    ExtraSmall,
    Small,
    /// Between Small and Large; the app default.
    #[default]
    Medium,
    /// Former “Default” / mid-large.
    #[serde(alias = "default")]
    Large,
    ExtraLarge,
}

impl std::fmt::Display for TextSizePreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl TextSizePreference {
    pub const ALL: [Self; 5] = [
        Self::ExtraSmall,
        Self::Small,
        Self::Medium,
        Self::Large,
        Self::ExtraLarge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ExtraSmall => "Extra small",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
            Self::ExtraLarge => "Extra large",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::ExtraSmall => Self::Small,
            Self::Small => Self::Medium,
            Self::Medium => Self::Large,
            Self::Large => Self::ExtraLarge,
            Self::ExtraLarge => Self::ExtraSmall,
        }
    }

    /// Multiplier applied to layout density text sizes (title / body / meta).
    ///
    /// `Medium` (default) sits in the middle of the five steps.
    pub fn scale(self) -> f32 {
        match self {
            Self::ExtraSmall => 0.75,
            Self::Small => 0.875,
            Self::Medium => 0.96,
            Self::Large => 1.05,
            Self::ExtraLarge => 1.25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    /// Desktop notifications for inbound messages when the window is not focused
    /// on that chat.
    pub notifications: bool,
    /// Show a StatusNotifier / tray icon.
    pub tray: bool,
    /// Intercept window close and hide to tray instead of quitting.
    pub close_to_tray: bool,
    /// Appearance: system / light / dark (design-system tokens).
    pub theme: ThemePreference,
    /// App-wide UI text size (scales density title / body / meta).
    pub text_size: TextSizePreference,
    /// UI mirror of libsimplex address `autoAccept` (core is source of truth).
    pub auto_accept_address: bool,
}

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self {
            notifications: true,
            tray: true,
            close_to_tray: true,
            theme: ThemePreference::System,
            text_size: TextSizePreference::Medium,
            auto_accept_address: false,
        }
    }
}

fn prefs_path() -> PathBuf {
    data_dir().join("desktop.json")
}

pub fn load_prefs() -> DesktopPrefs {
    let path = prefs_path();
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => DesktopPrefs::default(),
    }
}

pub fn save_prefs(prefs: &DesktopPrefs) {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(prefs) {
        let _ = fs::write(path, raw);
    }
}

/// Fire a desktop notification (best-effort; never panics the UI).
///
/// `show()` is a synchronous D-Bus round trip (~20 ms, or a long timeout when
/// no notification daemon answers), so it runs on its own short-lived thread
/// instead of stalling `update()` once per incoming message.
pub fn notify_message(title: &str, body: &str) {
    let title = crate::ui::truncate_graphemes(title, 80);
    let body = crate::ui::truncate_graphemes(body, 160);
    std::thread::spawn(move || {
        if let Err(err) = notify_rust::Notification::new()
            .appname("Cursus")
            .summary(&title)
            .body(&body)
            .icon("mail-message-new")
            .timeout(notify_rust::Timeout::Milliseconds(6_000))
            .show()
        {
            tracing::debug!(%err, "desktop notification failed");
        }
    });
}

/// Simple 32×32 accent disc used as the tray glyph.
fn tray_rgba_icon() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let cx = (SIZE as f32) / 2.0 - 0.5;
    let cy = cx;
    let r_outer = 13.0;
    let r_inner = 5.5;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = ((y * SIZE + x) * 4) as usize;
            if dist <= r_outer {
                // Accent blue (#5B8CFF)
                rgba[i] = 0x5B;
                rgba[i + 1] = 0x8C;
                rgba[i + 2] = 0xFF;
                let edge = (r_outer - dist).clamp(0.0, 1.0);
                rgba[i + 3] = (255.0 * edge.max(0.35)).round() as u8;
                if dist <= r_inner {
                    rgba[i] = 0xF8;
                    rgba[i + 1] = 0xFA;
                    rgba[i + 2] = 0xFC;
                    rgba[i + 3] = 255;
                }
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("tray icon rgba")
}

pub struct AppTray {
    _tray: TrayIcon,
    pub show_id: tray_icon::menu::MenuId,
    pub hide_id: tray_icon::menu::MenuId,
    pub quit_id: tray_icon::menu::MenuId,
}

impl AppTray {
    pub fn try_new() -> Option<Self> {
        let menu = Menu::new();
        let show = MenuItem::new("Show Cursus", true, None);
        let hide = MenuItem::new("Hide", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let show_id = show.id().clone();
        let hide_id = hide.id().clone();
        let quit_id = quit.id().clone();
        if menu.append(&show).is_err()
            || menu.append(&hide).is_err()
            || menu.append(&PredefinedMenuItem::separator()).is_err()
            || menu.append(&quit).is_err()
        {
            tracing::warn!("tray menu build failed");
            return None;
        }
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Cursus")
            .with_icon(tray_rgba_icon())
            .build()
        {
            Ok(tray) => Some(Self {
                _tray: tray,
                show_id,
                hide_id,
                quit_id,
            }),
            Err(err) => {
                tracing::warn!(%err, "system tray unavailable");
                None
            }
        }
    }
}

impl AppTray {
    /// Remove the tray without blocking the UI thread.
    ///
    /// tray-icon's ksni `Drop` joins its menu thread and blocks on
    /// `handle.shutdown().wait()`; on some StatusNotifier hosts that never
    /// returns, which froze Cursus on Quit / when the tray was switched off.
    /// The icon is not `Send`, so instead of dropping elsewhere we mark it
    /// passive (hidden) and leak the handle — a one-off, bounded cost.
    pub fn dismiss(self) {
        let _ = self._tray.set_visible(false);
        std::mem::forget(self);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Hide,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_size_cycles_all_steps() {
        let mut p = TextSizePreference::ExtraSmall;
        p = p.cycle();
        assert_eq!(p, TextSizePreference::Small);
        p = p.cycle();
        assert_eq!(p, TextSizePreference::Medium);
        p = p.cycle();
        assert_eq!(p, TextSizePreference::Large);
        p = p.cycle();
        assert_eq!(p, TextSizePreference::ExtraLarge);
        p = p.cycle();
        assert_eq!(p, TextSizePreference::ExtraSmall);
    }

    #[test]
    fn text_size_scale_ordered() {
        assert!(TextSizePreference::ExtraSmall.scale() < TextSizePreference::Small.scale());
        assert!(TextSizePreference::Small.scale() < TextSizePreference::Medium.scale());
        assert!(TextSizePreference::Medium.scale() < TextSizePreference::Large.scale());
        assert!(TextSizePreference::Large.scale() < TextSizePreference::ExtraLarge.scale());
        // Medium renders distinct font sizes from both neighbours.
        let body = |t: TextSizePreference| (15.0 * t.scale()).round();
        assert!(body(TextSizePreference::Small) < body(TextSizePreference::Medium));
        assert!(body(TextSizePreference::Medium) < body(TextSizePreference::Large));
    }

    #[test]
    fn prefs_deserialize_without_text_size() {
        let raw = r#"{"notifications":true,"tray":true,"close_to_tray":true,"theme":"system","auto_accept_address":false}"#;
        let prefs: DesktopPrefs = serde_json::from_str(raw).unwrap();
        assert_eq!(prefs.text_size, TextSizePreference::Medium);
    }

    #[test]
    fn prefs_migrate_old_default_label() {
        let raw = r#"{"notifications":true,"tray":true,"close_to_tray":true,"theme":"system","text_size":"default","auto_accept_address":false}"#;
        let prefs: DesktopPrefs = serde_json::from_str(raw).unwrap();
        assert_eq!(prefs.text_size, TextSizePreference::Large);
    }
}

