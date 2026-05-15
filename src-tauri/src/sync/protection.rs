//! Phase 3 — protected-chat encryption primitives. Mirror of
//! `app/src/main/java/com/ncy/notechaty/data/sync/protection/ProtectionCrypto.kt`
//! on the phone, byte-for-byte compatible: same algorithms, same iteration count,
//! same envelope shape, so the two sides decrypt each other's payloads.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;

/// Wire version of the sealed envelope. Phone uses the same value.
pub const ENVELOPE_VERSION: u32 = 1;
/// PBKDF2 round count. Must match the phone's `ProtectionCrypto.KDF_ITERATIONS`.
pub const KDF_ITERATIONS: u32 = 250_000;

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const SALT_BYTES: usize = 16;

/// Sealed envelope returned by `seal` and consumed by `open`. Serialized as
/// `{ "v": 1, "nonce": "<base64-12>", "ct": "<base64>" }` when stored in an event
/// payload.
#[derive(Debug, Clone)]
pub struct Sealed {
    pub nonce: [u8; NONCE_BYTES],
    pub ciphertext: Vec<u8>,
}

/// Generate a fresh 16-byte random salt, base64-encoded for direct insertion into
/// `ChatMetadata.protectionSalt`.
pub fn generate_salt() -> String {
    let mut bytes = [0u8; SALT_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    B64.encode(bytes)
}

/// Derive a 32-byte AES key from `pin` + base64-encoded `salt_b64`. Deterministic —
/// same inputs always produce the same key.
pub fn derive_key(pin: &str, salt_b64: &str) -> Result<[u8; KEY_BYTES]> {
    let salt = B64
        .decode(salt_b64)
        .with_context(|| format!("decode salt {salt_b64}"))?;
    let mut key = [0u8; KEY_BYTES];
    pbkdf2_hmac::<Sha256>(pin.as_bytes(), &salt, KDF_ITERATIONS, &mut key);
    Ok(key)
}

/// Encrypt `plaintext` under `key`; returns the freshly-generated nonce alongside
/// the ciphertext (which already includes the GCM tag at the end).
pub fn seal(key: &[u8; KEY_BYTES], plaintext: &[u8]) -> Result<Sealed> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow!("aes-gcm encrypt: {e}"))?;
    Ok(Sealed {
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Decrypt; returns `None` on tag mismatch (wrong PIN) or any other AEAD failure.
pub fn open(key: &[u8; KEY_BYTES], sealed: &Sealed) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(&sealed.nonce);
    cipher.decrypt(nonce, sealed.ciphertext.as_slice()).ok()
}

/// Decode + open an `{v,enc,nonce,ct}` envelope (as it appears inside an event
/// payload). `None` on missing fields, bad base64, wrong nonce length, or wrong key.
pub fn open_envelope(
    payload: &serde_json::Value,
    key: &[u8; KEY_BYTES],
) -> Option<Vec<u8>> {
    let nonce_b64 = payload.get("nonce")?.as_str()?;
    let ct_b64 = payload.get("ct")?.as_str()?;
    let nonce_vec = B64.decode(nonce_b64).ok()?;
    let ct = B64.decode(ct_b64).ok()?;
    if nonce_vec.len() != NONCE_BYTES {
        return None;
    }
    let mut nonce = [0u8; NONCE_BYTES];
    nonce.copy_from_slice(&nonce_vec);
    open(key, &Sealed { nonce, ciphertext: ct })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let salt = generate_salt();
        let key = derive_key("1234", &salt).unwrap();
        let pt = b"hello, locked chat";
        let s = seal(&key, pt).unwrap();
        assert_eq!(open(&key, &s).as_deref(), Some(&pt[..]));
    }

    #[test]
    fn wrong_pin_returns_none() {
        let salt = generate_salt();
        let key = derive_key("1234", &salt).unwrap();
        let s = seal(&key, b"x").unwrap();
        let wrong = derive_key("9999", &salt).unwrap();
        assert!(open(&wrong, &s).is_none());
    }
}
