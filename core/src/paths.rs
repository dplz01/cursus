use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Use `dir` as the data directory instead of the platform default. For
/// platforms where the app is told its private storage at run time (Android:
/// `Context.filesDir`). Call once, before anything reads [`data_dir`];
/// returns false if a directory was already set.
pub fn set_data_dir(dir: PathBuf) -> bool {
    DATA_DIR.set(dir).is_ok()
}

/// Data directory for Cursus: the one given to [`set_data_dir`], else XDG
/// (`~/.local/share/cursus` on Linux).
pub fn data_dir() -> PathBuf {
    if let Some(dir) = DATA_DIR.get() {
        return dir.clone();
    }
    // This crate's tests never touch the user's real profile.
    #[cfg(test)]
    return std::env::temp_dir().join(format!("cursus-test-data-{}", std::process::id()));
    #[cfg(not(test))]
    directories::ProjectDirs::from("chat", "Cursus", "cursus")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join("cursus-data"))
}

/// Database file prefix passed to `chat_migrate_init` (without `_chat.db` suffix).
pub fn db_prefix() -> PathBuf {
    data_dir().join("simplex_v1")
}

/// Directory for received files (`~/.local/share/cursus/files`).
pub fn files_dir() -> PathBuf {
    data_dir().join("files")
}

/// Temp directory on the same filesystem as [`files_dir`].
///
/// XFTP decrypts into the process temp dir then `rename`s into the files
/// folder. On Linux `/tmp` is often tmpfs, so a cross-device rename fails with
/// EXDEV and leaves an empty placeholder — keep temp next to the DB/files.
///
/// See `docs/architecture.md` § “XFTP receive + TMPDIR” for security notes.
pub fn temp_dir() -> PathBuf {
    data_dir().join("tmp")
}

/// Process-wide environment for libsimplex. Call first thing in `main`,
/// **before any thread exists**: `setenv` racing a `getenv` on another thread
/// (tokio, winit, tray, the Haskell RTS) is undefined behaviour and crashes
/// sporadically inside libc.
///
/// The agent decrypts XFTP downloads under getTemporaryDirectory(); pinning
/// TMPDIR keeps the following rename into files_dir on one filesystem (see
/// docs/architecture.md).
pub fn init_process_env() {
    let tmp = temp_dir();
    // SAFETY: documented precondition — still single-threaded.
    unsafe {
        std::env::set_var("TMPDIR", &tmp);
        std::env::set_var("TMP", &tmp);
        std::env::set_var("TEMP", &tmp);
    }
}

/// Create `path` (and its parents) for the owner only (`0700`).
pub(crate) fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path)?;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    Ok(())
}

/// Best-effort removal of everything inside `path` (the directory stays).
pub(crate) fn wipe_dir_contents(path: &Path) {
    for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
        let p = entry.path();
        // `symlink_metadata`: never follow a link out of our directory.
        if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Delete decrypted attachments, playback transcodes and unsent voice
/// recordings (`tmp/plain`, `tmp/play`, `tmp/rec`). Call when the session ends
/// or the app locks, so plaintext media never stays behind. XFTP staging is
/// left alone (may be mid-transfer); it is wiped on the next session open.
pub fn wipe_plaintext_cache() {
    for sub in ["plain", "play", "rec"] {
        wipe_dir_contents(&temp_dir().join(sub));
    }
    if let Some(ram) = runtime_cache_dir() {
        wipe_dir_contents(&ram);
    }
}

/// Resolve a core `fileSource.filePath` (absolute or files-folder-relative).
pub fn resolve_local_file(stored: &str) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        files_dir().join(path)
    }
}

/// RAM-backed private directory for plaintext caches:
/// `$XDG_RUNTIME_DIR/cursus`. The runtime dir is a per-user tmpfs (`0700`)
/// that the system empties at logout and reboot, so decrypted media there
/// never reaches persistent storage — not even after a crash or power loss.
/// `None` when there is no private runtime dir (then the disk cache is used).
pub fn runtime_cache_dir() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let base = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    let meta = std::fs::metadata(&base).ok()?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return None;
    }
    Some(base.join("cursus"))
}

/// Where short-lived plaintext (playback transcodes) goes: RAM if possible.
pub fn private_cache_dir() -> PathBuf {
    runtime_cache_dir().unwrap_or_else(temp_dir)
}

/// Free bytes on the filesystem holding `path` (or its nearest parent).
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let c = std::ffi::CString::new(probe.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: valid C string and out-pointer.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}

/// Whether a `size`-byte plaintext should go to the RAM-backed cache: only
/// with plenty of room left (tmpfs is shared with other apps and is memory).
pub fn fits_in_ram_cache(size: u64, free: u64) -> bool {
    const RESERVE: u64 = 64 * 1024 * 1024;
    size.saturating_mul(4) <= free && size.saturating_add(RESERVE) < free
}

/// File name of a decrypted CryptoFile in either cache directory.
///
/// Keyed by source path + key + nonce so re-decrypt is a cache hit.
pub fn plaintext_cache_name(encrypted: &Path, key: &str, nonce: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    encrypted.hash(&mut h);
    key.hash(&mut h);
    nonce.hash(&mut h);
    let stem = encrypted
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    format!("{:016x}_{stem}", h.finish())
}

/// On-disk cache path for a decrypted CryptoFile (under `tmp/plain/`), used
/// when the RAM cache is unavailable or too small.
pub fn plaintext_cache_path(encrypted: &Path, key: &str, nonce: &str) -> PathBuf {
    temp_dir().join("plain").join(plaintext_cache_name(encrypted, key, nonce))
}

/// Where `libsimplex` lives. In order: `$CURSUS_LIBS_DIR`; an installed
/// layout next to the binary (`<prefix>/bin/cursus` → `<prefix>/lib/cursus`,
/// used by the Flatpak and distro packages); the repo's `third_party/`.
pub fn default_libs_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CURSUS_LIBS_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(installed) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(|p| p.join("lib").join("cursus")))
        .filter(|dir| dir.join(lib_filename()).is_file())
    {
        return installed;
    }
    workspace_root()
        .map(|root| root.join("third_party").join("libsimplex"))
        .unwrap_or_else(|| PathBuf::from("third_party/libsimplex"))
}

fn workspace_root() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn lib_filename() -> &'static str {
    "libsimplex.so"
}

#[cfg(target_os = "macos")]
pub fn lib_filename() -> &'static str {
    "libsimplex.dylib"
}

#[cfg(target_os = "windows")]
pub fn lib_filename() -> &'static str {
    "libsimplex.dll"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_empties_dir_without_following_symlinks() {
        let root = std::env::temp_dir().join(format!("cursus-wipe-{}", std::process::id()));
        let inside = root.join("plain");
        let outside = root.join("keep");
        std::fs::create_dir_all(inside.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(inside.join("photo.jpg"), b"x").unwrap();
        std::fs::write(inside.join("sub/voice.wav"), b"x").unwrap();
        std::fs::write(outside.join("precious.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();
        wipe_dir_contents(&inside);
        assert_eq!(std::fs::read_dir(&inside).unwrap().count(), 0);
        assert!(outside.join("precious.txt").is_file(), "followed a symlink");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ram_cache_only_with_room_to_spare() {
        const MB: u64 = 1024 * 1024;
        assert!(fits_in_ram_cache(5 * MB, 1500 * MB));
        assert!(!fits_in_ram_cache(100 * MB, 300 * MB), "would take a third of tmpfs");
        assert!(!fits_in_ram_cache(10 * MB, 70 * MB), "must keep the reserve free");
        assert!(!fits_in_ram_cache(u64::MAX, u64::MAX));
    }

    #[test]
    fn plaintext_cache_path_stable_and_keyed() {
        let a = plaintext_cache_path(Path::new("/files/a.jpg"), "k1", "n1");
        let b = plaintext_cache_path(Path::new("/files/a.jpg"), "k1", "n1");
        let c = plaintext_cache_path(Path::new("/files/a.jpg"), "k2", "n1");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.to_string_lossy().contains("plain"));
        assert!(a.file_name().unwrap().to_string_lossy().contains("a.jpg"));
    }
}
