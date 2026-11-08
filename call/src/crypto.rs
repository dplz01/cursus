//! End-to-end media encryption of SimpleX calls.
//!
//! On top of DTLS-SRTP, the official clients encrypt every encoded media frame
//! with the call's shared key (negotiated by the chat core through the
//! invitation's X25519 keys) using WebRTC insertable streams — see
//! `callCryptoFunction` in `call.ts`:
//!
//! ```text
//! frame = initial (plaintext) ‖ AES-256-GCM(rest) incl. 16-byte tag ‖ IV (12 bytes)
//! ```
//!
//! For Opus `initial` is the single TOC byte (RFC 6716 §3.1). One Opus frame
//! is exactly one RTP payload, so we apply this to RTP payloads directly.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;

const IV_LENGTH: usize = 12;
/// Opus TOC byte stays in the clear (the browser's decoder needs it).
const AUDIO_PLAINTEXT: usize = 1;

#[derive(Clone)]
pub struct FrameCrypto {
    cipher: Aes256Gcm,
}

impl FrameCrypto {
    /// `aes_key`: the core's `sharedKey` (base64url, 32 bytes).
    pub fn from_shared_key(aes_key: &str) -> Result<Self, String> {
        let trimmed = aes_key.trim().trim_end_matches('=');
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(trimmed)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
            .map_err(|e| format!("call key: {e}"))?;
        if raw.len() != 32 {
            return Err(format!("call key: expected 32 bytes, got {}", raw.len()));
        }
        let cipher = Aes256Gcm::new_from_slice(&raw).map_err(|e| format!("call key: {e}"))?;
        Ok(Self { cipher })
    }

    pub fn encrypt_audio(&self, frame: &[u8]) -> Vec<u8> {
        let n = AUDIO_PLAINTEXT.min(frame.len());
        let (initial, plaintext) = frame.split_at(n);
        let iv = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = if plaintext.is_empty() {
            Vec::new()
        } else {
            // Encrypting with a fresh random nonce cannot fail.
            self.cipher.encrypt(&iv, plaintext).unwrap_or_default()
        };
        let mut out = Vec::with_capacity(n + ciphertext.len() + IV_LENGTH);
        out.extend_from_slice(initial);
        out.extend_from_slice(&ciphertext);
        out.extend_from_slice(iv.as_slice());
        out
    }

    pub fn decrypt_audio(&self, frame: &[u8]) -> Option<Vec<u8>> {
        if frame.len() < AUDIO_PLAINTEXT + IV_LENGTH {
            return None;
        }
        let (initial, rest) = frame.split_at(AUDIO_PLAINTEXT);
        let (ciphertext, iv) = rest.split_at(rest.len() - IV_LENGTH);
        let mut out = initial.to_vec();
        if !ciphertext.is_empty() {
            let plain = self.cipher.decrypt(Nonce::from_slice(iv), ciphertext).ok()?;
            out.extend_from_slice(&plain);
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 32])
    }

    #[test]
    fn round_trip_keeps_toc_byte_clear() {
        let c = FrameCrypto::from_shared_key(&key()).unwrap();
        let frame = [0x78u8, 1, 2, 3, 4, 5];
        let enc = c.encrypt_audio(&frame);
        assert_eq!(enc[0], 0x78);
        assert_eq!(enc.len(), 1 + 5 + 16 + 12);
        assert_eq!(c.decrypt_audio(&enc).unwrap(), frame);
    }

    #[test]
    fn tampered_frame_is_rejected() {
        let c = FrameCrypto::from_shared_key(&key()).unwrap();
        let mut enc = c.encrypt_audio(&[0x78, 9, 9, 9]);
        enc[2] ^= 1;
        assert!(c.decrypt_audio(&enc).is_none());
    }

    #[test]
    fn padded_and_short_keys() {
        let padded = base64::engine::general_purpose::URL_SAFE.encode([1u8; 32]);
        assert!(FrameCrypto::from_shared_key(&padded).is_ok());
        let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 16]);
        assert!(FrameCrypto::from_shared_key(&short).is_err());
    }
}
