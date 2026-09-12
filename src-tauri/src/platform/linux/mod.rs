//! Experimental Linux desktop backend; never falls back to an unrestricted process.
mod graphics;
pub(crate) mod narrator;
mod seccomp;
use crate::sandbox::{FileAccess, LinuxDisplayProtocol, SandboxPolicy};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::{fs::FileTypeExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};

/// Only the selected display service is exposed, alongside local PulseAudio.
pub struct Desktop {
    display: DisplayConnection,
    pulse: Option<PathBuf>,
}
#[derive(Debug)]
enum DisplayConnection {
    X11 {
        display: String,
        socket: PathBuf,
        authority: PathBuf,
    },
    Wayland {
        socket: PathBuf,
    },
}
impl Desktop {
    pub fn protocol(&self) -> LinuxDisplayProtocol {
        match self.display {
            DisplayConnection::X11 { .. } => LinuxDisplayProtocol::X11,
            DisplayConnection::Wayland { .. } => LinuxDisplayProtocol::Wayland,
        }
    }
    pub fn detect() -> io::Result<Self> {
        Self::detect_with_audio(true)
    }
    pub fn detect_with_audio(audio_output: bool) -> io::Result<Self> {
        if std::env::var_os("WAYLAND_SOCKET").is_some() {
            return Err(io::Error::other(
                "inherited WAYLAND_SOCKET is unsupported; use a named Wayland socket",
            ));
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let wayland = std::env::var_os("WAYLAND_DISPLAY");
        let session = std::env::var("XDG_SESSION_TYPE").ok();
        let display = if wayland.is_some() || session.as_deref() == Some("wayland") {
            let name = wayland.unwrap_or_else(|| "wayland-0".into());
            let socket = wayland_socket_path(Path::new(&name), runtime.as_deref())?;
            require_socket(&socket)?;
            DisplayConnection::Wayland {
                socket: fs::canonicalize(socket)?,
            }
        } else {
            let display = std::env::var("DISPLAY").map_err(|_| {
                io::Error::other("Linux desktop requires Wayland or local X11 DISPLAY")
            })?;
            let number = display_number(&display)?;
            let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
            require_socket(&socket)?;
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
            DisplayConnection::X11 {
                display,
                socket,
                authority: fs::canonicalize(authority)?,
            }
        };
        let pulse = if audio_output {
            let pulse = match std::env::var("PULSE_SERVER") {
                Ok(server) => PathBuf::from(server.strip_prefix("unix:").ok_or_else(|| {
                    io::Error::other("sandbox audio requires a local unix: PULSE_SERVER")
                })?),
                Err(_) => runtime
                    .ok_or_else(|| {
                        io::Error::other("XDG_RUNTIME_DIR is required for desktop audio")
                    })?
                    .join("pulse/native"),
            };
            require_socket(&pulse)?;
            Some(pulse)
        } else {
            None
        };
        Ok(Self { display, pulse })
    }
}
fn wayland_socket_path(name: &Path, runtime: Option<&Path>) -> io::Result<PathBuf> {
    if name.is_absolute() {
        return Ok(name.to_owned());
    }
    // Relative WAYLAND_DISPLAY is a socket name, never traversal into sibling services.
    if name.as_os_str().is_empty()
        || name.components().count() != 1
        || !matches!(
            name.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(io::Error::other(
            "WAYLAND_DISPLAY must be a socket name or absolute path",
        ));
    }
    let runtime = runtime.filter(|p| p.is_absolute()).ok_or_else(|| {
        io::Error::other("relative WAYLAND_DISPLAY requires absolute XDG_RUNTIME_DIR")
    })?;
    Ok(runtime.join(name))
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
        .compile_linux(
            desktop
                .map(Desktop::protocol)
                .unwrap_or(LinuxDisplayProtocol::X11),
        )
        .map_err(io::Error::other)?;
    let resources = policy.resources();
    let bwrap = Path::new("/usr/bin/bwrap");
    if !bwrap.is_file() {
        return Err(io::Error::other(
            "bubblewrap is required (/usr/bin/bwrap); unrestricted launch is disabled",
        ));
    }
    let filter_path = resources.launch.join("seccomp.bpf");
    fs::write(
        &filter_path,
        seccomp::program(policy.network == crate::sandbox::NetworkAccess::Internet)?,
    )?;
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
    if policy.network == crate::sandbox::NetworkAccess::Internet {
        command.arg("--share-net");
        for path in [
            "/etc/resolv.conf",
            "/etc/hosts",
            "/etc/nsswitch.conf",
            "/etc/ssl/certs",
        ] {
            if Path::new(path).exists() {
                command.args(["--ro-bind", path, path]);
            }
        }
    }
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
        "--perms",
        "0700",
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
        match &desktop.display {
            DisplayConnection::X11 {
                display,
                socket,
                authority,
            } => {
                command
                    .arg("--ro-bind")
                    .arg(socket)
                    .arg(socket)
                    .arg("--ro-bind")
                    .arg(authority)
                    .arg("/run/mona/Xauthority")
                    .env("DISPLAY", display)
                    .env("XAUTHORITY", "/run/mona/Xauthority")
                    .env("XDG_SESSION_TYPE", "x11");
            }
            DisplayConnection::Wayland { socket } => {
                command
                    .arg("--ro-bind")
                    .arg(socket)
                    .arg("/run/mona/wayland-0")
                    .env("WAYLAND_DISPLAY", "wayland-0")
                    .env("XDG_SESSION_TYPE", "wayland");
            }
        }
        if policy.desktop.audio_output {
            command
                .arg("--ro-bind")
                .arg(
                    desktop
                        .pulse
                        .as_ref()
                        .ok_or_else(|| io::Error::other("audio socket was not selected"))?,
                )
                .arg("/run/mona/pulse");
            let config = resources.launch.join("pulse-client.conf");
            fs::write(&config, "enable-shm=no\nautospawn=no\n")?;
            command
                .env("PULSE_SERVER", "unix:/run/mona/pulse")
                .env("PULSE_CLIENTCONFIG", config)
                .env("ALSOFT_DRIVERS", "pulse");
        } else {
            // No server socket, autospawn or ALSA device; keep ordinary audio separate from narration.
            command
                .env("PULSE_SERVER", "unix:/run/mona/audio-disabled")
                .env("ALSOFT_DRIVERS", "pulse");
        }
        let mut gpu_metadata = graphics::Metadata::default();
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
                    if desktop.protocol() == LinuxDisplayProtocol::Wayland {
                        gpu_metadata.expose(&mut command, &path)?;
                    }
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
    fn wayland_socket_resolution_rejects_traversal_and_missing_runtime() {
        let runtime = Some(Path::new("/run/user/1000"));
        assert_eq!(
            wayland_socket_path(Path::new("wayland-1"), runtime).unwrap(),
            Path::new("/run/user/1000/wayland-1")
        );
        assert_eq!(
            wayland_socket_path(Path::new("/custom/socket"), None).unwrap(),
            Path::new("/custom/socket")
        );
        for name in ["", ".", "..", "../bus", "sub/socket"] {
            assert!(wayland_socket_path(Path::new(name), runtime).is_err());
        }
        assert!(wayland_socket_path(Path::new("wayland-0"), None).is_err());
        assert!(wayland_socket_path(Path::new("wayland-0"), Some(Path::new("relative"))).is_err());
    }
    #[test]
    fn local_display_only() {
        assert_eq!(display_number(":0.1").unwrap(), "0");
        for value in ["tcp:0", "localhost:0", ":", ":0.", ":1.2.3", ":../../other"] {
            assert!(display_number(value).is_err());
        }
    }
    #[test]
    fn seccomp_has_complete_instructions() {
        let bytes = seccomp::program(false).unwrap();
        assert_eq!(bytes.len() % 8, 0);
        assert!(bytes.len() / 8 < 4096);
    }
}
