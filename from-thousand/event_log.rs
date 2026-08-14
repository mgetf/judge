//! Board event log (`events.jsonl`).

pub use crate::jsonl_log::{JsonlLog, JsonlLogError};

use crate::events::Event;

/// Append-only log of [`Event`] (board posts).
pub type EventLog = JsonlLog<Event>;

pub type EventLogError = JsonlLogError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, PostSettled, SlotWrite};

    #[tokio::test]
    async fn many_concurrent_appends_single_fd_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.jsonl");
        let log = std::sync::Arc::new(EventLog::new(&path));
        let n = 200usize;
        let mut tasks = Vec::new();
        for i in 0..n {
            let log = log.clone();
            let ev = Event::PostSettled(PostSettled {
                ts: i as i64,
                post_id: format!("post-{i}"),
                writes: vec![SlotWrite {
                    slot_index: (i % 1000) as u16,
                    character: "c".into(),
                    price_usdc_micro: i as u64,
                }],
                total_usdc_micro: i as u64,
                payer: None,
            });
            tasks.push(tokio::spawn(async move { log.append(&ev).await.unwrap() }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        let (events, bad) = log.load_all().await.unwrap();
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(events.len(), n);
    }

    #[tokio::test]
    async fn append_batch_writes_multiple_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("batch.jsonl");
        let log = EventLog::new(&path);
        let e1 = Event::PostSettled(PostSettled {
            ts: 1,
            post_id: "a".into(),
            writes: vec![SlotWrite {
                slot_index: 0,
                character: "a".into(),
                price_usdc_micro: 1,
            }],
            total_usdc_micro: 1,
            payer: None,
        });
        let e2 = Event::PostSettled(PostSettled {
            ts: 2,
            post_id: "b".into(),
            writes: vec![SlotWrite {
                slot_index: 1,
                character: "b".into(),
                price_usdc_micro: 2,
            }],
            total_usdc_micro: 2,
            payer: None,
        });
        log.append_batch(&[e1, e2]).await.unwrap();
        let (events, bad) = log.load_all().await.unwrap();
        assert!(bad.is_empty());
        assert_eq!(events.len(), 2);
    }
}
