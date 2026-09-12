use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitStatus;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

use zip::ZipArchive;

use super::fabric::{
    load_fabric_profile, maven_artifact_path, validate_profile, FabricError, FabricProfile,
};
#[cfg(windows)]
use super::file_io::path_is_link_or_reparse;
use super::file_io::read_bounded_file;
use super::installer::{
    load_instance as load_instance_manifest, managed_java_major as installed_java_major,
};
use super::model::{
    rules_allow, Argument, ArgumentValue, InstanceManifest, ModLoader, VersionMetadata,
};
use super::paths::MinecraftPaths;

const MAX_NATIVE_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_NATIVE_FILE_SIZE: u64 = 128 * 1024 * 1024;
const MAX_NATIVE_TOTAL_SIZE: u64 = 512 * 1024 * 1024;
#[cfg(windows)]
const MAX_JNA_DISPATCH_SIZE: u64 = 64 * 1024 * 1024;
const MAX_LOCAL_VERSION_METADATA_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum MinecraftLaunchError {
    InstanceNotFound(String),
    UnsafeLibraryPath(String),
    ArchiveTooLarge(String),
    MissingFile(PathBuf),
    IncompatibleJava { required: u32, found: String },
    Io(std::io::Error),
    Json(serde_json::Error),
    Install(super::installer::MinecraftInstallError),
    Fabric(FabricError),
    Sandbox(String),
    SandboxedProcessNotIsolated,
    SandboxRequired,
}

impl fmt::Display for MinecraftLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstanceNotFound(id) => write!(formatter, "instance was not found: {id}"),
            Self::UnsafeLibraryPath(path) => {
                write!(
                    formatter,
                    "version metadata contains an unsafe path: {path}"
                )
            }
            Self::ArchiveTooLarge(reason) => {
                write!(formatter, "native library archive is unsafe: {reason}")
            }
            Self::MissingFile(path) => {
                write!(formatter, "required file is missing: {}", path.display())
            }
            Self::IncompatibleJava { required, found } => write!(
                formatter,
                "Minecraft requires Java {required}, but the selected runtime reported: {found}"
            ),
            Self::Io(error) => write!(formatter, "process or file system error: {error}"),
            Self::Json(error) => write!(formatter, "version metadata error: {error}"),
            Self::Install(error) => write!(formatter, "instance metadata error: {error}"),
            Self::Fabric(error) => write!(formatter, "Fabric起動設定エラー: {error}"),
            Self::Sandbox(error) => write!(formatter, "sandbox launch error: {error}"),
            Self::SandboxedProcessNotIsolated => {
                write!(formatter, "Minecraft did not receive an AppContainer token")
            }
            Self::SandboxRequired => write!(
                formatter,
                "安全でない通常起動は無効です。インスタンスを再作成してください"
            ),
        }
    }
}

impl Error for MinecraftLaunchError {}

impl From<std::io::Error> for MinecraftLaunchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for MinecraftLaunchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<super::installer::MinecraftInstallError> for MinecraftLaunchError {
    fn from(error: super::installer::MinecraftInstallError) -> Self {
        Self::Install(error)
    }
}

impl From<FabricError> for MinecraftLaunchError {
    fn from(error: FabricError) -> Self {
        Self::Fabric(error)
    }
}

pub enum MinecraftProcess {
    #[cfg(target_os = "linux")]
    Bubblewrap(crate::platform::linux::BubblewrapProcess),
    #[cfg(target_os = "macos")]
    Seatbelt(crate::platform::macos::SeatbeltProcess),
    #[cfg(windows)]
    Sandboxed(crate::platform::windows::appcontainer_process::SpawnedAppContainerProcess),
}

impl MinecraftProcess {
    pub fn id(&self) -> u32 {
        match self {
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(child) => child.id(),
            #[cfg(target_os = "macos")]
            Self::Seatbelt(child) => child.id(),
            #[cfg(windows)]
            Self::Sandboxed(child) => child.id(),
        }
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, MinecraftLaunchError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(child) => child.try_wait().map_err(MinecraftLaunchError::Io),
            #[cfg(target_os = "macos")]
            Self::Seatbelt(child) => child.try_wait().map_err(MinecraftLaunchError::Io),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .try_wait()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }

    pub fn kill(&mut self) -> Result<(), MinecraftLaunchError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(child) => child.kill().map_err(MinecraftLaunchError::Io),
            #[cfg(target_os = "macos")]
            Self::Seatbelt(child) => child.kill().map_err(MinecraftLaunchError::Io),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .kill()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }

    pub fn wait(&mut self) -> Result<ExitStatus, MinecraftLaunchError> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(child) => child.wait().map_err(MinecraftLaunchError::Io),
            #[cfg(target_os = "macos")]
            Self::Seatbelt(child) => child.wait().map_err(MinecraftLaunchError::Io),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .wait()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }
}

impl Drop for MinecraftProcess {
    fn drop(&mut self) {
        // Enforce ownership here so registry or monitoring failures cannot turn a launched game
        // into an untracked process.
        if self.try_wait().ok().flatten().is_none() {
            let _ = self.kill();
            let _ = self.wait();
        }
    }
}

pub struct SpawnedMinecraft {
    pub child: MinecraftProcess,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Box<dyn Read + Send>,
    pub sandboxed: bool,
    pub narrator_token: Option<String>,
    #[cfg(windows)]
    pub cursor_broker: Option<Arc<crate::platform::windows::cursor_broker::CursorBroker>>,
}

#[derive(Debug, Clone)]
pub struct MinecraftIdentity {
    pub player_name: String,
    pub uuid: String,
}

#[derive(Debug)]
struct SandboxLayout {
    #[cfg(windows)]
    profile_name: String,
    #[cfg(windows)]
    sid: String,
    launch_root: PathBuf,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    cleanup_on_drop: bool,
    physical_root: PathBuf,
    virtual_root: PathBuf,
    #[cfg(windows)]
    drive: Option<crate::platform::windows::sandbox_drive::SandboxDrive>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for SandboxLayout {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = fs::remove_dir_all(&self.launch_root);
        }
    }
}

pub fn spawn_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
    identity: Option<&MinecraftIdentity>,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    let instance = load_instance(paths, instance_id)?;
    if !instance.sandboxed {
        return Err(MinecraftLaunchError::SandboxRequired);
    }
    let version_path = paths.version_json(&instance.version_id);
    require_file(&version_path)?;

    let version = serde_json::from_slice::<VersionMetadata>(&read_bounded_file(
        &version_path,
        MAX_LOCAL_VERSION_METADATA_SIZE,
    )?)?
    .for_current_platform()?;
    let fabric = load_instance_fabric_profile(paths, &instance)?;
    let detected_java_major = installed_java_major(paths, Path::new(&instance.java_path))?;
    if let Some(java_version) = &version.java_version {
        if detected_java_major < java_version.major_version {
            return Err(MinecraftLaunchError::IncompatibleJava {
                required: java_version.major_version,
                found: format!("Java {detected_java_major}"),
            });
        }
    }
    let mut sandbox = prepare_sandbox_layout(paths, &instance)?;
    let narrator_token = sandbox
        .as_ref()
        .map(|_| generate_narrator_token())
        .transpose()?;
    let sandbox_narrator_token = narrator_token.as_deref().ok_or_else(|| {
        MinecraftLaunchError::Sandbox("sandbox layout has no narrator token".to_owned())
    })?;
    let mut classpath = fabric
        .as_ref()
        .map(|profile| build_fabric_classpath(paths, profile))
        .transpose()?
        .unwrap_or_default();
    classpath.extend(build_classpath(paths, &version, instance.demo)?);
    let source_client_jar = paths.version_jar(&version.id);
    require_file(&source_client_jar)?;

    let physical_game_directory = PathBuf::from(&instance.game_directory);
    fs::create_dir_all(&physical_game_directory)?;
    let game_directory = sandbox_path(&sandbox, &physical_game_directory)?;
    let physical_natives_directory = sandbox
        .as_ref()
        .map(|layout| layout.launch_root.join("natives"))
        .unwrap_or_else(|| paths.instance(instance_id).join("natives"));
    fs::create_dir_all(&physical_natives_directory)?;
    let natives_directory = sandbox_path(&sandbox, &physical_natives_directory)?;

    let client_entry = if sandbox.is_some() {
        extract_native_libraries(paths, &version, &physical_natives_directory)?;
        #[cfg(windows)]
        extract_jna_dispatch(paths, &version, &physical_natives_directory.join("jna"))?;
        sandbox_path(&sandbox, &source_client_jar)?
    } else {
        source_client_jar
    };

    let launch_root = &sandbox.as_ref().expect("sandbox prepared").launch_root;
    fs::create_dir_all(launch_root.join("tmp"))?;
    let policy = super::sandbox_policy::policy_for_instance(paths, &instance, launch_root)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let narrator_bridge = prepare_narrator_bridge(&sandbox)?;
    #[cfg(windows)]
    let cursor_agent = prepare_cursor_agent(&sandbox)?;
    let mut classpath_entries = narrator_bridge.into_iter().collect::<Vec<_>>();
    classpath_entries.extend(
        classpath
            .iter()
            .map(|path| sandbox_path(&sandbox, path))
            .collect::<Result<Vec<_>, _>>()?,
    );
    classpath_entries.push(client_entry);
    let classpath = classpath_entries
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(classpath_separator());

    let player_name = identity
        .map(|identity| identity.player_name.clone())
        .unwrap_or_else(|| {
            if instance.demo {
                "DemoPlayer"
            } else {
                "Player"
            }
            .to_owned()
        });
    let uuid = identity
        .map(|identity| identity.uuid.clone())
        .unwrap_or_else(|| "00000000000000000000000000000000".to_owned());
    let substitutions = HashMap::from([
        ("${auth_player_name}", player_name),
        (
            "${version_name}",
            fabric
                .as_ref()
                .map(|profile| profile.id.clone())
                .unwrap_or_else(|| version.id.clone()),
        ),
        (
            "${game_directory}",
            game_directory.to_string_lossy().into_owned(),
        ),
        (
            "${assets_root}",
            sandbox_path(&sandbox, &paths.assets())?
                .to_string_lossy()
                .into_owned(),
        ),
        ("${assets_index_name}", version.assets.clone()),
        ("${auth_uuid}", uuid),
        ("${auth_access_token}", "0".to_owned()),
        ("${clientid}", String::new()),
        ("${auth_xuid}", String::new()),
        ("${user_type}", "legacy".to_owned()),
        ("${version_type}", version.version_type.clone()),
        (
            "${natives_directory}",
            natives_directory.to_string_lossy().into_owned(),
        ),
        ("${launcher_name}", "MonaLauncher".to_owned()),
        ("${launcher_version}", env!("CARGO_PKG_VERSION").to_owned()),
        ("${classpath}", classpath.clone()),
        ("${classpath_separator}", classpath_separator().to_owned()),
        (
            "${library_directory}",
            sandbox_path(&sandbox, &paths.libraries())?
                .to_string_lossy()
                .into_owned(),
        ),
    ]);

    let features = HashMap::from([
        ("is_demo_user".to_owned(), instance.demo),
        ("has_custom_resolution".to_owned(), false),
        ("has_quick_plays_support".to_owned(), false),
        ("is_quick_play_singleplayer".to_owned(), false),
        ("is_quick_play_multiplayer".to_owned(), false),
        ("is_quick_play_realms".to_owned(), false),
    ]);

    let mut arguments = vec![OsString::from("-Xms512M"), OsString::from("-Xmx2G")];
    let jvm_arguments = if version.arguments.jvm.is_empty() {
        vec![
            format!(
                "-Djava.library.path={}",
                natives_directory.to_string_lossy()
            ),
            "-cp".to_owned(),
            classpath.clone(),
        ]
    } else {
        expand_arguments(&version.arguments.jvm, &features, &substitutions)
    };
    arguments.extend(jvm_arguments.into_iter().map(OsString::from));
    if let Some(profile) = &fabric {
        arguments.extend(
            expand_arguments(&profile.arguments.jvm, &features, &substitutions)
                .into_iter()
                .map(OsString::from),
        );
    }
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    if sandbox.is_some() {
        arguments.push(OsString::from(format!(
            "-Dmonalauncher.narrator.enabled={}",
            policy.narrator
        )));
        arguments.push(OsString::from(format!(
            "-Dmonalauncher.narrator.token={}",
            sandbox_narrator_token
        )));
        if std::env::var_os("MONALAUNCHER_EXPECT_NARRATOR").is_some() {
            arguments.push(OsString::from("-Dmonalauncher.narrator.smoke=true"));
        }
    }
    #[cfg(windows)]
    if sandbox.is_some() {
        // JNA cannot safely unpack through a SUBST alias in an AppContainer. The trusted launcher
        // extracts jnidispatch.dll first, then the child only loads it from its sandbox drive.
        arguments.push(OsString::from(format!(
            "-Djna.boot.library.path={}",
            natives_directory.join("jna").display()
        )));
        arguments.push(OsString::from("-Djna.nounpack=true"));
        arguments.push(OsString::from(format!(
            "-agentpath:{}={}",
            cursor_agent
                .as_ref()
                .expect("sandbox layout has a cursor agent")
                .display(),
            sandbox_narrator_token
        )));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if cfg!(target_os = "macos") && !arguments.iter().any(|arg| arg == "-XstartOnFirstThread") {
            arguments.push(OsString::from("-XstartOnFirstThread"));
        }
        let temp = sandbox
            .as_ref()
            .expect("sandbox prepared")
            .launch_root
            .join("tmp");
        for property in [
            "java.io.tmpdir",
            "jna.tmpdir",
            "org.lwjgl.system.SharedLibraryExtractPath",
            "io.netty.native.workdir",
        ] {
            arguments.push(OsString::from(format!("-D{property}={}", temp.display())));
        }
        arguments.push(OsString::from(format!(
            "-Duser.home={}",
            game_directory.display()
        )));
    }
    arguments.push(OsString::from(
        fabric
            .as_ref()
            .map(|profile| profile.main_class.as_str())
            .unwrap_or(&version.main_class),
    ));
    let mut game_arguments = version
        .minecraft_arguments
        .as_deref()
        .map(|value| {
            value
                .split_whitespace()
                .map(|argument| substitute(argument, &substitutions))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| expand_arguments(&version.arguments.game, &features, &substitutions));
    if let Some(profile) = &fabric {
        game_arguments.extend(expand_arguments(
            &profile.arguments.game,
            &features,
            &substitutions,
        ));
    }
    arguments.extend(game_arguments.into_iter().map(OsString::from));

    let sandbox = sandbox.as_mut().ok_or_else(|| {
        MinecraftLaunchError::Sandbox("sandbox layout was not prepared".to_owned())
    })?;
    spawn_sandboxed(
        paths,
        &instance,
        sandbox,
        &arguments,
        &game_directory,
        sandbox_narrator_token,
        &policy,
    )
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn prepare_sandbox_layout(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<Option<SandboxLayout>, MinecraftLaunchError> {
    use std::os::unix::fs::DirBuilderExt;
    let root = fs::canonicalize(paths.root())?;
    // A random, launcher-owned directory outside writable game storage avoids stale links.
    let launch_root = paths
        .instance(&instance.id)
        .join(format!("sandbox-{}", generate_narrator_token()?));
    fs::DirBuilder::new().mode(0o700).create(&launch_root)?;
    fs::create_dir(launch_root.join("tmp"))?;
    Ok(Some(SandboxLayout {
        launch_root: fs::canonicalize(launch_root)?,
        cleanup_on_drop: true,
        physical_root: root.clone(),
        virtual_root: root,
    }))
}

#[cfg(target_os = "macos")]
fn spawn_sandboxed(
    _paths: &MinecraftPaths,
    instance: &InstanceManifest,
    sandbox: &mut SandboxLayout,
    arguments: &[OsString],
    _game_directory: &Path,
    narrator_token: &str,
    policy: &crate::sandbox::SandboxPolicy,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    let java = fs::canonicalize(&instance.java_path)?;
    let command = crate::platform::macos::command(&java, policy)?;
    let mut child = crate::platform::macos::spawn(command, arguments, sandbox.launch_root.clone())?;
    sandbox.cleanup_on_drop = false; // The process now owns cleanup, including pipe setup failures.
    let stdout = child
        .take_stdout()
        .ok_or_else(|| MinecraftLaunchError::Sandbox("stdout is unavailable".to_owned()))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| MinecraftLaunchError::Sandbox("stderr is unavailable".to_owned()))?;
    Ok(SpawnedMinecraft {
        child: MinecraftProcess::Seatbelt(child),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        sandboxed: true,
        narrator_token: policy.narrator.then(|| narrator_token.to_owned()),
    })
}

#[cfg(windows)]
fn spawn_sandboxed(
    _paths: &MinecraftPaths,
    instance: &InstanceManifest,
    sandbox: &mut SandboxLayout,
    arguments: &[OsString],
    game_directory: &Path,
    narrator_token: &str,
    policy: &crate::sandbox::SandboxPolicy,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    use crate::platform::windows::appcontainer_process::launch_with_policy;
    use crate::platform::windows::sandbox_acl::grant_policy_access;

    grant_policy_access(policy, &sandbox.sid)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let java_path = sandbox_alias(sandbox, Path::new(&instance.java_path))?;
    let mut child = launch_with_policy(
        &sandbox.profile_name,
        &java_path,
        arguments,
        game_directory,
        policy,
    )
    .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    if !child.token_info.is_app_container {
        let _ = child.kill();
        return Err(MinecraftLaunchError::SandboxedProcessNotIsolated);
    }
    let cursor_broker = Arc::new(
        crate::platform::windows::cursor_broker::CursorBroker::start(
            child.id(),
            narrator_token.to_owned(),
        )
        .map_err(MinecraftLaunchError::Sandbox)?,
    );
    child.retain_cursor_broker(Arc::clone(&cursor_broker));
    child.retain_sandbox_drive(
        sandbox
            .drive
            .take()
            .expect("sandbox drive exists while launching"),
    );
    child.retain_cleanup_directory(sandbox.launch_root.clone());
    let Some(stdout) = child.take_stdout() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(MinecraftLaunchError::Sandbox(
            "sandboxed stdout is unavailable".to_owned(),
        ));
    };
    let Some(stderr) = child.take_stderr() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(MinecraftLaunchError::Sandbox(
            "sandboxed stderr is unavailable".to_owned(),
        ));
    };
    Ok(SpawnedMinecraft {
        child: MinecraftProcess::Sandboxed(child),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        sandboxed: true,
        narrator_token: policy.narrator.then(|| narrator_token.to_owned()),
        cursor_broker: Some(cursor_broker),
    })
}

#[cfg(target_os = "linux")]
fn spawn_sandboxed(
    _paths: &MinecraftPaths,
    instance: &InstanceManifest,
    sandbox: &mut SandboxLayout,
    arguments: &[OsString],
    _game_directory: &Path,
    narrator_token: &str,
    policy: &crate::sandbox::SandboxPolicy,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    let java = fs::canonicalize(&instance.java_path)?;
    let desktop = crate::platform::linux::Desktop::detect_with_audio(policy.desktop.audio_output)?;
    let arguments = linux_display_arguments(&instance.version_id, desktop.protocol(), arguments);
    let command = crate::platform::linux::prepare(&java, policy, Some(&desktop))?;
    let mut child = command.spawn(&arguments, sandbox.launch_root.clone())?;
    sandbox.cleanup_on_drop = false;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| MinecraftLaunchError::Sandbox("sandbox stdout is unavailable".into()))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| MinecraftLaunchError::Sandbox("sandbox stderr is unavailable".into()))?;
    Ok(SpawnedMinecraft {
        child: MinecraftProcess::Bubblewrap(child),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        sandboxed: true,
        narrator_token: policy.narrator.then(|| narrator_token.to_owned()),
    })
}

#[cfg(any(target_os = "linux", test))]
fn linux_display_arguments(
    version: &str,
    display: crate::sandbox::LinuxDisplayProtocol,
    arguments: &[OsString],
) -> Vec<OsString> {
    let mut result = Vec::new();
    // Verified against 26.2's GLX._initGlfw and SharedConstants: the game otherwise
    // explicitly selects X11 even when GLFW supports Wayland. Other debug flags
    // remain unset. Do not assume the same private flags exist in other versions.
    if version == "26.2" && display == crate::sandbox::LinuxDisplayProtocol::Wayland {
        result.extend([
            OsString::from("-DMC_DEBUG_ENABLED=true"),
            OsString::from("-DMC_DEBUG_PREFER_WAYLAND=true"),
        ]);
    }
    result.extend_from_slice(arguments);
    result
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn spawn_sandboxed(
    _paths: &MinecraftPaths,
    _instance: &InstanceManifest,
    _sandbox: &mut SandboxLayout,
    _arguments: &[OsString],
    _game_directory: &Path,
    _narrator_token: &str,
    _policy: &crate::sandbox::SandboxPolicy,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    Err(MinecraftLaunchError::Sandbox(
        "AppContainer is only available on Windows".to_owned(),
    ))
}

#[cfg(windows)]
fn prepare_sandbox_layout(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<Option<SandboxLayout>, MinecraftLaunchError> {
    use crate::platform::windows::appcontainer_profile::{
        ensure_appcontainer_profile, profile_name_for_instance,
    };

    if !instance.sandboxed {
        return Ok(None);
    }
    let profile_name = profile_name_for_instance(&instance.id)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let profile = ensure_appcontainer_profile(&profile_name)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    // `validate_managed_java` returns a canonical Windows path (usually prefixed with `\\?\`).
    // Keep the sandbox root in the same canonical form so all containment checks compare like
    // with like and cannot be bypassed through junctions or alternate path spellings.
    let physical_root = fs::canonicalize(paths.root())?;
    let drive = crate::platform::windows::sandbox_drive::SandboxDrive::create(&physical_root)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let virtual_root = drive.root().to_owned();
    let launches = paths.instance(&instance.id).join("sandbox-launches");
    reset_sandbox_launches_directory(&launches)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?
        .as_nanos();
    let launch_root = launches.join(format!("{}-{nonce}", std::process::id()));
    fs::create_dir(&launch_root)?;
    Ok(Some(SandboxLayout {
        profile_name,
        sid: profile.sid,
        launch_root,
        physical_root,
        virtual_root,
        drive: Some(drive),
    }))
}

#[cfg(windows)]
fn reset_sandbox_launches_directory(path: &Path) -> Result<(), MinecraftLaunchError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            // Older releases granted the AppContainer modify access to the instance root. A game
            // launched by such a release could have left a junction here. Never recurse through
            // or apply ACLs to a path whose destination is outside the validated instance root.
            if !metadata.is_dir() || path_is_link_or_reparse(path)? {
                return Err(MinecraftLaunchError::Sandbox(format!(
                    "sandbox launch workspace is not a regular directory: {}",
                    path.display()
                )));
            }
            // std::fs::remove_dir_all does not follow directory symlinks. The top-level path was
            // checked above as well, so stale per-launch contents can be removed safely.
            fs::remove_dir_all(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir(path)?;
    if path_is_link_or_reparse(path)? {
        let _ = fs::remove_dir(path);
        return Err(MinecraftLaunchError::Sandbox(format!(
            "sandbox launch workspace became a reparse point: {}",
            path.display()
        )));
    }
    Ok(())
}

fn sandbox_path(
    sandbox: &Option<SandboxLayout>,
    physical: &Path,
) -> Result<PathBuf, MinecraftLaunchError> {
    sandbox.as_ref().map_or_else(
        || Ok(physical.to_owned()),
        |layout| sandbox_alias(layout, physical),
    )
}

fn sandbox_alias(
    sandbox: &SandboxLayout,
    physical: &Path,
) -> Result<PathBuf, MinecraftLaunchError> {
    let canonical = fs::canonicalize(physical)?;
    let relative = canonical
        .strip_prefix(&sandbox.physical_root)
        .map_err(|_| {
            MinecraftLaunchError::Sandbox(format!(
                "sandbox path is outside Minecraft storage: {}",
                physical.display()
            ))
        })?;
    Ok(sandbox.virtual_root.join(relative))
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn prepare_narrator_bridge(
    sandbox: &Option<SandboxLayout>,
) -> Result<Option<PathBuf>, MinecraftLaunchError> {
    let Some(layout) = sandbox else {
        return Ok(None);
    };
    let physical = layout.launch_root.join("narrator-bridge.jar");
    fs::write(
        &physical,
        include_bytes!(concat!(env!("OUT_DIR"), "/narrator-bridge.jar")),
    )?;
    Ok(Some(sandbox_alias(layout, &physical)?))
}

#[cfg(windows)]
fn prepare_cursor_agent(
    sandbox: &Option<SandboxLayout>,
) -> Result<Option<PathBuf>, MinecraftLaunchError> {
    let Some(layout) = sandbox else {
        return Ok(None);
    };
    let physical = layout.launch_root.join("cursor-agent.dll");
    fs::write(
        &physical,
        include_bytes!(concat!(env!("OUT_DIR"), "/cursor-agent.dll")),
    )?;
    Ok(Some(sandbox_alias(layout, &physical)?))
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn prepare_narrator_bridge(
    _sandbox: &Option<SandboxLayout>,
) -> Result<Option<PathBuf>, MinecraftLaunchError> {
    Ok(None)
}

fn extract_native_libraries(
    paths: &MinecraftPaths,
    version: &VersionMetadata,
    destination: &Path,
) -> Result<(), MinecraftLaunchError> {
    for library in &version.libraries {
        if !rules_allow(library.rules.as_deref(), &HashMap::new()) {
            continue;
        }
        let Some(native) = library.platform_native() else {
            continue;
        };
        let Some(relative_path) = native.path.as_deref() else {
            continue;
        };
        let archive = safe_library_path(paths, relative_path)?;
        require_file(&archive)?;
        extract_archive(&archive, destination, |path| {
            path.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case(if cfg!(target_os = "macos") {
                    "dylib"
                } else if cfg!(target_os = "linux") {
                    "so"
                } else {
                    "dll"
                })
            })
        })?;
    }
    Ok(())
}

#[cfg(windows)]
fn extract_jna_dispatch(
    paths: &MinecraftPaths,
    version: &VersionMetadata,
    destination: &Path,
) -> Result<(), MinecraftLaunchError> {
    let jna = version
        .libraries
        .iter()
        .filter_map(|library| library.downloads.artifact.as_ref())
        .filter_map(|artifact| artifact.path.as_deref())
        .find(|path| is_jna_core_library_path(path))
        .ok_or_else(|| MinecraftLaunchError::Sandbox("JNA core library is missing".to_owned()))?;
    let archive = safe_library_path(paths, jna)?;
    require_file(&archive)?;

    let platform = if cfg!(target_arch = "x86_64") {
        "win32-x86-64"
    } else if cfg!(target_arch = "x86") {
        "win32-x86"
    } else if cfg!(target_arch = "aarch64") {
        "win32-aarch64"
    } else {
        return Err(MinecraftLaunchError::Sandbox(
            "JNA does not provide jnidispatch.dll for this architecture".to_owned(),
        ));
    };
    let resource = format!("com/sun/jna/{platform}/jnidispatch.dll");
    let input = fs::File::open(archive)?;
    let mut jar =
        ZipArchive::new(input).map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let mut entry = jar
        .by_name(&resource)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    fs::create_dir_all(destination)?;
    let target = destination.join("jnidispatch.dll");
    let expected_size = entry.size();
    let mut output = fs::File::create(&target)?;
    if let Err(error) = copy_exact_bounded(
        &mut entry,
        &mut output,
        expected_size,
        MAX_JNA_DISPATCH_SIZE,
    ) {
        drop(output);
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn is_jna_core_library_path(path: &str) -> bool {
    path.replace('\\', "/").contains("/jna/jna/")
}

fn safe_library_path(
    paths: &MinecraftPaths,
    relative_path: &str,
) -> Result<PathBuf, MinecraftLaunchError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(MinecraftLaunchError::UnsafeLibraryPath(
            relative_path.to_owned(),
        ));
    }
    Ok(paths.libraries().join(relative))
}

fn extract_archive<F>(
    archive: &Path,
    destination: &Path,
    include: F,
) -> Result<(), MinecraftLaunchError>
where
    F: Fn(&Path) -> bool,
{
    let input = fs::File::open(archive)?;
    let mut zip =
        ZipArchive::new(input).map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    if zip.len() > MAX_NATIVE_ARCHIVE_ENTRIES {
        return Err(MinecraftLaunchError::ArchiveTooLarge(format!(
            "archive contains {} entries (limit: {MAX_NATIVE_ARCHIVE_ENTRIES})",
            zip.len()
        )));
    }
    let mut extracted_size = 0_u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(MinecraftLaunchError::Sandbox(format!(
                "archive contains an unsafe path: {}",
                entry.name()
            )));
        };
        if entry.is_dir() || !include(&relative) {
            continue;
        }
        let expected_size = entry.size();
        if expected_size > MAX_NATIVE_FILE_SIZE {
            return Err(MinecraftLaunchError::ArchiveTooLarge(format!(
                "{} is {expected_size} bytes (per-file limit: {MAX_NATIVE_FILE_SIZE})",
                entry.name()
            )));
        }
        extracted_size = extracted_size
            .checked_add(expected_size)
            .filter(|size| *size <= MAX_NATIVE_TOTAL_SIZE)
            .ok_or_else(|| {
                MinecraftLaunchError::ArchiveTooLarge(format!(
                    "selected entries exceed {MAX_NATIVE_TOTAL_SIZE} bytes"
                ))
            })?;
        let target =
            if destination.ends_with("natives") {
                destination.join(relative.file_name().ok_or_else(|| {
                    MinecraftLaunchError::UnsafeLibraryPath(entry.name().to_owned())
                })?)
            } else {
                destination.join(relative)
            };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        if let Err(error) =
            copy_exact_bounded(&mut entry, &mut output, expected_size, MAX_NATIVE_FILE_SIZE)
        {
            drop(output);
            let _ = fs::remove_file(target);
            return Err(error);
        }
    }
    Ok(())
}

fn copy_exact_bounded<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected_size: u64,
    maximum: u64,
) -> Result<(), MinecraftLaunchError> {
    if expected_size > maximum {
        return Err(MinecraftLaunchError::ArchiveTooLarge(format!(
            "entry is {expected_size} bytes (limit: {maximum})"
        )));
    }
    let copied = std::io::copy(&mut reader.take(expected_size + 1), writer)?;
    if copied != expected_size {
        return Err(MinecraftLaunchError::ArchiveTooLarge(format!(
            "entry size mismatch: expected {expected_size}, extracted {copied}"
        )));
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn prepare_sandbox_layout(
    _paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<Option<SandboxLayout>, MinecraftLaunchError> {
    if instance.sandboxed {
        Err(MinecraftLaunchError::Sandbox(
            "AppContainer is only available on Windows".to_owned(),
        ))
    } else {
        Ok(None)
    }
}

pub fn read_lines<R, F>(reader: R, mut on_line: F)
where
    R: Read,
    F: FnMut(String),
{
    use std::io::BufRead;

    const MAX_LOG_LINE_BYTES: usize = 256 * 1024;
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        let mut saw_data = false;
        let mut truncated = false;
        loop {
            let (consumed, newline) = {
                let available = match reader.fill_buf() {
                    Ok(available) => available,
                    Err(_) => return,
                };
                if available.is_empty() {
                    if !saw_data {
                        return;
                    }
                    break;
                }
                saw_data = true;
                let newline = available.iter().position(|byte| *byte == b'\n');
                let content_length = newline.unwrap_or(available.len());
                let remaining = MAX_LOG_LINE_BYTES.saturating_sub(line.len());
                let copied = content_length.min(remaining);
                line.extend_from_slice(&available[..copied]);
                truncated |= copied < content_length;
                (
                    newline.map_or(available.len(), |position| position + 1),
                    newline.is_some(),
                )
            };
            reader.consume(consumed);
            if newline {
                break;
            }
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let mut text = String::from_utf8_lossy(&line).into_owned();
        if truncated {
            text.push_str(" …[line truncated]");
        }
        on_line(text);
    }
}

fn load_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<InstanceManifest, MinecraftLaunchError> {
    match load_instance_manifest(paths, instance_id) {
        Ok(instance) => Ok(instance),
        Err(super::installer::MinecraftInstallError::InstanceMissing(_)) => Err(
            MinecraftLaunchError::InstanceNotFound(instance_id.to_owned()),
        ),
        Err(error) => Err(error.into()),
    }
}

fn load_instance_fabric_profile(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
) -> Result<Option<FabricProfile>, MinecraftLaunchError> {
    let ModLoader::Fabric { version } = &instance.mod_loader else {
        return Ok(None);
    };
    let profile = load_fabric_profile(paths, &instance.id)?;
    validate_profile(&profile, &instance.version_id, version)?;
    Ok(Some(profile))
}

fn build_fabric_classpath(
    paths: &MinecraftPaths,
    profile: &FabricProfile,
) -> Result<Vec<PathBuf>, MinecraftLaunchError> {
    let mut classpath = Vec::with_capacity(profile.libraries.len());
    for library in &profile.libraries {
        let relative = maven_artifact_path(&library.name)?;
        let target = paths.libraries().join(relative);
        require_file(&target)?;
        classpath.push(target);
    }
    Ok(classpath)
}

fn build_classpath(
    paths: &MinecraftPaths,
    version: &VersionMetadata,
    demo: bool,
) -> Result<Vec<PathBuf>, MinecraftLaunchError> {
    let features = HashMap::from([("is_demo_user".to_owned(), demo)]);
    let mut classpath = Vec::new();

    for library in &version.libraries {
        if !rules_allow(library.rules.as_deref(), &features) {
            continue;
        }

        let Some(artifact) = &library.downloads.artifact else {
            continue;
        };
        let Some(relative_path) = artifact.path.as_deref() else {
            continue;
        };

        let relative = Path::new(relative_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(MinecraftLaunchError::UnsafeLibraryPath(
                relative_path.to_owned(),
            ));
        }

        let target = paths.libraries().join(relative);
        require_file(&target)?;
        classpath.push(target);
    }

    Ok(classpath)
}

fn expand_arguments(
    arguments: &[Argument],
    features: &HashMap<String, bool>,
    substitutions: &HashMap<&str, String>,
) -> Vec<String> {
    let mut expanded = Vec::new();

    for argument in arguments {
        match argument {
            Argument::Plain(value) => expanded.push(substitute(value, substitutions)),
            Argument::Conditional { rules, value } if rules_allow(Some(rules), features) => {
                match value {
                    ArgumentValue::One(value) => {
                        expanded.push(substitute(value, substitutions));
                    }
                    ArgumentValue::Many(values) => {
                        expanded.extend(values.iter().map(|value| substitute(value, substitutions)))
                    }
                }
            }
            Argument::Conditional { .. } => {}
        }
    }

    expanded
}

fn substitute(value: &str, substitutions: &HashMap<&str, String>) -> String {
    substitutions
        .iter()
        .fold(value.to_owned(), |result, (key, replacement)| {
            result.replace(key, replacement)
        })
}

fn generate_narrator_token() -> Result<String, MinecraftLaunchError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| MinecraftLaunchError::Sandbox(format!("random token error: {error}")))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(token)
}

fn require_file(path: &Path) -> Result<(), MinecraftLaunchError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(MinecraftLaunchError::MissingFile(path.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wayland_compatibility_flags_are_scoped_to_verified_version_and_display() {
        use crate::sandbox::LinuxDisplayProtocol::{Wayland, X11};
        let args = vec![std::ffi::OsString::from("net.minecraft.client.main.Main")];
        assert_eq!(super::linux_display_arguments("26.2", X11, &args), args);
        assert_eq!(
            super::linux_display_arguments("1.21.8", Wayland, &args),
            args
        );
        let native = super::linux_display_arguments("26.2", Wayland, &args);
        assert_eq!(native.len(), 3);
        assert_eq!(native[0], "-DMC_DEBUG_ENABLED=true");
        assert_eq!(native[1], "-DMC_DEBUG_PREFER_WAYLAND=true");
        assert_eq!(native[2], args[0]);
    }
    use super::*;
    use crate::minecraft::model::{Rule, RuleOs};

    #[test]
    fn expands_only_matching_conditional_arguments() {
        let arguments = vec![
            Argument::Plain("--name".to_owned()),
            Argument::Plain("${name}".to_owned()),
            Argument::Conditional {
                rules: vec![Rule {
                    action: "allow".to_owned(),
                    os: Some(RuleOs {
                        name: Some(super::super::model::platform_os().to_owned()),
                        arch: None,
                        version: None,
                    }),
                    features: None,
                }],
                value: ArgumentValue::One("--windows".to_owned()),
            },
        ];

        assert_eq!(
            expand_arguments(
                &arguments,
                &HashMap::new(),
                &HashMap::from([("${name}", "DemoPlayer".to_owned())])
            ),
            ["--name", "DemoPlayer", "--windows"]
        );
    }

    #[test]
    fn identifies_jna_core_without_matching_jna_platform() {
        assert!(is_jna_core_library_path(
            "net/java/dev/jna/jna/5.17.0/jna-5.17.0.jar"
        ));
        assert!(!is_jna_core_library_path(
            "net/java/dev/jna/jna-platform/5.17.0/jna-platform-5.17.0.jar"
        ));
    }

    #[test]
    fn bounds_native_archive_entry_extraction() {
        let bytes = [1_u8; 8];
        let mut output = Vec::new();
        copy_exact_bounded(&mut bytes.as_slice(), &mut output, 8, 8).unwrap();
        assert_eq!(output, bytes);

        assert!(matches!(
            copy_exact_bounded(&mut bytes.as_slice(), &mut Vec::new(), 8, 7),
            Err(MinecraftLaunchError::ArchiveTooLarge(_))
        ));
        assert!(matches!(
            copy_exact_bounded(&mut bytes.as_slice(), &mut Vec::new(), 7, 8),
            Err(MinecraftLaunchError::ArchiveTooLarge(_))
        ));
    }

    #[test]
    fn bounds_oversized_log_lines_and_continues_reading() {
        let input = format!("{}\nnext\n", "x".repeat(300 * 1024));
        let mut lines = Vec::new();

        read_lines(input.as_bytes(), |line| lines.push(line));

        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" …[line truncated]"));
        assert!(lines[0].len() < 257 * 1024);
        assert_eq!(lines[1], "next");
    }

    #[test]
    fn generates_a_256_bit_hex_narrator_token() {
        let token = generate_narrator_token().unwrap();

        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[cfg(windows)]
    #[test]
    fn aliases_canonical_windows_paths_inside_the_minecraft_root() {
        let root =
            std::env::temp_dir().join(format!("monalauncher-sandbox-alias-{}", std::process::id()));
        let java = root.join("runtimes/temurin-25/checksum/runtime/bin/java.exe");
        fs::create_dir_all(java.parent().unwrap()).unwrap();
        fs::write(&java, []).unwrap();
        let canonical_java = fs::canonicalize(&java).unwrap();
        assert!(canonical_java.to_string_lossy().starts_with(r"\\?\"));

        let layout = SandboxLayout {
            profile_name: "test".to_owned(),
            sid: "test".to_owned(),
            launch_root: root.join("launch"),
            physical_root: fs::canonicalize(&root).unwrap(),
            virtual_root: PathBuf::from(r"P:\"),
            drive: None,
        };

        assert_eq!(
            sandbox_alias(&layout, &canonical_java).unwrap(),
            PathBuf::from(r"P:\runtimes\temurin-25\checksum\runtime\bin\java.exe")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
