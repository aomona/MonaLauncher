use crate::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn name() -> Result<&'static str> {
    match std::env::consts::OS {
        "macos" => Ok("seatbelt"),
        "linux" => Ok("bubblewrap-namespaces-only"),
        _ => Err("use the existing src-tauri sandbox_probe on Windows; this lab backend is not implemented".into()),
    }
}

pub fn command(root: &Path, java: &Path, program: &Path, gui: bool) -> Result<Command> {
    match std::env::consts::OS {
        "macos" => {
            let mut cmd = Command::new("/usr/bin/sandbox-exec");
            for (key, path) in [
                ("RUNTIME", root.join("runtime")),
                ("JAVA_HOME", java.to_path_buf()),
                ("SHARED", root.join("shared")),
                ("INSTANCE", root.join("instance")),
                ("GAME", root.join("game")),
            ] {
                cmd.arg("-D").arg(format!("{key}={}", path.display()));
            }
            let mut profile = include_str!("../profiles/macos.sb").to_owned();
            if gui {
                profile.push_str(include_str!("../profiles/macos-gui.sb"));
            }
            cmd.arg("-p").arg(profile).arg(program);
            Ok(cmd)
        }
        "linux" => {
            if gui {
                return Err(
                    "Linux desktop grants have not been implemented; run the headless probes first"
                        .into(),
                );
            }
            let bwrap = ["/usr/bin/bwrap", "/bin/bwrap"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
                .ok_or("bubblewrap is required; there is no unsandboxed fallback")?;
            let mut cmd = Command::new(bwrap);
            cmd.args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop",
                "ALL",
            ]);
            for path in [
                "/usr",
                "/bin",
                "/lib",
                "/lib64",
                "/etc/ld.so.cache",
                "/etc/alternatives",
            ] {
                if Path::new(path).exists() {
                    cmd.arg("--ro-bind").arg(path).arg(path);
                }
            }
            cmd.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
            for path in [
                java.to_path_buf(),
                root.join("runtime"),
                root.join("shared"),
                root.join("instance"),
            ] {
                cmd.arg("--ro-bind").arg(&path).arg(&path);
            }
            // Debian/Ubuntu JDK packages symlink these files into /etc. Bind
            // only their resolved files, not /etc or the entire config tree.
            for path in external_java_config_files(java)? {
                cmd.arg("--ro-bind").arg(&path).arg(&path);
            }
            cmd.arg("--bind")
                .arg(root.join("game"))
                .arg(root.join("game"));
            cmd.arg("--chdir")
                .arg(root.join("game"))
                .arg("--")
                .arg(program);
            Ok(cmd)
        }
        _ => Err("unsupported lab backend".into()),
    }
}

fn external_java_config_files(java: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for relative in ["conf/security/java.security", "conf/net.properties"] {
        let resolved = std::fs::canonicalize(java.join(relative))?;
        if !resolved.is_file() {
            return Err(format!(
                "Java configuration is not a regular file: {}",
                resolved.display()
            )
            .into());
        }
        if !resolved.starts_with(java) && !files.contains(&resolved) {
            files.push(resolved);
        }
    }
    Ok(files)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn distro_java_config_exposes_only_resolved_files() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mlab-jdk-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let java = root.join("java");
        std::fs::create_dir_all(java.join("conf/security")).unwrap();
        let external = root.join("java.security");
        std::fs::write(&external, "fixture").unwrap();
        std::fs::write(java.join("conf/net.properties"), "fixture").unwrap();
        std::os::unix::fs::symlink(&external, java.join("conf/security/java.security")).unwrap();
        assert_eq!(
            external_java_config_files(&java).unwrap(),
            vec![external.clone()]
        );
        std::fs::remove_file(external).unwrap();
        assert!(external_java_config_files(&java).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
