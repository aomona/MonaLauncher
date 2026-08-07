use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use windows::core::{Error as WindowsError, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, SetHandleInformation, ERROR_INSUFFICIENT_BUFFER, HANDLE, HANDLE_FLAG_INHERIT,
    STILL_ACTIVE, WAIT_OBJECT_0,
};
use windows::Win32::Security::{SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use super::appcontainer_profile::{derive_appcontainer_sid, AppContainerProfileError, OwnedSid};
use super::process_token::{process_token_info, ProcessTokenError, ProcessTokenInfo};

#[derive(Debug)]
pub struct SpawnedProcessInfo {
    pub process_id: u32,
    pub token_info: ProcessTokenInfo,
}

pub struct SpawnedAppContainerProcess {
    process: OwnedHandle,
    process_id: u32,
    stdout: Option<File>,
    stderr: Option<File>,
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
        // SAFETY: process is a process handle with PROCESS_TERMINATE access from CreateProcessW.
        unsafe { TerminateProcess(self.process.as_raw(), 1)? };
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

#[derive(Debug)]
pub enum AppContainerProcessError {
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

pub fn launch_in_appcontainer(
    profile_name: &str,
    executable: &Path,
    arguments: &[OsString],
    current_directory: &Path,
) -> Result<SpawnedAppContainerProcess, AppContainerProcessError> {
    if executable.as_os_str().is_empty() {
        return Err(AppContainerProcessError::EmptyExecutablePath);
    }

    let app_container_sid = derive_appcontainer_sid(profile_name)?;
    let attribute_list = AttributeList::new()?;
    let security_capabilities = security_capabilities(&app_container_sid);

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
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            Some(environment.as_mut_ptr().cast()),
            PCWSTR(current_directory_wide.as_ptr()),
            &startup_info.StartupInfo,
            &mut process_information,
        )?;
    }

    let process = OwnedHandle::new(process_information.hProcess);
    let _thread = OwnedHandle::new(process_information.hThread);
    let token_info = process_token_info(process.as_raw())?;

    Ok(SpawnedAppContainerProcess {
        process,
        process_id: process_information.dwProcessId,
        stdout: Some(stdout_pipe.into_reader()),
        stderr: Some(stderr_pipe.into_reader()),
        token_info,
    })
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
    child.wait()?;

    Ok(SpawnedProcessInfo {
        process_id,
        token_info,
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
    // `AC\Packages\...\AC\Temp`という二重パスになるため、通常の親環境を引き継ぐ。
    let mut variables = std::env::vars_os().collect::<Vec<_>>();
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

fn security_capabilities(app_container_sid: &OwnedSid) -> SECURITY_CAPABILITIES {
    SECURITY_CAPABILITIES {
        AppContainerSid: app_container_sid.as_raw(),
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
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
    fn new() -> Result<Self, AppContainerProcessError> {
        let mut required_size = 0_usize;
        // SAFETY: a null list is the documented size-query call; required_size is writable.
        let first_result =
            unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut required_size) };
        if let Err(error) = first_result {
            if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
                return Err(error.into());
            }
        }

        let storage_length = required_size.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; storage_length];
        let raw = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        // SAFETY: storage has the size returned by the query and remains owned by AttributeList.
        unsafe { InitializeProcThreadAttributeList(Some(raw), 1, None, &mut required_size)? };
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
}
