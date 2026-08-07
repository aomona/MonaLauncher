use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::minecraft::{
    installer::{detect_java_path, install_latest_demo_instance, list_instances},
    model::InstanceManifest,
    paths::MinecraftPaths,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaDetection {
    path: String,
}

fn minecraft_paths(app: &AppHandle) -> Result<MinecraftPaths, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("アプリのデータ保存先を取得できませんでした: {error}"))?;

    Ok(MinecraftPaths::new(app_data.join("minecraft")))
}

#[tauri::command]
pub fn detect_java() -> Result<JavaDetection, String> {
    let path = detect_java_path().ok_or_else(|| {
        "Java が見つかりません。JAVA_HOME または PATH に Java 25 を設定してください。".to_owned()
    })?;

    Ok(JavaDetection {
        path: path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn list_minecraft_instances(app: AppHandle) -> Result<Vec<InstanceManifest>, String> {
    list_instances(&minecraft_paths(&app)?).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn install_demo_instance(
    app: AppHandle,
    instance_id: String,
    name: String,
    java_path: String,
) -> Result<InstanceManifest, String> {
    let paths = minecraft_paths(&app)?;
    let event_app = app.clone();
    let display_name = if name.trim().is_empty() {
        instance_id.clone()
    } else {
        name
    };

    tauri::async_runtime::spawn_blocking(move || {
        install_latest_demo_instance(
            &paths,
            &instance_id,
            &display_name,
            &PathBuf::from(java_path),
            |progress| {
                let _ = event_app.emit("minecraft-install-progress", progress);
            },
        )
    })
    .await
    .map_err(|error| format!("インストール処理への参加に失敗しました: {error}"))?
    .map_err(|error| error.to_string())
}
