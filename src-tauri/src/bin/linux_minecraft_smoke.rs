//! Runs the production Linux installer/launcher in a desktop session without input automation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use monalauncher_lib::minecraft::{
        installer, launcher,
        paths::MinecraftPaths,
        permissions::{save_permissions, InstancePermissions},
        runtime,
    };
    use monalauncher_lib::probe::NarratorBroker;
    use std::{
        path::PathBuf,
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let mut args = std::env::args().skip(1);
    let root = args.next().ok_or(
        "usage: linux_minecraft_smoke DATA_ROOT [SECONDS] [VERSION] [narrator-on|narrator-off] [audio-on|audio-off]",
    )?;
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(60);
    let version = args.next().unwrap_or_else(|| "26.2".into());
    let narrator_enabled = match args.next().as_deref() {
        None | Some("narrator-on") => true,
        Some("narrator-off") => false,
        _ => return Err("invalid narrator setting".into()),
    };
    let audio_enabled = match args.next().as_deref() {
        None | Some("audio-on") => true,
        Some("audio-off") => false,
        _ => return Err("invalid audio setting".into()),
    };
    if !(10..=600).contains(&seconds)
        || args.next().is_some()
        || version.len() > 20
        || version.is_empty()
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("invalid arguments".into());
    }
    let wayland = monalauncher_lib::probe::Desktop::detect_with_audio(audio_enabled)?.protocol()
        == monalauncher_lib::sandbox::LinuxDisplayProtocol::Wayland;
    let paths = MinecraftPaths::new(PathBuf::from(root));
    let id = format!("linux-demo-{}", version.replace('.', "-"));
    if !paths.instance_manifest(&id).exists() {
        let major = installer::version_java_major(&version)?;
        let java = runtime::install_java_runtime(&paths, major, |p| eprintln!("{}", p.message))?;
        installer::install_sandbox_demo_instance(
            &paths,
            &id,
            &format!("Linux Sandbox Demo {version}"),
            &java,
            &version,
            |p| {
                if p.completed == p.total || p.completed % 250 == 0 {
                    eprintln!("{} {}/{} {}", p.stage, p.completed, p.total, p.message);
                }
            },
        )?;
    }
    save_permissions(
        &paths,
        &id,
        InstancePermissions {
            game_write: true,
            narrator: narrator_enabled,
            audio_output: audio_enabled,
            ..InstancePermissions::default()
        },
    )?;
    let game = paths.instance_game_directory(&id);
    let options = game.join("options.txt");
    if !options.exists() {
        std::fs::write(options, "fullscreen:false\noverrideWidth:854\noverrideHeight:480\nmaxFps:30\nrenderDistance:4\n")?;
    }
    let mut spawned = launcher::spawn_instance(&paths, &id, None)?;
    let narrator = if narrator_enabled {
        Some(Arc::new(NarratorBroker::start(
            spawned
                .narrator_token
                .take()
                .ok_or("missing narrator token")?,
        )?))
    } else {
        if spawned.narrator_token.is_some() {
            return Err("disabled narrator still received a broker token".into());
        }
        None
    };
    let graphics = Arc::new(AtomicBool::new(false));
    let observed_graphics = graphics.clone();
    let speech = narrator.clone();
    let sound = Arc::new(AtomicBool::new(false));
    let observed_sound = sound.clone();
    let protocol_seen = Arc::new(AtomicBool::new(false));
    let observed_protocol = protocol_seen.clone();
    let stdout = std::thread::spawn(move || {
        launcher::read_lines(spawned.stdout, |line| {
            if line.contains("MONALAUNCHER_NARRATOR\t") {
                observed_protocol.store(true, Ordering::Relaxed);
                if let Some(broker) = &speech {
                    broker.handle_line(&line);
                }
                return;
            }
            if line.ends_with("]: Sound engine started") {
                observed_sound.store(true, Ordering::Relaxed);
            }
            if line.contains("Using graphics backend OpenGL, using drivers:") {
                observed_graphics.store(true, Ordering::Relaxed);
            }
            println!("{line}");
        })
    });
    let stderr = std::thread::spawn(move || {
        launcher::read_lines(spawned.stderr, |line| eprintln!("{line}"))
    });
    eprintln!("bubblewrap supervisor PID {}", spawned.child.id());
    let start = Instant::now();
    let mut window_seen = false;
    let mut audio_stream_seen = false;
    while start.elapsed() < Duration::from_secs(seconds) {
        if let Some(status) = spawned.child.try_wait()? {
            return Err(format!("Minecraft exited early: {status}").into());
        }
        // Wayland has no universal external window-enumeration API. Its visibility
        // must be checked separately through the compositor, not inferred from XWayland.
        if !wayland {
            // Read-only X11 inspection in the dedicated guest; never activate or focus a window.
            let windows = Command::new("/usr/bin/xwininfo")
                .args(["-root", "-tree"])
                .output()?;
            let text = String::from_utf8_lossy(&windows.stdout);
            if let Some(line) = text
                .lines()
                .find(|line| line.contains("\"Minecraft") && line.contains("854x480"))
            {
                if let Some(id) = line.split_whitespace().next() {
                    let state = Command::new("/usr/bin/xwininfo")
                        .args(["-id", id])
                        .output()?;
                    window_seen |=
                        String::from_utf8_lossy(&state.stdout).contains("Map State: IsViewable");
                }
            }
        }
        let audio = Command::new("/usr/bin/pactl")
            .args(["list", "sink-inputs"])
            .output()?;
        audio_stream_seen |= String::from_utf8_lossy(&audio.stdout)
            .contains("application.process.binary = \"java\"");
        std::thread::sleep(Duration::from_secs(1));
    }
    spawned.child.kill()?;
    spawned.child.wait()?;
    stdout.join().map_err(|_| "stdout reader failed")?;
    stderr.join().map_err(|_| "stderr reader failed")?;
    let speech_counts = narrator
        .as_ref()
        .map(|broker| broker.playback_counts())
        .unwrap_or_default();
    let sound_started = sound.load(Ordering::Relaxed);
    let narrator_protocol = protocol_seen.load(Ordering::Relaxed);
    println!(
        "OBSERVATION {}",
        serde_json::json!({"display_protocol":if wayland {"wayland"} else {"x11"}, "window_viewable":if wayland {None} else {Some(window_seen)}, "graphics_initialized":graphics.load(Ordering::Relaxed), "audio_enabled":audio_enabled, "sound_engine":sound_started, "pulse_java_stream":audio_stream_seen, "narrator_enabled":narrator_enabled, "narrator_protocol":narrator_protocol, "speech_started":speech_counts.0, "speech_completed":speech_counts.1})
    );
    if (!wayland && !window_seen)
        || (wayland && !graphics.load(Ordering::Relaxed))
        || (audio_enabled && (!sound_started || !audio_stream_seen))
        || (!audio_enabled && audio_stream_seen)
    {
        return Err("desktop/sound observations incomplete".into());
    }
    if std::env::var_os("MONALAUNCHER_EXPECT_NARRATOR").is_some()
        && narrator_enabled
        && (speech_counts.0 == 0 || speech_counts.1 == 0)
    {
        return Err("narrator playback did not complete".into());
    }
    if !narrator_enabled && narrator_protocol {
        return Err("disabled narrator emitted requests".into());
    }
    if wayland {
        println!("SMOKE PASS: Wayland process/graphics/audio/narrator policy and shutdown; visibility requires separate compositor observation");
    } else {
        println!("SMOKE PASS: production Linux sandbox desktop/audio/narrator policy and shutdown");
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux_minecraft_smoke requires Linux");
    std::process::exit(1);
}
