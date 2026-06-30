use crate::jobs::{self, AppState, JobReporter};
use crate::models::{
    AppPreferences, ArtifactInfo, JobStatus, ProtectRequest, SigningAliasInspection, SigningConfig,
    SigningProfileInput, SigningValidation, ToolchainPaths, ToolchainStatus, VmpPlan,
};
use crate::{protect, scan, settings, signing, toolchain, vmp};
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
pub fn open_output_dir(path: String) -> Result<(), String> {
    let dir = resolve_open_dir(&path)?;
    open_dir(&dir)
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
}
