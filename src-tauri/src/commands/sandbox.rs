use serde::Serialize;

#[cfg(windows)]
use crate::platform::windows::appcontainer_profile::{
    ensure_appcontainer_profile, profile_name_for_instance,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProfileInfo {
    pub instance_id: String,
    pub profile_name: String,
    pub sid: String,
    pub created: bool,
}

#[tauri::command]
pub fn ensure_sandbox_profile(instance_id: String) -> Result<SandboxProfileInfo, String> {
    ensure_sandbox_profile_for_platform(instance_id)
}

#[cfg(windows)]
fn ensure_sandbox_profile_for_platform(instance_id: String) -> Result<SandboxProfileInfo, String> {
    let profile_name =
        profile_name_for_instance(&instance_id).map_err(|error| error.to_string())?;

    let profile = ensure_appcontainer_profile(&profile_name).map_err(|error| error.to_string())?;

    Ok(SandboxProfileInfo {
        instance_id,
        profile_name: profile.name,
        sid: profile.sid,
        created: profile.created,
    })
}

#[cfg(not(windows))]
fn ensure_sandbox_profile_for_platform(_instance_id: String) -> Result<SandboxProfileInfo, String> {
    Err("AppContainer is only available on Windows".to_owned())
}
