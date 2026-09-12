mod auth;
mod commands;
pub mod minecraft;
mod platform;
pub mod sandbox;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod probe {
    #[cfg(target_os = "linux")]
    pub use crate::platform::linux::{prepare, Desktop};
    pub use crate::platform::narrator_broker::NarratorBroker;
}

use tauri::Manager;

#[cfg(windows)]
pub mod probe {
    pub use crate::platform::windows::appcontainer_process::{
        launch_in_appcontainer, launch_probe_in_appcontainer, SpawnedAppContainerProcess,
        SpawnedProcessInfo,
    };
    pub use crate::platform::windows::appcontainer_profile::{
        ensure_appcontainer_profile, profile_name_for_instance,
    };
    pub use crate::platform::windows::narrator_broker::NarratorBroker;
    pub use crate::platform::windows::process_token::{
        current_process_token_info, ProcessTokenError, ProcessTokenInfo,
    };
    pub use crate::platform::windows::sandbox_acl::{
        grant_policy_access, lock_sandbox_launch_directory,
    };
    pub use crate::platform::windows::sandbox_drive::SandboxDrive;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(commands::auth::MicrosoftAuthState::default())
        .manage(commands::minecraft::MinecraftRuntimeState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::auth::begin_microsoft_sign_in,
            commands::auth::microsoft_auth_status,
            commands::auth::poll_microsoft_sign_in,
            commands::auth::refresh_minecraft_account,
            commands::auth::sign_out_microsoft,
            commands::minecraft::delete_minecraft_instance,
            commands::minecraft::diagnose_minecraft_instance,
            commands::minecraft::install_sandbox_instance,
            commands::minecraft::install_modrinth_mod,
            commands::minecraft::launch_minecraft_instance,
            commands::minecraft::list_fabric_loader_versions,
            commands::minecraft::list_instance_mods,
            commands::minecraft::list_minecraft_instances,
            commands::minecraft::list_minecraft_versions,
            commands::minecraft::remove_modrinth_mod,
            commands::minecraft::rename_minecraft_instance,
            commands::minecraft::minecraft_permission_support,
            commands::minecraft::update_minecraft_permissions,
            commands::minecraft::repair_minecraft_instance,
            commands::minecraft::search_modrinth_mods,
            commands::minecraft::stop_minecraft_instance,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application");

    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            app.state::<commands::minecraft::MinecraftRuntimeState>()
                .terminate_all();
        }
    });
}
