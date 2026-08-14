use crate::types::Ts;

pub fn now_ms() -> Ts {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as Ts)
        .unwrap_or(0)
}
