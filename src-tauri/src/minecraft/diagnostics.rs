use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;
use sha1::{Digest, Sha1};

use super::fabric::{load_fabric_profile, maven_artifact_path, validate_profile};
use super::file_io::{path_is_link_or_reparse, read_bounded_file};
use super::installer::{load_instance, managed_java_major, MinecraftInstallError};
use super::model::{rules_allow, AssetIndex, ModLoader, VersionMetadata};
use super::paths::MinecraftPaths;

const MAX_METADATA_SIZE: u64 = 16 * 1024 * 1024;
const MAX_CHECKED_FILES: usize = 100_000;
const MAX_CHECKED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDiagnosis {
    pub instance_id: String,
    pub status: String,
    pub checked_files: usize,
    pub issue_count: usize,
    pub repairable_count: usize,
    pub checks: Vec<DiagnosticCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
    pub repairable: bool,
}

pub fn diagnosis_failure(instance_id: String, repairable: bool) -> InstanceDiagnosis {
    InstanceDiagnosis {
        instance_id,
        status: if repairable {
            "repairable"
        } else {
            "attention"
        }
        .to_owned(),
        checked_files: 0,
        issue_count: 1,
        repairable_count: usize::from(repairable),
        checks: vec![DiagnosticCheck {
            id: "managed-metadata".to_owned(),
            label: "管理情報".to_owned(),
            status: "error".to_owned(),
            detail: if repairable {
                "管理対象ファイルを読み取れません。公式データから再構築できます".to_owned()
            } else {
                "保存先または管理情報が安全要件を満たしていません".to_owned()
            },
            repairable,
        }],
    }
}

#[derive(Debug, Clone)]
struct ManagedFile {
    path: PathBuf,
    size: Option<u64>,
    sha1: Option<String>,
}

#[derive(Default)]
struct FileSummary {
    total: usize,
    missing: usize,
    corrupt: usize,
}

pub fn diagnose_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<InstanceDiagnosis, MinecraftInstallError> {
    let instance = load_instance(paths, instance_id)?;
    let mut checks = Vec::new();

    checks.push(DiagnosticCheck {
        id: "sandbox".to_owned(),
        label: "サンドボックス".to_owned(),
        status: if instance.sandboxed { "ok" } else { "error" }.to_owned(),
        detail: if instance.sandboxed {
            if cfg!(target_os = "macos") {
                "Seatbelt起動が有効です（実験対応）"
            } else {
                "AppContainer起動が有効です"
            }
            .to_owned()
        } else {
            "安全でない旧形式のインスタンスです".to_owned()
        },
        repairable: false,
    });

    let version_path = paths.version_json(&instance.version_id);
    let version: VersionMetadata =
        serde_json::from_slice(&read_bounded_file(&version_path, MAX_METADATA_SIZE)?)?;
    if version.id != instance.version_id {
        return Err(MinecraftInstallError::VersionIdMismatch {
            expected: instance.version_id.clone(),
            actual: version.id,
        });
    }

    let java_major = managed_java_major(paths, Path::new(&instance.java_path))?;
    let required_java = version
        .java_version
        .as_ref()
        .map_or(8, |java| java.major_version);
    let java_ok = java_major >= required_java;
    checks.push(DiagnosticCheck {
        id: "java".to_owned(),
        label: "Javaランタイム".to_owned(),
        status: if java_ok { "ok" } else { "error" }.to_owned(),
        detail: if java_ok {
            format!("Java {java_major}（必要: {required_java}）")
        } else {
            format!("Java {required_java}以上が必要ですが、Java {java_major}です")
        },
        repairable: !java_ok,
    });

    let features = HashMap::from([("is_demo_user".to_owned(), instance.demo)]);
    let mut core_files = vec![ManagedFile {
        path: paths.version_jar(&instance.version_id),
        size: Some(version.downloads.client.size),
        sha1: Some(version.downloads.client.sha1.clone()),
    }];
    let mut library_files = BTreeMap::new();
    for library in &version.libraries {
        if !rules_allow(library.rules.as_deref(), &features) {
            continue;
        }
        for download in [
            library.downloads.artifact.as_ref(),
            library.platform_native(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(relative) = download.path.as_deref() {
                let path = safe_join(&paths.libraries(), relative)?;
                library_files.entry(path.clone()).or_insert(ManagedFile {
                    path,
                    size: Some(download.size),
                    sha1: Some(download.sha1.clone()),
                });
            }
        }
    }

    let asset_index_path = paths
        .asset_indexes()
        .join(format!("{}.json", version.asset_index.id));
    core_files.push(ManagedFile {
        path: asset_index_path.clone(),
        size: None,
        sha1: Some(version.asset_index.sha1.clone()),
    });
    let asset_index: AssetIndex =
        serde_json::from_slice(&read_bounded_file(&asset_index_path, MAX_METADATA_SIZE)?)?;
    let mut asset_files = BTreeMap::new();
    for object in asset_index.objects.into_values() {
        validate_asset_hash(&object.hash)?;
        let path = paths
            .asset_objects()
            .join(&object.hash[..2])
            .join(&object.hash);
        asset_files.entry(path.clone()).or_insert(ManagedFile {
            path,
            size: Some(object.size),
            sha1: Some(object.hash),
        });
    }
    let libraries = library_files.into_values().collect::<Vec<_>>();
    let assets = asset_files.into_values().collect::<Vec<_>>();

    let core_summary = check_files(&core_files)?;
    let library_summary = check_files(&libraries)?;
    let asset_summary = check_files(&assets)?;
    push_file_check(&mut checks, "core", "Minecraft本体", core_summary);
    push_file_check(&mut checks, "libraries", "共有ライブラリ", library_summary);
    push_file_check(&mut checks, "assets", "ゲーム素材", asset_summary);

    let mut checked_files = core_files.len() + libraries.len() + assets.len();
    if let ModLoader::Fabric { version: loader } = &instance.mod_loader {
        let profile = load_fabric_profile(paths, instance_id)?;
        validate_profile(&profile, &instance.version_id, loader)?;
        let mut files = Vec::with_capacity(profile.libraries.len());
        for library in &profile.libraries {
            files.push(ManagedFile {
                path: paths.libraries().join(maven_artifact_path(&library.name)?),
                size: library.size,
                sha1: library.sha1.clone(),
            });
        }
        checked_files += files.len();
        push_file_check(&mut checks, "fabric", "Fabric Loader", check_files(&files)?);
    }

    let issue_count = checks.iter().filter(|check| check.status != "ok").count();
    let repairable_count = checks
        .iter()
        .filter(|check| check.status != "ok" && check.repairable)
        .count();
    Ok(InstanceDiagnosis {
        instance_id: instance.id,
        status: if issue_count == 0 {
            "healthy"
        } else if issue_count == repairable_count {
            "repairable"
        } else {
            "attention"
        }
        .to_owned(),
        checked_files,
        issue_count,
        repairable_count,
        checks,
    })
}

fn check_files(files: &[ManagedFile]) -> Result<FileSummary, MinecraftInstallError> {
    let planned_bytes = files.iter().try_fold(0_u64, |total, file| {
        total.checked_add(file.size.unwrap_or(0))
    });
    if files.len() > MAX_CHECKED_FILES || planned_bytes.is_none_or(|size| size > MAX_CHECKED_BYTES)
    {
        return Err(MinecraftInstallError::DownloadPlanTooLarge);
    }
    let results = files
        .par_iter()
        .map(check_file)
        .collect::<Result<Vec<_>, _>>()?;
    let mut summary = FileSummary {
        total: files.len(),
        ..FileSummary::default()
    };
    for result in results {
        match result {
            FileState::Ok => {}
            FileState::Missing => summary.missing += 1,
            FileState::Corrupt => summary.corrupt += 1,
        }
    }
    Ok(summary)
}

enum FileState {
    Ok,
    Missing,
    Corrupt,
}

fn check_file(file: &ManagedFile) -> Result<FileState, MinecraftInstallError> {
    if !file.path.exists() {
        return Ok(FileState::Missing);
    }
    if !file.path.is_file() || path_is_link_or_reparse(&file.path)? {
        return Ok(FileState::Corrupt);
    }
    if let Some(expected) = file.size {
        if fs::metadata(&file.path).map_or(true, |metadata| metadata.len() != expected) {
            return Ok(FileState::Corrupt);
        }
    }
    if let Some(expected) = &file.sha1 {
        if file_sha1(&file.path)? != expected.to_ascii_lowercase() {
            return Ok(FileState::Corrupt);
        }
    }
    Ok(FileState::Ok)
}

fn push_file_check(checks: &mut Vec<DiagnosticCheck>, id: &str, label: &str, summary: FileSummary) {
    let issues = summary.missing + summary.corrupt;
    checks.push(DiagnosticCheck {
        id: id.to_owned(),
        label: label.to_owned(),
        status: if issues == 0 { "ok" } else { "error" }.to_owned(),
        detail: if issues == 0 {
            format!("{}ファイルを検証しました", summary.total)
        } else {
            format!(
                "{}ファイル中、欠損{}件・破損{}件",
                summary.total, summary.missing, summary.corrupt
            )
        },
        repairable: issues != 0,
    });
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, MinecraftInstallError> {
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

fn validate_asset_hash(hash: &str) -> Result<(), MinecraftInstallError> {
    if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MinecraftInstallError::InvalidAssetHash(hash.to_owned()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_diagnostic_library_paths() {
        assert!(safe_join(Path::new("libraries"), "org/example/demo.jar").is_ok());
        assert!(safe_join(Path::new("libraries"), "../outside.jar").is_err());
        assert!(validate_asset_hash(&"a".repeat(40)).is_ok());
        assert!(validate_asset_hash("invalid").is_err());
    }
}
