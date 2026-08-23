use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};

use super::model::{
    rules_allow, AssetIndex, DownloadInfo, InstallProgress, InstanceManifest, ModLoader,
    VersionManifest, VersionMetadata,
};
use super::paths::MinecraftPaths;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_OBJECT_BASE_URL: &str = "https://resources.download.minecraft.net";
const MAX_INSTANCE_NAME_LENGTH: usize = 80;

#[derive(Debug)]
pub enum MinecraftInstallError {
    EmptyInstanceId,
    EmptyInstanceName,
    InvalidInstanceIdCharacter(char),
    InstanceNameTooLong(usize),
    InstanceMissing(String),
    InstanceIdMismatch {
        expected: String,
        actual: String,
    },
    InvalidMetadataPath(String),
    InvalidAssetHash(String),
    JavaNotFound(PathBuf),
    LatestReleaseMissing(String),
    VersionMissing(String),
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
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
            Self::InstanceNameTooLong(length) => write!(
                formatter,
                "instance name must not exceed {MAX_INSTANCE_NAME_LENGTH} characters (got {length})"
            ),
            Self::InstanceMissing(instance_id) => {
                write!(formatter, "Minecraft instance was not found: {instance_id}")
            }
            Self::InstanceIdMismatch { expected, actual } => write!(
                formatter,
                "instance manifest ID mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidMetadataPath(path) => {
                write!(formatter, "metadata contains an unsafe path: {path}")
            }
            Self::InvalidAssetHash(hash) => {
                write!(formatter, "metadata contains an invalid asset hash: {hash}")
            }
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
            Self::Io(error) => write!(formatter, "file system error: {error}"),
            Self::Http(error) => write!(formatter, "download error: {error}"),
            Self::Json(error) => write!(formatter, "metadata error: {error}"),
        }
    }
}

impl Error for MinecraftInstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
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

#[derive(Debug, Clone)]
struct DownloadTask {
    url: String,
    sha1: String,
    size: u64,
    target: PathBuf,
}

#[derive(Clone, Copy)]
struct InstanceInstallOptions<'a> {
    requested_version: Option<&'a str>,
    sandboxed: bool,
    demo: bool,
}

pub fn install_latest_demo_instance<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    instance_name: &str,
    java_path: &Path,
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
            sandboxed: false,
            demo: true,
        },
        progress,
    )
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
    install_instance_from_manifest(
        paths,
        instance_id,
        instance_name,
        java_path,
        InstanceInstallOptions {
            requested_version: Some(version_id),
            sandboxed: true,
            demo,
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
            sandboxed: true,
            demo,
        },
        progress,
    )
}

pub fn list_available_versions() -> Result<VersionManifest, MinecraftInstallError> {
    let client = Client::builder().user_agent("MonaLauncher/0.1.0").build()?;
    fetch_json(&client, VERSION_MANIFEST_URL)
}

pub fn version_java_major(version_id: &str) -> Result<u32, MinecraftInstallError> {
    let client = Client::builder().user_agent("MonaLauncher/0.1.0").build()?;
    let manifest: VersionManifest = fetch_json(&client, VERSION_MANIFEST_URL)?;
    let selected = manifest
        .versions
        .into_iter()
        .find(|version| version.id == version_id)
        .ok_or_else(|| MinecraftInstallError::VersionMissing(version_id.to_owned()))?;
    let version: VersionMetadata = fetch_json(&client, &selected.url)?;
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

    if !java_path.is_file() {
        return Err(MinecraftInstallError::JavaNotFound(java_path.to_owned()));
    }

    create_base_directories(paths)?;

    let client = Client::builder().user_agent("MonaLauncher/0.1.0").build()?;

    progress_event(&progress, "metadata", 0, 1, "Fetching version manifest");
    let manifest: VersionManifest = fetch_json(&client, VERSION_MANIFEST_URL)?;
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

    let version_directory = paths.version_directory(&version.id);
    fs::create_dir_all(&version_directory)?;
    fs::write(paths.version_json(&version.id), &version_bytes)?;

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

        if let Some(native) = library.windows_native() {
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
    fs::write(
        paths
            .asset_indexes()
            .join(format!("{}.json", version.asset_index.id)),
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

    let instance = InstanceManifest {
        id: instance_id.to_owned(),
        name: instance_name.trim().to_owned(),
        version_id: version.id,
        java_path: java_path.to_string_lossy().into_owned(),
        game_directory: game_directory.to_string_lossy().into_owned(),
        demo: options.demo,
        sandboxed: options.sandboxed,
        mod_loader: ModLoader::Vanilla,
    };

    let instance_directory = paths.instance(instance_id);
    fs::create_dir_all(&instance_directory)?;
    fs::write(
        paths.instance_manifest(instance_id),
        serde_json::to_vec_pretty(&instance)?,
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
        let manifest_path = entry.path().join("instance.json");

        if !manifest_path.is_file() {
            continue;
        }

        let manifest: InstanceManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
        instances.push(manifest);
    }

    instances.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(instances)
}

pub fn rename_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
    name: &str,
) -> Result<InstanceManifest, MinecraftInstallError> {
    validate_instance_id(instance_id)?;
    let name = validate_instance_name(name)?;
    let manifest_path = paths.instance_manifest(instance_id);
    if !manifest_path.is_file() {
        return Err(MinecraftInstallError::InstanceMissing(
            instance_id.to_owned(),
        ));
    }

    let mut instance: InstanceManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    if instance.id != instance_id {
        return Err(MinecraftInstallError::InstanceIdMismatch {
            expected: instance_id.to_owned(),
            actual: instance.id,
        });
    }

    instance.name = name.to_owned();
    fs::write(manifest_path, serde_json::to_vec_pretty(&instance)?)?;
    Ok(instance)
}

pub fn delete_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<(), MinecraftInstallError> {
    validate_instance_id(instance_id)?;
    let manifest_path = paths.instance_manifest(instance_id);
    if !manifest_path.is_file() {
        return Err(MinecraftInstallError::InstanceMissing(
            instance_id.to_owned(),
        ));
    }

    let instance: InstanceManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if instance.id != instance_id {
        return Err(MinecraftInstallError::InstanceIdMismatch {
            expected: instance_id.to_owned(),
            actual: instance.id,
        });
    }

    fs::remove_dir_all(paths.instance(instance_id))?;
    Ok(())
}

pub fn detect_java_path() -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join("java.exe"));
    }

    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|directory| directory.join("java.exe")));
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok().or(Some(candidate)))
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
    if task.target.is_file()
        && fs::metadata(&task.target)?.len() == task.size
        && file_sha1(&task.target)? == task.sha1
    {
        return Ok(());
    }

    if let Some(parent) = task.target.parent() {
        fs::create_dir_all(parent)?;
    }

    let part_path = part_path_for(&task.target);
    let mut response = client.get(&task.url).send()?.error_for_status()?;
    let mut file = File::create(&part_path)?;
    let mut hasher = Sha1::new();
    let mut downloaded_size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = response.read(&mut buffer)?;

        if count == 0 {
            break;
        }

        file.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        downloaded_size += count as u64;
    }

    file.flush()?;
    drop(file);

    let actual_sha1 = format!("{:x}", hasher.finalize());

    if actual_sha1 != task.sha1 || downloaded_size != task.size {
        return Err(MinecraftInstallError::HashMismatch {
            path: task.target.clone(),
            expected: task.sha1.clone(),
            actual: actual_sha1,
        });
    }

    if task.target.exists() {
        fs::remove_file(&task.target)?;
    }

    fs::rename(part_path, &task.target)?;
    Ok(())
}

fn fetch_json<T: DeserializeOwned>(client: &Client, url: &str) -> Result<T, MinecraftInstallError> {
    Ok(client.get(url).send()?.error_for_status()?.json()?)
}

fn fetch_bytes(client: &Client, url: &str) -> Result<Vec<u8>, MinecraftInstallError> {
    Ok(client
        .get(url)
        .send()?
        .error_for_status()?
        .bytes()?
        .to_vec())
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

fn validate_instance_id(instance_id: &str) -> Result<(), MinecraftInstallError> {
    if instance_id.is_empty() {
        return Err(MinecraftInstallError::EmptyInstanceId);
    }

    for character in instance_id.chars() {
        if !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_')) {
            return Err(MinecraftInstallError::InvalidInstanceIdCharacter(character));
        }
    }

    Ok(())
}

fn validate_instance_name(name: &str) -> Result<&str, MinecraftInstallError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(MinecraftInstallError::EmptyInstanceName);
    }

    let length = name.chars().count();
    if length > MAX_INSTANCE_NAME_LENGTH {
        return Err(MinecraftInstallError::InstanceNameTooLong(length));
    }

    Ok(name)
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
        let manifest = InstanceManifest {
            id: manifest_id.to_owned(),
            name: "Test Instance".to_owned(),
            version_id: "1.21.8".to_owned(),
            java_path: "java.exe".to_owned(),
            game_directory: directory.join("game").to_string_lossy().into_owned(),
            demo: false,
            sandboxed: true,
            mod_loader: ModLoader::Vanilla,
        };
        fs::write(
            paths.instance_manifest(directory_id),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
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
