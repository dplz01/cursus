//! Keep the device awake while a call rings or runs.
//!
//! Phones (GNOME Mobile, Phosh, Plasma Mobile) blank the screen and suspend
//! within seconds of inactivity; a suspended device drops the WebRTC session
//! and the audio tools. While an [`AwakeGuard`] lives we hold:
//!
//! 1. an `org.freedesktop.portal.Inhibit` (suspend + idle) — works inside the
//!    Flatpak sandbox and on every desktop with xdg-desktop-portal;
//! 2. otherwise a logind `sleep:idle` inhibitor (plain installs without a
//!    portal backend).
//!
//! Both are tied to our D-Bus connection / fd, so they vanish if Cursus dies.
//! The connection lives on a small thread: acquiring never blocks the UI, and
//! dropping the guard releases it.

use std::sync::mpsc;
use std::thread;

const REASON: &str = "Call in progress";
/// Portal flags: 4 = suspend, 8 = idle (screen blank / session idle).
const PORTAL_SUSPEND_IDLE: u32 = 4 | 8;

pub struct AwakeGuard {
    release: Option<mpsc::Sender<()>>,
}

impl AwakeGuard {
    pub fn acquire() -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let spawned = thread::Builder::new().name("call-inhibit".into()).spawn(move || {
            let held = hold_portal().or_else(|e| {
                tracing::debug!("call inhibit: portal unavailable ({e}); trying logind");
                hold_logind()
            });
            match held {
                Ok(held) => {
                    // Wait for release (or the guard being dropped).
                    let _ = rx.recv();
                    held.release();
                }
                Err(e) => tracing::warn!("call inhibit: device may sleep during the call ({e})"),
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("call inhibit: {e}");
        }
        Self { release: Some(tx) }
    }
}

impl Drop for AwakeGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.release.take() {
            let _ = tx.send(());
        }
    }
}

enum Held {
    Portal { conn: zbus::blocking::Connection, request: zbus::zvariant::OwnedObjectPath },
    /// logind keeps the inhibitor until this fd is closed.
    Logind(zbus::zvariant::OwnedFd),
}

impl Held {
    fn release(self) {
        match self {
            Held::Portal { conn, request } => {
                let _ = conn.call_method(
                    Some("org.freedesktop.portal.Desktop"),
                    &request,
                    Some("org.freedesktop.portal.Request"),
                    "Close",
                    &(),
                );
            }
            Held::Logind(fd) => drop(fd),
        }
    }
}

fn hold_portal() -> Result<Held, String> {
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedObjectPath, Value};
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("reason", Value::from(REASON));
    let reply = conn
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Inhibit"),
            "Inhibit",
            &("", PORTAL_SUSPEND_IDLE, options),
        )
        .map_err(|e| e.to_string())?;
    let request: OwnedObjectPath = reply.body().deserialize().map_err(|e| e.to_string())?;
    Ok(Held::Portal { conn, request })
}

fn hold_logind() -> Result<Held, String> {
    let conn = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("sleep:idle", "Cursus", REASON, "block"),
        )
        .map_err(|e| e.to_string())?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize().map_err(|e| e.to_string())?;
    Ok(Held::Logind(fd))
}

/// Live: needs a session bus with a portal and a system bus with logind.
/// While it runs, `systemd-inhibit --list` shows "Cursus … Call in progress".
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn live_each_path() {
        let ms = std::env::var("INHIBIT_HOLD_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
        let hold = std::time::Duration::from_millis(ms);
        match hold_portal() {
            Ok(h) => {
                if let Held::Portal { request, .. } = &h {
                    println!("portal: request {}", request.as_str());
                }
                std::thread::sleep(hold);
                h.release();
            }
            Err(e) => println!("portal: {e}"),
        }
        let h = hold_logind().expect("logind inhibitor");
        println!("logind: held");
        std::thread::sleep(hold);
        h.release();
    }
}
