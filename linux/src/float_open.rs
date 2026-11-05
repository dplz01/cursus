//! Open files / media in a **floating** window on tiling compositors.
//!
//! Stacking desktops (GNOME, KDE, Xfce, …) already open handlers as floating
//! windows, so this only does work where windows would otherwise tile:
//!
//! - **Hyprland**: `hyprctl dispatch exec [float; …] <cmd>` — Hyprland applies
//!   the rule to the window of the launched process (or its children).
//! - **sway / i3**: spawn normally, then watch the tree for a *new* window that
//!   belongs to the launched process tree (or whose app id matches the default
//!   handler's desktop id, for D-Bus–activated apps) and float that one window
//!   with `[con_id=…] floating enable`. No persistent `for_window` rules are
//!   added, so the user's own launches are never affected.
//!
//! Everything is best effort: if the compositor tools are missing or nothing
//! matches, the file still opens (tiled).

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compositor {
    Hyprland,
    Sway,
    I3,
    Other,
}

fn detect() -> Compositor {
    let has = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    if has("HYPRLAND_INSTANCE_SIGNATURE") {
        Compositor::Hyprland
    } else if has("SWAYSOCK") {
        Compositor::Sway
    } else if has("I3SOCK") {
        Compositor::I3
    } else {
        Compositor::Other
    }
}

/// POSIX single-quote for the Hyprland `exec` string (it runs via `sh -c`).
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn quiet(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

/// Launch `program args…` so its window floats when the compositor allows it.
/// `hint_app_id`: expected app id / class of the window (e.g. `mpv`) if known.
pub fn spawn_floating(program: &str, args: &[&str], hint_app_id: Option<String>) -> std::io::Result<()> {
    match detect() {
        Compositor::Hyprland => {
            let cmd = std::iter::once(program)
                .chain(args.iter().copied())
                .map(sh_quote)
                .collect::<Vec<_>>()
                .join(" ");
            let ok = quiet(Command::new("hyprctl").args([
                "dispatch",
                "exec",
                &format!("[float; size 70% 75%; center] {cmd}"),
            ]))
            .status()
            .is_ok_and(|s| s.success());
            if ok {
                return Ok(());
            }
            quiet(Command::new(program).args(args)).spawn().map(|_| ())
        }
        c @ (Compositor::Sway | Compositor::I3) => {
            let msg = if c == Compositor::Sway { "swaymsg" } else { "i3-msg" };
            let before = window_ids(msg);
            let child = quiet(Command::new(program).args(args)).spawn()?;
            if let Some(before) = before {
                let root_pid = child.id();
                std::thread::spawn(move || {
                    float_new_window(msg, &before, root_pid, hint_app_id.as_deref());
                });
            }
            // Dropping `Child` does not kill it; the handler keeps running.
            Ok(())
        }
        Compositor::Other => quiet(Command::new(program).args(args)).spawn().map(|_| ()),
    }
}

/// Desktop id stem of the default handler for `path` (e.g. `org.gnome.Loupe`),
/// used to recognise D-Bus–activated windows that are not our descendants.
pub fn default_handler_id(path: &Path) -> Option<String> {
    let mime = Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let mime = String::from_utf8_lossy(&mime.stdout).trim().to_string();
    if mime.is_empty() {
        return None;
    }
    let desktop = Command::new("xdg-mime")
        .args(["query", "default", &mime])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let desktop = String::from_utf8_lossy(&desktop.stdout).trim().to_string();
    let stem = desktop.strip_suffix(".desktop").unwrap_or(&desktop).to_string();
    (!stem.is_empty()).then_some(stem)
}

fn get_tree(msg: &str) -> Option<serde_json::Value> {
    let out = Command::new(msg)
        .args(["-t", "get_tree"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Leaf windows: `(con_id, pid, app_id/class)`.
fn leaves(node: &serde_json::Value, out: &mut Vec<(i64, Option<u32>, String)>) {
    let kids = ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|k| node.get(*k).and_then(|v| v.as_array()))
        .flatten()
        .collect::<Vec<_>>();
    let is_window = node.get("pid").is_some() || node.get("window").is_some_and(|w| !w.is_null());
    if kids.is_empty() && is_window {
        if let Some(id) = node.get("id").and_then(|v| v.as_i64()) {
            let pid = node.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
            let app = node
                .get("app_id")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    node.get("window_properties")
                        .and_then(|w| w.get("class"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or_default()
                .to_string();
            out.push((id, pid, app));
        }
    }
    for k in kids {
        leaves(k, out);
    }
}

fn window_ids(msg: &str) -> Option<HashSet<i64>> {
    let tree = get_tree(msg)?;
    let mut all = Vec::new();
    leaves(&tree, &mut all);
    Some(all.into_iter().map(|(id, _, _)| id).collect())
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `pid (comm) state ppid …` — comm may contain spaces/parens.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

fn descends_from(mut pid: u32, root: u32) -> bool {
    for _ in 0..32 {
        if pid == root {
            return true;
        }
        match parent_pid(pid) {
            Some(p) if p > 1 && p != pid => pid = p,
            _ => return false,
        }
    }
    false
}

fn app_matches(app: &str, hint: &str) -> bool {
    if app.is_empty() || hint.is_empty() {
        return false;
    }
    let a = app.to_ascii_lowercase();
    let h = hint.to_ascii_lowercase();
    // Desktop ids are often reverse-DNS (`org.gnome.Loupe`) while the app id
    // may be just the last segment (`loupe`) or the full id.
    a == h || h.rsplit('.').next().is_some_and(|last| a == last)
}

fn float_new_window(msg: &str, before: &HashSet<i64>, root_pid: u32, hint: Option<&str>) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(150));
        let Some(tree) = get_tree(msg) else { return };
        let mut all = Vec::new();
        leaves(&tree, &mut all);
        let hit = all.into_iter().find(|(id, pid, app)| {
            !before.contains(id)
                && (pid.is_some_and(|p| descends_from(p, root_pid))
                    || hint.is_some_and(|h| app_matches(app, h)))
        });
        if let Some((id, _, _)) = hit {
            let _ = quiet(Command::new(msg).arg(format!(
                "[con_id={id}] floating enable, resize set width 70 ppt height 75 ppt, move position center"
            )))
            .status();
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
        assert_eq!(sh_quote("/x y/z.png"), "'/x y/z.png'");
    }

    #[test]
    fn app_id_matching() {
        assert!(app_matches("org.gnome.Loupe", "org.gnome.Loupe"));
        assert!(app_matches("loupe", "org.gnome.Loupe"));
        assert!(app_matches("mpv", "mpv"));
        assert!(!app_matches("firefox", "org.gnome.Loupe"));
        assert!(!app_matches("", "mpv"));
    }

    #[test]
    fn leaves_reads_sway_tree() {
        let tree = serde_json::json!({
            "id": 1, "nodes": [{
                "id": 2, "nodes": [
                    {"id": 10, "pid": 100, "app_id": "mpv", "nodes": [], "floating_nodes": []}
                ],
                "floating_nodes": [
                    {"id": 11, "pid": 101, "app_id": null,
                     "window_properties": {"class": "Feh"}, "nodes": [], "floating_nodes": []}
                ]
            }]
        });
        let mut out = Vec::new();
        leaves(&tree, &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.contains(&(10, Some(100), "mpv".into())));
        assert!(out.contains(&(11, Some(101), "Feh".into())));
    }
}
