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
    pub key_wrap_algorithm: String,
    pub wrapped_key_b64: String,
    pub key_wrap_nonce_b64: String,
    pub payload_nonce_b64: String,
    pub kdf_salt_b64: String,
    pub wrapped_key_sha256_b64: String,
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

const WRAP_SEED_A: [u8; 32] = [
    0x19, 0x7d, 0x42, 0xb8, 0xc1, 0x0e, 0x6a, 0x90, 0x25, 0xf4, 0x38, 0xdd, 0x61, 0xab, 0x0c, 0x73,
    0xe7, 0x56, 0x2d, 0x81, 0x9c, 0x04, 0xfa, 0x3b, 0x68, 0x12, 0xcf, 0xa5, 0x4e, 0x91, 0x37, 0xd0,
];
const WRAP_SEED_B: [u8; 32] = [
    0xc3, 0x55, 0x2f, 0x80, 0x0a, 0xee, 0x41, 0x72, 0xd9, 0x66, 0x13, 0xac, 0x5f, 0x98, 0x21, 0x47,
    0xbe, 0x02, 0xf6, 0x35, 0x8c, 0x7a, 0x10, 0xd4, 0x29, 0xb1, 0x6e, 0x03, 0x95, 0x4c, 0xea, 0x1f,
];

pub fn encrypt_bytes(
    plaintext: &[u8],
    context: &[u8],
    payload_file: impl Into<String>,
) -> Result<EncryptedBytes, String> {
    let mut payload_key = [0u8; 32];
    let mut kdf_salt = [0u8; 16];
    let mut payload_nonce = [0u8; 12];
    let mut key_wrap_nonce = [0u8; 12];
    OsRng.fill_bytes(&mut payload_key);
    OsRng.fill_bytes(&mut kdf_salt);
    OsRng.fill_bytes(&mut payload_nonce);
    OsRng.fill_bytes(&mut key_wrap_nonce);

    let cipher = Aes256Gcm::new_from_slice(&payload_key).map_err(|err| err.to_string())?;
    let nonce = Nonce::from_slice(&payload_nonce);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|err| format!("AES-GCM encryption failed: {err}"))?;

    let wrap_key = derive_wrap_key(context, &kdf_salt);
    let wrap_cipher = Aes256Gcm::new_from_slice(&wrap_key).map_err(|err| err.to_string())?;
    let wrapped_key = wrap_cipher
        .encrypt(Nonce::from_slice(&key_wrap_nonce), payload_key.as_slice())
        .map_err(|err| format!("AES-GCM key wrapping failed: {err}"))?;

    let mut plain_hasher = Sha256::new();
    plain_hasher.update(plaintext);

    let mut cipher_hasher = Sha256::new();
    cipher_hasher.update(&ciphertext);

    let mut wrapped_key_hasher = Sha256::new();
    wrapped_key_hasher.update(&wrapped_key);

    Ok(EncryptedBytes {
        metadata: EncryptedPayload {
            mode: "encrypted-dex-zip".to_string(),
            active: true,
            algorithm: "AES-256-GCM".to_string(),
            key_wrap_algorithm: "AES-256-GCM+loader-derived-key".to_string(),
            wrapped_key_b64: STANDARD.encode(wrapped_key),
            key_wrap_nonce_b64: STANDARD.encode(key_wrap_nonce),
            payload_nonce_b64: STANDARD.encode(payload_nonce),
            kdf_salt_b64: STANDARD.encode(kdf_salt),
            wrapped_key_sha256_b64: STANDARD.encode(wrapped_key_hasher.finalize()),
            plaintext_sha256_b64: STANDARD.encode(plain_hasher.finalize()),
            ciphertext_sha256_b64: STANDARD.encode(cipher_hasher.finalize()),
            plaintext_len: plaintext.len(),
            ciphertext_len: ciphertext.len(),
            payload_file: payload_file.into(),
        },
        ciphertext,
    })
}

fn derive_wrap_key(context: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut secret = [0u8; 32];
    for index in 0..secret.len() {
        secret[index] = WRAP_SEED_A[index] ^ WRAP_SEED_B[31 - index] ^ 0x5a;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"android-protector-dex-wrap-v2");
    hasher.update(secret);
    hasher.update(salt);
    hasher.update(context);
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
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
        assert!(!payload.metadata.wrapped_key_b64.is_empty());
        assert!(!payload.metadata.payload_nonce_b64.is_empty());
        assert!(!payload.metadata.wrapped_key_sha256_b64.is_empty());
        let metadata_json = serde_json::to_string(&payload.metadata).unwrap();
        assert!(!metadata_json.contains("keyB64"));
        assert_eq!(
            payload.metadata.payload_file,
            "assets/protector/dex-payload.bin"
        );
    }
}
