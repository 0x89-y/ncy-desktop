use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

pub fn random_nonce_b64() -> String {
    let mut nonce = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);
    B64.encode(nonce)
}

/// Constant-time check of a phone-supplied auth tag against the expected
/// HMAC-SHA256(secret, nonceBytes). Inputs are base64 strings as they arrive on
/// the wire (per protocol §5).
pub fn verify_auth(secret_b64: &str, nonce_b64: &str, auth_b64: &str) -> bool {
    let Ok(secret) = B64.decode(secret_b64) else {
        return false;
    };
    let Ok(nonce) = B64.decode(nonce_b64) else {
        return false;
    };
    let Ok(provided) = B64.decode(auth_b64) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(&secret) else {
        return false;
    };
    mac.update(&nonce);
    let expected = mac.finalize().into_bytes();
    expected.as_slice().ct_eq(provided.as_slice()).into()
}
