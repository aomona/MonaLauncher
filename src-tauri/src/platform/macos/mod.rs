//! Experimental Seatbelt backend. No unsandboxed fallback is permitted.
mod graphics_cache;
pub(crate) mod narrator;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::sandbox::{Backend, Resource, SandboxPolicy};

pub struct SeatbeltProcess {
    child: Child,
    launch_root: PathBuf,
}

impl SeatbeltProcess {
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
    pub fn kill(&mut self) -> io::Result<()> {
        // The child owns a fresh process group. Do not signal a possibly reused ID after reaping.
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        // SAFETY: kill accepts a numeric process group ID; process_group(0) creates this group.
        let result = unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }
}

impl Drop for SeatbeltProcess {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
        let _ = fs::remove_dir_all(&self.launch_root);
    }
}

pub fn command(java: &Path, policy: &SandboxPolicy) -> io::Result<Command> {
    policy
        .compile(Backend::Seatbelt)
        .map_err(io::Error::other)?;
    let profile = crate::sandbox::seatbelt::render(policy).map_err(io::Error::other)?;
    let game = policy.resources().path(Resource::Game);
    let temp = policy.resources().path(Resource::Temp);
    let mut command = Command::new("/usr/bin/sandbox-exec");
    for (role, _) in policy.requested_files() {
        let key = role.parameter();
        let path = policy.resources().path(*role);
        let value = path
            .to_str()
            .ok_or_else(|| io::Error::other("Seatbelt requires UTF-8 paths"))?;
        command.arg("-D").arg(format!("{key}={value}"));
    }
    for (index, path) in policy.readonly_game_directories().iter().enumerate() {
        let value = path
            .to_str()
            .ok_or_else(|| io::Error::other("Seatbelt requires UTF-8 paths"))?;
        command
            .arg("-D")
            .arg(format!("GAME_READONLY_{index}={value}"));
    }
    if let Some(caches) = &policy.caches {
        for (index, grant) in caches
            .grants(policy.skin_cache, policy.graphics_cache)
            .iter()
            .enumerate()
        {
            let value = grant
                .path
                .to_str()
                .ok_or_else(|| io::Error::other("Seatbelt requires UTF-8 cache paths"))?;
            command.arg("-D").arg(format!("CACHE_{index}={value}"));
        }
    }
    if policy.graphics_cache {
        let cache = graphics_cache::java_metal_cache()?;
        let value = cache
            .to_str()
            .ok_or_else(|| io::Error::other("Seatbelt requires UTF-8 cache paths"))?;
        command.arg("-D").arg(format!("JAVA_METAL_CACHE={value}"));
    }
    command.arg("-p").arg(profile).arg(java);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", game)
        .env("TMPDIR", temp)
        .env("LANG", "en_US.UTF-8")
        .current_dir(game)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    Ok(command)
}

pub fn spawn(
    mut command: Command,
    arguments: &[OsString],
    launch_root: PathBuf,
) -> io::Result<SeatbeltProcess> {
    let child = command.args(arguments).spawn()?;
    Ok(SeatbeltProcess { child, launch_root })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_and_desktop_service_permissions_are_enforced() {
        let root =
            std::env::temp_dir().join(format!("mona-service-permissions-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        for path in [
            "java",
            "versions/one",
            "libraries",
            "assets",
            "game",
            "launch/tmp",
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        let source = root.join("java/probe.c");
        let executable = root.join("java/probe");
        fs::write(&source, include_str!("permissions_probe.c")).unwrap();
        assert!(Command::new("/usr/bin/clang")
            .arg(&source)
            .args(["-lsandbox", "-o"])
            .arg(&executable)
            .status()
            .unwrap()
            .success());
        let mut policy = SandboxPolicy::minecraft(crate::sandbox::SandboxResources {
            data_root: root.clone(),
            runtimes_root: root.clone(),
            versions_root: root.join("versions"),
            instance_root: root.clone(),
            java_home: root.join("java"),
            libraries: root.join("libraries"),
            assets: root.join("assets"),
            version: root.join("versions/one"),
            game: root.join("game"),
            launch: root.join("launch"),
            temp: root.join("launch/tmp"),
        })
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        for enabled in [false, true] {
            policy.network = if enabled {
                crate::sandbox::NetworkAccess::Internet
            } else {
                crate::sandbox::NetworkAccess::Denied
            };
            policy.desktop.audio_output = enabled;
            policy.desktop.microphone = enabled;
            policy.desktop.clipboard = enabled;
            policy.desktop.integration = enabled;
            policy.graphics_cache = enabled;
            let output = command(&executable, &policy)
                .unwrap()
                .arg(listener.local_addr().unwrap().port().to_string())
                .arg(graphics_cache::java_metal_cache().unwrap())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            for key in [
                "network",
                "audio",
                "clipboard",
                "fullscreen",
                "graphics_cache",
                "microphone_policy",
            ] {
                assert_eq!(result[key], i32::from(enabled), "{key}: {result}");
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn seatbelt_confines_files_and_inherits_into_children() {
        let root = std::env::temp_dir().join(format!("mona-seatbelt-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        for name in ["launch", "libraries", "assets", "version", "game", "other"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::create_dir(root.join("launch/tmp")).unwrap();
        fs::write(root.join("libraries/shared"), "shared").unwrap();
        fs::write(root.join("other/secret"), "fixture\n").unwrap();
        std::os::unix::fs::symlink(root.join("other/secret"), root.join("game/escape")).unwrap();
        let policy = crate::sandbox::SandboxPolicy::minecraft(crate::sandbox::SandboxResources {
            data_root: root.clone(),
            runtimes_root: PathBuf::from("/bin"),
            versions_root: root.join("version"),
            instance_root: root.clone(),
            java_home: PathBuf::from("/bin"),
            libraries: root.join("libraries"),
            assets: root.join("assets"),
            version: root.join("version"),
            game: root.join("game"),
            launch: root.join("launch"),
            temp: root.join("launch/tmp"),
        })
        .unwrap();
        let mut sandbox_command = command(Path::new("/bin/sh"), &policy).unwrap();
        // All paths are positional parameters, never interpolated into shell source.
        let script = r#"
            set -eu
            read value < "$1/libraries/shared" || test "$value" = shared
            printf allowed > "$1/game/allowed"
            if (printf denied > "$1/libraries/new"); then exit 10; fi
            if (read value < "$1/other/secret"); then exit 11; fi
            if (read value < "$1/game/escape"); then exit 12; fi
            /bin/sh -c 'if (printf denied > "$1/other/new"); then exit 13; fi' child "$1"
        "#;
        let output = sandbox_command
            .args(["-c", script, "probe"])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(root.join("game/allowed")).unwrap(), b"allowed");
        assert!(!root.join("libraries/new").exists());
        assert!(!root.join("other/new").exists());
        let read_only = policy
            .clone()
            .with_file_access(Resource::Game, crate::sandbox::FileAccess::ReadOnly)
            .unwrap();
        let output = command(Path::new("/bin/sh"), &read_only)
            .unwrap()
            .args([
                "-c",
                "if (printf denied > \"$1/game/blocked\"); then exit 1; fi",
                "probe",
            ])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "read-only game policy was not enforced"
        );
        assert!(!root.join("game/blocked").exists());
        fs::remove_file(root.join("game/escape")).unwrap();
        let granular = policy
            .with_readonly_game_directories(&[crate::sandbox::GameDirectory::Worlds])
            .unwrap();
        fs::write(root.join("game/saves/existing"), "protected").unwrap();
        let output = command(Path::new("/bin/sh"), &granular)
            .unwrap()
            .args([
                "-c",
                r#"
            set -eu
            printf allowed > "$1/game/other"
            if (printf denied > "$1/game/saves/new"); then exit 20; fi
            if (printf denied > "$1/game/saves/existing"); then exit 21; fi
            if /bin/mv "$1/game/saves" "$1/game/renamed"; then exit 22; fi
        "#,
                "probe",
            ])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(root.join("game/saves/existing")).unwrap(),
            b"protected"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
