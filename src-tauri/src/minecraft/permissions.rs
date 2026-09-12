//! Persisted, bounded user preferences. Paths and backend exceptions stay launcher-owned.
use serde::{Deserialize, Serialize};

use super::{
    file_io::write_atomic, installer::load_instance, model::InstanceManifest, paths::MinecraftPaths,
};
use crate::sandbox::{FileAccess, PolicyError, Resource, SandboxPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstancePermissions {
    pub game_write: bool,
    pub narrator: bool,
}

impl Default for InstancePermissions {
    fn default() -> Self {
        Self {
            game_write: true,
            narrator: true,
        }
    }
}

impl InstancePermissions {
    pub fn apply(self, policy: SandboxPolicy) -> Result<SandboxPolicy, PolicyError> {
        let mut policy = policy.with_file_access(
            Resource::Game,
            if self.game_write {
                FileAccess::ReadWrite
            } else {
                FileAccess::ReadOnly
            },
        )?;
        policy.narrator = self.narrator;
        Ok(policy)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSupport {
    pub platform: &'static str,
    pub editable: bool,
}

pub fn permission_support() -> PermissionSupport {
    PermissionSupport {
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unsupported"
        },
        editable: cfg!(any(windows, target_os = "macos")),
    }
}

pub fn save_permissions(
    paths: &MinecraftPaths,
    instance_id: &str,
    permissions: InstancePermissions,
) -> Result<InstanceManifest, String> {
    if !permission_support().editable {
        return Err("このOSではインスタンスの権限設定にまだ対応していません".into());
    }
    let mut instance = load_instance(paths, instance_id).map_err(|e| e.to_string())?;
    if !instance.sandboxed {
        return Err("旧形式のインスタンスでは権限を変更できません".into());
    }
    instance.permissions = permissions;
    write_atomic(
        &paths.instance_manifest(instance_id),
        &serde_json::to_vec_pretty(&instance).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_or_incomplete_permission_requests() {
        for json in [
            r#"{"gameWrite":true,"narrator":false,"network":true}"#,
            r#"{"gameWrite":true}"#,
            r#"{"gameWrite":"false","narrator":true}"#,
            r#"{}"#,
        ] {
            assert!(serde_json::from_str::<InstancePermissions>(json).is_err());
        }
    }
}
