//! A governance engine as an event-sourced state machine.
//!
//! Design stance:
//!   * The LOG is the history. Every attempted act is journaled, accepted or not.
//!   * The REDUCER is the constitution. Legitimacy = "the reducer accepted it."
//!     There is no side channel; if it didn't fold into state, it didn't happen.
//!   * A CASE is the unit of decision. Cases move through explicit phases,
//!     and every transition is an event with an actor attached.
//!   * Franchise is a standing bench: Discord role → seat + weight (from
//!     `config.json`), imported wholesale, snapshotted when deliberation opens.
//!     A ballot is one outcome + a reason, counted at the voter's frozen weight.
//!   * Hearing is opt-in. Default is a record (document + verdict). Questioning
//!     the subject is a special case, not the shape of every case.
//!   * AI clerks are first-class but non-sovereign: they file notes, they never
//!     vote, and their acts are permanently marked.
//!   * Everything that exists has a stable link path.

pub mod app;
pub mod clock;
pub mod config;
pub mod discord;
pub mod event_log;
pub mod events;
pub mod html;
pub mod http;
pub mod ids;
pub mod links;
pub mod mock_discord;
pub mod reducer;
pub mod scoring;
pub mod session;
pub mod state;
pub mod testing;
pub mod types;

pub use app::{serve_judge, serve_on, AppState, JudgeOptions, JudgeServer};
pub use config::{ConfigSeat, CourtConfig, RoleBinding};
pub use events::{Event, RosterMember};
pub use ids::{CaseId, EvidenceId, InvalidId, NoteId, OutcomeId, PolicyId, PrincipalId, Weight};
pub use links::{path, Cite};
pub use mock_discord::{serve_mock_discord, MockDiscord, MockDiscordConfig, MockUser};
pub use reducer::GovState;
pub use scoring::{margin_of, tally, Tally};
pub use state::{
    Attempt, Ballot, BenchSeat, BenchSnapshot, Case, ClerkNote, Evidence, Fold, Outcome, Policy,
    PolicyVersion, Principal, Statement, Verdict,
};
pub use types::{rules_for, ClerkNoteKind, DecisionKind, Hearing, Phase, Reject, Rules, Seat, Ts};
