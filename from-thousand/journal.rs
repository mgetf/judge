//! Shared event-log mechanics: replay and durable append + apply.

use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;

use crate::jsonl_log::{JsonlLog, JsonlLogError};

/// Replay persisted events into an in-memory reducer (projection only; log stays canonical).
pub fn replay<E, R, F>(events: impl IntoIterator<Item = E>, reducer: &mut R, mut apply: F)
where
    F: FnMut(&mut R, E),
{
    for event in events {
        apply(reducer, event);
    }
}

/// One durable commit: `append_batch` then apply each event to the reducer in order.
pub async fn append_and_apply<E, R, F>(
    log: &JsonlLog<E>,
    reducer: &Arc<RwLock<R>>,
    events: Vec<E>,
    mut apply: F,
) -> Result<(), JsonlLogError>
where
    E: Serialize + DeserializeOwned + Clone,
    F: FnMut(&mut R, E),
{
    if events.is_empty() {
        return Ok(());
    }
    log.append_batch(&events).await?;
    let mut w = reducer.write().await;
    for event in events {
        apply(&mut *w, event);
    }
    Ok(())
}
