use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use windows::core::{Error as WindowsError, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, SetHandleInformation, ERROR_INSUFFICIENT_BUFFER, HANDLE, HANDLE_FLAG_INHERIT,
    STILL_ACTIVE, WAIT_OBJECT_0,
};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{
    PSID, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::SystemServices::SE_GROUP_ENABLED;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use super::appcontainer_profile::{derive_appcontainer_sid, AppContainerProfileError};
use super::cursor_broker::CursorBroker;
use super::process_token::{process_token_info, ProcessTokenError, ProcessTokenInfo};
use super::sandbox_drive::SandboxDrive;

#[derive(Debug)]
pub struct SpawnedProcessInfo {
    pub process_id: u32,
    pub token_info: ProcessTokenInfo,
    pub stdout: String,
    pub stderr: String,
}

pub struct SpawnedAppContainerProcess {
    process: OwnedHandle,
    job: OwnedHandle,
    process_id: u32,
    stdout: Option<File>,
    stderr: Option<File>,
    sandbox_drive: Option<SandboxDrive>,
    cursor_broker: Option<Arc<CursorBroker>>,
    cleanup_directory: Option<PathBuf>,
    pub token_info: ProcessTokenInfo,
}

impl SpawnedAppContainerProcess {
    pub fn id(&self) -> u32 {
        self.process_id
    }

    pub fn take_stdout(&mut self) -> Option<File> {
        self.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<File> {
        self.stderr.take()
    }

    pub fn retain_sandbox_drive(&mut self, drive: SandboxDrive) {
        self.sandbox_drive = Some(drive);
    }

    pub fn retain_cursor_broker(&mut self, broker: Arc<CursorBroker>) {
        self.cursor_broker = Some(broker);
    }

    pub fn retain_cleanup_directory(&mut self, directory: PathBuf) {
        self.cleanup_directory = Some(directory);
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, AppContainerProcessError> {
        let mut exit_code = 0_u32;

        // SAFETY: process is a live process handle returned by CreateProcessW.
        unsafe { GetExitCodeProcess(self.process.as_raw(), &mut exit_code)? };

        if exit_code == STILL_ACTIVE.0 as u32 {
            Ok(None)
        } else {
            Ok(Some(ExitStatus::from_raw(exit_code)))
        }
    }

    pub fn kill(&mut self) -> Result<(), AppContainerProcessError> {
        // Terminate the whole job rather than only the Java process. Minecraft or a mod may have
        // started descendants, and none of them may outlive the launcher-owned process object.
        // SAFETY: job is a live handle created by CreateJobObjectW.
        unsafe { TerminateJobObject(self.job.as_raw(), 1)? };
        Ok(())
    }

    pub fn wait(&mut self) -> Result<ExitStatus, AppContainerProcessError> {
        // SAFETY: process is a live process handle returned by CreateProcessW.
        let wait_result = unsafe { WaitForSingleObject(self.process.as_raw(), u32::MAX) };
        if wait_result != WAIT_OBJECT_0 {
            return Err(AppContainerProcessError::Windows(
                WindowsError::from_thread(),
            ));
        }

        self.try_wait()?
            .ok_or_else(|| AppContainerProcessError::Windows(WindowsError::from_thread()))
    }
}

impl Drop for SpawnedAppContainerProcess {
    fn drop(&mut self) {
        // A process that was spawned successfully must never outlive its owner because an early
        // error would otherwise leave an untracked AppContainer process and mapped drive behind.
        // TerminateJobObject is also called after the primary process exited so descendants are
        // not missed before the kill-on-close job handle is released.
        let primary_was_running = self.try_wait().ok().flatten().is_none();
        let _ = self.kill();
        if primary_was_running {
            let _ = self.wait();
        }
        self.cursor_broker.take();
        self.sandbox_drive.take();
        if let Some(directory) = self.cleanup_directory.take() {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

#[derive(Debug)]
pub enum AppContainerProcessError {
    Policy(crate::sandbox::PolicyError),
    EmptyExecutablePath,
    InteriorNullCharacter,
    MissingLocalAppData,
    Profile(AppContainerProfileError),
    ProcessToken(ProcessTokenError),
    Io(std::io::Error),
    Windows(WindowsError),
}

impl fmt::Display for AppContainerProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(error) => write!(formatter, "unsupported sandbox policy: {error}"),
            Self::EmptyExecutablePath => write!(formatter, "executable path must not be empty"),
            Self::InteriorNullCharacter => {
                write!(
                    formatter,
                    "Windows strings must not contain a null character"
                )
            }
            Self::MissingLocalAppData => write!(formatter, "LOCALAPPDATA is not available"),
            Self::Profile(error) => write!(formatter, "AppContainer profile error: {error}"),
            Self::ProcessToken(error) => write!(formatter, "process token error: {error}"),
            Self::Io(error) => write!(formatter, "process file system error: {error}"),
            Self::Windows(error) => write!(formatter, "Windows API error: {error}"),
        }
    }
}

impl Error for AppContainerProcessError {}

impl From<AppContainerProfileError> for AppContainerProcessError {
    fn from(error: AppContainerProfileError) -> Self {
        Self::Profile(error)
    }
}

impl From<ProcessTokenError> for AppContainerProcessError {
    fn from(error: ProcessTokenError) -> Self {
        Self::ProcessToken(error)
    }
}

impl From<std::io::Error> for AppContainerProcessError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<WindowsError> for AppContainerProcessError {
    fn from(error: WindowsError) -> Self {
        Self::Windows(error)
    }
}

pub(crate) fn launch_with_policy(
    profile_name: &str,
    executable: &Path,
    arguments: &[OsString],
    current_directory: &Path,
    policy: &crate::sandbox::SandboxPolicy,
) -> Result<SpawnedAppContainerProcess, AppContainerProcessError> {
    // Validate the shared policy before assigning explicit network capabilities.
    policy
        .compile(crate::sandbox::Backend::AppContainer)
        .map_err(AppContainerProcessError::Policy)?;
    launch_with_network(
        profile_name,
        executable,
        arguments,
        current_directory,
        policy.network == crate::sandbox::NetworkAccess::Internet,
    )
}

pub fn launch_in_appcontainer(
    profile_name: &str,
    executable: &Path,
    arguments: &[OsString],
    current_directory: &Path,
) -> Result<SpawnedAppContainerProcess, AppContainerProcessError> {
    launch_with_network(
        profile_name,
        executable,
        arguments,
        current_directory,
        false,
    )
}

fn launch_with_network(
    profile_name: &str,
    executable: &Path,
    arguments: &[OsString],
    current_directory: &Path,
    network: bool,
) -> Result<SpawnedAppContainerProcess, AppContainerProcessError> {
    if executable.as_os_str().is_empty() {
        return Err(AppContainerProcessError::EmptyExecutablePath);
    }

    let app_container_sid = derive_appcontainer_sid(profile_name)?;
    let job = kill_on_close_job()?;
    let attribute_list = AttributeList::new(2)?;
    let network_sids = if network {
        ["S-1-15-3-1", "S-1-15-3-2", "S-1-15-3-3"]
            .into_iter()
            .map(LocalSid::parse)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    let mut capabilities: Vec<_> = network_sids
        .iter()
        .map(|sid| SID_AND_ATTRIBUTES {
            Sid: sid.0,
            Attributes: SE_GROUP_ENABLED as u32,
        })
        .collect();
    let security_capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: app_container_sid.as_raw(),
        Capabilities: if capabilities.is_empty() {
            std::ptr::null_mut()
        } else {
            capabilities.as_mut_ptr()
        },
        CapabilityCount: capabilities.len() as u32,
        Reserved: 0,
    };

    // SAFETY: all pointers remain valid until CreateProcessW returns.
    unsafe {
        UpdateProcThreadAttribute(
            attribute_list.as_raw(),
            0,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            Some((&security_capabilities as *const SECURITY_CAPABILITIES).cast()),
            size_of::<SECURITY_CAPABILITIES>(),
            None,
            None,
        )?;
    }

    let stdout_pipe = Pipe::new()?;
    let stderr_pipe = Pipe::new()?;
    let inherited_handles = [stdout_pipe.write_handle(), stderr_pipe.write_handle()];
    // SAFETY: both handles are valid, explicitly inheritable pipe write ends. The array remains
    // alive until CreateProcessW returns and prevents unrelated inheritable parent handles from
    // crossing the AppContainer boundary.
    unsafe {
        UpdateProcThreadAttribute(
            attribute_list.as_raw(),
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(inherited_handles.as_ptr().cast_mut().cast()),
            size_of_val(&inherited_handles),
            None,
            None,
        )?;
    }
    let executable_wide = WideString::from_os(executable.as_os_str())?;
    let mut command_line = WideString::command_line(executable.as_os_str(), arguments)?;
    let current_directory_wide = WideString::from_os(current_directory.as_os_str())?;
    let mut environment = appcontainer_environment(profile_name)?;

    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info.StartupInfo.hStdOutput = stdout_pipe.write_handle();
    startup_info.StartupInfo.hStdError = stderr_pipe.write_handle();
    startup_info.StartupInfo.hStdInput = HANDLE::default();
    startup_info.lpAttributeList = attribute_list.as_raw();

    let mut process_information = PROCESS_INFORMATION::default();

    // SAFETY: command line and environment are mutable NUL-terminated buffers, handles intended
    // for the child are inheritable, and startup/process structures are initialized.
    unsafe {
        CreateProcessW(
            PCWSTR(executable_wide.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            true,
            // Keep `java.exe` so stdout/stderr remain available for logs and the narrator bridge,
            // but do not let the console-subsystem launcher create a visible terminal window.
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_SUSPENDED
                | CREATE_NO_WINDOW,
            Some(environment.as_mut_ptr().cast()),
            PCWSTR(current_directory_wide.as_ptr()),
            &startup_info.StartupInfo,
            &mut process_information,
        )?;
    }

    let process = OwnedHandle::new(process_information.hProcess);
    let _thread = OwnedHandle::new(process_information.hThread);
    let token_info = match process_token_info(process.as_raw()) {
        Ok(info) => info,
        Err(error) => {
            // SAFETY: the process handle came from CreateProcessW and has terminate access.
            let _ = unsafe { TerminateProcess(process.as_raw(), 1) };
            return Err(error.into());
        }
    };
    // SAFETY: both handles are live and the job is configured before any handle can escape.
    if let Err(error) = unsafe { AssignProcessToJobObject(job.as_raw(), process.as_raw()) } {
        // SAFETY: the process handle came from CreateProcessW and has terminate access.
        let _ = unsafe { TerminateProcess(process.as_raw(), 1) };
        return Err(error.into());
    }
    // SAFETY: the primary thread was created suspended and is resumed exactly once after the job
    // assignment and token inspection have both succeeded.
    if unsafe { ResumeThread(_thread.as_raw()) } == u32::MAX {
        // SAFETY: the process handle came from CreateProcessW and has terminate access.
        let _ = unsafe { TerminateProcess(process.as_raw(), 1) };
        return Err(WindowsError::from_thread().into());
    }

    Ok(SpawnedAppContainerProcess {
        process,
        job,
        process_id: process_information.dwProcessId,
        stdout: Some(stdout_pipe.into_reader()),
        stderr: Some(stderr_pipe.into_reader()),
        sandbox_drive: None,
        cursor_broker: None,
        cleanup_directory: None,
        token_info,
    })
}

fn kill_on_close_job() -> Result<OwnedHandle, AppContainerProcessError> {
    // SAFETY: no security attributes and no name are valid for an anonymous job object.
    let job = OwnedHandle::new(unsafe { CreateJobObjectW(None, PCWSTR::null())? });
    let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: information points to the declared structure for the selected information class.
    unsafe {
        SetInformationJobObject(
            job.as_raw(),
            JobObjectExtendedLimitInformation,
            (&raw const information).cast(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).map_err(|_| {
                AppContainerProcessError::Io(std::io::Error::other(
                    "job information structure is unexpectedly large",
                ))
            })?,
        )?;
    }
    Ok(job)
}

pub fn launch_probe_in_appcontainer(
    profile_name: &str,
    probe_executable: &Path,
) -> Result<SpawnedProcessInfo, AppContainerProcessError> {
    let mut child = launch_in_appcontainer(
        profile_name,
        probe_executable,
        &[OsString::from("--appcontainer-child")],
        probe_executable.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    let process_id = child.id();
    let token_info = child.token_info.clone();
    let Some(mut stdout) = child.take_stdout() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppContainerProcessError::Io(std::io::Error::other(
            "probe stdout is unavailable",
        )));
    };
    let Some(mut stderr) = child.take_stderr() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppContainerProcessError::Io(std::io::Error::other(
            "probe stderr is unavailable",
        )));
    };
    child.wait()?;
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    stdout.read_to_string(&mut stdout_text)?;
    stderr.read_to_string(&mut stderr_text)?;

    Ok(SpawnedProcessInfo {
        process_id,
        token_info,
        stdout: stdout_text,
        stderr: stderr_text,
    })
}

fn appcontainer_environment(profile_name: &str) -> Result<Vec<u16>, AppContainerProcessError> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(AppContainerProcessError::MissingLocalAppData)?;
    let profile_root = local_app_data
        .join("Packages")
        .join(profile_name)
        .join("AC");
    std::fs::create_dir_all(profile_root.join("Temp"))?;

    // AppContainerはLOCALAPPDATAやTEMPなどの既知フォルダーを自動的に仮想化する。
    // ここで既に仮想化済みのパッケージパスを設定すると、Windowsがもう一度変換して
    // `AC\Packages\...\AC\Temp`という二重パスになるため、互換性に必要な親環境だけを
    // 引き継ぐ。トークンやJava注入オプションなど、無関係な環境変数は子へ渡さない。
    let mut variables = std::env::vars_os()
        .filter(|(name, _)| appcontainer_environment_variable_allowed(name))
        .collect::<Vec<_>>();
    variables.sort_by_key(|(name, _)| name.to_string_lossy().to_ascii_uppercase());

    let mut block = Vec::new();
    for (name, value) in variables {
        let name_units = name.encode_wide().collect::<Vec<_>>();
        if name_units.contains(&0) {
            return Err(AppContainerProcessError::InteriorNullCharacter);
        }
        block.extend(name_units);
        block.push('=' as u16);
        let value_units = value.encode_wide().collect::<Vec<_>>();
        if value_units.contains(&0) {
            return Err(AppContainerProcessError::InteriorNullCharacter);
        }
        block.extend(value_units);
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn appcontainer_environment_variable_allowed(name: &OsStr) -> bool {
    matches!(
        name.to_string_lossy().to_ascii_uppercase().as_str(),
        "ALLUSERSPROFILE"
            | "APPDATA"
            | "COMMONPROGRAMFILES"
            | "COMMONPROGRAMFILES(X86)"
            | "COMMONPROGRAMW6432"
            | "COMPUTERNAME"
            | "COMSPEC"
            | "DRIVERDATA"
            | "HOMEDRIVE"
            | "HOMEPATH"
            | "LOCALAPPDATA"
            | "LOGONSERVER"
            | "NUMBER_OF_PROCESSORS"
            | "OS"
            | "PATH"
            | "PATHEXT"
            | "PROCESSOR_ARCHITECTURE"
            | "PROCESSOR_IDENTIFIER"
            | "PROCESSOR_LEVEL"
            | "PROCESSOR_REVISION"
            | "PROGRAMDATA"
            | "PROGRAMFILES"
            | "PROGRAMFILES(X86)"
            | "PROGRAMW6432"
            | "PUBLIC"
            | "SYSTEMDRIVE"
            | "SYSTEMROOT"
            | "TEMP"
            | "TMP"
            | "USERDOMAIN"
            | "USERDOMAIN_ROAMINGPROFILE"
            | "USERNAME"
            | "USERPROFILE"
            | "WINDIR"
    )
}

// ConvertStringSidToSidW allocations require LocalFree (not FreeSid).
struct LocalSid(PSID);
impl LocalSid {
    fn parse(value: &str) -> Result<Self, WindowsError> {
        let text: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let mut sid = PSID::default();
        // SAFETY: text is NUL-terminated; sid is a valid output pointer.
        unsafe {
            ConvertStringSidToSidW(PCWSTR(text.as_ptr()), &mut sid)?;
        }
        Ok(Self(sid))
    }
}
impl Drop for LocalSid {
    fn drop(&mut self) {
        // SAFETY: this object exclusively owns the allocation returned by conversion.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}

struct Pipe {
    read: OwnedHandle,
    write: OwnedHandle,
}

impl Pipe {
    fn new() -> Result<Self, AppContainerProcessError> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: BOOL(1),
        };
        let mut read = HANDLE::default();
        let mut write = HANDLE::default();

        // SAFETY: output handles and SECURITY_ATTRIBUTES are valid writable values.
        unsafe { CreatePipe(&mut read, &mut write, Some(&attributes), 0)? };
        let read = OwnedHandle::new(read);
        let write = OwnedHandle::new(write);

        // SAFETY: the parent-side read handle is valid; clearing inheritance keeps only the child
        // write endpoint alive after CreateProcessW.
        unsafe { SetHandleInformation(read.as_raw(), HANDLE_FLAG_INHERIT.0, Default::default())? };

        Ok(Self { read, write })
    }

    fn write_handle(&self) -> HANDLE {
        self.write.as_raw()
    }

    fn into_reader(self) -> File {
        let raw = self.read.into_raw();
        drop(self.write);
        // SAFETY: ownership of the valid read handle is transferred to File.
        unsafe { File::from_raw_handle(raw.0) }
    }
}

struct AttributeList {
    _storage: Vec<usize>,
    raw: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new(attribute_count: u32) -> Result<Self, AppContainerProcessError> {
        let mut required_size = 0_usize;
        // SAFETY: a null list is the documented size-query call; required_size is writable.
        let first_result = unsafe {
            InitializeProcThreadAttributeList(None, attribute_count, None, &mut required_size)
        };
        if let Err(error) = first_result {
            if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
                return Err(error.into());
            }
        }

        let storage_length = required_size.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; storage_length];
        let raw = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        // SAFETY: storage has the size returned by the query and remains owned by AttributeList.
        unsafe {
            InitializeProcThreadAttributeList(Some(raw), attribute_count, None, &mut required_size)?
        };
        Ok(Self {
            _storage: storage,
            raw,
        })
    }

    fn as_raw(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.raw
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: raw was initialized successfully and is deleted exactly once here.
        unsafe { DeleteProcThreadAttributeList(self.raw) };
    }
}

struct WideString {
    buffer: Vec<u16>,
}

impl WideString {
    fn from_os(value: &OsStr) -> Result<Self, AppContainerProcessError> {
        let mut buffer = value.encode_wide().collect::<Vec<_>>();
        if buffer.contains(&0) {
            return Err(AppContainerProcessError::InteriorNullCharacter);
        }
        buffer.push(0);
        Ok(Self { buffer })
    }

    fn command_line(
        executable: &OsStr,
        arguments: &[OsString],
    ) -> Result<Self, AppContainerProcessError> {
        let mut command = quote_windows_argument(executable)?;
        for argument in arguments {
            command.push(' ' as u16);
            command.extend(quote_windows_argument(argument)?);
        }
        command.push(0);
        Ok(Self { buffer: command })
    }

    fn as_ptr(&self) -> *const u16 {
        self.buffer.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u16 {
        self.buffer.as_mut_ptr()
    }
}

fn quote_windows_argument(value: &OsStr) -> Result<Vec<u16>, AppContainerProcessError> {
    let units = value.encode_wide().collect::<Vec<_>>();
    if units.contains(&0) {
        return Err(AppContainerProcessError::InteriorNullCharacter);
    }
    if !units.is_empty() && !units.iter().any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22)) {
        return Ok(units);
    }

    let mut quoted = vec!['"' as u16];
    let mut backslashes = 0;
    for unit in units {
        if unit == '\\' as u16 {
            backslashes += 1;
        } else if unit == '"' as u16 {
            quoted.extend(std::iter::repeat_n('\\' as u16, backslashes * 2 + 1));
            quoted.push(unit);
            backslashes = 0;
        } else {
            quoted.extend(std::iter::repeat_n('\\' as u16, backslashes));
            quoted.push(unit);
            backslashes = 0;
        }
    }
    quoted.extend(std::iter::repeat_n('\\' as u16, backslashes * 2));
    quoted.push('"' as u16);
    Ok(quoted)
}

struct OwnedHandle {
    handle: HANDLE,
}

// SAFETY: Windows process/pipe handles are kernel object references that may be transferred to
// another thread. OwnedHandle keeps unique ownership and only closes the handle once in Drop.
unsafe impl Send for OwnedHandle {}

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self { handle }
    }

    fn as_raw(&self) -> HANDLE {
        self.handle
    }

    fn into_raw(mut self) -> HANDLE {
        let handle = self.handle;
        self.handle = HANDLE::default();
        handle
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: this object uniquely owns a valid handle and closes it exactly once.
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoted(value: &str) -> String {
        String::from_utf16(&quote_windows_argument(OsStr::new(value)).unwrap()).unwrap()
    }

    #[test]
    fn quotes_empty_and_spaced_arguments() {
        assert_eq!(quoted(""), "\"\"");
        assert_eq!(quoted("hello world"), "\"hello world\"");
    }

    #[test]
    fn escapes_quotes_and_trailing_backslashes() {
        assert_eq!(quoted("a\\\"b"), "\"a\\\\\\\"b\"");
        assert_eq!(quoted("C:\\space here\\"), "\"C:\\space here\\\\\"");
    }

    #[test]
    fn appcontainer_environment_excludes_secrets_and_java_injection_options() {
        assert!(appcontainer_environment_variable_allowed(OsStr::new(
            "SystemRoot"
        )));
        assert!(appcontainer_environment_variable_allowed(OsStr::new(
            "PATH"
        )));
        assert!(!appcontainer_environment_variable_allowed(OsStr::new(
            "GITHUB_TOKEN"
        )));
        assert!(!appcontainer_environment_variable_allowed(OsStr::new(
            "JAVA_TOOL_OPTIONS"
        )));
    }
}
