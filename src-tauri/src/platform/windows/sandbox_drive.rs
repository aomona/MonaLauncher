use std::error::Error;
use std::fmt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetLogicalDrives, QueryDosDeviceW};
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
    expected_target: Vec<u16>,
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
                let expected_target = match query_device_target(&device) {
                    Ok(target) => target,
                    Err(_) => {
                        let _ = Command::new("subst.exe")
                            .args([&device, "/D"])
                            .creation_flags(CREATE_NO_WINDOW.0)
                            .status();
                        continue;
                    }
                };
                return Ok(Self {
                    device,
                    expected_target,
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
        // A drive letter can be removed and reused while a process is shutting down. Only delete
        // the exact DOS-device mapping created by this owner, never a replacement mapping.
        if query_device_target(&self.device).ok().as_deref()
            != Some(self.expected_target.as_slice())
        {
            return;
        }
        let _ = Command::new("subst.exe")
            .args([&self.device, "/D"])
            .creation_flags(CREATE_NO_WINDOW.0)
            .status();
    }
}

fn query_device_target(device: &str) -> Result<Vec<u16>, std::io::Error> {
    let name = device.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut buffer = vec![0_u16; 32 * 1024];
    // SAFETY: name is NUL-terminated and buffer is valid writable storage for the call.
    let written = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut buffer)) };
    if written == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let used = usize::try_from(written)
        .unwrap_or(buffer.len())
        .min(buffer.len());
    let end = buffer[..used]
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(used);
    buffer.truncate(end);
    Ok(buffer)
}
