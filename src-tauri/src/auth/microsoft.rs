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
const MAX_AUTH_RESPONSE_SIZE: u64 = 64 * 1024;

#[derive(Debug)]
pub enum MicrosoftAuthError {
    NotConfigured,
    InvalidClientId,
    Request(reqwest::Error),
    ResponseTooLarge,
    InvalidResponse(serde_json::Error),
    ServiceStatus(reqwest::StatusCode),
    AuthorizationDeclined,
    AuthorizationExpired,
    ServiceError(String),
    RefreshTokenMissing,
}

impl fmt::Display for MicrosoftAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(
                formatter,
                "Microsoft認証が未設定です。MONALAUNCHER_MICROSOFT_CLIENT_IDを設定して再ビルドしてください"
            ),
            Self::InvalidClientId => write!(formatter, "Microsoft client IDの形式が正しくありません"),
            Self::Request(error) => write!(formatter, "Microsoft認証サービスへ接続できませんでした: {error}"),
            Self::ResponseTooLarge => write!(formatter, "Microsoft認証サービスの応答が大きすぎます"),
            Self::InvalidResponse(error) => write!(formatter, "Microsoft認証サービスの応答を解釈できませんでした: {error}"),
            Self::ServiceStatus(status) => write!(formatter, "Microsoft認証サービスがHTTP {status}を返しました"),
            Self::AuthorizationDeclined => write!(formatter, "Microsoftアカウントでの認証がキャンセルされました"),
            Self::AuthorizationExpired => write!(formatter, "Microsoft認証コードの有効期限が切れました"),
            Self::ServiceError(code) => write!(formatter, "Microsoft認証サービスがエラーを返しました: {code}"),
            Self::RefreshTokenMissing => write!(formatter, "Microsoft認証応答に更新トークンがありません"),
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
    Authorized { refresh_token: String },
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

        Ok(DeviceAuthorization {
            device_code: body.device_code,
            user_code: body.user_code,
            verification_uri: body.verification_uri,
            expires_in: Duration::from_secs(body.expires_in),
            interval: Duration::from_secs(body.interval.max(1)),
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
                    Ok(TokenPoll::Authorized {
                        refresh_token: success.refresh_token,
                    })
                }
            }
            TokenResponse::Error(error) => match error.error.as_str() {
                "authorization_pending" => Ok(TokenPoll::Pending),
                "slow_down" => Ok(TokenPoll::SlowDown),
                "authorization_declined" => Err(MicrosoftAuthError::AuthorizationDeclined),
                "expired_token" | "bad_verification_code" => {
                    Err(MicrosoftAuthError::AuthorizationExpired)
                }
                code => Err(MicrosoftAuthError::ServiceError(code.to_owned())),
            },
            TokenResponse::Success(_) => Err(MicrosoftAuthError::ServiceStatus(status)),
        }
    }
}

fn configured_client_id() -> Result<String, MicrosoftAuthError> {
    let client_id = option_env!("MONALAUNCHER_MICROSOFT_CLIENT_ID")
        .map(str::to_owned)
        .or_else(|| std::env::var("MONALAUNCHER_MICROSOFT_CLIENT_ID").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or(MicrosoftAuthError::NotConfigured)?;

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
    response: Response,
) -> Result<T, MicrosoftAuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AUTH_RESPONSE_SIZE)
    {
        return Err(MicrosoftAuthError::ResponseTooLarge);
    }
    let body = response.bytes().await?;
    if body.len() as u64 > MAX_AUTH_RESPONSE_SIZE {
        return Err(MicrosoftAuthError::ResponseTooLarge);
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
    #[serde(rename = "access_token")]
    _access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_microsoft_client_ids() {
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
            r#"{"access_token":"secret-access","refresh_token":"secret-refresh"}"#,
        )
        .unwrap();
        assert!(matches!(
            response,
            TokenResponse::Success(TokenSuccessResponse { refresh_token, .. })
                if refresh_token == "secret-refresh"
        ));
    }
}
