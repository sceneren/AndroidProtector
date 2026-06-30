use crate::crypto;
use crate::jobs::{JobCanceled, JobReporter};
use crate::models::{ArtifactInfo, ArtifactKind, ProtectRequest, SigningConfig, SigningScheme};
use crate::{channel, loader, manifest, scan, toolchain, vmp};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashSet;
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

    let metadata_prefix = metadata_prefix(kind);
    let payload_file = format!("{metadata_prefix}/dex-payload.bin");
    reporter.stage("dex-encrypt", 0.42, "encrypting original DEX payload")?;
    let encrypted_payload = build_encrypted_dex_payload(&input, kind, &artifact, &payload_file)
        .map_err(|err| ProtectionError::new("dex-encrypt", err))?;
    reporter.log(
        "dex-encrypt",
        &format!(
            "encrypted {} original DEX files into {} bytes",
            artifact.dex_files.len(),
            encrypted_payload.metadata.ciphertext_len
        ),
    );
    let loader_artifact = ArtifactInfo {
        dex_files: if request.protection_options.dex_encryption {
            Vec::new()
        } else {
            artifact.dex_files.clone()
        },
        ..artifact.clone()
    };
    let loader_plan = loader::build_loader_injection_plan(kind, &loader_artifact);
    if loader_plan.dex_targets.is_empty() {
        for issue in &loader_plan.issues {
            reporter.log("package", issue);
        }
        return Err(ProtectionError::new(
            "package",
            "loader dex not found; cannot patch AndroidManifest to com.protector.runtime.ProtectorApplication",
        ));
    }
    if loader_plan.native_targets.is_empty() {
        reporter.log(
            "package",
            "native loader artifacts unavailable; Java loader will start in compatibility mode",
        );
    } else {
        reporter.log(
            "package",
            &format!(
                "loader injection planned: {} dex, {} native libraries",
                loader_plan.dex_targets.len(),
                loader_plan.native_targets.len()
            ),
        );
    }
    for issue in &loader_plan.issues {
        reporter.log("package", issue);
    }
    let manifest_patch = manifest::patch_manifest_in_artifact(&input, kind)
        .map_err(|err| ProtectionError::new("manifest-patch", err))?;
    reporter.log(
        "manifest-patch",
        &format!(
            "{}; original application: {}",
            manifest_patch.status,
            manifest_patch
                .original_application
                .as_deref()
                .unwrap_or("none")
        ),
    );
    for issue in &manifest_patch.issues {
        reporter.log("manifest-patch", issue);
    }

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
            dex_encryption: true,
            anti_debug: request.protection_options.anti_debug,
            signature_tamper_check: request.protection_options.signature_tamper_check,
            legacy_api_fallback: request.protection_options.legacy_api_fallback,
        },
        original_dex_files: artifact
            .dex_files
            .iter()
            .map(|dex| dex.name.clone())
            .collect(),
        manifest_patch: ManifestPatchPlan {
            status: manifest_patch.status.clone(),
            package_name: manifest_patch.package_name.clone(),
            original_application: manifest_patch.original_application.clone(),
            protector_application: manifest_patch.protector_application.clone(),
            issues: manifest_patch.issues.clone(),
        },
        native_loader: NativeLoaderPlan {
            java_entrypoint: "com.protector.runtime.ProtectorApplication".to_string(),
            native_library: "protector_vm".to_string(),
            status: loader_plan.status(),
            dex_files: loader_plan.dex_targets.clone(),
            native_libraries: loader_plan.native_targets.clone(),
            issues: loader_plan.issues.clone(),
        },
    };

    let metadata_entries = vec![
        (
            format!("{metadata_prefix}/protection-manifest.json"),
            serde_json::to_vec_pretty(&protection_manifest)
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
        (
            format!("{metadata_prefix}/vmp-plan.json"),
            serde_json::to_vec_pretty(&CompactVmpManifest::from_manifest(&vmp_manifest))
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
        (
            format!("{metadata_prefix}/dex-payload.json"),
            serde_json::to_vec_pretty(&encrypted_payload.metadata)
                .map_err(|err| ProtectionError::new("package", err.to_string()))?,
        ),
        (payload_file, encrypted_payload.ciphertext),
    ];
    rewrite_zip(
        &input,
        &raw_output,
        kind,
        &metadata_entries,
        &loader_plan.files,
        Some(&manifest_patch),
        true,
    )?;

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

    if request.channel_options.enabled {
        reporter.stage("channels", 0.96, "building multi-channel APK packages")?;
        if kind != ArtifactKind::Apk {
            return Err(ProtectionError::new(
                "channels",
                "multi-channel packaging only supports APK artifacts",
            ));
        }
        let channel_result =
            channel::write_channel_packages(&signed_or_unsigned, &request.channel_options.channels)
                .map_err(|err| ProtectionError::new("channels", err))?;
        reporter.log(
            "channels",
            &format!(
                "created {} channel packages under {}",
                channel_result.packages.len(),
                channel_result.output_dir
            ),
        );
        for package in channel_result.packages {
            reporter.log(
                "channels",
                &format!("{} => {}", package.channel, package.path),
            );
        }
    }

    reporter.stage("complete", 1.0, "output artifact ready")?;
    Ok(signed_or_unsigned.display().to_string())
}

fn build_encrypted_dex_payload(
    input: &Path,
    kind: ArtifactKind,
    artifact: &ArtifactInfo,
    payload_file: &str,
) -> Result<crypto::EncryptedBytes, String> {
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
                .map_err(|err| format!("failed to create dex payload zip: {err}"))?;
            writer
                .write_all(&bytes)
                .map_err(|err| format!("failed to write dex payload zip: {err}"))?;
        }
        writer
            .finish()
            .map_err(|err| format!("failed to finish dex payload zip: {err}"))?;
    }

    let context = serde_json::to_vec(&artifact.dex_files).unwrap_or_default();
    crypto::encrypt_bytes(cursor.get_ref(), &context, payload_file)
}

fn rewrite_zip(
    input: &Path,
    output: &Path,
    kind: ArtifactKind,
    metadata_entries: &[(String, Vec<u8>)],
    injected_entries: &[loader::LoaderInjectionFile],
    manifest_patch: Option<&manifest::ManifestPatch>,
    remove_original_dex: bool,
) -> Result<(), ProtectionError> {
    let input_file = File::open(input)
        .map_err(|err| ProtectionError::new("package", format!("failed to open input: {err}")))?;
    let mut archive = ZipArchive::new(input_file)
        .map_err(|err| ProtectionError::new("package", format!("failed to read zip: {err}")))?;
    let output_file = File::create(output).map_err(|err| {
        ProtectionError::new("package", format!("failed to create output: {err}"))
    })?;
    let mut writer = ZipWriter::new(output_file);
    let injected_targets = injected_entries
        .iter()
        .map(|entry| entry.target.as_str())
        .collect::<HashSet<_>>();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            ProtectionError::new("package", format!("failed to read entry #{index}: {err}"))
        })?;
        let name = entry.name().replace('\\', "/");
        if scan::is_signature_entry(&name)
            || is_existing_protector_entry(&name, kind)
            || injected_targets.contains(name.as_str())
            || (remove_original_dex && scan::is_dex_entry(&name, kind))
        {
            continue;
        }
        let options = SimpleFileOptions::default()
            .compression_method(entry.compression())
            .unix_permissions(entry.unix_mode().unwrap_or(0o644));
        if manifest_patch
            .as_ref()
            .is_some_and(|patch| patch.entry_name == name)
        {
            writer.start_file(name, options).map_err(|err| {
                ProtectionError::new("package", format!("failed to add patched manifest: {err}"))
            })?;
            writer
                .write_all(&manifest_patch.expect("checked manifest patch").bytes)
                .map_err(|err| {
                    ProtectionError::new(
                        "package",
                        format!("failed to write patched manifest: {err}"),
                    )
                })?;
            continue;
        }
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
    for entry in injected_entries {
        let bytes = entry
            .read_bytes()
            .map_err(|err| ProtectionError::new("package", err))?;
        let compression = match entry.kind {
            loader::LoaderArtifactKind::Dex => CompressionMethod::Deflated,
            loader::LoaderArtifactKind::NativeLibrary => CompressionMethod::Stored,
        };
        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .unix_permissions(0o644);
        writer
            .start_file(entry.target.as_str(), options)
            .map_err(|err| {
                ProtectionError::new(
                    "package",
                    format!(
                        "failed to add loader artifact {} from {}: {err}",
                        entry.target,
                        entry.source_label()
                    ),
                )
            })?;
        writer.write_all(&bytes).map_err(|err| {
            ProtectionError::new(
                "package",
                format!(
                    "failed to write loader artifact {} from {}: {err}",
                    entry.target,
                    entry.source_label()
                ),
            )
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
    let enable_v3 = matches!(signing.signing_scheme, SigningScheme::V1V2V3);
    command
        .arg("--v1-signing-enabled")
        .arg("true")
        .arg("--v2-signing-enabled")
        .arg("true")
        .arg("--v3-signing-enabled")
        .arg(if enable_v3 { "true" } else { "false" });
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
struct CompactVmpManifest {
    enabled: bool,
    vm_version: String,
    selected_method_count: usize,
    skipped_method_count: u32,
    selected_method_samples: Vec<vmp::VmpMethodEntry>,
    skipped_reasons: Vec<crate::models::SkipReason>,
}

impl CompactVmpManifest {
    fn from_manifest(manifest: &vmp::VmpManifest) -> Self {
        Self {
            enabled: manifest.enabled,
            vm_version: manifest.vm_version.clone(),
            selected_method_count: manifest.selected_methods.len(),
            skipped_method_count: manifest
                .skipped_reasons
                .iter()
                .map(|reason| reason.count)
                .sum(),
            selected_method_samples: manifest.selected_methods.iter().take(50).cloned().collect(),
            skipped_reasons: manifest.skipped_reasons.clone(),
        }
    }
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
    manifest_patch: ManifestPatchPlan,
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
struct ManifestPatchPlan {
    status: String,
    package_name: Option<String>,
    original_application: Option<String>,
    protector_application: String,
    issues: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeLoaderPlan {
    java_entrypoint: String,
    native_library: String,
    status: String,
    dex_files: Vec<String>,
    native_libraries: Vec<String>,
    issues: Vec<String>,
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
    use std::io::Read;

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
            signing_scheme: SigningScheme::V1V2,
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
    fn compact_vmp_manifest_caps_selected_method_samples() {
        let selected_methods = (0..60)
            .map(|index| vmp::VmpMethodEntry {
                dex_name: "classes.dex".to_string(),
                class_descriptor: format!("Lcom/example/C{index};"),
                method_name: "run".to_string(),
                access_flags: 0,
                code_units: 1,
            })
            .collect();
        let manifest = vmp::VmpManifest {
            enabled: true,
            vm_version: "test".to_string(),
            selected_methods,
            skipped_reasons: Vec::new(),
        };

        let compact = CompactVmpManifest::from_manifest(&manifest);

        assert_eq!(compact.selected_method_count, 60);
        assert_eq!(compact.selected_method_samples.len(), 50);
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
        assert!(arg_index(&args, "--v1-signing-enabled") < input_index);
        assert!(arg_index(&args, "--v2-signing-enabled") < input_index);
        assert!(arg_index(&args, "--v3-signing-enabled") < input_index);
        assert_eq!(
            args[arg_index(&args, "--v3-signing-enabled") + 1].as_str(),
            "false"
        );
        assert!(arg_index(&args, "--out") < input_index);
        assert_eq!(args.last().map(String::as_str), Some("input.apk"));
    }

    #[test]
    fn apk_signing_can_enable_v3() {
        let mut signing = test_signing_config();
        signing.signing_scheme = SigningScheme::V1V2V3;
        let command = build_apk_sign_command(
            "apksigner",
            Path::new("input.apk"),
            Path::new("output.apk"),
            &signing,
        );
        let args = command_args(&command);

        assert_eq!(
            args[arg_index(&args, "--v3-signing-enabled") + 1].as_str(),
            "true"
        );
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

    #[test]
    fn rewrite_zip_injects_loader_artifacts_and_replaces_duplicate_targets() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.apk");
        let output = temp.path().join("output.apk");
        let loader_dex = temp.path().join("loader.dex");
        let loader_so = temp.path().join("libprotector_vm.so");
        fs::write(&loader_dex, b"loader-dex").unwrap();
        fs::write(&loader_so, b"loader-so").unwrap();

        {
            let file = File::create(&input).unwrap();
            let mut writer = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file("classes.dex", options).unwrap();
            writer.write_all(b"app-dex").unwrap();
            writer.start_file("classes2.dex", options).unwrap();
            writer.write_all(b"old-loader").unwrap();
            writer.start_file("META-INF/CERT.RSA", options).unwrap();
            writer.write_all(b"signature").unwrap();
            writer.finish().unwrap();
        }

        let injected = vec![
            loader::LoaderInjectionFile {
                source: loader::LoaderArtifactSource::File(loader_dex),
                target: "classes2.dex".to_string(),
                kind: loader::LoaderArtifactKind::Dex,
            },
            loader::LoaderInjectionFile {
                source: loader::LoaderArtifactSource::File(loader_so),
                target: "lib/arm64-v8a/libprotector_vm.so".to_string(),
                kind: loader::LoaderArtifactKind::NativeLibrary,
            },
        ];
        rewrite_zip(
            &input,
            &output,
            ArtifactKind::Apk,
            &[("assets/protector/test.json".to_string(), b"{}".to_vec())],
            &injected,
            None,
            false,
        )
        .unwrap();

        let file = File::open(output).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive.by_name("META-INF/CERT.RSA").is_err());
        assert_eq!(
            read_zip_entry(&mut archive, "classes.dex"),
            b"app-dex".to_vec()
        );
        assert_eq!(
            read_zip_entry(&mut archive, "classes2.dex"),
            b"loader-dex".to_vec()
        );
        assert_eq!(
            read_zip_entry(&mut archive, "lib/arm64-v8a/libprotector_vm.so"),
            b"loader-so".to_vec()
        );
        assert_eq!(
            read_zip_entry(&mut archive, "assets/protector/test.json"),
            b"{}".to_vec()
        );
    }

    #[test]
    fn rewrite_zip_can_remove_original_dex_files() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.apk");
        let output = temp.path().join("output.apk");
        let loader_dex = temp.path().join("loader.dex");
        fs::write(&loader_dex, b"loader-dex").unwrap();

        {
            let file = File::create(&input).unwrap();
            let mut writer = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file("classes.dex", options).unwrap();
            writer.write_all(b"business-dex").unwrap();
            writer.start_file("classes2.dex", options).unwrap();
            writer.write_all(b"business-dex-2").unwrap();
            writer.finish().unwrap();
        }

        let injected = vec![loader::LoaderInjectionFile {
            source: loader::LoaderArtifactSource::File(loader_dex),
            target: "classes.dex".to_string(),
            kind: loader::LoaderArtifactKind::Dex,
        }];
        rewrite_zip(
            &input,
            &output,
            ArtifactKind::Apk,
            &[(
                "assets/protector/dex-payload.bin".to_string(),
                b"cipher".to_vec(),
            )],
            &injected,
            None,
            true,
        )
        .unwrap();

        let file = File::open(output).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert_eq!(
            read_zip_entry(&mut archive, "classes.dex"),
            b"loader-dex".to_vec()
        );
        assert!(archive.by_name("classes2.dex").is_err());
        assert_eq!(
            read_zip_entry(&mut archive, "assets/protector/dex-payload.bin"),
            b"cipher".to_vec()
        );
    }

    fn read_zip_entry(archive: &mut ZipArchive<File>, name: &str) -> Vec<u8> {
        let mut entry = archive.by_name(name).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        bytes
    }
}
