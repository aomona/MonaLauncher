//! Uses the same installer and Seatbelt launch path as the Tauri UI, without desktop automation.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use monalauncher_lib::minecraft::{installer, launcher, paths::MinecraftPaths, runtime};
    use monalauncher_lib::probe::NarratorBroker;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("usage: minecraft_smoke DATA_ROOT [SECONDS] [VERSION]")?;
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(90);
    let version = args.next().unwrap_or_else(|| "1.21.8".to_owned());
    if !(10..=600).contains(&seconds) || args.next().is_some() {
        return Err("invalid arguments".into());
    }
    if version.is_empty()
        || version.len() > 20
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("invalid smoke version".into());
    }
    let paths = MinecraftPaths::new(PathBuf::from(root));
    let fixture = if version == "1.21.8" {
        "macos-seatbelt-demo".to_owned()
    } else {
        format!("macos-demo-{}", version.replace('.', "-"))
    };
    let id = fixture.as_str();
    if !paths.instance_manifest(id).exists() {
        let major = installer::version_java_major(&version)?;
        let java = runtime::install_java_runtime(&paths, major, |p| eprintln!("{}", p.message))?;
        installer::install_sandbox_demo_instance(
            &paths,
            id,
            &format!("macOS Seatbelt Demo {version}"),
            &java,
            &version,
            |p| {
                if p.completed == p.total || p.completed % 250 == 0 {
                    eprintln!("{} {}/{} {}", p.stage, p.completed, p.total, p.message);
                }
            },
        )?;
    }
    let game = paths.instance_game_directory(id);
    let options = game.join("options.txt");
    if !options.exists() {
        std::fs::write(
            options,
            "fullscreen:false\noverrideWidth:854\noverrideHeight:480\nmaxFps:30\n",
        )?;
    }
    let mut spawned = launcher::spawn_instance(&paths, id, None)?;
    let narrator = Arc::new(NarratorBroker::start(
        spawned
            .narrator_token
            .take()
            .ok_or("missing narrator token")?,
    )?);
    let stdout_narrator = Arc::clone(&narrator);
    let expect_narrator = std::env::var_os("MONALAUNCHER_EXPECT_NARRATOR").is_some();
    eprintln!("Seatbelt Minecraft PID {}", spawned.child.id());
    let sound_started = Arc::new(AtomicBool::new(false));
    let sound_observer = Arc::clone(&sound_started);
    let stdout = std::thread::spawn(move || {
        launcher::read_lines(spawned.stdout, |line| {
            if stdout_narrator.handle_line(&line) {
                return;
            }
            if line.ends_with("]: Sound engine started") {
                sound_observer.store(true, Ordering::Relaxed);
            }
            println!("{line}");
        })
    });
    let stderr =
        std::thread::spawn(move || launcher::read_lines(spawned.stderr, |l| eprintln!("{l}")));
    let started = Instant::now();
    loop {
        if let Some(status) = spawned.child.try_wait()? {
            return Err(format!("Minecraft exited during smoke: {status}").into());
        }
        if started.elapsed() >= Duration::from_secs(seconds) {
            break;
        }
        // The GUI app already runs the main event loop; the CLI must service speech callbacks too.
        objc2_foundation::NSRunLoop::currentRunLoop().runUntilDate(
            &objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.01),
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    // A captured window buffer can contain pixels even when macOS never displays the window.
    // Inspect compositor visibility without activating the application or generating input.
    let mut checker = Command::new("/usr/bin/swift")
        .arg("-")
        .arg(spawned.child.id().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    checker
        .stdin
        .take()
        .ok_or("window checker stdin unavailable")?
        .write_all(include_bytes!("minecraft_window_state.swift"))?;
    let window = checker.wait_with_output()?;
    eprintln!(
        "Window observation: {}",
        String::from_utf8_lossy(&window.stdout)
    );
    if !window.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&window.stderr));
    }
    eprintln!("Smoke observation complete; stopping owned Minecraft process group.");
    spawned.child.kill()?;
    spawned.child.wait()?;
    stdout.join().map_err(|_| "stdout reader failed")?;
    stderr.join().map_err(|_| "stderr reader failed")?;
    let (speech_started, speech_completed) = narrator.playback_counts();
    eprintln!(
        "Narrator playback callbacks: started={speech_started}, completed={speech_completed}"
    );
    if !window.status.success() {
        return Err("Minecraft window is not on screen (or visibility inspection failed)".into());
    }
    if !sound_started.load(Ordering::Relaxed) {
        return Err("Minecraft sound engine did not initialize".into());
    }
    if expect_narrator && (speech_started == 0 || speech_completed == 0) {
        return Err("Minecraft narrator did not complete native speech playback".into());
    }
    eprintln!("SMOKE PASS: window on screen, sound engine initialized, process stopped.");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("minecraft_smoke currently supports macOS only");
    std::process::exit(1);
}
