use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::ZipArchive;

use super::installer::list_instances;
use super::model::{rules_allow, Argument, ArgumentValue, InstanceManifest, VersionMetadata};
use super::paths::MinecraftPaths;

#[derive(Debug)]
pub enum MinecraftLaunchError {
    InstanceNotFound(String),
    UnsafeLibraryPath(String),
    MissingFile(PathBuf),
    IncompatibleJava { required: u32, found: String },
    Io(std::io::Error),
    Json(serde_json::Error),
    Install(super::installer::MinecraftInstallError),
    Sandbox(String),
    SandboxedProcessNotIsolated,
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
            Self::Sandbox(error) => write!(formatter, "AppContainer launch error: {error}"),
            Self::SandboxedProcessNotIsolated => {
                write!(formatter, "Minecraft did not receive an AppContainer token")
            }
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

pub enum MinecraftProcess {
    Normal(Child),
    #[cfg(windows)]
    Sandboxed(crate::platform::windows::appcontainer_process::SpawnedAppContainerProcess),
}

impl MinecraftProcess {
    pub fn id(&self) -> u32 {
        match self {
            Self::Normal(child) => child.id(),
            #[cfg(windows)]
            Self::Sandboxed(child) => child.id(),
        }
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, MinecraftLaunchError> {
        match self {
            Self::Normal(child) => Ok(child.try_wait()?),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .try_wait()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }

    pub fn kill(&mut self) -> Result<(), MinecraftLaunchError> {
        match self {
            Self::Normal(child) => Ok(child.kill()?),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .kill()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }

    pub fn wait(&mut self) -> Result<ExitStatus, MinecraftLaunchError> {
        match self {
            Self::Normal(child) => Ok(child.wait()?),
            #[cfg(windows)]
            Self::Sandboxed(child) => child
                .wait()
                .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string())),
        }
    }
}

pub struct SpawnedMinecraft {
    pub child: MinecraftProcess,
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Box<dyn Read + Send>,
    pub sandboxed: bool,
}

#[derive(Debug)]
struct SandboxLayout {
    profile_name: String,
    sid: String,
    launch_root: PathBuf,
    physical_root: PathBuf,
    virtual_root: PathBuf,
    #[cfg(windows)]
    drive: Option<crate::platform::windows::sandbox_drive::SandboxDrive>,
}

pub fn spawn_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    let instance = load_instance(paths, instance_id)?;
    let version_path = paths.version_json(&instance.version_id);
    require_file(&version_path)?;

    let version: VersionMetadata = serde_json::from_slice(&fs::read(version_path)?)?;
    if let Some(java_version) = &version.java_version {
        validate_java_version(Path::new(&instance.java_path), java_version.major_version)?;
    }
    let mut sandbox = prepare_sandbox_layout(paths, &instance)?;
    let classpath = build_classpath(paths, &version, instance.demo)?;
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
        extract_jna_dispatch(paths, &version, &physical_natives_directory.join("jna"))?;
        sandbox_path(&sandbox, &source_client_jar)?
    } else {
        source_client_jar
    };

    let mut classpath_entries = classpath
        .iter()
        .map(|path| sandbox_path(&sandbox, path))
        .collect::<Result<Vec<_>, _>>()?;
    classpath_entries.push(client_entry);
    let classpath = classpath_entries
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(";");

    let substitutions = HashMap::from([
        (
            "${auth_player_name}",
            if instance.demo {
                "DemoPlayer"
            } else {
                "Player"
            }
            .to_owned(),
        ),
        ("${version_name}", version.id.clone()),
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
        (
            "${auth_uuid}",
            "00000000000000000000000000000000".to_owned(),
        ),
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
        ("${classpath_separator}", ";".to_owned()),
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
    if sandbox.is_some() {
        // JNA cannot safely unpack through a SUBST alias in an AppContainer. The trusted launcher
        // extracts jnidispatch.dll first, then the child only loads it from its sandbox drive.
        arguments.push(OsString::from(format!(
            "-Djna.boot.library.path={}",
            natives_directory.join("jna").display()
        )));
        arguments.push(OsString::from("-Djna.nounpack=true"));
    }
    arguments.push(OsString::from(&version.main_class));
    let game_arguments = version
        .minecraft_arguments
        .as_deref()
        .map(|value| {
            value
                .split_whitespace()
                .map(|argument| substitute(argument, &substitutions))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| expand_arguments(&version.arguments.game, &features, &substitutions));
    arguments.extend(game_arguments.into_iter().map(OsString::from));

    if instance.sandboxed {
        return spawn_sandboxed(
            paths,
            &instance,
            sandbox.as_mut().expect("sandbox layout is prepared"),
            &arguments,
            &game_directory,
        );
    }

    let mut command = Command::new(&instance.java_path);
    command
        .args(&arguments)
        .current_dir(&game_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    Ok(SpawnedMinecraft {
        child: MinecraftProcess::Normal(child),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        sandboxed: false,
    })
}

#[cfg(windows)]
fn spawn_sandboxed(
    paths: &MinecraftPaths,
    instance: &InstanceManifest,
    sandbox: &mut SandboxLayout,
    arguments: &[OsString],
    game_directory: &Path,
) -> Result<SpawnedMinecraft, MinecraftLaunchError> {
    use crate::platform::windows::appcontainer_process::launch_in_appcontainer;
    use crate::platform::windows::sandbox_acl::grant_minecraft_access;

    grant_minecraft_access(paths, instance, &sandbox.sid)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let java_path = sandbox_alias(sandbox, Path::new(&instance.java_path))?;
    let mut child =
        launch_in_appcontainer(&sandbox.profile_name, &java_path, arguments, game_directory)
            .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    if !child.token_info.is_app_container {
        let _ = child.kill();
        return Err(MinecraftLaunchError::SandboxedProcessNotIsolated);
    }
    child.retain_cursor_broker(
        crate::platform::windows::cursor_broker::CursorBroker::start(child.id()),
    );
    child.retain_sandbox_drive(
        sandbox
            .drive
            .take()
            .expect("sandbox drive exists while launching"),
    );
    let stdout = child.take_stdout().ok_or_else(|| {
        MinecraftLaunchError::Sandbox("sandboxed stdout is unavailable".to_owned())
    })?;
    let stderr = child.take_stderr().ok_or_else(|| {
        MinecraftLaunchError::Sandbox("sandboxed stderr is unavailable".to_owned())
    })?;
    Ok(SpawnedMinecraft {
        child: MinecraftProcess::Sandboxed(child),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
        sandboxed: true,
    })
}

#[cfg(not(windows))]
fn spawn_sandboxed(
    _paths: &MinecraftPaths,
    _instance: &InstanceManifest,
    _sandbox: &mut SandboxLayout,
    _arguments: &[OsString],
    _game_directory: &Path,
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
    let physical_root = paths.root().to_owned();
    let drive = crate::platform::windows::sandbox_drive::SandboxDrive::create(&physical_root)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    let virtual_root = drive.root().to_owned();
    let launches = paths.instance(&instance.id).join("sandbox-launches");
    fs::create_dir_all(&launches)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?
        .as_nanos();
    let launch_root = launches.join(format!("{}-{nonce}", std::process::id()));
    fs::create_dir(&launch_root)?;
    crate::platform::windows::sandbox_acl::lock_sandbox_launch_directory(
        &launch_root,
        &profile.sid,
    )
    .map_err(|error| MinecraftLaunchError::Sandbox(error.to_string()))?;
    Ok(Some(SandboxLayout {
        profile_name,
        sid: profile.sid,
        launch_root,
        physical_root,
        virtual_root,
        drive: Some(drive),
    }))
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
    let relative = physical.strip_prefix(&sandbox.physical_root).map_err(|_| {
        MinecraftLaunchError::Sandbox(format!(
            "sandbox path is outside Minecraft storage: {}",
            physical.display()
        ))
    })?;
    Ok(sandbox.virtual_root.join(relative))
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
        let Some(native) = library.windows_native() else {
            continue;
        };
        let Some(relative_path) = native.path.as_deref() else {
            continue;
        };
        let archive = safe_library_path(paths, relative_path)?;
        require_file(&archive)?;
        extract_archive(&archive, destination, |path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        })?;
    }
    Ok(())
}

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
    let mut output = fs::File::create(destination.join("jnidispatch.dll"))?;
    std::io::copy(&mut entry, &mut output)?;
    Ok(())
}

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
        let mut output = fs::File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

#[cfg(not(windows))]
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

    for line in BufReader::new(reader).lines().map_while(Result::ok) {
        on_line(line);
    }
}

fn load_instance(
    paths: &MinecraftPaths,
    instance_id: &str,
) -> Result<InstanceManifest, MinecraftLaunchError> {
    list_instances(paths)?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| MinecraftLaunchError::InstanceNotFound(instance_id.to_owned()))
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

fn require_file(path: &Path) -> Result<(), MinecraftLaunchError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(MinecraftLaunchError::MissingFile(path.to_owned()))
    }
}

fn validate_java_version(java_path: &Path, required: u32) -> Result<(), MinecraftLaunchError> {
    require_file(java_path)?;
    let output = Command::new(java_path).arg("-version").output()?;
    let reported = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version_text = reported.lines().next().unwrap_or("unknown").trim();
    let numbers = version_text
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok())
        .collect::<Vec<_>>();
    let detected = match numbers.as_slice() {
        [1, legacy, ..] => Some(*legacy),
        [major, ..] => Some(*major),
        [] => None,
    };

    if detected.is_some_and(|major| major >= required) {
        Ok(())
    } else {
        Err(MinecraftLaunchError::IncompatibleJava {
            required,
            found: version_text.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
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
                        name: Some("windows".to_owned()),
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
}
