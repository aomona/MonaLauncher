use std::error::Error;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use monalauncher_lib::minecraft::installer::{detect_java_path, install_latest_demo_instance};
use monalauncher_lib::minecraft::launcher::{read_lines, spawn_instance};
use monalauncher_lib::minecraft::paths::MinecraftPaths;

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is not set")?
        .join("me.aomona.monalauncher")
        .join("minecraft");
    let paths = MinecraftPaths::new(root);
    let java_path = detect_java_path().ok_or("Java was not found")?;
    let instance_id = "smoke-demo";

    println!("Java: {}", java_path.display());
    println!("Data: {}", paths.root().display());

    install_latest_demo_instance(
        &paths,
        instance_id,
        "Minecraft Smoke Demo",
        &java_path,
        |progress| {
            println!(
                "[install:{}] {}/{} {}",
                progress.stage, progress.completed, progress.total, progress.message
            );
        },
    )?;

    println!("[launcher] starting Minecraft");
    let spawned = spawn_instance(&paths, instance_id)?;
    println!("[launcher] sandboxed={}", spawned.sandboxed);
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
            return Err(
                format!("Minecraft exited before the smoke window elapsed: {status}").into(),
            );
        }

        if Instant::now() >= deadline {
            println!("[launcher] Minecraft stayed alive for 30 seconds; stopping smoke test");
            child.kill()?;
            child.wait()?;
            stdout_thread.join().ok();
            stderr_thread.join().ok();
            println!("[launcher] smoke test passed");
            return Ok(());
        }

        thread::sleep(Duration::from_millis(250));
    }
}
