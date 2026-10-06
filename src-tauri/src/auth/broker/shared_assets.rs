//! Exercises the dependency's actual Java/JNI bytes against this launcher's broker.
use super::{
    channel::PreparedBroker,
    protocol::{BrokerError, Command},
    service::Operations,
};
use std::{
    process::Command as ProcessCommand,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Fixture(Arc<AtomicUsize>);
impl Operations for Fixture {
    fn valid(&self) -> bool {
        true
    }

    fn execute(&mut self, command: &Command) -> Result<serde_json::Value, BrokerError> {
        assert!(matches!(command, Command::Properties {}));
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(serde_json::json!({ "sharedAssets": true }))
    }
}

#[test]
fn embedded_assets_load_and_obey_mona_network_policy_over_real_ipc() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path();
    for asset in enderpin::auth::bridge::ASSETS {
        std::fs::write(output.join(asset.name), asset.bytes).unwrap();
    }
    assert!(ProcessCommand::new("javac")
        .args(["--release", "17", "-d"])
        .arg(output)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/java/auth-bridge-smoke/SharedAssetsSmoke.java"
        ))
        .status()
        .unwrap()
        .success());
    for network in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let broker = PreparedBroker::new(Box::new(Fixture(Arc::clone(&calls))), network).unwrap();
        let mut command = ProcessCommand::new("java");
        command
            .arg(format!(
                "-javaagent:{}={}",
                output.join(enderpin::auth::bridge::AGENT.name).display(),
                output.join(enderpin::auth::bridge::NATIVE.name).display()
            ))
            .arg("-cp")
            .arg(output)
            .arg("SharedAssetsSmoke")
            .arg(if network { "allowed" } else { "denied" });
        broker.configure(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        let _guard = broker.into_guard();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "shared bridge JVM failed ({network})");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("shared bridge JVM timed out ({network})");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(calls.load(Ordering::Relaxed), usize::from(network));
    }
}
