//! Materialized view: [`SLOT_COUNT`] slots — each slot has one visible character and current price.
//!
//! ## History
//!
//! - [`ReducerState::transactions`] — every [`Event`] applied, in order (same order as the durable
//!   log when replayed 1:1). Query by index for full payload at “query time”.
//! - [`ReducerState::slot_history`] — per slot, ordered list of **event indices** into
//!   `transactions` where that slot was updated by a valid in-range post write.

use crate::events::{Event, PostSettled, SlotWrite};
use crate::post::ValidatePostError;

pub const BOARD_COLS: usize = 25;
pub const BOARD_ROWS: usize = 40;
pub const SLOT_COUNT: usize = BOARD_COLS * BOARD_ROWS;

/// Index into [`ReducerState::transactions`].
pub type EventIndex = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHistoryRef {
    pub event_index: EventIndex,
    pub write_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotState {
    pub character: String,
    pub price_usdc_micro: u64,
}

pub struct ReducerState {
    pub slots: Vec<SlotState>,
    /// Monotonic record of every applied event, in order (`transactions[i]` is the *i*-th event).
    pub transactions: Vec<Event>,
    /// Per slot: posts that updated this slot (in-range writes only).
    pub slot_history: Vec<Vec<SlotHistoryRef>>,
}

impl Default for ReducerState {
    fn default() -> Self {
        Self {
            slots: vec![
                SlotState {
                    character: " ".to_string(),
                    price_usdc_micro: 0,
                };
                SLOT_COUNT
            ],
            transactions: Vec::new(),
            slot_history: (0..SLOT_COUNT).map(|_| Vec::new()).collect(),
        }
    }
}

impl ReducerState {
    /// Lookup the *n*-th committed event (same index as in the JSONL order when replayed in full).
    pub fn event_by_index(&self, index: EventIndex) -> Option<&Event> {
        self.transactions.get(index)
    }

    /// How many events have been applied (len of the logical event log in memory).
    pub fn transaction_count(&self) -> usize {
        self.transactions.len()
    }

    /// Apply `writes` on a copy of the current slots (no event log). For dry-run board preview.
    pub fn preview_with_writes(&self, writes: &[SlotWrite]) -> Result<Self, ValidatePostError> {
        let mut preview = ReducerState {
            slots: self.slots.clone(),
            transactions: Vec::new(),
            slot_history: (0..SLOT_COUNT).map(|_| Vec::new()).collect(),
        };
        for write in writes {
            let i = write.slot_index as usize;
            if i >= SLOT_COUNT {
                continue;
            }
            let c = crate::post::validate_slot_character(&write.character)?;
            preview.slots[i].character = c.to_string();
            preview.slots[i].price_usdc_micro = write.price_usdc_micro;
        }
        Ok(preview)
    }

    pub fn apply_event(&mut self, event: Event) -> Result<(), ValidatePostError> {
        let Event::PostSettled(ref p) = event;
        let event_index = self.transactions.len();
        self.apply_post(p, event_index)?;
        self.transactions.push(event);
        Ok(())
    }

    fn apply_post(
        &mut self,
        post: &PostSettled,
        event_index: EventIndex,
    ) -> Result<(), ValidatePostError> {
        for (write_index, write) in post.writes.iter().enumerate() {
            self.apply_write(write, event_index, write_index)?;
        }
        Ok(())
    }

    fn apply_write(
        &mut self,
        write: &SlotWrite,
        event_index: EventIndex,
        write_index: usize,
    ) -> Result<(), ValidatePostError> {
        let i = write.slot_index as usize;
        if i >= SLOT_COUNT {
            return Ok(());
        }
        let c = crate::post::validate_slot_character(&write.character)?;
        self.slots[i].character = c.to_string();
        self.slots[i].price_usdc_micro = write.price_usdc_micro;
        self.slot_history[i].push(SlotHistoryRef {
            event_index,
            write_index,
        });
        Ok(())
    }

    /// Find the write for `slot_index` inside a settled post event.
    pub fn slot_write_in_post<'a>(post: &'a PostSettled, slot_index: u16) -> Option<&'a SlotWrite> {
        post.writes.iter().find(|w| w.slot_index == slot_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    fn post(ts: i64, writes: Vec<SlotWrite>) -> Event {
        let total = writes.iter().map(|w| w.price_usdc_micro).sum();
        Event::PostSettled(PostSettled {
            ts,
            post_id: format!("post-{ts}"),
            writes,
            total_usdc_micro: total,
            payer: None,
        })
    }

    fn write(idx: u16, ch: &str, price: u64) -> SlotWrite {
        SlotWrite {
            slot_index: idx,
            character: ch.to_string(),
            price_usdc_micro: price,
        }
    }

    #[test]
    fn default_board_is_spaces() {
        let s = ReducerState::default();
        assert_eq!(s.slots.len(), SLOT_COUNT);
        assert!(s
            .slots
            .iter()
            .all(|x| x.character == " " && x.price_usdc_micro == 0));
        assert!(s.transactions.is_empty());
        assert!(s.slot_history.iter().all(|h| h.is_empty()));
    }

    #[test]
    fn post_sets_multiple_slots_atomically_in_one_event() {
        let mut s = ReducerState::default();
        s.apply_event(post(
            1,
            vec![write(0, "h", 1), write(1, "i", 2), write(2, "!", 3)],
        ))
        .unwrap();
        assert_eq!(s.slots[0].character, "h");
        assert_eq!(s.slots[1].character, "i");
        assert_eq!(s.slots[2].character, "!");
        assert_eq!(s.transactions.len(), 1);
        assert_eq!(
            s.slot_history[0],
            vec![SlotHistoryRef {
                event_index: 0,
                write_index: 0
            }]
        );
        assert_eq!(
            s.slot_history[1],
            vec![SlotHistoryRef {
                event_index: 0,
                write_index: 1
            }]
        );
    }

    #[test]
    fn out_of_range_write_is_ignored_but_event_is_recorded() {
        let mut s = ReducerState::default();
        let ev = post(1, vec![write(SLOT_COUNT as u16, "!", 1)]);
        s.apply_event(ev.clone()).unwrap();
        assert_eq!(s.slots[SLOT_COUNT - 1].character, " ");
        assert_eq!(s.transactions.len(), 1);
        assert_eq!(s.event_by_index(0), Some(&ev));
        assert!(s.slot_history.iter().all(|h| h.is_empty()));
    }

    #[test]
    fn preview_with_writes_shows_placements() {
        let mut s = ReducerState::default();
        s.slots[0].character = "x".to_string();
        let preview = s.preview_with_writes(&[write(1, "y", 2)]).unwrap();
        assert_eq!(preview.slots[0].character, "x");
        assert_eq!(preview.slots[1].character, "y");
        assert_eq!(preview.slots[1].price_usdc_micro, 2);
    }

    #[test]
    fn apply_event_rejects_non_ascii_character() {
        let mut s = ReducerState::default();
        let err = s.apply_event(post(1, vec![write(3, "é", 1)])).unwrap_err();
        assert!(matches!(err, ValidatePostError::InvalidCharacter { .. }));
        assert!(s.transactions.is_empty());
    }

    #[test]
    fn replay_two_posts_same_slot() {
        let mut s = ReducerState::default();
        s.apply_event(post(1, vec![write(10, "a", 1)])).unwrap();
        s.apply_event(post(2, vec![write(10, "b", 2)])).unwrap();
        assert_eq!(s.slots[10].character, "b");
        assert_eq!(s.slots[10].price_usdc_micro, 2);
        assert_eq!(
            s.slot_history[10],
            vec![
                SlotHistoryRef {
                    event_index: 0,
                    write_index: 0
                },
                SlotHistoryRef {
                    event_index: 1,
                    write_index: 0
                }
            ]
        );
    }

    #[test]
    fn full_replay_from_vec_matches_incremental() {
        let events = vec![
            post(1, vec![write(0, "x", 1)]),
            post(2, vec![write(1, "y", 2)]),
            post(3, vec![write(0, "z", 3)]),
        ];
        let mut incremental = ReducerState::default();
        for e in &events {
            incremental.apply_event(e.clone()).unwrap();
        }

        let mut replay = ReducerState::default();
        for e in &events {
            replay.apply_event(e.clone()).unwrap();
        }

        assert_eq!(incremental.transactions, replay.transactions);
        assert_eq!(
            incremental.slot_history[0],
            vec![
                SlotHistoryRef {
                    event_index: 0,
                    write_index: 0
                },
                SlotHistoryRef {
                    event_index: 2,
                    write_index: 0
                }
            ]
        );
        assert_eq!(incremental.slots[0].character, "z");
    }

    #[test]
    fn slot_write_in_post_finds_matching_write() {
        let p = PostSettled {
            ts: 1,
            post_id: "x".into(),
            writes: vec![write(5, "a", 1), write(9, "b", 2)],
            total_usdc_micro: 3,
            payer: None,
        };
        assert_eq!(
            ReducerState::slot_write_in_post(&p, 9).unwrap().character,
            "b"
        );
        assert!(ReducerState::slot_write_in_post(&p, 0).is_none());
    }
}
