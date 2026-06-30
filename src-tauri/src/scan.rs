use crate::dex;
use crate::models::{ArtifactInfo, ArtifactKind, DexFileInfo};
use regex::Regex;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

pub fn scan_artifact(path: &Path) -> Result<ArtifactInfo, String> {
    if !path.exists() {
        return Err(format!("input does not exist: {}", path.display()));
    }
    let metadata = path
        .metadata()
        .map_err(|err| format!("failed to read metadata: {err}"))?;
    let kind = artifact_kind_from_path(path);
    if kind == ArtifactKind::Unknown {
        return Err("only .apk and .aab inputs are supported".to_string());
    }

    let file = File::open(path).map_err(|err| format!("failed to open artifact: {err}"))?;
    let mut zip = ZipArchive::new(file).map_err(|err| format!("failed to read zip: {err}"))?;
    let mut dex_files = Vec::new();
    let mut native_abis = BTreeSet::new();
    let mut warnings = Vec::new();
    let mut signed = false;
    let mut signature_schemes = BTreeSet::new();
    let mut manifest_text = None;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("failed to read zip entry #{index}: {err}"))?;
        let name = entry.name().replace('\\', "/");
        if entry.is_dir() {
            continue;
        }

        if is_manifest_entry(&name, kind) && manifest_text.is_none() {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|err| format!("failed to read manifest: {err}"))?;
            manifest_text = Some(String::from_utf8_lossy(&bytes).to_string());
        } else if is_dex_entry(&name, kind) {
            let size = entry.size();
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|err| format!("failed to read dex {name}: {err}"))?;
            match dex::parse_dex(&name, &bytes) {
                Ok(parsed) => {
                    let virtualizable_methods = parsed
                        .methods
                        .iter()
                        .filter(|method| method.skip_reason.is_none())
                        .count() as u32;
                    dex_files.push(DexFileInfo {
                        name: name.clone(),
                        size_bytes: size,
                        method_count: parsed.method_count,
                        class_count: parsed.class_count,
                        virtualizable_methods,
                    });
                }
                Err(err) => {
                    warnings.push(format!("failed to parse {name}: {err}"));
                    dex_files.push(DexFileInfo {
                        name: name.clone(),
                        size_bytes: size,
                        method_count: 0,
                        class_count: 0,
                        virtualizable_methods: 0,
                    });
                }
            }
        } else if let Some(abi) = native_abi_from_entry(&name, kind) {
            native_abis.insert(abi);
        }

        if is_signature_entry(&name) {
            signed = true;
            if name.ends_with(".RSA") {
                signature_schemes.insert("RSA/JAR".to_string());
            } else if name.ends_with(".DSA") {
                signature_schemes.insert("DSA/JAR".to_string());
            } else if name.ends_with(".EC") {
                signature_schemes.insert("EC/JAR".to_string());
            }
        }
    }

    if !signed && kind == ArtifactKind::Apk {
        warnings.push(
            "no JAR signature files found; APK may still use APK Signature Scheme v2+".to_string(),
        );
    }

    let manifest = manifest_text.unwrap_or_default();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    Ok(ArtifactInfo {
        path: path.display().to_string(),
        file_name,
        kind,
        size_bytes: metadata.len(),
        package_name: capture_manifest_attr(&manifest, "package"),
        version_name: capture_manifest_attr(&manifest, "android:versionName")
            .or_else(|| capture_manifest_attr(&manifest, "versionName")),
        version_code: capture_manifest_attr(&manifest, "android:versionCode")
            .or_else(|| capture_manifest_attr(&manifest, "versionCode")),
        application_class: capture_application_name(&manifest),
        min_sdk: capture_manifest_attr(&manifest, "android:minSdkVersion")
            .or_else(|| capture_manifest_attr(&manifest, "minSdkVersion")),
        target_sdk: capture_manifest_attr(&manifest, "android:targetSdkVersion")
            .or_else(|| capture_manifest_attr(&manifest, "targetSdkVersion")),
        dex_files,
        native_abis: native_abis.into_iter().collect(),
        signed,
        signature_schemes: signature_schemes.into_iter().collect(),
        entry_count: zip.len(),
        warnings,
    })
}

pub fn artifact_kind_from_path(path: &Path) -> ArtifactKind {
    match path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("apk") => ArtifactKind::Apk,
        Some("aab") => ArtifactKind::Aab,
        _ => ArtifactKind::Unknown,
    }
}

pub fn is_dex_entry(name: &str, kind: ArtifactKind) -> bool {
    match kind {
        ArtifactKind::Apk => {
            name.starts_with("classes") && name.ends_with(".dex") && !name.contains('/')
        }
        ArtifactKind::Aab => name.starts_with("base/dex/") && name.ends_with(".dex"),
        ArtifactKind::Unknown => false,
    }
}

pub fn is_manifest_entry(name: &str, kind: ArtifactKind) -> bool {
    match kind {
        ArtifactKind::Apk => name == "AndroidManifest.xml",
        ArtifactKind::Aab => name == "base/manifest/AndroidManifest.xml",
        ArtifactKind::Unknown => false,
    }
}

pub fn is_signature_entry(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("META-INF/")
        && (upper.ends_with(".RSA")
            || upper.ends_with(".DSA")
            || upper.ends_with(".EC")
            || upper.ends_with(".SF")
            || upper.ends_with(".MF")
            || upper == "META-INF/MANIFEST.MF")
}

fn native_abi_from_entry(name: &str, kind: ArtifactKind) -> Option<String> {
    let parts: Vec<&str> = name.split('/').collect();
    match kind {
        ArtifactKind::Apk if parts.len() >= 3 && parts[0] == "lib" => Some(parts[1].to_string()),
        ArtifactKind::Aab if parts.len() >= 4 && parts[0] == "base" && parts[1] == "lib" => {
            Some(parts[2].to_string())
        }
        _ => None,
    }
}

fn capture_manifest_attr(manifest_text: &str, attr: &str) -> Option<String> {
    if manifest_text.is_empty() {
        return None;
    }
    let pattern = format!(r#"{}\s*=\s*["']([^"']+)["']"#, regex::escape(attr));
    Regex::new(&pattern)
        .ok()
        .and_then(|regex| regex.captures(manifest_text))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn capture_application_name(manifest_text: &str) -> Option<String> {
    if manifest_text.is_empty() {
        return None;
    }
    Regex::new(r#"<application\b[^>]*(?:android:)?name\s*=\s*["']([^"']+)["']"#)
        .ok()
        .and_then(|regex| regex.captures(manifest_text))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_artifact_kind_from_extension() {
        assert_eq!(
            artifact_kind_from_path(&PathBuf::from("app.apk")),
            ArtifactKind::Apk
        );
        assert_eq!(
            artifact_kind_from_path(&PathBuf::from("app.aab")),
            ArtifactKind::Aab
        );
        assert_eq!(
            artifact_kind_from_path(&PathBuf::from("app.zip")),
            ArtifactKind::Unknown
        );
    }

    #[test]
    fn detects_dex_entries_by_artifact_type() {
        assert!(is_dex_entry("classes.dex", ArtifactKind::Apk));
        assert!(is_dex_entry("classes2.dex", ArtifactKind::Apk));
        assert!(!is_dex_entry("assets/classes.dex", ArtifactKind::Apk));
        assert!(is_dex_entry("base/dex/classes.dex", ArtifactKind::Aab));
    }

    #[test]
    fn captures_plain_manifest_attributes() {
        let manifest = r#"<manifest package="com.example" android:versionName="1.0"><application android:name=".App" /></manifest>"#;
        assert_eq!(
            capture_manifest_attr(manifest, "package").as_deref(),
            Some("com.example")
        );
        assert_eq!(capture_application_name(manifest).as_deref(), Some(".App"));
    }
}
