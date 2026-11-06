use std::path::{Path, PathBuf};

/// XDG data directory for Cursus (`~/.local/share/cursus` on Linux).
pub fn data_dir() -> PathBuf {
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

/// Default location for downloaded/built native libraries inside the repo.
pub fn default_libs_dir() -> PathBuf {
    workspace_root()
        .map(|root| root.join("third_party").join("libsimplex"))
        .unwrap_or_else(|| PathBuf::from("third_party/libsimplex"))
}

fn workspace_root() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf)
}

#[cfg(target_os = "linux")]
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
    use std::path::Path;

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
