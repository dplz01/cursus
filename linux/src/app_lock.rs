//! App lock + optional keyring unlock.
//!
//! **Verifier.** Cursus never stores the database password. To re-check it on
//! the lock screen, the moment the password has just *opened* the database
//! we derive an Argon2id hash with a fresh random salt and keep that hash in
//! memory only. Nothing touches disk; whoever could read it from RAM could
//! already read the SQLCipher key from libsimplex, so it adds no exposure.
//! (The core's `/db test key` only accepts printable ASCII without spaces,
//! so it can't verify every password.)
//!
//! **Idle tracking.** Input events bump [`touch`] from the event filter
//! without publishing App messages.
//!
//! **Keyring.** Opt-in "unlock automatically" keeps the password in the
//! desktop Secret Service (GNOME Keyring, KWallet, KeePassXC…), which is
//! itself encrypted with the login password — never in Cursus' files.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

static VERIFIER: Mutex<Option<String>> = Mutex::new(None);

/// Remember a verifier for `password` (call only after it opened the DB).
pub fn remember(password: &str) {
    let salt = SaltString::generate(&mut OsRng);
    if let Ok(hash) = Argon2::default().hash_password(password.as_bytes(), &salt) {
        if let Ok(mut v) = VERIFIER.lock() {
            *v = Some(hash.to_string());
        }
    }
}

/// Whether `password` matches the remembered one (slow by design, ~0.1–0.5 s).
pub fn verify(password: &str) -> bool {
    let Some(stored) = VERIFIER.lock().ok().and_then(|v| v.clone()) else {
        return false;
    };
    let Ok(parsed) = PasswordHash::new(&stored) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn has_verifier() -> bool {
    VERIFIER.lock().is_ok_and(|v| v.is_some())
}

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

/// Delay before another unlock attempt after `failures` wrong passwords:
/// none for the first three, then 2, 4, 8… seconds, capped at 60.
pub fn retry_delay_secs(failures: u32) -> u64 {
    if failures < 3 {
        0
    } else {
        (1u64 << (failures - 2).min(6)).min(60)
    }
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

    #[test]
    fn verifier_accepts_only_the_unlock_password() {
        remember("correct horse 🐴 battery");
        assert!(has_verifier());
        assert!(verify("correct horse 🐴 battery"));
        assert!(!verify("correct horse battery"));
        assert!(!verify(""));
    }

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

    #[test]
    fn retry_delay_grows_and_caps() {
        assert_eq!(retry_delay_secs(2), 0);
        assert_eq!(retry_delay_secs(3), 2);
        assert_eq!(retry_delay_secs(4), 4);
        assert_eq!(retry_delay_secs(20), 60);
    }
}
