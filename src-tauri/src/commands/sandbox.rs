use crate::sandbox::profile::profile_name_for_instance;
use crate::sandbox::types::SandboxProfilePreview;

#[tauri::command]
pub fn preview_sandbox_profile(instance_id: String) -> Result<SandboxProfilePreview, String> {
    let profile_name =
        profile_name_for_instance(&instance_id).map_err(|error| error.to_string())?;

    Ok(SandboxProfilePreview {
        instance_id,
        profile_name,
    })
}
