use super::{
    chat::ChatKeys,
    protocol::{BrokerError, Command, MAX_RESPONSE},
};
use crate::auth::minecraft_services::MinecraftSession;
use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, SystemTime},
};

/// A launch-scoped account lease; implementations must reject account switches and sign-out.
pub trait SessionSource: Send + Sync {
    fn current(&self) -> Result<MinecraftSession, BrokerError>;
    fn valid(&self) -> bool;
}

pub trait Operations: Send {
    fn execute(&mut self, command: &Command) -> Result<Value, BrokerError>;
    fn valid(&self) -> bool;
}

#[derive(Clone, Copy)]
pub enum UserAttributesSchema {
    Authlib6,
    Authlib9,
}

pub struct OfficialOperations {
    source: Arc<dyn SessionSource>,
    account_id: String,
    attributes_schema: UserAttributesSchema,
    client: Client,
    chat_keys: ChatKeys,
    #[cfg(test)]
    test_origin: Option<String>,
}

impl OfficialOperations {
    pub fn new(
        source: Arc<dyn SessionSource>,
        account_id: String,
        attributes_schema: UserAttributesSchema,
    ) -> Result<Self, BrokerError> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| BrokerError::Unavailable)?;
        Ok(Self {
            source,
            account_id,
            attributes_schema,
            client,
            chat_keys: ChatKeys::default(),
            #[cfg(test)]
            test_origin: None,
        })
    }
    fn endpoint(&self, official: &'static str) -> String {
        #[cfg(test)]
        if let Some(origin) = &self.test_origin {
            let path = reqwest::Url::parse(official).expect("constant official URL");
            return format!("{origin}{}", path.path());
        }
        official.to_owned()
    }
}

impl Operations for OfficialOperations {
    fn valid(&self) -> bool {
        self.source.valid()
    }
    fn execute(&mut self, command: &Command) -> Result<Value, BrokerError> {
        if !self.source.valid() {
            self.chat_keys.clear();
            return Err(BrokerError::Revoked);
        }
        let session = self.source.current()?;
        if session.uuid != self.account_id || !self.source.valid() {
            self.chat_keys.clear();
            return Err(BrokerError::Revoked);
        }
        let result = match command {
            Command::Join { server_hash } => {
                if !super::protocol::valid_server_hash(server_hash) {
                    return Err(BrokerError::InvalidRequest);
                }
                let response = self.client.post(self.endpoint("https://sessionserver.mojang.com/session/minecraft/join"))
                    .json(&json!({ "accessToken": session.access_token, "selectedProfile": self.account_id, "serverId": server_hash }))
                    .send().map_err(|_| BrokerError::Unavailable)?;
                if response.status() == reqwest::StatusCode::NO_CONTENT {
                    Ok(Value::Null)
                } else {
                    Err(status_error(response.status()))
                }
            }
            Command::Properties {} => {
                let response = self
                    .client
                    .get(self.endpoint("https://api.minecraftservices.com/player/attributes"))
                    .bearer_auth(&session.access_token)
                    .send()
                    .map_err(|_| BrokerError::Unavailable)?;
                sanitize_properties(read_response(response)?, self.attributes_schema)
            }
            Command::BlockList {} => {
                let response = self
                    .client
                    .get(self.endpoint("https://api.minecraftservices.com/privacy/blocklist"))
                    .bearer_auth(&session.access_token)
                    .send()
                    .map_err(|_| BrokerError::Unavailable)?;
                sanitize_blocks(read_response(response)?)
            }
            Command::Certificate {} => {
                if let Some(cached) = self.chat_keys.cached(SystemTime::now()) {
                    Ok(cached)
                } else {
                    let response = self
                        .client
                        .post(
                            self.endpoint("https://api.minecraftservices.com/player/certificates"),
                        )
                        .bearer_auth(&session.access_token)
                        .send()
                        .map_err(|_| BrokerError::Unavailable)?;
                    self.chat_keys
                        .install(read_response(response)?, SystemTime::now())
                }
            }
            Command::Sign { key_id, message } => {
                self.chat_keys
                    .sign(key_id, message, &self.account_id, SystemTime::now())
            }
            Command::Hello {} => Err(BrokerError::Unsupported),
        };
        if !self.source.valid() {
            self.chat_keys.clear();
            return Err(BrokerError::Revoked);
        }
        // Defense in depth: even an unexpected service echo must not return this credential.
        if result
            .as_ref()
            .is_ok_and(|value| value.to_string().contains(&session.access_token))
        {
            return Err(BrokerError::InvalidResponse);
        }
        result
    }
}

pub(super) fn status_error(status: reqwest::StatusCode) -> BrokerError {
    match status.as_u16() {
        401 => BrokerError::Unauthorized,
        403 => BrokerError::Forbidden,
        429 => BrokerError::RateLimited,
        _ => BrokerError::Unavailable,
    }
}

pub(super) fn read_response(response: Response) -> Result<Value, BrokerError> {
    if !response.status().is_success() {
        return Err(status_error(response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE as u64)
    {
        return Err(BrokerError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BrokerError::Unavailable)?;
    if bytes.len() > MAX_RESPONSE {
        return Err(BrokerError::InvalidResponse);
    }
    serde_json::from_slice(&bytes).map_err(|_| BrokerError::InvalidResponse)
}

fn sanitize_blocks(value: Value) -> Result<Value, BrokerError> {
    let profiles = value["blockedProfiles"]
        .as_array()
        .ok_or(BrokerError::InvalidResponse)?;
    if profiles.len() > 4096 || profiles.iter().any(|v| !v.as_str().is_some_and(valid_uuid)) {
        return Err(BrokerError::InvalidResponse);
    }
    Ok(json!({ "blockedProfiles": profiles }))
}

fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn sanitize_properties(value: Value, schema: UserAttributesSchema) -> Result<Value, BrokerError> {
    let mut privileges = serde_json::Map::new();
    for key in [
        "onlineChat",
        "multiplayerServer",
        "multiplayerRealms",
        "telemetry",
        "optionalTelemetry",
    ] {
        // Missing restrictions are a protocol error, never an implicit grant.
        let enabled = value["privileges"][key]["enabled"]
            .as_bool()
            .ok_or(BrokerError::InvalidResponse)?;
        privileges.insert(key.into(), json!({ "enabled": enabled }));
    }
    let filter = value["profanityFilterPreferences"]["enabled"]
        .as_bool()
        .ok_or(BrokerError::InvalidResponse)?;
    let scopes = value["banStatus"]["bannedScopes"]
        .as_object()
        .ok_or(BrokerError::InvalidResponse)?;
    if scopes.len() > 32 {
        return Err(BrokerError::InvalidResponse);
    }
    let mut bans = serde_json::Map::new();
    for (scope, ban) in scopes {
        if scope.len() > 64
            || !scope
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(BrokerError::InvalidResponse);
        }
        let mut clean = serde_json::Map::new();
        for key in ["banId", "expires", "reason", "reasonMessage"] {
            let field = &ban[key];
            if !field.is_null() && !field.as_str().is_some_and(|s| s.len() <= 4096) {
                return Err(BrokerError::InvalidResponse);
            }
            clean.insert(key.into(), field.clone());
        }
        bans.insert(scope.clone(), Value::Object(clean));
    }
    let mut clean = json!({ "privileges": privileges, "profanityFilterPreferences": { "enabled": filter }, "banStatus": { "bannedScopes": bans } });
    // authlib 9 consumes these restrictions too. Missing values are not grants.
    for (group, fields, allowed) in [
        (
            "friendsPreferences",
            &["friends", "acceptInvites"][..],
            &["DISABLED", "ENABLED"][..],
        ),
        (
            "chatPreferences",
            &["textCommunication"][..],
            &["DISABLED", "FRIENDS_ONLY", "ENABLED"][..],
        ),
    ] {
        if matches!(schema, UserAttributesSchema::Authlib9) || value.get(group).is_some() {
            let mut selected = serde_json::Map::new();
            for field in fields {
                let setting = value[group][*field]
                    .as_str()
                    .filter(|s| allowed.contains(s))
                    .ok_or(BrokerError::InvalidResponse)?;
                selected.insert((*field).into(), json!(setting));
            }
            clean[group] = Value::Object(selected);
        }
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::TcpListener,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct Source(Arc<AtomicBool>);
    impl SessionSource for Source {
        fn valid(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
        fn current(&self) -> Result<MinecraftSession, BrokerError> {
            Ok(MinecraftSession {
                player_name: "Probe".into(),
                uuid: "0123456789abcdef0123456789abcdef".into(),
                access_token: "only-the-test-parent-has-this-credential".into(),
                expires_in: Duration::from_secs(300),
            })
        }
    }
    fn fixture(
        status: &str,
        body: String,
        before_reply: impl FnOnce() + Send + 'static,
    ) -> (
        OfficialOperations,
        std::thread::JoinHandle<Vec<u8>>,
        Arc<AtomicBool>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
            }
            let length = String::from_utf8_lossy(&bytes)
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.parse::<usize>().ok())
                })
                .unwrap_or(0);
            let header = bytes.len();
            bytes.resize(header + length, 0);
            socket.read_exact(&mut bytes[header..]).unwrap();
            before_reply();
            socket.write_all(response.as_bytes()).unwrap();
            bytes
        });
        let valid = Arc::new(AtomicBool::new(true));
        let mut operations = OfficialOperations::new(
            Arc::new(Source(valid.clone())),
            "0123456789abcdef0123456789abcdef".into(),
            UserAttributesSchema::Authlib6,
        )
        .unwrap();
        operations.test_origin = Some(origin);
        (operations, worker, valid)
    }

    #[test]
    fn certificate_is_filtered_cached_and_revoked_with_its_account() {
        use super::super::chat::tests::{certificate, message, ACCOUNT};
        use base64::{engine::general_purpose::STANDARD, Engine};
        let now = SystemTime::now();
        let raw = certificate(now);
        let (mut operations, worker, valid) = fixture("200 OK", raw.to_string(), || {});
        let public = operations.execute(&Command::Certificate {}).unwrap();
        let request = String::from_utf8(worker.join().unwrap()).unwrap();
        assert!(request.starts_with("POST /player/certificates HTTP/1.1\r\n"));
        assert!(request.contains("authorization: Bearer only-the-test-parent-has-this-credential"));
        assert!(!public
            .to_string()
            .contains(raw["keyPair"]["privateKey"].as_str().unwrap()));
        assert_eq!(
            operations.execute(&Command::Certificate {}),
            Ok(public.clone())
        );
        let key_id = public["keyPair"]["privateKey"]
            .as_str()
            .unwrap()
            .strip_prefix(super::super::chat::KEY_MARKER)
            .unwrap()
            .to_owned();
        let command = Command::Sign {
            key_id,
            message: STANDARD.encode(message(now, 1, 0)),
        };
        assert!(operations.execute(&command).is_ok());
        assert_eq!(operations.account_id, ACCOUNT);
        valid.store(false, Ordering::Release);
        assert_eq!(operations.execute(&command), Err(BrokerError::Revoked));
        assert!(operations.chat_keys.cached(now).is_none());
    }

    #[test]
    fn certificate_revoked_during_fetch_is_neither_returned_nor_retained() {
        let valid = Arc::new(AtomicBool::new(true));
        let revoke = valid.clone();
        let (mut operations, worker, _) = fixture(
            "200 OK",
            super::super::chat::tests::certificate(SystemTime::now()).to_string(),
            move || {
                revoke.store(false, Ordering::Release);
            },
        );
        operations.source = Arc::new(Source(valid));
        assert_eq!(
            operations.execute(&Command::Certificate {}),
            Err(BrokerError::Revoked)
        );
        worker.join().unwrap();
        assert!(operations.chat_keys.cached(SystemTime::now()).is_none());
    }

    #[test]
    fn join_sends_only_the_pinned_account_and_returns_no_credential() {
        let (mut operations, worker, _) = fixture("204 No Content", String::new(), || {});
        assert_eq!(
            operations.execute(&Command::Join {
                server_hash: "-abc123".into()
            }),
            Ok(Value::Null)
        );
        let bytes = worker.join().unwrap();
        let request = String::from_utf8(bytes).unwrap();
        assert!(request.starts_with("POST /session/minecraft/join HTTP/1.1\r\n"));
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert!(body["accessToken"] == "only-the-test-parent-has-this-credential");
        assert_eq!(body["selectedProfile"], "0123456789abcdef0123456789abcdef");
        assert_eq!(body["serverId"], "-abc123");
        assert_eq!(body.as_object().unwrap().len(), 3);
    }

    #[test]
    fn rejects_redirects_service_errors_and_oversized_responses() {
        for (status, body, expected) in [
            (
                "302 Found\r\nLocation: http://127.0.0.1:1/must-not-follow",
                String::new(),
                BrokerError::Unavailable,
            ),
            (
                "401 Unauthorized",
                "untrusted-error-body".into(),
                BrokerError::Unauthorized,
            ),
            (
                "200 OK",
                " ".repeat(MAX_RESPONSE + 1),
                BrokerError::InvalidResponse,
            ),
        ] {
            let (mut operations, worker, _) = fixture(status, body, || {});
            assert_eq!(operations.execute(&Command::BlockList {}), Err(expected));
            let bytes = worker.join().unwrap();
            assert!(String::from_utf8_lossy(&bytes)
                .contains("authorization: Bearer only-the-test-parent-has-this-credential"));
        }
    }

    #[test]
    fn drops_successful_http_result_when_account_is_revoked_in_flight() {
        let revoked = Arc::new(AtomicBool::new(true));
        let revoke = revoked.clone();
        let (mut operations, worker, _) =
            fixture("200 OK", "{\"blockedProfiles\":[]}".into(), move || {
                revoke.store(false, Ordering::Release);
            });
        operations.source = Arc::new(Source(revoked));
        assert_eq!(
            operations.execute(&Command::BlockList {}),
            Err(BrokerError::Revoked)
        );
        worker.join().unwrap();
    }

    #[test]
    fn preserves_account_restrictions_and_discards_unrecognized_fields() {
        let mut privileges = serde_json::Map::new();
        for key in [
            "onlineChat",
            "multiplayerServer",
            "multiplayerRealms",
            "telemetry",
            "optionalTelemetry",
        ] {
            privileges.insert(key.into(), json!({ "enabled": false }));
        }
        let clean = sanitize_properties(json!({ "privileges": privileges, "profanityFilterPreferences": { "enabled": true }, "banStatus": { "bannedScopes": {} }, "accessToken": "not-returned" }), UserAttributesSchema::Authlib6).unwrap();
        assert_eq!(clean["privileges"]["multiplayerServer"]["enabled"], false);
        assert!(clean.get("accessToken").is_none());
        assert!(sanitize_properties(clean.clone(), UserAttributesSchema::Authlib9).is_err());
        let mut social = clean.clone();
        social["friendsPreferences"] =
            json!({ "friends": "DISABLED", "acceptInvites": "DISABLED", "extra": "not-returned" });
        social["chatPreferences"] = json!({ "textCommunication": "FRIENDS_ONLY" });
        let sanitized =
            sanitize_properties(social.clone(), UserAttributesSchema::Authlib9).unwrap();
        assert_eq!(
            sanitized["chatPreferences"]["textCommunication"],
            "FRIENDS_ONLY"
        );
        assert_eq!(sanitized["friendsPreferences"]["friends"], "DISABLED");
        assert!(sanitized["friendsPreferences"].get("extra").is_none());
        social["chatPreferences"]["textCommunication"] = json!("unknown");
        assert!(sanitize_properties(social, UserAttributesSchema::Authlib9).is_err());
        assert!(sanitize_properties(json!({}), UserAttributesSchema::Authlib6).is_err());
        assert!(sanitize_blocks(json!({ "blockedProfiles": ["credential-like-value"] })).is_err());
    }
}
