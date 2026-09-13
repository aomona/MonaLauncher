//! Runs an actual Fabric mod against a random synthetic token; never prints token contents.
use monalauncher_lib::auth::{
    broker::{protocol::BrokerError, service::SessionSource},
    minecraft_services::MinecraftSession,
};
use monalauncher_lib::minecraft::{
    installer,
    launcher::{self, MinecraftIdentity},
    model::ModLoader,
    paths::MinecraftPaths,
    permissions::{save_permissions, InstancePermissions},
    runtime,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

struct ProbeSession(MinecraftSession);
impl SessionSource for ProbeSession {
    fn valid(&self) -> bool {
        true
    }
    fn current(&self) -> Result<MinecraftSession, BrokerError> {
        Ok(self.0.clone())
    }
}
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: auth_broker_probe DATA_ROOT PROBE_JAR REPORT [VERSION]")?,
    );
    let probe = PathBuf::from(args.next().ok_or("missing probe jar")?);
    let report = PathBuf::from(args.next().ok_or("missing report output")?);
    let version = args.next().unwrap_or_else(|| "26.2".into());
    if args.next().is_some() || !["1.21.8", "26.2"].contains(&version.as_str()) {
        return Err("invalid arguments".into());
    }
    let paths = MinecraftPaths::new(root);
    let id = format!("auth-probe-brokered-{}", version.replace('.', "-"));
    if !paths.instance_manifest(&id).exists() {
        let java = runtime::install_java_runtime(
            &paths,
            installer::version_java_major(&version)?,
            |_| {},
        )?;
        installer::install_sandbox_instance_with_loader(
            &paths,
            &id,
            "Auth Probe (synthetic token)",
            &java,
            &version,
            false,
            ModLoader::Fabric {
                version: "0.19.5".into(),
            },
            |_| {},
        )?;
    }
    save_permissions(
        &paths,
        &id,
        InstancePermissions {
            account_authentication:
                monalauncher_lib::minecraft::permissions::AccountAuthentication::Brokered,
            network: false,
            narrator: false,
            ..InstancePermissions::default()
        },
    )?;
    let game = paths.instance_game_directory(&id);
    fs::create_dir_all(game.join("mods"))?;
    fs::copy(&probe, game.join("mods/mona-token-read-probe.jar"))?;
    fs::write(
        game.join("options.txt"),
        "fullscreen:false\noverrideWidth:854\noverrideHeight:480\nmaxFps:30\n",
    )?;
    let mut bytes = [0_u8; 48];
    getrandom::fill(&mut bytes).map_err(|_| "random source unavailable")?;
    let secret: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let fingerprint = format!("{:x}", Sha256::digest(secret.as_bytes()));
    // A prior direct-token launch could leave secrets in this regeneratable cache. The
    // real launch path must remove it before the adversarial Mod starts scanning files.
    fs::create_dir_all(game.join("profilekeys"))?;
    fs::write(game.join("profilekeys/legacy-auth-probe.json"), &secret)?;
    fs::write(
        game.join("auth-probe.properties"),
        format!(
            "sha256={fingerprint}\nlength={}\nheapDump=true\nnativeProbe=true\nparentPid={}\nparentAddress={}\n",
            secret.len(), std::process::id(), secret.as_ptr() as usize
        ),
    )?;
    let result = game.join("auth-probe-result.json");
    if result.exists() {
        fs::remove_file(&result)?;
    }
    let identity = MinecraftIdentity {
        player_name: "MonaProbe".into(),
        uuid: "0123456789abcdef0123456789abcdef".into(),
        broker: Some(Arc::new(ProbeSession(MinecraftSession {
            player_name: "MonaProbe".into(),
            uuid: "0123456789abcdef0123456789abcdef".into(),
            access_token: secret.clone(),
            expires_in: Duration::from_secs(300),
        }))),
    };
    let mut launched = launcher::spawn_instance(&paths, &id, Some(&identity))?;
    let (diagnostics, messages) = std::sync::mpsc::sync_channel(128);
    let stdout_diagnostics = diagnostics.clone();
    let stdout_secret = secret.clone();
    let stdout = std::thread::spawn(move || {
        launcher::read_lines(launched.stdout, |line| {
            let _ = stdout_diagnostics
                .try_send(line.replace(&stdout_secret, "[SYNTHETIC_TOKEN_REDACTED]"));
            if line.contains("MONALAUNCHER_AUTH_PROBE_COMPLETE") {
                println!("Fabric probe completed");
            }
            if line.contains("MONALAUNCHER_AUTH_PROBE_FAILED:") {
                eprintln!("Fabric probe reported failure");
            }
        })
    });
    let stderr_secret = secret.clone();
    let stderr = std::thread::spawn(move || {
        launcher::read_lines(launched.stderr, |line| {
            let _ =
                diagnostics.try_send(line.replace(&stderr_secret, "[SYNTHETIC_TOKEN_REDACTED]"));
        })
    });
    let start = Instant::now();
    let mut game_exited = false;
    while !result.exists() && start.elapsed() < Duration::from_secs(120) {
        if launched.child.try_wait()?.is_some() {
            game_exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    launched.child.kill()?;
    launched.child.wait()?;
    stdout.join().map_err(|_| "stdout thread failed")?;
    stderr.join().map_err(|_| "stderr thread failed")?;
    // The VM may be killed during dump creation, before Java's finally block can delete it.
    for entry in fs::read_dir(&game)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if ((name.starts_with("auth-probe-heap-") && name.ends_with(".hprof"))
            || (name.starts_with("auth-probe-native-")
                && (name.ends_with(".dylib") || name.ends_with(".so"))))
            && entry.file_type()?.is_file()
        {
            fs::remove_file(entry.path())?;
        }
    }
    // Remove synthetic credential occurrences from runtime logs before exposing diagnostics.
    for path in [game.join("logs/latest.log"), game.join("options.txt")] {
        if let Ok(text) = fs::read_to_string(&path) {
            fs::write(path, text.replace(&secret, "[SYNTHETIC_TOKEN_REDACTED]"))?;
        }
    }
    if !result.exists() {
        for line in messages.try_iter() {
            eprintln!("{line}");
        }
        return Err(if game_exited {
            "game exited before probe result"
        } else {
            "Fabric probe timed out"
        }
        .into());
    }
    let mut observation: serde_json::Value = serde_json::from_slice(&fs::read(result)?)?;
    observation["minecraft"] = version.into();
    observation["mode"] = "brokered-network-denied".into();
    observation["credentialKind"] = "random synthetic canary; no account credentials".into();
    observation["probeSha256"] = format!("{:x}", Sha256::digest(fs::read(&probe)?)).into();
    observation["platform"] = std::env::consts::OS.into();
    observation["sandboxed"] = launched.sandboxed.into();
    if let Some(parent) = report.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&report, serde_json::to_vec_pretty(&observation)?)?;
    if observation["liveJavaHeapDumpScanned"] != true
        || observation["nativeMemoryProbeRan"] != true
        || observation["nativeControlPassed"] != true
        || observation["nativeSelfReadAllowed"] != cfg!(target_os = "macos")
        || observation["nativeSelfReadError"] != if cfg!(target_os = "macos") { 0 } else { 1 }
        || observation["launcherMemoryReadAllowed"] != false
        || observation["launcherMemoryReadError"] != if cfg!(target_os = "macos") { 5 } else { 1 }
        || observation["tokenDetected"] != false
        || observation["legacyProfileKeyCacheVisible"] != false
        || observation["agentAdapterPresent"] != true
        || observation["chatAdapterPresent"] != true
        || observation["brokerHandshakeCompleted"] != true
    {
        return Err(
            "brokered probe failed: secret detected or heap/native/auth/chat adapter/IPC check missing".into(),
        );
    }
    println!(
        "PASS: live broker handshake completed and Fabric mod could not read the synthetic token; report {}",
        report.display()
    );
    Ok(())
}
