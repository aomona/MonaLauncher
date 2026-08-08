use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::minecraft::{
    installer::{
        detect_java_path, install_latest_demo_instance,
        install_latest_sandbox_instance as install_latest_sandbox_mode,
        install_sandbox_instance as install_sandbox_mode, latest_release_java_major,
        list_instances,
    },
    launcher::{read_lines, spawn_instance, MinecraftProcess},
    model::InstanceManifest,
    paths::MinecraftPaths,
    runtime::{install_java_8_runtime, install_java_runtime},
};

const SANDBOX_MINECRAFT_VERSION: &str = "1.12.2";

type SharedChild = Arc<Mutex<MinecraftProcess>>;

#[derive(Clone, Default)]
pub struct MinecraftRuntimeState {
    processes: Arc<Mutex<HashMap<String, SharedChild>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftLogEvent {
    instance_id: String,
    stream: String,
    line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftStatusEvent {
    instance_id: String,
    status: String,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftLaunchProgressEvent {
    instance_id: String,
    stage: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaDetection {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxPreparation {
    profile_name: String,
    sid: String,
    profile_created: bool,
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
pub fn prepare_instance_sandbox(
    app: AppHandle,
    instance_id: String,
) -> Result<SandboxPreparation, String> {
    prepare_instance_sandbox_for_platform(&app, &instance_id)
}

#[cfg(windows)]
fn prepare_instance_sandbox_for_platform(
    app: &AppHandle,
    instance_id: &str,
) -> Result<SandboxPreparation, String> {
    use crate::platform::windows::appcontainer_profile::{
        ensure_appcontainer_profile, profile_name_for_instance,
    };
    use crate::platform::windows::sandbox_acl::grant_minecraft_access;

    let paths = minecraft_paths(app)?;
    let instance = list_instances(&paths)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| format!("Instance was not found: {instance_id}"))?;
    let profile_name = profile_name_for_instance(instance_id).map_err(|error| error.to_string())?;
    let profile = ensure_appcontainer_profile(&profile_name).map_err(|error| error.to_string())?;

    grant_minecraft_access(&paths, &instance, &profile.sid).map_err(|error| error.to_string())?;

    Ok(SandboxPreparation {
        profile_name: profile.name,
        sid: profile.sid,
        profile_created: profile.created,
    })
}

#[cfg(not(windows))]
fn prepare_instance_sandbox_for_platform(
    _app: &AppHandle,
    _instance_id: &str,
) -> Result<SandboxPreparation, String> {
    Err("AppContainer is only available on Windows".to_owned())
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

#[tauri::command]
pub async fn install_sandbox_instance(
    app: AppHandle,
    instance_id: String,
    name: String,
    release_channel: String,
    demo: bool,
) -> Result<InstanceManifest, String> {
    let paths = minecraft_paths(&app)?;
    let event_app = app.clone();
    let display_name = if name.trim().is_empty() {
        instance_id.clone()
    } else {
        name
    };

    tauri::async_runtime::spawn_blocking(move || {
        if release_channel == "latest" {
            let java_major = latest_release_java_major().map_err(|error| error.to_string())?;
            let java = install_java_runtime(&paths, java_major, |progress| {
                let _ = event_app.emit("minecraft-install-progress", progress);
            })
            .map_err(|error| error.to_string())?;
            install_latest_sandbox_mode(
                &paths,
                &instance_id,
                &display_name,
                &java,
                demo,
                |progress| {
                    let _ = event_app.emit("minecraft-install-progress", progress);
                },
            )
            .map_err(|error| error.to_string())
        } else {
            let java = install_java_8_runtime(&paths, |progress| {
                let _ = event_app.emit("minecraft-install-progress", progress);
            })
            .map_err(|error| error.to_string())?;
            install_sandbox_mode(
                &paths,
                &instance_id,
                &display_name,
                &java,
                SANDBOX_MINECRAFT_VERSION,
                demo,
                |progress| {
                    let _ = event_app.emit("minecraft-install-progress", progress);
                },
            )
            .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| format!("インストール処理への参加に失敗しました: {error}"))?
}

#[tauri::command]
pub async fn launch_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<u32, String> {
    {
        let processes = state
            .processes
            .lock()
            .map_err(|_| "Minecraft process state is unavailable".to_owned())?;
        if processes.contains_key(&instance_id) {
            return Err("This instance is already running".to_owned());
        }
    }

    let paths = minecraft_paths(&app)?;
    emit_launch_progress(
        &app,
        &instance_id,
        "preparing",
        "AppContainerとゲームファイルを準備しています…",
    );
    let launch_instance_id = instance_id.clone();
    let spawned = tauri::async_runtime::spawn_blocking(move || {
        spawn_instance(&paths, &launch_instance_id).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("起動準備への参加に失敗しました: {error}"))??;
    emit_launch_progress(
        &app,
        &instance_id,
        "starting",
        "Minecraftプロセスを開始しました。ウィンドウを待っています…",
    );
    if spawned.sandboxed {
        emit_log(
            &app,
            &instance_id,
            "launcher",
            "AppContainerトークンを確認しました。隔離環境で起動します。",
        );
    }
    let pid = spawned.child.id();
    let child = Arc::new(Mutex::new(spawned.child));

    state
        .processes
        .lock()
        .map_err(|_| "Minecraft process state is unavailable".to_owned())?
        .insert(instance_id.clone(), Arc::clone(&child));

    emit_status(&app, &instance_id, "running", None);
    spawn_log_reader(app.clone(), instance_id.clone(), "stdout", spawned.stdout);
    spawn_log_reader(app.clone(), instance_id.clone(), "stderr", spawned.stderr);

    let processes = Arc::clone(&state.processes);
    std::thread::spawn(move || loop {
        let exit_status = match child.lock() {
            Ok(mut child) => child.try_wait(),
            Err(_) => return,
        };

        match exit_status {
            Ok(Some(status)) => {
                if let Ok(mut running) = processes.lock() {
                    running.remove(&instance_id);
                }
                emit_status(&app, &instance_id, "stopped", status.code());
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(error) => {
                emit_log(&app, &instance_id, "launcher", &error.to_string());
                return;
            }
        }
    });

    Ok(pid)
}

#[tauri::command]
pub fn stop_minecraft_instance(
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<(), String> {
    let child = state
        .processes
        .lock()
        .map_err(|_| "Minecraft process state is unavailable".to_owned())?
        .get(&instance_id)
        .cloned()
        .ok_or_else(|| "This instance is not running".to_owned())?;

    let result = child
        .lock()
        .map_err(|_| "Minecraft process is unavailable".to_owned())?
        .kill()
        .map_err(|error| format!("Failed to stop Minecraft: {error}"));

    result
}

fn spawn_log_reader<R>(app: AppHandle, instance_id: String, stream: &'static str, reader: R)
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        read_lines(reader, |line| emit_log(&app, &instance_id, stream, &line));
    });
}

fn emit_log(app: &AppHandle, instance_id: &str, stream: &str, line: &str) {
    let _ = app.emit(
        "minecraft-log",
        MinecraftLogEvent {
            instance_id: instance_id.to_owned(),
            stream: stream.to_owned(),
            line: line.to_owned(),
        },
    );
}

fn emit_status(app: &AppHandle, instance_id: &str, status: &str, exit_code: Option<i32>) {
    let _ = app.emit(
        "minecraft-status",
        MinecraftStatusEvent {
            instance_id: instance_id.to_owned(),
            status: status.to_owned(),
            exit_code,
        },
    );
}

fn emit_launch_progress(app: &AppHandle, instance_id: &str, stage: &str, message: &str) {
    let _ = app.emit(
        "minecraft-launch-progress",
        MinecraftLaunchProgressEvent {
            instance_id: instance_id.to_owned(),
            stage: stage.to_owned(),
            message: message.to_owned(),
        },
    );
}
