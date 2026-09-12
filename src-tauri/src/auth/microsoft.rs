use std::error::Error;
use std::fmt;
use std::time::Duration;

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode?mkt=ja-JP";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MINECRAFT_SCOPES: &str = "XboxLive.signin offline_access";
const DEFAULT_CLIENT_ID: &str = "f8d68570-e721-4aba-9c3e-1052d41e431a";
const MAX_AUTH_RESPONSE_SIZE: u64 = 64 * 1024;
const VERIFICATION_URIS: [&str; 2] = [
    "https://www.microsoft.com/link",
    "https://microsoft.com/devicelogin",
];
const MAX_DEVICE_CODE_LENGTH: usize = 4096;
const MAX_USER_CODE_LENGTH: usize = 64;
const MAX_TOKEN_LENGTH: usize = 32 * 1024;
const MAX_DEVICE_CODE_LIFETIME: u64 = 60 * 60;
const MAX_POLL_INTERVAL: u64 = 60;

#[derive(Debug)]
pub enum MicrosoftAuthError {
    InvalidClientId,
    Request(reqwest::Error),
    ResponseTooLarge,
    InvalidResponse(serde_json::Error),
    InvalidResponseData(&'static str),
    ServiceStatus(reqwest::StatusCode),
    AuthorizationDeclined,
    AuthorizationExpired,
    ServiceError(String),
    RefreshTokenMissing,
}

impl fmt::Display for MicrosoftAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidClientId => {
                write!(formatter, "Microsoft client IDの形式が正しくありません")
            }
            Self::Request(error) => write!(
                formatter,
                "Microsoft認証サービスへ接続できませんでした: {error}"
            ),
            Self::ResponseTooLarge => {
                write!(formatter, "Microsoft認証サービスの応答が大きすぎます")
            }
            Self::InvalidResponse(error) => write!(
                formatter,
                "Microsoft認証サービスの応答を解釈できませんでした: {error}"
            ),
            Self::InvalidResponseData(field) => write!(
                formatter,
                "Microsoft認証サービスの応答フィールドが正しくありません: {field}"
            ),
            Self::ServiceStatus(status) => write!(
                formatter,
                "Microsoft認証サービスがHTTP {status}を返しました"
            ),
            Self::AuthorizationDeclined => write!(
                formatter,
                "Microsoftアカウントでの認証がキャンセルされました"
            ),
            Self::AuthorizationExpired => {
                write!(formatter, "Microsoft認証コードの有効期限が切れました")
            }
            Self::ServiceError(code) => write!(
                formatter,
                "Microsoft認証サービスがエラーを返しました: {code}"
            ),
            Self::RefreshTokenMissing => {
                write!(formatter, "Microsoft認証応答に更新トークンがありません")
            }
        }
    }
}

impl Error for MicrosoftAuthError {}

impl From<reqwest::Error> for MicrosoftAuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<serde_json::Error> for MicrosoftAuthError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidResponse(error)
    }
}

#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: Duration,
    pub interval: Duration,
}

#[derive(Debug)]
pub enum TokenPoll {
    Pending,
    SlowDown,
    Authorized(MicrosoftAccessToken),
}

#[derive(Debug)]
pub struct MicrosoftAccessToken {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Clone)]
pub struct MicrosoftOAuthClient {
    client: Client,
    client_id: String,
}

impl MicrosoftOAuthClient {
    pub fn configured() -> bool {
        configured_client_id().is_ok()
    }

    pub fn from_configuration() -> Result<Self, MicrosoftAuthError> {
        let client_id = configured_client_id()?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client, client_id })
    }

    pub async fn begin_device_authorization(
        &self,
    ) -> Result<DeviceAuthorization, MicrosoftAuthError> {
        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", MINECRAFT_SCOPES),
            ])
            .send()
            .await?;
        let status = response.status();
        let body: DeviceCodeResponse = parse_bounded_json(response).await?;
        if !status.is_success() {
            return Err(MicrosoftAuthError::ServiceStatus(status));
        }
        validate_device_authorization(&body)?;

        Ok(DeviceAuthorization {
            device_code: body.device_code,
            user_code: body.user_code,
            verification_uri: body.verification_uri,
            expires_in: Duration::from_secs(body.expires_in),
            interval: Duration::from_secs(body.interval),
        })
    }

    pub async fn poll_device_authorization(
        &self,
        device_code: &str,
    ) -> Result<TokenPoll, MicrosoftAuthError> {
        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
            ])
            .send()
            .await?;
        let status = response.status();
        let body: TokenResponse = parse_bounded_json(response).await?;

        match body {
            TokenResponse::Success(success) if status.is_success() => {
                if success.refresh_token.is_empty() {
                    Err(MicrosoftAuthError::RefreshTokenMissing)
                } else {
                    Ok(TokenPoll::Authorized(MicrosoftAccessToken {
                        access_token: validate_token(success.access_token, "access_token")?,
                        refresh_token: validate_token(success.refresh_token, "refresh_token")?,
                    }))
                }
            }
            TokenResponse::Error(error) => match error.error.as_str() {
                "authorization_pending" => Ok(TokenPoll::Pending),
                "slow_down" => Ok(TokenPoll::SlowDown),
                "authorization_declined" => Err(MicrosoftAuthError::AuthorizationDeclined),
                "expired_token" | "bad_verification_code" => {
                    Err(MicrosoftAuthError::AuthorizationExpired)
                }
                code => Err(MicrosoftAuthError::ServiceError(sanitize_error_code(code))),
            },
            TokenResponse::Success(_) => Err(MicrosoftAuthError::ServiceStatus(status)),
        }
    }

    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<MicrosoftAccessToken, MicrosoftAuthError> {
        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("refresh_token", refresh_token),
                ("scope", MINECRAFT_SCOPES),
            ])
            .send()
            .await?;
        let status = response.status();
        let body: TokenResponse = parse_bounded_json(response).await?;

        match body {
            TokenResponse::Success(success) if status.is_success() => Ok(MicrosoftAccessToken {
                access_token: validate_token(success.access_token, "access_token")?,
                refresh_token: if success.refresh_token.is_empty() {
                    validate_token(refresh_token.to_owned(), "refresh_token")?
                } else {
                    validate_token(success.refresh_token, "refresh_token")?
                },
            }),
            TokenResponse::Error(error) => Err(MicrosoftAuthError::ServiceError(
                sanitize_error_code(&error.error),
            )),
            TokenResponse::Success(_) => Err(MicrosoftAuthError::ServiceStatus(status)),
        }
    }
}

fn validate_device_authorization(body: &DeviceCodeResponse) -> Result<(), MicrosoftAuthError> {
    if body.device_code.is_empty()
        || body.device_code.len() > MAX_DEVICE_CODE_LENGTH
        || body.device_code.chars().any(char::is_control)
    {
        return Err(MicrosoftAuthError::InvalidResponseData("device_code"));
    }
    if body.user_code.is_empty()
        || body.user_code.chars().count() > MAX_USER_CODE_LENGTH
        || body.user_code.chars().any(char::is_control)
    {
        return Err(MicrosoftAuthError::InvalidResponseData("user_code"));
    }
    if !VERIFICATION_URIS.contains(&body.verification_uri.as_str()) {
        return Err(MicrosoftAuthError::InvalidResponseData("verification_uri"));
    }
    if body.expires_in == 0 || body.expires_in > MAX_DEVICE_CODE_LIFETIME {
        return Err(MicrosoftAuthError::InvalidResponseData("expires_in"));
    }
    if body.interval == 0 || body.interval > MAX_POLL_INTERVAL {
        return Err(MicrosoftAuthError::InvalidResponseData("interval"));
    }
    Ok(())
}

fn validate_token(token: String, field: &'static str) -> Result<String, MicrosoftAuthError> {
    if token.is_empty() || token.len() > MAX_TOKEN_LENGTH || token.chars().any(char::is_control) {
        return Err(MicrosoftAuthError::InvalidResponseData(field));
    }
    Ok(token)
}

fn sanitize_error_code(code: &str) -> String {
    code.chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(64)
        .collect()
}

fn configured_client_id() -> Result<String, MicrosoftAuthError> {
    let client_id = std::env::var("MONALAUNCHER_MICROSOFT_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            option_env!("MONALAUNCHER_MICROSOFT_CLIENT_ID")
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_owned());

    if is_guid(&client_id) {
        Ok(client_id)
    } else {
        Err(MicrosoftAuthError::InvalidClientId)
    }
}

fn is_guid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

async fn parse_bounded_json<T: DeserializeOwned>(
    mut response: Response,
) -> Result<T, MicrosoftAuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_SIZE)
    {
        return Err(MicrosoftAuthError::ResponseTooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) as u64 > MAX_AUTH_RESPONSE_SIZE {
            return Err(MicrosoftAuthError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_poll_interval")]
    interval: u64,
}

fn default_poll_interval() -> u64 {
    5
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success(TokenSuccessResponse),
    Error(TokenErrorResponse),
}

#[derive(Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(rename = "expires_in")]
    _expires_in: u64,
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contacts Microsoft to validate the configured public client's device flow"]
    fn live_device_authorization_reaches_pending() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let client = MicrosoftOAuthClient::from_configuration().unwrap();
            let challenge = client.begin_device_authorization().await.unwrap();
            // Never print or persist the challenge codes. No user login is performed.
            tokio::time::sleep(challenge.interval).await;
            assert!(matches!(
                client
                    .poll_device_authorization(&challenge.device_code)
                    .await
                    .unwrap(),
                TokenPoll::Pending
            ));
        });
    }

    #[test]
    fn validates_microsoft_client_ids() {
        assert!(is_guid(DEFAULT_CLIENT_ID));
        assert!(is_guid("01234567-89ab-cdef-0123-456789abcdef"));
        assert!(!is_guid("not-a-client-id"));
        assert!(!is_guid("01234567-89ab-cdef-0123-456789abcdeg"));
    }

    #[test]
    fn parses_pending_token_response_without_exposing_tokens() {
        let response: TokenResponse =
            serde_json::from_str(r#"{"error":"authorization_pending"}"#).unwrap();
        assert!(matches!(
            response,
            TokenResponse::Error(TokenErrorResponse { error }) if error == "authorization_pending"
        ));
    }

    #[test]
    fn parses_successful_token_response() {
        let response: TokenResponse = serde_json::from_str(
            r#"{"access_token":"secret-access","refresh_token":"secret-refresh","expires_in":3600}"#,
        )
        .unwrap();
        assert!(matches!(
            response,
            TokenResponse::Success(TokenSuccessResponse { refresh_token, .. })
                if refresh_token == "secret-refresh"
        ));
    }

    #[test]
    fn validates_device_authorization_bounds_and_allowed_uri() {
        let valid = DeviceCodeResponse {
            device_code: "device-secret".to_owned(),
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: VERIFICATION_URIS[0].to_owned(),
            expires_in: 900,
            interval: 5,
        };
        assert!(validate_device_authorization(&valid).is_ok());

        let legacy = DeviceCodeResponse {
            verification_uri: VERIFICATION_URIS[1].to_owned(),
            ..valid
        };
        assert!(validate_device_authorization(&legacy).is_ok());

        let deceptive = DeviceCodeResponse {
            verification_uri: "https://example.com/device".to_owned(),
            ..legacy
        };
        assert!(matches!(
            validate_device_authorization(&deceptive),
            Err(MicrosoftAuthError::InvalidResponseData("verification_uri"))
        ));
    }

    #[test]
    fn sanitizes_unknown_remote_error_codes() {
        assert_eq!(sanitize_error_code("bad\ncode:secret"), "badcodesecret");
        assert_eq!(sanitize_error_code(&"a".repeat(100)).len(), 64);
    }
}
