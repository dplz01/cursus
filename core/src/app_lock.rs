//! App lock password check — shared by every app's lock screen.
//!
//! Cursus never stores the database password. To re-check it on the lock
//! screen, the moment the password has just *opened* the database the app
//! calls [`remember`]: an Argon2id hash with a fresh random salt is kept in
//! memory only. Nothing touches disk; whoever could read it from RAM could
//! already read the SQLCipher key from libsimplex, so it adds no exposure.
//! (The core's `/db test key` only accepts printable ASCII without spaces,
//! so it can't verify every password.)

use std::sync::Mutex;

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

/// Drop the verifier (the session closed; the next open sets a new one).
pub fn forget() {
    if let Ok(mut v) = VERIFIER.lock() {
        *v = None;
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

/// Delay before another unlock attempt after `failures` wrong passwords:
/// none for the first three, then 2, 4, 8… seconds, capped at 60.
pub fn retry_delay_secs(failures: u32) -> u64 {
    if failures < 3 {
        0
    } else {
        (1u64 << (failures - 2).min(6)).min(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_accepts_only_the_unlock_password() {
        // Not while another test starts libsimplex (see LIBSIMPLEX_STARTUP).
        let _startup = crate::LIBSIMPLEX_STARTUP.lock().unwrap_or_else(|e| e.into_inner());
        remember("correct horse 🐴 battery");
        assert!(has_verifier());
        assert!(verify("correct horse 🐴 battery"));
        assert!(!verify("correct horse battery"));
        assert!(!verify(""));
    }

    #[test]
    fn retry_delay_grows_and_caps() {
        assert_eq!(retry_delay_secs(2), 0);
        assert_eq!(retry_delay_secs(3), 2);
        assert_eq!(retry_delay_secs(4), 4);
        assert_eq!(retry_delay_secs(20), 60);
    }
}
