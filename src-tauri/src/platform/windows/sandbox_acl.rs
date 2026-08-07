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
            Self::Io(error) => write!(formatter, "failed to run icacls: {error}"),
        }
    }
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
        java_root,
        paths.assets().as_path(),
        paths.libraries().as_path(),
        paths.versions().as_path(),
    ] {
        grant(read_execute_path, appcontainer_sid, "RX")?;
    }

    grant(&paths.instance(&instance.id), appcontainer_sid, "M")?;

    Ok(())
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
