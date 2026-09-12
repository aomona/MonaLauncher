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
        assert!(policy.compile(backend).is_ok());
        let mut policy = fixture.policy();
        policy.desktop.audio_output = false;
        assert_eq!(
            policy.compile(backend).is_err(),
            backend == Backend::AppContainer
        );
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
    let wayland = policy.compile_linux(LinuxDisplayProtocol::Wayland).unwrap();
    assert!(wayland
        .exceptions
        .contains(&BackendException::LinuxGpuIdentificationRead));
    assert!(wayland
        .exceptions
        .contains(&BackendException::LinuxWaylandCompositorAccess));
    assert!(!wayland
        .exceptions
        .contains(&BackendException::LinuxX11PeerAccess));
    let x11 = policy.compile_linux(LinuxDisplayProtocol::X11).unwrap();
    for (a, b) in wayland.files.iter().zip(&x11.files) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.access, b.access);
    }
    policy.allow_linux_desktop_compatibility = false;
    assert!(policy.compile(Backend::Bubblewrap).is_err());
    assert!(policy.compile_linux(LinuxDisplayProtocol::Wayland).is_err());
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

#[test]
fn granular_files_and_service_requests_are_translated_without_widening_other_paths() {
    let fixture = Fixture::new();
    let mut policy = fixture
        .policy()
        .with_readonly_game_directories(&[GameDirectory::Worlds, GameDirectory::Mods])
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
            .any(|p| p.path == policy.resources().game && p.access == FileAccess::ReadWrite));
        for name in ["saves", "mods"] {
            assert!(plan
                .files
                .iter()
                .any(|p| p.path == policy.resources().game.join(name)
                    && p.access == FileAccess::ReadOnly));
        }
    }
    policy.desktop.audio_output = false;
    let profile = seatbelt::render(&policy).unwrap();
    assert!(!profile.contains("com.apple.audio.audiohald"));
    assert!(!profile.contains("com.apple.pasteboard.1"));
    policy.desktop.audio_output = true;
    policy.desktop.microphone = true;
    policy.desktop.clipboard = true;
    let profile = seatbelt::render(&policy).unwrap();
    assert!(profile.contains("(allow device-microphone)"));
    assert!(profile.contains("com.apple.pasteboard.1"));
    assert!(policy.compile(Backend::AppContainer).is_err());
    assert!(policy.compile(Backend::Bubblewrap).is_err());
}

#[cfg(unix)]
#[test]
fn granular_permissions_reject_preexisting_aliases() {
    let fixture = Fixture::new();
    let game = &fixture.policy().resources().game.clone();
    std::fs::write(game.join("original"), "fixture").unwrap();
    std::fs::hard_link(game.join("original"), game.join("alias")).unwrap();
    assert!(fixture
        .policy()
        .with_readonly_game_directories(&[GameDirectory::Worlds])
        .is_err());
    std::fs::remove_file(game.join("alias")).unwrap();
    std::os::unix::fs::symlink(game.join("original"), game.join("alias")).unwrap();
    assert!(fixture
        .policy()
        .with_readonly_game_directories(&[GameDirectory::Worlds])
        .is_err());
}

#[test]
fn runtime_cache_grants_are_owned_and_independent_of_game_write() {
    let fixture = Fixture::new();
    let mut policy = fixture
        .policy()
        .with_file_access(Resource::Game, FileAccess::ReadOnly)
        .unwrap();
    let caches = runtime_cache::RuntimeCaches::prepare(policy.resources()).unwrap();
    policy.caches = Some(caches.clone());
    for enabled in [false, true] {
        policy.skin_cache = enabled;
        policy.graphics_cache = enabled;
        policy.desktop.integration = enabled;
        for backend in [
            Backend::Seatbelt,
            Backend::Bubblewrap,
            Backend::AppContainer,
        ] {
            if !enabled && backend != Backend::Seatbelt {
                assert!(policy.compile(backend).is_err());
                continue;
            }
            let plan = policy.compile(backend).unwrap();
            assert!(plan.files.iter().any(|f| f.path == caches.skins
                && f.access
                    == if enabled {
                        FileAccess::ReadWrite
                    } else {
                        FileAccess::ReadOnly
                    }));
            assert!(plan
                .files
                .iter()
                .any(|f| f.path == caches.assets && f.access == FileAccess::ReadOnly));
            assert!(plan
                .files
                .iter()
                .any(|f| f.path == policy.resources().assets && f.access == FileAccess::ReadOnly));
        }
        let profile = seatbelt::render(&policy).unwrap();
        assert_eq!(
            profile.contains("TextInputUI.xpc.CursorUIViewService"),
            enabled
        );
        assert_eq!(profile.contains("JAVA_METAL_CACHE"), enabled);
    }
}

#[cfg(unix)]
#[test]
fn runtime_cache_aliases_are_rejected() {
    let fixture = Fixture::new();
    let resources = fixture.resources();
    std::os::unix::fs::symlink(
        &resources.assets,
        resources.instance_root.join("runtime-cache"),
    )
    .unwrap();
    assert!(runtime_cache::RuntimeCaches::prepare(&resources).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn seatbelt_runtime_cache_writes_can_be_revoked_and_restored() {
    let fixture = Fixture::new();
    let mut resources = fixture.resources();
    resources.runtimes_root = "/bin".into();
    resources.java_home = "/bin".into();
    let mut policy = SandboxPolicy::minecraft(resources)
        .unwrap()
        .with_file_access(Resource::Game, FileAccess::ReadOnly)
        .unwrap();
    let caches = runtime_cache::RuntimeCaches::prepare(policy.resources()).unwrap();
    std::fs::write(caches.skins.join("fixture"), "skin").unwrap();
    std::fs::write(caches.assets.join("immutable"), "asset").unwrap();
    policy.caches = Some(caches.clone());
    for enabled in [true, false, true] {
        policy.skin_cache = enabled;
        policy.graphics_cache = enabled;
        let output = crate::platform::macos::command(Path::new("/bin/sh"), &policy).unwrap()
            .args(["-c", "cat \"$1/skins/fixture\" >/dev/null || exit 10; if echo x > \"$1/immutable\"; then exit 11; fi; if echo x > \"$1/skins/new\"; then skin=1; else skin=0; fi; if echo x > \"$2/new\"; then graphics=1; else graphics=0; fi; test \"$skin:$graphics\" = \"$3:$3\"", "probe"])
            .arg(&caches.assets).arg(&caches.graphics).arg(if enabled { "1" } else { "0" })
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
