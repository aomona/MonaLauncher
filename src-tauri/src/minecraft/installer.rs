use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rayon::prelude::*;
use reqwest::blocking::{Client, Response};
use reqwest::Url;
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};

use super::fabric::{install_fabric, FabricError};
use super::file_io::{
    path_is_link_or_reparse, read_bounded_file, replace_file_atomic, write_atomic,
};
use super::model::{
    rules_allow, AssetIndex, DownloadInfo, InstallProgress, InstanceManifest, ModLoader,
    VersionManifest, VersionMetadata,
};
use super::paths::MinecraftPaths;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_OBJECT_BASE_URL: &str = "https://resources.download.minecraft.net";
const MAX_INSTANCE_ID_LENGTH: usize = 41;
const MAX_INSTANCE_NAME_LENGTH: usize = 80;
const MAX_METADATA_RESPONSE_SIZE: u64 = 16 * 1024 * 1024;
const MAX_DOWNLOAD_SIZE: u64 = 512 * 1024 * 1024;
const MAX_DOWNLOAD_TASKS: usize = 100_000;
const MAX_DOWNLOAD_PLAN_SIZE: u64 = 8 * 1024 * 1024 * 1024;
const MAX_INSTANCE_MANIFEST_SIZE: u64 = 1024 * 1024;
const MAX_METADATA_IDENTIFIER_LENGTH: usize = 128;
const MAX_VERSION_CATALOG_ENTRIES: usize = 20_000;
const MINECRAFT_DOWNLOAD_HOSTS: &[&str] = &[
    "launcher.mojang.com",
    "launchermeta.mojang.com",
    "libraries.minecraft.net",
    "piston-data.mojang.com",
    "piston-meta.mojang.com",
    "resources.download.minecraft.net",
];

#[derive(Debug)]
pub enum MinecraftInstallError {
    EmptyInstanceId,
    EmptyInstanceName,
    InvalidInstanceIdCharacter(char),
    InstanceIdTooLong(usize),
    InstanceNameTooLong(usize),
    InstanceAlreadyExists(String),
    InstanceMissing(String),
    InstanceIdMismatch {
        expected: String,
        actual: String,
    },
    VersionIdMismatch {
        expected: String,
        actual: String,
    },
    UnsafeInstancePath(PathBuf),
    UnmanagedJavaRuntime(PathBuf),
    InvalidMetadataPath(String),
    InvalidAssetHash(String),
    InvalidDownloadUrl(String),
    InvalidChecksum(String),
    ResponseTooLarge,
    DownloadPlanTooLarge,
    JavaNotFound(PathBuf),
    LatestReleaseMissing(String),
    VersionMissing(String),
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Fabric(FabricError),
}

impl fmt::Display for MinecraftInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInstanceId => write!(formatter, "instance ID must not be empty"),
            Self::EmptyInstanceName => write!(formatter, "instance name must not be empty"),
            Self::InvalidInstanceIdCharacter(character) => write!(
                formatter,
                "instance ID contains an invalid character: {character:?}"
            ),
            Self::InstanceIdTooLong(length) => write!(
                formatter,
                "instance ID must not exceed {MAX_INSTANCE_ID_LENGTH} characters (got {length})"
            ),
            Self::InstanceNameTooLong(length) => write!(
                formatter,
                "instance name must not exceed {MAX_INSTANCE_NAME_LENGTH} characters (got {length})"
            ),
            Self::InstanceAlreadyExists(instance_id) => {
                write!(
                    formatter,
                    "Minecraft instance already exists: {instance_id}"
                )
            }
            Self::InstanceMissing(instance_id) => {
                write!(formatter, "Minecraft instance was not found: {instance_id}")
            }
            Self::InstanceIdMismatch { expected, actual } => write!(
                formatter,
                "instance manifest ID mismatch: expected {expected}, got {actual}"
            ),
            Self::VersionIdMismatch { expected, actual } => write!(
                formatter,
                "version metadata ID mismatch: expected {expected}, got {actual}"
            ),
            Self::UnsafeInstancePath(path) => write!(
                formatter,
                "instance storage must be a real directory, not a link: {}",
                path.display()
            ),
            Self::UnmanagedJavaRuntime(path) => write!(
                formatter,
                "sandboxed instances must use a managed Java runtime: {}",
                path.display()
            ),
            Self::InvalidMetadataPath(path) => {
                write!(formatter, "metadata contains an unsafe path: {path}")
            }
            Self::InvalidAssetHash(hash) => {
                write!(formatter, "metadata contains an invalid asset hash: {hash}")
            }
            Self::InvalidDownloadUrl(url) => {
                write!(formatter, "metadata contains an unsafe download URL: {url}")
            }
            Self::InvalidChecksum(checksum) => {
                write!(formatter, "metadata contains an invalid SHA-1: {checksum}")
            }
            Self::ResponseTooLarge => write!(formatter, "downloaded response is too large"),
            Self::DownloadPlanTooLarge => write!(
                formatter,
                "download plan exceeds the safe file-count or total-size limit"
            ),
            Self::JavaNotFound(path) => {
                write!(
                    formatter,
                    "Java executable was not found: {}",
                    path.display()
                )
            }
            Self::LatestReleaseMissing(version) => {
                write!(formatter, "latest release metadata is missing: {version}")
            }
            Self::VersionMissing(version) => {
                write!(
                    formatter,
                    "Minecraft version metadata is missing: {version}"
                )
            }
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "SHA-1 mismatch for {}: expected {expected}, got {actual}",
                path.display()
            ),
            Self::SizeMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "size mismatch for {}: expected {expected}, got {actual}",
                path.display()
            ),
            Self::Io(error) => write!(formatter, "file system error: {error}"),
            Self::Http(error) => write!(formatter, "download error: {error}"),
            Self::Json(error) => write!(formatter, "metadata error: {error}"),
            Self::Fabric(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for MinecraftInstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Fabric(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MinecraftInstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<reqwest::Error> for MinecraftInstallError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for MinecraftInstallError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<FabricError> for MinecraftInstallError {
    fn from(error: FabricError) -> Self {
        Self::Fabric(error)
    }
}

#[derive(Debug, Clone)]
struct DownloadTask {
    url: String,
    sha1: String,
    size: u64,
    target: PathBuf,
}

#[derive(Clone)]
struct InstanceInstallOptions<'a> {
    requested_version: Option<&'a str>,
    demo: bool,
    mod_loader: ModLoader,
}

pub fn install_sandbox_demo_instance<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    version_id: &str,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    install_sandbox_instance(
        paths,
        instance_id,
        instance_name,
        java_path,
        version_id,
        true,
        progress,
    )
}

pub fn install_sandbox_instance<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    version_id: &str,
    demo: bool,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    install_sandbox_instance_with_loader(
        paths,
        instance_id,
        instance_name,
        java_path,
        version_id,
        demo,
        ModLoader::Vanilla,
        progress,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn install_sandbox_instance_with_loader<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    version_id: &str,
    demo: bool,
    mod_loader: ModLoader,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    install_instance_from_manifest(
        paths,
        instance_id,
        instance_name,
        java_path,
        InstanceInstallOptions {
            requested_version: Some(version_id),
            demo,
            mod_loader,
        },
        progress,
    )
}

pub fn install_latest_sandbox_demo_instance<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    install_latest_sandbox_instance(paths, instance_id, instance_name, java_path, true, progress)
}

pub fn install_latest_sandbox_instance<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    demo: bool,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    install_instance_from_manifest(
        paths,
        instance_id,
        instance_name,
        java_path,
        InstanceInstallOptions {
            requested_version: None,
            demo,
            mod_loader: ModLoader::Vanilla,
        },
        progress,
    )
}

pub fn list_available_versions() -> Result<VersionManifest, MinecraftInstallError> {
    let client = minecraft_client()?;
    let mut manifest = fetch_json(&client, VERSION_MANIFEST_URL)?;
    validate_version_manifest(&mut manifest)?;
    Ok(manifest)
}

pub fn version_java_major(version_id: &str) -> Result<u32, MinecraftInstallError> {
    validate_version_identifier(version_id)?;
    let client = minecraft_client()?;
    let mut manifest: VersionManifest = fetch_json(&client, VERSION_MANIFEST_URL)?;
    validate_version_manifest(&mut manifest)?;
    let selected = manifest
        .versions
        .into_iter()
        .find(|version| version.id == version_id)
        .ok_or_else(|| MinecraftInstallError::VersionMissing(version_id.to_owned()))?;
    let version_bytes = fetch_bytes(&client, &selected.url)?;
    verify_bytes_sha1(&version_bytes, &selected.sha1, PathBuf::from(version_id))?;
    let version: VersionMetadata = serde_json::from_slice(&version_bytes)?;
    if version.id != version_id {
        return Err(MinecraftInstallError::VersionIdMismatch {
            expected: version_id.to_owned(),
            actual: version.id,
        });
    }
    Ok(version
        .java_version
        .map(|java| java.major_version)
        .unwrap_or(8))
}

fn install_instance_from_manifest<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
    options: InstanceInstallOptions<'_>,
    progress: F,
) -> Result<InstanceManifest, MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    validate_instance_id(instance_id)?;
    let instance_name = validate_instance_name(instance_name)?;
    if let Some(version_id) = options.requested_version {
        validate_version_identifier(version_id)?;
    }

    if paths.instance_manifest(instance_id).is_file() {
        return Err(MinecraftInstallError::InstanceAlreadyExists(
            instance_id.to_owned(),
        ));
    }

    if !java_path.is_file() {
        return Err(MinecraftInstallError::JavaNotFound(java_path.to_owned()));
    }
    let managed_java = validate_managed_java(paths, java_path)?;

    create_base_directories(paths)?;

    let client = minecraft_client()?;

    progress_event(&progress, "metadata", 0, 1, "Fetching version manifest");
    let mut manifest: VersionManifest = fetch_json(&client, VERSION_MANIFEST_URL)?;
    validate_version_manifest(&mut manifest)?;
    let release_id = options
        .requested_version
        .map(str::to_owned)
        .unwrap_or(manifest.latest.release);
    let release = manifest
        .versions
        .into_iter()
        .find(|version| version.id == release_id)
        .ok_or_else(|| {
            if options.requested_version.is_some() {
                MinecraftInstallError::VersionMissing(release_id.clone())
            } else {
                MinecraftInstallError::LatestReleaseMissing(release_id.clone())
            }
        })?;

    progress_event(
        &progress,
        "metadata",
        0,
        1,
        &format!("Fetching Minecraft {release_id} metadata"),
    );
    let version_bytes = fetch_bytes(&client, &release.url)?;
    verify_bytes_sha1(
        &version_bytes,
        &release.sha1,
        paths.version_json(&release_id),
    )?;
    let version: VersionMetadata = serde_json::from_slice(&version_bytes)?;
    if version.id != release_id {
        return Err(MinecraftInstallError::VersionIdMismatch {
            expected: release_id,
            actual: version.id,
        });
    }
    validate_version_identifier(&version.id)?;
    validate_metadata_identifier(&version.asset_index.id)?;

    let version_directory = paths.version_directory(&version.id);
    fs::create_dir_all(&version_directory)?;
    write_atomic(&paths.version_json(&version.id), &version_bytes)?;

    progress_event(&progress, "client", 0, 1, "Downloading Minecraft client");
    download_file(
        &client,
        &DownloadTask::from_info(&version.downloads.client, paths.version_jar(&version.id)),
    )?;
    progress_event(&progress, "client", 1, 1, "Minecraft client ready");

    let features = HashMap::from([("is_demo_user".to_owned(), options.demo)]);
    let mut library_tasks = BTreeMap::<PathBuf, DownloadTask>::new();

    for library in &version.libraries {
        if !rules_allow(library.rules.as_deref(), &features) {
            continue;
        }

        if let Some(native) = library.platform_native() {
            if let Some(relative_path) = native.path.as_deref() {
                let target = safe_metadata_join(&paths.libraries(), relative_path)?;
                library_tasks
                    .entry(target.clone())
                    .or_insert_with(|| DownloadTask::from_info(native, target));
            }
        }

        if let Some(artifact) = &library.downloads.artifact {
            if let Some(relative_path) = artifact.path.as_deref() {
                let target = safe_metadata_join(&paths.libraries(), relative_path)?;
                library_tasks
                    .entry(target.clone())
                    .or_insert_with(|| DownloadTask::from_info(artifact, target));
            }
        }
    }

    download_tasks(
        &client,
        library_tasks.into_values().collect(),
        "libraries",
        &progress,
    )?;

    progress_event(&progress, "assets-index", 0, 1, "Downloading asset index");
    let asset_index_bytes = fetch_bytes(&client, &version.asset_index.url)?;
    verify_bytes_sha1(
        &asset_index_bytes,
        &version.asset_index.sha1,
        paths
            .asset_indexes()
            .join(format!("{}.json", version.asset_index.id)),
    )?;
    let asset_index: AssetIndex = serde_json::from_slice(&asset_index_bytes)?;
    fs::create_dir_all(paths.asset_indexes())?;
    write_atomic(
        paths
            .asset_indexes()
            .join(format!("{}.json", version.asset_index.id))
            .as_path(),
        &asset_index_bytes,
    )?;
    progress_event(&progress, "assets-index", 1, 1, "Asset index ready");

    let mut asset_tasks = BTreeMap::<PathBuf, DownloadTask>::new();

    for object in asset_index.objects.into_values() {
        validate_asset_hash(&object.hash)?;
        let prefix = &object.hash[..2];
        let target = paths.asset_objects().join(prefix).join(&object.hash);
        let url = format!("{ASSET_OBJECT_BASE_URL}/{prefix}/{}", object.hash);

        asset_tasks.entry(target.clone()).or_insert(DownloadTask {
            url,
            sha1: object.hash,
            size: object.size,
            target,
        });
    }

    download_tasks(
        &client,
        asset_tasks.into_values().collect(),
        "assets",
        &progress,
    )?;

    let game_directory = paths.instance_game_directory(instance_id);
    fs::create_dir_all(&game_directory)?;
    if let ModLoader::Fabric { version: loader } = &options.mod_loader {
        install_fabric(paths, instance_id, &version.id, loader, &progress)?;
    }

    let instance = InstanceManifest {
        id: instance_id.to_owned(),
        name: instance_name.to_owned(),
        version_id: version.id,
        java_path: managed_java.to_string_lossy().into_owned(),
        game_directory: game_directory.to_string_lossy().into_owned(),
        demo: options.demo,
        sandboxed: true,
        permissions: Default::default(),
        mod_loader: options.mod_loader,
    };

    let instance_directory = paths.instance(instance_id);
    fs::create_dir_all(&instance_directory)?;
    write_atomic(
        &paths.instance_manifest(instance_id),
        &serde_json::to_vec_pretty(&instance)?,
    )?;

    progress_event(
        &progress,
        "complete",
        1,
        1,
        if options.demo {
            "Demo instance is ready"
        } else {
            "Offline instance is ready"
        },
    );

    Ok(instance)
}

pub fn list_instances(
    paths: &MinecraftPaths,
) -> Result<Vec<InstanceManifest>, MinecraftInstallError> {
    if !paths.instances().is_dir() {
        return Ok(Vec::new());
    }

    let mut instances = Vec::new();

    for entry in fs::read_dir(paths.instances())? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || path_is_link_or_reparse(&entry.path())? {
            continue;
        }
        let Some(directory_id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if validate_instance_id(&directory_id).is_err()
            || !paths.instance_manifest(&directory_id).is_file()
        {
            continue;
        }
        instances.push(load_instance(paths, &directory_id)?);
    }

    instances.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(instances)
}

pub(crate) fn load_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<InstanceManifest, MinecraftInstallError> {
    let mut instance = load_instance_for_repair(paths, instance_id)?;
    if instance.sandboxed {
        instance.java_path = validate_managed_java(paths, Path::new(&instance.java_path))?
            .to_string_lossy()
            .into_owned();
    }
    Ok(instance)
}

pub(crate) fn load_instance_for_repair(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<InstanceManifest, MinecraftInstallError> {
    validate_instance_id(instance_id)?;
    let instance_directory = paths.instance(instance_id);
    if !instance_directory.is_dir() || path_is_link_or_reparse(&instance_directory)? {
        return Err(MinecraftInstallError::UnsafeInstancePath(
            instance_directory,
        ));
    }
    let manifest_path = paths.instance_manifest(instance_id);
    if !manifest_path.is_file() {
        return Err(MinecraftInstallError::InstanceMissing(
            instance_id.to_owned(),
        ));
    }
    let mut instance: InstanceManifest = serde_json::from_slice(&read_bounded_file(
        &manifest_path,
        MAX_INSTANCE_MANIFEST_SIZE,
    )?)?;
    if instance.id != instance_id {
        return Err(MinecraftInstallError::InstanceIdMismatch {
            expected: instance_id.to_owned(),
            actual: instance.id,
        });
    }
    instance.name = validate_instance_name(&instance.name)?.to_owned();
    validate_version_identifier(&instance.version_id)?;
    if let ModLoader::Fabric { version } = &instance.mod_loader {
        validate_metadata_identifier(version)?;
    }
    instance.game_directory = paths
        .instance_game_directory(instance_id)
        .to_string_lossy()
        .into_owned();
    let game_directory = Path::new(&instance.game_directory);
    if game_directory.exists()
        && (!game_directory.is_dir() || path_is_link_or_reparse(game_directory)?)
    {
        return Err(MinecraftInstallError::UnsafeInstancePath(
            game_directory.to_owned(),
        ));
    }
    Ok(instance)
}

pub fn rename_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
    name: &str,
) -> Result<InstanceManifest, MinecraftInstallError> {
    validate_instance_id(instance_id)?;
    let name = validate_instance_name(name)?;
    let manifest_path = paths.instance_manifest(instance_id);
    let mut instance = load_instance(paths, instance_id)?;

    instance.name = name.to_owned();
    write_atomic(&manifest_path, &serde_json::to_vec_pretty(&instance)?)?;
    Ok(instance)
}

pub fn delete_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<(), MinecraftInstallError> {
    validate_instance_id(instance_id)?;
    load_instance(paths, instance_id)?;

    fs::remove_dir_all(paths.instance(instance_id))?;
    Ok(())
}

fn create_base_directories(paths: &MinecraftPaths) -> Result<(), MinecraftInstallError> {
    for directory in [
        paths.root().to_owned(),
        paths.instances(),
        paths.libraries(),
        paths.asset_indexes(),
        paths.asset_objects(),
        paths.versions(),
    ] {
        fs::create_dir_all(directory)?;
    }

    Ok(())
}

fn download_tasks<F>(
    client: &Client,
    tasks: Vec<DownloadTask>,
    stage: &str,
    progress: &F,
) -> Result<(), MinecraftInstallError>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    let aggregate_size = tasks.iter().try_fold(0_u64, |total, task| {
        total
            .checked_add(task.size)
            .filter(|size| *size <= MAX_DOWNLOAD_PLAN_SIZE)
            .ok_or(MinecraftInstallError::DownloadPlanTooLarge)
    })?;
    if tasks.len() > MAX_DOWNLOAD_TASKS || aggregate_size > MAX_DOWNLOAD_PLAN_SIZE {
        return Err(MinecraftInstallError::DownloadPlanTooLarge);
    }
    let total = tasks.len();
    let completed = AtomicUsize::new(0);

    progress_event(
        progress,
        stage,
        0,
        total,
        &format!("Preparing {total} files"),
    );

    tasks.par_iter().try_for_each(|task| {
        download_file(client, task)?;
        let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
        progress_event(
            progress,
            stage,
            current,
            total,
            &format!("{current} / {total}"),
        );
        Ok::<(), MinecraftInstallError>(())
    })?;

    Ok(())
}

fn download_file(client: &Client, task: &DownloadTask) -> Result<(), MinecraftInstallError> {
    let url = validate_https_url(&task.url)?;
    let expected_sha1 = validate_sha1(&task.sha1)?;
    if task.size > MAX_DOWNLOAD_SIZE {
        return Err(MinecraftInstallError::ResponseTooLarge);
    }

    if task.target.is_file()
        && fs::metadata(&task.target)?.len() == task.size
        && file_sha1(&task.target)? == expected_sha1
    {
        return Ok(());
    }

    if let Some(parent) = task.target.parent() {
        fs::create_dir_all(parent)?;
    }

    let part_path = part_path_for(&task.target);
    let mut response = client.get(url).send()?.error_for_status()?;
    if let Some(actual) = response.content_length() {
        if actual != task.size || actual > MAX_DOWNLOAD_SIZE {
            return Err(MinecraftInstallError::SizeMismatch {
                path: task.target.clone(),
                expected: task.size,
                actual,
            });
        }
    }
    let mut file = File::create(&part_path)?;
    let mut hasher = Sha1::new();
    let mut downloaded_size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = response.read(&mut buffer)?;

        if count == 0 {
            break;
        }

        downloaded_size += count as u64;
        if downloaded_size > task.size || downloaded_size > MAX_DOWNLOAD_SIZE {
            drop(file);
            let _ = fs::remove_file(&part_path);
            return Err(MinecraftInstallError::SizeMismatch {
                path: task.target.clone(),
                expected: task.size,
                actual: downloaded_size,
            });
        }
        file.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }

    file.flush()?;
    drop(file);

    let actual_sha1 = format!("{:x}", hasher.finalize());

    if downloaded_size != task.size {
        let _ = fs::remove_file(&part_path);
        return Err(MinecraftInstallError::SizeMismatch {
            path: task.target.clone(),
            expected: task.size,
            actual: downloaded_size,
        });
    }

    if actual_sha1 != expected_sha1 {
        let _ = fs::remove_file(&part_path);
        return Err(MinecraftInstallError::HashMismatch {
            path: task.target.clone(),
            expected: expected_sha1,
            actual: actual_sha1,
        });
    }

    replace_file_atomic(&part_path, &task.target)?;
    Ok(())
}

fn fetch_json<T: DeserializeOwned>(client: &Client, url: &str) -> Result<T, MinecraftInstallError> {
    Ok(serde_json::from_slice(&fetch_bytes(client, url)?)?)
}

fn fetch_bytes(client: &Client, url: &str) -> Result<Vec<u8>, MinecraftInstallError> {
    let response = client
        .get(validate_https_url(url)?)
        .send()?
        .error_for_status()?;
    read_bounded(response, MAX_METADATA_RESPONSE_SIZE)
}

fn read_bounded(mut response: Response, maximum: u64) -> Result<Vec<u8>, MinecraftInstallError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(MinecraftInstallError::ResponseTooLarge);
    }

    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(MinecraftInstallError::ResponseTooLarge);
    }
    Ok(bytes)
}

fn minecraft_client() -> Result<Client, MinecraftInstallError> {
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if !minecraft_download_url_allowed(attempt.url()) {
            attempt.error("download redirect must use an approved Minecraft HTTPS host")
        } else {
            attempt.follow()
        }
    });
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(5 * 60))
        .redirect(redirect_policy)
        .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn validate_https_url(value: &str) -> Result<Url, MinecraftInstallError> {
    let url = Url::parse(value)
        .map_err(|_| MinecraftInstallError::InvalidDownloadUrl(value.to_owned()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || !minecraft_download_url_allowed(&url)
    {
        return Err(MinecraftInstallError::InvalidDownloadUrl(value.to_owned()));
    }
    Ok(url)
}

fn minecraft_download_url_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|port| port == 443)
        && url
            .host_str()
            .is_some_and(|host| MINECRAFT_DOWNLOAD_HOSTS.contains(&host))
}

fn validate_sha1(value: &str) -> Result<String, MinecraftInstallError> {
    let value = value.trim();
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MinecraftInstallError::InvalidChecksum(value.to_owned()));
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_version_manifest(manifest: &mut VersionManifest) -> Result<(), MinecraftInstallError> {
    if manifest.versions.len() > MAX_VERSION_CATALOG_ENTRIES {
        return Err(MinecraftInstallError::DownloadPlanTooLarge);
    }
    validate_version_identifier(&manifest.latest.release)?;
    validate_version_identifier(&manifest.latest.snapshot)?;
    for version in &mut manifest.versions {
        validate_version_identifier(&version.id)?;
        validate_https_url(&version.url)?;
        version.sha1 = validate_sha1(&version.sha1)?;
        if version.version_type.is_empty()
            || version.version_type.len() > 32
            || version.version_type.chars().any(char::is_control)
            || version.release_time.len() > 64
            || version.release_time.chars().any(char::is_control)
        {
            return Err(MinecraftInstallError::InvalidMetadataPath(
                version.id.clone(),
            ));
        }
    }
    Ok(())
}

fn verify_bytes_sha1(
    bytes: &[u8],
    expected: &str,
    path: PathBuf,
) -> Result<(), MinecraftInstallError> {
    let actual = format!("{:x}", Sha1::digest(bytes));

    if actual != expected {
        return Err(MinecraftInstallError::HashMismatch {
            path,
            expected: expected.to_owned(),
            actual,
        });
    }

    Ok(())
}

fn file_sha1(path: &Path) -> Result<String, MinecraftInstallError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
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

fn safe_metadata_join(root: &Path, relative: &str) -> Result<PathBuf, MinecraftInstallError> {
    let path = Path::new(relative);

    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(MinecraftInstallError::InvalidMetadataPath(
            relative.to_owned(),
        ));
    }

    Ok(root.join(path))
}

pub(crate) fn validate_instance_id(instance_id: &str) -> Result<(), MinecraftInstallError> {
    if instance_id.is_empty() {
        return Err(MinecraftInstallError::EmptyInstanceId);
    }

    for character in instance_id.chars() {
        if !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_')) {
            return Err(MinecraftInstallError::InvalidInstanceIdCharacter(character));
        }
    }

    let length = instance_id.chars().count();
    if length > MAX_INSTANCE_ID_LENGTH {
        return Err(MinecraftInstallError::InstanceIdTooLong(length));
    }

    Ok(())
}

pub(crate) fn validate_instance_name(name: &str) -> Result<&str, MinecraftInstallError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(MinecraftInstallError::EmptyInstanceName);
    }

    let length = name.chars().count();
    if length > MAX_INSTANCE_NAME_LENGTH {
        return Err(MinecraftInstallError::InstanceNameTooLong(length));
    }
    if name.chars().any(char::is_control) {
        return Err(MinecraftInstallError::InvalidMetadataPath(
            "instance name contains a control character".to_owned(),
        ));
    }

    Ok(name)
}

pub(crate) fn validate_metadata_identifier(value: &str) -> Result<(), MinecraftInstallError> {
    if value.is_empty()
        || value.chars().count() > MAX_METADATA_IDENTIFIER_LENGTH
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(MinecraftInstallError::InvalidMetadataPath(value.to_owned()));
    }
    Ok(())
}

fn validate_version_identifier(value: &str) -> Result<(), MinecraftInstallError> {
    if value.is_empty()
        || value.chars().count() > MAX_METADATA_IDENTIFIER_LENGTH
        || value.trim() != value
        || matches!(value, "." | "..")
        || value.ends_with('.')
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+' | b' ')
        })
    {
        return Err(MinecraftInstallError::InvalidMetadataPath(value.to_owned()));
    }
    Ok(())
}

fn validate_managed_java(
    paths: &MinecraftPaths,
    java_path: &Path,
) -> Result<PathBuf, MinecraftInstallError> {
    if java_path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        || !java_path.file_name().is_some_and(|name| {
            name.eq_ignore_ascii_case(crate::minecraft::model::java_executable_name())
        })
    {
        return Err(MinecraftInstallError::UnmanagedJavaRuntime(
            java_path.to_owned(),
        ));
    }
    managed_java_path_and_major(paths, java_path).map(|(path, _)| path)
}

pub(crate) fn managed_java_major(
    paths: &MinecraftPaths,
    java_path: &Path,
) -> Result<u32, MinecraftInstallError> {
    managed_java_path_and_major(paths, java_path).map(|(_, major)| major)
}

fn managed_java_path_and_major(
    paths: &MinecraftPaths,
    java_path: &Path,
) -> Result<(PathBuf, u32), MinecraftInstallError> {
    let java = fs::canonicalize(java_path)
        .map_err(|_| MinecraftInstallError::JavaNotFound(java_path.to_owned()))?;
    let runtimes = fs::canonicalize(paths.runtimes())
        .map_err(|_| MinecraftInstallError::UnmanagedJavaRuntime(java.clone()))?;
    let relative = java
        .strip_prefix(&runtimes)
        .map_err(|_| MinecraftInstallError::UnmanagedJavaRuntime(java.clone()))?;
    let mut components = relative.components();
    let family = components
        .next()
        .and_then(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .and_then(|value| value.strip_prefix("temurin-"));
    let checksum = components.next().and_then(|component| match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    });
    let major = family.and_then(|value| value.parse::<u32>().ok());
    let valid_checksum = checksum.is_some_and(|value| {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let in_bin_directory = java
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"));
    if !valid_checksum || !in_bin_directory || major.is_none_or(|value| value == 0) {
        return Err(MinecraftInstallError::UnmanagedJavaRuntime(java));
    }
    Ok((java, major.unwrap_or_default()))
}

fn validate_asset_hash(hash: &str) -> Result<(), MinecraftInstallError> {
    if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MinecraftInstallError::InvalidAssetHash(hash.to_owned()));
    }

    Ok(())
}

fn part_path_for(target: &Path) -> PathBuf {
    let mut file_name = target
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("download"));
    file_name.push(".part");
    target.with_file_name(file_name)
}

fn progress_event<F>(progress: &F, stage: &str, completed: usize, total: usize, message: &str)
where
    F: Fn(InstallProgress),
{
    progress(InstallProgress {
        stage: stage.to_owned(),
        completed,
        total,
        message: message.to_owned(),
    });
}

impl DownloadTask {
    fn from_info(info: &DownloadInfo, target: PathBuf) -> Self {
        Self {
            url: info.url.clone(),
            sha1: info.sha1.clone(),
            size: info.size,
            target,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_minecraft_paths(test_name: &str) -> MinecraftPaths {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        MinecraftPaths::new(std::env::temp_dir().join(format!(
            "monalauncher-{test_name}-{}-{nonce}",
            std::process::id()
        )))
    }

    fn write_test_instance(paths: &MinecraftPaths, directory_id: &str, manifest_id: &str) {
        let directory = paths.instance(directory_id);
        fs::create_dir_all(directory.join("game/saves/test-world")).unwrap();
        let java = paths
            .runtimes()
            .join("temurin-21")
            .join("a".repeat(64))
            .join("runtime")
            .join("bin")
            .join(crate::minecraft::model::java_executable_name());
        fs::create_dir_all(java.parent().unwrap()).unwrap();
        fs::write(&java, b"test runtime").unwrap();
        let manifest = InstanceManifest {
            id: manifest_id.to_owned(),
            name: "Test Instance".to_owned(),
            version_id: "1.21.8".to_owned(),
            java_path: java.to_string_lossy().into_owned(),
            game_directory: directory.join("game").to_string_lossy().into_owned(),
            demo: false,
            sandboxed: true,
            permissions: Default::default(),
            mod_loader: ModLoader::Vanilla,
        };
        fs::write(
            paths.instance_manifest(directory_id),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn saves_permissions_without_replacing_other_settings_and_uses_them_at_launch() {
        use crate::minecraft::{
            permissions::{save_permissions, InstancePermissions},
            sandbox_policy::policy_for_instance,
        };
        use crate::sandbox::{Backend, FileAccess, Resource};
        let paths = temporary_minecraft_paths("instance-permissions");
        write_test_instance(&paths, "trusted", "trusted");
        write_test_instance(&paths, "other", "other");
        let before = load_instance(&paths, "trusted").unwrap();
        let changed = InstancePermissions {
            game_write: false,
            narrator: false,
        };
        save_permissions(&paths, "trusted", changed).unwrap();
        // Reloading and another metadata edit must retain the persisted permission choices.
        rename_instance(&paths, "trusted", "Renamed").unwrap();
        let loaded = load_instance(&paths, "trusted").unwrap();
        assert_eq!(loaded.permissions, changed);
        assert_eq!(loaded.java_path, before.java_path);
        assert_eq!(loaded.version_id, before.version_id);
        assert_eq!(loaded.mod_loader, before.mod_loader);
        assert_eq!(
            load_instance(&paths, "other").unwrap().permissions,
            InstancePermissions::default()
        );
        let launch = paths.instance("trusted").join("launch-test");
        for dir in [
            paths.assets(),
            paths.libraries(),
            paths.version_directory(&loaded.version_id),
            launch.join("tmp"),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        let policy = policy_for_instance(&paths, &loaded, &launch).unwrap();
        assert!(policy
            .requested_files()
            .contains(&(Resource::Game, FileAccess::ReadOnly)));
        for backend in [Backend::Seatbelt, Backend::AppContainer] {
            assert!(!policy.compile(backend).unwrap().narrator);
        }
        save_permissions(&paths, "trusted", InstancePermissions::default()).unwrap();
        let restored = load_instance(&paths, "trusted").unwrap();
        let policy = policy_for_instance(&paths, &restored, &launch).unwrap();
        assert!(policy.narrator);
        assert!(policy
            .requested_files()
            .contains(&(Resource::Game, FileAccess::ReadWrite)));
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn permission_save_rejects_unsafe_or_legacy_instances_without_changes() {
        use crate::minecraft::permissions::{save_permissions, InstancePermissions};
        let paths = temporary_minecraft_paths("reject-instance-permissions");
        write_test_instance(&paths, "trusted", "trusted");
        let manifest = paths.instance_manifest("trusted");
        let mut instance = load_instance(&paths, "trusted").unwrap();
        instance.sandboxed = false;
        fs::write(&manifest, serde_json::to_vec_pretty(&instance).unwrap()).unwrap();
        let before = fs::read(&manifest).unwrap();
        assert!(save_permissions(
            &paths,
            "trusted",
            InstancePermissions {
                game_write: false,
                narrator: false
            }
        )
        .is_err());
        assert!(save_permissions(&paths, "../trusted", InstancePermissions::default()).is_err());
        assert_eq!(fs::read(&manifest).unwrap(), before);
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn rejects_parent_directory_from_metadata() {
        let result = safe_metadata_join(Path::new("libraries"), "../secret.jar");
        assert!(matches!(
            result,
            Err(MinecraftInstallError::InvalidMetadataPath(_))
        ));
    }

    #[test]
    fn accepts_maven_library_path() {
        let result = safe_metadata_join(
            Path::new("libraries"),
            "org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1.jar",
        );

        assert_eq!(
            result.unwrap(),
            Path::new("libraries/org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1.jar")
        );
    }

    #[test]
    fn validates_sha1_asset_hash() {
        assert!(validate_asset_hash("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(validate_asset_hash("../bad").is_err());
    }

    #[test]
    fn validates_and_trims_instance_names() {
        assert_eq!(validate_instance_name("  My World  ").unwrap(), "My World");
        assert!(matches!(
            validate_instance_name("   "),
            Err(MinecraftInstallError::EmptyInstanceName)
        ));
        assert!(matches!(
            validate_instance_name(&"a".repeat(MAX_INSTANCE_NAME_LENGTH + 1)),
            Err(MinecraftInstallError::InstanceNameTooLong(_))
        ));
    }

    #[test]
    fn validates_instance_id_length_and_download_metadata() {
        assert!(validate_instance_id(&"a".repeat(MAX_INSTANCE_ID_LENGTH)).is_ok());
        assert!(matches!(
            validate_instance_id(&"a".repeat(MAX_INSTANCE_ID_LENGTH + 1)),
            Err(MinecraftInstallError::InstanceIdTooLong(_))
        ));

        assert!(validate_https_url("https://piston-data.mojang.com/file.jar").is_ok());
        assert!(validate_https_url("http://example.com/file.jar").is_err());
        assert!(validate_https_url("https://user@example.com/file.jar").is_err());
        assert!(validate_https_url("https://example.com/file.jar").is_err());
        assert!(validate_https_url("https://piston-data.mojang.com:444/file.jar").is_err());

        let checksum = "A".repeat(40);
        assert_eq!(validate_sha1(&checksum).unwrap(), "a".repeat(40));
        assert!(validate_sha1("invalid").is_err());
        assert!(validate_metadata_identifier("1.21.8").is_ok());
        assert!(validate_metadata_identifier("../escape").is_err());
        assert!(validate_version_identifier("1.14.2 Pre-Release 4").is_ok());
        assert!(validate_version_identifier(" 1.14").is_err());
        assert!(validate_version_identifier("1.14/escape").is_err());
    }

    #[test]
    fn normalizes_the_game_directory_from_the_trusted_instance_id() {
        let paths = temporary_minecraft_paths("normalize-game-directory");
        write_test_instance(&paths, "trusted", "trusted");
        let manifest_path = paths.instance_manifest("trusted");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["gameDirectory"] = serde_json::Value::String("C:\\outside".to_owned());
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();

        let loaded = load_instance(&paths, "trusted").unwrap();

        assert_eq!(
            Path::new(&loaded.game_directory),
            paths.instance_game_directory("trusted")
        );
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn rejects_an_external_java_for_a_sandboxed_instance() {
        let paths = temporary_minecraft_paths("external-java");
        write_test_instance(&paths, "trusted", "trusted");
        let external = paths
            .root()
            .join("external/bin")
            .join(crate::minecraft::model::java_executable_name());
        fs::create_dir_all(external.parent().unwrap()).unwrap();
        fs::write(&external, b"unmanaged").unwrap();
        let manifest_path = paths.instance_manifest("trusted");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["javaPath"] = serde_json::Value::String(external.to_string_lossy().into_owned());
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();

        assert!(matches!(
            load_instance(&paths, "trusted"),
            Err(MinecraftInstallError::UnmanagedJavaRuntime(_))
        ));
        let repairable = load_instance_for_repair(&paths, "trusted").unwrap();
        assert_eq!(repairable.id, "trusted");
        assert_eq!(Path::new(&repairable.java_path), external);
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn derives_the_managed_java_major_from_the_verified_runtime_layout() {
        let paths = temporary_minecraft_paths("managed-java-major");
        write_test_instance(&paths, "trusted", "trusted");
        let instance = load_instance(&paths, "trusted").unwrap();

        assert_eq!(
            managed_java_major(&paths, Path::new(&instance.java_path)).unwrap(),
            21
        );
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn rejects_oversized_download_plans_before_network_access() {
        let client = minecraft_client().unwrap();
        let task = DownloadTask {
            url: "https://piston-data.mojang.com/file".to_owned(),
            sha1: "a".repeat(40),
            size: MAX_DOWNLOAD_PLAN_SIZE,
            target: PathBuf::from("unused"),
        };
        let result = download_tasks(&client, vec![task.clone(), task], "test", &|_| {});

        assert!(matches!(
            result,
            Err(MinecraftInstallError::DownloadPlanTooLarge)
        ));
    }

    #[test]
    fn deletes_only_the_requested_instance_directory() {
        let paths = temporary_minecraft_paths("delete-instance");
        write_test_instance(&paths, "delete-me", "delete-me");
        write_test_instance(&paths, "keep-me", "keep-me");

        delete_instance(&paths, "delete-me").unwrap();

        assert!(!paths.instance("delete-me").exists());
        assert!(paths.instance("keep-me").exists());
        fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn refuses_to_delete_when_manifest_id_does_not_match_directory() {
        let paths = temporary_minecraft_paths("delete-mismatch");
        write_test_instance(&paths, "requested", "different");

        let result = delete_instance(&paths, "requested");

        assert!(matches!(
            result,
            Err(MinecraftInstallError::InstanceIdMismatch { .. })
        ));
        assert!(paths.instance("requested").exists());
        fs::remove_dir_all(paths.root()).unwrap();
    }
}
