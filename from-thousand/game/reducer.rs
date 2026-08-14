//! Pure deterministic fold: validate + apply [`GameEvent`]s onto [`GameState`].

use std::collections::BTreeSet;

use super::error::ApplyError;
use super::events::{EpochClosed, ForceCommitted, GameCreated, GameEvent};
use super::state::{resolve_outcome, GameState};
use super::types::{CellForce, CellId, Control, Phase, Side};

/// Apply a single event. On error, `state` is left unchanged.
pub fn apply_event(state: &mut GameState, event: GameEvent) -> Result<(), ApplyError> {
    let mut next = state.clone();
    apply_event_mut(&mut next, event)?;
    *state = next;
    Ok(())
}

/// Fold an iterator of events from an uninitialized state.
pub fn fold_events<I>(events: I) -> Result<GameState, ApplyError>
where
    I: IntoIterator<Item = GameEvent>,
{
    let mut state = GameState::uninitialized();
    for event in events {
        apply_event_mut(&mut state, event)?;
    }
    Ok(state)
}

/// Replay from an already-collected history (clones events into the new state's history).
pub fn replay(history: &[GameEvent]) -> Result<GameState, ApplyError> {
    fold_events(history.iter().cloned())
}

fn apply_event_mut(state: &mut GameState, event: GameEvent) -> Result<(), ApplyError> {
    match &event {
        GameEvent::GameCreated(e) => apply_created(state, e)?,
        GameEvent::ForceCommitted(e) => apply_commit(state, e)?,
        GameEvent::EpochClosed(e) => apply_epoch_closed(state, e)?,
    }
    state.history.push(event);
    Ok(())
}

fn apply_created(state: &mut GameState, e: &GameCreated) -> Result<(), ApplyError> {
    if state.phase != Phase::Uninitialized {
        return Err(ApplyError::AlreadyCreated);
    }
    if e.game_id.is_empty() {
        return Err(ApplyError::EmptyGameId);
    }
    if e.width == 0 || e.height == 0 {
        return Err(ApplyError::InvalidDimensions);
    }
    if e.max_epochs == 0 {
        return Err(ApplyError::InvalidMaxEpochs);
    }
    if e.black_home.x >= e.width || e.black_home.y >= e.height {
        return Err(ApplyError::HomeOutOfBounds(e.black_home));
    }
    if e.white_home.x >= e.width || e.white_home.y >= e.height {
        return Err(ApplyError::HomeOutOfBounds(e.white_home));
    }
    if e.black_home == e.white_home {
        return Err(ApplyError::HomesNotDistinct);
    }

    let n = e.width as usize * e.height as usize;
    state.phase = Phase::Open;
    state.game_id = e.game_id.clone();
    state.width = e.width;
    state.height = e.height;
    state.black_home = e.black_home;
    state.white_home = e.white_home;
    state.max_epochs = e.max_epochs;
    state.open_epoch = 1;
    state.epochs_closed = 0;
    state.cells = vec![CellForce::default(); n];
    state.control = vec![Control::Neutral; n];
    state.applied_commit_ids.clear();
    // Same formula as later epochs: held={home}∪controlled ⇒ home ∪ N4(home) on an empty board.
    state.black_supply = compute_supply(state, Side::Black);
    state.white_supply = compute_supply(state, Side::White);
    Ok(())
}

fn apply_commit(state: &mut GameState, e: &ForceCommitted) -> Result<(), ApplyError> {
    match state.phase {
        Phase::Uninitialized => return Err(ApplyError::NotCreated),
        Phase::Finished(_) => return Err(ApplyError::AlreadyFinished),
        Phase::Open => {}
    }
    if e.commit_id.is_empty() {
        return Err(ApplyError::EmptyCommitId);
    }
    if e.amount == 0 {
        return Err(ApplyError::ZeroAmount);
    }
    if !state.in_bounds(e.cell) {
        return Err(ApplyError::CellOutOfBounds(e.cell));
    }
    if state.applied_commit_ids.contains(&e.commit_id) {
        return Err(ApplyError::DuplicateCommitId(e.commit_id.clone()));
    }
    if !state.supply(e.side).contains(&e.cell) {
        return Err(ApplyError::NotInSupply {
            side: e.side,
            cell: e.cell,
        });
    }

    let idx = state.index(e.cell).expect("in bounds");
    let cell = &mut state.cells[idx];
    match e.side {
        Side::Black => {
            cell.pending_black =
                cell.pending_black
                    .checked_add(e.amount)
                    .ok_or(ApplyError::ForceOverflow {
                        side: e.side,
                        cell: e.cell,
                    })?;
        }
        Side::White => {
            cell.pending_white =
                cell.pending_white
                    .checked_add(e.amount)
                    .ok_or(ApplyError::ForceOverflow {
                        side: e.side,
                        cell: e.cell,
                    })?;
        }
    }
    state.applied_commit_ids.insert(e.commit_id.clone());
    Ok(())
}

fn apply_epoch_closed(state: &mut GameState, e: &EpochClosed) -> Result<(), ApplyError> {
    match state.phase {
        Phase::Uninitialized => return Err(ApplyError::NotCreated),
        Phase::Finished(_) => return Err(ApplyError::AlreadyFinished),
        Phase::Open => {}
    }
    if e.epoch != state.open_epoch {
        return Err(ApplyError::EpochMismatch {
            expected: state.open_epoch,
            got: e.epoch,
        });
    }

    // Atomically: pending → persistent (overflow-checked), clear pending.
    let width = state.width;
    for (idx, cell) in state.cells.iter_mut().enumerate() {
        let at = CellId::new((idx % width as usize) as u16, (idx / width as usize) as u16);
        cell.black =
            cell.black
                .checked_add(cell.pending_black)
                .ok_or(ApplyError::ForceOverflow {
                    side: Side::Black,
                    cell: at,
                })?;
        cell.white =
            cell.white
                .checked_add(cell.pending_white)
                .ok_or(ApplyError::ForceOverflow {
                    side: Side::White,
                    cell: at,
                })?;
        cell.pending_black = 0;
        cell.pending_white = 0;
    }

    // Unique-max control.
    for (i, cell) in state.cells.iter().enumerate() {
        state.control[i] = cell.control();
    }

    // Frozen four-neighbor supply for the next epoch.
    state.black_supply = compute_supply(state, Side::Black);
    state.white_supply = compute_supply(state, Side::White);

    state.epochs_closed += 1;
    let black_n = state.controlled_count(Side::Black);
    let white_n = state.controlled_count(Side::White);
    if let Some(outcome) = resolve_outcome(
        black_n,
        white_n,
        state.cell_count(),
        state.epochs_closed,
        state.max_epochs,
    ) {
        state.phase = Phase::Finished(outcome);
        state.open_epoch = 0;
    } else {
        state.open_epoch = state
            .epochs_closed
            .checked_add(1)
            .expect("epochs_closed + 1 fits u32 when max_epochs is u32");
    }
    Ok(())
}

/// Supply = held ∪ N4(held), where held = {home} ∪ cells uniquely controlled by `side`.
///
/// Home always anchors supply even when contested/neutral, so the empty-board case
/// (home ∪ neighbors) is the same formula — never “home only.”
pub fn compute_supply(state: &GameState, side: Side) -> BTreeSet<CellId> {
    let mut held = BTreeSet::new();
    held.insert(state.home(side));
    for (i, ctrl) in state.control.iter().enumerate() {
        if *ctrl == Control::from_side(side) {
            held.insert(state.cell_id_at(i));
        }
    }
    let mut supply = held.clone();
    for cell in &held {
        for n in state.neighbors4(*cell) {
            supply.insert(n);
        }
    }
    supply
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::game::events::{EpochClosed, ForceCommitted, GameCreated, GameEvent};
    use crate::game::types::{Outcome, Side};

    fn created(w: u16, h: u16, max_epochs: u32) -> GameEvent {
        GameEvent::GameCreated(GameCreated {
            game_id: "g1".into(),
            width: w,
            height: h,
            black_home: CellId::new(0, 0),
            white_home: CellId::new(w - 1, h - 1),
            max_epochs,
        })
    }

    fn commit(id: &str, side: Side, x: u16, y: u16, amount: u64) -> GameEvent {
        GameEvent::ForceCommitted(ForceCommitted {
            commit_id: id.into(),
            side,
            cell: CellId::new(x, y),
            amount,
            payer: "0xpayer".into(),
        })
    }

    fn close(epoch: u32) -> GameEvent {
        GameEvent::EpochClosed(EpochClosed { epoch })
    }

    #[test]
    fn create_initializes_supply_as_home_and_neighbors() {
        let s = fold_events([created(5, 5, 10)]).unwrap();
        assert_eq!(s.phase, Phase::Open);
        assert_eq!(s.open_epoch, 1);
        let b: Vec<_> = s.black_supply.iter().copied().collect();
        assert_eq!(
            b,
            vec![CellId::new(0, 0), CellId::new(0, 1), CellId::new(1, 0)]
        );
        let w: Vec<_> = s.white_supply.iter().copied().collect();
        assert_eq!(
            w,
            vec![CellId::new(3, 4), CellId::new(4, 3), CellId::new(4, 4)]
        );
        assert!(s.control.iter().all(|c| *c == Control::Neutral));
    }

    #[test]
    fn commit_goes_to_pending_only() {
        let mut s = fold_events([created(3, 3, 5)]).unwrap();
        apply_event(&mut s, commit("c1", Side::Black, 0, 0, 7)).unwrap();
        let idx = s.index(CellId::new(0, 0)).unwrap();
        assert_eq!(s.cells[idx].pending_black, 7);
        assert_eq!(s.cells[idx].black, 0);
        assert_eq!(s.control[idx], Control::Neutral);
    }

    #[test]
    fn epoch_close_adds_pending_and_sets_control() {
        let s = fold_events([
            created(3, 3, 5),
            commit("c1", Side::Black, 0, 0, 3),
            commit("c2", Side::White, 2, 2, 2),
            close(1),
        ])
        .unwrap();
        assert_eq!(s.epochs_closed, 1);
        assert_eq!(s.open_epoch, 2);
        assert_eq!(s.cells[0].black, 3);
        assert_eq!(s.cells[0].pending_black, 0);
        assert_eq!(s.control[0], Control::Black);
        let white_home = s.index(CellId::new(2, 2)).unwrap();
        assert_eq!(s.control[white_home], Control::White);
    }

    #[test]
    fn equal_force_is_neutral() {
        let s = fold_events([
            created(3, 3, 5),
            commit("b", Side::Black, 0, 0, 5),
            commit("w", Side::White, 0, 0, 5),
            close(1),
        ]);
        // White cannot commit to black home unless in white supply — on 3x3 white home is (2,2),
        // supply is (2,2),(1,2),(2,1). Black home (0,0) is not in white supply.
        assert!(s.is_err());

        // Use a shared frontier cell: create adjacent homes on a tiny board where supplies overlap.
        // 2x1: black (0,0) supply {(0,0),(1,0)}; white (1,0) supply {(1,0),(0,0)} — both can contest (0,0).
        let s = fold_events([
            GameEvent::GameCreated(GameCreated {
                game_id: "g".into(),
                width: 2,
                height: 1,
                black_home: CellId::new(0, 0),
                white_home: CellId::new(1, 0),
                max_epochs: 3,
            }),
            commit("b", Side::Black, 0, 0, 5),
            commit("w", Side::White, 0, 0, 5),
            close(1),
        ])
        .unwrap();
        assert_eq!(s.control[0], Control::Neutral);
        assert_eq!(s.cells[0].black, 5);
        assert_eq!(s.cells[0].white, 5);
    }

    #[test]
    fn duplicate_commit_id_rejected() {
        let mut s = fold_events([created(3, 3, 5)]).unwrap();
        apply_event(&mut s, commit("same", Side::Black, 0, 0, 1)).unwrap();
        let err = apply_event(&mut s, commit("same", Side::Black, 0, 1, 1)).unwrap_err();
        assert!(matches!(err, ApplyError::DuplicateCommitId(_)));
        assert_eq!(s.history.len(), 2); // create + first commit only
    }

    #[test]
    fn supply_legality_rejects_out_of_supply() {
        let mut s = fold_events([created(5, 5, 5)]).unwrap();
        let err = apply_event(&mut s, commit("x", Side::Black, 2, 2, 1)).unwrap_err();
        assert!(matches!(err, ApplyError::NotInSupply { .. }));
    }

    #[test]
    fn overflow_on_pending_is_hard_error() {
        let mut s = fold_events([created(3, 3, 5)]).unwrap();
        apply_event(&mut s, commit("a", Side::Black, 0, 0, u64::MAX)).unwrap();
        let err = apply_event(&mut s, commit("b", Side::Black, 0, 0, 1)).unwrap_err();
        assert!(matches!(err, ApplyError::ForceOverflow { .. }));
    }

    #[test]
    fn majority_finishes_game() {
        // 2x2 = 4 cells, majority needs 3.
        let s = fold_events([
            GameEvent::GameCreated(GameCreated {
                game_id: "m".into(),
                width: 2,
                height: 2,
                black_home: CellId::new(0, 0),
                white_home: CellId::new(1, 1),
                max_epochs: 10,
            }),
            commit("1", Side::Black, 0, 0, 1),
            commit("2", Side::Black, 1, 0, 1),
            commit("3", Side::Black, 0, 1, 1),
            close(1),
        ])
        .unwrap();
        assert_eq!(s.phase, Phase::Finished(Outcome::Winner(Side::Black)));
        assert_eq!(s.controlled_count(Side::Black), 3);
    }

    #[test]
    fn max_epoch_draw_when_tied() {
        let s = fold_events([
            GameEvent::GameCreated(GameCreated {
                game_id: "d".into(),
                width: 2,
                height: 1,
                black_home: CellId::new(0, 0),
                white_home: CellId::new(1, 0),
                max_epochs: 1,
            }),
            commit("b", Side::Black, 0, 0, 1),
            commit("w", Side::White, 1, 0, 1),
            close(1),
        ])
        .unwrap();
        assert_eq!(s.phase, Phase::Finished(Outcome::Draw));
    }

    #[test]
    fn payer_is_attribution_only_replay_ignores_identity() {
        let a = fold_events([
            created(3, 3, 3),
            GameEvent::ForceCommitted(ForceCommitted {
                commit_id: "c".into(),
                side: Side::Black,
                cell: CellId::new(0, 0),
                amount: 4,
                payer: "alice".into(),
            }),
            close(1),
        ])
        .unwrap();
        let b = fold_events([
            created(3, 3, 3),
            GameEvent::ForceCommitted(ForceCommitted {
                commit_id: "c".into(),
                side: Side::Black,
                cell: CellId::new(0, 0),
                amount: 4,
                payer: "bob".into(),
            }),
            close(1),
        ])
        .unwrap();
        // Force / control identical; only history payer strings differ.
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.control, b.control);
        assert_eq!(a.black_supply, b.black_supply);
    }

    #[test]
    fn failed_apply_does_not_mutate() {
        let mut s = fold_events([created(3, 3, 3)]).unwrap();
        let before = s.clone();
        let _ = apply_event(&mut s, commit("x", Side::Black, 2, 2, 1));
        assert_eq!(s, before);
    }

    #[test]
    fn monotonic_persistent_force() {
        let events = [
            created(3, 3, 5),
            commit("1", Side::Black, 0, 0, 2),
            close(1),
            commit("2", Side::Black, 0, 0, 3),
            close(2),
        ];
        let mut s = GameState::uninitialized();
        let mut prev_total = 0u64;
        for e in events {
            apply_event(&mut s, e).unwrap();
            let total: u64 = s.cells.iter().map(|c| c.black + c.white).sum();
            assert!(total >= prev_total);
            prev_total = total;
        }
        assert_eq!(s.cells[0].black, 5);
    }
}
