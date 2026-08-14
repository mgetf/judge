//! Best-effort view log writer (batched `views.jsonl` + in-memory projection).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};

use crate::journal::append_and_apply;
use crate::jsonl_log::JsonlLog;
use crate::view_events::ViewRecorded;
use crate::view_reducer::ViewReducerState;

pub const VIEW_QUEUE_CAPACITY: usize = 4096;
pub const VIEW_MAX_BATCH: usize = 256;
pub const VIEW_FLUSH_INTERVAL_MS: u64 = 500;

pub type ViewTx = mpsc::Sender<ViewRecorded>;

/// Spawns the sole task that appends view events. HTTP handlers only enqueue.
pub fn spawn_view_worker(
    log: Arc<JsonlLog<ViewRecorded>>,
    reducer: Arc<RwLock<ViewReducerState>>,
) -> ViewTx {
    let (tx, mut rx) = mpsc::channel(VIEW_QUEUE_CAPACITY);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(VIEW_FLUSH_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut batch: Vec<ViewRecorded> = Vec::new();

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(ev) => {
                            batch.push(ev);
                            if batch.len() >= VIEW_MAX_BATCH {
                                flush_view_batch(&log, &reducer, &mut batch).await;
                            }
                        }
                        None => {
                            if !batch.is_empty() {
                                flush_view_batch(&log, &reducer, &mut batch).await;
                            }
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        flush_view_batch(&log, &reducer, &mut batch).await;
                    }
                }
            }
        }
    });
    tx
}

async fn flush_view_batch(
    log: &JsonlLog<ViewRecorded>,
    reducer: &Arc<RwLock<ViewReducerState>>,
    batch: &mut Vec<ViewRecorded>,
) {
    if batch.is_empty() {
        return;
    }
    let events = std::mem::take(batch);
    if let Err(e) = append_and_apply(log, reducer, events, |r, ev| r.apply(ev)).await {
        tracing::error!(?e, "views.jsonl append_and_apply failed");
    }
}
