use std::error::Error;
use std::fmt;
use std::mem::size_of;
use std::string::FromUtf16Error;

use windows::core::{Error as WindowsError, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, TokenAppContainerSid, TokenIsAppContainer, TOKEN_APPCONTAINER_INFORMATION,
    TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// 現在のプロセスのアクセストークンから読み取ったAppContainer情報。
#[derive(Debug, PartialEq, Eq)]
pub struct ProcessTokenInfo {
    pub is_app_container: bool,
    pub app_container_sid: Option<String>,
}

/// アクセストークン検査中に発生し得るエラー。
#[derive(Debug)]
pub enum ProcessTokenError {
    Utf16(FromUtf16Error),
    Windows(WindowsError),
}

impl fmt::Display for ProcessTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf16(error) => {
                write!(
                    formatter,
                    "Windows returned an invalid UTF-16 string: {error}"
                )
            }
            Self::Windows(error) => {
                write!(formatter, "Windows API error: {error}")
            }
        }
    }
}

impl Error for ProcessTokenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Utf16(error) => Some(error),
            Self::Windows(error) => Some(error),
        }
    }
}

impl From<FromUtf16Error> for ProcessTokenError {
    fn from(error: FromUtf16Error) -> Self {
        Self::Utf16(error)
    }
}

impl From<WindowsError> for ProcessTokenError {
    fn from(error: WindowsError) -> Self {
        Self::Windows(error)
    }
}

/// 現在のプロセスが持つAppContainer情報を調べる。
pub fn current_process_token_info() -> Result<ProcessTokenInfo, ProcessTokenError> {
    // SAFETY:
    // GetCurrentProcessは現在プロセスを表す疑似ハンドルを返し、引数もポインターも要求しない。
    // OpenProcessTokenで実ハンドルを取得し、その実ハンドルだけOwnedHandleが所有する。
    let process = unsafe { GetCurrentProcess() };
    process_token_info(process)
}

pub(crate) fn process_token_info(process: HANDLE) -> Result<ProcessTokenInfo, ProcessTokenError> {
    let token = open_process_token(process)?;

    let is_app_container = query_token_bool(token.as_raw(), TokenIsAppContainer)?;
    let app_container_sid = query_app_container_sid(token.as_raw())?;

    Ok(ProcessTokenInfo {
        is_app_container,
        app_container_sid,
    })
}

fn open_process_token(process: HANDLE) -> Result<OwnedHandle, ProcessTokenError> {
    let mut token = HANDLE::default();

    // SAFETY:
    // - processはGetCurrentProcessから取得した現在プロセス用の有効な疑似ハンドル。
    // - TOKEN_QUERYだけを要求するため、トークンを変更する権限は要求しない。
    // - tokenは出力先として十分なサイズを持つ初期化済みHANDLE。
    unsafe {
        OpenProcessToken(process, TOKEN_QUERY, &mut token)?;
    }

    Ok(OwnedHandle::new(token))
}

fn query_token_bool(
    token: HANDLE,
    information_class: TOKEN_INFORMATION_CLASS,
) -> Result<bool, ProcessTokenError> {
    let mut value = 0_u32;
    let mut return_length = 0_u32;

    // TokenIsAppContainerはDWORDを返す情報クラス。
    // 0以外をtrueとして扱うのは、Win32のBOOL/DWORDの表現に合わせている。
    // SAFETY:
    // - tokenはOwnedHandleが所有する有効なトークンハンドル。
    // - valueはDWORD一個分の書き込み可能な領域。
    // - GetTokenInformationは呼び出し中だけvalueへ書き込む。
    unsafe {
        GetTokenInformation(
            token,
            information_class,
            Some((&mut value as *mut u32).cast()),
            size_of::<u32>() as u32,
            &mut return_length,
        )?;
    }

    Ok(value != 0)
}

fn query_app_container_sid(token: HANDLE) -> Result<Option<String>, ProcessTokenError> {
    let mut required_length = 0_u32;

    // SAFETY:
    // - NULLの出力先で必要なバッファサイズだけを問い合わせるWin32の標準手順。
    // - required_lengthはサイズを書き込める出力先。
    let first_result =
        unsafe { GetTokenInformation(token, TokenAppContainerSid, None, 0, &mut required_length) };

    if let Err(error) = first_result {
        if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return Err(error.into());
        }
    }

    if required_length < size_of::<TOKEN_APPCONTAINER_INFORMATION>() as u32 {
        return Ok(None);
    }

    // usizeの配列で確保して、TOKEN_APPCONTAINER_INFORMATIONのアラインメントを満たす。
    let storage_length = (required_length as usize).div_ceil(size_of::<usize>());
    let mut storage = vec![0_usize; storage_length];
    let mut return_length = 0_u32;

    // SAFETY:
    // - tokenはTOKEN_QUERY権限で開かれた有効なトークンハンドル。
    // - storageはWindowsが返した必要サイズ以上の書き込み可能な領域。
    unsafe {
        GetTokenInformation(
            token,
            TokenAppContainerSid,
            Some(storage.as_mut_ptr().cast()),
            required_length,
            &mut return_length,
        )?;
    }

    // SAFETY:
    // - GetTokenInformationがTOKEN_APPCONTAINER_INFORMATIONのレイアウトで書き込んだ領域。
    // - storageはusize配列のため、構造体に必要なアラインメントを満たす。
    let information =
        unsafe { std::ptr::read(storage.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>()) };

    if information.TokenAppContainer.is_invalid() {
        return Ok(None);
    }

    Ok(Some(sid_to_string(information.TokenAppContainer)?))
}

/// Windowsが返したバイナリ形式のSIDを、表示可能な文字列へ変換する。
fn sid_to_string(sid: windows::Win32::Security::PSID) -> Result<String, ProcessTokenError> {
    let mut string_sid = PWSTR::null();

    // SAFETY:
    // - sidはGetTokenInformationが返した有効なSIDポインター。
    // - string_sidはWindows APIが結果を書き込む出力先。
    unsafe {
        ConvertSidToStringSidW(sid, &mut string_sid)?;
    }

    // SAFETY:
    // ConvertSidToStringSidWの成功後、string_sidはNUL終端UTF-16文字列を指す。
    let result = unsafe { string_sid.to_string() };

    // SAFETY:
    // ConvertSidToStringSidWが確保した文字列領域はLocalFreeで解放する。
    unsafe {
        let _ = LocalFree(Some(HLOCAL(string_sid.0.cast())));
    }

    Ok(result?)
}

/// CloseHandleが必要なWindowsハンドルの所有権を表す。
#[derive(Debug)]
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
        // - handleはOpenProcessTokenから取得し、このOwnedHandleだけが所有している。
        // - Dropはこの値に対して一度だけ呼ばれるため、二重解放しない。
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
