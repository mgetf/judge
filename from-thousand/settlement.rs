//! Centralized settlement bottleneck (single consumer).
//!
//! **All** payment recording flows through one async task. HTTP handlers enqueue work and await a
//! reply; **no** `200 OK` is sent until the event is **durable** on disk.
//!
//! ## Group commit
//!
//! The worker batches queued commands: one [`EventLog::append_batch`] call (single `write` + one
//! `fsync`), then applies all events to the reducer in order and only then sends success to each
//! client. If the process dies before `fsync` completes, clients have not received a success
//! response and can retry; after `fsync`, replay from JSONL restores state.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, RwLock};

use crate::event_log::{EventLog, EventLogError};
use crate::events::Event;
use crate::journal::append_and_apply;
use crate::reducer::ReducerState;

/// Bounded queue depth for backpressure when settlement falls behind.
pub const SETTLEMENT_QUEUE_CAPACITY: usize = 1024;

/// Max events per durable commit (one `fsync` per batch).
pub const SETTLEMENT_MAX_BATCH: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum SettlementError {
    #[error(transparent)]
    EventLog(#[from] EventLogError),
    /// Same underlying failure broadcast to every command in a batch (errors are not always `Clone`).
    #[error("durable commit failed: {0}")]
    DurableCommitFailed(String),
}

/// One unit of work for the settlement worker: durable event + HTTP reply channel.
pub struct SettlementCommand {
    pub event: Event,
    pub reply: oneshot::Sender<Result<(), SettlementError>>,
}

pub type SettlementTx = mpsc::Sender<SettlementCommand>;

/// Spawns the **only** task that may append settlement events and mutate reducer state from them.
pub fn spawn_settlement_worker(
    event_log: Arc<EventLog>,
    reduced: Arc<RwLock<ReducerState>>,
) -> SettlementTx {
    let (tx, mut rx) = mpsc::channel(SETTLEMENT_QUEUE_CAPACITY);
    tokio::spawn(async move {
        while let Some(first) = rx.recv().await {
            let mut batch = vec![first];
            while batch.len() < SETTLEMENT_MAX_BATCH {
                match rx.try_recv() {
                    Ok(cmd) => batch.push(cmd),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                }
            }

            let events: Vec<Event> = batch
                .iter()
                .map(|c: &SettlementCommand| c.event.clone())
                .collect();
            let replies: Vec<_> = batch.into_iter().map(|c| c.reply).collect();

            match append_and_apply(&event_log, &reduced, events, |r, e| {
                r.apply_event(e)
                    .expect("post events are validated before durable commit")
            })
            .await
            {
                Ok(()) => {
                    for reply in replies {
                        let _ = reply.send(Ok(()));
                    }
                }
                Err(e) => {
                    tracing::error!(?e, "event log append_and_apply");
                    let msg = e.to_string();
                    for reply in replies {
                        let _ = reply.send(Err(SettlementError::DurableCommitFailed(msg.clone())));
                    }
                }
            }
        }
    });
    tx
}
