use std::error::Error;
use std::fmt;
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
/// 共有ゲームファイルとJavaは読み取り・実行、インスタンス固有の場所は変更を許可する。
pub fn grant_minecraft_access(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    appcontainer_sid: &str,
) -> Result<(), SandboxAclError> {
    let java_path = Path::new(&instance.java_path);
    let java_root = java_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| SandboxAclError::InvalidJavaPath(java_path.to_owned()))?;

    for read_execute_path in [
        paths.root(),
        java_root,
        paths.assets().as_path(),
        paths.libraries().as_path(),
        paths.versions().as_path(),
    ] {
        grant(read_execute_path, appcontainer_sid, "RX")?;
    }

    let instance_directory = paths.instance(&instance.id);
    grant(&instance_directory, appcontainer_sid, "M")?;
    set_low_integrity(&instance_directory)?;

    Ok(())
}

fn set_low_integrity(path: &Path) -> Result<(), SandboxAclError> {
    let output = Command::new("icacls.exe")
        .arg(path)
        .args(["/setintegritylevel", "(OI)(CI)L", "/Q"])
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

fn grant(path: &Path, sid: &str, permission: &str) -> Result<(), SandboxAclError> {
    if !path.exists() {
        return Err(SandboxAclError::MissingPath(path.to_owned()));
    }

    let principal = format!("*{sid}:(OI)(CI){permission}");
    let output = Command::new("icacls.exe")
        .arg(path)
        .args(["/grant", &principal, "/Q"])
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
