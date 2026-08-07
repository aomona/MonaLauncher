#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use monalauncher_lib::minecraft::installer::install_sandbox_demo_instance;
#[cfg(windows)]
use monalauncher_lib::minecraft::launcher::{read_lines, spawn_instance};
#[cfg(windows)]
use monalauncher_lib::minecraft::paths::MinecraftPaths;
#[cfg(windows)]
use monalauncher_lib::minecraft::runtime::install_java_8_runtime;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is unavailable")?
        .join("me.aomona.monalauncher")
        .join("minecraft");
    let paths = MinecraftPaths::new(root);
    let instance_id = "appcontainer-demo";
    let java = install_java_8_runtime(&paths, |progress| {
        println!("[runtime] {}", progress.message);
    })?;
    install_sandbox_demo_instance(
        &paths,
        instance_id,
        "AppContainer Demo",
        &java,
        "1.12.2",
        |progress| {
            if progress.completed == 0 || progress.completed == progress.total {
                println!(
                    "[install:{}] {}/{} {}",
                    progress.stage, progress.completed, progress.total, progress.message
                );
            }
        },
    )?;

    println!("[launcher] starting Minecraft in AppContainer");
    let spawned = spawn_instance(&paths, instance_id)?;
    if !spawned.sandboxed {
        return Err("launcher did not use AppContainer".into());
    }
    let mut child = spawned.child;
    let stdout_thread = thread::spawn(move || {
        read_lines(spawned.stdout, |line| println!("[stdout] {line}"));
    });
    let stderr_thread = thread::spawn(move || {
        read_lines(spawned.stderr, |line| eprintln!("[stderr] {line}"));
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            stdout_thread.join().ok();
            stderr_thread.join().ok();
            return Err(format!(
                "sandboxed Minecraft exited before the smoke window elapsed: {status}"
            )
            .into());
        }
        if Instant::now() >= deadline {
            println!("[launcher] sandboxed Minecraft stayed alive for 30 seconds");
            child.kill()?;
            child.wait()?;
            stdout_thread.join().ok();
            stderr_thread.join().ok();
            println!("[launcher] AppContainer Minecraft smoke test passed");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("AppContainer is only available on Windows");
    std::process::exit(1);
}
