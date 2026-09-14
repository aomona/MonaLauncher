//! Actual Fabric key-read comparison. Only the public test fixture is used, never accounts.
use super::{
    chat::{
        tests::{certificate, ACCOUNT},
        ChatKeys,
    },
    protocol::{BrokerError, Command},
    service::{Operations, SessionSource},
};
use crate::{
    auth::minecraft_services::MinecraftSession,
    minecraft::{
        installer, launcher,
        model::ModLoader,
        paths::MinecraftPaths,
        permissions::{save_permissions, AccountAuthentication, InstancePermissions},
        runtime,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

pub(super) struct SyntheticSession;
impl SessionSource for SyntheticSession {
    fn valid(&self) -> bool {
        true
    }
    fn current(&self) -> Result<MinecraftSession, BrokerError> {
        Err(BrokerError::Unsupported)
    }
}
pub(super) struct Fixture(pub(super) ChatKeys);
impl Operations for Fixture {
    fn valid(&self) -> bool {
        true
    }
    fn execute(&mut self, command: &Command) -> Result<Value, BrokerError> {
        match command {
            Command::Certificate {} => {
                let now = SystemTime::now();
                match self.0.cached(now) {
                    Some(value) => Ok(value),
                    None => self.0.install(certificate(now), now),
                }
            }
            Command::Sign { key_id, message } => {
                self.0.sign(key_id, message, ACCOUNT, SystemTime::now())
            }
            _ => Err(BrokerError::Unsupported),
        }
    }
}

// Minimal DER reader restricted to this checked-in test fixture, not untrusted input.
fn element<'a>(bytes: &mut &'a [u8], tag: u8) -> &'a [u8] {
    assert_eq!(bytes[0], tag);
    let first = bytes[1];
    let (length, header) = if first < 128 {
        (first as usize, 2)
    } else {
        let size = (first & 127) as usize;
        let length = bytes[2..2 + size]
            .iter()
            .fold(0, |n, byte| n * 256 + *byte as usize);
        (length, 2 + size)
    };
    let value = &bytes[header..header + length];
    *bytes = &bytes[header + length..];
    value
}
fn patterns() -> Vec<(String, Vec<u8>)> {
    let mut pkcs8 = include_bytes!("fixtures/test-only-chat-key.pk8").as_slice();
    let mut sequence = element(&mut pkcs8, 0x30);
    element(&mut sequence, 2);
    element(&mut sequence, 0x30);
    let mut octets = element(&mut sequence, 4);
    let mut rsa = element(&mut octets, 0x30);
    for _ in 0..3 {
        element(&mut rsa, 2);
    } // version, public modulus, public exponent
    let exponent = element(&mut rsa, 2);
    let exponent = exponent.strip_prefix(&[0]).unwrap_or(exponent);
    let mut patterns = vec![("private_exponent_der".into(), exponent[..32].to_vec())];
    // Three alignments cover the fragment in both PKCS#8/PKCS#1 Base64. UTF-16 variants
    // cover strings on JVMs which do not compact their Latin-1 character storage.
    for offset in (0..3).chain(24..27) {
        let text = STANDARD.encode(&exponent[offset..offset + 24]);
        patterns.push((
            format!("private_base64_{offset}_ascii"),
            text.as_bytes().to_vec(),
        ));
        patterns.push((
            format!("private_base64_{offset}_utf16be"),
            text.bytes().flat_map(|byte| [0, byte]).collect(),
        ));
        patterns.push((
            format!("private_base64_{offset}_utf16le"),
            text.bytes().flat_map(|byte| [byte, 0]).collect(),
        ));
    }
    patterns
}
fn configuration(mode: &str) -> String {
    let patterns = patterns();
    let mut text = format!(
        "chatKeyProbe=true\nchatKeyMode={mode}\npattern.count={}\n",
        patterns.len()
    );
    for (index, (name, bytes)) in patterns.iter().enumerate() {
        let rolling = bytes.iter().fold(0_u64, |value, byte| {
            value.wrapping_mul(257).wrapping_add(*byte as u64)
        });
        text.push_str(&format!("pattern.{index}.name={name}\npattern.{index}.length={}\npattern.{index}.rolling={rolling:016x}\npattern.{index}.sha256={:x}\n", bytes.len(), Sha256::digest(bytes)));
    }
    text
}

#[test]
fn fingerprints_cover_private_binary_and_pem_without_containing_the_secret() {
    let patterns = patterns();
    assert_eq!(patterns.len(), 19);
    let fixture = certificate(SystemTime::now());
    let pem = fixture["keyPair"]["privateKey"]
        .as_str()
        .unwrap()
        .as_bytes();
    assert!(patterns
        .iter()
        .filter(|(name, _)| name.ends_with("ascii"))
        .any(|(_, pattern)| pem.windows(pattern.len()).any(|bytes| bytes == pattern)));
    assert!(include_bytes!("fixtures/test-only-chat-key.pk8")
        .windows(32)
        .any(|bytes| bytes == patterns[0].1));
    for width in [64, 76] {
        let encoded = STANDARD.encode(include_bytes!("fixtures/test-only-chat-key.pk8"));
        let wrapped = encoded
            .as_bytes()
            .chunks(width)
            .map(|line| std::str::from_utf8(line).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(patterns
            .iter()
            .filter(|(name, _)| name.ends_with("ascii"))
            .any(|(_, pattern)| wrapped
                .as_bytes()
                .windows(pattern.len())
                .any(|bytes| bytes == pattern)));
    }
    let config = configuration("brokered");
    assert!(!config.contains(fixture["keyPair"]["privateKey"].as_str().unwrap()));
}

#[test]
#[ignore = "requires MONALAUNCHER_KEY_PROBE_ROOT, MONALAUNCHER_KEY_PROBE_JAR and MONALAUNCHER_KEY_PROBE_REPORT; optional MONALAUNCHER_KEY_PROBE_VERSION; launches two actual Fabric games"]
fn actual_fabric_private_key_heap_comparison() {
    let paths = MinecraftPaths::new(PathBuf::from(
        std::env::var_os("MONALAUNCHER_KEY_PROBE_ROOT").expect("game root"),
    ));
    let jar = PathBuf::from(std::env::var_os("MONALAUNCHER_KEY_PROBE_JAR").expect("probe jar"));
    let report = PathBuf::from(std::env::var_os("MONALAUNCHER_KEY_PROBE_REPORT").expect("report"));
    let version = std::env::var("MONALAUNCHER_KEY_PROBE_VERSION").unwrap_or_else(|_| "26.2".into());
    assert!(["1.21.8", "26.2"].contains(&version.as_str()));
    let id = format!("auth-key-probe-{}", version.replace('.', "-"));
    if !paths.instance_manifest(&id).exists() {
        let java = runtime::install_java_runtime(
            &paths,
            installer::version_java_major(&version).unwrap(),
            |_| {},
        )
        .unwrap();
        installer::install_sandbox_instance_with_loader(
            &paths,
            &id,
            "Synthetic Chat Key Probe",
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
        &id,
        InstancePermissions {
            account_authentication: AccountAuthentication::Brokered,
            network: true,
            narrator: false,
            ..InstancePermissions::default()
        },
    )
    .unwrap();
    let game = paths.instance_game_directory(&id);
    fs::create_dir_all(game.join("mods")).unwrap();
    fs::copy(&jar, game.join("mods/mona-token-read-probe.jar")).unwrap();
    fs::write(
        game.join("options.txt"),
        "fullscreen:false\noverrideWidth:854\noverrideHeight:480\nmaxFps:30\n",
    )
    .unwrap();
    let identity = launcher::MinecraftIdentity {
        player_name: "MonaProbe".into(),
        uuid: ACCOUNT.into(),
        broker: Some(Arc::new(SyntheticSession)),
    };
    let mut observations = Vec::new();
    for mode in ["direct", "brokered"] {
        fs::write(game.join("auth-probe.properties"), configuration(mode)).unwrap();
        let fixture_path = game.join("synthetic-chat-key.pem");
        if mode == "direct" {
            fs::write(
                &fixture_path,
                certificate(SystemTime::now())["keyPair"]["privateKey"]
                    .as_str()
                    .unwrap()
                    .replace("BEGIN PRIVATE KEY", "BEGIN RSA PRIVATE KEY")
                    .replace("END PRIVATE KEY", "END RSA PRIVATE KEY"),
            )
            .unwrap();
        } else {
            assert!(!fixture_path.exists());
        }
        let result_path = game.join("chat-key-probe-result.json");
        if result_path.exists() {
            fs::remove_file(&result_path).unwrap();
        }
        let mut spawned = launcher::spawn_instance_with_factory(
            &paths,
            &id,
            Some(&identity),
            launcher::LaunchMode::Default,
            |_, _, _| Ok(Box::new(Fixture(ChatKeys::default()))),
        )
        .unwrap();
        let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stdout_failed = Arc::clone(&failed);
        let stderr_failed = Arc::clone(&failed);
        let stdout = std::thread::spawn(move || {
            launcher::read_lines(spawned.stdout, |line| {
                if line.contains("MONALAUNCHER_AUTH_PROBE_FAILED:") {
                    stdout_failed.store(true, std::sync::atomic::Ordering::Release);
                }
            })
        });
        let stderr = std::thread::spawn(move || {
            launcher::read_lines(spawned.stderr, |line| {
                if line.contains("MONALAUNCHER_AUTH_PROBE_FAILED:") {
                    stderr_failed.store(true, std::sync::atomic::Ordering::Release);
                }
            })
        });
        let deadline = Instant::now() + Duration::from_secs(180);
        while !result_path.exists() && Instant::now() < deadline {
            if failed.load(std::sync::atomic::Ordering::Acquire)
                || spawned.child.try_wait().unwrap().is_some()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = spawned.child.kill();
        spawned.child.wait().unwrap();
        stdout.join().unwrap();
        stderr.join().unwrap();
        for entry in fs::read_dir(&game).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("auth-probe-heap-")
            {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        if fixture_path.exists() {
            fs::remove_file(&fixture_path).unwrap();
        }
        let mut result: Value =
            serde_json::from_slice(&fs::read(&result_path).expect("key-read Mod did not finish"))
                .unwrap();
        result["mode"] = json!(mode);
        result["minecraft"] = json!(version);
        result["platform"] = json!(std::env::consts::OS);
        let expected_detection = mode == "direct";
        for field in [
            "encodedPrivateKeyDetected",
            "cachePrivateKeyDetected",
            "heapPrivateKeyDetected",
            "keyEncodingAvailable",
        ] {
            assert_eq!(result[field], expected_detection, "{mode}: {field}");
        }
        assert_eq!(result["opaquePrivateKey"], !expected_detection);
        assert_eq!(result["liveHeapScanned"], true);
        assert_eq!(result["fixtureFileRemoved"], true);
        observations.push(result);
        println!("PASS: {version} {mode} private-key read probe");
    }
    let result = json!({"schema":1,"scope":"public synthetic RSA fixture, binary private exponent and Base64/UTF-16 representations; live Java heap only; no real accounts", "probeSha256":format!("{:x}",Sha256::digest(fs::read(&jar).unwrap())), "observations":observations});
    fs::create_dir_all(report.parent().unwrap()).unwrap();
    fs::write(report, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
