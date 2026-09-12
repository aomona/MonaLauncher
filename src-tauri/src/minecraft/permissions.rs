//! Persisted, bounded user preferences. Paths and backend exceptions stay launcher-owned.
use serde::{Deserialize, Serialize};

use super::{
    file_io::write_atomic, installer::load_instance, model::InstanceManifest, paths::MinecraftPaths,
};
use crate::sandbox::{
    FileAccess, GameDirectory, NetworkAccess, PolicyError, Resource, SandboxPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstancePermissions {
    pub game_write: bool,
    pub narrator: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default = "enabled")]
    pub audio_output: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default = "enabled")]
    pub worlds_write: bool,
    #[serde(default = "enabled")]
    pub screenshots_write: bool,
    #[serde(default = "enabled")]
    pub resource_packs_write: bool,
    #[serde(default = "enabled")]
    pub shader_packs_write: bool,
    #[serde(default = "enabled")]
    pub mods_write: bool,
    #[serde(default = "enabled")]
    pub config_write: bool,
    #[serde(default = "enabled")]
    pub logs_write: bool,
}

fn enabled() -> bool {
    true
}

impl Default for InstancePermissions {
    fn default() -> Self {
        Self {
            game_write: true,
            narrator: true,
            network: false,
            audio_output: true,
            microphone: false,
            clipboard: false,
            worlds_write: true,
            screenshots_write: true,
            resource_packs_write: true,
            shader_packs_write: true,
            mods_write: true,
            config_write: true,
            logs_write: true,
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
        policy.network = if self.network {
            NetworkAccess::Internet
        } else {
            NetworkAccess::Denied
        };
        policy.desktop.audio_output = self.audio_output;
        policy.desktop.microphone = self.microphone;
        policy.desktop.clipboard = self.clipboard;
        let restricted: Vec<_> = [
            (GameDirectory::Worlds, self.worlds_write),
            (GameDirectory::Screenshots, self.screenshots_write),
            (GameDirectory::ResourcePacks, self.resource_packs_write),
            (GameDirectory::ShaderPacks, self.shader_packs_write),
            (GameDirectory::Mods, self.mods_write),
            (GameDirectory::Config, self.config_write),
            (GameDirectory::Logs, self.logs_write),
        ]
        .into_iter()
        .filter_map(|(area, writable)| (!writable).then_some(area))
        .collect();
        policy = policy.with_readonly_game_directories(&restricted)?;
        Ok(policy)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSupport {
    pub platform: &'static str,
    pub editable: bool,
    pub audio_output: bool,
    pub microphone: bool,
    pub clipboard: bool,
}

pub fn permission_support() -> PermissionSupport {
    PermissionSupport {
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unsupported"
        },
        editable: cfg!(any(windows, target_os = "macos", target_os = "linux")),
        audio_output: cfg!(any(target_os = "macos", target_os = "linux")),
        microphone: cfg!(target_os = "macos"),
        clipboard: cfg!(target_os = "macos"),
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
    validate_change(instance.permissions, permissions, &permission_support())?;
    instance.permissions = permissions;
    write_atomic(
        &paths.instance_manifest(instance_id),
        &serde_json::to_vec_pretty(&instance).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(instance)
}

fn validate_change(
    previous: InstancePermissions,
    next: InstancePermissions,
    support: &PermissionSupport,
) -> Result<(), String> {
    // Imported settings may be incompatible. Permit unchanged fields and reductions so
    // the UI can repair them one at a time; launch still validates the entire policy.
    if (!support.audio_output && !next.audio_output && previous.audio_output)
        || (!support.microphone && next.microphone && !previous.microphone)
        || (!support.clipboard && next.clipboard && !previous.clipboard)
    {
        return Err(
            "このOSでは指定された音声・マイク・クリップボードの個別制御に対応していません".into(),
        );
    }
    if next.microphone && !next.audio_output && (!previous.microphone || previous.audio_output) {
        return Err("マイクを許可するには通常音声へのアクセスも有効にしてください".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_imports_can_be_repaired_without_accepting_new_unsupported_grants() {
        let support = PermissionSupport {
            platform: "windows",
            editable: true,
            audio_output: false,
            microphone: false,
            clipboard: false,
        };
        let defaults = InstancePermissions::default();
        let imported = InstancePermissions {
            microphone: true,
            clipboard: true,
            audio_output: false,
            ..defaults
        };
        assert!(validate_change(defaults, imported, &support).is_err());
        let reduced = InstancePermissions {
            microphone: false,
            ..imported
        };
        assert!(validate_change(imported, reduced, &support).is_ok());
        assert!(validate_change(reduced, defaults, &support).is_ok());
    }
    #[test]
    fn older_saved_permissions_keep_the_previous_access_without_enabling_network_or_capture() {
        let old: InstancePermissions =
            serde_json::from_str(r#"{"gameWrite":false,"narrator":true}"#).unwrap();
        assert!(!old.game_write && old.worlds_write && old.audio_output);
        assert!(!old.network && !old.microphone && !old.clipboard);
        assert_eq!(
            serde_json::from_str::<InstancePermissions>(&serde_json::to_string(&old).unwrap())
                .unwrap(),
            old
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_permission_requests() {
        for json in [
            r#"{"gameWrite":true,"narrator":false,"arbitraryHostPath":true}"#,
            r#"{"gameWrite":true}"#,
            r#"{"gameWrite":"false","narrator":true}"#,
            r#"{}"#,
        ] {
            assert!(serde_json::from_str::<InstancePermissions>(json).is_err());
        }
    }
}
