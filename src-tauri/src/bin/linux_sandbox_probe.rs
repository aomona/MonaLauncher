//! Exercises the production bubblewrap command using controlled accessible/denied resources.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use monalauncher_lib::{
        probe::{prepare, Desktop},
        sandbox::*,
    };
    use std::{ffi::OsString, fs, net::TcpListener};
    let desktop = match std::env::args().nth(1).as_deref() {
        None => None,
        Some("--wayland") => {
            let desktop = Desktop::detect()?;
            if desktop.protocol() != LinuxDisplayProtocol::Wayland {
                return Err("Wayland was required for this probe".into());
            }
            Some(desktop)
        }
        _ => return Err("usage: linux_sandbox_probe [--wayland]".into()),
    };
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
    let root = std::env::temp_dir().join(format!("mona-linux-policy-{:x?}", nonce));
    for path in [
        "runtimes/java",
        "versions/one",
        "libraries",
        "assets",
        "instances/one/game",
        "instances/one/launch/tmp",
        "instances/two",
        "host",
    ] {
        fs::create_dir_all(root.join(path))?;
    }
    let root = fs::canonicalize(root)?;
    let resources = SandboxResources {
        data_root: root.clone(),
        runtimes_root: root.join("runtimes"),
        java_home: root.join("runtimes/java"),
        versions_root: root.join("versions"),
        version: root.join("versions/one"),
        libraries: root.join("libraries"),
        assets: root.join("assets"),
        instance_root: root.join("instances/one"),
        game: root.join("instances/one/game"),
        launch: root.join("instances/one/launch"),
        temp: root.join("instances/one/launch/tmp"),
    };
    fs::write(root.join("host/secret"), "host fixture")?;
    fs::write(root.join("instances/two/secret"), "other instance")?;
    fs::write(resources.libraries.join("shared"), "shared fixture")?;
    std::os::unix::fs::symlink(root.join("host/secret"), resources.game.join("escape"))?;
    let tcp = TcpListener::bind("127.0.0.1:0")?;
    let socket = std::os::unix::net::UnixListener::bind(root.join("host/service"))?;
    let script = resources.libraries.join("probe.py");
    fs::write(&script, include_str!("linux_sandbox_probe.py"))?;
    let mut args = vec![
        script.as_os_str().to_owned(),
        root.as_os_str().to_owned(),
        OsString::from(tcp.local_addr()?.port().to_string()),
    ];
    if desktop.is_some() {
        args.push(OsString::from("wayland"));
    }
    let baseline = std::process::Command::new("/usr/bin/python3")
        .args(&args)
        .output()?;
    if !baseline.status.success() {
        return Err("unsandboxed control failed".into());
    }
    let control: serde_json::Value = serde_json::from_slice(&baseline.stdout)?;
    for check in [
        "host_read",
        "other_read",
        "symlink_read",
        "shared_write",
        "game_write",
        "child_host_read",
        "tcp",
        "host_socket",
    ] {
        if control[check] != true {
            return Err(format!("control {check} failed: {control}").into());
        }
    }
    let policy = SandboxPolicy::minecraft(resources.clone())?;
    for writable in [true, false] {
        let policy = policy.clone().with_file_access(
            Resource::Game,
            if writable {
                FileAccess::ReadWrite
            } else {
                FileAccess::ReadOnly
            },
        )?;
        let output = prepare(
            std::path::Path::new("/usr/bin/python3"),
            &policy,
            desktop.as_ref(),
        )?
        .output(&args)?;
        if !output.status.success() {
            return Err(format!(
                "sandbox failed to start: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if desktop.is_some() {
            if control["wayland_connect"] != true || result["wayland_connect"] != true {
                return Err("Wayland connection failed in control or sandbox".into());
            }
            for check in ["x11_path", "x11_abstract", "session_bus", "sys_network"] {
                if control[check] != true {
                    return Err(format!("desktop positive control {check} failed").into());
                }
            }
            for check in [
                "x11_path",
                "x11_abstract",
                "display_env",
                "xauthority_env",
                "session_bus",
                "sys_network",
                "drm_primary",
                "input_devices",
                "gpu_config",
            ] {
                if result[check] != false {
                    return Err(format!("unexpected desktop access {check}: {result}").into());
                }
            }
        }
        if desktop.is_some()
            && control["gpu_vendor_read"] == true
            && (result["gpu_vendor_read"] != true || result["gpu_vendor_readonly_mount"] != true)
        {
            return Err("GPU identification must remain readable through read-only mounts".into());
        }
        for check in [
            "host_read",
            "other_read",
            "symlink_read",
            "shared_write",
            "child_host_read",
            "tcp",
            "host_socket",
            "inet_socket",
            "inet6_socket",
            "unshare",
            "ptrace",
        ] {
            if result[check] != false {
                return Err(format!("unexpected grant {check}: {result}").into());
            }
        }
        for check in [
            "shared_read",
            "temp_write",
            "unix_socket",
            "seccomp",
            "no_new_privs",
            "clone3_fallback",
        ] {
            if result[check] != true {
                return Err(format!("required check {check} failed: {result}").into());
            }
        }
        if result["game_write"] != writable {
            return Err(format!("game write mismatch: {result}").into());
        }
        println!("POLICY PASS game_write={writable}: {result}");
    }
    // A detached descendant must die with the owned namespace supervisor.
    let args = vec![OsString::from("-c"), OsString::from(
        "import os,time; p=os.fork(); os.setsid() if p==0 else None; print('ready',flush=True); time.sleep(120)"
    )];
    let prepared = prepare(std::path::Path::new("/usr/bin/python3"), &policy, None)?;
    let launch = resources.launch.clone();
    let mut child = std::thread::spawn(move || prepared.spawn(&args, launch))
        .join()
        .map_err(|_| "caller thread panicked")??;
    let stdout = child.take_stdout().ok_or("stdout missing")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stdout).lines() {
            if line.is_ok() {
                let _ = tx.send(false);
            }
        }
        let _ = tx.send(true);
    });
    if rx.recv_timeout(std::time::Duration::from_secs(5))? {
        return Err("sandbox died when its caller thread exited".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    if child.try_wait()?.is_some() {
        return Err("sandbox did not survive its caller thread".into());
    }
    println!("PROCESS PASS: sandbox survived its short-lived caller thread");
    child.kill()?;
    child.wait()?;
    loop {
        if rx.recv_timeout(std::time::Duration::from_secs(5))? {
            break;
        }
    }
    println!("PROCESS PASS: detached descendants closed their pipes after supervisor termination");
    drop(child);
    drop(socket);
    drop(tcp);
    fs::remove_dir_all(root)?;
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux_sandbox_probe requires Linux");
    std::process::exit(1);
}
