use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};

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

pub struct SpawnedMinecraft {
    pub child: Child,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
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
    let classpath = build_classpath(paths, &version)?;
    let client_jar = paths.version_jar(&version.id);
    require_file(&client_jar)?;

    let game_directory = PathBuf::from(&instance.game_directory);
    fs::create_dir_all(&game_directory)?;
    let natives_directory = paths.instance(instance_id).join("natives");
    fs::create_dir_all(&natives_directory)?;

    let mut classpath_entries = classpath;
    classpath_entries.push(client_jar);
    let classpath = classpath_entries
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(";");

    let substitutions = HashMap::from([
        ("${auth_player_name}", "DemoPlayer".to_owned()),
        ("${version_name}", version.id.clone()),
        (
            "${game_directory}",
            game_directory.to_string_lossy().into_owned(),
        ),
        (
            "${assets_root}",
            paths.assets().to_string_lossy().into_owned(),
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
        ("${classpath}", classpath),
        ("${classpath_separator}", ";".to_owned()),
        (
            "${library_directory}",
            paths.libraries().to_string_lossy().into_owned(),
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

    let mut command = Command::new(&instance.java_path);
    command
        .arg("-Xms512M")
        .arg("-Xmx2G")
        .args(expand_arguments(
            &version.arguments.jvm,
            &features,
            &substitutions,
        ))
        .arg(&version.main_class)
        .args(expand_arguments(
            &version.arguments.game,
            &features,
            &substitutions,
        ))
        .current_dir(&game_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout was configured with Stdio::piped");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was configured with Stdio::piped");

    Ok(SpawnedMinecraft {
        child,
        stdout,
        stderr,
    })
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
) -> Result<Vec<PathBuf>, MinecraftLaunchError> {
    let features = HashMap::from([("is_demo_user".to_owned(), true)]);
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

    if detected == Some(required) {
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
}
