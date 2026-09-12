//! Launcher-owned cache locations outside writable game and transient launch data.
use super::{FileAccess, FileGrant, PolicyError, SandboxResources};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct RuntimeCaches {
    pub assets: PathBuf,
    pub skins: PathBuf,
    pub graphics: PathBuf,
}

/// Reject aliases before any creation or recursive ACL update.
pub fn owned_directory(parent: &Path, name: &str) -> Result<PathBuf, PolicyError> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(PolicyError("cache directory is a reparse point".into()));
                }
            }
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(PolicyError(
                    "cache directory is an alias or not a directory".into(),
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|e| PolicyError(e.to_string()))?;
        }
        Err(e) => return Err(PolicyError(e.to_string())),
    }
    if fs::canonicalize(&path).map_err(|e| PolicyError(e.to_string()))? != path {
        return Err(PolicyError("cache directory is an alias".into()));
    }
    Ok(path)
}

impl RuntimeCaches {
    pub fn prepare(resources: &SandboxResources) -> Result<Self, PolicyError> {
        let root = owned_directory(&resources.instance_root, "runtime-cache")?;
        for protected in [
            &resources.game,
            &resources.launch,
            &resources.java_home,
            &resources.assets,
            &resources.libraries,
            &resources.version,
        ] {
            if root.starts_with(protected) || protected.starts_with(&root) {
                return Err(PolicyError(
                    "runtime cache overlaps another resource".into(),
                ));
            }
        }
        let assets = owned_directory(&root, "assets")?;
        let skins = owned_directory(&assets, "skins")?;
        let graphics = owned_directory(&root, "graphics")?;
        for path in [&skins, &graphics] {
            super::game_files::validate_tree(path)?;
        }
        Ok(Self {
            assets,
            skins,
            graphics,
        })
    }

    pub fn grants(&self, skin: bool, graphics: bool) -> Vec<FileGrant> {
        let access = |enabled| {
            if enabled {
                FileAccess::ReadWrite
            } else {
                FileAccess::ReadOnly
            }
        };
        vec![
            FileGrant {
                path: self.assets.clone(),
                access: FileAccess::ReadOnly,
            },
            FileGrant {
                path: self.skins.clone(),
                access: access(skin),
            },
            FileGrant {
                path: self.graphics.clone(),
                access: access(graphics),
            },
        ]
    }
}
