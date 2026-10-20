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

pub fn set_cookie(name: &str, value: &str, max_age: i64, secure: bool) -> String {
    let mut cookie = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_cookie(name: &str, secure: bool) -> String {
    let mut cookie = format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
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

/// How long a login lasts. The expiry is inside the signed value, so a copied
/// cookie stops working on schedule even if the browser ignores `Max-Age`.
pub const SESSION_TTL_MS: i64 = 30 * 24 * 3_600_000;

/// Signed session value: `{principal}|{expires_ms}`.
pub fn session_value(principal: &PrincipalId, now_ms: i64) -> String {
    format!("{}|{}", principal.as_str(), now_ms + SESSION_TTL_MS)
}

pub fn principal_from_session(value: &str, now_ms: i64) -> Option<PrincipalId> {
    let (id, exp) = value.rsplit_once('|')?;
    let exp: i64 = exp.parse().ok()?;
    if now_ms >= exp {
        return None;
    }
    PrincipalId::parse(id).ok()
}

pub fn principal_from_headers(headers: &HeaderMap, secret: &str) -> Option<PrincipalId> {
    let raw = cookie_from_headers(headers, SESSION_COOKIE)?;
    let value = decode(secret, &raw)?;
    principal_from_session(&value, crate::clock::now_ms())
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

    #[test]
    fn sessions_expire() {
        let p = PrincipalId::parse("100").unwrap();
        let v = session_value(&p, 1_000);
        assert_eq!(principal_from_session(&v, 1_000), Some(p.clone()));
        assert_eq!(principal_from_session(&v, 1_000 + SESSION_TTL_MS), None);
        // Pre-expiry cookies (bare principal id) no longer log anyone in.
        assert_eq!(principal_from_session("100", 1_000), None);
    }
}
