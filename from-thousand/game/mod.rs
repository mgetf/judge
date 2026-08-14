//! Deterministic plutocratic territorial war (MVP core).
//!
//! Self-contained event log + pure reducer. Not wired into the HTTP billboard yet.
//!
//! # Constitution (executable summary)
//!
//! - Configurable `width`×`height` grid; two sides only (`Black` / `White`); configurable homes.
//! - Events: [`GameCreated`], [`ForceCommitted`], [`EpochClosed`] — serde-tagged JSON.
//! - Commits apply to **pending** force only. [`EpochClosed`] atomically folds pending into
//!   persistent force, recomputes unique-max control (ties ⇒ neutral), freezes four-neighbor
//!   supply, and resolves majority / max-epoch victory.
//! - Initial supply = home ∪ immediate four-neighbors (same formula as later epochs with an
//!   empty controlled set — home always anchors).
//! - Force is `u64` play money, strictly additive, overflow-checked (never saturating).
//! - `payer` is attribution only; `commit_id` provides idempotency (duplicates rejected).
//! - Applied history is retained for JSONL serialize / replay.
//!
//! Offline simulation and OpenRouter adapters live in [`sim`] and [`openrouter`].
//! Run experiments with the `war_sim` binary (`--scripted` / `--openrouter`).

pub mod capital;
pub mod error;
pub mod events;
pub mod jsonl;
pub mod openrouter;
pub mod reducer;
pub mod settlement;
pub mod sim;
pub mod state;
pub mod types;
pub mod vault_types;

#[cfg(test)]
mod scenario;

pub use error::ApplyError;
pub use events::{EpochClosed, ForceCommitted, GameCreated, GameEvent};
pub use jsonl::{decode_jsonl, encode_jsonl, replay_jsonl, round_trip_replay};
pub use openrouter::{
    decide_openrouter, encode_model_traces, parse_decision_content, CappedChatClient, ChatClient,
    ChatCompletionRequest, ChatCompletionResponse, ModelCallTrace, ModelTraceLine,
    OpenRouterConfig, OpenRouterError, OpenRouterPolicy, ReqwestChatClient,
};
pub use reducer::{apply_event, compute_supply, fold_events, replay};
pub use sim::{
    apply_epoch_with_side_order, commit_id, frontier_cells, resolve_decision, run_simulation,
    validate_decision, Allocation, CellPublic, ConcentratedFrontierPolicy, Decision,
    DecisionRejection, PassPolicy, Policy, PublicView, ScriptedPolicy, SideDecisionRecord,
    SimConfig, SimResult, SimTranscript, StoppedReason, UniformFrontierPolicy,
};
pub use state::{resolve_outcome, BoardSnapshot, CellSnapshot, GameState};
pub use types::{CellForce, CellId, Control, Outcome, Phase, Side};
