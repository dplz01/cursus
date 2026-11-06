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

/// Inactivity before the app lock asks for the password again. Hiding the
/// window to the tray always locks (when the lock is on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockAfter {
    /// As soon as the window loses focus.
    Immediately,
    #[default]
    Seconds30,
    Minute1,
    Minutes5,
    Minutes15,
}

impl LockAfter {
    pub const ALL: [Self; 5] = [
        Self::Immediately,
        Self::Seconds30,
        Self::Minute1,
        Self::Minutes5,
        Self::Minutes15,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Immediately => "Immediately",
            Self::Seconds30 => "After 30 seconds",
            Self::Minute1 => "After 1 minute",
            Self::Minutes5 => "After 5 minutes",
            Self::Minutes15 => "After 15 minutes",
        }
    }

    /// Idle seconds before locking (`0` = on focus loss).
    pub fn secs(self) -> u64 {
        match self {
            Self::Immediately => 0,
            Self::Seconds30 => 30,
            Self::Minute1 => 60,
            Self::Minutes5 => 300,
            Self::Minutes15 => 900,
        }
    }
}

impl std::fmt::Display for LockAfter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
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
    /// Notifications show only the chat name and "New message", never the
    /// text (notification daemons may keep a history). On by default.
    pub hide_notification_text: bool,
    /// Ask before downloading from file servers the sender chose (they would
    /// see your IP address). On by default, like the official app.
    pub protect_ip: bool,
    /// Ask for the password again after inactivity / when hidden to tray.
    pub app_lock: bool,
    /// When the app lock kicks in.
    pub lock_after: LockAfter,
    /// Last-message text under each chat in the list (official default on).
    pub chat_previews: bool,
    /// Blur photos and video posters in chats until tapped.
    pub blur_media: bool,
    /// Download images, voice and short videos automatically (official
    /// "Auto-accept images", default on).
    pub auto_download: bool,
    /// Remove tracking parameters (`utm_*`, `fbclid`…) before opening links.
    pub clean_links: bool,
    /// Unlock at start with the password stored in the system keyring.
    pub keyring_unlock: bool,
    /// Prefs schema version — lets new privacy defaults reach old files.
    /// Missing in files written before versioning → 0.
    #[serde(default)]
    pub version: u32,
}

/// Current [`DesktopPrefs::version`].
const PREFS_VERSION: u32 = 1;

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self {
            notifications: true,
            tray: true,
            // Hidden in the tray the app keeps running with decrypted media
            // cached; quitting is the safe default.
            close_to_tray: false,
            theme: ThemePreference::System,
            text_size: TextSizePreference::Medium,
            auto_accept_address: false,
            hide_notification_text: true,
            protect_ip: true,
            app_lock: false,
            lock_after: LockAfter::default(),
            chat_previews: true,
            blur_media: false,
            auto_download: true,
            clean_links: true,
            keyring_unlock: false,
            version: PREFS_VERSION,
        }
    }
}

fn prefs_path() -> PathBuf {
    data_dir().join("desktop.json")
}

pub fn load_prefs() -> DesktopPrefs {
    let path = prefs_path();
    match fs::read_to_string(&path) {
        Ok(raw) => {
            let mut prefs: DesktopPrefs = serde_json::from_str(&raw).unwrap_or_default();
            // Files written before v1 stored the old `close_to_tray: true`
            // default, not a choice; the new privacy defaults apply once.
            if prefs.version < 1 {
                prefs.close_to_tray = false;
                prefs.hide_notification_text = true;
                prefs.protect_ip = true;
                prefs.version = PREFS_VERSION;
                save_prefs(&prefs);
            }
            prefs
        }
        Err(_) => DesktopPrefs::default(),
    }
}

pub fn save_prefs(prefs: &DesktopPrefs) {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(prefs) {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
        {
            let _ = f.write_all(raw.as_bytes());
        }
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

/// App logo for the tray (decoded from the embedded 64 px PNG).
fn tray_rgba_icon() -> Icon {
    let img = image::load_from_memory(crate::desktop_entry::logo_png(64))
        .expect("embedded logo")
        .to_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("tray icon rgba")
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
        // Fixed ids: the menu event handler is installed once per process and
        // must still match after the tray is recreated.
        let show = MenuItem::with_id("cursus-show", "Show Cursus", true, None);
        let hide = MenuItem::with_id("cursus-hide", "Hide", true, None);
        let quit = MenuItem::with_id("cursus-quit", "Quit", true, None);
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
    fn old_prefs_get_new_privacy_defaults() {
        let raw = r#"{"notifications":true,"tray":true,"close_to_tray":true,"theme":"system"}"#;
        let prefs: DesktopPrefs = serde_json::from_str(raw).unwrap();
        assert_eq!(prefs.version, 0, "pre-versioning files must migrate");
        assert!(prefs.hide_notification_text && prefs.protect_ip);
        let fresh = DesktopPrefs::default();
        assert!(!fresh.close_to_tray && fresh.hide_notification_text && fresh.protect_ip);
        assert_eq!(fresh.version, PREFS_VERSION);
    }

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

