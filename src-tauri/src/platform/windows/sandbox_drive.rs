use std::error::Error;
use std::fmt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::Storage::FileSystem::GetLogicalDrives;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Debug)]
pub enum SandboxDriveError {
    NoDriveLetter,
    MappingFailed(String),
    Io(std::io::Error),
}

impl fmt::Display for SandboxDriveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDriveLetter => write!(formatter, "no drive letter is available for sandbox"),
            Self::MappingFailed(details) => {
                write!(formatter, "failed to map sandbox drive: {details}")
            }
            Self::Io(error) => write!(formatter, "sandbox drive path error: {error}"),
        }
    }
}

impl Error for SandboxDriveError {}

impl From<std::io::Error> for SandboxDriveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub struct SandboxDrive {
    device: String,
    root: PathBuf,
}

impl SandboxDrive {
    pub fn create(target: &Path) -> Result<Self, SandboxDriveError> {
        let target = target.canonicalize()?;
        // SAFETY: GetLogicalDrives takes no pointers and returns the current drive mask.
        let used = unsafe { GetLogicalDrives() };

        for letter in ('P'..='Z').rev() {
            let bit = 1_u32 << (letter as u32 - 'A' as u32);
            if used & bit != 0 {
                continue;
            }

            let device = format!("{letter}:");
            let output = Command::new("subst.exe")
                .arg(&device)
                .arg(&target)
                .creation_flags(CREATE_NO_WINDOW.0)
                .output()?;
            if output.status.success() {
                return Ok(Self {
                    device,
                    root: PathBuf::from(format!("{letter}:\\")),
                });
            }
        }

        Err(SandboxDriveError::NoDriveLetter)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for SandboxDrive {
    fn drop(&mut self) {
        let _ = Command::new("subst.exe")
            .args([&self.device, "/D"])
            .creation_flags(CREATE_NO_WINDOW.0)
            .status();
    }
}
