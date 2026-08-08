#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use monalauncher_lib::minecraft::installer::install_sandbox_instance;
#[cfg(windows)]
use monalauncher_lib::minecraft::launcher::{read_lines, spawn_instance};
#[cfg(windows)]
use monalauncher_lib::minecraft::paths::MinecraftPaths;
#[cfg(windows)]
use monalauncher_lib::minecraft::runtime::install_java_25_runtime;
#[cfg(windows)]
use monalauncher_lib::probe::NarratorBroker;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is unavailable")?
        .join("me.aomona.monalauncher")
        .join("minecraft");
    let paths = MinecraftPaths::new(root);
    let demo = std::env::var_os("MONALAUNCHER_OFFLINE").is_none();
    let (instance_id, instance_name, mode_name) = if demo {
        (
            "appcontainer-latest-demo",
            "Latest AppContainer Demo",
            "demo",
        )
    } else {
        (
            "appcontainer-latest-offline",
            "Latest AppContainer Offline",
            "offline",
        )
    };
    if std::env::var_os("MONALAUNCHER_SKIP_INSTALL").is_none() {
        let java = install_java_25_runtime(&paths, |progress| {
            println!("[runtime] {}", progress.message);
        })?;
        install_sandbox_instance(
            &paths,
            instance_id,
            instance_name,
            &java,
            "26.2",
            demo,
            |progress| {
                if progress.completed == 0 || progress.completed == progress.total {
                    println!(
                        "[install:{}] {}/{} {}",
                        progress.stage, progress.completed, progress.total, progress.message
                    );
                }
            },
        )?;
    }

    println!("[launcher] starting latest Minecraft ({mode_name}) in AppContainer");
    let spawned = spawn_instance(&paths, instance_id)?;
    if !spawned.sandboxed {
        return Err("launcher did not use AppContainer".into());
    }
    let narrator_token = spawned
        .narrator_token
        .clone()
        .ok_or("sandboxed launch did not return a narrator token")?;
    let mut child = spawned.child;
    let narrator_failed = Arc::new(AtomicBool::new(false));
    let narration_requested = Arc::new(AtomicBool::new(false));
    let stdout_narration_requested = Arc::clone(&narration_requested);
    let narrator_broker = NarratorBroker::start(narrator_token)?;
    let stdout_narrator_failed = Arc::clone(&narrator_failed);
    let stdout_thread = thread::spawn(move || {
        read_lines(spawned.stdout, |line| {
            if narrator_broker.handle_line(&line) {
                if !stdout_narration_requested.swap(true, Ordering::Relaxed) {
                    println!("[narrator] request forwarded to the trusted SAPI broker");
                }
                return;
            }
            record_narrator_failure(&stdout_narrator_failed, &line);
            println!("[stdout] {line}");
        });
    });
    let stderr_narrator_failed = Arc::clone(&narrator_failed);
    let stderr_thread = thread::spawn(move || {
        read_lines(spawned.stderr, |line| {
            record_narrator_failure(&stderr_narrator_failed, &line);
            eprintln!("[stderr] {line}");
        });
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            stdout_thread.join().ok();
            stderr_thread.join().ok();
            return Err(format!(
                "latest sandboxed Minecraft exited before the smoke window elapsed: {status}"
            )
            .into());
        }
        if Instant::now() >= deadline {
            println!("[launcher] latest sandboxed Minecraft stayed alive for 30 seconds");
            child.kill()?;
            child.wait()?;
            stdout_thread.join().ok();
            stderr_thread.join().ok();
            if narrator_failed.load(Ordering::Relaxed) {
                return Err("Minecraft reported a JNA or narrator initialization failure".into());
            }
            if std::env::var_os("MONALAUNCHER_EXPECT_NARRATOR").is_some()
                && !narration_requested.load(Ordering::Relaxed)
            {
                return Err("Minecraft did not send a narrator request to the launcher".into());
            }
            println!("[launcher] latest AppContainer Minecraft ({mode_name}) smoke test passed");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(windows)]
fn record_narrator_failure(failed: &AtomicBool, line: &str) {
    if line.contains("Error while loading the narrator")
        || line.contains("Failed to create temporary file for /com/sun/jna")
    {
        failed.store(true, Ordering::Relaxed);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("AppContainer is only available on Windows");
    std::process::exit(1);
}
