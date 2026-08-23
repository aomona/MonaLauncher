use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use super::model::{InstanceManifest, ModLoader};
use super::modrinth::{
    ModrinthClient, ModrinthDependency, ModrinthError, ModrinthFile, ModrinthProject,
    ModrinthVersion,
};
use super::paths::MinecraftPaths;

const MAX_MOD_SIZE: u64 = 512 * 1024 * 1024;
const MAX_REGISTRY_SIZE: u64 = 1024 * 1024;
const MAX_DEPENDENCIES: usize = 64;
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
    RegistryTooLarge,
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
            Self::RegistryTooLarge => write!(formatter, "Modインストール記録が大きすぎます"),
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallResult {
    pub installed: Vec<InstalledMod>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallProgress {
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
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

    for item in planned {
        let staged = staging.join(&item.file.filename);
        let target = mods_directory.join(&item.file.filename);
        if staged.is_file() {
            if target.exists() {
                fs::remove_file(&target)?;
            }
            fs::rename(staged, &target)?;
        }
        let previous_direct = registry
            .mods
            .iter()
            .find(|installed| installed.project_id == item.project.id)
            .is_some_and(|installed| installed.direct);
        let installed = InstalledMod {
            project_id: item.project.id,
            version_id: item.version.id,
            title: sanitize_text(item.project.title, 120),
            version_number: sanitize_text(item.version.version_number, 80),
            file_name: item.file.filename,
            sha512: item.file.hashes.sha512.to_ascii_lowercase(),
            size: item.file.size,
            direct: item.direct || previous_direct,
        };
        registry
            .mods
            .retain(|existing| existing.project_id != installed.project_id);
        registry.mods.push(installed.clone());
        installed_now.push(installed);
    }

    save_registry(paths, &instance.id, &registry)?;
    remove_replaced_files(paths, instance, &old_registry, &registry)?;
    Ok(ModInstallResult {
        installed: installed_now,
    })
}

fn remove_replaced_files(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    old_registry: &[InstalledMod],
    new_registry: &ModRegistry,
) -> Result<(), ModInstallError> {
    let referenced = new_registry
        .mods
        .iter()
        .map(|installed| installed.file_name.to_lowercase())
        .collect::<HashSet<_>>();
    for old in old_registry {
        validate_file_name(&old.file_name)?;
        if referenced.contains(&old.file_name.to_lowercase()) {
            continue;
        }
        let path = paths
            .instance_mods_directory(&instance.id)
            .join(&old.file_name);
        if path.is_file() && file_sha512(&path)? == old.sha512 {
            fs::remove_file(path)?;
        }
    }
    Ok(())
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
    let metadata = fs::metadata(&path)?;
    if metadata.len() > MAX_REGISTRY_SIZE {
        return Err(ModInstallError::RegistryTooLarge);
    }
    let registry: ModRegistry = serde_json::from_slice(&fs::read(path)?)?;
    for installed in &registry.mods {
        validate_file_name(&installed.file_name)?;
        validate_sha512(&installed.sha512)?;
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
    fs::write(paths.instance_mod_registry(instance_id), bytes)?;
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
    if staging.exists() {
        fs::remove_dir_all(staging)?;
    }
    fs::create_dir_all(staging)?;
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
}
