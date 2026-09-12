//! Experimental Seatbelt backend. No unsandboxed fallback is permitted.
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
        fs::remove_dir_all(root).unwrap();
    }
}
