use std::error::Error;
use std::fmt;
use std::string::FromUtf16Error;

use windows::core::{Error as WindowsError, PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, ERROR_ALREADY_EXISTS, HLOCAL};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows::Win32::Security::{FreeSid, PSID};

const PROFILE_PREFIX: &str = "me.aomona.monalauncher.";
const MAX_PROFILE_NAME_LENGTH: usize = 64;

/// AppContainerプロフィールを用意した結果。
#[derive(Debug)]
pub struct AppContainerProfile {
    pub name: String,
    pub sid: String,
    pub created: bool,
}

/// AppContainerプロフィールの処理で発生し得るエラー。
#[derive(Debug)]
pub enum AppContainerProfileError {
    EmptyInstanceId,
    InvalidInstanceIdCharacter(char),
    ProfileNameTooLong { actual: usize, maximum: usize },
    InteriorNullCharacter,

    Utf16(FromUtf16Error),

    Windows(WindowsError),
}

impl fmt::Display for AppContainerProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInstanceId => {
                write!(formatter, "instance ID must not be empty")
            }

            Self::InvalidInstanceIdCharacter(character) => {
                write!(
                    formatter,
                    "instance ID contains an invalid character: {character:?}"
                )
            }

            Self::ProfileNameTooLong { actual, maximum } => {
                write!(
                    formatter,
                    "AppContainer profile name is too long: \
                     {actual} characters; maximum is {maximum}"
                )
            }

            Self::InteriorNullCharacter => {
                write!(
                    formatter,
                    "Windows strings must not contain a null character"
                )
            }

            Self::Windows(error) => {
                write!(formatter, "Windows API error: {error}")
            }

            Self::Utf16(error) => {
                write!(
                    formatter,
                    "Windows returned an invalid UTF-16 string: {error}"
                )
            }
        }
    }
}

impl Error for AppContainerProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Utf16(error) => Some(error),
            Self::Windows(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WindowsError> for AppContainerProfileError {
    fn from(error: WindowsError) -> Self {
        Self::Windows(error)
    }
}

impl From<FromUtf16Error> for AppContainerProfileError {
    fn from(error: FromUtf16Error) -> Self {
        Self::Utf16(error)
    }
}

/// MonaLauncherのインスタンスIDから、AppContainerプロフィール名を作る。
pub fn profile_name_for_instance(instance_id: &str) -> Result<String, AppContainerProfileError> {
    if instance_id.is_empty() {
        return Err(AppContainerProfileError::EmptyInstanceId);
    }

    for character in instance_id.chars() {
        let allowed = character.is_ascii_alphanumeric() || matches!(character, '-' | '_');

        if !allowed {
            return Err(AppContainerProfileError::InvalidInstanceIdCharacter(
                character,
            ));
        }
    }

    let profile_name = format!("{PROFILE_PREFIX}{instance_id}");

    if profile_name.len() > MAX_PROFILE_NAME_LENGTH {
        return Err(AppContainerProfileError::ProfileNameTooLong {
            actual: profile_name.len(),
            maximum: MAX_PROFILE_NAME_LENGTH,
        });
    }

    Ok(profile_name)
}

/// AppContainerプロフィールを作成する。
///
/// 同じ名前のプロフィールが既に存在する場合は、
/// 既存のプロフィールからSIDを取得する。
pub fn ensure_appcontainer_profile(
    profile_name: &str,
) -> Result<AppContainerProfile, AppContainerProfileError> {
    let profile_name_wide = WideCString::new(profile_name)?;
    let display_name_wide = WideCString::new("MonaLauncher Sandbox")?;
    let description_wide = WideCString::new("Sandbox profile managed by MonaLauncher")?;

    // SAFETY:
    // - 3つのPCWSTRは、末尾がNULの有効なUTF-16バッファを指している。
    // - バッファはAPI呼び出しが終わるまで生存している。
    // - Capabilityは指定しないためNoneを渡している。
    let creation_result = unsafe {
        CreateAppContainerProfile(
            profile_name_wide.as_pcwstr(),
            display_name_wide.as_pcwstr(),
            description_wide.as_pcwstr(),
            None,
        )
    };

    let (owned_sid, created) = match creation_result {
        Ok(sid) => (OwnedSid::new(sid), true),

        Err(error) if error.code() == ERROR_ALREADY_EXISTS.to_hresult() => {
            // SAFETY:
            // - profile_name_wideは有効なNUL終端UTF-16文字列。
            // - 返されたSIDはOwnedSidがFreeSidで解放する。
            let sid = unsafe {
                DeriveAppContainerSidFromAppContainerName(profile_name_wide.as_pcwstr())
            }?;

            (OwnedSid::new(sid), false)
        }

        Err(error) => {
            return Err(error.into());
        }
    };

    let sid = sid_to_string(owned_sid.as_raw())?;

    Ok(AppContainerProfile {
        name: profile_name.to_owned(),
        sid,
        created,
    })
}

/// Rustの文字列を、Win32 API用のNUL終端UTF-16文字列として保持する。
#[derive(Debug)]
struct WideCString {
    buffer: Vec<u16>,
}

impl WideCString {
    fn new(value: &str) -> Result<Self, AppContainerProfileError> {
        let mut buffer = Vec::with_capacity(value.len() + 1);

        for unit in value.encode_utf16() {
            if unit == 0 {
                return Err(AppContainerProfileError::InteriorNullCharacter);
            }

            buffer.push(unit);
        }

        // Win32のPCWSTRは末尾のNULを使って文字列の終端を判断する。
        buffer.push(0);

        Ok(Self { buffer })
    }

    fn as_pcwstr(&self) -> PCWSTR {
        PCWSTR(self.buffer.as_ptr())
    }
}

/// FreeSidで解放する必要があるSIDの所有権を表す。
#[derive(Debug)]
struct OwnedSid {
    sid: PSID,
}

impl OwnedSid {
    fn new(sid: PSID) -> Self {
        Self { sid }
    }

    fn as_raw(&self) -> PSID {
        PSID(self.sid.0)
    }
}

impl Drop for OwnedSid {
    fn drop(&mut self) {
        if self.sid.is_invalid() {
            return;
        }

        // SAFETY:
        // - このSIDはCreateAppContainerProfileまたは
        //   DeriveAppContainerSidFromAppContainerNameから取得した。
        // - どちらのAPIもFreeSidによる解放を要求している。
        // - OwnedSidはSIDを一度だけ所有し、Dropも一度だけ実行される。
        unsafe {
            let _ = FreeSid(PSID(self.sid.0));
        }
    }
}

/// バイナリ形式のSIDを、`S-1-15-2-...`形式へ変換する。
fn sid_to_string(sid: PSID) -> Result<String, AppContainerProfileError> {
    let mut string_sid = PWSTR::null();

    // SAFETY:
    // - sidはWindowsから返された有効なSID。
    // - string_sidは結果を書き込むための出力先。
    unsafe {
        ConvertSidToStringSidW(sid, &mut string_sid)?;
    }

    // SAFETY:
    // ConvertSidToStringSidWの成功後は、
    // string_sidがNUL終端UTF-16文字列を指している。
    let result = unsafe { string_sid.to_string() };

    // SAFETY:
    // ConvertSidToStringSidWが確保した領域は、
    // 成功・失敗にかかわらずここで解放する。
    unsafe {
        let _ = LocalFree(Some(HLOCAL(string_sid.0.cast())));
    }

    let sid_string = result?;

    Ok(sid_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_profile_name() {
        let result = profile_name_for_instance("sandbox-test");

        assert_eq!(result.unwrap(), "me.aomona.monalauncher.sandbox-test",);
    }

    #[test]
    fn rejects_empty_instance_id() {
        let result = profile_name_for_instance("");

        assert!(matches!(
            result,
            Err(AppContainerProfileError::EmptyInstanceId),
        ));
    }

    #[test]
    fn rejects_path_separator() {
        let result = profile_name_for_instance("../secret");

        assert!(matches!(
            result,
            Err(AppContainerProfileError::InvalidInstanceIdCharacter('.')),
        ));
    }

    #[test]
    fn rejects_whitespace() {
        let result = profile_name_for_instance("test instance");

        assert!(matches!(
            result,
            Err(AppContainerProfileError::InvalidInstanceIdCharacter(' ')),
        ));
    }

    #[test]
    fn rejects_overly_long_name() {
        let instance_id = "a".repeat(64);

        let result = profile_name_for_instance(&instance_id);

        assert!(matches!(
            result,
            Err(AppContainerProfileError::ProfileNameTooLong { .. }),
        ));
    }
}
