use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::file_io::{read_bounded_file, replace_file_atomic, write_atomic};
use super::model::{Argument, ArgumentValue, Arguments, InstallProgress};
use super::paths::MinecraftPaths;

const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const FABRIC_MAVEN_BASE_URL: &str = "https://maven.fabricmc.net/";
const FABRIC_DOWNLOAD_HOSTS: &[&str] = &["meta.fabricmc.net", "maven.fabricmc.net"];
pub const FABRIC_CLIENT_MAIN_CLASS: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";
const MAX_META_RESPONSE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_CHECKSUM_RESPONSE_SIZE: u64 = 1024;
const MAX_LIBRARY_SIZE: u64 = 64 * 1024 * 1024;
const MAX_LIBRARIES: usize = 256;
const MAX_LIBRARY_PLAN_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PROFILE_ARGUMENTS: usize = 4096;
const MAX_PROFILE_ARGUMENT_LENGTH: usize = 8192;
const MAX_LOADER_VERSIONS: usize = 2048;

#[derive(Debug)]
pub enum FabricError {
    Http(reqwest::Error),
    ServiceStatus(StatusCode),
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
    ProfileIdMismatch {
        expected: String,
        actual: String,
    },
    UnexpectedMainClass(String),
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
    LibraryPlanTooLarge,
    InvalidArgument,
    InvalidVersionIdentifier(String),
    InvalidInstanceIdentifier(String),
}

impl fmt::Display for FabricError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "Fabric Metaへの接続に失敗しました: {error}"),
            Self::ServiceStatus(status) => {
                write!(formatter, "Fabric MetaがHTTP {status}を返しました")
            }
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
            Self::ProfileIdMismatch { expected, actual } => write!(
                formatter,
                "Fabric profile IDが一致しません: expected {expected}, got {actual}"
            ),
            Self::UnexpectedMainClass(main_class) => write!(
                formatter,
                "Fabric profileが想定外のmain classを指定しました: {main_class}"
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
            Self::LibraryPlanTooLarge => write!(
                formatter,
                "Fabric profileのライブラリ数または合計サイズが上限を超えています"
            ),
            Self::InvalidArgument => {
                write!(formatter, "Fabric profileの起動引数が大きすぎるか不正です")
            }
            Self::InvalidVersionIdentifier(value) => {
                write!(formatter, "Fabricのバージョン識別子が不正です: {value}")
            }
            Self::InvalidInstanceIdentifier(value) => {
                write!(formatter, "Fabricのインスタンス識別子が不正です: {value}")
            }
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
    let response = client.get(url).send()?;
    let status = response.status();
    let bytes = read_bounded(response, MAX_META_RESPONSE_SIZE)?;
    let entries = parse_loader_response(status, &bytes)?;
    if entries.len() > MAX_LOADER_VERSIONS {
        return Err(FabricError::ResponseTooLarge);
    }

    for entry in &entries {
        validate_version_identifier(&entry.loader.version)?;
    }

    Ok(entries
        .into_iter()
        .map(|entry| FabricLoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect())
}

fn parse_loader_response(
    status: StatusCode,
    bytes: &[u8],
) -> Result<Vec<LoaderEntry>, FabricError> {
    if status.is_success() {
        return Ok(serde_json::from_slice(bytes)?);
    }
    if status == StatusCode::BAD_REQUEST {
        if let Ok(entries) = serde_json::from_slice::<Vec<LoaderEntry>>(bytes) {
            if entries.is_empty() {
                return Ok(entries);
            }
        }
    }
    Err(FabricError::ServiceStatus(status))
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
    validate_instance_identifier(instance_id)?;
    validate_version_identifier(minecraft_version)?;
    validate_version_identifier(loader_version)?;
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
    validate_profile(&profile, minecraft_version, loader_version)?;

    let total = profile.libraries.len();
    let mut installed_size = 0_u64;
    for (index, library) in profile.libraries.iter().enumerate() {
        progress(InstallProgress {
            stage: "loader".to_owned(),
            completed: index,
            total,
            message: format!("Fabricライブラリを確認しています: {}", library.name),
        });
        installed_size = installed_size
            .checked_add(install_library(&client, paths, library)?)
            .filter(|size| *size <= MAX_LIBRARY_PLAN_SIZE)
            .ok_or(FabricError::LibraryPlanTooLarge)?;
    }

    let instance_directory = paths.instance(instance_id);
    fs::create_dir_all(&instance_directory)?;
    fs::create_dir_all(paths.instance_game_directory(instance_id).join("mods"))?;
    write_atomic(&paths.instance_fabric_profile(instance_id), &profile_bytes)?;
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
    validate_instance_identifier(instance_id)?;
    Ok(serde_json::from_slice(&read_bounded_file(
        &paths.instance_fabric_profile(instance_id),
        MAX_META_RESPONSE_SIZE,
    )?)?)
}

pub fn validate_profile(
    profile: &FabricProfile,
    minecraft_version: &str,
    loader_version: &str,
) -> Result<(), FabricError> {
    if profile.inherits_from != minecraft_version {
        return Err(FabricError::InheritanceMismatch {
            expected: minecraft_version.to_owned(),
            actual: profile.inherits_from.clone(),
        });
    }
    let expected_id = format!("fabric-loader-{loader_version}-{minecraft_version}");
    if profile.id != expected_id {
        return Err(FabricError::ProfileIdMismatch {
            expected: expected_id,
            actual: profile.id.clone(),
        });
    }
    if profile.main_class != FABRIC_CLIENT_MAIN_CLASS {
        return Err(FabricError::UnexpectedMainClass(profile.main_class.clone()));
    }
    if profile.libraries.len() > MAX_LIBRARIES {
        return Err(FabricError::LibraryPlanTooLarge);
    }
    let mut planned_size = 0_u64;
    for library in &profile.libraries {
        maven_artifact_path(&library.name)?;
        let repository = library.url.as_deref().unwrap_or(FABRIC_MAVEN_BASE_URL);
        if repository.trim_end_matches('/') != FABRIC_MAVEN_BASE_URL.trim_end_matches('/') {
            return Err(FabricError::UnsupportedRepository(repository.to_owned()));
        }
        if let Some(checksum) = &library.sha1 {
            validate_checksum(checksum)?;
        }
        if let Some(size) = library.size {
            if size > MAX_LIBRARY_SIZE {
                return Err(FabricError::LibraryPlanTooLarge);
            }
            planned_size = planned_size
                .checked_add(size)
                .filter(|total| *total <= MAX_LIBRARY_PLAN_SIZE)
                .ok_or(FabricError::LibraryPlanTooLarge)?;
        }
    }
    validate_arguments(&profile.arguments)?;
    Ok(())
}

fn validate_arguments(arguments: &Arguments) -> Result<(), FabricError> {
    let mut count = 0_usize;
    for argument in arguments.game.iter().chain(&arguments.jvm) {
        let values: Vec<&str> = match argument {
            Argument::Plain(value) => vec![value],
            Argument::Conditional { value, .. } => match value {
                ArgumentValue::One(value) => vec![value],
                ArgumentValue::Many(values) => values.iter().map(String::as_str).collect(),
            },
        };
        count = count
            .checked_add(values.len())
            .filter(|total| *total <= MAX_PROFILE_ARGUMENTS)
            .ok_or(FabricError::InvalidArgument)?;
        if values.iter().any(|value| {
            value.len() > MAX_PROFILE_ARGUMENT_LENGTH || value.chars().any(|item| item == '\0')
        }) {
            return Err(FabricError::InvalidArgument);
        }
    }
    Ok(())
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
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if !fabric_download_url_allowed(attempt.url()) {
            attempt.error("Fabric redirect must use an approved HTTPS host")
        } else {
            attempt.follow()
        }
    });
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(redirect_policy)
        .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn fabric_download_url_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|port| port == 443)
        && url
            .host_str()
            .is_some_and(|host| FABRIC_DOWNLOAD_HOSTS.contains(&host))
}

fn loader_versions_url(minecraft_version: &str) -> Result<Url, FabricError> {
    validate_version_identifier(minecraft_version)?;
    let mut url = Url::parse(FABRIC_META_BASE_URL).map_err(|_| FabricError::InvalidBaseUrl)?;
    url.path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?
        .push(minecraft_version);
    Ok(url)
}

fn loader_profile_url(minecraft_version: &str, loader_version: &str) -> Result<Url, FabricError> {
    validate_version_identifier(loader_version)?;
    let mut url = loader_versions_url(minecraft_version)?;
    url.path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?
        .push(loader_version)
        .push("profile")
        .push("json");
    Ok(url)
}

fn validate_version_identifier(value: &str) -> Result<(), FabricError> {
    if value.is_empty()
        || value.len() > 128
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(FabricError::InvalidVersionIdentifier(value.to_owned()));
    }
    Ok(())
}

fn validate_instance_identifier(value: &str) -> Result<(), FabricError> {
    if value.is_empty()
        || value.len() > 41
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(FabricError::InvalidInstanceIdentifier(value.to_owned()));
    }
    Ok(())
}

fn install_library(
    client: &Client,
    paths: &MinecraftPaths,
    library: &FabricLibrary,
) -> Result<u64, FabricError> {
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
        return Ok(fs::metadata(&target)?.len());
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
    read_bounded(response, maximum)
}

fn read_bounded(mut response: Response, maximum: u64) -> Result<Vec<u8>, FabricError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(FabricError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(FabricError::ResponseTooLarge);
    }
    Ok(bytes)
}

fn download_library(
    client: &Client,
    url: Url,
    target: &Path,
    expected_sha1: &str,
    expected_size: Option<u64>,
) -> Result<u64, FabricError> {
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
    replace_file_atomic(&part, target)?;
    Ok(actual_size)
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
    fn accepts_only_safe_minecraft_version_segments() {
        let url = loader_versions_url("1.21.8").unwrap();

        assert_eq!(
            url.as_str(),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.8"
        );
        assert!(loader_versions_url("../unsafe value").is_err());
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
    fn treats_fabric_bad_request_with_empty_catalog_as_unsupported() {
        let entries = parse_loader_response(StatusCode::BAD_REQUEST, b"[]").unwrap();

        assert!(entries.is_empty());
        assert!(matches!(
            parse_loader_response(StatusCode::INTERNAL_SERVER_ERROR, b"[]"),
            Err(FabricError::ServiceStatus(
                StatusCode::INTERNAL_SERVER_ERROR
            ))
        ));
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
        assert!(validate_profile(&profile, "1.21.8", "0.19.3").is_ok());
        assert!(matches!(
            validate_profile(&profile, "1.21.7", "0.19.3"),
            Err(FabricError::InheritanceMismatch { .. })
        ));
    }
}
