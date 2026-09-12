use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!("mona-policy-{nonce:x?}"));
        for relative in [
            "runtimes/jdk/bin",
            "versions/selected",
            "assets",
            "libraries",
            "instances/one/game",
            "instances/one/launch/tmp",
        ] {
            std::fs::create_dir_all(root.join(relative)).unwrap();
        }
        Self(std::fs::canonicalize(root).unwrap())
    }
    fn resources(&self) -> SandboxResources {
        SandboxResources {
            data_root: self.0.clone(),
            runtimes_root: self.0.join("runtimes"),
            versions_root: self.0.join("versions"),
            version: self.0.join("versions/selected"),
            instance_root: self.0.join("instances/one"),
            java_home: self.0.join("runtimes/jdk"),
            libraries: self.0.join("libraries"),
            assets: self.0.join("assets"),
            game: self.0.join("instances/one/game"),
            launch: self.0.join("instances/one/launch"),
            temp: self.0.join("instances/one/launch/tmp"),
        }
    }
    fn policy(&self) -> SandboxPolicy {
        SandboxPolicy::minecraft(self.resources()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn both_backends_use_the_same_application_grants() {
    let fixture = Fixture::new();
    let policy = fixture.policy();
    for backend in [
        Backend::AppContainer,
        Backend::Seatbelt,
        Backend::Bubblewrap,
    ] {
        let plan = policy.compile(backend).unwrap();
        for (resource, requested) in policy.requested_files() {
            let expected = if backend == Backend::AppContainer && *resource == Resource::Launch {
                FileAccess::ReadWrite
            } else {
                *requested
            };
            assert!(plan
                .files
                .iter()
                .any(|grant| grant.path == policy.resources().path(*resource)
                    && grant.access == expected));
        }
    }
}

#[test]
fn windows_compatibility_is_explicit_and_can_be_rejected() {
    let fixture = Fixture::new();
    let mut policy = fixture.policy();
    let plan = policy.compile(Backend::AppContainer).unwrap();
    assert_eq!(
        plan.exceptions,
        [
            BackendException::WindowsWritableLaunch,
            BackendException::WindowsInstanceMetadataRead,
            BackendException::WindowsAllVersionsRead,
            BackendException::WindowsPrivateProfileStorage
        ]
    );
    assert!(policy
        .compile(Backend::Seatbelt)
        .unwrap()
        .exceptions
        .is_empty());
    policy.allow_windows_compatibility = false;
    assert!(policy.compile(Backend::AppContainer).is_err());
    assert!(policy.compile(Backend::Seatbelt).is_ok());
}

#[test]
fn unsupported_requests_fail_closed_on_each_backend() {
    let fixture = Fixture::new();
    for backend in [
        Backend::AppContainer,
        Backend::Seatbelt,
        Backend::Bubblewrap,
    ] {
        let mut policy = fixture.policy();
        policy.network = NetworkAccess::Internet;
        assert!(policy.compile(backend).is_err());
        let mut policy = fixture.policy();
        policy.desktop.audio_output = false;
        assert!(policy.compile(backend).is_err());
        let mut policy = fixture.policy();
        policy.desktop.window_and_input = false;
        assert!(policy.compile(backend).is_err());
    }
}

#[test]
fn linux_desktop_compatibility_is_explicit_and_can_be_rejected() {
    let fixture = Fixture::new();
    let mut policy = fixture.policy();
    assert_eq!(
        policy.compile(Backend::Bubblewrap).unwrap().exceptions,
        [
            BackendException::LinuxSystemRuntimeRead,
            BackendException::LinuxX11PeerAccess,
            BackendException::LinuxPulseAudioServiceAccess,
        ]
    );
    policy.allow_linux_desktop_compatibility = false;
    assert!(policy.compile(Backend::Bubblewrap).is_err());
    assert!(policy.compile(Backend::Seatbelt).is_ok());
}

#[test]
fn game_read_only_is_translated_by_both_backends() {
    let fixture = Fixture::new();
    let policy = fixture
        .policy()
        .with_file_access(Resource::Game, FileAccess::ReadOnly)
        .unwrap();
    for backend in [
        Backend::AppContainer,
        Backend::Seatbelt,
        Backend::Bubblewrap,
    ] {
        let plan = policy.compile(backend).unwrap();
        assert!(plan
            .files
            .iter()
            .any(|g| g.path == policy.resources().game && g.access == FileAccess::ReadOnly));
    }
    let profile = seatbelt::render(&policy).unwrap();
    assert!(profile.contains("(allow file-read* (subpath (param \"GAME\")))"));
    assert!(!profile.contains("file-write* (subpath (param \"GAME\"))"));
    assert!(
        !profile.contains(fixture.0.to_str().unwrap()),
        "paths must not be interpolated into SBPL"
    );
}

#[test]
fn rejects_writable_code_or_missing_and_overlapping_resources() {
    let fixture = Fixture::new();
    assert!(fixture
        .policy()
        .with_file_access(Resource::Libraries, FileAccess::ReadWrite)
        .is_err());
    let mut resources = fixture.resources();
    resources.game = resources.instance_root.clone();
    assert!(SandboxPolicy::minecraft(resources).is_err());
    let mut resources = fixture.resources();
    resources.launch = resources.game.clone();
    assert!(SandboxPolicy::minecraft(resources).is_err());
    let mut resources = fixture.resources();
    resources.version = fixture.0.join("absent");
    assert!(SandboxPolicy::minecraft(resources).is_err());
}

#[test]
fn narrator_permission_is_carried_to_both_backends() {
    let fixture = Fixture::new();
    let mut policy = fixture.policy();
    policy.narrator = false;
    for backend in [
        Backend::AppContainer,
        Backend::Seatbelt,
        Backend::Bubblewrap,
    ] {
        assert!(!policy.compile(backend).unwrap().narrator);
    }
}

#[test]
fn windows_rejects_read_only_temp_instead_of_inheriting_write_access() {
    let fixture = Fixture::new();
    let policy = fixture
        .policy()
        .with_file_access(Resource::Temp, FileAccess::ReadOnly)
        .unwrap();
    assert!(policy.compile(Backend::AppContainer).is_err());
    assert!(policy.compile(Backend::Seatbelt).is_ok());
}

#[cfg(unix)]
#[test]
fn resolves_aliases_before_checking_writable_boundaries() {
    let fixture = Fixture::new();
    let mut resources = fixture.resources();
    let link = resources.instance_root.join("game-alias");
    std::os::unix::fs::symlink(&resources.java_home, &link).unwrap();
    resources.game = link;
    assert!(SandboxPolicy::minecraft(resources).is_err());
}
