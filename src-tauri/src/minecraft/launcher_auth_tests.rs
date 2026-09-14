//! Startup regression paired with the opt-in official-class IPC test.
use super::*;
use crate::auth::broker::{
    protocol::BrokerError,
    service::{SessionSource, UserAttributesSchema},
};
use crate::auth::minecraft_services::MinecraftSession;
use std::cell::Cell;

struct NoAccount;
impl SessionSource for NoAccount {
    fn valid(&self) -> bool {
        true
    }
    fn current(&self) -> Result<MinecraftSession, BrokerError> {
        panic!("startup validation must not fetch account credentials")
    }
}

pub(crate) fn assert_brokered_startup(installed_root: &Path, version: &str) {
    let (major, schema) = match version {
        "1.21.8" => (21, UserAttributesSchema::Authlib6),
        "26.2" => (25, UserAttributesSchema::Authlib9),
        _ => panic!("unsupported test version"),
    };
    let temporary = tempfile::tempdir().unwrap();
    let paths = MinecraftPaths::new(temporary.path().to_owned());
    let installed = MinecraftPaths::new(installed_root.to_owned());
    let metadata = fs::read(installed.version_json(version)).unwrap();
    let parsed: VersionMetadata = serde_json::from_slice(&metadata).unwrap();
    fs::create_dir_all(paths.version_directory(version)).unwrap();
    fs::write(paths.version_json(version), metadata).unwrap();
    for library in &parsed.libraries {
        if library.name.starts_with("com.mojang:authlib:") {
            let relative = library
                .downloads
                .artifact
                .as_ref()
                .unwrap()
                .path
                .as_ref()
                .unwrap();
            let target = safe_library_path(&paths, relative).unwrap();
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(safe_library_path(&installed, relative).unwrap(), target).unwrap();
        }
    }
    // Only the managed runtime's path is consulted before the factory. This file must
    // never execute: the injected factory returns an error before sandbox preparation.
    let java = paths
        .runtimes()
        .join(format!("temurin-{major}"))
        .join("0".repeat(64))
        .join("bin")
        .join(super::super::model::java_executable_name());
    fs::create_dir_all(java.parent().unwrap()).unwrap();
    fs::write(&java, b"not an executable").unwrap();
    let id = "broker-startup-regression";
    fs::create_dir_all(paths.instance(id)).unwrap();
    let permissions = super::super::permissions::InstancePermissions {
        account_authentication: super::super::permissions::AccountAuthentication::Brokered,
        ..Default::default()
    };
    let manifest = serde_json::json!({
        "id":id, "name":"Broker startup regression", "versionId":version,
        "javaPath":java, "gameDirectory":paths.instance_game_directory(id),
        "demo":false, "sandboxed":true,
        "permissions":permissions
    });
    fs::write(
        paths.instance_manifest(id),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let identity = MinecraftIdentity {
        player_name: "StartupProbe".into(),
        uuid: "00000000000000000000000000000001".into(),
        broker: Some(Arc::new(NoAccount)),
    };
    let reached = Cell::new(false);
    let result = spawn_instance_with_factory(
        &paths,
        id,
        Some(&identity),
        LaunchMode::Default,
        |_, uuid, actual| {
            reached.set(true);
            assert_eq!(uuid, identity.uuid);
            assert!(matches!(
                (actual, schema),
                (
                    UserAttributesSchema::Authlib6,
                    UserAttributesSchema::Authlib6
                ) | (
                    UserAttributesSchema::Authlib9,
                    UserAttributesSchema::Authlib9
                )
            ));
            Err(BrokerError::Unsupported)
        },
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("the sentinel factory must prevent process creation"),
    };
    assert!(
        reached.get(),
        "normal launch did not reach broker creation: {error}"
    );
    assert!(matches!(error, MinecraftLaunchError::Sandbox(ref message)
        if message == &BrokerError::Unsupported.to_string()));
    println!(
        "BROKER_NORMAL_STARTUP_OK {version}; official authlib validated before factory sentinel"
    );
}
