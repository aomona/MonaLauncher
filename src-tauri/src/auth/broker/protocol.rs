use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST: usize = 64 * 1024;
pub const MAX_RESPONSE: usize = 256 * 1024;
pub const GAME_TOKEN: &str = "MONALAUNCHER_BROKERED_NO_ACCESS_TOKEN";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Hello {},
    Join { server_hash: String },
    Properties {},
    BlockList {},
    Certificate {},
    Sign { key_id: String, message: String },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrokerError {
    InvalidRequest,
    Unsupported,
    Revoked,
    NetworkDenied,
    RateLimited,
    Unauthorized,
    Forbidden,
    Unavailable,
    InvalidResponse,
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authentication broker: {self:?}")
    }
}
impl std::error::Error for BrokerError {}

pub fn response(id: u64, result: Result<Value, BrokerError>) -> Vec<u8> {
    let value = match result {
        Ok(result) => serde_json::json!({ "id": id, "result": result }),
        Err(error) => serde_json::json!({ "id": id, "error": error }),
    };
    serde_json::to_vec(&value).expect("protocol response contains only serializable JSON")
}

pub fn valid_server_hash(hash: &str) -> bool {
    let digits = hash.strip_prefix('-').unwrap_or(hash);
    !digits.is_empty() && digits.len() <= 40 && digits.bytes().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_generic_proxy_inputs_and_unknown_parameters() {
        for request in [
            r#"{"id":1,"command":{"type":"get","url":"http://localhost/"}}"#,
            r#"{"id":1,"command":{"type":"join","server_hash":"abc","uuid":"other"}}"#,
            r#"{"id":1,"command":{"type":"join","server_hash":"abc","access_token":"fake"}}"#,
            r#"{"id":1,"command":{"type":"hello"},"authorization":"fake"}"#,
        ] {
            assert!(serde_json::from_str::<Request>(request).is_err());
        }
        assert!(valid_server_hash("-abc012"));
        for value in ["", "-", "http://localhost", "\nabc", &"a".repeat(41)] {
            assert!(!valid_server_hash(value));
        }
    }
}
