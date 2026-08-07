use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::model::InstallProgress;
use super::paths::MinecraftPaths;

#[derive(Debug)]
pub enum RuntimeInstallError {
    AssetMissing(u32),
    UnsafeArchivePath(String),
    JavaMissing(PathBuf),
    HashMismatch { expected: String, actual: String },
    Io(std::io::Error),
    Http(reqwest::Error),
    Zip(zip::result::ZipError),
}

impl fmt::Display for RuntimeInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetMissing(major) => {
                write!(formatter, "Adoptium did not return a Java {major} runtime")
            }
            Self::UnsafeArchivePath(path) => {
                write!(formatter, "Java archive contains an unsafe path: {path}")
            }
            Self::JavaMissing(path) => write!(
                formatter,
                "downloaded Java runtime has no bin/java.exe under {}",
                path.display()
            ),
            Self::HashMismatch { expected, actual } => write!(
                formatter,
                "Java runtime SHA-256 mismatch: expected {expected}, got {actual}"
            ),
            Self::Io(error) => write!(formatter, "Java runtime file system error: {error}"),
            Self::Http(error) => write!(formatter, "Java runtime download error: {error}"),
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
    let client = Client::builder().user_agent("MonaLauncher/0.1.0").build()?;
    let assets_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client.get(assets_url).send()?.error_for_status()?.json()?;
    let package = &assets
        .first()
        .ok_or(RuntimeInstallError::AssetMissing(major))?
        .binary
        .package;
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

fn download_and_verify(
    client: &Client,
    url: &str,
    expected: &str,
    target: &Path,
) -> Result<(), RuntimeInstallError> {
    if target.is_file() && file_sha256(target)? == expected {
        return Ok(());
    }

    let part = target.with_extension("zip.part");
    let mut response = client.get(url).send()?.error_for_status()?;
    let mut output = File::create(&part)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    output.flush()?;
    drop(output);

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(RuntimeInstallError::HashMismatch {
            expected: expected.to_owned(),
            actual,
        });
    }
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(part, target)?;
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
    let input = File::open(archive)?;
    let mut zip = ZipArchive::new(input)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
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
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn find_java(root: &Path) -> Result<Option<PathBuf>, RuntimeInstallError> {
    if !root.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let direct = path.join("bin").join("java.exe");
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
