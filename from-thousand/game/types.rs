//! Core value types for the plutocratic territorial war MVP.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Contending side. MVP is strictly two-sided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Black,
    White,
}

impl Side {
    pub const ALL: [Side; 2] = [Side::Black, Side::White];

    pub fn opponent(self) -> Side {
        match self {
            Side::Black => Side::White,
            Side::White => Side::Black,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Black => write!(f, "black"),
            Side::White => write!(f, "white"),
        }
    }
}

/// Grid coordinate. Origin `(0,0)` is the north-west corner; `x` grows east, `y` grows south.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct CellId {
    pub x: u16,
    pub y: u16,
}

impl CellId {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

/// Unique-max control of a cell's persistent force. Equal force ⇒ [`Control::Neutral`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    #[default]
    Neutral,
    Black,
    White,
}

impl Control {
    pub fn from_side(side: Side) -> Self {
        match side {
            Side::Black => Control::Black,
            Side::White => Control::White,
        }
    }

    pub fn as_side(self) -> Option<Side> {
        match self {
            Control::Neutral => None,
            Control::Black => Some(Side::Black),
            Control::White => Some(Side::White),
        }
    }
}

/// Terminal outcome after majority or max-epoch resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Winner(Side),
    Draw,
}

/// Lifecycle phase of a game fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// No [`super::events::GameCreated`] applied yet.
    Uninitialized,
    /// Accepting [`super::events::ForceCommitted`] for the open epoch.
    Open,
    /// Terminal; further commits / epoch closes are rejected.
    Finished(Outcome),
}

/// Persistent + pending force on one cell. Pending is epoch-local until [`super::events::EpochClosed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CellForce {
    pub black: u64,
    pub white: u64,
    pub pending_black: u64,
    pub pending_white: u64,
}

impl CellForce {
    pub fn persistent(&self, side: Side) -> u64 {
        match side {
            Side::Black => self.black,
            Side::White => self.white,
        }
    }

    pub fn pending(&self, side: Side) -> u64 {
        match side {
            Side::Black => self.pending_black,
            Side::White => self.pending_white,
        }
    }

    pub fn control(&self) -> Control {
        use std::cmp::Ordering;
        match self.black.cmp(&self.white) {
            Ordering::Greater => Control::Black,
            Ordering::Less => Control::White,
            Ordering::Equal => Control::Neutral,
        }
    }
}
