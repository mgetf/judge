//! Live `see` updates. Every accepted fold publishes a seq; HTML listens
//! over SSE, Discord edits the channel view message.

use tokio::sync::broadcast;

#[derive(Clone)]
pub struct Live {
    tx: broadcast::Sender<u64>,
}

impl Live {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }

    pub fn publish(&self, seq: u64) {
        let _ = self.tx.send(seq);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<u64> {
        self.tx.subscribe()
    }
}

impl Default for Live {
    fn default() -> Self {
        Self::new()
    }
}
