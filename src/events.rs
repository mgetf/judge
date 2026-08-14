use serde::{Deserialize, Serialize};

use crate::ids::{CaseId, EvidenceId, NoteId, OutcomeId, PolicyId, PrincipalId, Weight};
use crate::links::Cite;
use crate::types::{ClerkNoteKind, DecisionKind, Hearing, Seat, Ts};

/// One row of a wholesale Discord-role import, after config resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterMember {
    pub id: PrincipalId,
    pub display_name: String,
    pub seat: Seat,
    pub weight: Weight,
    #[serde(default)]
    pub discord_role_ids: Vec<String>,
}

/// The only way anything happens. Adapters (HTTP, Discord sync) emit these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PrincipalSeen {
        ts: Ts,
        id: PrincipalId,
        display_name: String,
    },
    /// Replaces every seat. Principals not listed keep their identity and lose
    /// the bench. Adapter applies `config.json` to guild roles, then emits this.
    RosterSynced { ts: Ts, members: Vec<RosterMember> },

    CaseOpened {
        ts: Ts,
        id: CaseId,
        kind: DecisionKind,
        #[serde(default)]
        hearing: Hearing,
        opened_by: PrincipalId,
        brief: String,
        subject: Option<PrincipalId>,
        target_case: Option<CaseId>,
    },
    Recused {
        ts: Ts,
        case: CaseId,
        who: PrincipalId,
        by: PrincipalId,
    },
    RecusalLifted {
        ts: Ts,
        case: CaseId,
        who: PrincipalId,
        by: PrincipalId,
    },

    EvidenceFiled {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
        id: EvidenceId,
        label: String,
        body: String,
        /// Public URL of an uploaded exhibit (R2 or `/blobs/…`).
        #[serde(default)]
        href: Option<String>,
        #[serde(default)]
        filename: Option<String>,
    },
    SubjectNotified {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
    },
    ResponseFiled {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
        body: String,
    },
    OutcomeProposed {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
        id: OutcomeId,
        body: String,
        enacts_policy: Option<PolicyId>,
    },
    PolicyCited {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
        policy: PolicyId,
    },
    DeliberationOpened {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
    },
    /// Last ballot from a voter replaces the previous one.
    VoteCast {
        ts: Ts,
        case: CaseId,
        voter: PrincipalId,
        outcome: OutcomeId,
        reason: String,
    },
    ClerkNoteFiled {
        ts: Ts,
        case: CaseId,
        clerk: PrincipalId,
        id: NoteId,
        kind: ClerkNoteKind,
        body: String,
        #[serde(default)]
        cites: Vec<Cite>,
    },
    CaseClosed {
        ts: Ts,
        case: CaseId,
        by: PrincipalId,
    },
}
