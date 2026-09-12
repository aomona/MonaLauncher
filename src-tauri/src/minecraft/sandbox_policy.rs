//! The single Minecraft-to-sandbox resource mapping used by both launcher backends.
use super::{model::InstanceManifest, paths::MinecraftPaths};
use crate::sandbox::{PolicyError, SandboxPolicy, SandboxResources};
use std::path::Path;

pub fn policy_for_instance(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    launch: &Path,
) -> Result<SandboxPolicy, PolicyError> {
    super::installer::validate_instance_id(&instance.id).map_err(|e| PolicyError(e.to_string()))?;
    let java =
        std::fs::canonicalize(&instance.java_path).map_err(|e| PolicyError(e.to_string()))?;
    let runtimes =
        std::fs::canonicalize(paths.runtimes()).map_err(|e| PolicyError(e.to_string()))?;
    if !java.starts_with(&runtimes) {
        return Err(PolicyError(
            "sandbox Java is outside managed runtimes".into(),
        ));
    }
    let expected = if cfg!(windows) { "java.exe" } else { "java" };
    if !java
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
    {
        return Err(PolicyError("sandbox executable is not Java".into()));
    }
    let java_home = java
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| PolicyError("Java home is missing".into()))?;
    let policy = SandboxPolicy::minecraft(SandboxResources {
        data_root: paths.root().to_owned(),
        runtimes_root: runtimes,
        versions_root: paths.versions(),
        instance_root: paths.instance(&instance.id),
        java_home: java_home.to_owned(),
        libraries: paths.libraries(),
        assets: paths.assets(),
        version: paths.version_directory(&instance.version_id),
        game: paths.instance_game_directory(&instance.id),
        launch: launch.to_owned(),
        temp: launch.join("tmp"),
    })?;
    instance.permissions.apply(policy)
}
