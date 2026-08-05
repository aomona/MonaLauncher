mod commands;
mod platform;

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
