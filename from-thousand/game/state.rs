//! Materialized game state derived by folding [`crate::game::events::GameEvent`]s.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::types::{CellForce, CellId, Control, Outcome, Phase, Side};

/// Deterministic territorial-war state. Pure data — mutation only via the reducer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub phase: Phase,
    pub game_id: String,
    pub width: u16,
    pub height: u16,
    pub black_home: CellId,
    pub white_home: CellId,
    pub max_epochs: u32,
    /// Currently open epoch (1-based). Meaningful only while [`Phase::Open`].
    pub open_epoch: u32,
    /// How many epochs have been successfully closed.
    pub epochs_closed: u32,
    pub cells: Vec<CellForce>,
    pub control: Vec<Control>,
    /// Frozen supply for the open epoch (sorted for determinism / snapshots).
    pub black_supply: BTreeSet<CellId>,
    pub white_supply: BTreeSet<CellId>,
    /// Applied commit IDs (idempotency / duplicate rejection).
    pub applied_commit_ids: BTreeSet<String>,
    /// Full applied history — enough to serialize as JSONL and replay.
    pub history: Vec<super::events::GameEvent>,
}

impl Default for GameState {
    fn default() -> Self {
        Self::uninitialized()
    }
}

impl GameState {
    pub fn uninitialized() -> Self {
        Self {
            phase: Phase::Uninitialized,
            game_id: String::new(),
            width: 0,
            height: 0,
            black_home: CellId::new(0, 0),
            white_home: CellId::new(0, 0),
            max_epochs: 0,
            open_epoch: 0,
            epochs_closed: 0,
            cells: Vec::new(),
            control: Vec::new(),
            black_supply: BTreeSet::new(),
            white_supply: BTreeSet::new(),
            applied_commit_ids: BTreeSet::new(),
            history: Vec::new(),
        }
    }

    pub fn cell_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn in_bounds(&self, cell: CellId) -> bool {
        (cell.x as usize) < self.width as usize && (cell.y as usize) < self.height as usize
    }

    pub fn index(&self, cell: CellId) -> Option<usize> {
        if self.in_bounds(cell) {
            Some(cell.y as usize * self.width as usize + cell.x as usize)
        } else {
            None
        }
    }

    pub fn cell_id_at(&self, index: usize) -> CellId {
        let w = self.width as usize;
        CellId::new((index % w) as u16, (index / w) as u16)
    }

    pub fn home(&self, side: Side) -> CellId {
        match side {
            Side::Black => self.black_home,
            Side::White => self.white_home,
        }
    }

    pub fn supply(&self, side: Side) -> &BTreeSet<CellId> {
        match side {
            Side::Black => &self.black_supply,
            Side::White => &self.white_supply,
        }
    }

    pub fn controlled_count(&self, side: Side) -> usize {
        let want = Control::from_side(side);
        self.control.iter().filter(|c| **c == want).count()
    }

    /// Four-neighbors in bounds, in NESW order.
    pub fn neighbors4(&self, cell: CellId) -> Vec<CellId> {
        let mut out = Vec::with_capacity(4);
        let dirs: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
        for (dx, dy) in dirs {
            let nx = cell.x as i32 + dx;
            let ny = cell.y as i32 + dy;
            if nx >= 0 && ny >= 0 && (nx as u16) < self.width && (ny as u16) < self.height {
                out.push(CellId::new(nx as u16, ny as u16));
            }
        }
        out
    }

    /// Exact board view for tests / simulations (persistent force + control + frozen supply).
    pub fn snapshot(&self) -> BoardSnapshot {
        let cells = self
            .cells
            .iter()
            .enumerate()
            .map(|(i, f)| CellSnapshot {
                cell: self.cell_id_at(i),
                black: f.black,
                white: f.white,
                pending_black: f.pending_black,
                pending_white: f.pending_white,
                control: self.control[i],
            })
            .collect();
        BoardSnapshot {
            phase: self.phase,
            open_epoch: self.open_epoch,
            epochs_closed: self.epochs_closed,
            cells,
            black_supply: self.black_supply.iter().copied().collect(),
            white_supply: self.white_supply.iter().copied().collect(),
            black_controlled: self.controlled_count(Side::Black),
            white_controlled: self.controlled_count(Side::White),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellSnapshot {
    pub cell: CellId,
    pub black: u64,
    pub white: u64,
    pub pending_black: u64,
    pub pending_white: u64,
    pub control: Control,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardSnapshot {
    pub phase: Phase,
    pub open_epoch: u32,
    pub epochs_closed: u32,
    pub cells: Vec<CellSnapshot>,
    pub black_supply: Vec<CellId>,
    pub white_supply: Vec<CellId>,
    pub black_controlled: usize,
    pub white_controlled: usize,
}

impl BoardSnapshot {
    pub fn force(&self, cell: CellId) -> Option<(u64, u64)> {
        self.cells
            .iter()
            .find(|c| c.cell == cell)
            .map(|c| (c.black, c.white))
    }

    pub fn control_at(&self, cell: CellId) -> Option<Control> {
        self.cells
            .iter()
            .find(|c| c.cell == cell)
            .map(|c| c.control)
    }
}

/// Outcome of majority / max-epoch resolution (if any).
pub fn resolve_outcome(
    black_controlled: usize,
    white_controlled: usize,
    cell_count: usize,
    epochs_closed: u32,
    max_epochs: u32,
) -> Option<Outcome> {
    let majority_threshold = cell_count / 2 + 1;
    if black_controlled >= majority_threshold {
        return Some(Outcome::Winner(Side::Black));
    }
    if white_controlled >= majority_threshold {
        return Some(Outcome::Winner(Side::White));
    }
    if epochs_closed >= max_epochs {
        return Some(match black_controlled.cmp(&white_controlled) {
            std::cmp::Ordering::Greater => Outcome::Winner(Side::Black),
            std::cmp::Ordering::Less => Outcome::Winner(Side::White),
            std::cmp::Ordering::Equal => Outcome::Draw,
        });
    }
    None
}
