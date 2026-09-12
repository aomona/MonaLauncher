use std::error::Error;
use std::fmt;

const REFRESH_TOKEN_TARGET: &str = "MonaLauncher/MicrosoftRefreshToken";

#[derive(Debug)]
pub enum TokenStoreError {
    Worker(String),
    #[cfg(not(any(windows, target_os = "macos")))]
    UnsupportedPlatform,
    #[cfg(windows)]
    TokenTooLarge,
    InvalidUtf8(std::string::FromUtf8Error),
    #[cfg(windows)]
    Windows(windows::core::Error),
    #[cfg(target_os = "macos")]
    Keychain(security_framework::base::Error),
}

impl fmt::Display for TokenStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Worker(error) => write!(
                formatter,
                "認証情報のバックグラウンド処理に失敗しました: {error}"
            ),
            #[cfg(not(any(windows, target_os = "macos")))]
            Self::UnsupportedPlatform => {
                write!(formatter, "安全なトークン保存はこのOSに対応していません")
            }
            #[cfg(windows)]
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
            #[cfg(target_os = "macos")]
            Self::Keychain(error) => {
                write!(
                    formatter,
                    "macOSキーチェーンを操作できませんでした: {error}"
                )
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

// Credential APIs can wait for OS consent for an unbounded time. Keep every production
// access on the blocking pool, including calls made from already-async Tauri commands.
pub async fn save_refresh_token(token: &str) -> Result<(), TokenStoreError> {
    let token = token.to_owned();
    run_store_operation(move || save_secret(REFRESH_TOKEN_TARGET, &token)).await
}

pub async fn load_refresh_token() -> Result<Option<String>, TokenStoreError> {
    run_store_operation(|| load_secret(REFRESH_TOKEN_TARGET)).await
}

pub async fn delete_refresh_token() -> Result<(), TokenStoreError> {
    run_store_operation(|| delete_secret(REFRESH_TOKEN_TARGET)).await
}

async fn run_store_operation<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, TokenStoreError> + Send + 'static,
) -> Result<T, TokenStoreError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| TokenStoreError::Worker(error.to_string()))?
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

#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "Microsoft account";
// errSecItemNotFound from Security.framework. Other errors must remain visible.
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn save_secret(target: &str, secret: &str) -> Result<(), TokenStoreError> {
    security_framework::passwords::set_generic_password(target, KEYCHAIN_ACCOUNT, secret.as_bytes())
        .map_err(TokenStoreError::Keychain)
}

#[cfg(target_os = "macos")]
fn load_secret(target: &str) -> Result<Option<String>, TokenStoreError> {
    use security_framework::passwords::{generic_password, PasswordOptions};

    match generic_password(PasswordOptions::new_generic_password(
        target,
        KEYCHAIN_ACCOUNT,
    )) {
        Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(TokenStoreError::Keychain(error)),
    }
}

#[cfg(target_os = "macos")]
fn delete_secret(target: &str) -> Result<(), TokenStoreError> {
    match security_framework::passwords::delete_generic_password(target, KEYCHAIN_ACCOUNT) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(TokenStoreError::Keychain(error)),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn save_secret(_target: &str, _secret: &str) -> Result<(), TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn load_secret(_target: &str) -> Result<Option<String>, TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn delete_secret(_target: &str) -> Result<(), TokenStoreError> {
    Err(TokenStoreError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_credential_consent_yields_the_calling_thread() {
        use std::future::Future;
        use std::sync::mpsc;
        use std::task::{Context, Poll, Waker};
        use std::time::Duration;

        let caller = std::thread::current().id();
        let (entered, worker) = mpsc::channel();
        let (release, consent) = mpsc::channel();
        let mut operation = std::pin::pin!(run_store_operation(move || {
            entered.send(std::thread::current().id()).unwrap();
            // Simulates a blocked OS prompt without accessing the real keychain. The timeout
            // also bounds the test if a regression runs this closure on the calling thread.
            consent
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| TokenStoreError::Worker(error.to_string()))?;
            Ok(42)
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(
            operation.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_ne!(worker.recv_timeout(Duration::from_secs(5)).unwrap(), caller);

        // Unrelated work can still run while the credential operation is blocked.
        let mut unrelated = std::pin::pin!(async { "responsive" });
        assert_eq!(
            unrelated.as_mut().poll(&mut context),
            Poll::Ready("responsive")
        );
        release.send(()).unwrap();
        assert_eq!(tauri::async_runtime::block_on(operation).unwrap(), 42);
    }

    #[test]
    fn credential_worker_preserves_storage_errors() {
        let result = tauri::async_runtime::block_on(run_store_operation(|| {
            String::from_utf8(vec![0xff]).map_err(TokenStoreError::InvalidUtf8)
        }));
        assert!(matches!(result, Err(TokenStoreError::InvalidUtf8(_))));
    }

    #[test]
    fn credential_target_is_app_specific() {
        assert_eq!(REFRESH_TOKEN_TARGET, "MonaLauncher/MicrosoftRefreshToken");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "writes a disposable item to the current user's macOS keychain"]
    fn macos_keychain_round_trip() {
        // Use a unique service, never the real account's refresh token.
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let target = format!("MonaLauncher/Test/{nonce:x?}");
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_secret(&self.0);
            }
        }
        let _cleanup = Cleanup(target.clone());
        assert!(load_secret(&target).unwrap().is_none());
        delete_secret(&target).unwrap();
        save_secret(&target, "test-only-first").unwrap();
        assert_eq!(
            load_secret(&target).unwrap().as_deref(),
            Some("test-only-first")
        );
        save_secret(&target, "test-only-rotated").unwrap();
        assert_eq!(
            load_secret(&target).unwrap().as_deref(),
            Some("test-only-rotated")
        );
        delete_secret(&target).unwrap();
        assert!(load_secret(&target).unwrap().is_none());
        delete_secret(&target).unwrap();
    }
}
