//! Open files / media in a **floating** window on tiling compositors.
//!
//! Stacking desktops (GNOME, KDE, Xfce, …) already open handlers as floating
//! windows, so this only does work where windows would otherwise tile:
//!
//! Hyprland, sway and i3 all work the same way: spawn normally, then watch
//! for a *new* window that belongs to the launched process tree (or whose app
//! id matches the default handler's desktop id — handlers such as eog are
//! D-Bus–activated, so they are not our descendants and exec-time rules never
//! match them) and float, size and center that one window. No persistent
//! window rules are added, so the user's own launches are never affected.
//!
//! - **Hyprland ≥ 0.5x (Lua IPC)**: `hyprctl dispatch 'hl.dsp.window.float({…,
//!   window = "address:0x…"})'`, each call guarded by `hl.get_window(…)` so a
//!   vanished window can never fall back to the focused one. Older Hyprland
//!   gets the classic `setfloating` / `resizewindowpixel` / `centerwindow`.
//! - **sway / i3**: `[con_id=…] floating enable, resize …, move position center`.
//!
//! Matching, in order: a window of the launched process tree; a window whose
//! app id is one of the plausible handlers (xdg-mime / gio default for the
//! file type, the default file manager — single-instance managers such as
//! Nautilus / COSMIC Files open the window from their existing process);
//! finally the first new window that takes focus within the wait.
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

fn quiet(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

/// Launch `program args…` so its window floats when the compositor allows it.
/// `hints`: plausible app ids / classes of the window (e.g. `mpv`).
pub fn spawn_floating(program: &str, args: &[&str], hints: Vec<String>) -> std::io::Result<()> {
    match detect() {
        Compositor::Hyprland => {
            let before = hypr_clients().map(|c| c.into_iter().map(|w| w.id).collect::<HashSet<_>>());
            let child = quiet(Command::new(program).args(args)).spawn()?;
            if let Some(before) = before {
                let root_pid = child.id();
                std::thread::spawn(move || {
                    if let Some(addr) = wait_new_window(&before, root_pid, &hints, || {
                        hypr_clients()
                    }) {
                        hypr_float(&addr);
                    }
                });
            }
            Ok(())
        }
        c @ (Compositor::Sway | Compositor::I3) => {
            let msg = if c == Compositor::Sway { "swaymsg" } else { "i3-msg" };
            let before = window_ids(msg);
            let child = quiet(Command::new(program).args(args)).spawn()?;
            if let Some(before) = before {
                let root_pid = child.id();
                std::thread::spawn(move || {
                    float_new_window(msg, &before, root_pid, &hints);
                });
            }
            // Dropping `Child` does not kill it; the handler keeps running.
            Ok(())
        }
        Compositor::Other => quiet(Command::new(program).args(args)).spawn().map(|_| ()),
    }
}

/// Float the file-chooser dialog titled `title` that is about to open.
///
/// rfd talks to the XDG desktop portal, so the dialog window belongs to the
/// portal process (`xdg-desktop-portal-gnome` / `-gtk` / `-kde` / cosmic),
/// not to Cursus. Call this right before showing the dialog: it watches for a
/// new window with that title, or from a portal app, and floats it centred.
pub fn float_next_dialog(title: &str) {
    let title = title.to_string();
    match detect() {
        Compositor::Hyprland => {
            let Some(before) = hypr_clients().map(|c| c.into_iter().map(|w| w.id).collect::<HashSet<_>>()) else {
                return;
            };
            std::thread::spawn(move || {
                if let Some(addr) = wait_dialog(&before, &title, hypr_clients) {
                    hypr_float_sized(&addr, 0.60, 0.70);
                }
            });
        }
        c @ (Compositor::Sway | Compositor::I3) => {
            let msg = if c == Compositor::Sway { "swaymsg" } else { "i3-msg" };
            let Some(before) = window_ids(msg) else { return };
            std::thread::spawn(move || {
                let list = || {
                    let tree = get_tree(msg)?;
                    let mut all = Vec::new();
                    leaves(&tree, &mut all);
                    Some(all)
                };
                if let Some(id) = wait_dialog(&before, &title, list) {
                    let _ = quiet(Command::new(msg).arg(format!(
                        "[con_id={id}] floating enable, resize set width 60 ppt height 70 ppt, move position center"
                    )))
                    .status();
                }
            });
        }
        Compositor::Other => {}
    }
}

fn is_portal_app(app: &str) -> bool {
    let a = app.to_ascii_lowercase();
    a.contains("xdg-desktop-portal") || a.contains("portal")
}

fn pick_dialog<Id: Eq + std::hash::Hash + Clone>(all: &[Win<Id>], before: &HashSet<Id>, title: &str) -> Option<Id> {
    let fresh: Vec<&Win<Id>> = all.iter().filter(|w| !before.contains(&w.id)).collect();
    fresh
        .iter()
        .find(|w| w.title == title)
        .or_else(|| fresh.iter().find(|w| is_portal_app(&w.app)))
        .map(|w| w.id.clone())
}

/// Dialogs may take a moment (portal start-up); wait up to 15s.
fn wait_dialog<Id: Eq + std::hash::Hash + Clone>(
    before: &HashSet<Id>,
    title: &str,
    list: impl Fn() -> Option<Vec<Win<Id>>>,
) -> Option<Id> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(120));
        if let Some(id) = pick_dialog(&list()?, before, title) {
            return Some(id);
        }
    }
    None
}

fn stdout_of(cmd: &mut Command) -> Option<String> {
    let out = cmd.stderr(Stdio::null()).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn desktop_stem(desktop: &str) -> String {
    desktop.trim().strip_suffix(".desktop").unwrap_or(desktop.trim()).to_string()
}

/// Plausible app ids for the window that will show `path`: the xdg-mime and
/// gio defaults for its type, and the default file manager (what xdg-open
/// falls back to for archives / unknown types). Used to recognise
/// D-Bus–activated windows that are not our descendants.
pub fn handler_ids(path: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    let mime = stdout_of(Command::new("xdg-mime").args(["query", "filetype"]).arg(path));
    if let Some(mime) = mime.as_deref() {
        if let Some(d) = stdout_of(Command::new("xdg-mime").args(["query", "default", mime])) {
            ids.push(desktop_stem(&d));
        }
        // `gio mime TYPE` → “Default application for “TYPE”: org.gnome.Nautilus.desktop”
        if let Some(out) = stdout_of(Command::new("gio").args(["mime", mime])) {
            if let Some(d) = out
                .lines()
                .next()
                .and_then(|l| l.rsplit(':').next())
                .filter(|d| d.trim().ends_with(".desktop"))
            {
                ids.push(desktop_stem(d));
            }
        }
    }
    if let Some(d) = stdout_of(Command::new("xdg-mime").args(["query", "default", "inode/directory"])) {
        ids.push(desktop_stem(&d));
    }
    ids.retain(|s| !s.is_empty());
    ids.dedup();
    ids
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

/// Leaf windows of a sway / i3 tree.
fn leaves(node: &serde_json::Value, out: &mut Vec<Win<i64>>) {
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
            let focused = node.get("focused").and_then(|v| v.as_bool()).unwrap_or(false);
            let title = node.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            out.push(Win { id, pid, app, focused, title });
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
    Some(all.into_iter().map(|w| w.id).collect())
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

/// A top-level window as seen by the compositor IPC.
#[derive(Debug, Clone, PartialEq)]
struct Win<Id> {
    id: Id,
    pid: Option<u32>,
    app: String,
    /// Has keyboard focus (new windows normally get it on tiling WMs).
    focused: bool,
    /// Window title (dialogs are matched by it).
    title: String,
}

/// Poll `list` until a window that was not in `before` appears and belongs to
/// the launched process tree, matches a handler id, or — as a last resort —
/// is new *and focused* (8s timeout).
fn wait_new_window<Id: Eq + std::hash::Hash + Clone>(
    before: &HashSet<Id>,
    root_pid: u32,
    hints: &[String],
    list: impl Fn() -> Option<Vec<Win<Id>>>,
) -> Option<Id> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let own_pid = std::process::id();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(150));
        let all = list()?;
        if let Some(id) = pick_new_window(&all, before, root_pid, own_pid, hints) {
            return Some(id);
        }
    }
    None
}

fn pick_new_window<Id: Eq + std::hash::Hash + Clone>(
    all: &[Win<Id>],
    before: &HashSet<Id>,
    root_pid: u32,
    own_pid: u32,
    hints: &[String],
) -> Option<Id> {
    let fresh: Vec<&Win<Id>> = all
        .iter()
        .filter(|w| !before.contains(&w.id) && w.pid != Some(own_pid))
        .collect();
    fresh
        .iter()
        .find(|w| w.pid.is_some_and(|p| descends_from(p, root_pid)))
        .or_else(|| fresh.iter().find(|w| hints.iter().any(|h| app_matches(&w.app, h))))
        .or_else(|| fresh.iter().find(|w| w.focused))
        .map(|w| w.id.clone())
}

fn float_new_window(msg: &str, before: &HashSet<i64>, root_pid: u32, hints: &[String]) {
    let list = || {
        let tree = get_tree(msg)?;
        let mut all = Vec::new();
        leaves(&tree, &mut all);
        Some(all)
    };
    if let Some(id) = wait_new_window(before, root_pid, hints, list) {
        let _ = quiet(Command::new(msg).arg(format!(
            "[con_id={id}] floating enable, resize set width 70 ppt height 75 ppt, move position center"
        )))
        .status();
    }
}

/// Hyprland's control socket (what `hyprctl` talks to). Speaking it directly
/// works without the `hyprctl` binary — e.g. inside the Flatpak, once the user
/// grants the socket (`flatpak override --filesystem=xdg-run/hypr`).
fn hypr_socket() -> Option<std::path::PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let mut candidates = Vec::new();
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR") {
        candidates.push(Path::new(&run).join("hypr").join(&sig).join(".socket.sock"));
    }
    // Hyprland < 0.40 kept it under /tmp.
    candidates.push(Path::new("/tmp/hypr").join(&sig).join(".socket.sock"));
    candidates.into_iter().find(|p| p.exists())
}

/// One request/reply on the Hyprland socket (`j/clients`, `dispatch …`,
/// `[[BATCH]]…`). `None` when Hyprland is unreachable.
fn hypr_request(request: &str) -> Option<String> {
    use std::io::{Read, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(hypr_socket()?).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// Flatpak on Hyprland without the compositor socket: viewers open tiled.
/// Returns the one-time hint shown to the user. Granting the socket is the
/// user's call — it also lets Cursus run commands through Hyprland.
pub fn sandbox_float_hint() -> Option<&'static str> {
    let flatpak = std::env::var_os("FLATPAK_ID").is_some();
    (flatpak && detect() == Compositor::Hyprland && hypr_socket().is_none()).then_some(
        "Opened tiled. To float viewers, allow Cursus to talk to Hyprland: \
         flatpak override --user --filesystem=xdg-run/hypr:ro chat.cursus.Cursus",
    )
}

fn hypr_json(what: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&hypr_request(&format!("j/{what}"))?).ok()
}

fn hypr_clients() -> Option<Vec<Win<String>>> {
    let clients = hypr_json("clients")?;
    let active = hypr_json("activewindow")
        .and_then(|w| w.get("address").and_then(|a| a.as_str()).map(str::to_string));
    Some(
        clients
            .as_array()?
            .iter()
            .filter_map(|c| {
                let id = c.get("address")?.as_str()?.to_string();
                Some(Win {
                    title: c.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    focused: active.as_deref() == Some(id.as_str()),
                    id,
                    pid: c.get("pid").and_then(|p| p.as_i64()).filter(|p| *p > 0).map(|p| p as u32),
                    app: c
                        .get("class")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .or_else(|| c.get("initialClass").and_then(|v| v.as_str()))
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect(),
    )
}

/// `fw` × `fh` of the focused monitor in logical px.
fn hypr_target_size(fw: f64, fh: f64) -> (i64, i64) {
    let mon = hypr_json("monitors").and_then(|m| {
        m.as_array()?
            .iter()
            .find(|x| x.get("focused").and_then(|f| f.as_bool()) == Some(true))
            .cloned()
    });
    let (w, h, scale) = mon
        .map(|m| {
            (
                m.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0),
                m.get("height").and_then(|v| v.as_f64()).unwrap_or(1080.0),
                m.get("scale").and_then(|v| v.as_f64()).filter(|s| *s > 0.0).unwrap_or(1.0),
            )
        })
        .unwrap_or((1920.0, 1080.0, 1.0));
    ((w / scale * fw).round() as i64, (h / scale * fh).round() as i64)
}

/// Lua dispatch that only runs if `addr` still exists (never the focused window).
fn hypr_lua_guarded(addr: &str, dispatcher: &str) -> String {
    format!(
        "hl.get_window(\"address:{addr}\") and {dispatcher} or error(\"cursus: window gone\")"
    )
}

fn hypr_float(addr: &str) {
    hypr_float_sized(addr, 0.70, 0.75);
}

fn hypr_float_sized(addr: &str, fw: f64, fh: f64) {
    let (w, h) = hypr_target_size(fw, fh);
    let sel = format!("address:{addr}");
    let steps = [
        format!("hl.dsp.window.float({{ action = \"enable\", window = \"{sel}\" }})"),
        format!("hl.dsp.window.resize({{ x = {w}, y = {h}, window = \"{sel}\" }})"),
        format!("hl.dsp.window.center({{ window = \"{sel}\" }})"),
    ];
    let mut lua_ok = true;
    for step in &steps {
        let reply = hypr_request(&format!("dispatch {}", hypr_lua_guarded(addr, step))).unwrap_or_default();
        if reply.contains("window gone") {
            return;
        }
        if reply.contains("error") && reply.contains("expected") {
            // Pre-Lua Hyprland: the string is not Lua at all.
            lua_ok = false;
            break;
        }
    }
    if !lua_ok {
        let _ = hypr_request(&format!(
            "[[BATCH]]dispatch setfloating {sel}; dispatch resizewindowpixel exact {w} {h},{sel}; dispatch centerwindow"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: u32, pid: u32, app: &str, focused: bool) -> Win<u32> {
        Win { id, pid: Some(pid), app: app.into(), focused, title: String::new() }
    }

    #[test]
    fn dialog_matches_title_or_portal() {
        let before: HashSet<u32> = [1].into();
        let mut d = win(2, 50, "xdg-desktop-portal-gnome", false);
        d.title = "Attach file".into();
        let other = win(3, 51, "firefox", true);
        assert_eq!(pick_dialog(&[other.clone(), d.clone()], &before, "Attach file"), Some(2));
        d.title = "Something".into();
        assert_eq!(pick_dialog(&[other, d], &before, "Attach file"), Some(2));
    }

    #[test]
    fn picks_file_manager_by_hint_over_focus() {
        let before: HashSet<u32> = [1].into();
        let all = vec![
            win(1, 10, "firefox", false),
            win(2, 999_999, "something", true),
            win(3, 999_998, "org.gnome.Nautilus", false),
        ];
        let hints = vec!["org.gnome.Nautilus".to_string()];
        assert_eq!(pick_new_window(&all, &before, 4_000_000, 5, &hints), Some(3));
    }

    #[test]
    fn falls_back_to_focused_new_window_but_never_old_or_own() {
        let before: HashSet<u32> = [1].into();
        let all = vec![win(1, 10, "firefox", true), win(2, 77, "cursus", true), win(3, 88, "x", true)];
        assert_eq!(pick_new_window(&all, &before, 4_000_000, 77, &[]), Some(3));
        let none = vec![win(1, 10, "firefox", true), win(3, 88, "x", false)];
        assert_eq!(pick_new_window(&none, &before, 4_000_000, 77, &[]), None);
    }

    /// Live: the socket client sees the same windows as `hyprctl`.
    #[test]
    #[ignore]
    fn hypr_socket_matches_hyprctl() {
        let ours = hypr_clients().expect("Hyprland socket reachable");
        let cli = std::process::Command::new("hyprctl").args(["clients", "-j"]).output().unwrap();
        let theirs: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
        assert_eq!(ours.len(), theirs.as_array().unwrap().len());
        assert!(hypr_json("monitors").is_some_and(|m| m.as_array().is_some_and(|a| !a.is_empty())));
    }

    #[test]
    fn hypr_guard_wraps_dispatch() {
        let lua = hypr_lua_guarded("0xabc", "hl.dsp.window.center({ window = \"address:0xabc\" })");
        assert!(lua.starts_with("hl.get_window(\"address:0xabc\") and hl.dsp.window.center("));
        assert!(lua.ends_with("or error(\"cursus: window gone\")"));
    }

    #[test]
    fn app_id_matching() {
        assert!(app_matches("org.gnome.Loupe", "org.gnome.Loupe"));
        assert!(app_matches("loupe", "org.gnome.Loupe"));
        assert!(app_matches("eog", "org.gnome.eog"));
        assert!(app_matches("com.system76.CosmicViewer", "com.system76.CosmicViewer"));
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
        assert!(out.iter().any(|w| w.id == 10 && w.pid == Some(100) && w.app == "mpv"));
        assert!(out.iter().any(|w| w.id == 11 && w.pid == Some(101) && w.app == "Feh"));
    }
}

#[cfg(test)]
mod live {
    /// Manual: `cargo test -p cursus-linux live_open_png -- --ignored` on a tiling desktop.
    #[test]
    #[ignore]
    fn live_open_png() {
        let path = std::env::var("CURSUS_FLOAT_TEST").expect("set CURSUS_FLOAT_TEST to an image path");
        let p = std::path::Path::new(&path);
        let handler = super::handler_ids(p);
        eprintln!("handlers: {handler:?}");
        super::spawn_floating("xdg-open", &[&path], handler).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(6));
    }
}

#[cfg(test)]
mod live_dialog {
    /// Manual: `cargo test -p cursus-linux live_dialog -- --ignored` (close it by hand or via IPC).
    #[test]
    #[ignore]
    fn live_dialog() {
        super::float_next_dialog("Attach file");
        let picked = rfd::FileDialog::new().set_title("Attach file").pick_file();
        eprintln!("picked: {picked:?}");
    }
}
