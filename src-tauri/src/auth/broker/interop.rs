//! Opt-in integration against installed official jars, with synthetic keys and no service traffic.
use super::{
    channel::PreparedBroker,
    chat::{
        tests::{certificate, ACCOUNT},
        ChatKeys,
    },
    protocol::{BrokerError, Command},
    service::Operations,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    process::Command as ProcessCommand,
    time::{Duration, Instant, SystemTime},
};

struct Fixture(ChatKeys);
impl Operations for Fixture {
    fn valid(&self) -> bool {
        true
    }
    fn execute(&mut self, command: &Command) -> Result<Value, BrokerError> {
        match command {
            Command::Certificate {} => self
                .0
                .install(certificate(SystemTime::now()), SystemTime::now()),
            Command::Sign { key_id, message } => {
                self.0.sign(key_id, message, ACCOUNT, SystemTime::now())
            }
            _ => Err(BrokerError::Unsupported),
        }
    }
}

#[test]
#[ignore = "requires MONALAUNCHER_CHAT_SMOKE_ROOT, MONALAUNCHER_CHAT_SMOKE_JAVA and MONALAUNCHER_CHAT_SMOKE_VERSION; official jars already installed"]
fn official_game_chat_signer_uses_opaque_key_over_real_ipc() {
    let root = PathBuf::from(std::env::var_os("MONALAUNCHER_CHAT_SMOKE_ROOT").expect("game root"));
    let java =
        PathBuf::from(std::env::var_os("MONALAUNCHER_CHAT_SMOKE_JAVA").expect("Java executable"));
    let version = std::env::var("MONALAUNCHER_CHAT_SMOKE_VERSION").expect("game version");
    assert!(["1.21.8", "26.2"].contains(&version.as_str()));
    let metadata: Value = serde_json::from_slice(
        &std::fs::read(root.join(format!("versions/{version}/{version}.json"))).unwrap(),
    )
    .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path();
    assert!(ProcessCommand::new("javac")
        .args(["--release", "21", "-d"])
        .arg(output)
        .arg("java/auth-bridge-smoke/ChatSigningSmoke.java")
        .status()
        .unwrap()
        .success());
    let agent = output.join("auth-bridge.jar");
    let native = output.join(env!("MONALAUNCHER_AUTH_NATIVE"));
    std::fs::write(
        &agent,
        include_bytes!(concat!(env!("OUT_DIR"), "/auth-bridge.jar")),
    )
    .unwrap();
    std::fs::write(
        output.join("auth-bootstrap.jar"),
        include_bytes!(concat!(env!("OUT_DIR"), "/auth-bootstrap.jar")),
    )
    .unwrap();
    std::fs::write(
        &native,
        include_bytes!(concat!(
            env!("OUT_DIR"),
            "/",
            env!("MONALAUNCHER_AUTH_NATIVE")
        )),
    )
    .unwrap();
    let mut classpath = vec![
        output.to_path_buf(),
        root.join(format!("versions/{version}/{version}.jar")),
    ];
    for library in metadata["libraries"].as_array().unwrap() {
        if let Some(path) = library["downloads"]["artifact"]["path"].as_str() {
            let jar = root.join("libraries").join(path);
            if jar.is_file() {
                classpath.push(jar);
            }
        }
    }
    let prepared = PreparedBroker::new(Box::new(Fixture(ChatKeys::default())), true).unwrap();
    let mut command = ProcessCommand::new(java);
    command
        .arg(format!(
            "-javaagent:{}={}",
            agent.display(),
            native.display()
        ))
        .arg("-cp")
        .arg(std::env::join_paths(classpath).unwrap())
        .arg("me.aomona.authsmoke.ChatSigningSmoke")
        .arg(&version)
        .current_dir(output);
    prepared.configure(&mut command).unwrap();
    let mut child = command.spawn().unwrap();
    let guard = prepared.into_guard();
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Java chat smoke exceeded 60 seconds");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    drop(guard);
    assert!(status.success(), "official game chat signing smoke failed");
}
