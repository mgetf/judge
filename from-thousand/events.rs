use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Durable facts for board state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PostSettled(PostSettled),
}

/// One atomic board purchase: 1..N slot writes, one settlement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PostSettled {
    pub ts: i64,
    pub post_id: String,
    pub writes: Vec<SlotWrite>,
    pub total_usdc_micro: u64,
    /// x402 payer address (0x…) when payment settles; absent for unpaid/dev posts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SlotWrite {
    pub slot_index: u16,
    /// Exactly one printable ASCII character (`U+0020`..=`U+007E`).
    #[schemars(length(min = 1, max = 1), regex(pattern = r"^[\x20-\x7E]$"))]
    pub character: String,
    pub price_usdc_micro: u64,
}
