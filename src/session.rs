use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::ids::PrincipalId;

type HmacSha256 = Hmac<Sha256>;

pub const SESSION_COOKIE: &str = "judge_sid";
pub const OAUTH_STATE_COOKIE: &str = "judge_oauth_state";

fn sign(secret: &str, msg: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn verify(secret: &str, msg: &str, sig_hex: &str) -> bool {
    let expected = sign(secret, msg);
    expected.len() == sig_hex.len()
        && expected
            .bytes()
            .zip(sig_hex.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

pub fn encode(secret: &str, value: &str) -> String {
    format!("{value}.{}", sign(secret, value))
}

pub fn decode(secret: &str, cookie: &str) -> Option<String> {
    let (value, sig) = cookie.rsplit_once('.')?;
    if value.is_empty() || !verify(secret, value, sig) {
        return None;
    }
    Some(value.to_string())
}

pub fn set_cookie(name: &str, value: &str, max_age: i64) -> String {
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}")
}

pub fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

pub fn cookie_from_headers(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

pub fn principal_from_headers(headers: &HeaderMap, secret: &str) -> Option<PrincipalId> {
    let raw = cookie_from_headers(headers, SESSION_COOKIE)?;
    let id = decode(secret, &raw)?;
    PrincipalId::parse(id).ok()
}

pub fn random_state() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let c = encode("secret", "100");
        assert_eq!(decode("secret", &c).as_deref(), Some("100"));
        assert!(decode("other", &c).is_none());
        assert!(decode("secret", "100.deadbeef").is_none());
    }
}
