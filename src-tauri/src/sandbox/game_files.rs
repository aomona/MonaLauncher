//! Fixed game-directory restrictions. No caller-supplied paths or recursive host ACL targets.
use super::PolicyError;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameDirectory {
    Worlds,
    Screenshots,
    ResourcePacks,
    ShaderPacks,
    Mods,
    Config,
    Logs,
}
impl GameDirectory {
    pub fn name(self) -> &'static str {
        match self {
            Self::Worlds => "saves",
            Self::Screenshots => "screenshots",
            Self::ResourcePacks => "resourcepacks",
            Self::ShaderPacks => "shaderpacks",
            Self::Mods => "mods",
            Self::Config => "config",
            Self::Logs => "logs",
        }
    }
}

/// Pre-existing aliases could bypass mount/path restrictions or redirect Windows ACL edits.
/// The trusted launcher invokes this before starting a game, never while it is running.
pub fn validate_tree(root: &Path) -> Result<(), PolicyError> {
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|e| PolicyError(e.to_string()))?;
        let mut alias = metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            alias |= metadata.file_attributes() & 0x400 != 0;
            if metadata.is_file() && !alias {
                use std::os::windows::io::AsRawHandle;
                use windows::Win32::{
                    Foundation::HANDLE,
                    Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
                };
                let file = fs::File::open(&path).map_err(|e| PolicyError(e.to_string()))?;
                let mut info = BY_HANDLE_FILE_INFORMATION::default();
                // SAFETY: file retains the handle and info is a valid output structure.
                unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
                    .map_err(|e| PolicyError(e.to_string()))?;
                alias |= info.nNumberOfLinks != 1;
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            alias |= metadata.is_file() && metadata.nlink() != 1;
        }
        if alias || !(metadata.is_dir() || metadata.is_file()) {
            return Err(PolicyError(format!(
                "granular permissions require regular files/directories without aliases: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|e| PolicyError(e.to_string()))? {
                pending.push(entry.map_err(|e| PolicyError(e.to_string()))?.path());
            }
        }
    }
    Ok(())
}
