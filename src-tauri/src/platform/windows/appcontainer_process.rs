use std::error::Error;
use std::fmt;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{Error as WindowsError, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::SECURITY_CAPABILITIES;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTUPINFOEXW,
};

use super::appcontainer_profile::{derive_appcontainer_sid, AppContainerProfileError, OwnedSid};
use super::process_token::{process_token_info, ProcessTokenError, ProcessTokenInfo};

/// AppContainerとして起動したプロセスの検査結果。
#[derive(Debug)]
pub struct SpawnedProcessInfo {
    pub process_id: u32,
    pub token_info: ProcessTokenInfo,
}

/// AppContainerプロセスの起動中に発生し得るエラー。
#[derive(Debug)]
pub enum AppContainerProcessError {
    EmptyExecutablePath,
    InteriorNullCharacter,
    Profile(AppContainerProfileError),
    ProcessToken(ProcessTokenError),
    Windows(WindowsError),
}

impl fmt::Display for AppContainerProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExecutablePath => {
                write!(formatter, "executable path must not be empty")
            }
            Self::InteriorNullCharacter => {
                write!(
                    formatter,
                    "Windows strings must not contain a null character"
                )
            }
            Self::Profile(error) => write!(formatter, "AppContainer profile error: {error}"),
            Self::ProcessToken(error) => write!(formatter, "process token error: {error}"),
            Self::Windows(error) => write!(formatter, "Windows API error: {error}"),
        }
    }
}

impl Error for AppContainerProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            Self::ProcessToken(error) => Some(error),
            Self::Windows(error) => Some(error),
            Self::EmptyExecutablePath | Self::InteriorNullCharacter => None,
        }
    }
}

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

impl From<WindowsError> for AppContainerProcessError {
    fn from(error: WindowsError) -> Self {
        Self::Windows(error)
    }
}

/// Probe自身をAppContainerとして起動し、子プロセスのトークンを検査する。
///
/// 引数を自由に受け取る一般的なプロセス起動APIは、コマンドラインのクォート規則を
/// 含めて別途設計する必要がある。そのため、この段階ではProbe専用の引数だけを使う。
pub fn launch_probe_in_appcontainer(
    profile_name: &str,
    probe_executable: &Path,
) -> Result<SpawnedProcessInfo, AppContainerProcessError> {
    let app_container_sid = derive_appcontainer_sid(profile_name)?;
    let attribute_list = AttributeList::new()?;
    let security_capabilities = security_capabilities(&app_container_sid);

    // SAFETY:
    // - attribute_listはInitializeProcThreadAttributeListで初期化済み。
    // - security_capabilitiesはCreateProcessWが呼ばれるまで生存する。
    // - AppContainer SIDはOwnedSidが所有し、同じ呼び出し中は有効なまま保持される。
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

    let executable = WideString::from_path(probe_executable)?;
    let mut child_command_line = WideString::probe_command_line(probe_executable)?;
    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.lpAttributeList = attribute_list.as_raw();

    let mut process_information = PROCESS_INFORMATION::default();

    // SAFETY:
    // - executableはフルパスのNUL終端UTF-16文字列。
    // - child_argumentはCreateProcessWが書き換え可能なNUL終端バッファ。
    // - startup_infoは属性リストを保持し、呼び出し中まで生存する。
    // - process_informationはWindowsが結果を書き込める初期化済み出力先。
    unsafe {
        CreateProcessW(
            PCWSTR(executable.as_ptr()),
            Some(PWSTR(child_command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR::null(),
            &startup_info.StartupInfo,
            &mut process_information,
        )?;
    }

    let process = OwnedHandle::new(process_information.hProcess);
    let _thread = OwnedHandle::new(process_information.hThread);
    let token_info = process_token_info(process.as_raw())?;

    // 子プロセスを残さないよう、検査後に終了を待つ。
    // SAFETY:
    // - processはCreateProcessWが返した有効なプロセスハンドル。
    // - INFINITEにより子プロセスが終了するまで待機する。
    let wait_result = unsafe { WaitForSingleObject(process.as_raw(), INFINITE) };

    if wait_result != WAIT_OBJECT_0 {
        return Err(AppContainerProcessError::Windows(
            WindowsError::from_thread(),
        ));
    }

    Ok(SpawnedProcessInfo {
        process_id: process_information.dwProcessId,
        token_info,
    })
}

fn security_capabilities(app_container_sid: &OwnedSid) -> SECURITY_CAPABILITIES {
    SECURITY_CAPABILITIES {
        AppContainerSid: app_container_sid.as_raw(),
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    }
}

/// PROC_THREAD_ATTRIBUTE_LIST用の、十分なアラインメントを持つ領域を管理する。
struct AttributeList {
    _storage: Vec<usize>,
    raw: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new() -> Result<Self, AppContainerProcessError> {
        let mut required_size = 0_usize;

        // SAFETY:
        // - NULLの属性リストで必要サイズだけを問い合わせるWin32の標準手順。
        // - required_sizeはサイズを書き込める出力先。
        let first_result =
            unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut required_size) };

        if let Err(error) = first_result {
            if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
                return Err(error.into());
            }
        }

        if required_size == 0 {
            return Err(AppContainerProcessError::Windows(
                WindowsError::from_thread(),
            ));
        }

        let storage_length = required_size.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; storage_length];
        let raw = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());

        // SAFETY:
        // - rawはrequired_size以上の書き込み可能な、適切にアラインされた領域を指す。
        // - 属性数1に必要な領域を確保済み。
        unsafe {
            InitializeProcThreadAttributeList(Some(raw), 1, None, &mut required_size)?;
        }

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
        // SAFETY:
        // - rawはこの型のnewで初期化に成功した属性リスト。
        // - 属性リストの寿命が終わるため、Windows APIの対応する解放処理を一度だけ行う。
        unsafe {
            DeleteProcThreadAttributeList(self.raw);
        }
    }
}

struct WideString {
    buffer: Vec<u16>,
}

impl WideString {
    fn from_path(path: &Path) -> Result<Self, AppContainerProcessError> {
        if path.as_os_str().is_empty() {
            return Err(AppContainerProcessError::EmptyExecutablePath);
        }

        let buffer = path.as_os_str().encode_wide().collect::<Vec<_>>();
        Self::from_units(buffer)
    }

    fn probe_command_line(path: &Path) -> Result<Self, AppContainerProcessError> {
        if path.as_os_str().is_empty() {
            return Err(AppContainerProcessError::EmptyExecutablePath);
        }

        let path_units = path.as_os_str().encode_wide().collect::<Vec<_>>();

        if path_units.contains(&0) {
            return Err(AppContainerProcessError::InteriorNullCharacter);
        }

        let mut buffer = Vec::with_capacity(path_units.len() + 1 + 1 + 21);
        buffer.push('"' as u16);
        buffer.extend(path_units);
        buffer.push('"' as u16);
        buffer.push(' ' as u16);
        buffer.extend("--appcontainer-child".encode_utf16());

        Self::from_units(buffer)
    }

    fn from_units(mut buffer: Vec<u16>) -> Result<Self, AppContainerProcessError> {
        if buffer.contains(&0) {
            return Err(AppContainerProcessError::InteriorNullCharacter);
        }

        buffer.push(0);

        Ok(Self { buffer })
    }

    fn as_ptr(&self) -> *const u16 {
        self.buffer.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u16 {
        self.buffer.as_mut_ptr()
    }
}

/// CloseHandleが必要なWindowsハンドルを所有する。
struct OwnedHandle {
    handle: HANDLE,
}

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self { handle }
    }

    fn as_raw(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.handle.is_invalid() {
            return;
        }

        // SAFETY:
        // - handleはCreateProcessWから取得し、このOwnedHandleだけが所有している。
        // - Dropは一度だけ実行されるため、二重解放しない。
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
