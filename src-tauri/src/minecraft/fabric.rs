use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::model::{Arguments, InstallProgress};
use super::paths::MinecraftPaths;

const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const FABRIC_MAVEN_BASE_URL: &str = "https://maven.fabricmc.net/";
const MAX_META_RESPONSE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_CHECKSUM_RESPONSE_SIZE: u64 = 1024;
const MAX_LIBRARY_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum FabricError {
    Http(reqwest::Error),
    InvalidBaseUrl,
    ResponseTooLarge,
    Json(serde_json::Error),
    Io(std::io::Error),
    LoaderVersionMissing {
        minecraft_version: String,
        loader_version: String,
    },
    InheritanceMismatch {
        expected: String,
        actual: String,
    },
    InvalidMavenCoordinate(String),
    UnsupportedRepository(String),
    InvalidChecksum(String),
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
}

impl fmt::Display for FabricError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "Fabric Metaへの接続に失敗しました: {error}"),
            Self::InvalidBaseUrl => write!(formatter, "Fabric MetaのURL設定が正しくありません"),
            Self::ResponseTooLarge => write!(formatter, "Fabric Metaの応答が大きすぎます"),
            Self::Json(error) => write!(
                formatter,
                "Fabric Metaの応答を解釈できませんでした: {error}"
            ),
            Self::Io(error) => write!(formatter, "Fabricファイルの操作に失敗しました: {error}"),
            Self::LoaderVersionMissing {
                minecraft_version,
                loader_version,
            } => write!(
                formatter,
                "Fabric Loader {loader_version}はMinecraft {minecraft_version}に対応していません"
            ),
            Self::InheritanceMismatch { expected, actual } => write!(
                formatter,
                "Fabric profileのMinecraftバージョンが一致しません: expected {expected}, got {actual}"
            ),
            Self::InvalidMavenCoordinate(coordinate) => {
                write!(formatter, "Fabricライブラリ名が正しくありません: {coordinate}")
            }
            Self::UnsupportedRepository(repository) => write!(
                formatter,
                "Fabric profileが許可されていない配布元を指定しました: {repository}"
            ),
            Self::InvalidChecksum(checksum) => {
                write!(formatter, "FabricライブラリのSHA-1が正しくありません: {checksum}")
            }
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "FabricライブラリのSHA-1が一致しません: {}: expected {expected}, got {actual}",
                path.display()
            ),
            Self::SizeMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "Fabricライブラリのサイズが一致しません: {}: expected {expected}, got {actual}",
                path.display()
            ),
        }
    }
}

impl Error for FabricError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for FabricError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for FabricError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for FabricError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderComponent,
}

#[derive(Debug, Deserialize)]
struct LoaderComponent {
    version: String,
    stable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricProfile {
    pub id: String,
    pub inherits_from: String,
    pub main_class: String,
    #[serde(default)]
    pub arguments: Arguments,
    pub libraries: Vec<FabricLibrary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FabricLibrary {
    pub name: String,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

pub fn list_loader_versions(
    minecraft_version: &str,
) -> Result<Vec<FabricLoaderVersion>, FabricError> {
    let client = fabric_client()?;
    let url = loader_versions_url(minecraft_version)?;
    let bytes = fetch_bounded(&client, url, MAX_META_RESPONSE_SIZE)?;
    let entries: Vec<LoaderEntry> = serde_json::from_slice(&bytes)?;

    Ok(entries
        .into_iter()
        .map(|entry| FabricLoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect())
}

pub fn install_fabric<F>(
    paths: &MinecraftPaths,
    instance_id: &str,
    minecraft_version: &str,
    loader_version: &str,
    progress: &F,
) -> Result<(), FabricError>
where
    F: Fn(InstallProgress),
{
    let available = list_loader_versions(minecraft_version)?;
    if !available
        .iter()
        .any(|candidate| candidate.version == loader_version)
    {
        return Err(FabricError::LoaderVersionMissing {
            minecraft_version: minecraft_version.to_owned(),
            loader_version: loader_version.to_owned(),
        });
    }

    let client = fabric_client()?;
    progress(InstallProgress {
        stage: "loader".to_owned(),
        completed: 0,
        total: 1,
        message: format!("Fabric Loader {loader_version}のprofileを取得しています"),
    });
    let profile_url = loader_profile_url(minecraft_version, loader_version)?;
    let profile_bytes = fetch_bounded(&client, profile_url, MAX_META_RESPONSE_SIZE)?;
    let profile: FabricProfile = serde_json::from_slice(&profile_bytes)?;
    if profile.inherits_from != minecraft_version {
        return Err(FabricError::InheritanceMismatch {
            expected: minecraft_version.to_owned(),
            actual: profile.inherits_from,
        });
    }

    let total = profile.libraries.len();
    for (index, library) in profile.libraries.iter().enumerate() {
        progress(InstallProgress {
            stage: "loader".to_owned(),
            completed: index,
            total,
            message: format!("Fabricライブラリを確認しています: {}", library.name),
        });
        install_library(&client, paths, library)?;
    }

    let instance_directory = paths.instance(instance_id);
    fs::create_dir_all(&instance_directory)?;
    fs::create_dir_all(paths.instance_game_directory(instance_id).join("mods"))?;
    fs::write(paths.instance_fabric_profile(instance_id), profile_bytes)?;
    progress(InstallProgress {
        stage: "loader".to_owned(),
        completed: total,
        total,
        message: format!("Fabric Loader {loader_version}を準備しました"),
    });
    Ok(())
}

pub fn load_fabric_profile(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<FabricProfile, FabricError> {
    Ok(serde_json::from_slice(&fs::read(
        paths.instance_fabric_profile(instance_id),
    )?)?)
}

pub fn maven_artifact_path(coordinate: &str) -> Result<PathBuf, FabricError> {
    let (coordinates, extension) = coordinate
        .split_once('@')
        .map_or((coordinate, "jar"), |(coordinates, extension)| {
            (coordinates, extension)
        });
    if !valid_maven_component(extension) {
        return Err(FabricError::InvalidMavenCoordinate(coordinate.to_owned()));
    }

    let parts = coordinates.split(':').collect::<Vec<_>>();
    if !(parts.len() == 3 || parts.len() == 4)
        || parts.iter().any(|part| !valid_maven_component(part))
        || parts[0]
            .split('.')
            .any(|segment| !valid_maven_component(segment))
    {
        return Err(FabricError::InvalidMavenCoordinate(coordinate.to_owned()));
    }

    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let file_name = format!("{artifact}-{version}{classifier}.{extension}");
    let mut path = PathBuf::new();
    for segment in group.split('.') {
        path.push(segment);
    }
    path.push(artifact);
    path.push(version);
    path.push(file_name);
    Ok(path)
}

fn valid_maven_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
        && value != "."
        && value != ".."
}

fn fabric_client() -> Result<Client, FabricError> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn loader_versions_url(minecraft_version: &str) -> Result<Url, FabricError> {
    let mut url = Url::parse(FABRIC_META_BASE_URL).map_err(|_| FabricError::InvalidBaseUrl)?;
    url.path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?
        .push(minecraft_version);
    Ok(url)
}

fn loader_profile_url(minecraft_version: &str, loader_version: &str) -> Result<Url, FabricError> {
    let mut url = loader_versions_url(minecraft_version)?;
    url.path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?
        .push(loader_version)
        .push("profile")
        .push("json");
    Ok(url)
}

fn install_library(
    client: &Client,
    paths: &MinecraftPaths,
    library: &FabricLibrary,
) -> Result<(), FabricError> {
    let repository = library.url.as_deref().unwrap_or(FABRIC_MAVEN_BASE_URL);
    if repository.trim_end_matches('/') != FABRIC_MAVEN_BASE_URL.trim_end_matches('/') {
        return Err(FabricError::UnsupportedRepository(repository.to_owned()));
    }
    let relative = maven_artifact_path(&library.name)?;
    let url = maven_artifact_url(&relative)?;
    let expected_sha1 = match library.sha1.as_deref() {
        Some(checksum) => validate_checksum(checksum)?,
        None => fetch_checksum(client, &url)?,
    };
    let target = paths.libraries().join(&relative);

    if target.is_file()
        && file_sha1(&target)? == expected_sha1
        && library
            .size
            .is_none_or(|expected| fs::metadata(&target).is_ok_and(|item| item.len() == expected))
    {
        return Ok(());
    }
    download_library(client, url, &target, &expected_sha1, library.size)
}

fn maven_artifact_url(relative: &Path) -> Result<Url, FabricError> {
    let mut url = Url::parse(FABRIC_MAVEN_BASE_URL).map_err(|_| FabricError::InvalidBaseUrl)?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?;
    segments.pop_if_empty();
    for component in relative.components() {
        let segment = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| FabricError::InvalidMavenCoordinate(relative.display().to_string()))?;
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

fn fetch_checksum(client: &Client, artifact_url: &Url) -> Result<String, FabricError> {
    let checksum_url =
        Url::parse(&format!("{artifact_url}.sha1")).map_err(|_| FabricError::InvalidBaseUrl)?;
    let bytes = fetch_bounded(client, checksum_url, MAX_CHECKSUM_RESPONSE_SIZE)?;
    let text = String::from_utf8_lossy(&bytes);
    let checksum = text.split_whitespace().next().unwrap_or_default();
    validate_checksum(checksum)
}

fn validate_checksum(checksum: &str) -> Result<String, FabricError> {
    let checksum = checksum.trim();
    if checksum.len() != 40 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FabricError::InvalidChecksum(checksum.to_owned()));
    }
    Ok(checksum.to_ascii_lowercase())
}

fn fetch_bounded(client: &Client, url: Url, maximum: u64) -> Result<Vec<u8>, FabricError> {
    let response = client.get(url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(FabricError::ResponseTooLarge);
    }
    let bytes = response.bytes()?;
    if bytes.len() as u64 > maximum {
        return Err(FabricError::ResponseTooLarge);
    }
    Ok(bytes.to_vec())
}

fn download_library(
    client: &Client,
    url: Url,
    target: &Path,
    expected_sha1: &str,
    expected_size: Option<u64>,
) -> Result<(), FabricError> {
    if expected_size.is_some_and(|size| size > MAX_LIBRARY_SIZE) {
        return Err(FabricError::ResponseTooLarge);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_path_for(target);
    let mut response = client.get(url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_LIBRARY_SIZE)
    {
        return Err(FabricError::ResponseTooLarge);
    }
    let mut file = File::create(&part)?;
    let mut hasher = Sha1::new();
    let mut actual_size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        actual_size += count as u64;
        if actual_size > MAX_LIBRARY_SIZE {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err(FabricError::ResponseTooLarge);
        }
    }
    file.flush()?;
    drop(file);

    let actual_sha1 = format!("{:x}", hasher.finalize());
    if actual_sha1 != expected_sha1 {
        let _ = fs::remove_file(&part);
        return Err(FabricError::HashMismatch {
            path: target.to_owned(),
            expected: expected_sha1.to_owned(),
            actual: actual_sha1,
        });
    }
    if let Some(expected) = expected_size {
        if actual_size != expected {
            let _ = fs::remove_file(&part);
            return Err(FabricError::SizeMismatch {
                path: target.to_owned(),
                expected,
                actual: actual_size,
            });
        }
    }
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(part, target)?;
    Ok(())
}

fn part_path_for(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "fabric-library".into());
    name.push(".part");
    target.with_file_name(name)
}

fn file_sha1(path: &Path) -> Result<String, FabricError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_minecraft_version_as_one_url_segment() {
        let url = loader_versions_url("../unsafe value").unwrap();

        assert_eq!(
            url.as_str(),
            "https://meta.fabricmc.net/v2/versions/loader/..%2Funsafe%20value"
        );
    }

    #[test]
    fn parses_loader_catalog_entries() {
        let entries: Vec<LoaderEntry> = serde_json::from_str(
            r#"[{
                "loader": { "version": "0.19.3", "stable": true },
                "intermediary": { "version": "1.21.8", "stable": true }
            }]"#,
        )
        .unwrap();

        assert_eq!(entries[0].loader.version, "0.19.3");
        assert!(entries[0].loader.stable);
    }

    #[test]
    fn builds_safe_maven_artifact_paths() {
        assert_eq!(
            maven_artifact_path("net.fabricmc:fabric-loader:0.19.3").unwrap(),
            Path::new("net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar")
        );
        assert_eq!(
            maven_artifact_path("example:demo:1.0:client@zip").unwrap(),
            Path::new("example/demo/1.0/demo-1.0-client.zip")
        );
    }

    #[test]
    fn rejects_unsafe_maven_coordinates() {
        for coordinate in [
            "../evil:demo:1.0",
            "example:../demo:1.0",
            "example:demo:../../evil",
            "example:demo",
            "example:demo:1.0@../jar",
        ] {
            assert!(maven_artifact_path(coordinate).is_err(), "{coordinate}");
        }
    }

    #[test]
    fn builds_official_maven_urls() {
        let path = maven_artifact_path("net.fabricmc:fabric-loader:0.19.3").unwrap();
        let url = maven_artifact_url(&path).unwrap();

        assert_eq!(
            url.as_str(),
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar"
        );
    }

    #[test]
    fn parses_current_fabric_profile_shape() {
        let profile: FabricProfile = serde_json::from_str(
            r#"{
                "id": "fabric-loader-0.19.3-1.21.8",
                "inheritsFrom": "1.21.8",
                "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments": { "game": [], "jvm": ["-DFabricMcEmu=true"] },
                "libraries": [{
                    "name": "net.fabricmc:fabric-loader:0.19.3",
                    "url": "https://maven.fabricmc.net/"
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(profile.inherits_from, "1.21.8");
        assert_eq!(profile.libraries.len(), 1);
    }
}
