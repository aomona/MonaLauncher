//! Experimental Seatbelt backend. No unsandboxed fallback is permitted.
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

pub const PROFILE: &str = include_str!("minecraft.sb");

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

pub fn command(
    java: &Path,
    grants: &[(&str, PathBuf)],
    game: &Path,
    temp: &Path,
) -> io::Result<Command> {
    let mut command = Command::new("/usr/bin/sandbox-exec");
    for (key, path) in grants {
        let path = fs::canonicalize(path)?;
        let value = path
            .to_str()
            .ok_or_else(|| io::Error::other("Seatbelt requires UTF-8 paths"))?;
        command.arg("-D").arg(format!("{key}={value}"));
    }
    command.arg("-p").arg(PROFILE).arg(java);
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
        for name in [
            "launch",
            "libraries",
            "assets",
            "version",
            "game",
            "tmp",
            "other",
        ] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("libraries/shared"), "shared").unwrap();
        fs::write(root.join("other/secret"), "fixture\n").unwrap();
        std::os::unix::fs::symlink(root.join("other/secret"), root.join("game/escape")).unwrap();
        let grants = [
            ("JAVA_HOME", PathBuf::from("/bin")),
            ("LAUNCH", root.join("launch")),
            ("LIBRARIES", root.join("libraries")),
            ("ASSETS", root.join("assets")),
            ("VERSION", root.join("version")),
            ("GAME", root.join("game")),
            ("TEMP", root.join("tmp")),
        ];
        let mut command = command(
            Path::new("/bin/sh"),
            &grants,
            &root.join("game"),
            &root.join("tmp"),
        )
        .unwrap();
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
        let output = command
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
        fs::remove_dir_all(root).unwrap();
    }
}
