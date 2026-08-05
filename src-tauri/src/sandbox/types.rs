use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProfilePreview {
    pub instance_id: String,
    pub profile_name: String,
}
