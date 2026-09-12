use std::error::Error;
use std::fmt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sandbox::{Backend, FileAccess, SandboxPolicy};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
pub enum SandboxAclError {
    Policy(String),
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
            Self::Policy(error) => write!(formatter, "unsupported sandbox policy: {error}"),
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

/// Apply the compiled common policy; compatibility additions are explicit in its plan.
pub fn grant_policy_access(
    policy: &SandboxPolicy,
    appcontainer_sid: &str,
) -> Result<(), SandboxAclError> {
    let plan = policy
        .compile(Backend::AppContainer)
        .map_err(|error| SandboxAclError::Policy(error.to_string()))?;
    // Never recurse ACL changes through game-controlled junctions or symlinks.
    crate::sandbox::game_files::validate_tree(&policy.resources().game)
        .map_err(|e| SandboxAclError::Policy(e.to_string()))?;
    update_deny(&policy.resources().game, appcontainer_sid, None, true)?;
    if let Some(caches) = &policy.caches {
        for path in [&caches.skins, &caches.graphics] {
            crate::sandbox::game_files::validate_tree(path)
                .map_err(|e| SandboxAclError::Policy(e.to_string()))?;
            update_deny(path, appcontainer_sid, None, true)?;
        }
    }
    for path in &plan.traverse {
        grant(path, appcontainer_sid, "RX", false)?;
    }
    // Apply parent read-only grants first, then explicit writable children. This also replaces
    // the broad inherited instance grants from older launcher versions.
    for access in [FileAccess::ReadOnly, FileAccess::ReadWrite] {
        for file in plan.files.iter().filter(|file| file.access == access) {
            if file.path == policy.resources().launch {
                lock_sandbox_launch_directory(&file.path, appcontainer_sid)?;
                continue;
            }
            let permission = match access {
                FileAccess::ReadOnly => "RX",
                FileAccess::ReadWrite => "M",
            };
            grant(&file.path, appcontainer_sid, permission, true)?;
            if file.path == policy.resources().instance_root {
                set_integrity_level(&file.path, "M")?;
            } else if access == FileAccess::ReadWrite {
                set_integrity_level(&file.path, "L")?;
            }
        }
    }
    if let Some(caches) = &policy.caches {
        if !policy.skin_cache {
            update_deny(
                &caches.skins,
                appcontainer_sid,
                Some("(OI)(CI)(WD,AD,WEA,WA,DE,DC,WDAC,WO)"),
                true,
            )?;
        }
    }
    if !policy.readonly_game_directories().is_empty() {
        // Parent DELETE_CHILD must not allow replacement of a protected directory.
        update_deny(
            &policy.resources().game,
            appcontainer_sid,
            Some("(DC)"),
            false,
        )?;
        for path in policy.readonly_game_directories() {
            update_deny(
                path,
                appcontainer_sid,
                Some("(OI)(CI)(WD,AD,WEA,WA,DE,DC,WDAC,WO)"),
                true,
            )?;
        }
    }
    Ok(())
}

fn update_deny(
    path: &Path,
    sid: &str,
    rights: Option<&str>,
    recursive: bool,
) -> Result<(), SandboxAclError> {
    let mut command = Command::new("icacls.exe");
    command.arg(path);
    if let Some(rights) = rights {
        command.arg("/deny").arg(format!("*{sid}:{rights}"));
    } else {
        command.arg("/remove:d").arg(format!("*{sid}"));
    }
    if recursive {
        command.arg("/T");
    }
    let output = command
        .args(["/L", "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        return Err(SandboxAclError::GrantFailed {
            path: path.to_owned(),
            details: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
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
