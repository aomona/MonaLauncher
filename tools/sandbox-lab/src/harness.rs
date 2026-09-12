use crate::{backend, probe, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct Execution {
    name: String,
    sandboxed: bool,
    gui_profile: bool,
    passed: bool,
    exit_code: Option<i32>,
    exit_status: String,
    timed_out: bool,
    checks: BTreeMap<String, bool>,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    started_at_unix_seconds: u64,
    completed: bool,
    backend: String,
    os: String,
    architecture: String,
    host_version: String,
    java_version: String,
    passed: bool,
    executions: Vec<Execution>,
    not_tested: Vec<&'static str>,
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Fixture {
    fn create() -> Result<Self> {
        // Short path also fits macOS's sockaddr_un. create_dir is exclusive.
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = PathBuf::from("/tmp").join(format!("mlab-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        let fixture = Self(fs::canonicalize(&path)?);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700))?;
        }
        for dir in [
            "runtime/classes",
            "shared",
            "instance",
            "game/home",
            "host",
            "other-instance",
        ] {
            fs::create_dir_all(fixture.0.join(dir))?;
        }
        for file in [
            "shared/asset",
            "instance/manifest",
            "host/secret",
            "other-instance/manifest",
        ] {
            fs::write(fixture.0.join(file), b"sandbox-lab fixture")?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(fixture.0.join("host"), fixture.0.join("game/escape"))?;
        fs::copy(std::env::current_exe()?, fixture.0.join("runtime/probe"))?;
        Ok(fixture)
    }
}

struct Services {
    tcp: TcpListener,
    udp_port: u16,
    #[cfg(unix)]
    unix: std::os::unix::net::UnixListener,
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl Services {
    fn start(root: &Path) -> Result<Self> {
        let tcp = TcpListener::bind("127.0.0.1:0")?;
        tcp.set_nonblocking(true)?;
        let udp = UdpSocket::bind("127.0.0.1:0")?;
        udp.set_read_timeout(Some(Duration::from_millis(100)))?;
        let udp_port = udp.local_addr()?.port();
        #[cfg(unix)]
        let unix = std::os::unix::net::UnixListener::bind(root.join("host/service.sock"))?;
        #[cfg(unix)]
        unix.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_udp = Arc::clone(&stop);
        let echo = thread::spawn(move || {
            let mut bytes = [0; 32];
            while !stop_udp.load(Ordering::Relaxed) {
                if let Ok((size, peer)) = udp.recv_from(&mut bytes) {
                    let _ = udp.send_to(&bytes[..size], peer);
                }
            }
        });
        let listener = tcp.try_clone()?;
        let stop_tcp = Arc::clone(&stop);
        let accept = thread::spawn(move || {
            while !stop_tcp.load(Ordering::Relaxed) {
                let _ = listener.accept();
                thread::sleep(Duration::from_millis(10));
            }
        });
        Ok(Self {
            tcp,
            udp_port,
            #[cfg(unix)]
            unix,
            stop,
            workers: vec![echo, accept],
        })
    }
    fn args(&self, root: &Path) -> Result<Vec<String>> {
        Ok(vec![
            root.to_string_lossy().into_owned(),
            self.tcp.local_addr()?.port().to_string(),
            self.udp_port.to_string(),
            "parent".into(),
        ])
    }
    fn drain_unix(&self) {
        #[cfg(unix)]
        while self.unix.accept().is_ok() {}
    }
}

impl Drop for Services {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let mut java_home = None;
    let mut report_path = None;
    let mut lwjgl = None;
    if !args.len().is_multiple_of(2) {
        return Err("options require a value; see --help".into());
    }
    for pair in args.chunks_exact(2) {
        let slot = match pair[0].as_str() {
            "--java-home" => &mut java_home,
            "--report" => &mut report_path,
            "--lwjgl" => &mut lwjgl,
            _ => return Err(format!("unknown option: {}", pair[0]).into()),
        };
        if slot.replace(PathBuf::from(&pair[1])).is_some() {
            return Err("duplicate option".into());
        }
    }
    let report_path = report_path.ok_or("--report is required")?;
    let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // Invalidate any earlier PASS before setup, including on an interrupted run.
    write_report(
        &report_path,
        &serde_json::json!({
            "schema_version": 1, "started_at_unix_seconds": started_at,
            "completed": false, "passed": false, "phase": "setup_or_execution"
        }),
    )?;
    let backend = backend::name()?;
    let java_home = fs::canonicalize(java_home.ok_or("--java-home is required")?)?;
    let fixture = Fixture::create()?;
    let root = &fixture.0;
    let java = java_home.join("bin/java");
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("java");
    let classes = root.join("runtime/classes");
    let mut compile = clean(Command::new(java_home.join("bin/javac")), root);
    compile
        .args(["--release", "17", "-d"])
        .arg(&classes)
        .arg(source_root.join("SandboxProbe.java"));
    let compilation = execute("compile-java", false, false, compile)?;
    if compilation.exit_code != Some(0) || compilation.timed_out {
        return Err(format!("javac failed: {}", compilation.stderr).into());
    }
    if let Some(jars) = lwjgl.as_ref() {
        prepare_lwjgl(root, &java_home, &source_root, jars)?;
    }
    let mut version = clean(Command::new(&java), root);
    version.arg("-version");
    let version = execute("java-version", false, false, version)?;
    if version.exit_code != Some(0) {
        return Err(format!("java -version failed: {}", version.stderr).into());
    }
    let mut report = Report {
        schema_version: 1,
        started_at_unix_seconds: started_at,
        completed: true,
        backend: backend.into(),
        os: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        host_version: String::from_utf8_lossy(&Command::new("uname").arg("-srv").output()?.stdout)
            .trim()
            .into(),
        java_version: version.stderr.trim().into(),
        passed: false,
        executions: vec![],
        not_tested: vec![
            "public Internet / IPv6 / DNS",
            "network or file access mediated by host Mach services",
            "hostile inherited descriptors",
            "hard links and concurrent path replacement",
            "process-tree cleanup on launcher crash",
            "memory / CPU / process limits",
            "Minecraft / mods",
            "audio / keyboard / mouse",
            "Linux seccomp / Wayland / X11 cross-client access / desktop service mediation",
            "Windows AppContainer in this run",
        ],
    };
    let services = Services::start(root)?;
    let probe_args = services.args(root)?;
    for gui in if lwjgl.is_some() {
        vec![false, true]
    } else {
        vec![false]
    } {
        for jvm in [false, true] {
            for isolated in [false, true] {
                services.drain_unix();
                let program = if jvm {
                    java.clone()
                } else {
                    root.join("runtime/probe")
                };
                let cmd = if isolated {
                    backend::command(root, &java_home, &program, gui)?
                } else {
                    Command::new(&program)
                };
                let mut cmd = clean(cmd, root);
                if jvm {
                    cmd.arg("-XX:-UsePerfData")
                        .arg(format!("-Djava.io.tmpdir={}", root.join("game").display()))
                        .arg("-cp")
                        .arg(&classes)
                        .arg("SandboxProbe");
                } else {
                    cmd.arg("--probe");
                }
                cmd.args(&probe_args);
                let mut result = execute(if jvm { "java" } else { "native" }, isolated, gui, cmd)?;
                result.passed = result.exit_code == Some(0)
                    && !result.timed_out
                    && probe::matches(&result.checks, isolated);
                eprintln!(
                    "{} {} gui={gui}: {}",
                    result.name,
                    if isolated { backend } else { "control" },
                    if result.passed { "PASS" } else { "FAIL" }
                );
                report.executions.push(result);
            }
        }
    }
    if lwjgl.is_some() {
        for isolated in [false, true] {
            let cmd = if isolated {
                backend::command(root, &java_home, &java, true)?
            } else {
                Command::new(&java)
            };
            let mut cmd = clean(cmd, root);
            if cfg!(target_os = "macos") {
                cmd.arg("-XstartOnFirstThread");
            }
            backend::gui_environment(&mut cmd)?;
            cmd.arg("-XX:-UsePerfData")
                .arg(format!("-Djava.io.tmpdir={}", root.join("game").display()))
                .arg(format!(
                    "-Dorg.lwjgl.system.SharedLibraryExtractPath={}",
                    root.join("game/natives").display()
                ))
                .arg("-cp")
                .arg(lwjgl_classpath(root)?)
                .arg("LwjglProbe");
            let mut result = execute("lwjgl", isolated, true, cmd)?;
            result.passed = result.exit_code == Some(0)
                && !result.timed_out
                && result.checks.len() == 2
                && ["glfw_window", "opengl_readback"]
                    .iter()
                    .all(|key| result.checks.get(*key) == Some(&true));
            eprintln!(
                "lwjgl isolated={isolated}: {}",
                if result.passed { "PASS" } else { "FAIL" }
            );
            report.executions.push(result);
        }
    } else {
        report.not_tested.push("LWJGL / GPU / window creation");
    }
    report.passed = report.executions.iter().all(|e| e.passed);
    write_report(&report_path, &report)?;
    eprintln!("Report: {}", report_path.display());
    if !report.passed {
        return Err("one or more probes failed; inspect the report".into());
    }
    Ok(())
}

fn write_report(path: &Path, report: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}

fn clean(mut cmd: Command, root: &Path) -> Command {
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "en_US.UTF-8")
        .env("HOME", root.join("game/home"))
        .env("TMPDIR", root.join("game"))
        .current_dir(root.join("game"));
    cmd
}

fn execute(name: &str, sandboxed: bool, gui: bool, mut cmd: Command) -> Result<Execution> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let out = thread::spawn(move || {
        let mut bytes = vec![];
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = thread::spawn(move || {
        let mut bytes = vec![];
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if start.elapsed() > Duration::from_secs(40) {
            // Only the harness-owned process group is targeted. This is a lab
            // timeout, not evidence of production descendant containment.
            #[cfg(unix)]
            Command::new("/bin/kill")
                .arg("-KILL")
                .arg("--")
                .arg(format!("-{}", child.id()))
                .status()?;
            let _ = child.kill();
            break (child.wait()?, true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = out.join().map_err(|_| "stdout reader panicked")??;
    let stderr = err.join().map_err(|_| "stderr reader panicked")??;
    let checks = probe::parse(&stdout)?;
    Ok(Execution {
        name: name.into(),
        sandboxed,
        gui_profile: gui,
        passed: false,
        exit_code: status.code(),
        exit_status: status.to_string(),
        timed_out,
        checks,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

fn lwjgl_classpath(root: &Path) -> Result<std::ffi::OsString> {
    let mut paths = vec![root.join("runtime/classes")];
    let mut jars = fs::read_dir(root.join("runtime/lwjgl"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    jars.sort();
    paths.extend(jars);
    Ok(std::env::join_paths(paths)?)
}

fn prepare_lwjgl(root: &Path, java_home: &Path, sources: &Path, jars: &Path) -> Result<()> {
    if !matches!(std::env::consts::OS, "macos" | "linux") {
        return Err("LWJGL lab supports macOS and Linux only".into());
    }
    fs::create_dir(root.join("runtime/lwjgl"))?;
    for entry in fs::read_dir(jars)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "jar") {
            fs::copy(
                &path,
                root.join("runtime/lwjgl")
                    .join(path.file_name().ok_or("jar name missing")?),
            )?;
        }
    }
    let mut cmd = clean(Command::new(java_home.join("bin/javac")), root);
    cmd.args(["--release", "17", "-cp"])
        .arg(lwjgl_classpath(root)?)
        .arg("-d")
        .arg(root.join("runtime/classes"))
        .arg(sources.join("LwjglProbe.java"));
    let result = execute("compile-lwjgl", false, false, cmd)?;
    if result.exit_code != Some(0) || result.timed_out {
        return Err(format!("LWJGL compilation failed: {}", result.stderr).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_setup_invalidates_an_earlier_success() {
        let fixture = Fixture::create().unwrap();
        let report = fixture.0.join("old-report.json");
        fs::write(&report, r#"{"passed":true,"completed":true}"#).unwrap();
        assert!(run(&[
            "--java-home".into(),
            fixture.0.join("missing-jdk").display().to_string(),
            "--report".into(),
            report.display().to_string(),
        ])
        .is_err());
        let result: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(result["passed"], false);
        assert_eq!(result["completed"], false);
    }
}
