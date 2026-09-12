//! OS-independent permission requests and explicit backend compatibility differences.
pub mod game_files;
pub mod seatbelt;
pub use game_files::GameDirectory;
#[cfg(test)]
mod tests;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    AppContainer,
    Seatbelt,
    Bubblewrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDisplayProtocol {
    X11,
    Wayland,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    JavaHome,
    Libraries,
    Assets,
    Version,
    Game,
    Launch,
    Temp,
}

impl Resource {
    pub fn parameter(self) -> &'static str {
        match self {
            Self::JavaHome => "JAVA_HOME",
            Self::Libraries => "LIBRARIES",
            Self::Assets => "ASSETS",
            Self::Version => "VERSION",
            Self::Game => "GAME",
            Self::Launch => "LAUNCH",
            Self::Temp => "TEMP",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileGrant {
    pub path: PathBuf,
    pub access: FileAccess,
}

/// Trusted paths, resolved before passing the policy to a backend.
#[derive(Debug, Clone)]
pub struct SandboxResources {
    pub data_root: PathBuf,
    pub runtimes_root: PathBuf,
    pub versions_root: PathBuf,
    pub instance_root: PathBuf,
    pub java_home: PathBuf,
    pub libraries: PathBuf,
    pub assets: PathBuf,
    pub version: PathBuf,
    pub game: PathBuf,
    pub launch: PathBuf,
    pub temp: PathBuf,
}

impl SandboxResources {
    fn canonicalize(mut self) -> Result<Self, PolicyError> {
        for path in [
            &mut self.data_root,
            &mut self.runtimes_root,
            &mut self.versions_root,
            &mut self.instance_root,
            &mut self.java_home,
            &mut self.libraries,
            &mut self.assets,
            &mut self.version,
            &mut self.game,
            &mut self.launch,
            &mut self.temp,
        ] {
            *path = std::fs::canonicalize(&*path).map_err(|error| {
                PolicyError(format!("sandbox resource {}: {error}", path.display()))
            })?;
            if !path.is_dir() {
                return Err(PolicyError(format!(
                    "sandbox resource is not a directory: {}",
                    path.display()
                )));
            }
        }
        // A writable root must not expose code, launch metadata, or another resource via aliases.
        for writable in [&self.game, &self.temp] {
            for protected in [
                &self.java_home,
                &self.libraries,
                &self.assets,
                &self.version,
                &self.launch,
            ] {
                if protected.starts_with(writable)
                    || (protected != &self.launch && writable.starts_with(protected))
                {
                    return Err(PolicyError(
                        "writable sandbox resource overlaps protected data".into(),
                    ));
                }
            }
        }
        if self.game.starts_with(&self.launch) || self.launch.starts_with(&self.game) {
            return Err(PolicyError("game and launch resources overlap".into()));
        }
        if !self.version.starts_with(&self.versions_root)
            || !self.java_home.starts_with(&self.runtimes_root)
        {
            return Err(PolicyError(
                "runtime or version resource is outside its store".into(),
            ));
        }
        if !self.game.starts_with(&self.instance_root)
            || self.game == self.instance_root
            || !self.launch.starts_with(&self.instance_root)
            || self.launch == self.instance_root
            || !self.temp.starts_with(&self.launch)
            || self.temp == self.launch
        {
            return Err(PolicyError(
                "game/launch/temp are outside their owned directories".into(),
            ));
        }
        Ok(self)
    }

    pub fn path(&self, resource: Resource) -> &Path {
        match resource {
            Resource::JavaHome => &self.java_home,
            Resource::Libraries => &self.libraries,
            Resource::Assets => &self.assets,
            Resource::Version => &self.version,
            Resource::Game => &self.game,
            Resource::Launch => &self.launch,
            Resource::Temp => &self.temp,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    Denied,
    Internet,
}

#[derive(Debug, Clone, Copy)]
pub struct DesktopPermissions {
    pub window_and_input: bool,
    pub audio_output: bool,
    pub microphone: bool,
    pub clipboard: bool,
}

/// This preset is defined once. Backends may not silently grant unsupported requests.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    resources: SandboxResources,
    files: Vec<(Resource, FileAccess)>,
    game_readonly: Vec<PathBuf>,
    pub network: NetworkAccess,
    pub desktop: DesktopPermissions,
    pub narrator: bool,
    pub allow_windows_compatibility: bool,
    pub allow_linux_desktop_compatibility: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendException {
    WindowsWritableLaunch,
    WindowsInstanceMetadataRead,
    WindowsAllVersionsRead,
    WindowsPrivateProfileStorage,
    LinuxSystemRuntimeRead,
    LinuxX11PeerAccess,
    LinuxWaylandCompositorAccess,
    LinuxGpuIdentificationRead,
    LinuxPulseAudioServiceAccess,
}

#[derive(Debug)]
pub struct PolicyError(pub String);
impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for PolicyError {}

#[derive(Debug)]
pub struct CompiledPolicy {
    pub backend: Backend,
    pub files: Vec<FileGrant>,
    pub traverse: Vec<PathBuf>,
    pub exceptions: Vec<BackendException>,
    pub narrator: bool,
}

impl SandboxPolicy {
    /// Select the actual display service, without changing common file/network grants.
    pub fn compile_linux(
        &self,
        display: LinuxDisplayProtocol,
    ) -> Result<CompiledPolicy, PolicyError> {
        let mut plan = self.compile(Backend::Bubblewrap)?;
        if display == LinuxDisplayProtocol::Wayland {
            plan.exceptions
                .push(BackendException::LinuxGpuIdentificationRead);
            for exception in &mut plan.exceptions {
                if *exception == BackendException::LinuxX11PeerAccess {
                    *exception = BackendException::LinuxWaylandCompositorAccess;
                }
            }
        }
        Ok(plan)
    }
    pub fn minecraft(resources: SandboxResources) -> Result<Self, PolicyError> {
        use FileAccess::*;
        use Resource::*;
        Ok(Self {
            resources: resources.canonicalize()?,
            files: vec![
                (JavaHome, ReadOnly),
                (Libraries, ReadOnly),
                (Assets, ReadOnly),
                (Version, ReadOnly),
                (Game, ReadWrite),
                (Launch, ReadOnly),
                (Temp, ReadWrite),
            ],
            game_readonly: vec![],
            network: NetworkAccess::Denied,
            desktop: DesktopPermissions {
                window_and_input: true,
                audio_output: true,
                microphone: false,
                clipboard: false,
            },
            narrator: true,
            // Explicitly preserve the existing AppContainer compatibility contract for this preset.
            allow_windows_compatibility: true,
            // X11 and PulseAudio expose service capabilities beyond window/audio output.
            allow_linux_desktop_compatibility: true,
        })
    }

    pub fn resources(&self) -> &SandboxResources {
        &self.resources
    }
    pub fn requested_files(&self) -> &[(Resource, FileAccess)] {
        &self.files
    }

    /// Current backends support reducing writable game/temp access, not exposing shared code.
    pub fn with_file_access(
        mut self,
        resource: Resource,
        access: FileAccess,
    ) -> Result<Self, PolicyError> {
        if access == FileAccess::ReadWrite && !matches!(resource, Resource::Game | Resource::Temp) {
            return Err(PolicyError(
                "write access to shared or launcher-owned resources is unsupported".into(),
            ));
        }
        for (role, current) in &mut self.files {
            if *role == resource {
                *current = access;
            }
        }
        Ok(self)
    }

    pub fn with_readonly_game_directories(
        mut self,
        directories: &[GameDirectory],
    ) -> Result<Self, PolicyError> {
        if !directories.is_empty() {
            game_files::validate_tree(&self.resources.game)?;
        }
        for directory in directories {
            let path = self.resources.game.join(directory.name());
            std::fs::create_dir_all(&path).map_err(|e| PolicyError(e.to_string()))?;
            if std::fs::canonicalize(&path).map_err(|e| PolicyError(e.to_string()))? != path {
                return Err(PolicyError("game directory is an alias".into()));
            }
            if !self.game_readonly.contains(&path) {
                self.game_readonly.push(path);
            }
        }
        Ok(self)
    }
    pub fn readonly_game_directories(&self) -> &[PathBuf] {
        &self.game_readonly
    }

    pub fn compile(&self, backend: Backend) -> Result<CompiledPolicy, PolicyError> {
        if self.desktop.microphone && !self.desktop.audio_output {
            return Err(PolicyError(
                "microphone access requires the shared audio service to remain enabled".into(),
            ));
        }
        if !self.desktop.window_and_input {
            return Err(PolicyError(
                "display/input separation is unsupported".into(),
            ));
        }
        if backend == Backend::AppContainer && !self.desktop.audio_output {
            return Err(PolicyError(
                "AppContainer does not support disabling playback independently".into(),
            ));
        }
        if backend != Backend::Seatbelt && (self.desktop.microphone || self.desktop.clipboard) {
            return Err(PolicyError(
                "independent microphone/clipboard grants are unsupported on this backend".into(),
            ));
        }
        let mut plan = CompiledPolicy {
            backend,
            files: self
                .files
                .iter()
                .map(|(role, access)| FileGrant {
                    path: self.resources.path(*role).to_owned(),
                    access: *access,
                })
                .collect(),
            traverse: vec![],
            exceptions: vec![],
            narrator: self.narrator,
        };
        plan.files
            .extend(self.game_readonly.iter().map(|path| FileGrant {
                path: path.clone(),
                access: FileAccess::ReadOnly,
            }));
        if backend == Backend::Bubblewrap {
            if !self.allow_linux_desktop_compatibility {
                return Err(PolicyError(
                    "Linux desktop requires explicit display/PulseAudio compatibility permissions"
                        .into(),
                ));
            }
            plan.exceptions.extend([
                BackendException::LinuxSystemRuntimeRead,
                BackendException::LinuxX11PeerAccess,
            ]);
            if self.desktop.audio_output {
                plan.exceptions
                    .push(BackendException::LinuxPulseAudioServiceAccess);
            }
        }
        if backend == Backend::AppContainer {
            if !self.allow_windows_compatibility {
                return Err(PolicyError(
                    "AppContainer requires explicit Minecraft compatibility permissions".into(),
                ));
            }
            if self
                .files
                .iter()
                .any(|(role, access)| *role == Resource::Temp && *access == FileAccess::ReadOnly)
            {
                return Err(PolicyError(
                    "AppContainer cannot make temp read-only inside its writable launch workspace"
                        .into(),
                ));
            }
            for grant in &mut plan.files {
                if grant.path == self.resources.launch {
                    grant.access = FileAccess::ReadWrite;
                }
            }
            plan.files.extend([
                FileGrant {
                    path: self.resources.versions_root.clone(),
                    access: FileAccess::ReadOnly,
                },
                FileGrant {
                    path: self.resources.instance_root.clone(),
                    access: FileAccess::ReadOnly,
                },
            ]);
            plan.traverse.extend([
                self.resources.data_root.clone(),
                self.resources
                    .instance_root
                    .parent()
                    .ok_or_else(|| PolicyError("instance parent missing".into()))?
                    .to_owned(),
                self.resources.runtimes_root.clone(),
            ]);
            for ancestor in self.resources.java_home.ancestors().skip(1) {
                if ancestor == self.resources.runtimes_root {
                    break;
                }
                if ancestor.starts_with(&self.resources.runtimes_root) {
                    plan.traverse.push(ancestor.to_owned());
                }
            }
            plan.exceptions.extend([
                BackendException::WindowsWritableLaunch,
                BackendException::WindowsInstanceMetadataRead,
                BackendException::WindowsAllVersionsRead,
                BackendException::WindowsPrivateProfileStorage,
            ]);
        }
        Ok(plan)
    }
}
