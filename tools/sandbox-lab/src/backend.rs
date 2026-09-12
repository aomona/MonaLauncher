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
