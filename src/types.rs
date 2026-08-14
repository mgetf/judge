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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Required => "required",
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
    /// Evidence and outcomes accumulate; recusals are recorded.
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
    #[error("duplicate evidence {0}")]
    DuplicateEvidence(crate::ids::EvidenceId),
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
