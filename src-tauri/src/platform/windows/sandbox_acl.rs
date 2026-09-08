use std::error::Error;
use std::fmt;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::minecraft::model::InstanceManifest;
use crate::minecraft::paths::MinecraftPaths;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
pub enum SandboxAclError {
    MissingPath(PathBuf),
    InvalidJavaPath(PathBuf),
    GrantFailed { path: PathBuf, details: String },
    IntegrityLevelFailed { path: PathBuf, details: String },
    LockLaunchDirectoryFailed { path: PathBuf, details: String },
    Io(std::io::Error),
}

impl fmt::Display for SandboxAclError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPath(path) => {
                write!(
                    formatter,
                    "sandbox access target is missing: {}",
                    path.display()
                )
            }
            Self::InvalidJavaPath(path) => {
                write!(
                    formatter,
                    "Java path has no runtime root: {}",
                    path.display()
                )
            }
            Self::GrantFailed { path, details } => write!(
                formatter,
                "failed to grant AppContainer access to {}: {details}",
                path.display()
            ),
            Self::IntegrityLevelFailed { path, details } => write!(
                formatter,
                "failed to set low integrity on {}: {details}",
                path.display()
            ),
            Self::LockLaunchDirectoryFailed { path, details } => write!(
                formatter,
                "failed to lock sandbox launch directory {}: {details}",
                path.display()
            ),
            Self::Io(error) => write!(formatter, "failed to run icacls: {error}"),
        }
    }
}

/// 起動ごとに作る専用ディレクトリを、そのAppContainerだけが変更できるようにする。
///
/// インスタンス側から継承したACLを明示ACLへ変換し、対象SIDの許可をMへ統一する。
/// ユーザー・SYSTEM・AdministratorsのACLは保持する。
pub fn lock_sandbox_launch_directory(
    path: &Path,
    appcontainer_sid: &str,
) -> Result<(), SandboxAclError> {
    if !path.is_dir() {
        return Err(SandboxAclError::MissingPath(path.to_owned()));
    }
    let sid = format!("*{appcontainer_sid}");
    let principal = format!("*{appcontainer_sid}:(OI)(CI)M");
    for arguments in [
        vec!["/inheritance:d".to_owned(), "/Q".to_owned()],
        vec!["/remove:g".to_owned(), sid, "/Q".to_owned()],
        vec!["/grant".to_owned(), principal, "/Q".to_owned()],
    ] {
        let output = Command::new("icacls.exe")
            .arg(path)
            .args(arguments)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if !output.status.success() {
            let details = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(SandboxAclError::LockLaunchDirectoryFailed {
                path: path.to_owned(),
                details: details.trim().to_owned(),
            });
        }
    }
    set_integrity_level(path, "L")?;
    Ok(())
}

impl Error for SandboxAclError {}

impl From<std::io::Error> for SandboxAclError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Minecraftに必要な場所だけをAppContainer SIDへ公開する。
///
/// 設定・起動メタデータは読み取り専用に保ち、ゲームが変更できるのは `game` と
/// 起動ごとの作業ディレクトリだけにする。`grant:r` を使うことで、旧バージョンが
/// インスタンス全体へ付けた変更権限も起動時に縮小される。
pub fn grant_minecraft_access(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    appcontainer_sid: &str,
) -> Result<(), SandboxAclError> {
    let java_path = fs::canonicalize(Path::new(&instance.java_path))
        .map_err(|_| SandboxAclError::InvalidJavaPath(PathBuf::from(&instance.java_path)))?;
    let runtimes = fs::canonicalize(paths.runtimes())
        .map_err(|_| SandboxAclError::InvalidJavaPath(java_path.clone()))?;
    if !java_path.starts_with(&runtimes)
        || !java_path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("java.exe"))
    {
        return Err(SandboxAclError::InvalidJavaPath(java_path));
    }
    let java_root = java_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| SandboxAclError::InvalidJavaPath(java_path.to_owned()))?;

    // Traverse-only roots are deliberately non-inheriting: an instance must not be able to read
    // another instance's settings merely because all data lives below the same Minecraft root.
    for traverse_path in [
        paths.root(),
        paths.instances().as_path(),
        paths.runtimes().as_path(),
    ] {
        grant(traverse_path, appcontainer_sid, "RX", false)?;
    }
    let mut java_ancestor = java_root.parent();
    while let Some(ancestor) = java_ancestor {
        if ancestor == paths.runtimes() {
            break;
        }
        if ancestor.starts_with(&runtimes) {
            grant(ancestor, appcontainer_sid, "RX", false)?;
        }
        java_ancestor = ancestor.parent();
    }

    for read_execute_path in [
        java_root,
        paths.assets().as_path(),
        paths.libraries().as_path(),
        paths.versions().as_path(),
    ] {
        grant(read_execute_path, appcontainer_sid, "RX", true)?;
    }

    let instance_directory = paths.instance(&instance.id);
    let game_directory = paths.instance_game_directory(&instance.id);
    grant(&instance_directory, appcontainer_sid, "RX", true)?;
    set_integrity_level(&instance_directory, "M")?;
    grant(&game_directory, appcontainer_sid, "M", true)?;
    set_integrity_level(&game_directory, "L")?;

    Ok(())
}

fn set_integrity_level(path: &Path, level: &str) -> Result<(), SandboxAclError> {
    let label = format!("(OI)(CI){level}");
    let output = Command::new("icacls.exe")
        .arg(path)
        .args(["/setintegritylevel", &label, "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Err(SandboxAclError::IntegrityLevelFailed {
        path: path.to_owned(),
        details: details.trim().to_owned(),
    })
}

fn grant(path: &Path, sid: &str, permission: &str, inherit: bool) -> Result<(), SandboxAclError> {
    if !path.exists() {
        return Err(SandboxAclError::MissingPath(path.to_owned()));
    }

    let inheritance = if inherit { "(OI)(CI)" } else { "" };
    let principal = format!("*{sid}:{inheritance}{permission}");
    let output = Command::new("icacls.exe")
        .arg(path)
        .args(["/grant:r", &principal, "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Err(SandboxAclError::GrantFailed {
        path: path.to_owned(),
        details: details.trim().to_owned(),
    })
}
