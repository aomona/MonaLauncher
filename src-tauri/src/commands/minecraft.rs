use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::auth::{
    acquire_minecraft_session, has_microsoft_authorization, MicrosoftAuthState,
};
use crate::minecraft::file_io::{path_is_link_or_reparse, read_bounded_file, write_atomic};
use crate::minecraft::{
    diagnostics::{diagnose_instance, diagnosis_failure, InstanceDiagnosis},
    fabric::{list_loader_versions, FabricLoaderVersion},
    installer::{
        delete_instance, install_sandbox_instance_with_loader as install_sandbox_mode,
        list_available_versions, list_instances, load_instance, load_instance_for_repair,
        rename_instance, validate_instance_id, validate_instance_name,
        validate_metadata_identifier, version_java_major,
    },
    launcher::{read_lines, spawn_instance, MinecraftIdentity, MinecraftProcess},
    model::{InstanceManifest, ModLoader, VersionManifest},
    modrinth::{ModSearchResponse, ModrinthClient},
    modrinth_installer::{
        install_modrinth_project, list_installed_mods, remove_modrinth_project, InstalledMod,
        ModInstallProgress, ModInstallResult, ModRemovalResult,
    },
    paths::MinecraftPaths,
    permissions::{permission_support, save_permissions, InstancePermissions, PermissionSupport},
    runtime::install_java_runtime,
};

type SharedChild = Arc<Mutex<MinecraftProcess>>;

#[derive(Clone, Default)]
pub struct MinecraftRuntimeState {
    registry: Arc<Mutex<MinecraftProcessRegistry>>,
}

impl MinecraftRuntimeState {
    pub(crate) fn terminate_all(&self) {
        let processes = match self.registry.lock() {
            Ok(mut registry) => {
                registry.operations.clear();
                registry.shared_installation = false;
                registry
                    .processes
                    .drain()
                    .map(|(_, process)| process)
                    .collect::<Vec<_>>()
            }
            Err(poisoned) => {
                let mut registry = poisoned.into_inner();
                registry.operations.clear();
                registry.shared_installation = false;
                registry
                    .processes
                    .drain()
                    .map(|(_, process)| process)
                    .collect::<Vec<_>>()
            }
        };

        for process in processes {
            let mut process = process
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

#[derive(Default)]
struct MinecraftProcessRegistry {
    processes: HashMap<String, SharedChild>,
    operations: HashSet<String>,
    shared_installation: bool,
}

struct InstanceOperationGuard {
    registry: Arc<Mutex<MinecraftProcessRegistry>>,
    instance_id: String,
    shared_installation: bool,
}

impl Drop for InstanceOperationGuard {
    fn drop(&mut self) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.operations.remove(&self.instance_id);
        if self.shared_installation {
            registry.shared_installation = false;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthInstallProgressEvent {
    instance_id: String,
    completed: usize,
    total: usize,
    message: String,
}

fn minecraft_paths(app: &AppHandle) -> Result<MinecraftPaths, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("アプリのデータ保存先を取得できませんでした: {error}"))?;

    Ok(MinecraftPaths::new(app_data.join("minecraft")))
}

#[tauri::command]
pub fn list_minecraft_instances(app: AppHandle) -> Result<Vec<InstanceManifest>, String> {
    list_instances(&minecraft_paths(&app)?).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn diagnose_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<InstanceDiagnosis, String> {
    let operation = reserve_instance_operation(&state, &instance_id, true)?;
    let paths = minecraft_paths(&app)?;
    let diagnosis_instance_id = instance_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, String>(match load_instance_for_repair(&paths, &instance_id) {
            Err(_) => diagnosis_failure(diagnosis_instance_id, false),
            Ok(_) => match diagnose_instance(&paths, &instance_id) {
                Ok(diagnosis) => diagnosis,
                Err(_) => diagnosis_failure(diagnosis_instance_id, true),
            },
        })
    })
    .await
    .map_err(|error| format!("診断処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
}

#[tauri::command]
pub async fn repair_minecraft_instance(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
) -> Result<InstanceDiagnosis, String> {
    let operation = reserve_shared_installation(&state, &instance_id)?;
    let paths = minecraft_paths(&app)?;
    let mut instance =
        load_instance_for_repair(&paths, &instance_id).map_err(|error| error.to_string())?;
    let event_app = app.clone();
    let temporary_id = format!(
        "repair-{:032x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "システム時刻が正しくないため修復を開始できません".to_owned())?
            .as_nanos()
    );
    validate_instance_id(&temporary_id).map_err(|error| error.to_string())?;
    if paths.instance(&temporary_id).exists() {
        return Err("修復用の一時領域が既に存在します。もう一度試してください".to_owned());
    }

    let repair_paths = paths.clone();
    let repair_instance_id = instance_id.clone();
    let cleanup_id = temporary_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let outcome = (|| {
            let java_major =
                version_java_major(&instance.version_id).map_err(|error| error.to_string())?;
            let java = install_java_runtime(&repair_paths, java_major, |progress| {
                let _ = event_app.emit("minecraft-install-progress", progress);
            })
            .map_err(|error| error.to_string())?;
            install_sandbox_mode(
                &repair_paths,
                &temporary_id,
                "MonaLauncher repair",
                &java,
                &instance.version_id,
                instance.demo,
                instance.mod_loader.clone(),
                |progress| {
                    let _ = event_app.emit("minecraft-install-progress", progress);
                },
            )
            .map_err(|error| error.to_string())?;

            if matches!(instance.mod_loader, ModLoader::Fabric { .. }) {
                let profile = read_bounded_file(
                    &repair_paths.instance_fabric_profile(&temporary_id),
                    2 * 1024 * 1024,
                )
                .map_err(|error| format!("修復済みFabric profileを読めませんでした: {error}"))?;
                write_atomic(
                    &repair_paths.instance_fabric_profile(&repair_instance_id),
                    &profile,
                )
                .map_err(|error| format!("Fabric profileを更新できませんでした: {error}"))?;
            }
            instance.java_path = java.to_string_lossy().into_owned();
            write_atomic(
                &repair_paths.instance_manifest(&repair_instance_id),
                &serde_json::to_vec_pretty(&instance)
                    .map_err(|error| format!("インスタンス設定を保存できませんでした: {error}"))?,
            )
            .map_err(|error| format!("インスタンス設定を更新できませんでした: {error}"))?;
            diagnose_instance(&repair_paths, &repair_instance_id).map_err(|error| error.to_string())
        })();

        let temporary = repair_paths.instance(&cleanup_id);
        if temporary.is_dir() && path_is_link_or_reparse(&temporary).is_ok_and(|is_link| !is_link) {
            let _ = std::fs::remove_dir_all(&temporary);
        }
        outcome
    })
    .await
    .map_err(|error| format!("修復処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
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
pub fn minecraft_permission_support() -> PermissionSupport {
    permission_support()
}

#[tauri::command]
pub fn update_minecraft_permissions(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
    permissions: InstancePermissions,
) -> Result<InstanceManifest, String> {
    let _operation = reserve_instance_operation(&state, &instance_id, false)?;
    save_permissions(&minecraft_paths(&app)?, &instance_id, permissions)
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
    validate_metadata_identifier(&minecraft_version).map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || list_loader_versions(&minecraft_version))
        .await
        .map_err(|error| format!("Fabric Loader一覧の取得処理への参加に失敗しました: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn search_modrinth_mods(
    app: AppHandle,
    instance_id: String,
    query: String,
    offset: u32,
) -> Result<ModSearchResponse, String> {
    let paths = minecraft_paths(&app)?;
    let instance = find_instance(&paths, &instance_id)?;
    if !matches!(instance.mod_loader, ModLoader::Fabric { .. }) {
        return Err("ModrinthのFabric mod検索はFabricインスタンスで利用できます".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        ModrinthClient::new()
            .and_then(|client| client.search_mods(&query, &instance.version_id, "fabric", offset))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Modrinth検索処理への参加に失敗しました: {error}"))?
}

#[tauri::command]
pub fn list_instance_mods(
    app: AppHandle,
    instance_id: String,
) -> Result<Vec<InstalledMod>, String> {
    let paths = minecraft_paths(&app)?;
    find_instance(&paths, &instance_id)?;
    list_installed_mods(&paths, &instance_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn install_modrinth_mod(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
    project_id: String,
) -> Result<ModInstallResult, String> {
    let operation = reserve_instance_operation(&state, &instance_id, false)?;
    let paths = minecraft_paths(&app)?;
    let instance = find_instance(&paths, &instance_id)?;
    let event_app = app.clone();
    let event_instance_id = instance_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        install_modrinth_project(
            &paths,
            &instance,
            &project_id,
            |progress: ModInstallProgress| {
                let _ = event_app.emit(
                    "modrinth-install-progress",
                    ModrinthInstallProgressEvent {
                        instance_id: event_instance_id.clone(),
                        completed: progress.completed,
                        total: progress.total,
                        message: progress.message,
                    },
                );
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Modrinthインストール処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
}

#[tauri::command]
pub async fn remove_modrinth_mod(
    app: AppHandle,
    state: State<'_, MinecraftRuntimeState>,
    instance_id: String,
    project_id: String,
) -> Result<ModRemovalResult, String> {
    let operation = reserve_instance_operation(&state, &instance_id, false)?;
    let paths = minecraft_paths(&app)?;
    let instance = find_instance(&paths, &instance_id)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        remove_modrinth_project(&paths, &instance, &project_id).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Modrinth削除処理への参加に失敗しました: {error}"))?;
    drop(operation);
    result
}

fn find_instance(paths: &MinecraftPaths, instance_id: &str) -> Result<InstanceManifest, String> {
    load_instance(paths, instance_id).map_err(|error| error.to_string())
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
    validate_instance_id(&instance_id).map_err(|error| error.to_string())?;
    let display_name = validate_instance_name(&name)
        .map_err(|error| error.to_string())?
        .to_owned();
    let operation = reserve_shared_installation(&state, &instance_id)?;
    let paths = minecraft_paths(&app)?;
    if paths.instance(&instance_id).exists() {
        return Err(format!(
            "MinecraftインスタンスID「{instance_id}」は既に使われています"
        ));
    }
    let event_app = app.clone();
    let mod_loader = mod_loader.unwrap_or_default();
    let cleanup_paths = paths.clone();
    let install_instance_id = instance_id.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let java_major = version_java_major(&version_id).map_err(|error| error.to_string())?;
        let java = install_java_runtime(&paths, java_major, |progress| {
            let _ = event_app.emit("minecraft-install-progress", progress);
        })
        .map_err(|error| error.to_string())?;
        install_sandbox_mode(
            &paths,
            &install_instance_id,
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
    .await;
    let result = joined
        .map_err(|error| format!("インストール処理への参加に失敗しました: {error}"))
        .and_then(std::convert::identity);
    if result.is_err() && !cleanup_paths.instance_manifest(&instance_id).exists() {
        let partial = cleanup_paths.instance(&instance_id);
        if partial.is_dir() {
            let _ = std::fs::remove_dir_all(partial);
        }
    }
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
    let instance = find_instance(&paths, &instance_id)?;
    if !instance.sandboxed {
        return Err("安全でない通常起動は無効です。インスタンスを再作成してください".to_owned());
    }
    let identity = if !instance.demo && has_microsoft_authorization().await? {
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
        })
    } else {
        None
    };
    emit_launch_progress(
        &app,
        &instance_id,
        "preparing",
        "隔離環境とゲームファイルを準備しています…",
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
            if cfg!(target_os = "macos") {
                "Seatbelt経由で起動しました（実験対応）。"
            } else if cfg!(target_os = "linux") {
                "bubblewrap + seccomp経由で起動を要求しました（実験対応）。"
            } else {
                "AppContainerトークンを確認しました。隔離環境で起動します。"
            },
        );
    }
    let pid = spawned.child.id();
    let child = Arc::new(Mutex::new(spawned.child));

    match state.registry.lock() {
        Ok(mut registry) => {
            registry
                .processes
                .insert(instance_id.clone(), Arc::clone(&child));
        }
        Err(_) => {
            let mut process = child
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _ = process.kill();
            let _ = process.wait();
            return Err("Minecraft process state is unavailable".to_owned());
        }
    }
    drop(operation);

    emit_status(&app, &instance_id, "running", None);
    spawn_stdout_reader(
        app.clone(),
        instance_id.clone(),
        spawned.stdout,
        spawned.narrator_token,
        #[cfg(windows)]
        spawned.cursor_broker,
    );
    spawn_log_reader(app.clone(), instance_id.clone(), "stderr", spawned.stderr);

    let registry = Arc::clone(&state.registry);
    std::thread::spawn(move || loop {
        let exit_status = child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .try_wait();

        match exit_status {
            Ok(Some(status)) => {
                registry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .processes
                    .remove(&instance_id);
                emit_status(&app, &instance_id, "stopped", status.code());
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(error) => {
                emit_log(&app, &instance_id, "launcher", &error.to_string());
                let mut process = child
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let _ = process.kill();
                let _ = process.wait();
                registry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .processes
                    .remove(&instance_id);
                emit_status(&app, &instance_id, "stopped", None);
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
    validate_instance_id(&instance_id).map_err(|error| error.to_string())?;
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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .kill()
        .map_err(|error| format!("Failed to stop Minecraft: {error}"));

    result
}

fn reserve_instance_operation(
    state: &MinecraftRuntimeState,
    instance_id: &str,
    allow_running: bool,
) -> Result<InstanceOperationGuard, String> {
    validate_instance_id(instance_id).map_err(|error| error.to_string())?;
    let registry = Arc::clone(&state.registry);
    {
        let mut state = registry
            .lock()
            .map_err(|_| "Minecraft process state is unavailable".to_owned())?;
        if state.shared_installation {
            return Err("Minecraft共有ファイルをインストール中です".to_owned());
        }
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
        shared_installation: false,
    })
}

fn reserve_shared_installation(
    state: &MinecraftRuntimeState,
    instance_id: &str,
) -> Result<InstanceOperationGuard, String> {
    validate_instance_id(instance_id).map_err(|error| error.to_string())?;
    let registry = Arc::clone(&state.registry);
    {
        let mut state = registry
            .lock()
            .map_err(|_| "Minecraft process state is unavailable".to_owned())?;
        if state.shared_installation || !state.operations.is_empty() || !state.processes.is_empty()
        {
            return Err(
                "実行中のゲームまたは別の操作があるため、共有ファイルを更新できません".to_owned(),
            );
        }
        state.shared_installation = true;
        state.operations.insert(instance_id.to_owned());
    }
    Ok(InstanceOperationGuard {
        registry,
        instance_id: instance_id.to_owned(),
        shared_installation: true,
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
    #[cfg(windows)] cursor_broker: Option<
        Arc<crate::platform::windows::cursor_broker::CursorBroker>,
    >,
) where
    R: std::io::Read + Send + 'static,
{
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        use crate::platform::narrator_broker::NarratorBroker;

        let narrator_broker = narrator_token.and_then(|token| match NarratorBroker::start(token) {
            Ok(broker) => Some(broker),
            Err(error) => {
                emit_log(
                    &app,
                    &instance_id,
                    "launcher",
                    &format!("ナレーターブローカーを開始できませんでした: {error}"),
                );
                None
            }
        });
        std::thread::spawn(move || {
            read_lines(reader, |line| {
                #[cfg(not(windows))]
                let cursor_protocol = false;
                #[cfg(windows)]
                let cursor_protocol = cursor_broker
                    .as_ref()
                    .is_some_and(|broker| broker.handle_line(&line));
                let narrator_protocol = narrator_broker
                    .as_ref()
                    .is_some_and(|broker| broker.handle_line(&line));
                // Suppress protocol secrets even if native speech initialization failed.
                let narrator_protocol =
                    narrator_protocol || line.contains("MONALAUNCHER_NARRATOR\t");
                if !cursor_protocol && !narrator_protocol {
                    emit_log(&app, &instance_id, "stdout", &line);
                }
            });
        });
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
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
    fn shared_installation_blocks_every_other_instance_operation() {
        let state = MinecraftRuntimeState::default();
        let install = reserve_shared_installation(&state, "installing").unwrap();

        assert!(reserve_instance_operation(&state, "other", false).is_err());
        assert!(reserve_shared_installation(&state, "second").is_err());

        drop(install);
        assert!(reserve_instance_operation(&state, "other", false).is_ok());
    }

    #[test]
    fn existing_instance_operation_blocks_shared_installation() {
        let state = MinecraftRuntimeState::default();
        let operation = reserve_instance_operation(&state, "active", false).unwrap();

        assert!(reserve_shared_installation(&state, "installing").is_err());

        drop(operation);
        assert!(reserve_shared_installation(&state, "installing").is_ok());
    }

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
