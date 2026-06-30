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
    pub mode: String,
    pub active: bool,
    pub algorithm: String,
    pub key_b64: String,
    pub nonce_b64: String,
    pub salt_b64: String,
    pub key_hash_b64: String,
    pub plaintext_sha256_b64: String,
    pub ciphertext_sha256_b64: String,
    pub plaintext_len: usize,
    pub ciphertext_len: usize,
    pub payload_file: String,
}

#[derive(Debug, Clone)]
pub struct EncryptedBytes {
    pub metadata: EncryptedPayload,
    pub ciphertext: Vec<u8>,
}

pub fn encrypt_bytes(
    plaintext: &[u8],
    context: &[u8],
    payload_file: impl Into<String>,
) -> Result<EncryptedBytes, String> {
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

    let mut cipher_hasher = Sha256::new();
    cipher_hasher.update(&ciphertext);

    Ok(EncryptedBytes {
        metadata: EncryptedPayload {
            mode: "encrypted-dex-zip".to_string(),
            active: true,
            algorithm: "AES-256-GCM".to_string(),
            key_b64: STANDARD.encode(key),
            nonce_b64: STANDARD.encode(nonce_bytes),
            salt_b64: STANDARD.encode(salt),
            key_hash_b64: STANDARD.encode(key_hasher.finalize()),
            plaintext_sha256_b64: STANDARD.encode(plain_hasher.finalize()),
            ciphertext_sha256_b64: STANDARD.encode(cipher_hasher.finalize()),
            plaintext_len: plaintext.len(),
            ciphertext_len: ciphertext.len(),
            payload_file: payload_file.into(),
        },
        ciphertext,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_payload_without_leaking_plaintext() {
        let payload = encrypt_bytes(
            b"classes.dex bytes",
            b"com.example",
            "assets/protector/dex-payload.bin",
        )
        .unwrap();
        assert_eq!(payload.metadata.algorithm, "AES-256-GCM");
        assert!(!String::from_utf8_lossy(&payload.ciphertext).contains("classes"));
        assert!(!payload.metadata.key_b64.is_empty());
        assert!(!payload.metadata.nonce_b64.is_empty());
        assert!(!payload.metadata.key_hash_b64.is_empty());
        assert_eq!(
            payload.metadata.payload_file,
            "assets/protector/dex-payload.bin"
        );
    }
}
