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
    for (index, _) in policy.readonly_game_directories().iter().enumerate() {
        profile.push_str(&format!(
            "\n(deny file-write* (subpath (param \"GAME_READONLY_{index}\")))\n"
        ));
    }
    if policy.network == super::NetworkAccess::Internet {
        profile.push_str(include_str!("../platform/macos/network.sb"));
    }
    if policy.desktop.audio_output || policy.desktop.microphone {
        profile.push_str(include_str!("../platform/macos/audio.sb"));
    }
    if policy.desktop.microphone {
        profile.push_str("\n(allow device-microphone)\n");
    }
    if policy.desktop.clipboard {
        profile.push_str("\n(allow mach-lookup (global-name \"com.apple.pasteboard.1\"))\n");
    }
    Ok(profile)
}
