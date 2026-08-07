mod commands;
pub mod minecraft;
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
        .manage(commands::minecraft::MinecraftRuntimeState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::minecraft::detect_java,
            commands::minecraft::install_demo_instance,
            commands::minecraft::launch_minecraft_instance,
            commands::minecraft::list_minecraft_instances,
            commands::minecraft::stop_minecraft_instance,
            commands::sandbox::ensure_sandbox_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
