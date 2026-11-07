//! One Cursus process per data directory.
//!
//! With "close hides to tray" the process keeps running (and keeps the
//! SQLCipher databases open) after its window closes. Launching Cursus again
//! used to start a second process that showed the unlock screen and then
//! failed with `ErrorBusy … database is locked`, whatever the password.
//!
//! Now the first process listens on `<data dir>/instance.sock`; a later launch
//! connects, asks it to show its window and exits.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const SHOW: &[u8] = b"show\n";

pub enum Instance {
    /// This process owns the data directory; keep the listener alive.
    Primary(UnixListener),
    /// Another process owns it and was asked to show its window.
    Forwarded,
}

pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join("instance.sock")
}

/// Become the primary instance for `dir`, or hand the launch to the one
/// already running. `None` when the socket can't be set up (no single-instance
/// guard then — never refuse to start over this).
pub fn acquire(dir: &Path) -> Option<Instance> {
    let path = socket_path(dir);
    let _ = std::fs::create_dir_all(dir);
    for _ in 0..2 {
        match UnixListener::bind(&path) {
            Ok(listener) => return Some(Instance::Primary(listener)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                if ask_to_show(&path) {
                    return Some(Instance::Forwarded);
                }
                // Left behind by a crash / kill: nobody is listening.
                let _ = std::fs::remove_file(&path);
            }
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "single instance: bind failed");
                return None;
            }
        }
    }
    None
}

fn ask_to_show(path: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(SHOW).is_err() {
        return false;
    }
    // The primary acks once it queued the request; a hung process doesn't.
    let mut ack = [0u8; 2];
    matches!(stream.read(&mut ack), Ok(n) if n > 0)
}

/// Where "show" requests go once the UI subscribed; requests that arrive
/// earlier are remembered and delivered on subscribe.
static SINK: Mutex<(Option<tokio::sync::mpsc::UnboundedSender<()>>, bool)> =
    Mutex::new((None, false));

fn deliver() {
    let mut guard = SINK.lock().unwrap_or_else(|e| e.into_inner());
    let sent = guard.0.as_ref().is_some_and(|tx| tx.send(()).is_ok());
    if !sent {
        guard.1 = true;
    }
}

/// Serve show requests until the process exits. The socket file stays behind
/// afterwards; the next launch finds nobody listening and replaces it.
pub fn serve(listener: UnixListener) {
    std::thread::Builder::new()
        .name("cursus-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 16];
                if matches!(stream.read(&mut buf), Ok(n) if buf[..n] == *SHOW) {
                    tracing::info!("single instance: another launch asked to show the window");
                    deliver();
                    let _ = stream.write_all(b"ok");
                }
            }
        })
        .ok();
}

/// Show requests for the UI (one `()` per later launch).
pub fn requests() -> tokio::sync::mpsc::UnboundedReceiver<()> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut guard = SINK.lock().unwrap_or_else(|e| e.into_inner());
    if std::mem::take(&mut guard.1) {
        let _ = tx.send(());
    }
    guard.0 = Some(tx);
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cursus-si-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn second_launch_is_forwarded_to_the_first() {
        let dir = temp_dir("fwd");
        let Some(Instance::Primary(listener)) = acquire(&dir) else {
            panic!("first launch must be primary");
        };
        let mut rx = requests();
        serve(listener);
        assert!(matches!(acquire(&dir), Some(Instance::Forwarded)));
        assert!(rx.blocking_recv().is_some(), "primary got the show request");
    }

    #[test]
    fn stale_socket_is_replaced() {
        let dir = temp_dir("stale");
        // A bound-then-dropped listener leaves the file with nobody behind it.
        drop(UnixListener::bind(socket_path(&dir)).unwrap());
        assert!(socket_path(&dir).exists());
        assert!(matches!(acquire(&dir), Some(Instance::Primary(_))));
    }
}
