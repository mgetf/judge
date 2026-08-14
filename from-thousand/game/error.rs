//! Errors from validating / applying game events.

use thiserror::Error;

use super::types::{CellId, Side};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ApplyError {
    #[error("game not created yet")]
    NotCreated,

    #[error("game already created")]
    AlreadyCreated,

    #[error("game already finished")]
    AlreadyFinished,

    #[error("invalid dimensions: width and height must be > 0")]
    InvalidDimensions,

    #[error("max_epochs must be > 0")]
    InvalidMaxEpochs,

    #[error("home cell out of bounds: {0}")]
    HomeOutOfBounds(CellId),

    #[error("black and white homes must be distinct")]
    HomesNotDistinct,

    #[error("cell out of bounds: {0}")]
    CellOutOfBounds(CellId),

    #[error("commit amount must be > 0")]
    ZeroAmount,

    #[error("duplicate commit_id: {0}")]
    DuplicateCommitId(String),

    #[error("cell {cell} is not in {side} supply")]
    NotInSupply { side: Side, cell: CellId },

    #[error("force overflow on cell {cell} for {side}")]
    ForceOverflow { side: Side, cell: CellId },

    #[error("epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch { expected: u32, got: u32 },

    #[error("empty commit_id")]
    EmptyCommitId,

    #[error("empty game_id")]
    EmptyGameId,

    #[error("jsonl io error: {0}")]
    JsonlIo(String),

    #[error("jsonl decode error on line {line}: {message}")]
    JsonlDecode { line: usize, message: String },
}
