use crate::crypto;
use crate::jobs::{JobCanceled, JobReporter};
use crate::models::{ArtifactInfo, ArtifactKind, ProtectRequest, SigningConfig};
use crate::{scan, toolchain, vmp};
use chrono::Utc;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct ProtectionError {
    pub stage: String,
    pub message: String,
}

impl ProtectionError {
    fn new(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProtectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.stage, self.message)
    }
}

impl std::error::Error for ProtectionError {}

impl From<JobCanceled> for ProtectionError {
    fn from(_: JobCanceled) -> Self {
        ProtectionError::new("canceled", "job canceled")
    }
}

pub fn run_protection(
    request: ProtectRequest,
    reporter: &JobReporter,
) -> Result<String, ProtectionError> {
    reporter.stage("scan", 0.05, "scanning input artifact")?;
    let input = PathBuf::from(&request.input_path);
    let artifact = scan::scan_artifact(&input).map_err(|err| ProtectionError::new("scan", err))?;
    let kind = request.artifact_kind.unwrap_or(artifact.kind);
    if kind == ArtifactKind::Unknown {
        return Err(ProtectionError::new("scan", "unsupported artifact type"));
    }

    reporter.stage(
        "toolchain",
        0.12,
        "detecting Android signing and packaging tools",
    )?;
    let toolchain = toolchain::detect_toolchain(request.toolchain_paths.as_ref());
    for issue in &toolchain.issues {
        reporter.log("toolchain", issue);
    }

    reporter.stage("vmp-transform", 0.25, "planning optional VMP transform")?;
    let vmp_manifest = if request.vmp_options.enabled {
        let manifest = vmp::build_vmp_manifest(&request)
            .map_err(|err| ProtectionError::new("vmp-transform", err))?;
        reporter.log(
            "vmp-transform",
            &format!(
                "selected {} methods for VMP",
                manifest.selected_methods.len()
            ),
        );
        manifest
    } else {
        reporter.log("vmp-transform", "VMP disabled");
        vmp::VmpManifest {
            enabled: false,
            vm_version: "dex-bytecode-vm-v1".to_string(),
            selected_methods: Vec::new(),
            skipped_reasons: Vec::new(),
        }
    };

    reporter.stage("dex-encrypt", 0.42, "building encrypted DEX/VMP payload")?;
    let encrypted_payload = build_encrypted_dex_payload(&input, kind, &artifact)
        .map_err(|err| ProtectionError::new("dex-encrypt", err))?;

    reporter.stage(
        "package",
        0.58,
        "rewriting artifact with protector metadata",
    )?;
    let output_dir = resolve_output_dir(&request, &input)?;
    fs::create_dir_all(&output_dir).map_err(|err| {
        ProtectionError::new("package", format!("failed to create output dir: {err}"))
    })?;
    let temp = tempdir().map_err(|err| {
        ProtectionError::new("package", format!("failed to create temp dir: {err}"))
    })?;
    let raw_output = temp.path().join(output_file_name(&input, kind, "raw"));
    let final_output = output_dir.join(output_file_name(&input, kind, "protected"));

    let protection_manifest = ProtectionManifest {
        format_version: 1,
        created_at: Utc::now().to_rfc3339(),
        source_file: input
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| input.display().to_string()),
        kind,
        features: ProtectionFeatures {
            vmp: request.vmp_options.enabled,
            dex_encryption: request.protection_options.dex_encryption,
            anti_debug: request.protection_options.anti_debug,
            signature_tamper_check: request.protection_options.signature_tamper_check,
            legacy_api_fallback: request.protection_options.legacy_api_fallback,
        },
        original_dex_files: artifact
            .dex_files
            .iter()
            .map(|dex| dex.name.clone())
            .collect(),
        native_loader: NativeLoaderPlan {
            java_entrypoint: "com.protector.runtime.ProtectorApplication".to_string(),
            native_library: "protector_vm".to_string(),
            status:
                "source-included; binary injection boundary is isolated for loader build artifacts"
                    .to_string(),
        },
    };

    let metadata_prefix = metadata_prefix(kind);
    let metadata_entries = vec![
        (
            format!("{metadata_prefix}/protection-manifest.json"),
            serde_json::to_vec_pretty(&protection_manifest)
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
        (
            format!("{metadata_prefix}/vmp-plan.json"),
            serde_json::to_vec_pretty(&vmp_manifest)
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
        (
            format!("{metadata_prefix}/dex-payload.json"),
            serde_json::to_vec_pretty(&encrypted_payload)
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
    ];
    rewrite_zip(&input, &raw_output, kind, &metadata_entries)?;

    reporter.stage("sign", 0.76, "aligning and signing output")?;
    let signed_or_unsigned = sign_or_copy_output(
        &raw_output,
        &final_output,
        kind,
        &request,
        &toolchain,
        reporter,
    )?;

    reporter.stage("verify", 0.9, "verifying signed artifact")?;
    verify_output(&signed_or_unsigned, kind, &toolchain, reporter)?;

    reporter.stage("complete", 1.0, "output artifact ready")?;
    Ok(signed_or_unsigned.display().to_string())
}

fn build_encrypted_dex_payload(
    input: &Path,
    kind: ArtifactKind,
    artifact: &ArtifactInfo,
) -> Result<crypto::EncryptedPayload, String> {
    let file = File::open(input).map_err(|err| format!("failed to open artifact: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("failed to read zip: {err}"))?;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|err| format!("failed to read zip entry #{index}: {err}"))?;
            let name = entry.name().replace('\\', "/");
            if !scan::is_dex_entry(&name, kind) {
                continue;
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|err| format!("failed to read dex {name}: {err}"))?;
            writer
                .start_file(name, options)
                .map_err(|err| format!("failed to create payload zip: {err}"))?;
            writer
                .write_all(&bytes)
                .map_err(|err| format!("failed to write payload zip: {err}"))?;
        }
        writer
            .finish()
            .map_err(|err| format!("failed to finish payload zip: {err}"))?;
    }

    let context = serde_json::to_vec(&artifact.dex_files).unwrap_or_default();
    crypto::encrypt_bytes(cursor.get_ref(), &context)
}

fn rewrite_zip(
    input: &Path,
    output: &Path,
    kind: ArtifactKind,
    metadata_entries: &[(String, Vec<u8>)],
) -> Result<(), ProtectionError> {
    let input_file = File::open(input)
        .map_err(|err| ProtectionError::new("package", format!("failed to open input: {err}")))?;
    let mut archive = ZipArchive::new(input_file)
        .map_err(|err| ProtectionError::new("package", format!("failed to read zip: {err}")))?;
    let output_file = File::create(output).map_err(|err| {
        ProtectionError::new("package", format!("failed to create output: {err}"))
    })?;
    let mut writer = ZipWriter::new(output_file);

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            ProtectionError::new("package", format!("failed to read entry #{index}: {err}"))
        })?;
        let name = entry.name().replace('\\', "/");
        if scan::is_signature_entry(&name) || is_existing_protector_entry(&name, kind) {
            continue;
        }
        let options = SimpleFileOptions::default()
            .compression_method(entry.compression())
            .unix_permissions(entry.unix_mode().unwrap_or(0o644));
        if entry.is_dir() {
            writer.add_directory(name, options).map_err(|err| {
                ProtectionError::new("package", format!("failed to add directory: {err}"))
            })?;
        } else {
            writer.start_file(name, options).map_err(|err| {
                ProtectionError::new("package", format!("failed to add file: {err}"))
            })?;
            std::io::copy(&mut entry, &mut writer).map_err(|err| {
                ProtectionError::new("package", format!("failed to copy file: {err}"))
            })?;
        }
    }

    let metadata_options =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in metadata_entries {
        writer.start_file(name, metadata_options).map_err(|err| {
            ProtectionError::new("package", format!("failed to add metadata {name}: {err}"))
        })?;
        writer.write_all(bytes).map_err(|err| {
            ProtectionError::new("package", format!("failed to write metadata {name}: {err}"))
        })?;
    }
    writer.finish().map_err(|err| {
        ProtectionError::new("package", format!("failed to finalize output zip: {err}"))
    })?;
    Ok(())
}

fn sign_or_copy_output(
    raw_output: &Path,
    final_output: &Path,
    kind: ArtifactKind,
    request: &ProtectRequest,
    toolchain: &crate::models::ToolchainStatus,
    reporter: &JobReporter,
) -> Result<PathBuf, ProtectionError> {
    match kind {
        ArtifactKind::Apk => {
            let aligned_output = raw_output.with_extension("aligned.apk");
            if let Some(zipalign) = toolchain::command_path(&toolchain.zipalign) {
                let mut command = toolchain::command_for_tool(&zipalign);
                command.args([
                    "-p",
                    "-f",
                    "4",
                    &raw_output.display().to_string(),
                    &aligned_output.display().to_string(),
                ]);
                run_command("sign", &mut command)?;
            } else {
                fs::copy(raw_output, &aligned_output).map_err(|err| {
                    ProtectionError::new(
                        "sign",
                        format!("zipalign unavailable and copy failed: {err}"),
                    )
                })?;
                reporter.log("sign", "zipalign unavailable; copied APK without alignment");
            }

            if let Some(signing) = request
                .signing_config
                .as_ref()
                .filter(|cfg| !cfg.keystore_path.is_empty())
            {
                let apksigner = toolchain::command_path(&toolchain.apksigner).ok_or_else(|| {
                    ProtectionError::new("sign", "apksigner is required for APK signing")
                })?;
                sign_apk(&apksigner, &aligned_output, final_output, signing)?;
                Ok(final_output.to_path_buf())
            } else {
                fs::copy(&aligned_output, final_output).map_err(|err| {
                    ProtectionError::new("sign", format!("failed to copy unsigned APK: {err}"))
                })?;
                reporter.log("sign", "no signing config supplied; output APK is unsigned");
                Ok(final_output.to_path_buf())
            }
        }
        ArtifactKind::Aab => {
            if let Some(signing) = request
                .signing_config
                .as_ref()
                .filter(|cfg| !cfg.keystore_path.is_empty())
            {
                let jarsigner = toolchain::command_path(&toolchain.jarsigner).ok_or_else(|| {
                    ProtectionError::new("sign", "jarsigner is required for AAB signing")
                })?;
                sign_aab(&jarsigner, raw_output, final_output, signing)?;
            } else {
                fs::copy(raw_output, final_output).map_err(|err| {
                    ProtectionError::new("sign", format!("failed to copy unsigned AAB: {err}"))
                })?;
                reporter.log("sign", "no signing config supplied; output AAB is unsigned");
            }
            Ok(final_output.to_path_buf())
        }
        ArtifactKind::Unknown => Err(ProtectionError::new("sign", "unsupported artifact type")),
    }
}

fn sign_apk(
    apksigner: &str,
    input: &Path,
    output: &Path,
    signing: &SigningConfig,
) -> Result<(), ProtectionError> {
    let mut command = build_apk_sign_command(apksigner, input, output, signing);
    run_command("sign", &mut command)
}

fn build_apk_sign_command(
    apksigner: &str,
    input: &Path,
    output: &Path,
    signing: &SigningConfig,
) -> Command {
    let mut command = toolchain::command_for_tool(apksigner);
    command
        .arg("sign")
        .arg("--ks")
        .arg(&signing.keystore_path)
        .arg("--ks-key-alias")
        .arg(&signing.alias)
        .arg("--ks-pass")
        .arg("env:ANDROID_PROTECTOR_KS_PASS")
        .env("ANDROID_PROTECTOR_KS_PASS", &signing.store_password);
    if let Some(key_password) = signing
        .key_password
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        command
            .arg("--key-pass")
            .arg("env:ANDROID_PROTECTOR_KEY_PASS")
            .env("ANDROID_PROTECTOR_KEY_PASS", key_password);
    }
    if let Some(store_type) = signing
        .store_type
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        command.arg("--ks-type").arg(store_type);
    }
    command.arg("--out").arg(output).arg(input);
    command
}

fn sign_aab(
    jarsigner: &str,
    input: &Path,
    output: &Path,
    signing: &SigningConfig,
) -> Result<(), ProtectionError> {
    let mut command = build_aab_sign_command(jarsigner, input, output, signing);
    run_command("sign", &mut command)
}

fn build_aab_sign_command(
    jarsigner: &str,
    input: &Path,
    output: &Path,
    signing: &SigningConfig,
) -> Command {
    let mut command = toolchain::command_for_tool(jarsigner);
    command
        .arg("-keystore")
        .arg(&signing.keystore_path)
        .arg("-storepass")
        .arg(&signing.store_password);
    if let Some(key_password) = signing
        .key_password
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        command.arg("-keypass").arg(key_password);
    }
    if let Some(store_type) = signing
        .store_type
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        command.arg("-storetype").arg(store_type);
    }
    command
        .arg("-signedjar")
        .arg(output)
        .arg(input)
        .arg(&signing.alias);
    command
}

fn verify_output(
    output: &Path,
    kind: ArtifactKind,
    toolchain: &crate::models::ToolchainStatus,
    reporter: &JobReporter,
) -> Result<(), ProtectionError> {
    match kind {
        ArtifactKind::Apk => {
            if let Some(apksigner) = toolchain::command_path(&toolchain.apksigner) {
                let mut command = toolchain::command_for_tool(&apksigner);
                command.args(["verify", "--verbose", &output.display().to_string()]);
                run_command("verify", &mut command)?;
            } else {
                reporter.log(
                    "verify",
                    "apksigner unavailable; skipped APK signature verification",
                );
            }
        }
        ArtifactKind::Aab => {
            if let Some(bundletool) = toolchain::command_path(&toolchain.bundletool) {
                if bundletool.ends_with(".jar") {
                    let mut command = toolchain::command_for_tool("java");
                    command.args([
                        "-jar",
                        &bundletool,
                        "validate",
                        "--bundle",
                        &output.display().to_string(),
                    ]);
                    run_command("verify", &mut command)?;
                } else {
                    let mut command = toolchain::command_for_tool(&bundletool);
                    command.args(["validate", "--bundle", &output.display().to_string()]);
                    run_command("verify", &mut command)?;
                }
            } else if let Some(jarsigner) = toolchain::command_path(&toolchain.jarsigner) {
                let mut command = toolchain::command_for_tool(&jarsigner);
                command.args(["-verify", "-certs", &output.display().to_string()]);
                run_command("verify", &mut command)?;
            } else {
                reporter.log(
                    "verify",
                    "bundletool/jarsigner unavailable; skipped AAB verification",
                );
            }
        }
        ArtifactKind::Unknown => {
            return Err(ProtectionError::new("verify", "unsupported artifact type"))
        }
    }
    Ok(())
}

fn run_command(stage: &str, command: &mut Command) -> Result<(), ProtectionError> {
    let output = command
        .output()
        .map_err(|err| ProtectionError::new(stage, format!("failed to execute command: {err}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ProtectionError::new(stage, summarize_output(&output)))
    }
}

fn summarize_output(output: &Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let summary = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(12)
        .collect::<Vec<_>>()
        .join("\n");
    if summary.is_empty() {
        format!("command failed with status {}", output.status)
    } else {
        summary
    }
}

fn resolve_output_dir(request: &ProtectRequest, input: &Path) -> Result<PathBuf, ProtectionError> {
    if !request.output_dir.trim().is_empty() {
        return Ok(PathBuf::from(&request.output_dir));
    }
    input
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ProtectionError::new("package", "input has no parent directory"))
}

fn output_file_name(input: &Path, kind: ArtifactKind, suffix: &str) -> String {
    let stem = input
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let ext = match kind {
        ArtifactKind::Apk => "apk",
        ArtifactKind::Aab => "aab",
        ArtifactKind::Unknown => "zip",
    };
    format!("{stem}.{suffix}.{ext}")
}

fn metadata_prefix(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Apk => "assets/protector",
        ArtifactKind::Aab => "base/assets/protector",
        ArtifactKind::Unknown => "assets/protector",
    }
}

fn is_existing_protector_entry(name: &str, kind: ArtifactKind) -> bool {
    name.starts_with(metadata_prefix(kind))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtectionManifest {
    format_version: u32,
    created_at: String,
    source_file: String,
    kind: ArtifactKind,
    features: ProtectionFeatures,
    original_dex_files: Vec<String>,
    native_loader: NativeLoaderPlan,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtectionFeatures {
    vmp: bool,
    dex_encryption: bool,
    anti_debug: bool,
    signature_tamper_check: bool,
    legacy_api_fallback: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeLoaderPlan {
    java_entrypoint: String,
    native_library: String,
    status: String,
}

#[cfg(test)]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn arg_index(args: &[String], value: &str) -> usize {
        args.iter()
            .position(|arg| arg == value)
            .unwrap_or_else(|| panic!("missing command argument: {value}"))
    }

    fn test_signing_config() -> SigningConfig {
        SigningConfig {
            keystore_path: "release.jks".to_string(),
            store_password: "store-pass".to_string(),
            key_password: Some("key-pass".to_string()),
            alias: "release".to_string(),
            store_type: Some("JKS".to_string()),
        }
    }

    #[test]
    fn output_names_keep_artifact_extension() {
        assert_eq!(
            output_file_name(Path::new("release.apk"), ArtifactKind::Apk, "protected"),
            "release.protected.apk"
        );
        assert_eq!(
            output_file_name(Path::new("release.aab"), ArtifactKind::Aab, "raw"),
            "release.raw.aab"
        );
    }

    #[test]
    fn metadata_prefix_matches_artifact_layout() {
        assert_eq!(metadata_prefix(ArtifactKind::Apk), "assets/protector");
        assert_eq!(metadata_prefix(ArtifactKind::Aab), "base/assets/protector");
    }

    #[test]
    fn computes_sha256_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn apk_signing_options_are_before_input_file() {
        let signing = test_signing_config();
        let command = build_apk_sign_command(
            "apksigner",
            Path::new("input.apk"),
            Path::new("output.apk"),
            &signing,
        );
        let args = command_args(&command);
        let input_index = arg_index(&args, "input.apk");

        assert!(arg_index(&args, "--key-pass") < input_index);
        assert!(arg_index(&args, "--ks-type") < input_index);
        assert!(arg_index(&args, "--out") < input_index);
        assert_eq!(args.last().map(String::as_str), Some("input.apk"));
    }

    #[test]
    fn aab_signing_options_are_before_input_file() {
        let signing = test_signing_config();
        let command = build_aab_sign_command(
            "jarsigner",
            Path::new("input.aab"),
            Path::new("output.aab"),
            &signing,
        );
        let args = command_args(&command);
        let input_index = arg_index(&args, "input.aab");

        assert!(arg_index(&args, "-keypass") < input_index);
        assert!(arg_index(&args, "-storetype") < input_index);
        assert!(arg_index(&args, "-signedjar") < input_index);
        assert_eq!(args.last().map(String::as_str), Some("release"));
    }
}
