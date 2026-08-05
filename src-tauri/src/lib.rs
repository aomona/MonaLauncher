mod commands;
mod platform;

#[cfg(windows)]
pub mod probe {
    pub use crate::platform::windows::appcontainer_process::{
        launch_probe_in_appcontainer, SpawnedProcessInfo,
    };
    pub use crate::platform::windows::appcontainer_profile::{
        ensure_appcontainer_profile, profile_name_for_instance,
    };
    pub use crate::platform::windows::process_token::{
        current_process_token_info, ProcessTokenError, ProcessTokenInfo,
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::sandbox::ensure_sandbox_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
