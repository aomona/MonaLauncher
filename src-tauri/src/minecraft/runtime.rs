use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::file_io::replace_file_atomic;
use super::model::InstallProgress;
use super::paths::MinecraftPaths;

const MAX_RUNTIME_METADATA_SIZE: u64 = 1024 * 1024;
const MAX_RUNTIME_ARCHIVE_SIZE: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_RUNTIME_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const RUNTIME_DOWNLOAD_HOSTS: &[&str] = &[
    "api.adoptium.net",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Debug)]
pub enum RuntimeInstallError {
    AssetMissing(u32),
    InvalidPackageName(String),
    InvalidChecksum(String),
    InvalidDownloadUrl(String),
    ResponseTooLarge,
    ArchiveTooLarge,
    UnsafeArchivePath(String),
    JavaMissing(PathBuf),
    HashMismatch { expected: String, actual: String },
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Zip(zip::result::ZipError),
}

impl fmt::Display for RuntimeInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetMissing(major) => {
                write!(formatter, "Adoptium did not return a Java {major} runtime")
            }
            Self::InvalidPackageName(name) => {
                write!(
                    formatter,
                    "Adoptium returned an unsafe Java package name: {name}"
                )
            }
            Self::InvalidChecksum(checksum) => {
                write!(
                    formatter,
                    "Adoptium returned an invalid SHA-256: {checksum}"
                )
            }
            Self::InvalidDownloadUrl(url) => {
                write!(
                    formatter,
                    "Adoptium returned an unsafe Java download URL: {url}"
                )
            }
            Self::ResponseTooLarge => write!(formatter, "Java runtime response is too large"),
            Self::ArchiveTooLarge => write!(formatter, "Java runtime archive expands too large"),
            Self::UnsafeArchivePath(path) => {
                write!(formatter, "Java archive contains an unsafe path: {path}")
            }
            Self::JavaMissing(path) => write!(
                formatter,
                "downloaded Java runtime has no Java executable under {}",
                path.display()
            ),
            Self::HashMismatch { expected, actual } => write!(
                formatter,
                "Java runtime SHA-256 mismatch: expected {expected}, got {actual}"
            ),
            Self::Io(error) => write!(formatter, "Java runtime file system error: {error}"),
            Self::Http(error) => write!(formatter, "Java runtime download error: {error}"),
            Self::Json(error) => write!(formatter, "Java runtime metadata error: {error}"),
            Self::Zip(error) => write!(formatter, "Java runtime archive error: {error}"),
        }
    }
}

impl Error for RuntimeInstallError {}

impl From<std::io::Error> for RuntimeInstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<reqwest::Error> for RuntimeInstallError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for RuntimeInstallError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<zip::result::ZipError> for RuntimeInstallError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    checksum: String,
    link: String,
    name: String,
}

pub fn install_java_21_runtime<F>(
    paths: &MinecraftPaths,
    progress: F,
) -> Result<PathBuf, RuntimeInstallError>
where
    F: Fn(InstallProgress),
{
    install_java_runtime(paths, 21, progress)
}

pub fn install_java_25_runtime<F>(
    paths: &MinecraftPaths,
    progress: F,
) -> Result<PathBuf, RuntimeInstallError>
where
    F: Fn(InstallProgress),
{
    install_java_runtime(paths, 25, progress)
}

pub fn install_java_8_runtime<F>(
    paths: &MinecraftPaths,
    progress: F,
) -> Result<PathBuf, RuntimeInstallError>
where
    F: Fn(InstallProgress),
{
    install_java_runtime(paths, 8, progress)
}

pub fn install_java_runtime<F>(
    paths: &MinecraftPaths,
    major: u32,
    progress: F,
) -> Result<PathBuf, RuntimeInstallError>
where
    F: Fn(InstallProgress),
{
    progress_event(
        &progress,
        0,
        1,
        &format!("Java {major} metadataを取得しています"),
    );
    let client = runtime_client()?;
    let package = validate_runtime_package(fetch_runtime_package(&client, major)?)?;
    let runtime_family = format!("temurin-{major}");
    let runtime_directory = paths
        .runtimes()
        .join(&runtime_family)
        .join(&package.checksum);

    if let Some(java) = find_java(&runtime_directory)? {
        progress_event(&progress, 1, 1, &format!("Java {major}は準備済みです"));
        return Ok(java);
    }

    fs::create_dir_all(paths.runtimes().join("downloads"))?;
    let archive = paths.runtimes().join("downloads").join(&package.name);
    progress_event(
        &progress,
        0,
        1,
        &format!("Java {major}をダウンロードしています"),
    );
    download_and_verify(&client, &package.link, &package.checksum, &archive)?;

    let temporary = paths
        .runtimes()
        .join(&runtime_family)
        .join(format!("{}.part", package.checksum));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(&temporary)?;
    progress_event(&progress, 0, 1, &format!("Java {major}を展開しています"));
    extract_archive(&archive, &temporary)?;
    let java = find_java(&temporary)?
        .ok_or_else(|| RuntimeInstallError::JavaMissing(temporary.clone()))?;
    let relative_java = java
        .strip_prefix(&temporary)
        .map_err(|_| RuntimeInstallError::JavaMissing(temporary.clone()))?
        .to_owned();
    if runtime_directory.exists() {
        fs::remove_dir_all(&runtime_directory)?;
    }
    fs::rename(&temporary, &runtime_directory)?;
    let java = runtime_directory.join(relative_java);
    progress_event(&progress, 1, 1, &format!("Java {major}の準備ができました"));
    Ok(java)
}

fn fetch_runtime_package(
    client: &Client,
    major: u32,
) -> Result<AdoptiumPackage, RuntimeInstallError> {
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else {
        std::env::consts::OS
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x32",
        other => other,
    };
    for image_type in if cfg!(target_os = "macos") {
        ["jdk", "jre"]
    } else {
        ["jre", "jdk"]
    } {
        let assets_url = format!(
            "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type={image_type}&os={os}&vendor=eclipse"
        );
        let response = client.get(assets_url).send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        let response = response.error_for_status()?;
        let assets: Vec<AdoptiumAsset> =
            serde_json::from_slice(&read_bounded(response, MAX_RUNTIME_METADATA_SIZE)?)?;

        if let Some(asset) = assets.into_iter().next() {
            return Ok(asset.binary.package);
        }
    }

    Err(RuntimeInstallError::AssetMissing(major))
}

fn download_and_verify(
    client: &Client,
    url: &str,
    expected: &str,
    target: &Path,
) -> Result<(), RuntimeInstallError> {
    let url = validate_download_url(url)?;
    let expected = validate_sha256(expected)?;
    if target.is_file() && file_sha256(target)? == expected {
        return Ok(());
    }

    let part = target.with_extension("zip.part");
    let mut response = client.get(url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RUNTIME_ARCHIVE_SIZE)
    {
        return Err(RuntimeInstallError::ResponseTooLarge);
    }
    let mut output = File::create(&part)?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > MAX_RUNTIME_ARCHIVE_SIZE {
            drop(output);
            let _ = fs::remove_file(&part);
            return Err(RuntimeInstallError::ResponseTooLarge);
        }
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    output.flush()?;
    drop(output);

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        let _ = fs::remove_file(&part);
        return Err(RuntimeInstallError::HashMismatch { expected, actual });
    }
    replace_file_atomic(&part, target)?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, RuntimeInstallError> {
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_archive(archive: &Path, destination: &Path) -> Result<(), RuntimeInstallError> {
    if archive.to_string_lossy().ends_with(".tar.gz") {
        return extract_tar_archive(archive, destination);
    }
    let input = File::open(archive)?;
    let mut zip = ZipArchive::new(input)?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(RuntimeInstallError::ArchiveTooLarge);
    }
    let mut extracted_size = 0_u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let expected_size = entry.size();
        extracted_size = extracted_size
            .checked_add(expected_size)
            .filter(|size| *size <= MAX_EXTRACTED_RUNTIME_SIZE)
            .ok_or(RuntimeInstallError::ArchiveTooLarge)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| RuntimeInstallError::UnsafeArchivePath(entry.name().to_owned()))?;
        let target = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(target)?;
        let copied = std::io::copy(&mut entry.by_ref().take(expected_size + 1), &mut output)?;
        if copied != expected_size {
            return Err(RuntimeInstallError::ArchiveTooLarge);
        }
    }
    Ok(())
}

// Extract only regular files/directories; links and device nodes cannot escape staging.
fn extract_tar_archive(archive: &Path, destination: &Path) -> Result<(), RuntimeInstallError> {
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0_u64;
    for (index, entry) in archive.entries()?.enumerate() {
        if index >= MAX_ARCHIVE_ENTRIES {
            return Err(RuntimeInstallError::ArchiveTooLarge);
        }
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let kind = entry.header().entry_type();
        if path.is_absolute()
            || path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
            || !(kind.is_file() || kind.is_dir())
        {
            return Err(RuntimeInstallError::UnsafeArchivePath(
                path.display().to_string(),
            ));
        }
        let size = entry.size();
        total = total
            .checked_add(size)
            .filter(|n| *n <= MAX_EXTRACTED_RUNTIME_SIZE)
            .ok_or(RuntimeInstallError::ArchiveTooLarge)?;
        let target = destination.join(path);
        if kind.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        if std::io::copy(&mut entry.by_ref().take(size + 1), &mut output)? != size {
            return Err(RuntimeInstallError::ArchiveTooLarge);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(entry.header().mode()? & 0o755),
            )?;
        }
    }
    Ok(())
}

fn runtime_client() -> Result<Client, RuntimeInstallError> {
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if !runtime_download_url_allowed(attempt.url()) {
            attempt.error("Java download redirect must use an approved HTTPS host")
        } else {
            attempt.follow()
        }
    });
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10 * 60))
        .redirect(redirect_policy)
        .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn read_bounded(mut response: Response, maximum: u64) -> Result<Vec<u8>, RuntimeInstallError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(RuntimeInstallError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(RuntimeInstallError::ResponseTooLarge);
    }
    Ok(bytes)
}

fn validate_runtime_package(
    mut package: AdoptiumPackage,
) -> Result<AdoptiumPackage, RuntimeInstallError> {
    validate_package_name(&package.name)?;
    validate_download_url(&package.link)?;
    package.checksum = validate_sha256(&package.checksum)?;
    Ok(package)
}

fn validate_package_name(name: &str) -> Result<(), RuntimeInstallError> {
    let path = Path::new(name);
    let mut components = path.components();
    let single_component = matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none();
    if !single_component
        || name.chars().count() > 240
        || name.chars().any(char::is_control)
        || !(name.ends_with(".zip") || name.ends_with(".tar.gz"))
    {
        return Err(RuntimeInstallError::InvalidPackageName(name.to_owned()));
    }
    Ok(())
}

fn validate_download_url(value: &str) -> Result<Url, RuntimeInstallError> {
    let url =
        Url::parse(value).map_err(|_| RuntimeInstallError::InvalidDownloadUrl(value.to_owned()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || !runtime_download_url_allowed(&url)
    {
        return Err(RuntimeInstallError::InvalidDownloadUrl(value.to_owned()));
    }
    Ok(url)
}

fn runtime_download_url_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|port| port == 443)
        && url
            .host_str()
            .is_some_and(|host| RUNTIME_DOWNLOAD_HOSTS.contains(&host))
}

fn validate_sha256(value: &str) -> Result<String, RuntimeInstallError> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RuntimeInstallError::InvalidChecksum(value.to_owned()));
    }
    Ok(value.to_ascii_lowercase())
}

fn find_java(root: &Path) -> Result<Option<PathBuf>, RuntimeInstallError> {
    if !root.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let home = if cfg!(target_os = "macos") {
            path.join("Contents/Home")
        } else {
            path
        };
        let direct = home.join("bin").join(super::model::java_executable_name());
        if direct.is_file() {
            return Ok(Some(direct));
        }
    }
    Ok(None)
}

fn progress_event<F>(progress: &F, completed: usize, total: usize, message: &str)
where
    F: Fn(InstallProgress),
{
    progress(InstallProgress {
        stage: "runtime".to_owned(),
        completed,
        total,
        message: message.to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tar_rejects_links_and_preserves_executable_files() {
        let root = std::env::temp_dir().join(format!("mona-tar-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let archive = root.join("runtime.tar.gz");
        let write_archive = |link: bool| {
            let encoder = flate2::write::GzEncoder::new(
                File::create(&archive).unwrap(),
                flate2::Compression::default(),
            );
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o755);
            if link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_link_name("../../outside").unwrap();
                header.set_cksum();
                builder
                    .append_data(&mut header, "jdk/bin/java", &b""[..])
                    .unwrap();
            } else {
                header.set_size(4);
                header.set_cksum();
                builder
                    .append_data(&mut header, "jdk/bin/java", &b"java"[..])
                    .unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        };
        let dest = root.join("out");
        fs::create_dir(&dest).unwrap();
        write_archive(true);
        assert!(matches!(
            extract_archive(&archive, &dest),
            Err(RuntimeInstallError::UnsafeArchivePath(_))
        ));
        write_archive(false);
        extract_archive(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("jdk/bin/java")).unwrap(), b"java");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dest.join("jdk/bin/java"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_runtime_package_metadata() {
        assert!(validate_package_name("OpenJDK-jre_x64_windows_hotspot.zip").is_ok());
        assert!(validate_package_name("../runtime.zip").is_err());
        assert!(validate_package_name("runtime.tar.gz").is_ok());

        assert!(validate_download_url("https://github.com/adoptium/runtime.zip").is_ok());
        assert!(validate_download_url("http://example.com/runtime.zip").is_err());
        assert!(validate_download_url("https://user@example.com/runtime.zip").is_err());
        assert!(validate_download_url("https://example.com/runtime.zip").is_err());
        assert!(validate_download_url("https://github.com:444/runtime.zip").is_err());

        let checksum = "A".repeat(64);
        assert_eq!(validate_sha256(&checksum).unwrap(), "a".repeat(64));
        assert!(validate_sha256("../invalid").is_err());
    }
}
