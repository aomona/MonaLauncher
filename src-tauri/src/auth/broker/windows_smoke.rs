//! Windows CI probe: actual AppContainer process creation, inherited handle, JVM and JNI.
use super::{
    channel::PreparedBroker,
    chat::{
        tests::{certificate, ACCOUNT},
        ChatKeys,
    },
    protocol::{BrokerError, Command},
    service::Operations,
};
use crate::platform::windows::{
    appcontainer_process::launch_with_network, appcontainer_profile::ensure_appcontainer_profile,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::Read,
    os::windows::io::AsRawHandle,
    path::Path,
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
pub(super) fn grant_read(path: &Path, sid: &str) {
    assert!(
        ProcessCommand::new("icacls.exe")
            .arg(path)
            .args(["/grant", &format!("*{sid}:(OI)(CI)RX"), "/T", "/Q"])
            .output()
            .unwrap()
            .status
            .success(),
        "grant probe runtime read access"
    );
}
pub(super) struct ProbeProfile {
    pub(super) profile: crate::platform::windows::appcontainer_profile::AppContainerProfile,
    java_home: std::path::PathBuf,
}
impl ProbeProfile {
    pub(super) fn new(java_home: &Path, suffix: &str) -> Self {
        let profile =
            ensure_appcontainer_profile(&format!("auth-ipc-{}-{suffix}", std::process::id()))
                .unwrap();
        assert!(profile.created, "probe must use a new disposable profile");
        Self {
            profile,
            java_home: java_home.to_owned(),
        }
    }
}
impl Drop for ProbeProfile {
    fn drop(&mut self) {
        let acl = ProcessCommand::new("icacls.exe")
            .arg(&self.java_home)
            .args(["/remove:g", &format!("*{}", self.profile.sid), "/T", "/Q"])
            .output();
        let name: Vec<u16> = self.profile.name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a NUL-terminated name of a dedicated profile created by this test only.
        let deleted = unsafe {
            windows::Win32::Security::Isolation::DeleteAppContainerProfile(windows::core::PCWSTR(
                name.as_ptr(),
            ))
        };
        if !std::thread::panicking() {
            assert!(
                acl.is_ok_and(|output| output.status.success()),
                "remove disposable JDK ACL"
            );
            assert!(deleted.is_ok(), "delete disposable AppContainer profile");
        }
    }
}
#[test]
#[ignore = "requires Windows, JAVA_HOME JDK 21+, javac; creates dedicated AppContainer profiles and grants them JDK read access"]
fn appcontainer_java_uses_inherited_auth_channel() {
    let java_home = std::path::PathBuf::from(std::env::var_os("JAVA_HOME").expect("JAVA_HOME"));
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    for (name, bytes) in [
        (
            "auth-bridge.jar",
            include_bytes!(concat!(env!("OUT_DIR"), "/auth-bridge.jar")).as_slice(),
        ),
        (
            "auth-bootstrap.jar",
            include_bytes!(concat!(env!("OUT_DIR"), "/auth-bootstrap.jar")).as_slice(),
        ),
        (
            "auth-bridge.dll",
            include_bytes!(concat!(env!("OUT_DIR"), "/auth-bridge.dll")).as_slice(),
        ),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    assert!(ProcessCommand::new(java_home.join("bin/javac.exe"))
        .args(["--release", "21", "-d"])
        .arg(root)
        .arg("java/auth-bridge-smoke/WindowsIpcSmoke.java")
        .status()
        .unwrap()
        .success());
    let public = certificate(SystemTime::now())["keyPair"]["publicKey"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(root.join("public.pem"), public).unwrap();
    let mut foreign_keys = ChatKeys::default();
    let foreign = foreign_keys
        .install(certificate(SystemTime::now()), SystemTime::now())
        .unwrap();
    let foreign_id = foreign["keyPair"]["privateKey"]
        .as_str()
        .unwrap()
        .strip_prefix("MONALAUNCHER_REMOTE_CHAT_KEY:")
        .unwrap();
    for network in [false, true] {
        let profile_guard = ProbeProfile::new(&java_home, if network { "on" } else { "off" });
        let profile = &profile_guard.profile;
        grant_read(root, &profile.sid);
        grant_read(&java_home, &profile.sid);
        let broker = PreparedBroker::new(Box::new(Fixture(ChatKeys::default())), network).unwrap();
        let mut arguments: Vec<OsString> = vec![
            format!(
                "-Dmonalauncher.auth.handle={}",
                broker.child_handle().as_raw_handle() as usize
            )
            .into(),
            format!(
                "-javaagent:{}={}",
                root.join("auth-bridge.jar").display(),
                root.join("auth-bridge.dll").display()
            )
            .into(),
            "-cp".into(),
            root.as_os_str().to_owned(),
            "WindowsIpcSmoke".into(),
            if network {
                "allowed".into()
            } else {
                "denied".into()
            },
            root.join("public.pem").into_os_string(),
            foreign_id.into(),
        ];
        // The broker uses a local synthetic operation. Neither JVM has any network capability.
        let drive =
            crate::platform::windows::sandbox_drive::SandboxDrive::create(&java_home).unwrap();
        arguments.insert(0, format!("-Djava.home={}", drive.root().display()).into());
        let mut child = launch_with_network(
            &profile.name,
            &drive.root().join("bin/java.exe"),
            &arguments,
            root,
            false,
            Some(&broker),
        )
        .unwrap();
        child.retain_sandbox_drive(drive);
        child.retain_auth_broker(broker.into_guard());
        assert!(child.token_info.is_app_container);
        let readers: Vec<_> = [child.take_stdout().unwrap(), child.take_stderr().unwrap()]
            .into_iter()
            .map(|mut file| {
                std::thread::spawn(move || {
                    let mut output = String::new();
                    file.read_to_string(&mut output).unwrap();
                    output
                })
            })
            .collect();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "AppContainer JNI probe timed out"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        let output = readers
            .into_iter()
            .map(|reader| reader.join().unwrap())
            .collect::<String>();
        assert!(
            status.success(),
            "synthetic AppContainer probe failed: {output}"
        );
        let marker = if network {
            "WINDOWS_AUTH_JNI_SIGNATURE_AND_FOREIGN_KEY_OK"
        } else {
            "WINDOWS_AUTH_NETWORK_DENIED_OK"
        };
        assert!(
            output.contains(marker),
            "missing synthetic probe result: {output}"
        );
        println!("{marker}; AppContainer token verified");
    }
}
