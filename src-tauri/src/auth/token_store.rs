use std::error::Error;
use std::fmt;

const REFRESH_TOKEN_TARGET: &str = "MonaLauncher/MicrosoftRefreshToken";

#[derive(Debug)]
pub enum TokenStoreError {
    #[cfg(not(windows))]
    UnsupportedPlatform,
    TokenTooLarge,
    InvalidUtf8(std::string::FromUtf8Error),
    #[cfg(windows)]
    Windows(windows::core::Error),
}

impl fmt::Display for TokenStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(windows))]
            Self::UnsupportedPlatform => {
                write!(formatter, "安全なトークン保存はこのOSに対応していません")
            }
            Self::TokenTooLarge => {
                write!(formatter, "認証トークンがWindowsの保存上限を超えています")
            }
            Self::InvalidUtf8(error) => {
                write!(formatter, "保存された認証トークンが壊れています: {error}")
            }
            #[cfg(windows)]
            Self::Windows(error) => {
                write!(formatter, "Windows資格情報を操作できませんでした: {error}")
            }
        }
    }
}

impl Error for TokenStoreError {}

impl From<std::string::FromUtf8Error> for TokenStoreError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        Self::InvalidUtf8(error)
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for TokenStoreError {
    fn from(error: windows::core::Error) -> Self {
        Self::Windows(error)
    }
}

pub fn save_refresh_token(token: &str) -> Result<(), TokenStoreError> {
    save_secret(REFRESH_TOKEN_TARGET, token)
}

pub fn load_refresh_token() -> Result<Option<String>, TokenStoreError> {
    load_secret(REFRESH_TOKEN_TARGET)
}

pub fn delete_refresh_token() -> Result<(), TokenStoreError> {
    delete_secret(REFRESH_TOKEN_TARGET)
}

#[cfg(windows)]
fn save_secret(target: &str, secret: &str) -> Result<(), TokenStoreError> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    let mut target = wide_null(target);
    let mut username = wide_null("Microsoft account");
    let size = u32::try_from(secret.len()).map_err(|_| TokenStoreError::TokenTooLarge)?;
    if size > CRED_MAX_CREDENTIAL_BLOB_SIZE {
        return Err(TokenStoreError::TokenTooLarge);
    }
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: size,
        CredentialBlob: secret.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };

    // SAFETY: all pointers refer to live buffers for the duration of the synchronous call.
    // CredWriteW copies the credential into the current user's Windows Credential Manager.
    unsafe { CredWriteW(&credential, 0)? };
    Ok(())
}

#[cfg(windows)]
fn load_secret(target: &str) -> Result<Option<String>, TokenStoreError> {
    use std::ptr;
    use std::slice;
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    struct OwnedCredential(*mut CREDENTIALW);
    impl Drop for OwnedCredential {
        fn drop(&mut self) {
            // SAFETY: CredReadW allocated this single credential buffer.
            unsafe { CredFree(self.0.cast()) };
        }
    }

    let target = wide_null(target);
    let mut credential = ptr::null_mut();
    // SAFETY: target is null terminated and credential points to writable output storage.
    let result = unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        )
    };
    if let Err(error) = result {
        if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
            return Ok(None);
        }
        return Err(error.into());
    }
    if credential.is_null() {
        return Ok(None);
    }

    let credential = OwnedCredential(credential);
    // SAFETY: the owned CREDENTIALW and its blob are valid until OwnedCredential is dropped.
    let bytes = unsafe {
        let value = &*credential.0;
        slice::from_raw_parts(
            value.CredentialBlob,
            usize::try_from(value.CredentialBlobSize).unwrap_or(0),
        )
        .to_vec()
    };
    Ok(Some(String::from_utf8(bytes)?))
}

#[cfg(windows)]
fn delete_secret(target: &str) -> Result<(), TokenStoreError> {
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide_null(target);
    // SAFETY: target is a valid null-terminated UTF-16 string.
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(not(windows))]
fn save_secret(_target: &str, _secret: &str) -> Result<(), TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(not(windows))]
fn load_secret(_target: &str) -> Result<Option<String>, TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(not(windows))]
fn delete_secret(_target: &str) -> Result<(), TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_target_is_app_specific() {
        assert_eq!(REFRESH_TOKEN_TARGET, "MonaLauncher/MicrosoftRefreshToken");
    }
}
