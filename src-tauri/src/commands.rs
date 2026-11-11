use crate::jobs::{self, AppState, JobReporter};
use crate::models::{
    ApkSigningResult, AppPreferences, ArtifactInfo, JobStatus, ProtectRequest,
    SigningAliasInspection, SigningConfig, SigningProfileInput, SigningValidation, ToolchainPaths,
    ToolchainStatus, VmpPlan,
};
use crate::{channel, protect, scan, settings, signing, toolchain, vmp};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::State;
use tempfile::tempdir;
use uuid::Uuid;

#[tauri::command]
pub fn detect_toolchain(paths: Option<ToolchainPaths>) -> ToolchainStatus {
    toolchain::detect_toolchain(paths.as_ref())
}

#[tauri::command]
pub fn scan_artifact(path: String) -> Result<ArtifactInfo, String> {
    scan::scan_artifact(Path::new(&path))
}

#[tauri::command]
pub fn estimate_vmp(request: ProtectRequest) -> Result<VmpPlan, String> {
    vmp::estimate_vmp(&request)
}

#[tauri::command]
pub fn validate_signing(config: SigningConfig) -> SigningValidation {
    signing::validate_signing_config(&config)
}

#[tauri::command]
pub fn inspect_signing_aliases(config: SigningConfig) -> SigningAliasInspection {
    signing::inspect_aliases(&config)
}

#[tauri::command]
pub fn load_app_preferences() -> Result<AppPreferences, String> {
    settings::load_preferences()
}

#[tauri::command]
pub fn save_signing_profile(input: SigningProfileInput) -> Result<AppPreferences, String> {
    settings::save_signing_profile(input)
}

#[tauri::command]
pub fn get_signing_profile_input(id: String) -> Result<SigningProfileInput, String> {
    settings::signing_profile_input(&id)?.ok_or_else(|| format!("signing profile not found: {id}"))
}

#[tauri::command]
pub fn delete_signing_profile(id: String) -> Result<AppPreferences, String> {
    settings::delete_signing_profile(id)
}

#[tauri::command]
pub fn set_selected_signing_profile(id: Option<String>) -> Result<AppPreferences, String> {
    settings::set_selected_signing_profile(id)
}

#[tauri::command]
pub fn save_last_output_dir(path: String) -> Result<AppPreferences, String> {
    settings::save_last_output_dir(path)
}

#[tauri::command]
pub fn protect_artifact(
    state: State<AppState>,
    mut request: ProtectRequest,
) -> Result<String, String> {
    if request.input_path.trim().is_empty() {
        return Err("input path is required".to_string());
    }
    if request.signing_config.is_none() {
        if let Some(profile_id) = request.signing_profile_id.as_deref() {
            request.signing_config = Some(
                settings::signing_config_for_profile(profile_id)?
                    .ok_or_else(|| format!("signing profile not found: {profile_id}"))?,
            );
        }
    }
    if !request.output_dir.trim().is_empty() {
        let _ = settings::save_last_output_dir(request.output_dir.clone());
    }
    let id = Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    jobs::insert_queued_job(&state, id.clone(), cancel.clone());

    let reporter = JobReporter::new(state.jobs.clone(), id.clone(), cancel);
    std::thread::spawn(move || match protect::run_protection(request, &reporter) {
        Ok(output_path) => reporter.succeed(output_path),
        Err(err) if err.stage == "canceled" => reporter.canceled(),
        Err(err) => reporter.fail(err.to_string()),
    });

    Ok(id)
}

#[tauri::command]
pub fn get_job_status(state: State<AppState>, job_id: String) -> Result<JobStatus, String> {
    jobs::get_status(&state, &job_id).ok_or_else(|| format!("job not found: {job_id}"))
}

#[tauri::command]
pub fn cancel_job(state: State<AppState>, job_id: String) -> Result<bool, String> {
    Ok(jobs::cancel_job(&state, &job_id))
}

#[tauri::command]
pub async fn sign_apk(
    input_path: String,
    output_dir: String,
    signing_profile_id: String,
    toolchain_paths: Option<ToolchainPaths>,
) -> Result<ApkSigningResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sign_apk_impl(input_path, output_dir, signing_profile_id, toolchain_paths)
    })
    .await
    .map_err(|err| format!("[sign] APK signing task failed: {err}"))?
}

fn sign_apk_impl(
    input_path: String,
    output_dir: String,
    signing_profile_id: String,
    toolchain_paths: Option<ToolchainPaths>,
) -> Result<ApkSigningResult, String> {
    let input = Path::new(input_path.trim());
    let output_dir = Path::new(output_dir.trim());
    if input.as_os_str().is_empty() {
        return Err("[scan] 请选择需要签名的 APK。".to_string());
    }
    if output_dir.as_os_str().is_empty() {
        return Err("[output] 请选择签名输出目录。".to_string());
    }
    if signing_profile_id.trim().is_empty() {
        return Err("[sign] 请选择签名信息。".to_string());
    }
    let artifact = scan::scan_artifact(input).map_err(|err| format!("[scan] {err}"))?;
    if artifact.kind != crate::models::ArtifactKind::Apk {
        return Err("[scan] 签名apk 只支持 APK 文件。".to_string());
    }
    let signing = settings::signing_config_for_profile(signing_profile_id.trim())?
        .ok_or_else(|| "[sign] 签名信息不存在，请重新选择。".to_string())?;
    sign_apk_to_dir(input, output_dir, &signing, toolchain_paths.as_ref())
}

fn sign_apk_to_dir(
    input: &Path,
    output_dir: &Path,
    signing: &SigningConfig,
    toolchain_paths: Option<&ToolchainPaths>,
) -> Result<ApkSigningResult, String> {
    let stem = input
        .file_stem()
        .ok_or_else(|| "[output] invalid APK filename".to_string())?
        .to_string_lossy();
    let output_path = output_dir.join(format!("{stem}.signed.apk"));
    if output_path.exists() {
        return Err(format!(
            "[output] 输出文件已存在，请选择其他目录：{}",
            output_path.display()
        ));
    }
    let detected = toolchain::detect_toolchain(toolchain_paths);
    let zipalign = toolchain::command_path(&detected.zipalign).ok_or_else(|| {
        "[toolchain] 未找到 zipalign，请配置 Android SDK build-tools。".to_string()
    })?;
    let apksigner = toolchain::command_path(&detected.apksigner).ok_or_else(|| {
        "[toolchain] 未找到 apksigner，请配置 Android SDK build-tools。".to_string()
    })?;
    fs::create_dir_all(output_dir).map_err(|err| format!("[output] {err}"))?;
    let scratch = tempfile::tempdir_in(output_dir).map_err(|err| format!("[sign] {err}"))?;
    let aligned = scratch.path().join("aligned.apk");
    let signed = scratch.path().join("signed.apk");

    let mut align = toolchain::command_for_tool(&zipalign);
    align.args(["-f", "-p", "4"]).arg(input).arg(&aligned);
    run_simple_command("zipalign", &mut align)?;
    protect::sign_apk_with_apksigner(&apksigner, &aligned, &signed, signing)?;
    let mut verify = toolchain::command_for_tool(&apksigner);
    verify.args(["verify", "--verbose"]).arg(&signed);
    run_simple_command("verify", &mut verify)?;

    // Publish only verified output, without replacing a file created during signing.
    tempfile::TempPath::try_from_path(signed)
        .map_err(|err| format!("[output] {err}"))?
        .persist_noclobber(&output_path)
        .map_err(|err| format!("[output] publish signed APK: {err}"))?;
    Ok(ApkSigningResult {
        output_dir: output_dir.display().to_string(),
        output_path: output_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn package_channels(
    input_path: String,
    output_dir: String,
    channels: Vec<String>,
    signing_profile_id: Option<String>,
    toolchain_paths: Option<ToolchainPaths>,
    method: Option<crate::models::ChannelPackagingMethod>,
) -> Result<channel::ChannelPackageResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        package_channels_impl(
            input_path,
            output_dir,
            channels,
            signing_profile_id,
            toolchain_paths,
            method.unwrap_or_default(),
        )
    })
    .await
    .map_err(|err| format!("channel packaging task failed: {err}"))?
}

fn package_channels_impl(
    input_path: String,
    output_dir: String,
    channels: Vec<String>,
    signing_profile_id: Option<String>,
    toolchain_paths: Option<ToolchainPaths>,
    method: crate::models::ChannelPackagingMethod,
) -> Result<channel::ChannelPackageResult, String> {
    let input = Path::new(input_path.trim());
    let output_dir = Path::new(output_dir.trim());
    if input.as_os_str().is_empty() {
        return Err("source APK path is required".to_string());
    }
    if output_dir.as_os_str().is_empty() {
        return Err("channel output directory is required".to_string());
    }
    if scan::scan_artifact(input)?.kind != crate::models::ArtifactKind::Apk {
        return Err("multi-channel packaging only supports APK files".to_string());
    }
    let channels = channel::validate_channels(&channels)?;
    let signing_profile_id = signing_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(profile_id) = signing_profile_id {
        let signing = settings::signing_config_for_profile(profile_id)?
            .ok_or_else(|| format!("signing profile not found: {profile_id}"))?;
        return sign_apk_then_package_channels(
            input,
            output_dir,
            &channels,
            &signing,
            toolchain_paths,
            method,
        );
    }
    package_signed_channels(
        input,
        output_dir,
        &channels,
        method,
        toolchain_paths.as_ref(),
    )
}

fn package_signed_channels(
    input: &Path,
    output_dir: &Path,
    channels: &[String],
    method: crate::models::ChannelPackagingMethod,
    toolchain_paths: Option<&ToolchainPaths>,
) -> Result<channel::ChannelPackageResult, String> {
    match method {
        crate::models::ChannelPackagingMethod::Existing => {
            channel::write_channel_packages_to_dir(input, output_dir, channels)
        }
        crate::models::ChannelPackagingMethod::VasDolly => {
            crate::vasdolly::write_channel_packages(input, output_dir, channels, toolchain_paths)
        }
    }
}

fn sign_apk_then_package_channels(
    input: &Path,
    output_dir: &Path,
    channels: &[String],
    signing: &SigningConfig,
    toolchain_paths: Option<ToolchainPaths>,
    method: crate::models::ChannelPackagingMethod,
) -> Result<channel::ChannelPackageResult, String> {
    if input
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .as_deref()
        != Some("apk")
    {
        return Err("multi-channel packaging only supports APK files".to_string());
    }
    let detected = toolchain::detect_toolchain(toolchain_paths.as_ref());
    let zipalign = toolchain::command_path(&detected.zipalign)
        .ok_or_else(|| "zipalign is required before signing unsigned APK".to_string())?;
    let apksigner = toolchain::command_path(&detected.apksigner)
        .ok_or_else(|| "apksigner is required for signing unsigned APK".to_string())?;

    let temp = tempdir().map_err(|err| format!("failed to create signing temp dir: {err}"))?;
    let file_name = input
        .file_name()
        .map(|value| value.to_owned())
        .unwrap_or_else(|| "source.apk".into());
    let stem = input
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string());
    let aligned = temp.path().join(format!("{stem}.aligned.apk"));
    let signed = temp.path().join(file_name);

    let mut zipalign_command = toolchain::command_for_tool(&zipalign);
    zipalign_command
        .arg("-f")
        .arg("-p")
        .arg("4")
        .arg(input)
        .arg(&aligned);
    run_simple_command("zipalign", &mut zipalign_command)?;
    protect::sign_apk_with_apksigner(&apksigner, &aligned, &signed, signing)?;
    package_signed_channels(
        &signed,
        output_dir,
        channels,
        method,
        toolchain_paths.as_ref(),
    )
}

fn run_simple_command(stage: &str, command: &mut Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|err| format!("[{stage}] failed to start command: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit code {:?}", output.status.code())
    };
    Err(format!("[{stage}] {detail}"))
}

#[tauri::command]
pub fn open_output_dir(path: String) -> Result<(), String> {
    let dir = resolve_open_dir(&path)?;
    open_dir(&dir)
}

#[tauri::command]
pub fn save_job_log(state: State<AppState>, job_id: String, path: String) -> Result<(), String> {
    let status =
        jobs::get_status(&state, &job_id).ok_or_else(|| format!("job not found: {job_id}"))?;
    let path = resolve_log_path(&path)?;
    fs::write(&path, format_job_log(&status))
        .map_err(|err| format!("failed to save job log: {err}"))?;
    Ok(())
}

fn resolve_open_dir(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("output path is required".to_string());
    }

    let path = PathBuf::from(trimmed);
    let dir = if path.is_dir() {
        path
    } else if path.exists() {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("failed to resolve output directory: {}", path.display()))?
    } else if path.extension().is_some() {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("failed to resolve output directory: {}", path.display()))?
    } else {
        path
    };

    if !dir.exists() {
        return Err(format!(
            "output directory does not exist: {}",
            dir.display()
        ));
    }
    if !dir.is_dir() {
        return Err(format!("output path is not a directory: {}", dir.display()));
    }

    Ok(dir)
}

fn open_dir(dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(dir);
        toolchain::hide_command_window(&mut command);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(dir);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(dir);
        command
    };

    command
        .spawn()
        .map_err(|err| format!("failed to open output directory: {err}"))?;
    Ok(())
}

fn resolve_log_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("log path is required".to_string());
    }

    let path = PathBuf::from(trimmed);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if !parent.exists() {
            return Err(format!(
                "log directory does not exist: {}",
                parent.display()
            ));
        }
        if !parent.is_dir() {
            return Err(format!(
                "log parent path is not a directory: {}",
                parent.display()
            ));
        }
    }

    Ok(path)
}

fn format_job_log(status: &JobStatus) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Job ID: {}", status.id));
    lines.push(format!("Lifecycle: {:?}", status.lifecycle));
    lines.push(format!("Stage: {}", status.stage));
    lines.push(format!("Progress: {}%", (status.progress * 100.0).round()));
    if let Some(started_at) = status.started_at.as_ref() {
        lines.push(format!("Started At: {started_at}"));
    }
    if let Some(finished_at) = status.finished_at.as_ref() {
        lines.push(format!("Finished At: {finished_at}"));
    }
    if let Some(output_path) = status.output_path.as_ref() {
        lines.push(format!("Output: {output_path}"));
    }
    if let Some(error) = status.error.as_ref() {
        lines.push(format!("Error: {error}"));
    }
    lines.push(String::new());
    lines.push("Logs:".to_string());
    for entry in &status.logs {
        lines.push(format!(
            "[{}] [{}] {}",
            entry.timestamp, entry.stage, entry.message
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn apk_signing_requires_input_output_and_profile() {
        for (input, output, profile, expected) in [
            (" ", "out", "profile", "[scan]"),
            ("app.apk", " ", "profile", "[output]"),
            ("app.apk", "out", " ", "[sign]"),
        ] {
            let error =
                sign_apk_impl(input.into(), output.into(), profile.into(), None).unwrap_err();
            assert!(error.starts_with(expected), "{error}");
        }
    }

    #[test]
    fn apk_signing_rejects_aab_before_loading_profile_or_writing_output() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("app.aab");
        let output = temp.path().join("output");
        let mut zip = zip::ZipWriter::new(fs::File::create(&input).unwrap());
        zip.start_file(
            "base/manifest/AndroidManifest.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<manifest package=\"com.example.test\" />")
            .unwrap();
        zip.finish().unwrap();

        let error = sign_apk_impl(
            input.display().to_string(),
            output.display().to_string(),
            "unused-profile".into(),
            None,
        )
        .unwrap_err();
        assert!(error.contains("只支持 APK"), "{error}");
        assert!(!output.exists());
    }

    #[test]
    fn apk_signing_preserves_existing_output() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("应用 & release.APK");
        let output = temp.path().join("应用 & release.signed.apk");
        fs::write(&output, b"existing signed APK").unwrap();
        let error =
            sign_apk_to_dir(&input, temp.path(), &SigningConfig::default(), None).unwrap_err();
        assert!(error.contains("已存在"), "{error}");
        assert_eq!(fs::read(output).unwrap(), b"existing signed APK");
    }

    #[test]
    #[ignore = "requires JDK/SDK and APK_SIGNING_TEST_FIXTURES with APKs and fixture.jks"]
    fn signs_real_apks_without_channel_packaging() {
        use crate::models::SigningScheme;
        use std::collections::BTreeMap;
        use std::io::Read;

        fn content_entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
            let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
            let mut entries = BTreeMap::new();
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).unwrap();
                if entry.is_dir() || scan::is_signature_entry(entry.name()) {
                    continue;
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                entries.insert(entry.name().to_string(), bytes);
            }
            entries
        }

        let fixtures =
            PathBuf::from(std::env::var("APK_SIGNING_TEST_FIXTURES").expect("fixture directory"));
        let temp = tempfile::tempdir().unwrap();
        let mut signing = SigningConfig {
            keystore_path: fixtures.join("fixture.jks").display().to_string(),
            store_password: "fixture-only".into(),
            key_password: Some("fixture-only".into()),
            alias: "fixture".into(),
            ..SigningConfig::default()
        };
        let detected = toolchain::detect_toolchain(None);
        let apksigner = toolchain::command_path(&detected.apksigner).unwrap();
        for source in ["unsigned", "v1", "v2", "v3"] {
            let original_input = fixtures.join(format!("{source}.apk"));
            // Exercise Unicode, spaces, metacharacters and uppercase extensions.
            let input = temp.path().join(format!("应用 & {source}.APK"));
            fs::copy(original_input, &input).unwrap();
            let original = fs::read(&input).unwrap();
            for scheme in [
                SigningScheme::V1V2,
                SigningScheme::V2V3,
                SigningScheme::V1V2V3,
            ] {
                signing.signing_scheme = scheme;
                let output_dir = temp.path().join(format!("签名 & {source}-{scheme:?}"));
                let result = sign_apk_to_dir(&input, &output_dir, &signing, None).unwrap();
                let output = Path::new(&result.output_path);
                assert_eq!(
                    output.file_name().unwrap(),
                    format!("应用 & {source}.signed.apk").as_str()
                );
                assert_eq!(Path::new(&result.output_dir), output_dir);
                assert_eq!(content_entries(&input), content_entries(output));
                assert_eq!(fs::read(&input).unwrap(), original);
                assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 1);
                let expects_v1 = scheme != SigningScheme::V2V3;
                let expects_v3 = scheme != SigningScheme::V1V2;
                let mut verify = toolchain::command_for_tool(&apksigner);
                // Force verification on a platform that needs V1 when requested;
                // the fixture's minSdk 28 otherwise allows the verifier to skip it.
                verify
                    .args([
                        "verify",
                        "--verbose",
                        "--min-sdk-version",
                        if expects_v1 { "21" } else { "24" },
                    ])
                    .arg(output);
                let verification = verify.output().unwrap();
                assert!(verification.status.success(), "{verification:?}");
                let text = String::from_utf8_lossy(&verification.stdout);
                assert!(text.contains("Verified using v2 scheme (APK Signature Scheme v2): true"));
                assert!(
                    text.contains(&format!(
                        "Verified using v1 scheme (JAR signing): {expects_v1}"
                    )),
                    "{text}"
                );
                assert!(
                    text.contains(&format!(
                        "Verified using v3 scheme (APK Signature Scheme v3): {expects_v3}"
                    )),
                    "{text}"
                );
            }
        }
        signing.key_password = Some("wrong-test-password".into());
        let failed_dir = temp.path().join("failed");
        assert!(
            sign_apk_to_dir(&fixtures.join("unsigned.apk"), &failed_dir, &signing, None).is_err()
        );
        assert_eq!(fs::read_dir(failed_dir).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "requires Java and signed fixtures; run scripts/test-vasdolly.ps1"]
    fn packages_real_apks_with_both_methods() {
        use crate::models::ChannelPackagingMethod;
        let fixtures =
            PathBuf::from(std::env::var("VASDOLLY_TEST_FIXTURES").expect("fixture directory"));
        let temp = tempfile::tempdir().unwrap();
        for scheme in ["v1", "v2", "v3"] {
            let input = fixtures.join(format!("{scheme}.apk"));
            let original = fs::read(&input).unwrap();
            let output = temp.path().join(format!("渠道 {scheme} & output"));
            let result = package_channels_impl(
                input.display().to_string(),
                output.display().to_string(),
                vec!["huawei".into(), "yyb".into()],
                None,
                None,
                ChannelPackagingMethod::VasDolly,
            )
            .unwrap();
            assert_eq!(result.packages.len(), 2);
            assert_eq!(result.packages[0].value, "huawei");
            assert!(Path::new(&result.packages[1].path).is_file());
            assert_eq!(fs::read(&input).unwrap(), original);
            let detected = toolchain::detect_toolchain(None);
            let signer = toolchain::command_path(&detected.apksigner).unwrap();
            for package in &result.packages {
                let mut verify = toolchain::command_for_tool(&signer);
                verify.arg("verify").arg(&package.path);
                run_simple_command("verify", &mut verify).unwrap();
            }
            let existing = fs::read(&result.packages[0].path).unwrap();
            assert!(package_channels_impl(
                input.display().to_string(),
                output.display().to_string(),
                vec!["huawei".into()],
                None,
                None,
                ChannelPackagingMethod::VasDolly,
            )
            .unwrap_err()
            .contains("已存在"));
            assert_eq!(fs::read(&result.packages[0].path).unwrap(), existing);
        }
        let legacy = package_channels_impl(
            fixtures.join("v2.apk").display().to_string(),
            temp.path().join("legacy").display().to_string(),
            vec!["oppo".into()],
            None,
            None,
            ChannelPackagingMethod::Existing,
        )
        .unwrap();
        assert_eq!(
            channel::read_sign_key_value(Path::new(&legacy.packages[0].path)).unwrap(),
            "{\"chn\":\"oppo\"}"
        );
        let failed_output = temp.path().join("unsigned");
        assert!(package_channels_impl(
            fixtures.join("unsigned.apk").display().to_string(),
            failed_output.display().to_string(),
            vec!["huawei".into()],
            None,
            None,
            ChannelPackagingMethod::VasDolly,
        )
        .is_err());
        assert!(!failed_output.join("unsigned_huawei.apk").exists());
        let signing = SigningConfig {
            keystore_path: fixtures.join("fixture.jks").display().to_string(),
            store_password: "fixture-only".into(),
            key_password: Some("fixture-only".into()),
            alias: "fixture".into(),
            ..SigningConfig::default()
        };
        let presigned = sign_apk_then_package_channels(
            &fixtures.join("unsigned.apk"),
            &temp.path().join("presigned"),
            &["vivo".into()],
            &signing,
            None,
            ChannelPackagingMethod::VasDolly,
        )
        .unwrap();
        assert_eq!(presigned.packages[0].value, "vivo");
    }

    #[test]
    fn resolves_existing_output_directory() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_open_dir(&temp.path().display().to_string()).unwrap(),
            temp.path()
        );
    }

    #[test]
    fn resolves_parent_for_existing_output_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("protected.apk");
        fs::write(&file, b"apk").unwrap();

        assert_eq!(
            resolve_open_dir(&file.display().to_string()).unwrap(),
            temp.path()
        );
    }

    #[test]
    fn resolves_log_path_when_parent_exists() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("job.log");

        assert_eq!(resolve_log_path(&file.display().to_string()).unwrap(), file);
    }

    #[test]
    fn formats_job_log_with_key_fields() {
        let status = JobStatus {
            id: "job-1".to_string(),
            lifecycle: crate::models::JobLifecycle::Failed,
            stage: "sign".to_string(),
            progress: 0.76,
            logs: vec![crate::models::JobLogEntry {
                timestamp: "2026-06-30T10:00:00Z".to_string(),
                stage: "sign".to_string(),
                message: "apksigner failed".to_string(),
            }],
            output_path: Some("C:\\out\\app.apk".to_string()),
            error: Some("[sign] apksigner failed".to_string()),
            started_at: Some("2026-06-30T09:59:00Z".to_string()),
            finished_at: Some("2026-06-30T10:00:01Z".to_string()),
        };

        let log = format_job_log(&status);

        assert!(log.contains("Job ID: job-1"));
        assert!(log.contains("Lifecycle: Failed"));
        assert!(log.contains("Output: C:\\out\\app.apk"));
        assert!(log.contains("[sign] apksigner failed"));
    }
}
