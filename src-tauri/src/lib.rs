mod channel;
mod commands;
mod crypto;
mod dex;
mod jobs;
mod loader;
mod manifest;
mod models;
mod protect;
mod scan;
mod settings;
mod signing;
mod toolchain;
mod vmp;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::detect_toolchain,
            commands::scan_artifact,
            commands::estimate_vmp,
            commands::validate_signing,
            commands::inspect_signing_aliases,
            commands::load_app_preferences,
            commands::save_signing_profile,
            commands::get_signing_profile_input,
            commands::delete_signing_profile,
            commands::set_selected_signing_profile,
            commands::save_last_output_dir,
            commands::protect_artifact,
            commands::get_job_status,
            commands::cancel_job,
            commands::package_channels,
            commands::open_output_dir,
            commands::save_job_log
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Android protector desktop app");
}
