use std::fmt;
use std::str::FromStr;

use crate::ids::{CaseId, OutcomeId, PrincipalId};

/// Milliseconds since epoch. Injected in tests; production adapters use the
/// server clock.
pub type Ts = i64;

/// Standing position on the court. Mapped from a Discord role id in config.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Seat {
    Chief,
    Justice,
    /// May file notes. Never votes, opens, or closes.
    Clerk {
        #[serde(default)]
        model: Option<String>,
    },
}

impl Seat {
    pub fn is_clerk(&self) -> bool {
        matches!(self, Seat::Clerk { .. })
    }

    pub fn may_vote(&self) -> bool {
        matches!(self, Seat::Chief | Seat::Justice)
    }

    /// Tie-break when two mapped Discord roles share a weight.
    pub fn rank(&self) -> u8 {
        match self {
            Seat::Chief => 2,
            Seat::Justice => 1,
            Seat::Clerk { .. } => 0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Seat::Chief => "chief",
            Seat::Justice => "justice",
            Seat::Clerk { .. } => "clerk",
        }
    }
}

/// Whether the subject is called in. Default is a record, not a trial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hearing {
    /// Document and record a verdict. No notice, no response slot.
    #[default]
    None,
    /// Subject must be notified and given a chance to respond (or the window
    /// lapses) before deliberation.
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// Document a fact and freeze a verdict (cheats, smurfs, obvious bans).
    Record,
    /// Day-to-day mod action. Fast, modest margin.
    Routine,
    /// Hire / fire / demote. Hearing is still opt-in on the case.
    Personnel,
    /// Enact or amend a standing policy (becomes citable precedent).
    Policy,
    /// Change the rules of the rules (quorums, kinds).
    Constitutional,
    /// A case about a closed case. Verdict `vacate` nullifies the target.
    Appeal,
}

impl DecisionKind {
    pub fn all() -> [Self; 6] {
        [
            Self::Record,
            Self::Routine,
            Self::Personnel,
            Self::Policy,
            Self::Constitutional,
            Self::Appeal,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Routine => "routine",
            Self::Personnel => "personnel",
            Self::Policy => "policy",
            Self::Constitutional => "constitutional",
            Self::Appeal => "appeal",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Record => "Record — fact / obvious ban",
            Self::Routine => "Routine — day-to-day mod action",
            Self::Personnel => "Personnel — hire, fire, demote",
            Self::Policy => "Policy — standing rule / precedent",
            Self::Constitutional => "Constitutional — change the rules",
            Self::Appeal => "Appeal — reopen a closed case",
        }
    }

    pub fn menu_hint(self) -> &'static str {
        match self {
            Self::Record => "Cheats, smurfs, bans already obvious on the record",
            Self::Routine => "Fast mod work. Modest vote margin.",
            Self::Personnel => "Hearing is still opt-in on the case",
            Self::Policy => "Becomes citable after the verdict",
            Self::Constitutional => "Quorums, kinds, the rules of the rules",
            Self::Appeal => "Vacate a closed case. Set the target in /case.",
        }
    }
}

impl fmt::Display for DecisionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DecisionKind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "record" => Ok(Self::Record),
            "routine" => Ok(Self::Routine),
            "personnel" => Ok(Self::Personnel),
            "policy" => Ok(Self::Policy),
            "constitutional" => Ok(Self::Constitutional),
            "appeal" => Ok(Self::Appeal),
            _ => Err(()),
        }
    }
}

impl Hearing {
    pub fn all() -> [Self; 2] {
        [Self::None, Self::Required]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Required => "required",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::None => "No hearing — document and decide",
            Self::Required => "Hearing required — notify the subject",
        }
    }

    pub fn menu_hint(self) -> &'static str {
        match self {
            Self::None => "Default. No notice, no response slot.",
            Self::Required => "Subject must be told and given a chance to answer",
        }
    }
}

impl fmt::Display for Hearing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Hearing {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Ok(Self::None),
            "required" => Ok(Self::Required),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    /// Minimum fraction of *sitting bench weight* that must cast a ballot.
    pub quorum_weight_frac: f64,
    /// Winner's weight must exceed runner-up by this factor, else lapse.
    /// Exact ties always lapse, regardless of this number.
    pub min_margin: f64,
    /// Used only when [`Hearing::Required`]. If the kind has `None`, a 48h
    /// fallback applies.
    pub response_window_ms: Option<i64>,
}

pub const fn rules_for(kind: DecisionKind) -> Rules {
    match kind {
        DecisionKind::Record => Rules {
            quorum_weight_frac: 0.50,
            min_margin: 1.0,
            response_window_ms: None,
        },
        DecisionKind::Routine => Rules {
            quorum_weight_frac: 0.50,
            min_margin: 1.0,
            response_window_ms: None,
        },
        DecisionKind::Personnel => Rules {
            quorum_weight_frac: 0.50,
            min_margin: 1.15,
            response_window_ms: Some(48 * 3_600_000),
        },
        DecisionKind::Policy => Rules {
            quorum_weight_frac: 0.50,
            min_margin: 1.10,
            response_window_ms: None,
        },
        DecisionKind::Constitutional => Rules {
            quorum_weight_frac: 2.0 / 3.0,
            min_margin: 1.50,
            response_window_ms: None,
        },
        DecisionKind::Appeal => Rules {
            quorum_weight_frac: 0.50,
            min_margin: 1.25,
            response_window_ms: None,
        },
    }
}

/// How long deliberation stays open before a case may close without quorum
/// (and lapse). Under constitution 2, closing earlier needs quorum.
pub const DELIBERATION_WINDOW_MS: i64 = 72 * 3_600_000;

/// Window the subject has to respond when a hearing is required.
pub fn response_window_ms(kind: DecisionKind, hearing: Hearing) -> Option<i64> {
    if hearing != Hearing::Required {
        return None;
    }
    Some(rules_for(kind).response_window_ms.unwrap_or(48 * 3_600_000))
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Outcomes accumulate; recusals are recorded. Chat is the record.
    Intake,
    /// Subject has been formally notified ([`Hearing::Required`] only).
    Noticed,
    /// Voting is open. Bench is frozen.
    Deliberation,
    /// Closed with a verdict snapshot. Immutable except by Appeal.
    Closed,
    /// Closed without verdict (quorum, tie, or margin failed).
    Lapsed,
    /// Verdict nullified by a successful appeal. History remains.
    Vacated,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Noticed => "noticed",
            Self::Deliberation => "deliberation",
            Self::Closed => "closed",
            Self::Lapsed => "lapsed",
            Self::Vacated => "vacated",
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClerkNoteKind {
    Summary,
    Contradiction,
    PrecedentLink,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Reject {
    #[error("unknown principal {0}")]
    UnknownPrincipal(PrincipalId),
    #[error("unknown case {0}")]
    UnknownCase(CaseId),
    #[error("duplicate case {0}")]
    DuplicateCase(CaseId),
    #[error("case {case} is in {have:?}, need {need}")]
    WrongPhase {
        case: CaseId,
        have: Phase,
        need: &'static str,
    },
    #[error("{0} is not seated on the court")]
    NotSeated(PrincipalId),
    #[error("{0} is not on this case's bench")]
    NotOnBench(PrincipalId),
    #[error("{0} is recused from this case")]
    Recused(PrincipalId),
    #[error("subject {0} cannot sit or vote on their own case")]
    SubjectCannotAct(PrincipalId),
    #[error("clerk {0} cannot act in a sovereign capacity")]
    ClerkCannotSovereign(PrincipalId),
    #[error("human {0} cannot file a clerk note")]
    HumanCannotClerk(PrincipalId),
    #[error("hearing is not required on this case")]
    HearingNotRequired,
    #[error("hearing required but no subject named")]
    MissingSubject,
    #[error("appeal is missing a target case")]
    MissingTarget,
    #[error("appeal target {0} is not closed")]
    AppealTargetNotClosed(CaseId),
    #[error("subject unheard; window ends at {window_ends}")]
    SubjectUnheard { window_ends: Ts },
    #[error("unknown outcome {0}")]
    UnknownOutcome(OutcomeId),
    #[error("duplicate outcome {0}")]
    DuplicateOutcome(OutcomeId),
    #[error("duplicate note {0}")]
    DuplicateNote(crate::ids::NoteId),
    #[error("empty reason")]
    EmptyReason,
    #[error("empty brief")]
    EmptyBrief,
    #[error("empty body")]
    EmptyBody,
    #[error("no voting bench to snapshot")]
    EmptyBench,
    #[error("deliberation needs quorum before it can close; otherwise wait until {opens_at}")]
    TooEarlyToClose { opens_at: Ts },
    #[error("unknown policy {0}")]
    UnknownPolicy(crate::ids::PolicyId),
    /// A rejection read back from the log; the reason is as written then.
    #[error("{0}")]
    Recorded(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hearing_none_has_no_window() {
        assert_eq!(
            response_window_ms(DecisionKind::Personnel, Hearing::None),
            None
        );
    }

    #[test]
    fn hearing_required_falls_back_to_48h_when_kind_has_none() {
        assert_eq!(
            response_window_ms(DecisionKind::Record, Hearing::Required),
            Some(48 * 3_600_000)
        );
    }
}
