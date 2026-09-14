//! Explicit opt-in real-account/real-game test. All account secrets stay in this Rust process.
use super::{
    acquire_minecraft_session, authentication_generation, broker_source, MicrosoftAuthState,
};
use crate::minecraft::{
    installer, launcher,
    model::ModLoader,
    paths::MinecraftPaths,
    permissions::{save_permissions, AccountAuthentication, InstancePermissions},
    runtime,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "uses the saved Microsoft account; requires MONALAUNCHER_ONLINE_ROOT, MONALAUNCHER_ONLINE_PROBE_JAR and MONALAUNCHER_ONLINE_SERVER; matching local server must already be running with accepted EULA; optional MONALAUNCHER_ONLINE_VERSION and MONALAUNCHER_ONLINE_REPEATS"]
fn saved_account_joins_secure_profile_server_and_sends_chat() {
    let paths = MinecraftPaths::new(PathBuf::from(
        std::env::var_os("MONALAUNCHER_ONLINE_ROOT").expect("game root"),
    ));
    let probe =
        PathBuf::from(std::env::var_os("MONALAUNCHER_ONLINE_PROBE_JAR").expect("probe jar"));
    let server = PathBuf::from(
        std::env::var_os("MONALAUNCHER_ONLINE_SERVER").expect("local server directory"),
    );
    let version = std::env::var("MONALAUNCHER_ONLINE_VERSION").unwrap_or_else(|_| "26.2".into());
    assert!(["1.21.8", "26.2"].contains(&version.as_str()));
    let repeats = std::env::var("MONALAUNCHER_ONLINE_REPEATS")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .unwrap();
    assert!((1..=3).contains(&repeats));
    let prepared: serde_json::Value =
        serde_json::from_slice(&fs::read(server.join("preparation.json")).unwrap()).unwrap();
    assert_eq!(prepared["minecraft"], version);
    let message = format!("MONA_AUTH_PROBE_SIGNED_CHAT_{}", version.replace('.', "_"));
    let properties = fs::read_to_string(server.join("server.properties")).unwrap();
    for required in [
        "server-ip=127.0.0.1",
        "server-port=35565",
        "online-mode=true",
        "enforce-secure-profile=true",
    ] {
        assert!(
            properties.lines().any(|line| line == required),
            "local server configuration differs"
        );
    }
    assert!(fs::read_to_string(server.join("eula.txt"))
        .unwrap()
        .lines()
        .any(|line| line == "eula=true"));
    let log_path = server.join("logs/latest.log");
    let state = MicrosoftAuthState::default();
    let generation = authentication_generation(&state);
    let session = tauri::async_runtime::block_on(acquire_minecraft_session(&state))
        .unwrap_or_else(|_| panic!("Saved-account authentication failed; use the launcher sign-in flow before retrying"));
    let identity = launcher::MinecraftIdentity {
        broker: Some(broker_source(&state, session.uuid.clone(), generation)),
        player_name: session.player_name,
        uuid: session.uuid,
    };
    let instance_id = format!("auth-online-{}", version.replace('.', "-"));
    let id = instance_id.as_str();
    if !paths.instance_manifest(id).exists() {
        let java = runtime::install_java_runtime(
            &paths,
            installer::version_java_major(&version).unwrap(),
            |_| {},
        )
        .unwrap();
        installer::install_sandbox_instance_with_loader(
            &paths,
            id,
            "Auth Online Probe",
            &java,
            &version,
            false,
            ModLoader::Fabric {
                version: "0.19.5".into(),
            },
            |_| {},
        )
        .unwrap();
    }
    save_permissions(
        &paths,
        id,
        InstancePermissions {
            account_authentication: AccountAuthentication::Brokered,
            network: true,
            narrator: false,
            ..InstancePermissions::default()
        },
    )
    .unwrap();
    let game = paths.instance_game_directory(id);
    fs::create_dir_all(game.join("mods")).unwrap();
    fs::copy(probe, game.join("mods/mona-token-read-probe.jar")).unwrap();
    fs::write(
        game.join("auth-probe.properties"),
        format!("onlineConnection=true\nonlineVersion={version}\n"),
    )
    .unwrap();
    fs::write(
        game.join("options.txt"),
        "fullscreen:false\noverrideWidth:854\noverrideHeight:480\nmaxFps:30\n",
    )
    .unwrap();
    let mut observations = Vec::new();
    for _ in 0..repeats {
        let log_offset = fs::metadata(&log_path).unwrap().len() as usize;
        let result = game.join("online-probe-result.json");
        if result.exists() {
            fs::remove_file(&result).unwrap();
        }
        let mut child = launcher::spawn_instance(&paths, id, Some(&identity)).unwrap();
        // Drain both streams, without printing exception bodies, tokens or account identifiers.
        let stdout = std::thread::spawn(move || launcher::read_lines(child.stdout, |_| {}));
        let stderr = std::thread::spawn(move || launcher::read_lines(child.stderr, |_| {}));
        let deadline = Instant::now() + Duration::from_secs(150);
        while !result.exists() && Instant::now() < deadline {
            if child.child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = child.child.kill();
        child.child.wait().unwrap();
        stdout.join().unwrap();
        stderr.join().unwrap();
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&result).expect("online probe did not produce a result"),
        )
        .unwrap();
        assert_eq!(
            value["connected"], true,
            "online probe phase: {}",
            value["phase"]
        );
        assert_eq!(value["chatSessionOpaque"], true);
        assert_eq!(value["privateKeyEncoded"], false);
        assert_eq!(value["privateKeyCacheAbsent"], true);
        assert_eq!(value["chatSent"], true);
        assert_eq!(value["stillConnected"], true);
        let log = fs::read(&log_path).unwrap();
        assert!(log.len() >= log_offset, "server log rotated during test");
        let fresh_log = String::from_utf8_lossy(&log[log_offset..]);
        assert!(
            fresh_log.contains(&message),
            "server did not receive the probe message"
        );
        assert!(
            fresh_log
                .lines()
                .filter(|line| line.contains(&message))
                .all(|line| !line.contains("Not Secure")),
            "server classified the probe message as unsigned/untrusted"
        );
        observations.push(value);
        fs::write(
            game.join("online-probe-history.json"),
            serde_json::to_vec_pretty(&observations).unwrap(),
        )
        .unwrap();
        println!("PASS: {version} real Fabric game joined loopback online-mode/secure-profile server; opaque key and server-received chat verified");
    }
}
