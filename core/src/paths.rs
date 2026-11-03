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

/// Cache path for a decrypted CryptoFile (under `tmp/plain/`).
///
/// Keyed by source path + key + nonce so re-decrypt is a cache hit.
pub fn plaintext_cache_path(encrypted: &Path, key: &str, nonce: &str) -> PathBuf {
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
    temp_dir()
        .join("plain")
        .join(format!("{:016x}_{stem}", h.finish()))
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
