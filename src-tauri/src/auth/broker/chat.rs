//! Launch-local chat keys. The game receives an opaque handle, never a private key encoding.
use super::protocol::BrokerError;
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{RsaKeyPair, RSA_PKCS1_SHA256},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const KEY_MARKER: &str = "MONALAUNCHER_REMOTE_CHAT_KEY:";
pub const MAX_CHAT_BYTES: usize = 64 + 1024 + 20 * 256;
const MAX_KEYS: usize = 4;
const MAX_SESSIONS: usize = 64;

#[derive(Default)]
pub struct ChatKeys {
    keys: Vec<ChatKey>,
}

struct ChatKey {
    id: String,
    private: RsaKeyPair,
    certificate: Value,
    expires: SystemTime,
    refresh: SystemTime,
    sessions: BTreeMap<[u8; 16], u32>,
}

impl ChatKeys {
    pub fn cached(&mut self, now: SystemTime) -> Option<Value> {
        self.keys.retain(|key| now < key.expires);
        self.keys
            .last()
            .filter(|key| now < key.refresh)
            .map(|key| key.certificate.clone())
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    pub fn install(&mut self, value: Value, now: SystemTime) -> Result<Value, BrokerError> {
        self.keys.retain(|key| now < key.expires);
        if self.keys.len() >= MAX_KEYS {
            return Err(BrokerError::RateLimited);
        }
        let private_pem = text(&value["keyPair"]["privateKey"], 16384)?;
        let public_pem = text(&value["keyPair"]["publicKey"], 4096)?;
        let private_der = pem(private_pem, &["PRIVATE KEY", "RSA PRIVATE KEY"])?;
        let private = RsaKeyPair::from_pkcs8(&private_der)
            .or_else(|_| RsaKeyPair::from_der(&private_der))
            .map_err(|_| BrokerError::InvalidResponse)?;
        // Minecraft's chat protocol has fixed 256-byte signatures.
        if private.public().modulus_len() != 256 {
            return Err(BrokerError::InvalidResponse);
        }
        let public_der = pem(public_pem, &["PUBLIC KEY", "RSA PUBLIC KEY"])?;
        if public_der != subject_public_key_info(private.public().as_ref()) {
            return Err(BrokerError::InvalidResponse);
        }
        let signature = text(&value["publicKeySignatureV2"], 2048)?;
        let signature_bytes = STANDARD
            .decode(signature)
            .map_err(|_| BrokerError::InvalidResponse)?;
        if !(256..=1024).contains(&signature_bytes.len()) {
            return Err(BrokerError::InvalidResponse);
        }
        let expires_text = text(&value["expiresAt"], 64)?;
        let refresh_text = text(&value["refreshedAfter"], 64)?;
        let expires = timestamp(expires_text)?;
        let refresh = timestamp(refresh_text)?;
        if expires <= now
            || expires.duration_since(now).unwrap_or_default() > Duration::from_secs(31 * 86400)
            || refresh > expires
        {
            return Err(BrokerError::InvalidResponse);
        }
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|_| BrokerError::Unavailable)?;
        let id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let certificate = json!({
            "keyPair": { "privateKey": format!("{KEY_MARKER}{id}"), "publicKey": format!("-----BEGIN RSA PUBLIC KEY-----\n{}\n-----END RSA PUBLIC KEY-----", STANDARD.encode(&public_der)) },
            "publicKeySignatureV2": signature, "expiresAt": expires_text, "refreshedAfter": refresh_text
        });
        self.keys.push(ChatKey {
            id,
            private,
            certificate: certificate.clone(),
            expires,
            refresh: refresh.max(now + Duration::from_secs(60)).min(expires),
            sessions: BTreeMap::new(),
        });
        Ok(certificate)
    }

    pub fn sign(
        &mut self,
        id: &str,
        encoded: &str,
        account: &str,
        now: SystemTime,
    ) -> Result<Value, BrokerError> {
        if id.len() != 64
            || !id.bytes().all(|c| c.is_ascii_hexdigit())
            || encoded.len() > MAX_CHAT_BYTES.div_ceil(3) * 4
        {
            return Err(BrokerError::InvalidRequest);
        }
        let key = self
            .keys
            .iter_mut()
            .find(|key| key.id == id && now < key.expires)
            .ok_or(BrokerError::Revoked)?;
        let message = STANDARD
            .decode(encoded)
            .map_err(|_| BrokerError::InvalidRequest)?;
        let (session, index) = validate_message(&message, account, now)?;
        match key.sessions.get(&session) {
            Some(previous) if index <= *previous => return Err(BrokerError::InvalidRequest),
            None if index != 0 => return Err(BrokerError::InvalidRequest),
            None if key.sessions.len() >= MAX_SESSIONS => return Err(BrokerError::RateLimited),
            _ => {}
        }
        let mut signature = [0; 256];
        key.private
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                &message,
                &mut signature,
            )
            .map_err(|_| BrokerError::Unavailable)?;
        key.sessions.insert(session, index);
        Ok(json!({ "signature": STANDARD.encode(signature) }))
    }
}

fn text(value: &Value, max: usize) -> Result<&str, BrokerError> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max)
        .ok_or(BrokerError::InvalidResponse)
}
fn timestamp(value: &str) -> Result<SystemTime, BrokerError> {
    let time =
        chrono::DateTime::parse_from_rfc3339(value).map_err(|_| BrokerError::InvalidResponse)?;
    let seconds = u64::try_from(time.timestamp()).map_err(|_| BrokerError::InvalidResponse)?;
    UNIX_EPOCH
        .checked_add(Duration::new(seconds, time.timestamp_subsec_nanos()))
        .ok_or(BrokerError::InvalidResponse)
}
fn pem(value: &str, labels: &[&str]) -> Result<Vec<u8>, BrokerError> {
    for label in labels {
        if let Some(body) = value
            .trim()
            .strip_prefix(&format!("-----BEGIN {label}-----"))
            .and_then(|s| s.strip_suffix(&format!("-----END {label}-----")))
        {
            let encoded: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
            return STANDARD
                .decode(encoded)
                .map_err(|_| BrokerError::InvalidResponse);
        }
    }
    Err(BrokerError::InvalidResponse)
}

// Encode only the already validated public key. Do not parse attacker-controlled ASN.1 here.
fn der(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![tag];
    if bytes.len() < 128 {
        result.push(bytes.len() as u8);
    } else {
        let length = bytes.len().to_be_bytes();
        let length = &length[length.iter().position(|b| *b != 0).unwrap()..];
        result.push(0x80 | length.len() as u8);
        result.extend(length);
    }
    result.extend(bytes);
    result
}
fn subject_public_key_info(public: &[u8]) -> Vec<u8> {
    let mut body = vec![
        0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
    ];
    let mut bits = vec![0];
    bits.extend(public);
    body.extend(der(0x03, &bits));
    der(0x30, &body)
}

fn validate_message(
    bytes: &[u8],
    account: &str,
    now: SystemTime,
) -> Result<([u8; 16], u32), BrokerError> {
    let invalid = BrokerError::InvalidRequest;
    if !(64..=MAX_CHAT_BYTES).contains(&bytes.len()) || bytes[..4] != 1_u32.to_be_bytes() {
        return Err(invalid);
    }
    let uuid: String = bytes[4..20].iter().map(|b| format!("{b:02x}")).collect();
    if uuid != account.replace('-', "").to_ascii_lowercase() {
        return Err(BrokerError::Forbidden);
    }
    let session: [u8; 16] = bytes[20..36].try_into().map_err(|_| invalid)?;
    if session == [0; 16] {
        return Err(invalid);
    }
    let index = u32::from_be_bytes(bytes[36..40].try_into().map_err(|_| invalid)?);
    if index > i32::MAX as u32 {
        return Err(invalid);
    }
    let seconds = i64::from_be_bytes(bytes[48..56].try_into().map_err(|_| invalid)?);
    let now = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid)?
        .as_secs();
    let seconds = u64::try_from(seconds).map_err(|_| invalid)?;
    if seconds > now.saturating_add(30) || seconds.saturating_add(300) < now {
        return Err(invalid);
    }
    let len = u32::from_be_bytes(bytes[56..60].try_into().map_err(|_| invalid)?) as usize;
    if len > 1024 || 64 + len > bytes.len() {
        return Err(invalid);
    }
    let content = std::str::from_utf8(&bytes[60..60 + len]).map_err(|_| invalid)?;
    if content.encode_utf16().count() > 256 {
        return Err(invalid);
    }
    let seen =
        u32::from_be_bytes(bytes[60 + len..64 + len].try_into().map_err(|_| invalid)?) as usize;
    if seen > 20 || bytes.len() != 64 + len + seen * 256 {
        return Err(invalid);
    }
    Ok((session, index))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use ring::signature::{UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
    pub(crate) const ACCOUNT: &str = "0123456789abcdef0123456789abcdef";

    pub(crate) fn certificate(now: SystemTime) -> Value {
        let private = include_bytes!("fixtures/test-only-chat-key.pk8");
        let pair = RsaKeyPair::from_pkcs8(private).unwrap();
        let public = subject_public_key_info(pair.public().as_ref());
        let timestamp = |seconds| {
            chrono::DateTime::<chrono::Utc>::from(now + Duration::from_secs(seconds)).to_rfc3339()
        };
        json!({ "keyPair": {
            "privateKey": format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----", STANDARD.encode(private)),
            "publicKey": format!("-----BEGIN RSA PUBLIC KEY-----\n{}\n-----END RSA PUBLIC KEY-----", STANDARD.encode(public))
        }, "publicKeySignatureV2": STANDARD.encode([0; 512]), "expiresAt": timestamp(3600), "refreshedAfter": timestamp(120) })
    }

    pub(crate) fn message(now: SystemTime, session: u8, index: u32) -> Vec<u8> {
        let mut bytes = 1_u32.to_be_bytes().to_vec();
        bytes.extend([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef].repeat(2));
        bytes.extend([session; 16]);
        bytes.extend(index.to_be_bytes());
        bytes.extend(123_i64.to_be_bytes());
        bytes.extend(
            now.duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_be_bytes(),
        );
        let content = "合成鍵での検証".as_bytes();
        bytes.extend((content.len() as u32).to_be_bytes());
        bytes.extend(content);
        bytes.extend(0_u32.to_be_bytes());
        bytes
    }
    fn handle(certificate: &Value) -> &str {
        certificate["keyPair"]["privateKey"]
            .as_str()
            .unwrap()
            .strip_prefix(KEY_MARKER)
            .unwrap()
    }

    #[test]
    fn retains_private_key_and_returns_verifiable_chat_signatures() {
        let now = SystemTime::now();
        let raw = certificate(now);
        let mut keys = ChatKeys::default();
        let public = keys.install(raw.clone(), now).unwrap();
        assert!(!public
            .to_string()
            .contains(raw["keyPair"]["privateKey"].as_str().unwrap()));
        assert!(!public
            .to_string()
            .contains(&STANDARD.encode(include_bytes!("fixtures/test-only-chat-key.pk8"))));
        assert_eq!(public["keyPair"]["publicKey"], raw["keyPair"]["publicKey"]);
        assert_eq!(keys.cached(now), Some(public.clone()));
        let bytes = message(now, 1, 0);
        let signed = keys
            .sign(handle(&public), &STANDARD.encode(&bytes), ACCOUNT, now)
            .unwrap();
        let pair =
            RsaKeyPair::from_pkcs8(include_bytes!("fixtures/test-only-chat-key.pk8")).unwrap();
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, pair.public().as_ref())
            .verify(
                &bytes,
                &STANDARD
                    .decode(signed["signature"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            keys.sign(handle(&public), &STANDARD.encode(&bytes), ACCOUNT, now),
            Err(BrokerError::InvalidRequest)
        );
        assert!(keys
            .sign(
                handle(&public),
                &STANDARD.encode(message(now, 1, 1)),
                ACCOUNT,
                now
            )
            .is_ok());
    }

    #[test]
    fn rejects_wrong_accounts_arbitrary_signing_and_malformed_chat() {
        let now = SystemTime::now();
        let original = message(now, 1, 0);
        assert_eq!(
            validate_message(&original, "ffffffffffffffffffffffffffffffff", now),
            Err(BrokerError::Forbidden)
        );
        let mut mutations = vec![b"arbitrary payload".to_vec()];
        for (offset, replacement) in [
            (0, vec![0; 4]),
            (20, vec![0; 16]),
            (36, u32::MAX.to_be_bytes().to_vec()),
            (48, 0_u64.to_be_bytes().to_vec()),
            (56, 1025_u32.to_be_bytes().to_vec()),
            (60, vec![0xff]),
        ] {
            let mut bad = original.clone();
            bad[offset..offset + replacement.len()].copy_from_slice(&replacement);
            mutations.push(bad);
        }
        let mut trailing = original.clone();
        trailing.push(0);
        mutations.push(trailing);
        let mut seen = original.clone();
        let end = seen.len();
        seen[end - 4..].copy_from_slice(&21_u32.to_be_bytes());
        mutations.push(seen);
        for bad in mutations {
            assert!(validate_message(&bad, ACCOUNT, now).is_err());
        }
        for len in 0..original.len() {
            assert!(validate_message(&original[..len], ACCOUNT, now).is_err());
        }
        assert!(validate_message(&original, ACCOUNT, now + Duration::from_secs(301)).is_err());
        assert!(validate_message(&original, ACCOUNT, now - Duration::from_secs(31)).is_err());
        let mut maximum = original[..56].to_vec();
        let text = "😀".repeat(128);
        maximum.extend((text.len() as u32).to_be_bytes());
        maximum.extend(text.as_bytes());
        maximum.extend(20_u32.to_be_bytes());
        maximum.extend([0; 20 * 256]);
        assert!(validate_message(&maximum, ACCOUNT, now).is_ok());
    }

    #[test]
    fn expiry_revocation_rotation_and_resource_limits_fail_closed() {
        let now = SystemTime::now();
        let mut keys = ChatKeys::default();
        let first = keys.install(certificate(now), now).unwrap();
        assert_eq!(
            keys.sign(
                handle(&first),
                &STANDARD.encode(message(now, 1, 1)),
                ACCOUNT,
                now
            ),
            Err(BrokerError::InvalidRequest)
        );
        for session in 1..=64 {
            assert!(keys
                .sign(
                    handle(&first),
                    &STANDARD.encode(message(now, session, 0)),
                    ACCOUNT,
                    now
                )
                .is_ok());
        }
        assert_eq!(
            keys.sign(
                handle(&first),
                &STANDARD.encode(message(now, 65, 0)),
                ACCOUNT,
                now
            ),
            Err(BrokerError::RateLimited)
        );
        let later = now + Duration::from_secs(121);
        assert!(keys.cached(later).is_none());
        for _ in 1..MAX_KEYS {
            keys.install(certificate(later), later).unwrap();
        }
        assert!(keys
            .sign(
                handle(&first),
                &STANDARD.encode(message(later, 1, 2)),
                ACCOUNT,
                later
            )
            .is_ok());
        assert_eq!(
            keys.install(certificate(later), later),
            Err(BrokerError::RateLimited)
        );
        assert_eq!(
            keys.sign(
                handle(&first),
                &STANDARD.encode(message(later, 1, 3)),
                ACCOUNT,
                now + Duration::from_secs(3600)
            ),
            Err(BrokerError::Revoked)
        );
        keys.clear();
        assert!(keys.cached(now).is_none());
        assert_eq!(
            keys.sign(
                handle(&first),
                &STANDARD.encode(message(now, 1, 3)),
                ACCOUNT,
                now
            ),
            Err(BrokerError::Revoked)
        );
    }

    #[test]
    fn rejects_inconsistent_or_unbounded_certificates() {
        let now = SystemTime::now();
        for (path, invalid) in [
            ("expiresAt", json!("1970-01-01T00:00:00Z")),
            ("refreshedAfter", json!("9999-01-01T00:00:00Z")),
            ("publicKeySignatureV2", json!("bad signature")),
        ] {
            let mut value = certificate(now);
            value[path] = invalid;
            assert!(ChatKeys::default().install(value, now).is_err());
        }
        let mut value = certificate(now);
        value["keyPair"]["publicKey"] =
            json!("-----BEGIN PUBLIC KEY-----AAAA-----END PUBLIC KEY-----");
        assert!(ChatKeys::default().install(value, now).is_err());
        let mut value = certificate(now);
        value["keyPair"]["privateKey"] = json!("x".repeat(16385));
        assert!(ChatKeys::default().install(value, now).is_err());
    }
}
