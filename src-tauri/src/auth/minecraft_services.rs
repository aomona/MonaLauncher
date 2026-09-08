use std::error::Error;
use std::fmt;
use std::time::Duration;

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const XBOX_USER_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const MINECRAFT_ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const MAX_SERVICE_RESPONSE_SIZE: u64 = 256 * 1024;
const MAX_TOKEN_LENGTH: usize = 32 * 1024;
const MAX_USER_HASH_LENGTH: usize = 256;
const MAX_SESSION_LIFETIME: u64 = 7 * 24 * 60 * 60;

#[derive(Debug)]
pub enum MinecraftAuthError {
    Request(reqwest::Error),
    ResponseTooLarge(&'static str),
    InvalidResponse {
        service: &'static str,
        source: serde_json::Error,
    },
    ServiceStatus {
        service: &'static str,
        status: reqwest::StatusCode,
        message: Option<String>,
    },
    MissingXboxClaim,
    MissingEntitlement,
    InvalidProfile,
    InvalidSession(&'static str),
}

impl fmt::Display for MinecraftAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(
                formatter,
                "Minecraft認証サービスへ接続できませんでした: {error}"
            ),
            Self::ResponseTooLarge(service) => write!(formatter, "{service}の応答が大きすぎます"),
            Self::InvalidResponse { service, source } => {
                write!(formatter, "{service}の応答を解釈できませんでした: {source}")
            }
            Self::ServiceStatus {
                service,
                status,
                message,
            } => {
                write!(formatter, "{service}がHTTP {status}を返しました")?;
                if let Some(message) = message {
                    write!(formatter, ": {message}")?;
                }
                Ok(())
            }
            Self::MissingXboxClaim => {
                write!(formatter, "Xboxプロフィール情報を取得できませんでした")
            }
            Self::MissingEntitlement => write!(
                formatter,
                "このアカウントではMinecraft: Java Editionの所有を確認できませんでした"
            ),
            Self::InvalidProfile => {
                write!(formatter, "Minecraftプロフィールの形式が正しくありません")
            }
            Self::InvalidSession(field) => write!(
                formatter,
                "Minecraft認証サービスの応答フィールドが正しくありません: {field}"
            ),
        }
    }
}

impl Error for MinecraftAuthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            Self::InvalidResponse { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for MinecraftAuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

#[derive(Debug, Clone)]
pub struct MinecraftSession {
    pub player_name: String,
    pub uuid: String,
    pub expires_in: Duration,
}

#[derive(Clone)]
pub struct MinecraftServicesClient {
    client: Client,
}

impl MinecraftServicesClient {
    pub fn new() -> Result<Self, MinecraftAuthError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub async fn authenticate(
        &self,
        microsoft_access_token: &str,
    ) -> Result<MinecraftSession, MinecraftAuthError> {
        validate_token(microsoft_access_token, "microsoft_access_token")?;
        let xbox: XboxTokenResponse = self
            .post_json(
                "Xbox Live authentication",
                XBOX_USER_AUTH_URL,
                &XboxUserAuthenticationRequest {
                    properties: XboxUserAuthenticationProperties {
                        auth_method: "RPS",
                        site_name: "user.auth.xboxlive.com",
                        rps_ticket: format!("d={microsoft_access_token}"),
                    },
                    relying_party: "http://auth.xboxlive.com",
                    token_type: "JWT",
                },
            )
            .await?;
        validate_token(&xbox.token, "xbox_token")?;
        let xsts: XboxTokenResponse = self
            .post_json(
                "Xbox Secure Token Service",
                XSTS_AUTH_URL,
                &XstsAuthorizationRequest {
                    properties: XstsAuthorizationProperties {
                        sandbox_id: "RETAIL",
                        user_tokens: [xbox.token],
                    },
                    relying_party: "rp://api.minecraftservices.com/",
                    token_type: "JWT",
                },
            )
            .await?;
        validate_token(&xsts.token, "xsts_token")?;
        let xsts_claim = first_claim(&xsts)?;
        validate_user_hash(&xsts_claim.uhs)?;
        let user_hash = xsts_claim.uhs.clone();

        let minecraft: MinecraftLoginResponse = self
            .post_json(
                "Minecraft Services authentication",
                MINECRAFT_LOGIN_URL,
                &MinecraftLoginRequest {
                    identity_token: format!("XBL3.0 x={user_hash};{}", xsts.token),
                },
            )
            .await?;
        validate_token(&minecraft.access_token, "minecraft_access_token")?;
        if minecraft.expires_in == 0 || minecraft.expires_in > MAX_SESSION_LIFETIME {
            return Err(MinecraftAuthError::InvalidSession("expires_in"));
        }

        let entitlements: MinecraftEntitlements = self
            .get_bearer_json(
                "Minecraft entitlement service",
                MINECRAFT_ENTITLEMENTS_URL,
                &minecraft.access_token,
            )
            .await?;
        if entitlements.items.is_empty() {
            return Err(MinecraftAuthError::MissingEntitlement);
        }

        let profile: MinecraftProfile = self
            .get_bearer_json(
                "Minecraft profile service",
                MINECRAFT_PROFILE_URL,
                &minecraft.access_token,
            )
            .await?;
        if !valid_profile_id(&profile.id) || !valid_player_name(&profile.name) {
            return Err(MinecraftAuthError::InvalidProfile);
        }

        Ok(MinecraftSession {
            player_name: profile.name,
            uuid: profile.id,
            expires_in: Duration::from_secs(minecraft.expires_in),
        })
    }

    async fn post_json<T: DeserializeOwned, B: Serialize>(
        &self,
        service: &'static str,
        url: &str,
        body: &B,
    ) -> Result<T, MinecraftAuthError> {
        let response = self.client.post(url).json(body).send().await?;
        parse_service_response(service, response).await
    }

    async fn get_bearer_json<T: DeserializeOwned>(
        &self,
        service: &'static str,
        url: &str,
        access_token: &str,
    ) -> Result<T, MinecraftAuthError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await?;
        parse_service_response(service, response).await
    }
}

fn first_claim(response: &XboxTokenResponse) -> Result<&XboxUserClaim, MinecraftAuthError> {
    response
        .display_claims
        .xui
        .first()
        .ok_or(MinecraftAuthError::MissingXboxClaim)
}

async fn parse_service_response<T: DeserializeOwned>(
    service: &'static str,
    mut response: Response,
) -> Result<T, MinecraftAuthError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SERVICE_RESPONSE_SIZE)
    {
        return Err(MinecraftAuthError::ResponseTooLarge(service));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) as u64 > MAX_SERVICE_RESPONSE_SIZE {
            return Err(MinecraftAuthError::ResponseTooLarge(service));
        }
        body.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        let message = serde_json::from_slice::<ServiceErrorResponse>(&body)
            .ok()
            .and_then(|error| error.error_message.or(error.error))
            .map(sanitize_service_message);
        return Err(MinecraftAuthError::ServiceStatus {
            service,
            status,
            message,
        });
    }
    serde_json::from_slice(&body)
        .map_err(|source| MinecraftAuthError::InvalidResponse { service, source })
}

fn sanitize_service_message(message: String) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect()
}

fn valid_profile_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_player_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn validate_token(value: &str, field: &'static str) -> Result<(), MinecraftAuthError> {
    if value.is_empty() || value.len() > MAX_TOKEN_LENGTH || value.chars().any(char::is_control) {
        return Err(MinecraftAuthError::InvalidSession(field));
    }
    Ok(())
}

fn validate_user_hash(value: &str) -> Result<(), MinecraftAuthError> {
    if value.is_empty()
        || value.len() > MAX_USER_HASH_LENGTH
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(MinecraftAuthError::InvalidSession("uhs"));
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XboxUserAuthenticationRequest {
    properties: XboxUserAuthenticationProperties,
    relying_party: &'static str,
    token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XboxUserAuthenticationProperties {
    auth_method: &'static str,
    site_name: &'static str,
    rps_ticket: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XstsAuthorizationRequest {
    properties: XstsAuthorizationProperties,
    relying_party: &'static str,
    token_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct XstsAuthorizationProperties {
    sandbox_id: &'static str,
    user_tokens: [String; 1],
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxTokenResponse {
    token: String,
    display_claims: XboxDisplayClaims,
}

#[derive(Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserClaim>,
}

#[derive(Deserialize)]
struct XboxUserClaim {
    uhs: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftLoginRequest {
    identity_token: String,
}

#[derive(Deserialize)]
struct MinecraftLoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct MinecraftEntitlements {
    items: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct MinecraftProfile {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceErrorResponse {
    error: Option<String>,
    error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_xbox_user_authentication_contract() {
        let request = XboxUserAuthenticationRequest {
            properties: XboxUserAuthenticationProperties {
                auth_method: "RPS",
                site_name: "user.auth.xboxlive.com",
                rps_ticket: "d=secret".to_owned(),
            },
            relying_party: "http://auth.xboxlive.com",
            token_type: "JWT",
        };
        let value = serde_json::to_value(request).unwrap();

        assert_eq!(value["Properties"]["AuthMethod"], "RPS");
        assert_eq!(value["Properties"]["RpsTicket"], "d=secret");
        assert_eq!(value["RelyingParty"], "http://auth.xboxlive.com");
    }

    #[test]
    fn validates_minecraft_profile_fields() {
        assert!(valid_profile_id("0123456789abcdef0123456789abcdef"));
        assert!(!valid_profile_id("../profile"));
        assert!(valid_player_name("Mona_Player"));
        assert!(!valid_player_name("name with spaces"));
    }

    #[test]
    fn sanitizes_remote_error_messages() {
        assert_eq!(
            sanitize_service_message("Invalid app\nregistration".to_owned()),
            "Invalid appregistration"
        );
    }

    #[test]
    fn validates_remote_session_fields() {
        assert!(validate_token("header.payload.signature", "token").is_ok());
        assert!(validate_token("", "token").is_err());
        assert!(validate_user_hash("1234567890abcdef").is_ok());
        assert!(validate_user_hash("bad;claim").is_err());
    }
}
