//! Pure BufRead/Write JSONL helpers for [`GameEvent`].
//!
//! The async [`crate::jsonl_log::JsonlLog`] is intentionally not used here — game tests and
//! offline replay should stay sync and dependency-light.

use std::io::{BufRead, Write};

use super::error::ApplyError;
use super::events::GameEvent;
use super::reducer::fold_events;
use super::state::GameState;

/// Encode events as JSONL (one JSON object per line, trailing newline after each).
pub fn encode_jsonl(events: &[GameEvent], mut w: impl Write) -> Result<(), ApplyError> {
    for event in events {
        let line = serde_json::to_string(event).map_err(|e| ApplyError::JsonlIo(e.to_string()))?;
        writeln!(w, "{line}").map_err(|e| ApplyError::JsonlIo(e.to_string()))?;
    }
    Ok(())
}

/// Decode JSONL into events. Empty lines are skipped. Corrupt lines are hard errors.
pub fn decode_jsonl(r: impl BufRead) -> Result<Vec<GameEvent>, ApplyError> {
    let mut events = Vec::new();
    for (i, line) in r.lines().enumerate() {
        let line_no = i + 1;
        let line = line.map_err(|e| ApplyError::JsonlIo(e.to_string()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event =
            serde_json::from_str::<GameEvent>(trimmed).map_err(|e| ApplyError::JsonlDecode {
                line: line_no,
                message: e.to_string(),
            })?;
        events.push(event);
    }
    Ok(events)
}

/// Decode JSONL bytes and fold into a [`GameState`].
pub fn replay_jsonl(r: impl BufRead) -> Result<GameState, ApplyError> {
    let events = decode_jsonl(r)?;
    fold_events(events)
}

/// Encode `state.history` to JSONL and fold it back (round-trip helper).
pub fn round_trip_replay(state: &GameState) -> Result<GameState, ApplyError> {
    let mut buf = Vec::new();
    encode_jsonl(&state.history, &mut buf)?;
    replay_jsonl(buf.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::events::{EpochClosed, ForceCommitted, GameCreated, GameEvent};
    use crate::game::reducer::replay;
    use crate::game::types::{CellId, Side};
    use std::io::Cursor;

    fn sample_events() -> Vec<GameEvent> {
        vec![
            GameEvent::GameCreated(GameCreated {
                game_id: "rt".into(),
                width: 3,
                height: 3,
                black_home: CellId::new(0, 0),
                white_home: CellId::new(2, 2),
                max_epochs: 4,
            }),
            GameEvent::ForceCommitted(ForceCommitted {
                commit_id: "c1".into(),
                side: Side::Black,
                cell: CellId::new(0, 0),
                amount: 9,
                payer: "p".into(),
            }),
            GameEvent::EpochClosed(EpochClosed { epoch: 1 }),
        ]
    }

    #[test]
    fn jsonl_bytes_round_trip() {
        let events = sample_events();
        let mut buf = Vec::new();
        encode_jsonl(&events, &mut buf).unwrap();
        let decoded = decode_jsonl(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, events);
        let state = replay_jsonl(Cursor::new(&buf)).unwrap();
        assert_eq!(state.history, events);
        assert_eq!(state.cells[0].black, 9);
    }

    #[test]
    fn jsonl_tempfile_round_trip() {
        let events = sample_events();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.jsonl");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            encode_jsonl(&events, &mut f).unwrap();
        }
        let f = std::fs::File::open(&path).unwrap();
        let decoded = decode_jsonl(std::io::BufReader::new(f)).unwrap();
        assert_eq!(decoded, events);
        let replayed = replay(&decoded).unwrap();
        let direct = fold_events(events.clone()).unwrap();
        assert_eq!(replayed.cells, direct.cells);
        assert_eq!(replayed.control, direct.control);
        assert_eq!(replayed.black_supply, direct.black_supply);
        assert_eq!(replayed.white_supply, direct.white_supply);
        assert_eq!(replayed.phase, direct.phase);
    }

    #[test]
    fn serde_tags_are_snake_case() {
        let ev = GameEvent::EpochClosed(EpochClosed { epoch: 2 });
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""type":"epoch_closed""#));
        assert!(s.contains(r#""epoch":2"#));
    }
}
