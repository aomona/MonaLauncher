//! Pure translation, also tested on non-macOS hosts. Paths are passed as parameters, never SBPL.
use super::{Backend, FileAccess, PolicyError, SandboxPolicy};

pub fn render(policy: &SandboxPolicy) -> Result<String, PolicyError> {
    policy.compile(Backend::Seatbelt)?;
    let mut profile = include_str!("../platform/macos/minecraft.sb").to_owned();
    for (resource, access) in policy.requested_files() {
        let operations = match access {
            FileAccess::ReadOnly => "file-read*",
            FileAccess::ReadWrite => "file-read* file-write*",
        };
        profile.push_str(&format!(
            "\n(allow {operations} (subpath (param \"{}\")))\n",
            resource.parameter()
        ));
    }
    Ok(profile)
}
