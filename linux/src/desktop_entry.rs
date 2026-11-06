//! Launcher integration for installs that don't bring their own `.desktop`
//! file (AppImage, `cargo install`): Cursus registers itself in
//! `~/.local/share/applications` with its icon, pointing at the running
//! binary (or the AppImage file). Flatpak and distro packages ship their own
//! entry, so nothing is written for them.
//!
//! Runs on every start but only writes when something changed (first run,
//! AppImage moved, new version of the icon). `cursus --uninstall-desktop`
//! removes the entry and stops it from coming back.

use std::fs;
use std::path::{Path, PathBuf};

/// Desktop id for native installs: matches the Wayland app id `cursus`, so
/// docks and task switchers find the icon. (The Flatpak uses its own
/// `chat.cursus.Cursus`, so both can be installed side by side.)
const DESKTOP_ID: &str = "cursus";

const ICONS: &[(u32, &[u8])] = &[
    (16, include_bytes!("../assets/logo/cursus-16.png")),
    (24, include_bytes!("../assets/logo/cursus-24.png")),
    (32, include_bytes!("../assets/logo/cursus-32.png")),
    (48, include_bytes!("../assets/logo/cursus-48.png")),
    (64, include_bytes!("../assets/logo/cursus-64.png")),
    (128, include_bytes!("../assets/logo/cursus-128.png")),
    (256, include_bytes!("../assets/logo/cursus-256.png")),
    (512, include_bytes!("../assets/logo/cursus-512.png")),
];

/// App logo as PNG bytes (window and tray icons).
pub fn logo_png(size: u32) -> &'static [u8] {
    ICONS
        .iter()
        .find(|(s, _)| *s >= size)
        .or(ICONS.last())
        .map(|(_, b)| *b)
        .unwrap_or_default()
}

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

fn entry_path(data: &Path) -> PathBuf {
    data.join("applications").join(format!("{DESKTOP_ID}.desktop"))
}

fn icon_path(data: &Path, size: u32) -> PathBuf {
    data.join(format!("icons/hicolor/{size}x{size}/apps/{DESKTOP_ID}.png"))
}

/// User asked not to have a launcher entry.
fn opt_out_marker() -> PathBuf {
    cursus_core::data_dir().join("no-desktop-entry")
}

/// What the launcher should run: the AppImage file itself when inside one
/// (`$APPIMAGE`), else this binary.
fn launch_target() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

/// `Exec=` value: quoted per the Desktop Entry spec.
fn exec_quoted(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("\"");
    for c in raw.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn entry_text(target: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Cursus\n\
         GenericName=Private messenger\n\
         Comment=SimpleX Chat client for Linux desktops and phones\n\
         Exec={}\n\
         Icon={DESKTOP_ID}\n\
         Terminal=false\n\
         Categories=Network;InstantMessaging;Chat;\n\
         Keywords=SimpleX;chat;messenger;private;encrypted;\n\
         StartupNotify=true\n\
         StartupWMClass={DESKTOP_ID}\n\
         X-Purism-FormFactor=Workstation;Mobile;\n",
        exec_quoted(target)
    )
}

fn write_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<bool> {
    if fs::read(path).is_ok_and(|old| old == contents) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(true)
}

/// Create / refresh the launcher entry. `force` ignores the opt-out marker
/// (`--install-desktop`). Returns the entry path when something was written.
pub fn install(force: bool) -> std::io::Result<Option<PathBuf>> {
    // Flatpak exports its own entry; packages in /usr or /app ship theirs.
    if std::env::var_os("FLATPAK_ID").is_some() {
        return Ok(None);
    }
    let Some(target) = launch_target() else {
        return Ok(None);
    };
    let packaged = std::env::var_os("APPIMAGE").is_none()
        && (target.starts_with("/usr") || target.starts_with("/app"));
    if packaged {
        return Ok(None);
    }
    if force {
        let _ = fs::remove_file(opt_out_marker());
    } else if opt_out_marker().exists() {
        return Ok(None);
    }
    let Some(data) = data_home() else {
        return Ok(None);
    };
    let mut changed = false;
    for (size, png) in ICONS {
        changed |= write_if_changed(&icon_path(&data, *size), png)?;
    }
    let entry = entry_path(&data);
    changed |= write_if_changed(&entry, entry_text(&target).as_bytes())?;
    Ok(changed.then_some(entry))
}

/// Remove the entry and icons, and don't recreate them on later starts.
pub fn uninstall() -> std::io::Result<()> {
    if let Some(data) = data_home() {
        let _ = fs::remove_file(entry_path(&data));
        for (size, _) in ICONS {
            let _ = fs::remove_file(icon_path(&data, *size));
        }
    }
    let marker = opt_out_marker();
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(marker, b"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_paths_are_quoted() {
        assert_eq!(exec_quoted(Path::new("/home/me/Apps/Cursus.AppImage")), "\"/home/me/Apps/Cursus.AppImage\"");
        assert_eq!(exec_quoted(Path::new("/home/me/My \"Apps\"/c$")), "\"/home/me/My \\\"Apps\\\"/c\\$\"");
    }

    #[test]
    fn entry_points_at_target_with_icon() {
        let text = entry_text(Path::new("/opt/c/cursus"));
        assert!(text.contains("Exec=\"/opt/c/cursus\"\n"));
        assert!(text.contains("Icon=cursus\n"));
        assert!(text.contains("StartupWMClass=cursus\n"));
    }

    #[test]
    fn logo_sizes_round_up() {
        assert_eq!(logo_png(30).len(), ICONS[2].1.len());
        assert_eq!(logo_png(9999).len(), ICONS.last().unwrap().1.len());
    }
}
