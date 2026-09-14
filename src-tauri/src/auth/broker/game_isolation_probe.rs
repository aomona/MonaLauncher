//! Concurrent, actual Fabric clients using synthetic signing keys and the production IPC.
use super::{
    chat::ChatKeys,
    game_key_probe::{Fixture, SyntheticSession},
    protocol::{BrokerError, Command},
    service::Operations,
};
use crate::minecraft::{
    file_io::{read_bounded_file, write_atomic},
    installer,
    launcher::{self, MinecraftIdentity, MinecraftProcess},
    model::ModLoader,
    paths::MinecraftPaths,
    permissions::{save_permissions, AccountAuthentication, InstancePermissions},
    runtime,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Audit {
    foreign_rejections: AtomicUsize,
    signatures: AtomicUsize,
    dropped: AtomicUsize,
}
struct AuditedFixture {
    inner: Fixture,
    audit: Arc<Audit>,
    valid: Arc<AtomicBool>,
}
impl Operations for AuditedFixture {
    fn valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }
    fn execute(&mut self, command: &Command) -> Result<Value, BrokerError> {
        let result = self.inner.execute(command);
        if matches!(command, Command::Sign { .. }) {
            if result == Err(BrokerError::Revoked) {
                self.audit
                    .foreign_rejections
                    .fetch_add(1, Ordering::Release);
            } else if result.is_ok() {
                self.audit.signatures.fetch_add(1, Ordering::Release);
            }
        }
        result
    }
}
impl Drop for AuditedFixture {
    fn drop(&mut self) {
        self.inner.0.clear();
        self.audit.dropped.fetch_add(1, Ordering::Release);
    }
}
struct Game {
    child: MinecraftProcess,
    readers: Vec<JoinHandle<()>>,
    failed: Arc<AtomicBool>,
    game: PathBuf,
}
impl Game {
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
    fn wait(&mut self, name: &str) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(120);
        let path = self.game.join(name);
        loop {
            assert!(
                !self.failed.load(Ordering::Acquire),
                "Mod isolation probe failed"
            );
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "game exited during isolation probe"
            );
            if path.exists() {
                return read_bounded_file(&path, 4096).unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "isolation barrier timed out: {name}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Game {
    fn drop(&mut self) {
        self.stop();
    }
}

fn prepare(paths: &MinecraftPaths, version: &str, id: &str, jar: &Path, peer: &Path) -> PathBuf {
    if !paths.instance_manifest(id).exists() {
        let java = runtime::install_java_runtime(
            paths,
            installer::version_java_major(version).unwrap(),
            |_| {},
        )
        .unwrap();
        installer::install_sandbox_instance_with_loader(
            paths,
            id,
            "Concurrent Auth Probe",
            &java,
            version,
            false,
            ModLoader::Fabric {
                version: "0.19.5".into(),
            },
            |_| {},
        )
        .unwrap();
    }
    save_permissions(
        paths,
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
    fs::copy(jar, game.join("mods/mona-token-read-probe.jar")).unwrap();
    fs::write(
        game.join("options.txt"),
        "fullscreen:false\noverrideWidth:640\noverrideHeight:360\nmaxFps:15\n",
    )
    .unwrap();
    // Properties escapes are significant even though these managed paths are not secrets.
    let peer = peer
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    fs::write(
        game.join("auth-probe.properties"),
        format!("instanceIsolation=true\npeerReadyPath={peer}\n"),
    )
    .unwrap();
    for name in [
        "isolation-ready.txt",
        "isolation-peer.txt",
        "isolation-cross.json",
        "isolation-after-peer-exit.txt",
        "isolation-survivor.json",
        "isolation-revoked.txt",
        "isolation-result.json",
    ] {
        let path = game.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    game
}
fn launch(
    paths: &MinecraftPaths,
    id: &str,
    game: PathBuf,
    audit: Arc<Audit>,
    valid: Arc<AtomicBool>,
) -> Game {
    let identity = MinecraftIdentity {
        player_name: "MonaProbe".into(),
        uuid: super::chat::tests::ACCOUNT.into(),
        broker: Some(Arc::new(SyntheticSession)),
    };
    let spawned = launcher::spawn_instance_with_factory(
        paths,
        id,
        Some(&identity),
        launcher::LaunchMode::Default,
        |_, _, _| {
            Ok(Box::new(AuditedFixture {
                inner: Fixture(ChatKeys::default()),
                audit,
                valid,
            }))
        },
    )
    .unwrap();
    let failed = Arc::new(AtomicBool::new(false));
    let readers = [spawned.stdout, spawned.stderr]
        .into_iter()
        .map(|stream| {
            let failed = Arc::clone(&failed);
            std::thread::spawn(move || {
                launcher::read_lines(stream, |line| {
                    if line.contains("MONALAUNCHER_AUTH_PROBE_FAILED:") {
                        failed.store(true, Ordering::Release);
                    }
                })
            })
        })
        .collect();
    Game {
        child: spawned.child,
        readers,
        failed,
        game,
    }
}
fn wait_for_drop(audit: &Audit) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while audit.dropped.load(Ordering::Acquire) == 0 {
        assert!(
            Instant::now() < deadline,
            "key owner remained after shutdown/revocation"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(audit.dropped.load(Ordering::Acquire), 1);
}

#[test]
#[ignore = "requires MONALAUNCHER_ISOLATION_ROOT, MONALAUNCHER_ISOLATION_JAR, MONALAUNCHER_ISOLATION_REPORT; optional MONALAUNCHER_ISOLATION_VERSION; starts two simultaneous Fabric games without real accounts"]
fn concurrent_fabric_instances_reject_foreign_keys_and_survive_peer_exit() {
    let paths = MinecraftPaths::new(PathBuf::from(
        std::env::var_os("MONALAUNCHER_ISOLATION_ROOT").expect("root"),
    ));
    let jar = PathBuf::from(std::env::var_os("MONALAUNCHER_ISOLATION_JAR").expect("jar"));
    let report = PathBuf::from(std::env::var_os("MONALAUNCHER_ISOLATION_REPORT").expect("report"));
    let version = std::env::var("MONALAUNCHER_ISOLATION_VERSION").unwrap_or_else(|_| "26.2".into());
    assert!(["1.21.8", "26.2"].contains(&version.as_str()));
    let ids = [
        format!("auth-isolate-a-{}", version.replace('.', "-")),
        format!("auth-isolate-b-{}", version.replace('.', "-")),
    ];
    let game_a = prepare(
        &paths,
        &version,
        &ids[0],
        &jar,
        &paths
            .instance_game_directory(&ids[1])
            .join("isolation-ready.txt"),
    );
    let game_b = prepare(
        &paths,
        &version,
        &ids[1],
        &jar,
        &paths
            .instance_game_directory(&ids[0])
            .join("isolation-ready.txt"),
    );
    let audit_a = Arc::new(Audit::default());
    let audit_b = Arc::new(Audit::default());
    let valid_b = Arc::new(AtomicBool::new(true));
    let mut a = launch(
        &paths,
        &ids[0],
        game_a,
        Arc::clone(&audit_a),
        Arc::new(AtomicBool::new(true)),
    );
    let mut b = launch(
        &paths,
        &ids[1],
        game_b,
        Arc::clone(&audit_b),
        Arc::clone(&valid_b),
    );
    let key_a = a.wait("isolation-ready.txt");
    let key_b = b.wait("isolation-ready.txt");
    assert_eq!(key_a.len(), 64);
    assert_eq!(key_b.len(), 64);
    assert_ne!(key_a, key_b);
    write_atomic(&a.game.join("isolation-peer.txt"), &key_b).unwrap();
    write_atomic(&b.game.join("isolation-peer.txt"), &key_a).unwrap();
    let cross_a: Value = serde_json::from_slice(&a.wait("isolation-cross.json")).unwrap();
    let cross_b: Value = serde_json::from_slice(&b.wait("isolation-cross.json")).unwrap();
    for (result, audit) in [(&cross_a, &audit_a), (&cross_b, &audit_b)] {
        assert_eq!(result["foreignKeyRejected"], true);
        assert_eq!(result["ownSignatureVerified"], true);
        assert_eq!(result["peerReadyFileReadable"], false);
        assert_eq!(audit.foreign_rejections.load(Ordering::Acquire), 1);
        assert_eq!(audit.signatures.load(Ordering::Acquire), 1);
    }
    a.stop();
    wait_for_drop(&audit_a);
    assert_eq!(audit_b.dropped.load(Ordering::Acquire), 0);
    write_atomic(&b.game.join("isolation-after-peer-exit.txt"), b"continue").unwrap();
    let survivor: Value = serde_json::from_slice(&b.wait("isolation-survivor.json")).unwrap();
    assert_eq!(survivor["signatureAfterPeerExitVerified"], true);
    assert_eq!(audit_b.signatures.load(Ordering::Acquire), 2);
    valid_b.store(false, Ordering::Release);
    wait_for_drop(&audit_b);
    write_atomic(&b.game.join("isolation-revoked.txt"), b"revoked").unwrap();
    let revoked: Value = serde_json::from_slice(&b.wait("isolation-result.json")).unwrap();
    assert_eq!(revoked["signatureAfterRevocationRejected"], true);
    assert_eq!(audit_b.signatures.load(Ordering::Acquire), 2);
    b.stop();
    let result = json!({"schema":1,"minecraft":version,"platform":std::env::consts::OS,"probeSha256":format!("{:x}",Sha256::digest(fs::read(&jar).unwrap())),
        "twoGamesConcurrent":true,"distinctKeyHandles":true,"gameA":cross_a,"gameB":cross_b,"survivor":survivor,"revoked":revoked,
        "rustForeignKeyRejections":[1,1],"rustSuccessfulSignatures":[1,2],"keyOwnersReleased":[true,true],
        "scope":"Synthetic public RSA fixture; actual sandboxed Fabric JVMs, forged RemotePrivateKey bypasses Java marker checks and reaches Rust. Peer files were known to exist before read attempts. Lease invalidation is injected through Operations.valid, not the Microsoft UI. Does not prevent an already-authorized malicious game from relaying allowed operations to another game."});
    fs::create_dir_all(report.parent().unwrap()).unwrap();
    fs::write(report, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!(
        "PASS: {version} concurrent Fabric keys isolated; peer exit and idle revocation verified"
    );
}
