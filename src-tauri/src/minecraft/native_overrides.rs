//! Pinned upstream native artifacts for Linux ARM64, absent from Mojang's x64 Linux rules.
use super::model::{DownloadInfo, VersionMetadata};
use std::{collections::BTreeMap, io, sync::OnceLock};
fn pins() -> &'static BTreeMap<String, DownloadInfo> {
    static PINS: OnceLock<BTreeMap<String, DownloadInfo>> = OnceLock::new();
    PINS.get_or_init(|| {
        serde_json::from_str(include_str!("linux-arm64-natives.json"))
            .expect("checked-in native pins are valid")
    })
}
pub fn trusted_url(url: &str) -> bool {
    pins().values().any(|pin| pin.url == url)
}
impl VersionMetadata {
    pub fn for_current_platform(self) -> io::Result<Self> {
        if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            self.with_linux_arm64_natives()
        } else {
            Ok(self)
        }
    }
    fn with_linux_arm64_natives(mut self) -> io::Result<Self> {
        for library in &mut self.libraries {
            if let Some(artifact) = &mut library.downloads.artifact {
                if let Some(path) = &artifact.path {
                    if path.starts_with("org/lwjgl/") && path.ends_with("-natives-linux.jar") {
                        *artifact = pins().get(path).cloned().ok_or_else(|| io::Error::other(format!(
                            "Linux ARM64 native support is not pinned for {path}; currently LWJGL 3.4.1 is supported"
                        )))?;
                    }
                }
            }
            if library
                .natives
                .as_ref()
                .is_some_and(|n| n.contains_key("linux"))
            {
                return Err(io::Error::other(
                    "legacy Linux natives are not supported on ARM64",
                ));
            }
        }
        Ok(self)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_urls_are_exact_pins_not_an_open_maven_allowlist() {
        for (old, native) in pins() {
            assert!(old.ends_with("-natives-linux.jar"));
            assert!(native
                .path
                .as_ref()
                .unwrap()
                .ends_with("-natives-linux-arm64.jar"));
            assert!(trusted_url(&native.url));
            assert!(!trusted_url(&format!("{}?untrusted", native.url)));
            assert_eq!(native.sha1.len(), 40);
        }
        assert!(!trusted_url(
            "https://repo.maven.apache.org/maven2/untrusted.jar"
        ));
    }
}
