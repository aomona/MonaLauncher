use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use crate::auth::microsoft::{MicrosoftAuthError, MicrosoftOAuthClient, TokenPoll};
use crate::auth::minecraft_services::{MinecraftServicesClient, MinecraftSession};
use crate::auth::token_store::{delete_refresh_token, load_refresh_token, save_refresh_token};

#[derive(Clone, Default)]
pub struct MicrosoftAuthState {
    pending: Arc<Mutex<Option<PendingAuthorization>>>,
    minecraft_session: Arc<Mutex<Option<CachedMinecraftSession>>>,
}

struct CachedMinecraftSession {
    session: MinecraftSession,
    refresh_at: Instant,
}

struct PendingAuthorization {
    session_id: String,
    device_code: String,
    expires_at: Instant,
    next_poll_at: Instant,
    interval: Duration,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftAuthStatus {
    configured: bool,
    authorized: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftSignInChallenge {
    session_id: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftSignInPoll {
    status: &'static str,
    retry_after: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftAccountProfile {
    name: String,
    uuid: String,
}

#[tauri::command]
pub fn microsoft_auth_status() -> Result<MicrosoftAuthStatus, String> {
    Ok(MicrosoftAuthStatus {
        configured: MicrosoftOAuthClient::configured(),
        authorized: load_refresh_token()
            .map_err(|error| error.to_string())?
            .is_some(),
    })
}

#[tauri::command]
pub async fn begin_microsoft_sign_in(
    state: State<'_, MicrosoftAuthState>,
) -> Result<MicrosoftSignInChallenge, String> {
    let client = MicrosoftOAuthClient::from_configuration().map_err(|error| error.to_string())?;
    let authorization = client
        .begin_device_authorization()
        .await
        .map_err(|error| error.to_string())?;
    let session_id = random_session_id()?;
    let now = Instant::now();

    *state
        .pending
        .lock()
        .map_err(|_| "Microsoft認証状態を利用できません".to_owned())? =
        Some(PendingAuthorization {
            session_id: session_id.clone(),
            device_code: authorization.device_code,
            expires_at: now + authorization.expires_in,
            next_poll_at: now + authorization.interval,
            interval: authorization.interval,
        });

    Ok(MicrosoftSignInChallenge {
        session_id,
        user_code: authorization.user_code,
        verification_uri: authorization.verification_uri,
        expires_in: authorization.expires_in.as_secs(),
        interval: authorization.interval.as_secs(),
    })
}

#[tauri::command]
pub async fn poll_microsoft_sign_in(
    state: State<'_, MicrosoftAuthState>,
    session_id: String,
) -> Result<MicrosoftSignInPoll, String> {
    let now = Instant::now();
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "Microsoft認証状態を利用できません".to_owned())?
        .take()
        .ok_or_else(|| "Microsoft認証セッションがありません".to_owned())?;

    if pending.session_id != session_id {
        restore_pending(&state, pending)?;
        return Err("Microsoft認証セッションが一致しません".to_owned());
    }
    if now >= pending.expires_at {
        return Err("Microsoft認証コードの有効期限が切れました".to_owned());
    }
    if now < pending.next_poll_at {
        let retry_after = pending.next_poll_at.duration_since(now).as_secs().max(1);
        restore_pending(&state, pending)?;
        return Ok(MicrosoftSignInPoll {
            status: "pending",
            retry_after: Some(retry_after),
        });
    }

    let client = MicrosoftOAuthClient::from_configuration().map_err(|error| error.to_string())?;
    match client.poll_device_authorization(&pending.device_code).await {
        Ok(TokenPoll::Authorized(token)) => {
            save_refresh_token(&token.refresh_token).map_err(|error| error.to_string())?;
            Ok(MicrosoftSignInPoll {
                status: "authorized",
                retry_after: None,
            })
        }
        Ok(TokenPoll::Pending) => {
            pending.next_poll_at = Instant::now() + pending.interval;
            let retry_after = pending.interval.as_secs();
            restore_pending(&state, pending)?;
            Ok(MicrosoftSignInPoll {
                status: "pending",
                retry_after: Some(retry_after),
            })
        }
        Ok(TokenPoll::SlowDown) => {
            pending.interval += Duration::from_secs(5);
            pending.next_poll_at = Instant::now() + pending.interval;
            let retry_after = pending.interval.as_secs();
            restore_pending(&state, pending)?;
            Ok(MicrosoftSignInPoll {
                status: "pending",
                retry_after: Some(retry_after),
            })
        }
        Err(error @ MicrosoftAuthError::Request(_)) => {
            pending.next_poll_at = Instant::now() + pending.interval;
            restore_pending(&state, pending)?;
            Err(error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn refresh_minecraft_account(
    state: State<'_, MicrosoftAuthState>,
) -> Result<MinecraftAccountProfile, String> {
    let session = acquire_minecraft_session(&state).await?;
    Ok(MinecraftAccountProfile {
        name: session.player_name,
        uuid: session.uuid,
    })
}

#[tauri::command]
pub fn sign_out_microsoft(state: State<'_, MicrosoftAuthState>) -> Result<(), String> {
    *state
        .pending
        .lock()
        .map_err(|_| "Microsoft認証状態を利用できません".to_owned())? = None;
    *state
        .minecraft_session
        .lock()
        .map_err(|_| "Minecraft認証状態を利用できません".to_owned())? = None;
    delete_refresh_token().map_err(|error| error.to_string())
}

pub(crate) async fn acquire_minecraft_session(
    state: &MicrosoftAuthState,
) -> Result<MinecraftSession, String> {
    if let Some(session) = cached_minecraft_session(state)? {
        return Ok(session);
    }

    let refresh_token = load_refresh_token()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Microsoftアカウントへサインインしてください".to_owned())?;
    let microsoft =
        MicrosoftOAuthClient::from_configuration().map_err(|error| error.to_string())?;
    let access = microsoft
        .refresh_access_token(&refresh_token)
        .await
        .map_err(|error| error.to_string())?;
    save_refresh_token(&access.refresh_token).map_err(|error| error.to_string())?;

    let minecraft = MinecraftServicesClient::new().map_err(|error| error.to_string())?;
    let session = minecraft
        .authenticate(&access.access_token, microsoft.client_id())
        .await
        .map_err(|error| error.to_string())?;
    let refresh_at = Instant::now() + session.expires_in.saturating_sub(Duration::from_secs(60));
    *state
        .minecraft_session
        .lock()
        .map_err(|_| "Minecraft認証状態を利用できません".to_owned())? =
        Some(CachedMinecraftSession {
            session: session.clone(),
            refresh_at,
        });
    Ok(session)
}

fn cached_minecraft_session(
    state: &MicrosoftAuthState,
) -> Result<Option<MinecraftSession>, String> {
    let mut cached = state
        .minecraft_session
        .lock()
        .map_err(|_| "Minecraft認証状態を利用できません".to_owned())?;
    if cached
        .as_ref()
        .is_some_and(|session| Instant::now() < session.refresh_at)
    {
        return Ok(cached.as_ref().map(|session| session.session.clone()));
    }
    *cached = None;
    Ok(None)
}

pub(crate) fn has_microsoft_authorization() -> Result<bool, String> {
    Ok(load_refresh_token()
        .map_err(|error| error.to_string())?
        .is_some())
}

fn restore_pending(
    state: &MicrosoftAuthState,
    pending: PendingAuthorization,
) -> Result<(), String> {
    *state
        .pending
        .lock()
        .map_err(|_| "Microsoft認証状態を利用できません".to_owned())? = Some(pending);
    Ok(())
}

fn random_session_id() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Microsoft認証セッションを作成できませんでした: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_256_bit_hex_values() {
        let first = random_session_id().unwrap();
        let second = random_session_id().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}
