//! Wipe decrypted media when Cursus is terminated rather than quit.
//!
//! `QuitApp` already wipes the plaintext cache. Logout / shutdown send
//! SIGTERM or SIGHUP, Ctrl+C sends SIGINT, and a panic unwinds past the event
//! loop — none of those reach `QuitApp`. Handlers here catch them.
//!
//! Signal *handlers* (unlike blocked signal masks) are reset to the default
//! when a child process `exec`s, so recorders / players we spawn keep their
//! normal SIGINT/SIGTERM behaviour. The handler only writes the signal number
//! to a pipe (async-signal-safe); a watcher thread does the wipe, restores
//! the default action and re-raises so the process ends with the usual status.

use std::sync::atomic::{AtomicI32, Ordering};

static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(sig: libc::c_int) {
    let fd = PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = sig as u8;
        // SAFETY: write(2) is async-signal-safe; one byte from a valid stack slot.
        unsafe {
            libc::write(fd, (&byte as *const u8).cast(), 1);
        }
    }
}

/// Install the termination handlers and the panic hook (call once, early).
pub fn install() {
    let mut fds = [0 as libc::c_int; 2];
    // CLOEXEC: spawned helpers must not inherit our pipe.
    // SAFETY: valid out-array of two fds.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return;
    }
    let read_fd = fds[0];
    PIPE_WRITE.store(fds[1], Ordering::Relaxed);
    let spawned = std::thread::Builder::new()
        .name("exit-wipe".into())
        .spawn(move || {
            let mut byte = 0u8;
            loop {
                // SAFETY: reading one byte into a valid buffer from our pipe.
                let n = unsafe { libc::read(read_fd, (&mut byte as *mut u8).cast(), 1) };
                if n == 1 {
                    break;
                }
                if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            let sig = byte as libc::c_int;
            cursus_core::wipe_plaintext_cache();
            // SAFETY: restore the default action and re-deliver the signal.
            unsafe {
                libc::signal(sig, libc::SIG_DFL);
                libc::raise(sig);
            }
        });
    if spawned.is_err() {
        return;
    }
    for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        // SAFETY: zeroed sigaction + handler pointer; standard installation.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Into cursus.log too: launched from a launcher / Flatpak, stderr is
        // lost and a crash would leave no trace.
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>");
        tracing::error!(
            thread = name,
            "panic: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        // Only the event-loop thread takes the app down; a panic in a tokio
        // worker / blocking task is caught there, and wiping the plaintext
        // cache under the still-running UI would break open media.
        if name == "main" {
            cursus_core::wipe_plaintext_cache();
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    /// A forked child with seeded cache files receives SIGTERM: the files
    /// must be gone and the child must still die *by* SIGTERM.
    #[test]
    fn sigterm_wipes_plaintext_cache() {
        let root = std::env::temp_dir().join(format!("cursus-exitwipe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let runtime = root.join("run");
        let data = root.join("data");
        std::fs::create_dir_all(&runtime).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        let ram_file = runtime.join("cursus/plain/photo.jpg");
        let disk_file = data.join("cursus/tmp/plain/video.mp4");

        // SAFETY: the child only touches files, installs handlers and raises.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            // SAFETY: single-threaded child after fork.
            unsafe {
                std::env::set_var("XDG_RUNTIME_DIR", &runtime);
                std::env::set_var("XDG_DATA_HOME", &data);
            }
            for f in [&ram_file, &disk_file] {
                std::fs::create_dir_all(f.parent().unwrap()).unwrap();
                std::fs::write(f, b"plaintext").unwrap();
            }
            super::install();
            unsafe {
                libc::raise(libc::SIGTERM);
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
            unsafe { libc::_exit(42) };
        }
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0) };
        assert!(libc::WIFSIGNALED(status), "child must die by signal, status {status}");
        assert_eq!(libc::WTERMSIG(status), libc::SIGTERM);
        assert!(!ram_file.exists(), "RAM cache not wiped");
        assert!(!disk_file.exists(), "disk cache not wiped");
        let _ = std::fs::remove_dir_all(root);
    }
}
