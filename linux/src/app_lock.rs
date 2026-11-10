//! App lock + optional keyring unlock.
//!
//! **Verifier.** Lives in `cursus_core::app_lock` (shared with the other
//! apps): an in-memory Argon2id hash of the password that opened the DB.
//!
//! **Idle tracking.** Input events bump [`touch`] from the event filter
//! without publishing App messages.
//!
//! **Keyring.** Opt-in "unlock automatically" keeps the password in the
//! desktop Secret Service (GNOME Keyring, KWallet, KeePassXC…), which is
//! itself encrypted with the login password — never in Cursus' files.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

// The password verifier is shared with the other apps' lock screens.
pub use cursus_core::app_lock::{has_verifier, remember, retry_delay_secs, verify};

fn epoch() -> &'static Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now)
}

static LAST_INPUT_MS: AtomicU64 = AtomicU64::new(0);

/// Record user activity (keyboard, pointer, touch).
pub fn touch() {
    LAST_INPUT_MS.store(epoch().elapsed().as_millis() as u64, Ordering::Relaxed);
}

/// Seconds since the last input event.
pub fn idle_secs() -> u64 {
    let now = epoch().elapsed().as_millis() as u64;
    now.saturating_sub(LAST_INPUT_MS.load(Ordering::Relaxed)) / 1000
}


const KEYRING_LABEL: &str = "Cursus database password";

fn keyring_attrs() -> std::collections::HashMap<&'static str, &'static str> {
    std::collections::HashMap::from([("application", "chat.cursus"), ("purpose", "database-password")])
}

/// Store the DB password in the Secret Service.
pub async fn keyring_store(password: String) -> Result<(), String> {
    let password = zeroize::Zeroizing::new(password);
    let keyring = oo7::Keyring::new().await.map_err(|e| format!("No system keyring: {e}"))?;
    keyring.unlock().await.map_err(|e| format!("Keyring locked: {e}"))?;
    keyring
        .create_item(KEYRING_LABEL, &keyring_attrs(), password.as_str(), true)
        .await
        .map_err(|e| format!("Could not save to keyring: {e}"))
}

/// Read the DB password from the Secret Service, if stored.
pub async fn keyring_load() -> Result<Option<crate::Secret>, String> {
    let keyring = oo7::Keyring::new().await.map_err(|e| format!("No system keyring: {e}"))?;
    keyring.unlock().await.map_err(|e| format!("Keyring locked: {e}"))?;
    let items = keyring
        .search_items(&keyring_attrs())
        .await
        .map_err(|e| format!("Keyring error: {e}"))?;
    let Some(item) = items.first() else {
        return Ok(None);
    };
    let _ = item.unlock().await;
    let secret = item.secret().await.map_err(|e| format!("Keyring error: {e}"))?;
    let text = String::from_utf8(secret.to_vec()).map_err(|_| "Keyring item is not text".to_string())?;
    Ok(Some(crate::Secret::new(text)))
}

/// Remove the stored DB password from the Secret Service.
pub async fn keyring_delete() -> Result<(), String> {
    let keyring = oo7::Keyring::new().await.map_err(|e| format!("No system keyring: {e}"))?;
    keyring
        .delete(&keyring_attrs())
        .await
        .map_err(|e| format!("Keyring error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Secret Service round trip (needs a running keyring).
    #[test]
    #[ignore]
    fn keyring_round_trip() {
        use iced::futures::executor::block_on;
        block_on(keyring_store("cursus keyring test ✓".into())).expect("store");
        let got = block_on(keyring_load()).expect("load").expect("item present");
        assert_eq!(got.as_str(), "cursus keyring test ✓");
        block_on(keyring_delete()).expect("delete");
        assert!(block_on(keyring_load()).expect("load after delete").is_none());
    }
}
