use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use super::file_io::{path_is_link_or_reparse, read_bounded_file, write_atomic};
use super::model::{InstanceManifest, ModLoader};
use super::modrinth::{
    validate_identifier, ModrinthClient, ModrinthDependency, ModrinthError, ModrinthFile,
    ModrinthProject, ModrinthVersion,
};
use super::paths::MinecraftPaths;

const MAX_MOD_SIZE: u64 = 512 * 1024 * 1024;
const MAX_REGISTRY_SIZE: u64 = 1024 * 1024;
const MAX_DEPENDENCIES: usize = 64;
const MAX_REGISTRY_MODS: usize = 1024;
const MAX_MOD_INSTALL_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MODRINTH_CDN_HOST: &str = "cdn.modrinth.com";

#[derive(Debug)]
pub enum ModInstallError {
    Modrinth(ModrinthError),
    Io(std::io::Error),
    Json(serde_json::Error),
    FabricRequired,
    NoCompatibleVersion(String),
    InvalidVersionCompatibility {
        version_id: String,
        minecraft_version: String,
    },
    MissingPrimaryFile(String),
    InvalidFileName(String),
    InvalidDownloadUrl(String),
    InvalidHash(String),
    FileTooLarge {
        file_name: String,
        size: u64,
    },
    HashMismatch {
        file_name: String,
        expected: String,
        actual: String,
    },
    SizeMismatch {
        file_name: String,
        expected: u64,
        actual: u64,
    },
    DependencyCycle(String),
    DependencyLimitExceeded,
    UnsupportedRequiredDependency(String),
    DependencyProjectMismatch {
        expected: String,
        actual: String,
    },
    VersionConflict {
        project_id: String,
        first: String,
        second: String,
    },
    FileNameConflict(String),
    ModNotInstalled(String),
    DependencyOnly(String),
    ModifiedTrackedFile(String),
    DuplicateRegistryProject(String),
    RemovalRollbackFailed(String),
    InstallationRollbackFailed(String),
    RegistryTooLarge,
    InstallPlanTooLarge,
    InvalidInstance(String),
}

impl fmt::Display for ModInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Modrinth(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "Modファイルの操作に失敗しました: {error}"),
            Self::Json(error) => {
                write!(
                    formatter,
                    "Modインストール記録を解釈できませんでした: {error}"
                )
            }
            Self::FabricRequired => {
                write!(
                    formatter,
                    "ModrinthのFabric modはFabricインスタンスへ導入してください"
                )
            }
            Self::NoCompatibleVersion(project_id) => write!(
                formatter,
                "現在のMinecraft/Fabricに対応するModrinthバージョンがありません: {project_id}"
            ),
            Self::InvalidVersionCompatibility {
                version_id,
                minecraft_version,
            } => write!(
                formatter,
                "依存mod {version_id}はMinecraft {minecraft_version} / Fabricに対応していません"
            ),
            Self::MissingPrimaryFile(version_id) => {
                write!(
                    formatter,
                    "Modrinthバージョンに実行用JARがありません: {version_id}"
                )
            }
            Self::InvalidFileName(file_name) => {
                write!(
                    formatter,
                    "Modrinthのファイル名が安全ではありません: {file_name}"
                )
            }
            Self::InvalidDownloadUrl(url) => {
                write!(
                    formatter,
                    "許可されていないModrinthダウンロードURLです: {url}"
                )
            }
            Self::InvalidHash(hash) => {
                write!(formatter, "ModrinthのSHA-512が正しくありません: {hash}")
            }
            Self::FileTooLarge { file_name, size } => write!(
                formatter,
                "Modファイルが大きすぎます: {file_name} ({size} bytes)"
            ),
            Self::HashMismatch {
                file_name,
                expected,
                actual,
            } => write!(
                formatter,
                "ModファイルのSHA-512が一致しません: {file_name}: expected {expected}, got {actual}"
            ),
            Self::SizeMismatch {
                file_name,
                expected,
                actual,
            } => write!(
                formatter,
                "Modファイルのサイズが一致しません: {file_name}: expected {expected}, got {actual}"
            ),
            Self::DependencyCycle(project_id) => {
                write!(formatter, "Modの必須依存関係が循環しています: {project_id}")
            }
            Self::DependencyLimitExceeded => write!(
                formatter,
                "必須依存modが{MAX_DEPENDENCIES}件を超えたため中止しました"
            ),
            Self::UnsupportedRequiredDependency(dependency) => write!(
                formatter,
                "Modrinth外の必須依存関係は自動導入できません: {dependency}"
            ),
            Self::DependencyProjectMismatch { expected, actual } => write!(
                formatter,
                "依存modのproject IDが一致しません: expected {expected}, got {actual}"
            ),
            Self::VersionConflict {
                project_id,
                first,
                second,
            } => write!(
                formatter,
                "同じmodに異なる必須バージョンが要求されています: {project_id}: {first} / {second}"
            ),
            Self::FileNameConflict(file_name) => {
                write!(
                    formatter,
                    "別のmodとファイル名が重複しています: {file_name}"
                )
            }
            Self::ModNotInstalled(project_id) => {
                write!(formatter, "導入記録にないmodは削除できません: {project_id}")
            }
            Self::DependencyOnly(project_id) => write!(
                formatter,
                "このmodは別のmodが必要としているため、単独では削除できません: {project_id}"
            ),
            Self::ModifiedTrackedFile(file_name) => write!(
                formatter,
                "導入後に内容が変わったmodファイルは自動削除しません: {file_name}"
            ),
            Self::DuplicateRegistryProject(project_id) => write!(
                formatter,
                "Modインストール記録に同じproject IDが重複しています: {project_id}"
            ),
            Self::RemovalRollbackFailed(message) => {
                write!(formatter, "Mod削除の取り消しに失敗しました: {message}")
            }
            Self::InstallationRollbackFailed(message) => {
                write!(formatter, "Mod導入の取り消しに失敗しました: {message}")
            }
            Self::RegistryTooLarge => write!(formatter, "Modインストール記録が大きすぎます"),
            Self::InstallPlanTooLarge => {
                write!(formatter, "Mod導入の合計サイズが安全上限を超えています")
            }
            Self::InvalidInstance(instance_id) => {
                write!(
                    formatter,
                    "Mod操作のインスタンス指定が不正です: {instance_id}"
                )
            }
        }
    }
}

impl Error for ModInstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Modrinth(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ModrinthError> for ModInstallError {
    fn from(error: ModrinthError) -> Self {
        Self::Modrinth(error)
    }
}

impl From<std::io::Error> for ModInstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ModInstallError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
    pub file_name: String,
    pub sha512: String,
    pub size: u64,
    pub direct: bool,
    #[serde(default)]
    pub required_dependencies: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallResult {
    pub installed: Vec<InstalledMod>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRemovalResult {
    pub requested: InstalledMod,
    pub removed: Vec<InstalledMod>,
    pub retained_as_dependency: bool,
    pub cleanup_pending: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallProgress {
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModRegistry {
    #[serde(default)]
    mods: Vec<InstalledMod>,
}

struct PlannedMod {
    project: ModrinthProject,
    version: ModrinthVersion,
    file: ModrinthFile,
    direct: bool,
}

#[derive(Default)]
struct Resolution {
    planned: Vec<PlannedMod>,
    indexes: HashMap<String, usize>,
    visiting: HashSet<String>,
}

pub fn list_installed_mods(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<Vec<InstalledMod>, ModInstallError> {
    validate_instance_id(instance_id)?;
    let mut registry = load_registry(paths, instance_id)?;
    registry.mods.sort_by_key(|item| item.title.to_lowercase());
    Ok(registry.mods)
}

pub fn install_modrinth_project<F>(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    project_id: &str,
    progress: F,
) -> Result<ModInstallResult, ModInstallError>
where
    F: Fn(ModInstallProgress),
{
    validate_instance(paths, instance)?;
    if !matches!(instance.mod_loader, ModLoader::Fabric { .. }) {
        return Err(ModInstallError::FabricRequired);
    }

    let client = ModrinthClient::new()?;
    let mut resolution = Resolution::default();
    resolve_project(
        &client,
        project_id,
        &instance.version_id,
        true,
        &mut resolution,
    )?;

    let registry = load_registry(paths, &instance.id)?;
    validate_file_conflicts(paths, instance, &registry, &resolution.planned)?;
    let staging = paths.instance_mod_staging(&instance.id);
    reset_staging_directory(paths, instance, &staging)?;
    let result = (|| {
        stage_downloads(
            &client,
            paths,
            instance,
            &registry,
            &resolution.planned,
            &staging,
            &progress,
        )?;
        commit_installation(paths, instance, registry, resolution.planned, &staging)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

pub fn remove_modrinth_project(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    project_id: &str,
) -> Result<ModRemovalResult, ModInstallError> {
    validate_instance(paths, instance)?;
    if !matches!(instance.mod_loader, ModLoader::Fabric { .. }) {
        return Err(ModInstallError::FabricRequired);
    }
    validate_identifier(project_id)?;

    let mut registry = load_registry(paths, &instance.id)?;
    let requested_index = registry
        .mods
        .iter()
        .position(|installed| installed.project_id == project_id)
        .ok_or_else(|| ModInstallError::ModNotInstalled(project_id.to_owned()))?;
    let requested = registry.mods[requested_index].clone();
    if !requested.direct {
        return Err(ModInstallError::DependencyOnly(project_id.to_owned()));
    }
    registry.mods[requested_index].direct = false;

    let retained_projects = retained_project_ids(&mut registry)?;
    let retained_as_dependency = retained_projects.contains(project_id);
    let removed = registry
        .mods
        .iter()
        .filter(|installed| !installed.direct && !retained_projects.contains(&installed.project_id))
        .cloned()
        .collect::<Vec<_>>();
    let removed_projects = removed
        .iter()
        .map(|installed| installed.project_id.as_str())
        .collect::<HashSet<_>>();
    registry
        .mods
        .retain(|installed| !removed_projects.contains(installed.project_id.as_str()));

    let staging = create_removal_staging_directory(paths, instance)?;
    let moved = match stage_removed_files(paths, instance, &removed, &staging) {
        Ok(moved) => moved,
        Err(error) => {
            let _ = fs::remove_dir(&staging);
            return Err(error);
        }
    };
    if let Err(error) = save_registry(paths, &instance.id, &registry) {
        return Err(rollback_removal(&moved, &staging, error));
    }
    let cleanup_pending = cleanup_staged_files(&moved, &staging);

    Ok(ModRemovalResult {
        requested,
        removed,
        retained_as_dependency,
        cleanup_pending,
    })
}

fn validate_instance_id(instance_id: &str) -> Result<(), ModInstallError> {
    if instance_id.is_empty()
        || instance_id.len() > 41
        || !instance_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ModInstallError::InvalidInstance(instance_id.to_owned()));
    }
    Ok(())
}

fn validate_instance(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<(), ModInstallError> {
    validate_instance_id(&instance.id)?;
    if Path::new(&instance.game_directory) != paths.instance_game_directory(&instance.id) {
        return Err(ModInstallError::InvalidInstance(instance.id.clone()));
    }
    for directory in [
        paths.instance(&instance.id),
        paths.instance_game_directory(&instance.id),
    ] {
        if !directory.is_dir() || path_is_link_or_reparse(&directory)? {
            return Err(ModInstallError::InvalidInstance(instance.id.clone()));
        }
    }
    let mods = paths.instance_mods_directory(&instance.id);
    if mods.exists() && (!mods.is_dir() || path_is_link_or_reparse(&mods)?) {
        return Err(ModInstallError::InvalidInstance(instance.id.clone()));
    }
    Ok(())
}

fn resolve_project(
    client: &ModrinthClient,
    project_id: &str,
    minecraft_version: &str,
    direct: bool,
    resolution: &mut Resolution,
) -> Result<(), ModInstallError> {
    if let Some(index) = resolution.indexes.get(project_id).copied() {
        resolution.planned[index].direct |= direct;
        return Ok(());
    }
    let versions = client.project_versions(project_id, minecraft_version, "fabric")?;
    let version = select_preferred_version(&versions)
        .cloned()
        .ok_or_else(|| ModInstallError::NoCompatibleVersion(project_id.to_owned()))?;
    resolve_version(client, version, minecraft_version, direct, resolution)
}

fn resolve_version(
    client: &ModrinthClient,
    version: ModrinthVersion,
    minecraft_version: &str,
    direct: bool,
    resolution: &mut Resolution,
) -> Result<(), ModInstallError> {
    if let Some(index) = resolution.indexes.get(&version.project_id).copied() {
        let planned = &mut resolution.planned[index];
        if planned.version.id != version.id {
            return Err(ModInstallError::VersionConflict {
                project_id: version.project_id,
                first: planned.version.id.clone(),
                second: version.id,
            });
        }
        planned.direct |= direct;
        return Ok(());
    }
    if resolution.planned.len() + resolution.visiting.len() >= MAX_DEPENDENCIES {
        return Err(ModInstallError::DependencyLimitExceeded);
    }
    validate_version_compatibility(&version, minecraft_version)?;
    if !resolution.visiting.insert(version.project_id.clone()) {
        return Err(ModInstallError::DependencyCycle(version.project_id));
    }

    for dependency in version
        .dependencies
        .iter()
        .filter(|dependency| dependency.dependency_type == "required")
    {
        resolve_required_dependency(client, dependency, minecraft_version, resolution)?;
    }

    let project = client.project(&version.project_id)?;
    let file = select_primary_jar(&version)
        .cloned()
        .ok_or_else(|| ModInstallError::MissingPrimaryFile(version.id.clone()))?;
    let project_id = version.project_id.clone();
    let index = resolution.planned.len();
    resolution.planned.push(PlannedMod {
        project,
        version,
        file,
        direct,
    });
    resolution.indexes.insert(project_id.clone(), index);
    resolution.visiting.remove(&project_id);
    Ok(())
}

fn resolve_required_dependency(
    client: &ModrinthClient,
    dependency: &ModrinthDependency,
    minecraft_version: &str,
    resolution: &mut Resolution,
) -> Result<(), ModInstallError> {
    if let Some(version_id) = dependency.version_id.as_deref() {
        let version = client.version(version_id)?;
        if let Some(expected_project) = dependency.project_id.as_deref() {
            if version.project_id != expected_project {
                return Err(ModInstallError::DependencyProjectMismatch {
                    expected: expected_project.to_owned(),
                    actual: version.project_id,
                });
            }
        }
        return resolve_version(client, version, minecraft_version, false, resolution);
    }
    if let Some(project_id) = dependency.project_id.as_deref() {
        return resolve_project(client, project_id, minecraft_version, false, resolution);
    }
    Err(ModInstallError::UnsupportedRequiredDependency(
        dependency
            .file_name
            .clone()
            .unwrap_or_else(|| "unknown dependency".to_owned()),
    ))
}

fn retained_project_ids(registry: &mut ModRegistry) -> Result<HashSet<String>, ModInstallError> {
    let indexes = registry
        .mods
        .iter()
        .enumerate()
        .map(|(index, installed)| (installed.project_id.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut queue = registry
        .mods
        .iter()
        .filter(|installed| installed.direct)
        .map(|installed| installed.project_id.clone())
        .collect::<VecDeque<_>>();
    let mut retained = HashSet::new();
    let mut client = None;

    while let Some(project_id) = queue.pop_front() {
        if !retained.insert(project_id.clone()) {
            continue;
        }
        let Some(index) = indexes.get(&project_id).copied() else {
            continue;
        };
        let dependencies = match registry.mods[index].required_dependencies.clone() {
            Some(dependencies) => dependencies,
            None => {
                if client.is_none() {
                    client = Some(ModrinthClient::new()?);
                }
                let dependencies = remote_required_dependency_ids(
                    client.as_ref().expect("Modrinth client was initialized"),
                    &registry.mods[index],
                )?;
                registry.mods[index].required_dependencies = Some(dependencies.clone());
                dependencies
            }
        };
        for dependency in dependencies {
            if indexes.contains_key(&dependency) {
                queue.push_back(dependency);
            }
        }
    }

    Ok(retained)
}

fn remote_required_dependency_ids(
    client: &ModrinthClient,
    installed: &InstalledMod,
) -> Result<Vec<String>, ModInstallError> {
    let version = client.version(&installed.version_id)?;
    if version.project_id != installed.project_id {
        return Err(ModInstallError::DependencyProjectMismatch {
            expected: installed.project_id.clone(),
            actual: version.project_id,
        });
    }
    let mut dependencies = Vec::new();
    for dependency in version
        .dependencies
        .iter()
        .filter(|dependency| dependency.dependency_type == "required")
    {
        let project_id = if let Some(project_id) = dependency.project_id.as_deref() {
            validate_identifier(project_id)?;
            project_id.to_owned()
        } else if let Some(version_id) = dependency.version_id.as_deref() {
            client.version(version_id)?.project_id
        } else {
            return Err(ModInstallError::UnsupportedRequiredDependency(
                dependency
                    .file_name
                    .clone()
                    .unwrap_or_else(|| "unknown dependency".to_owned()),
            ));
        };
        dependencies.push(project_id);
    }
    dependencies.sort();
    dependencies.dedup();
    Ok(dependencies)
}

fn planned_required_dependency_ids(
    version: &ModrinthVersion,
    planned: &[PlannedMod],
) -> Result<Vec<String>, ModInstallError> {
    let mut dependencies = Vec::new();
    for dependency in version
        .dependencies
        .iter()
        .filter(|dependency| dependency.dependency_type == "required")
    {
        let planned_dependency = if let Some(version_id) = dependency.version_id.as_deref() {
            planned
                .iter()
                .find(|candidate| candidate.version.id == version_id)
        } else if let Some(project_id) = dependency.project_id.as_deref() {
            planned
                .iter()
                .find(|candidate| candidate.project.id == project_id)
        } else {
            None
        };
        let project_id = planned_dependency
            .map(|candidate| candidate.project.id.clone())
            .ok_or_else(|| {
                ModInstallError::UnsupportedRequiredDependency(
                    dependency
                        .file_name
                        .clone()
                        .or_else(|| dependency.project_id.clone())
                        .or_else(|| dependency.version_id.clone())
                        .unwrap_or_else(|| "unknown dependency".to_owned()),
                )
            })?;
        dependencies.push(project_id);
    }
    dependencies.sort();
    dependencies.dedup();
    Ok(dependencies)
}

fn select_preferred_version(versions: &[ModrinthVersion]) -> Option<&ModrinthVersion> {
    versions
        .iter()
        .find(|version| version.version_type == "release")
        .or_else(|| versions.first())
}

fn validate_version_compatibility(
    version: &ModrinthVersion,
    minecraft_version: &str,
) -> Result<(), ModInstallError> {
    if !version
        .game_versions
        .iter()
        .any(|candidate| candidate == minecraft_version)
        || !version.loaders.iter().any(|loader| loader == "fabric")
    {
        return Err(ModInstallError::InvalidVersionCompatibility {
            version_id: version.id.clone(),
            minecraft_version: minecraft_version.to_owned(),
        });
    }
    Ok(())
}

fn select_primary_jar(version: &ModrinthVersion) -> Option<&ModrinthFile> {
    version
        .files
        .iter()
        .find(|file| file.primary && file.file_type.is_none() && is_jar_name(&file.filename))
        .or_else(|| {
            version
                .files
                .iter()
                .find(|file| file.file_type.is_none() && is_jar_name(&file.filename))
        })
}

fn validate_file_conflicts(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    registry: &ModRegistry,
    planned: &[PlannedMod],
) -> Result<(), ModInstallError> {
    let mut planned_size = 0_u64;
    let mut owners = HashMap::<String, String>::new();
    for installed in &registry.mods {
        validate_file_name(&installed.file_name)?;
        owners.insert(
            installed.file_name.to_lowercase(),
            installed.project_id.clone(),
        );
    }
    for item in planned {
        validate_file_name(&item.file.filename)?;
        validate_sha512(&item.file.hashes.sha512)?;
        validate_download_url(&item.file.url)?;
        if item.file.size > MAX_MOD_SIZE {
            return Err(ModInstallError::FileTooLarge {
                file_name: item.file.filename.clone(),
                size: item.file.size,
            });
        }
        planned_size = planned_size
            .checked_add(item.file.size)
            .filter(|size| *size <= MAX_MOD_INSTALL_SIZE)
            .ok_or(ModInstallError::InstallPlanTooLarge)?;
        let key = item.file.filename.to_lowercase();
        if let Some(owner) = owners.get(&key) {
            if owner != &item.project.id {
                return Err(ModInstallError::FileNameConflict(
                    item.file.filename.clone(),
                ));
            }
        }
        owners.insert(key, item.project.id.clone());

        let target = paths
            .instance_mods_directory(&instance.id)
            .join(&item.file.filename);
        let tracked_target = registry.mods.iter().any(|installed| {
            installed.project_id == item.project.id && installed.file_name == item.file.filename
        });
        if target.is_file()
            && !tracked_target
            && file_sha512(&target)? != item.file.hashes.sha512.to_ascii_lowercase()
        {
            return Err(ModInstallError::FileNameConflict(
                item.file.filename.clone(),
            ));
        }
    }
    Ok(())
}

fn stage_downloads<F>(
    client: &ModrinthClient,
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    registry: &ModRegistry,
    planned: &[PlannedMod],
    staging: &Path,
    progress: &F,
) -> Result<(), ModInstallError>
where
    F: Fn(ModInstallProgress),
{
    let mods_directory = paths.instance_mods_directory(&instance.id);
    fs::create_dir_all(&mods_directory)?;
    for (index, item) in planned.iter().enumerate() {
        progress(ModInstallProgress {
            completed: index,
            total: planned.len(),
            message: format!(
                "{} {}を確認しています",
                item.project.title, item.version.version_number
            ),
        });
        let target = mods_directory.join(&item.file.filename);
        let already_installed = target.is_file()
            && file_matches(&target, &item.file)?
            && registry.mods.iter().any(|installed| {
                installed.project_id == item.project.id
                    && installed.version_id == item.version.id
                    && installed.file_name == item.file.filename
            });
        if already_installed {
            continue;
        }
        download_mod_file(client, &item.file, &staging.join(&item.file.filename))?;
    }
    progress(ModInstallProgress {
        completed: planned.len(),
        total: planned.len(),
        message: "Modファイルの検証が完了しました".to_owned(),
    });
    Ok(())
}

fn commit_installation(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    mut registry: ModRegistry,
    planned: Vec<PlannedMod>,
    staging: &Path,
) -> Result<ModInstallResult, ModInstallError> {
    let mods_directory = paths.instance_mods_directory(&instance.id);
    let old_registry = registry.mods.clone();
    let mut installed_now = Vec::with_capacity(planned.len());
    let mut files_to_publish = Vec::new();
    let required_dependencies = planned
        .iter()
        .map(|item| {
            Ok((
                item.project.id.clone(),
                planned_required_dependency_ids(&item.version, &planned)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, ModInstallError>>()?;

    for item in planned {
        let staged = staging.join(&item.file.filename);
        if staged.is_file() {
            files_to_publish.push(item.file.filename.clone());
        }
        let previous_direct = registry
            .mods
            .iter()
            .find(|installed| installed.project_id == item.project.id)
            .is_some_and(|installed| installed.direct);
        let item_required_dependencies = required_dependencies
            .get(&item.project.id)
            .cloned()
            .unwrap_or_default();
        let installed = InstalledMod {
            project_id: item.project.id,
            version_id: item.version.id,
            title: sanitize_text(item.project.title, 120),
            version_number: sanitize_text(item.version.version_number, 80),
            file_name: item.file.filename,
            sha512: item.file.hashes.sha512.to_ascii_lowercase(),
            size: item.file.size,
            direct: item.direct || previous_direct,
            required_dependencies: Some(item_required_dependencies),
        };
        registry
            .mods
            .retain(|existing| existing.project_id != installed.project_id);
        registry.mods.push(installed.clone());
        installed_now.push(installed);
    }

    let backups = staging.join("backups");
    fs::create_dir(&backups)?;
    let mut moved_backups = Vec::<(std::path::PathBuf, std::path::PathBuf)>::new();
    let mut published = Vec::<std::path::PathBuf>::new();
    let publish_result = (|| {
        for (index, file_name) in files_to_publish.iter().enumerate() {
            let staged = staging.join(file_name);
            let target = mods_directory.join(file_name);
            if target.exists() {
                let backup = backups.join(format!("new-{index}-{file_name}"));
                fs::rename(&target, &backup)?;
                moved_backups.push((target.clone(), backup));
            }
            fs::rename(&staged, &target)?;
            published.push(target);
        }

        let referenced = registry
            .mods
            .iter()
            .map(|installed| installed.file_name.to_lowercase())
            .collect::<HashSet<_>>();
        for (index, old) in old_registry.iter().enumerate() {
            validate_file_name(&old.file_name)?;
            if referenced.contains(&old.file_name.to_lowercase()) {
                continue;
            }
            let target = mods_directory.join(&old.file_name);
            if target.is_file() && file_sha512(&target)? == old.sha512 {
                let backup = backups.join(format!("old-{index}-{}", old.file_name));
                fs::rename(&target, &backup)?;
                moved_backups.push((target, backup));
            }
        }
        save_registry(paths, &instance.id, &registry)
    })();
    if let Err(error) = publish_result {
        return Err(rollback_installation(&published, &moved_backups, error));
    }
    let _ = fs::remove_dir_all(&backups);
    Ok(ModInstallResult {
        installed: installed_now,
    })
}

fn rollback_installation(
    published: &[std::path::PathBuf],
    moved_backups: &[(std::path::PathBuf, std::path::PathBuf)],
    original_error: ModInstallError,
) -> ModInstallError {
    for target in published.iter().rev() {
        if target.exists() && fs::remove_file(target).is_err() {
            return ModInstallError::InstallationRollbackFailed(format!(
                "{original_error}; 新しいファイルを除去できません: {}",
                target.display()
            ));
        }
    }
    for (target, backup) in moved_backups.iter().rev() {
        if !backup.exists() {
            continue;
        }
        if target.exists() || fs::rename(backup, target).is_err() {
            return ModInstallError::InstallationRollbackFailed(format!(
                "{original_error}; 元のファイルを復元できません: {}",
                target.display()
            ));
        }
    }
    original_error
}

fn create_removal_staging_directory(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<std::path::PathBuf, ModInstallError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = paths.instance(&instance.id).join(format!(
        "modrinth-removal-staging-{}-{timestamp:x}",
        process::id()
    ));
    let expected_parent = paths.instance(&instance.id);
    if staging.parent() != Some(expected_parent.as_path()) {
        return Err(ModInstallError::InvalidFileName(
            staging.display().to_string(),
        ));
    }
    fs::create_dir(&staging)?;
    Ok(staging)
}

fn stage_removed_files(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    removed: &[InstalledMod],
    staging: &Path,
) -> Result<Vec<(std::path::PathBuf, std::path::PathBuf)>, ModInstallError> {
    let mods_directory = paths.instance_mods_directory(&instance.id);
    for installed in removed {
        validate_file_name(&installed.file_name)?;
        validate_tracked_mod_file(&mods_directory.join(&installed.file_name), installed)?;
    }

    let mut moved = Vec::new();
    for installed in removed {
        let target = mods_directory.join(&installed.file_name);
        if !validate_tracked_mod_file(&target, installed)? {
            continue;
        }
        let staged = staging.join(&installed.file_name);
        if let Err(error) = fs::rename(&target, &staged) {
            return Err(rollback_removal(
                &moved,
                staging,
                ModInstallError::Io(error),
            ));
        }
        moved.push((target, staged.clone()));
        match validate_tracked_mod_file(&staged, installed) {
            Ok(true) => {}
            Ok(false) => {
                return Err(rollback_removal(
                    &moved,
                    staging,
                    ModInstallError::ModifiedTrackedFile(installed.file_name.clone()),
                ));
            }
            Err(error) => return Err(rollback_removal(&moved, staging, error)),
        }
    }
    Ok(moved)
}

fn validate_tracked_mod_file(
    path: &Path,
    installed: &InstalledMod,
) -> Result<bool, ModInstallError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(ModInstallError::Io(error)),
    };
    if !metadata.is_file()
        || metadata.len() != installed.size
        || file_sha512(path)? != installed.sha512
    {
        return Err(ModInstallError::ModifiedTrackedFile(
            installed.file_name.clone(),
        ));
    }
    Ok(true)
}

fn rollback_removal(
    moved: &[(std::path::PathBuf, std::path::PathBuf)],
    staging: &Path,
    original_error: ModInstallError,
) -> ModInstallError {
    for (target, staged) in moved.iter().rev() {
        if !staged.exists() {
            continue;
        }
        if target.exists() {
            return ModInstallError::RemovalRollbackFailed(format!(
                "{original_error}; 復元先に別のファイルがあります: {}",
                target.display()
            ));
        }
        if let Err(error) = fs::rename(staged, target) {
            return ModInstallError::RemovalRollbackFailed(format!(
                "{original_error}; {}: {error}",
                target.display()
            ));
        }
    }
    let _ = fs::remove_dir(staging);
    original_error
}

fn cleanup_staged_files(
    moved: &[(std::path::PathBuf, std::path::PathBuf)],
    staging: &Path,
) -> bool {
    let mut cleanup_pending = false;
    for (_, staged) in moved {
        if staged.exists() && fs::remove_file(staged).is_err() {
            cleanup_pending = true;
        }
    }
    if staging.exists() && fs::remove_dir(staging).is_err() {
        cleanup_pending = true;
    }
    cleanup_pending
}

fn download_mod_file(
    client: &ModrinthClient,
    file: &ModrinthFile,
    destination: &Path,
) -> Result<(), ModInstallError> {
    let url = validate_download_url(&file.url)?;
    let expected_hash = validate_sha512(&file.hashes.sha512)?;
    if file.size > MAX_MOD_SIZE {
        return Err(ModInstallError::FileTooLarge {
            file_name: file.filename.clone(),
            size: file.size,
        });
    }
    let mut response = client.download(url)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_MOD_SIZE || size != file.size)
    {
        return Err(ModInstallError::SizeMismatch {
            file_name: file.filename.clone(),
            expected: file.size,
            actual: response.content_length().unwrap_or_default(),
        });
    }
    let mut output = File::create(destination)?;
    let mut hasher = Sha512::new();
    let mut actual_size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        actual_size += count as u64;
        if actual_size > MAX_MOD_SIZE {
            drop(output);
            let _ = fs::remove_file(destination);
            return Err(ModInstallError::FileTooLarge {
                file_name: file.filename.clone(),
                size: actual_size,
            });
        }
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    output.flush()?;
    drop(output);

    if actual_size != file.size {
        let _ = fs::remove_file(destination);
        return Err(ModInstallError::SizeMismatch {
            file_name: file.filename.clone(),
            expected: file.size,
            actual: actual_size,
        });
    }
    let actual_hash = format!("{:x}", hasher.finalize());
    if actual_hash != expected_hash {
        let _ = fs::remove_file(destination);
        return Err(ModInstallError::HashMismatch {
            file_name: file.filename.clone(),
            expected: expected_hash,
            actual: actual_hash,
        });
    }
    Ok(())
}

fn load_registry(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<ModRegistry, ModInstallError> {
    let path = paths.instance_mod_registry(instance_id);
    if !path.exists() {
        return Ok(ModRegistry::default());
    }
    let mut registry: ModRegistry = serde_json::from_slice(
        &read_bounded_file(&path, MAX_REGISTRY_SIZE).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                ModInstallError::RegistryTooLarge
            } else {
                ModInstallError::Io(error)
            }
        })?,
    )?;
    if registry.mods.len() > MAX_REGISTRY_MODS {
        return Err(ModInstallError::RegistryTooLarge);
    }
    let mut project_ids = HashSet::new();
    let mut file_names = HashSet::new();
    for installed in &mut registry.mods {
        validate_identifier(&installed.project_id)?;
        validate_identifier(&installed.version_id)?;
        validate_file_name(&installed.file_name)?;
        installed.title = sanitize_text(std::mem::take(&mut installed.title), 120);
        installed.version_number = sanitize_text(std::mem::take(&mut installed.version_number), 80);
        installed.sha512 = validate_sha512(&installed.sha512)?;
        if installed.size > MAX_MOD_SIZE {
            return Err(ModInstallError::FileTooLarge {
                file_name: installed.file_name.clone(),
                size: installed.size,
            });
        }
        if !project_ids.insert(installed.project_id.clone()) {
            return Err(ModInstallError::DuplicateRegistryProject(
                installed.project_id.clone(),
            ));
        }
        if !file_names.insert(installed.file_name.to_lowercase()) {
            return Err(ModInstallError::FileNameConflict(
                installed.file_name.clone(),
            ));
        }
        if let Some(dependencies) = &mut installed.required_dependencies {
            if dependencies.len() > MAX_DEPENDENCIES {
                return Err(ModInstallError::DependencyLimitExceeded);
            }
            for dependency in dependencies.iter() {
                validate_identifier(dependency)?;
            }
            dependencies.sort();
            dependencies.dedup();
        }
    }
    Ok(registry)
}

fn save_registry(
    paths: &MinecraftPaths,
    instance_id: &str,
    registry: &ModRegistry,
) -> Result<(), ModInstallError> {
    let bytes = serde_json::to_vec_pretty(registry)?;
    if bytes.len() as u64 > MAX_REGISTRY_SIZE {
        return Err(ModInstallError::RegistryTooLarge);
    }
    write_atomic(&paths.instance_mod_registry(instance_id), &bytes)?;
    Ok(())
}

fn reset_staging_directory(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    staging: &Path,
) -> Result<(), ModInstallError> {
    let expected_parent = paths.instance(&instance.id);
    if staging.parent() != Some(expected_parent.as_path()) {
        return Err(ModInstallError::InvalidFileName(
            staging.display().to_string(),
        ));
    }
    match fs::symlink_metadata(staging) {
        Ok(metadata) => {
            // Old launcher versions allowed the game to modify the instance root. Refuse a
            // junction left at the trusted staging path instead of recursively touching its
            // destination with launcher privileges.
            if !metadata.is_dir() || path_is_link_or_reparse(staging)? {
                return Err(ModInstallError::InvalidInstance(instance.id.clone()));
            }
            fs::remove_dir_all(staging)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir(staging)?;
    if path_is_link_or_reparse(staging)? {
        let _ = fs::remove_dir(staging);
        return Err(ModInstallError::InvalidInstance(instance.id.clone()));
    }
    Ok(())
}

fn validate_file_name(file_name: &str) -> Result<(), ModInstallError> {
    let path = Path::new(file_name);
    let mut components = path.components();
    let valid_component = matches!(components.next(), Some(Component::Normal(_)));
    if !valid_component
        || components.next().is_some()
        || file_name.chars().count() > 240
        || file_name.chars().any(char::is_control)
        || file_name.contains(['\\', ':'])
        || !is_jar_name(file_name)
    {
        return Err(ModInstallError::InvalidFileName(file_name.to_owned()));
    }
    Ok(())
}

fn is_jar_name(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
}

fn validate_download_url(value: &str) -> Result<Url, ModInstallError> {
    let url =
        Url::parse(value).map_err(|_| ModInstallError::InvalidDownloadUrl(value.to_owned()))?;
    if url.scheme() != "https"
        || url.host_str() != Some(MODRINTH_CDN_HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return Err(ModInstallError::InvalidDownloadUrl(value.to_owned()));
    }
    Ok(url)
}

fn validate_sha512(value: &str) -> Result<String, ModInstallError> {
    let value = value.trim();
    if value.len() != 128 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ModInstallError::InvalidHash(value.to_owned()));
    }
    Ok(value.to_ascii_lowercase())
}

fn file_matches(path: &Path, file: &ModrinthFile) -> Result<bool, ModInstallError> {
    Ok(fs::metadata(path)?.len() == file.size
        && file_sha512(path)? == file.hashes.sha512.to_ascii_lowercase())
}

fn file_sha512(path: &Path) -> Result<String, ModInstallError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sanitize_text(value: String, maximum: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(maximum)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_instance(test_name: &str) -> (std::path::PathBuf, MinecraftPaths, InstanceManifest) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "monalauncher-mod-removal-{test_name}-{}-{timestamp}",
            process::id()
        ));
        let paths = MinecraftPaths::new(root.clone());
        let instance_id = "testmods";
        let game_directory = paths.instance_game_directory(instance_id);
        fs::create_dir_all(&game_directory).unwrap();
        let instance = InstanceManifest {
            id: instance_id.to_owned(),
            name: "Removal test".to_owned(),
            version_id: "1.21.8".to_owned(),
            java_path: String::new(),
            game_directory: game_directory.to_string_lossy().into_owned(),
            demo: false,
            sandboxed: true,
            permissions: Default::default(),
            mod_loader: ModLoader::Fabric {
                version: "0.17.2".to_owned(),
            },
        };
        (root, paths, instance)
    }

    fn installed_mod(project_id: &str, direct: bool, dependencies: &[&str]) -> InstalledMod {
        let contents = project_id.as_bytes();
        InstalledMod {
            project_id: project_id.to_owned(),
            version_id: format!("V{}", &project_id[..7]),
            title: project_id.to_owned(),
            version_number: "1.0.0".to_owned(),
            file_name: format!("{project_id}.jar"),
            sha512: format!("{:x}", Sha512::digest(contents)),
            size: contents.len() as u64,
            direct,
            required_dependencies: Some(
                dependencies
                    .iter()
                    .map(|dependency| (*dependency).to_owned())
                    .collect(),
            ),
        }
    }

    fn write_registry(paths: &MinecraftPaths, instance_id: &str, mods: Vec<InstalledMod>) {
        let mods_directory = paths.instance_mods_directory(instance_id);
        fs::create_dir_all(&mods_directory).unwrap();
        for installed in &mods {
            fs::write(
                mods_directory.join(&installed.file_name),
                installed.project_id.as_bytes(),
            )
            .unwrap();
        }
        save_registry(paths, instance_id, &ModRegistry { mods }).unwrap();
    }

    fn version(version_type: &str) -> ModrinthVersion {
        ModrinthVersion {
            id: "12345678".to_owned(),
            project_id: "ABCDEFGH".to_owned(),
            name: "Test".to_owned(),
            version_number: "1.0.0".to_owned(),
            version_type: version_type.to_owned(),
            date_published: "2026-01-01T00:00:00Z".to_owned(),
            game_versions: vec!["1.21.8".to_owned()],
            loaders: vec!["fabric".to_owned()],
            dependencies: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn prefers_release_versions() {
        let versions = [version("beta"), version("release")];

        assert_eq!(
            select_preferred_version(&versions).unwrap().version_type,
            "release"
        );
    }

    #[test]
    fn accepts_only_single_jar_file_names() {
        assert!(validate_file_name("sodium+1.0.jar").is_ok());
        for invalid in ["../evil.jar", "sub/mod.jar", "mod.exe", "C:\\evil.jar"] {
            assert!(validate_file_name(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn accepts_only_modrinth_https_downloads() {
        assert!(validate_download_url(
            "https://cdn.modrinth.com/data/AANobbMI/versions/123/file.jar"
        )
        .is_ok());
        assert!(validate_download_url("http://cdn.modrinth.com/file.jar").is_err());
        assert!(validate_download_url("https://example.com/file.jar").is_err());
    }

    #[test]
    fn validates_sha512_hashes() {
        assert!(validate_sha512(&"a".repeat(128)).is_ok());
        assert!(validate_sha512("../bad").is_err());
    }

    #[test]
    fn restores_mod_files_when_registry_commit_fails() {
        let (root, paths, instance) = test_instance("install-rollback");
        let old = installed_mod("AAAAAAA1", true, &[]);
        write_registry(&paths, &instance.id, vec![old.clone()]);
        let staging = paths.instance_mod_staging(&instance.id);
        fs::create_dir_all(&staging).unwrap();
        let replacement = b"replacement mod";
        let replacement_name = "replacement.jar";
        fs::write(staging.join(replacement_name), replacement).unwrap();
        let file = ModrinthFile {
            hashes: super::super::modrinth::ModrinthFileHashes {
                sha512: format!("{:x}", Sha512::digest(replacement)),
                sha1: "0".repeat(40),
            },
            url: "https://cdn.modrinth.com/data/test/replacement.jar".to_owned(),
            filename: replacement_name.to_owned(),
            primary: true,
            size: replacement.len() as u64,
            file_type: None,
        };
        let planned = PlannedMod {
            project: ModrinthProject {
                id: old.project_id.clone(),
                title: "Replacement".to_owned(),
                description: String::new(),
                slug: None,
            },
            version: ModrinthVersion {
                id: "12345678".to_owned(),
                project_id: old.project_id.clone(),
                name: "Replacement".to_owned(),
                version_number: "2.0.0".to_owned(),
                version_type: "release".to_owned(),
                date_published: "2026-01-01T00:00:00Z".to_owned(),
                game_versions: vec![instance.version_id.clone()],
                loaders: vec!["fabric".to_owned()],
                dependencies: Vec::new(),
                files: vec![file.clone()],
            },
            file,
            direct: true,
        };
        let registry_path = paths.instance_mod_registry(&instance.id);
        fs::remove_file(&registry_path).unwrap();
        fs::create_dir(&registry_path).unwrap();

        let result = commit_installation(
            &paths,
            &instance,
            ModRegistry {
                mods: vec![old.clone()],
            },
            vec![planned],
            &staging,
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read(
                paths
                    .instance_mods_directory(&instance.id)
                    .join(&old.file_name)
            )
            .unwrap(),
            old.project_id.as_bytes()
        );
        assert!(!paths
            .instance_mods_directory(&instance.id)
            .join(replacement_name)
            .exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removes_only_the_requested_mod_and_orphaned_dependencies() {
        let (root, paths, instance) = test_instance("orphans");
        let first = installed_mod("AAAAAAA1", true, &["CCCCCCC3", "DDDDDDD4"]);
        let second = installed_mod("BBBBBBB2", true, &["CCCCCCC3"]);
        let shared = installed_mod("CCCCCCC3", false, &[]);
        let orphan = installed_mod("DDDDDDD4", false, &[]);
        write_registry(
            &paths,
            &instance.id,
            vec![
                first.clone(),
                second.clone(),
                shared.clone(),
                orphan.clone(),
            ],
        );

        let result = remove_modrinth_project(&paths, &instance, &first.project_id).unwrap();
        let removed = result
            .removed
            .iter()
            .map(|installed| installed.project_id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(removed, HashSet::from(["AAAAAAA1", "DDDDDDD4"]));
        assert!(!result.retained_as_dependency);
        assert!(!result.cleanup_pending);

        let remaining = list_installed_mods(&paths, &instance.id).unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining
            .iter()
            .any(|installed| installed.project_id == second.project_id));
        assert!(remaining
            .iter()
            .any(|installed| installed.project_id == shared.project_id));
        assert!(!paths
            .instance_mods_directory(&instance.id)
            .join(first.file_name)
            .exists());
        assert!(!paths
            .instance_mods_directory(&instance.id)
            .join(orphan.file_name)
            .exists());
        assert!(paths
            .instance_mods_directory(&instance.id)
            .join(shared.file_name)
            .is_file());

        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_a_removed_direct_mod_when_another_mod_requires_it() {
        let (root, paths, instance) = test_instance("shared-direct");
        let requested = installed_mod("AAAAAAA1", true, &[]);
        let consumer = installed_mod("BBBBBBB2", true, &["AAAAAAA1"]);
        write_registry(&paths, &instance.id, vec![requested.clone(), consumer]);

        let result = remove_modrinth_project(&paths, &instance, &requested.project_id).unwrap();
        assert!(result.retained_as_dependency);
        assert!(result.removed.is_empty());
        let remaining = list_installed_mods(&paths, &instance.id).unwrap();
        let retained = remaining
            .iter()
            .find(|installed| installed.project_id == requested.project_id)
            .unwrap();
        assert!(!retained.direct);
        assert!(paths
            .instance_mods_directory(&instance.id)
            .join(requested.file_name)
            .is_file());

        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_remove_a_tracked_mod_that_was_modified() {
        let (root, paths, instance) = test_instance("modified");
        let requested = installed_mod("AAAAAAA1", true, &[]);
        write_registry(&paths, &instance.id, vec![requested.clone()]);
        let target = paths
            .instance_mods_directory(&instance.id)
            .join(&requested.file_name);
        fs::write(&target, b"CHANGED!").unwrap();

        let error = remove_modrinth_project(&paths, &instance, &requested.project_id).unwrap_err();
        assert!(matches!(error, ModInstallError::ModifiedTrackedFile(_)));
        assert!(target.is_file());
        assert!(
            list_installed_mods(&paths, &instance.id)
                .unwrap()
                .first()
                .unwrap()
                .direct
        );

        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_remove_a_dependency_directly() {
        let (root, paths, instance) = test_instance("dependency-only");
        let dependency = installed_mod("AAAAAAA1", false, &[]);
        write_registry(&paths, &instance.id, vec![dependency.clone()]);

        let error = remove_modrinth_project(&paths, &instance, &dependency.project_id).unwrap_err();
        assert!(matches!(error, ModInstallError::DependencyOnly(_)));

        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
}
