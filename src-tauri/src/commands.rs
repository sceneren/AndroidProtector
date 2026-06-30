use crate::jobs::{self, AppState, JobReporter};
use crate::models::{
    AppPreferences, ArtifactInfo, JobStatus, ProtectRequest, SigningAliasInspection, SigningConfig,
    SigningProfileInput, SigningValidation, ToolchainPaths, ToolchainStatus, VmpPlan,
};
use crate::{channel, protect, scan, settings, signing, toolchain, vmp};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::State;
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
pub fn package_channels(
    input_path: String,
    output_dir: String,
    channels: Vec<String>,
) -> Result<channel::ChannelPackageResult, String> {
    let input = Path::new(input_path.trim());
    let output_dir = Path::new(output_dir.trim());
    if input.as_os_str().is_empty() {
        return Err("source APK path is required".to_string());
    }
    if output_dir.as_os_str().is_empty() {
        return Err("channel output directory is required".to_string());
    }
    channel::write_channel_packages_to_dir(input, output_dir, &channels)
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
