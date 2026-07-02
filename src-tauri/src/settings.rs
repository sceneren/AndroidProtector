use crate::models::{
    AppPreferences, SigningConfig, SigningProfile, SigningProfileInput, SigningScheme,
};
use crate::signing;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const CONFIG_DIR_NAME: &str = "AndroidProtector";
const SECRET_PREFIX_V2: &str = "v2:";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredPreferences {
    signing_profiles: Vec<StoredSigningProfile>,
    last_output_dir: Option<String>,
    selected_signing_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredSigningProfile {
    id: String,
    name: String,
    keystore_path: String,
    alias: String,
    store_type: Option<String>,
    signing_scheme: Option<SigningScheme>,
    store_password: String,
    key_password: Option<String>,
    certificate_summary: Option<String>,
    created_at: String,
    updated_at: String,
}

pub fn load_preferences() -> Result<AppPreferences, String> {
    let stored = load_stored_preferences()?;
    Ok(AppPreferences {
        signing_profiles: stored
            .signing_profiles
            .iter()
            .map(to_public_profile)
            .collect(),
        last_output_dir: stored.last_output_dir,
        selected_signing_profile_id: stored.selected_signing_profile_id,
    })
}

pub fn save_last_output_dir(path: String) -> Result<AppPreferences, String> {
    let mut stored = load_stored_preferences()?;
    stored.last_output_dir = clean_opt(&path);
    save_stored_preferences(&stored)?;
    load_preferences()
}

pub fn set_selected_signing_profile(id: Option<String>) -> Result<AppPreferences, String> {
    let mut stored = load_stored_preferences()?;
    stored.selected_signing_profile_id = id.filter(|value| {
        stored
            .signing_profiles
            .iter()
            .any(|profile| profile.id == *value)
    });
    save_stored_preferences(&stored)?;
    load_preferences()
}

pub fn save_signing_profile(input: SigningProfileInput) -> Result<AppPreferences, String> {
    if input
        .key_password
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Err("alias key password is required".to_string());
    }

    let config = SigningConfig {
        keystore_path: input.keystore_path.trim().to_string(),
        store_password: input.store_password.clone(),
        key_password: input.key_password.clone(),
        alias: input.alias.trim().to_string(),
        store_type: clean_opt(input.store_type.as_deref().unwrap_or_default()),
        signing_scheme: input.signing_scheme,
    };
    let validation = signing::validate_signing_config(&config);
    if !validation.valid {
        return Err(validation.issues.join("\n"));
    }

    let now = Utc::now().to_rfc3339();
    let mut stored = load_stored_preferences()?;
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_created_at = stored
        .signing_profiles
        .iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let profile = StoredSigningProfile {
        id: id.clone(),
        name: if input.name.trim().is_empty() {
            config.alias.clone()
        } else {
            input.name.trim().to_string()
        },
        keystore_path: config.keystore_path,
        alias: config.alias,
        store_type: config.store_type,
        signing_scheme: Some(config.signing_scheme),
        store_password: encode_secret(&input.store_password),
        key_password: input
            .key_password
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(|value| encode_secret(value)),
        certificate_summary: validation.certificate_summary,
        created_at: existing_created_at,
        updated_at: now,
    };

    if let Some(existing) = stored
        .signing_profiles
        .iter_mut()
        .find(|profile| profile.id == id)
    {
        *existing = profile;
    } else {
        stored.signing_profiles.push(profile);
    }
    stored.selected_signing_profile_id = Some(id);
    save_stored_preferences(&stored)?;
    load_preferences()
}

pub fn delete_signing_profile(id: String) -> Result<AppPreferences, String> {
    let mut stored = load_stored_preferences()?;
    stored.signing_profiles.retain(|profile| profile.id != id);
    if stored.selected_signing_profile_id.as_deref() == Some(&id) {
        stored.selected_signing_profile_id = stored
            .signing_profiles
            .first()
            .map(|profile| profile.id.clone());
    }
    save_stored_preferences(&stored)?;
    load_preferences()
}

pub fn signing_config_for_profile(id: &str) -> Result<Option<SigningConfig>, String> {
    let stored = load_stored_preferences()?;
    let Some(profile) = stored
        .signing_profiles
        .iter()
        .find(|profile| profile.id == id)
    else {
        return Ok(None);
    };
    Ok(Some(SigningConfig {
        keystore_path: profile.keystore_path.clone(),
        store_password: decode_secret(&profile.store_password)?,
        key_password: profile
            .key_password
            .as_ref()
            .map(|value| decode_secret(value))
            .transpose()?,
        alias: profile.alias.clone(),
        store_type: profile.store_type.clone(),
        signing_scheme: profile.signing_scheme.unwrap_or_default(),
    }))
}

pub fn signing_profile_input(id: &str) -> Result<Option<SigningProfileInput>, String> {
    let stored = load_stored_preferences()?;
    let Some(profile) = stored
        .signing_profiles
        .iter()
        .find(|profile| profile.id == id)
    else {
        return Ok(None);
    };

    Ok(Some(SigningProfileInput {
        id: Some(profile.id.clone()),
        name: profile.name.clone(),
        keystore_path: profile.keystore_path.clone(),
        store_password: decode_secret(&profile.store_password)?,
        key_password: profile
            .key_password
            .as_ref()
            .map(|value| decode_secret(value))
            .transpose()?,
        alias: profile.alias.clone(),
        store_type: profile.store_type.clone(),
        signing_scheme: profile.signing_scheme.unwrap_or_default(),
    }))
}

fn load_stored_preferences() -> Result<StoredPreferences, String> {
    let path = existing_settings_path()?;
    if !path.exists() {
        return Ok(StoredPreferences::default());
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read settings {}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("failed to parse settings: {err}"))
}

fn save_stored_preferences(preferences: &StoredPreferences) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create settings dir {}: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(preferences)
        .map_err(|err| format!("failed to serialize settings: {err}"))?;
    fs::write(&path, text)
        .map_err(|err| format!("failed to write settings {}: {err}", path.display()))
}

fn to_public_profile(profile: &StoredSigningProfile) -> SigningProfile {
    SigningProfile {
        id: profile.id.clone(),
        name: profile.name.clone(),
        keystore_path: profile.keystore_path.clone(),
        alias: profile.alias.clone(),
        store_type: profile.store_type.clone(),
        signing_scheme: profile.signing_scheme.unwrap_or_default(),
        certificate_summary: profile.certificate_summary.clone(),
        created_at: profile.created_at.clone(),
        updated_at: profile.updated_at.clone(),
    }
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(config_root(CONFIG_DIR_NAME)?.join("settings.json"))
}

fn existing_settings_path() -> Result<PathBuf, String> {
    let path = settings_path()?;
    if path.exists() {
        return Ok(path);
    }
    let legacy_path = config_root(&legacy_config_dir_name())?.join("settings.json");
    if legacy_path.exists() {
        return Ok(legacy_path);
    }
    Ok(path)
}

fn config_root(dir_name: &str) -> Result<PathBuf, String> {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join(dir_name))
            .map_err(|_| "APPDATA is not set".to_string())
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|home| {
                Path::new(&home)
                    .join("Library")
                    .join("Application Support")
                    .join(dir_name)
            })
            .map_err(|_| "HOME is not set".to_string())
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|home| Path::new(&home).join(".config")))
            .map(|path| path.join(dir_name))
            .map_err(|_| "HOME is not set".to_string())
    }
}

fn legacy_config_dir_name() -> String {
    ["Android", "Third", "gen", "Protector"].concat()
}

fn clean_opt(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn encode_secret(secret: &str) -> String {
    let key = current_secret_key();
    format!("{SECRET_PREFIX_V2}{}", encode_secret_with_key(secret, &key))
}

fn encode_secret_with_key(secret: &str, key: &[u8]) -> String {
    let bytes = secret
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect::<Vec<_>>();
    STANDARD.encode(bytes)
}

fn decode_secret(encoded: &str) -> Result<String, String> {
    if let Some(encoded) = encoded.strip_prefix(SECRET_PREFIX_V2) {
        return decode_secret_with_key(encoded, &current_secret_key());
    }
    decode_secret_with_key(encoded, &legacy_secret_key())
}

fn decode_secret_with_key(encoded: &str, key: &[u8]) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|err| format!("failed to decode saved secret: {err}"))?
        .into_iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect::<Vec<_>>();
    String::from_utf8(bytes).map_err(|err| format!("saved secret is not valid utf-8: {err}"))
}

fn current_secret_key() -> Vec<u8> {
    secret_key(b"android-protector-v1")
}

fn legacy_secret_key() -> Vec<u8> {
    let domain = ["android-", "third", "gen-protector-v1"].concat();
    secret_key(domain.as_bytes())
}

fn secret_key(domain: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    if let Ok(value) = std::env::var("USERNAME").or_else(|_| std::env::var("USER")) {
        hasher.update(value.as_bytes());
    }
    if let Ok(value) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscated_secret_roundtrips() {
        let encoded = encode_secret("password");
        assert_ne!(encoded, "password");
        assert!(encoded.starts_with(SECRET_PREFIX_V2));
        assert_eq!(decode_secret(&encoded).unwrap(), "password");
    }

    #[test]
    fn legacy_obfuscated_secret_decodes() {
        let encoded = encode_secret_with_key("password", &legacy_secret_key());
        assert_eq!(decode_secret(&encoded).unwrap(), "password");
    }
}
