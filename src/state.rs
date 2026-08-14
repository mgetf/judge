use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{CaseId, EvidenceId, NoteId, OutcomeId, PolicyId, PrincipalId, Weight};
use crate::links::Cite;
use crate::types::{ClerkNoteKind, DecisionKind, Hearing, Phase, Seat, Ts};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub id: PrincipalId,
    pub display_name: String,
    pub seat: Option<Seat>,
    pub weight: Weight,
    pub discord_role_ids: Vec<String>,
    pub seen_ts: Ts,
}

impl Principal {
    pub fn is_clerk(&self) -> bool {
        self.seat.as_ref().is_some_and(Seat::is_clerk)
    }

    pub fn is_voting_seat(&self) -> bool {
        self.seat.as_ref().is_some_and(Seat::may_vote) && self.weight > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub ts: Ts,
    pub filed_by: PrincipalId,
    pub label: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub ts: Ts,
    pub by: PrincipalId,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub id: OutcomeId,
    pub proposed_by: PrincipalId,
    pub body: String,
    pub enacts_policy: Option<PolicyId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ballot {
    pub ts: Ts,
    pub voter: PrincipalId,
    pub outcome: OutcomeId,
    pub reason: String,
    /// Copied from the frozen bench at cast time.
    pub weight: Weight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClerkNote {
    pub id: NoteId,
    pub ts: Ts,
    pub clerk: PrincipalId,
    pub kind: ClerkNoteKind,
    pub body: String,
    pub cites: Vec<Cite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchSeat {
    pub principal: PrincipalId,
    pub seat: Seat,
    pub weight: Weight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchSnapshot {
    pub ts: Ts,
    pub seats: Vec<BenchSeat>,
}

impl BenchSnapshot {
    pub fn sitting_weight(&self) -> u64 {
        self.seats.iter().map(|s| s.weight as u64).sum()
    }

    pub fn seat(&self, id: &PrincipalId) -> Option<&BenchSeat> {
        self.seats.iter().find(|s| &s.principal == id)
    }
}

/// Frozen at close. The verdict is a snapshot, never a live tally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub ts: Ts,
    pub closed_by: PrincipalId,
    pub winner: OutcomeId,
    pub ordering: Vec<(OutcomeId, u64)>,
    pub cast_weight: u64,
    pub sitting_weight: u64,
    pub distinct_voters: usize,
    pub margin: f64,
    pub hearing: Hearing,
    pub bench: BenchSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub id: CaseId,
    pub kind: DecisionKind,
    pub hearing: Hearing,
    pub phase: Phase,
    pub opened_by: PrincipalId,
    pub opened_ts: Ts,
    pub brief: String,
    pub subject: Option<PrincipalId>,
    pub target_case: Option<CaseId>,
    pub recused: BTreeSet<PrincipalId>,
    pub bench: Option<BenchSnapshot>,
    pub evidence: BTreeMap<EvidenceId, Evidence>,
    pub notified_ts: Option<Ts>,
    pub response: Option<Statement>,
    pub outcomes: BTreeMap<OutcomeId, Outcome>,
    pub ballots: BTreeMap<PrincipalId, Ballot>,
    pub clerk_notes: BTreeMap<NoteId, ClerkNote>,
    pub cited_policies: BTreeSet<PolicyId>,
    pub verdict: Option<Verdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub ts: Ts,
    pub body: String,
    pub enacted_by_case: CaseId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub id: PolicyId,
    pub versions: Vec<PolicyVersion>,
    pub cited_in: BTreeSet<CaseId>,
    pub repealed: bool,
}

/// Every attempted act, in submit order. Rejections are part of the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fold {
    Accepted,
    Rejected(crate::types::Reject),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub seq: u64,
    pub event: crate::events::Event,
    pub fold: Fold,
}
