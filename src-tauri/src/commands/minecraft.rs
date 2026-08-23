use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::auth::{
    acquire_minecraft_session, has_microsoft_authorization, MicrosoftAuthState,
};
use crate::minecraft::{
    fabric::{list_loader_versions, FabricLoaderVersion},
    installer::{
        delete_instance, detect_java_path,
        install_sandbox_instance_with_loader as install_sandbox_mode, list_available_versions,
        list_instances, rename_instance, version_java_major,
    },
    launcher::{read_lines, spawn_instance, MinecraftIdentity, MinecraftProcess},
    model::{InstanceManifest, ModLoader, VersionManifest},
    paths::MinecraftPaths,
    runtime::install_java_runtime,
};

type SharedChild = Arc<Mutex<MinecraftProcess>>;

#[derive(Clone, Default)]
pub struct MinecraftRuntimeState {
    registry: Arc<Mutex<MinecraftProcessRegistry>>,
}

#[derive(Default)]
struct MinecraftProcessRegistry {
    processes: HashMap<String, SharedChild>,
    operations: HashSet<String>,
}

struct InstanceOperationGuard {
    registry: Arc<Mutex<MinecraftProcessRegistry>>,
    instance_id: String,
}

impl Drop for InstanceOperationGuard {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.operations.remove(&self.instance_id);
        }
    }
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
pub fn rename_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
    name: String,
) -> Result<InstanceManifest, String> {
    let _operation = reserve_instance_operation(&state, &instance_id, true)?;
    rename_instance(&minecraft_paths(&app)?, &instance_id, &name).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<(), String> {
    let operation = reserve_instance_operation(&state, &instance_id, false)?;
    let paths = minecraft_paths(&app)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        delete_instance(&paths, &instance_id).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("削除処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
}

#[tauri::command]
pub async fn list_minecraft_versions() -> Result<VersionManifest, String> {
    tauri::async_runtime::spawn_blocking(list_available_versions)
        .await
        .map_err(|error| format!("バージョン一覧の取得処理への参加に失敗しました: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_fabric_loader_versions(
    minecraft_version: String,
) -> Result<Vec<FabricLoaderVersion>, String> {
    tauri::async_runtime::spawn_blocking(move || list_loader_versions(&minecraft_version))
        .await
        .map_err(|error| format!("Fabric Loader一覧の取得処理への参加に失敗しました: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn prepare_instance_sandbox(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<SandboxPreparation, String> {
    let _operation = reserve_instance_operation(&state, &instance_id, false)?;
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
pub async fn install_sandbox_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
    name: String,
    version_id: String,
    demo: bool,
    mod_loader: Option<ModLoader>,
) -> Result<InstanceManifest, String> {
    let operation = reserve_instance_operation(&state, &instance_id, false)?;
    let paths = minecraft_paths(&app)?;
    let event_app = app.clone();
    let display_name = if name.trim().is_empty() {
        instance_id.clone()
    } else {
        name
    };
    let mod_loader = mod_loader.unwrap_or_default();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let java_major = version_java_major(&version_id).map_err(|error| error.to_string())?;
        let java = install_java_runtime(&paths, java_major, |progress| {
            let _ = event_app.emit("minecraft-install-progress", progress);
        })
        .map_err(|error| error.to_string())?;
        install_sandbox_mode(
            &paths,
            &instance_id,
            &display_name,
            &java,
            &version_id,
            demo,
            mod_loader,
            |progress| {
                let _ = event_app.emit("minecraft-install-progress", progress);
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("インストール処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
}

#[tauri::command]
pub async fn launch_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    auth_state: State<'_, MicrosoftAuthState>,
    instance_id: String,
) -> Result<u32, String> {
    let operation = reserve_instance_operation(&state, &instance_id, false)?;

    let paths = minecraft_paths(&app)?;
    let instance = list_instances(&paths)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| format!("Instance was not found: {instance_id}"))?;
    let identity = if !instance.demo && has_microsoft_authorization()? {
        emit_launch_progress(
            &app,
            &instance_id,
            "authenticating",
            "MicrosoftアカウントとMinecraftの所有権を確認しています…",
        );
        let session = acquire_minecraft_session(&auth_state).await?;
        Some(MinecraftIdentity {
            player_name: session.player_name,
            uuid: session.uuid,
            access_token: session.access_token,
            client_id: session.client_id,
            xuid: session.xuid,
        })
    } else {
        None
    };
    emit_launch_progress(
        &app,
        &instance_id,
        "preparing",
        "AppContainerとゲームファイルを準備しています…",
    );
    let launch_instance_id = instance_id.clone();
    let spawned = tauri::async_runtime::spawn_blocking(move || {
        spawn_instance(&paths, &launch_instance_id, identity.as_ref())
            .map_err(|error| error.to_string())
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
        .registry
        .lock()
        .map_err(|_| "Minecraft process state is unavailable".to_owned())?
        .processes
        .insert(instance_id.clone(), Arc::clone(&child));
    drop(operation);

    emit_status(&app, &instance_id, "running", None);
    spawn_stdout_reader(
        app.clone(),
        instance_id.clone(),
        spawned.stdout,
        spawned.narrator_token,
    );
    spawn_log_reader(app.clone(), instance_id.clone(), "stderr", spawned.stderr);

    let registry = Arc::clone(&state.registry);
    std::thread::spawn(move || loop {
        let exit_status = match child.lock() {
            Ok(mut child) => child.try_wait(),
            Err(_) => return,
        };

        match exit_status {
            Ok(Some(status)) => {
                if let Ok(mut registry) = registry.lock() {
                    registry.processes.remove(&instance_id);
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
        .registry
        .lock()
        .map_err(|_| "Minecraft process state is unavailable".to_owned())?
        .processes
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

fn reserve_instance_operation(
    state: &MinecraftRuntimeState,
    instance_id: &str,
    allow_running: bool,
) -> Result<InstanceOperationGuard, String> {
    let registry = Arc::clone(&state.registry);
    {
        let mut state = registry
            .lock()
            .map_err(|_| "Minecraft process state is unavailable".to_owned())?;
        if !allow_running && state.processes.contains_key(instance_id) {
            return Err("This instance is currently running".to_owned());
        }
        if !state.operations.insert(instance_id.to_owned()) {
            return Err("Another operation is already using this instance".to_owned());
        }
    }

    Ok(InstanceOperationGuard {
        registry,
        instance_id: instance_id.to_owned(),
    })
}

fn spawn_log_reader<R>(app: AppHandle, instance_id: String, stream: &'static str, reader: R)
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        read_lines(reader, |line| emit_log(&app, &instance_id, stream, &line));
    });
}

fn spawn_stdout_reader<R>(
    app: AppHandle,
    instance_id: String,
    reader: R,
    narrator_token: Option<String>,
) where
    R: std::io::Read + Send + 'static,
{
    #[cfg(windows)]
    {
        use crate::platform::windows::narrator_broker::NarratorBroker;

        let Some(narrator_token) = narrator_token else {
            spawn_log_reader(app, instance_id, "stdout", reader);
            return;
        };
        match NarratorBroker::start(narrator_token) {
            Ok(broker) => {
                std::thread::spawn(move || {
                    read_lines(reader, |line| {
                        if !broker.handle_line(&line) {
                            emit_log(&app, &instance_id, "stdout", &line);
                        }
                    });
                });
            }
            Err(error) => {
                emit_log(
                    &app,
                    &instance_id,
                    "launcher",
                    &format!("ナレーターブローカーを開始できませんでした: {error}"),
                );
                spawn_log_reader(app, instance_id, "stdout", reader);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = narrator_token;
        spawn_log_reader(app, instance_id, "stdout", reader);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_operation_guard_blocks_only_the_same_instance() {
        let state = MinecraftRuntimeState::default();
        let first = reserve_instance_operation(&state, "first", false).unwrap();

        assert!(reserve_instance_operation(&state, "first", false).is_err());
        assert!(reserve_instance_operation(&state, "second", false).is_ok());

        drop(first);
        assert!(reserve_instance_operation(&state, "first", false).is_ok());
    }
}
