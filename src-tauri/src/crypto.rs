use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedPayload {
    pub algorithm: String,
    pub nonce_b64: String,
    pub salt_b64: String,
    pub key_hash_b64: String,
    pub plaintext_sha256_b64: String,
    pub ciphertext_b64: String,
}

pub fn encrypt_bytes(plaintext: &[u8], context: &[u8]) -> Result<EncryptedPayload, String> {
    let mut key = [0u8; 32];
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut key);
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|err| err.to_string())?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|err| format!("AES-GCM encryption failed: {err}"))?;

    let mut key_hasher = Sha256::new();
    key_hasher.update(key);
    key_hasher.update(salt);
    key_hasher.update(context);

    let mut plain_hasher = Sha256::new();
    plain_hasher.update(plaintext);

    Ok(EncryptedPayload {
        algorithm: "AES-256-GCM".to_string(),
        nonce_b64: STANDARD.encode(nonce_bytes),
        salt_b64: STANDARD.encode(salt),
        key_hash_b64: STANDARD.encode(key_hasher.finalize()),
        plaintext_sha256_b64: STANDARD.encode(plain_hasher.finalize()),
        ciphertext_b64: STANDARD.encode(ciphertext),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_payload_without_leaking_plaintext() {
        let payload = encrypt_bytes(b"classes.dex bytes", b"com.example").unwrap();
        assert_eq!(payload.algorithm, "AES-256-GCM");
        assert!(!payload.ciphertext_b64.contains("classes"));
        assert!(!payload.nonce_b64.is_empty());
        assert!(!payload.key_hash_b64.is_empty());
    }
}
