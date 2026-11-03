//! First-run account metadata (not the SQLCipher key — that stays in memory only).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::data_dir;

/// Persisted after welcome completes. Presence means “unlock with password”, not setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountMeta {
    pub display_name: String,
    /// Schema marker so future fields can migrate.
    pub version: u32,
    /// When true, local file receives use CryptoFile (`encrypt=on`).
    pub encrypt_local_files: bool,
}

impl Default for AccountMeta {
    fn default() -> Self {
        Self {
            display_name: "Cursus".into(),
            version: 1,
            encrypt_local_files: true,
        }
    }
}

pub fn account_path() -> PathBuf {
    data_dir().join("account.json")
}

/// True when the user has finished the welcome wizard at least once.
pub fn setup_complete() -> bool {
    account_path().is_file()
}

/// True when a SimpleX SQLCipher DB already exists under the default prefix.
pub fn database_exists() -> bool {
    let prefix = crate::paths::db_prefix();
    let chat = PathBuf::from(format!("{}_chat.db", prefix.display()));
    let agent = PathBuf::from(format!("{}_agent.db", prefix.display()));
    chat.is_file() || agent.is_file()
}

pub fn load_account() -> Option<AccountMeta> {
    let raw = fs::read_to_string(account_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_account(meta: &AccountMeta) -> std::io::Result<()> {
    if let Some(parent) = account_path().parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(account_path(), raw)
}

/// Wipe profile data so the next launch is a first-run welcome (destructive).
pub fn wipe_local_data() -> std::io::Result<()> {
    let dir = data_dir();
    if dir.is_dir() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_roundtrip_defaults() {
        let meta = AccountMeta::default();
        assert!(meta.encrypt_local_files);
        assert_eq!(meta.version, 1);
        let raw = serde_json::to_string(&meta).unwrap();
        let back: AccountMeta = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.display_name, "Cursus");
    }
}
