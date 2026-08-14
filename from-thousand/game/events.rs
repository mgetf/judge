//! Durable game facts. Serde-tagged JSON (`type` discriminant, snake_case).

use serde::{Deserialize, Serialize};

use super::types::{CellId, Side};

/// Append-only event log for one territorial war.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameEvent {
    GameCreated(GameCreated),
    ForceCommitted(ForceCommitted),
    EpochClosed(EpochClosed),
}

/// Opens a game: grid, homes, and epoch budget.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameCreated {
    pub game_id: String,
    pub width: u16,
    pub height: u16,
    pub black_home: CellId,
    pub white_home: CellId,
    /// Inclusive count of epochs that will be closed before forced resolution (if no majority).
    pub max_epochs: u32,
}

/// Client-authored force commitment for the currently open epoch.
///
/// `payer` is attribution only — it never grants privileges.
/// `commit_id` is the idempotency key; duplicates are rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForceCommitted {
    pub commit_id: String,
    pub side: Side,
    pub cell: CellId,
    /// Strictly positive play-money force (u64). Additive only.
    pub amount: u64,
    pub payer: String,
}

/// Closes the named open epoch: pending → persistent, recompute control/supply, resolve victory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpochClosed {
    /// Must equal the currently open epoch index (1-based after create).
    pub epoch: u32,
}
