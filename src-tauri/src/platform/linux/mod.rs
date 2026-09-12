//! Experimental Linux desktop backend; never falls back to an unrestricted process.
pub(crate) mod narrator;
mod seccomp;
use crate::sandbox::{Backend, FileAccess, SandboxPolicy};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::{fs::FileTypeExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};

/// X11 (including XWayland) and a local PulseAudio-compatible server.
/// These services are compatibility grants, not an output-only security boundary.
pub struct Desktop {
    display: String,
    x_socket: PathBuf,
    authority: PathBuf,
    pulse: PathBuf,
}
impl Desktop {
    pub fn detect() -> io::Result<Self> {
        let display = std::env::var("DISPLAY").map_err(|_| {
            io::Error::other("Linux desktop sandbox requires local X11/XWayland DISPLAY")
        })?;
        let number = display_number(&display)?;
        let x_socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        require_socket(&x_socket)?;
        let authority = std::env::var_os("XAUTHORITY")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Xauthority"))
            })
            .ok_or_else(|| io::Error::other("XAUTHORITY is required"))?;
        if !authority.is_absolute() || !authority.is_file() {
            return Err(io::Error::other(
                "XAUTHORITY must name an absolute regular file",
            ));
        }
        let pulse = match std::env::var("PULSE_SERVER") {
            Ok(server) => PathBuf::from(server.strip_prefix("unix:").ok_or_else(|| {
                io::Error::other("sandbox audio requires a local unix: PULSE_SERVER")
            })?),
            Err(_) => PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
                io::Error::other("XDG_RUNTIME_DIR is required for desktop audio")
            })?)
            .join("pulse/native"),
        };
        require_socket(&pulse)?;
        Ok(Self {
            display,
            x_socket,
            authority: fs::canonicalize(authority)?,
            pulse,
        })
    }
}
fn require_socket(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || !fs::metadata(path)?.file_type().is_socket() {
        return Err(io::Error::other(
            "desktop connection must be an absolute Unix socket",
        ));
    }
    Ok(())
}
fn display_number(display: &str) -> io::Result<&str> {
    let local = display
        .strip_prefix(':')
        .ok_or_else(|| io::Error::other("remote X11 is unsupported"))?;
    let mut parts = local.split('.');
    let number = parts.next().unwrap_or_default();
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !numeric(number) || parts.next().is_some_and(|s| !numeric(s)) || parts.next().is_some() {
        return Err(io::Error::other("invalid local X11 display"));
    }
    Ok(number)
}

pub struct PreparedCommand {
    command: Command,
    // Kept open until spawn; the child's pre_exec clears CLOEXEC on this FD only.
    _filter: File,
}
impl PreparedCommand {
    pub fn output(&mut self, arguments: &[OsString]) -> io::Result<Output> {
        self.command.args(arguments).output()
    }
    pub fn spawn(
        mut self,
        arguments: &[OsString],
        launch_root: PathBuf,
    ) -> io::Result<BubblewrapProcess> {
        self.command.args(arguments);
        let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
        let (lifetime, until_drop) = std::sync::mpsc::channel::<()>();
        // PR_SET_PDEATHSIG watches the spawning Linux thread, not just its process.
        // A Tokio blocking-pool thread may retire while the game is still running.
        let spawn_thread = std::thread::Builder::new()
            .name("minecraft-linux-parent".into())
            .spawn(move || {
                let _filter = self._filter;
                let result = self.command.spawn();
                let started = result.is_ok();
                if let Err(error) = result_sender.send(result) {
                    if let Ok(mut child) = error.0 {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    return;
                }
                if started {
                    let _ = until_drop.recv();
                }
            })?;
        let child = result_receiver.recv().map_err(io::Error::other)??;
        Ok(BubblewrapProcess {
            child,
            launch_root,
            lifetime: Some(lifetime),
            spawn_thread: Some(spawn_thread),
        })
    }
}

pub fn prepare(
    program: &Path,
    policy: &SandboxPolicy,
    desktop: Option<&Desktop>,
) -> io::Result<PreparedCommand> {
    let plan = policy
        .compile(Backend::Bubblewrap)
        .map_err(io::Error::other)?;
    let resources = policy.resources();
    let bwrap = Path::new("/usr/bin/bwrap");
    if !bwrap.is_file() {
        return Err(io::Error::other(
            "bubblewrap is required (/usr/bin/bwrap); unrestricted launch is disabled",
        ));
    }
    let filter_path = resources.launch.join("seccomp.bpf");
    fs::write(&filter_path, seccomp::program()?)?;
    let filter = File::open(filter_path)?;
    let fd = filter.as_raw_fd();
    let mut command = Command::new(bwrap);
    command.args([
        "--unshare-all",
        "--unshare-user",
        "--die-with-parent",
        "--new-session",
        "--disable-userns",
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
        "/etc/fonts",
        "/etc/localtime",
    ] {
        if Path::new(path).exists() {
            command.args(["--ro-bind", path, path]);
        }
    }
    command.args([
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/run/mona",
    ]);
    for grant in &plan.files {
        command
            .arg(if grant.access == FileAccess::ReadOnly {
                "--ro-bind"
            } else {
                "--bind"
            })
            .arg(&grant.path)
            .arg(&grant.path);
    }
    // Managed Adoptium runtimes are self-contained. Do not expose arbitrary external
    // symlink targets from a JDK: missing resources must fail rather than widen access.
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &resources.game)
        .env("TMPDIR", &resources.temp)
        .env("XDG_RUNTIME_DIR", "/run/mona")
        .env("LANG", "C.UTF-8");
    if let Some(desktop) = desktop {
        command
            .arg("--ro-bind")
            .arg(&desktop.x_socket)
            .arg(&desktop.x_socket)
            .arg("--ro-bind")
            .arg(&desktop.authority)
            .arg("/run/mona/Xauthority")
            .arg("--ro-bind")
            .arg(&desktop.pulse)
            .arg("/run/mona/pulse");
        let config = resources.launch.join("pulse-client.conf");
        fs::write(&config, "enable-shm=no\nautospawn=no\n")?;
        command
            .env("DISPLAY", &desktop.display)
            .env("XAUTHORITY", "/run/mona/Xauthority")
            .env("PULSE_SERVER", "unix:/run/mona/pulse")
            .env("PULSE_CLIENTCONFIG", config)
            .env("ALSOFT_DRIVERS", "pulse");
        if let Ok(entries) = fs::read_dir("/dev/dri") {
            for entry in entries {
                let path = entry?.path();
                let render = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_prefix("renderD"))
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                if render && fs::symlink_metadata(&path)?.file_type().is_char_device() {
                    command.arg("--dev-bind").arg(&path).arg(&path);
                }
            }
        }
        // A narrow diagnostic override for the known UTM software-rendering setup.
        if std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref() == Ok("1") {
            command.env("LIBGL_ALWAYS_SOFTWARE", "1");
        }
    }
    command
        .arg("--seccomp")
        .arg(fd.to_string())
        .arg("--chdir")
        .arg(&resources.game)
        .arg("--")
        .arg(program)
        .current_dir(&resources.game)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: only the async-signal-safe fcntl syscall runs between fork and exec.
    // The captured FD belongs to _filter and remains open through spawn.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(PreparedCommand {
        command,
        _filter: filter,
    })
}

pub struct BubblewrapProcess {
    child: Child,
    launch_root: PathBuf,
    lifetime: Option<std::sync::mpsc::Sender<()>>,
    spawn_thread: Option<std::thread::JoinHandle<()>>,
}
impl BubblewrapProcess {
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
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        // Killing the namespace supervisor tears down its PID namespace. The owned
        // Child handle avoids signalling a reaped/reused numeric PID.
        self.child.kill()
    }
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }
}
impl Drop for BubblewrapProcess {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
        self.lifetime.take();
        if let Some(thread) = self.spawn_thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_dir_all(&self.launch_root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_display_only() {
        assert_eq!(display_number(":0.1").unwrap(), "0");
        for value in ["tcp:0", "localhost:0", ":", ":0.", ":1.2.3", ":../../other"] {
            assert!(display_number(value).is_err());
        }
    }
    #[test]
    fn seccomp_has_complete_instructions() {
        let bytes = seccomp::program().unwrap();
        assert_eq!(bytes.len() % 8, 0);
        assert!(bytes.len() / 8 < 4096);
    }
}
