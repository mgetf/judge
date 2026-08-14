use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{
    event_log::EventLog,
    jsonl_log::JsonlLog,
    reducer::{ReducerState, SLOT_COUNT},
    settlement::{spawn_settlement_worker, SettlementTx},
    util::now_ms,
    view_events::ViewRecorded,
    view_reducer::ViewReducerState,
    view_worker::{spawn_view_worker, ViewTx},
    x402::{X402Config, X402Settler},
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: String,
    pub event_log_path: String,
    pub view_log_path: String,
    pub x402: X402Config,
    /// When true, `POST /post` works without x402 (local dev/tests only).
    pub allow_unpaid_posts: bool,
}

/// Application state. **`settlement_tx`** is the sole entry point for recording
/// payments: one background task drains the queue and runs append → apply in order.
#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<AppConfig>,
    pub event_log: Arc<EventLog>,
    pub reduced: Arc<RwLock<ReducerState>>,
    /// Enqueue-only handle to the global settlement worker (see [`crate::settlement`]).
    pub settlement_tx: SettlementTx,
    /// Serializes paid writes per slot without blocking unrelated slots during x402 settlement.
    pub slot_locks: Arc<Vec<tokio::sync::Mutex<()>>>,
    pub x402_settler: Option<Arc<X402Settler>>,
    pub view_log: Arc<JsonlLog<ViewRecorded>>,
    pub view_reduced: Arc<RwLock<ViewReducerState>>,
    view_tx: ViewTx,
}

impl AppState {
    pub fn new(cfg: AppConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let path = cfg.event_log_path.clone();
        let event_log = Arc::new(EventLog::new(path));
        let reduced = Arc::new(RwLock::new(ReducerState::default()));
        let settlement_tx = spawn_settlement_worker(event_log.clone(), reduced.clone());

        let view_log = Arc::new(JsonlLog::new(cfg.view_log_path.clone()));
        let view_reduced = Arc::new(RwLock::new(ViewReducerState::default()));
        let view_tx = spawn_view_worker(view_log.clone(), view_reduced.clone());

        let slot_locks = (0..SLOT_COUNT)
            .map(|_| tokio::sync::Mutex::new(()))
            .collect();

        let x402_settler = if cfg.x402.enabled {
            let key = cfg
                .x402
                .settler_private_key
                .as_deref()
                .ok_or("SLUG_X402_SETTLER_PRIVATE_KEY required")?;
            Some(Arc::new(X402Settler::new(&cfg.x402.rpc_url, key)?))
        } else {
            None
        };

        Ok(Self {
            cfg: Arc::new(cfg),
            event_log,
            reduced,
            settlement_tx,
            slot_locks: Arc::new(slot_locks),
            x402_settler,
            view_log,
            view_reduced,
            view_tx,
        })
    }

    /// Enqueue a view event (best-effort; drops if the queue is full).
    pub fn record_view(&self, method: &str, path: &str, repr: &str) {
        let _ = self.view_tx.try_send(ViewRecorded {
            ts: now_ms(),
            method: method.to_string(),
            path: path.to_string(),
            repr: repr.to_string(),
        });
    }
}
