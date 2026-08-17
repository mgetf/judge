//! Acts a seated person can take. Adapters (Discord buttons, HTML, slash
//! commands) parse into this; [`Action::into_event`] is the only bridge to
//! the log.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::events::Event;
use crate::ids::{CaseId, EvidenceId, OutcomeId, PolicyId, PrincipalId};
use crate::links::Cite;
use crate::types::{DecisionKind, Hearing, Ts};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenCase {
        id: CaseId,
        kind: DecisionKind,
        hearing: Hearing,
        brief: String,
        subject: Option<PrincipalId>,
        target_case: Option<CaseId>,
    },
    FileEvidence {
        case: CaseId,
        id: EvidenceId,
        label: String,
        body: String,
    },
    ProposeOutcome {
        case: CaseId,
        id: OutcomeId,
        body: String,
        enacts_policy: Option<PolicyId>,
    },
    NotifySubject {
        case: CaseId,
    },
    FileResponse {
        case: CaseId,
        body: String,
    },
    OpenDeliberation {
        case: CaseId,
    },
    CastVote {
        case: CaseId,
        outcome: OutcomeId,
        reason: String,
    },
    CloseCase {
        case: CaseId,
    },
}

impl Action {
    pub fn into_event(self, who: PrincipalId, ts: Ts) -> Event {
        match self {
            Action::OpenCase {
                id,
                kind,
                hearing,
                brief,
                subject,
                target_case,
            } => Event::CaseOpened {
                ts,
                id,
                kind,
                hearing,
                opened_by: who,
                brief,
                subject,
                target_case,
            },
            Action::FileEvidence {
                case,
                id,
                label,
                body,
            } => Event::EvidenceFiled {
                ts,
                case,
                by: who,
                id,
                label,
                body,
            },
            Action::ProposeOutcome {
                case,
                id,
                body,
                enacts_policy,
            } => Event::OutcomeProposed {
                ts,
                case,
                by: who,
                id,
                body,
                enacts_policy,
            },
            Action::NotifySubject { case } => Event::SubjectNotified { ts, case, by: who },
            Action::FileResponse { case, body } => Event::ResponseFiled {
                ts,
                case,
                by: who,
                body,
            },
            Action::OpenDeliberation { case } => Event::DeliberationOpened { ts, case, by: who },
            Action::CastVote {
                case,
                outcome,
                reason,
            } => Event::VoteCast {
                ts,
                case,
                voter: who,
                outcome,
                reason,
            },
            Action::CloseCase { case } => Event::CaseClosed { ts, case, by: who },
        }
    }

    pub fn cite(&self) -> Cite {
        match self {
            Action::OpenCase { id, .. } => Cite::Case { id: id.clone() },
            Action::FileEvidence { case, .. }
            | Action::ProposeOutcome { case, .. }
            | Action::NotifySubject { case }
            | Action::FileResponse { case, .. }
            | Action::OpenDeliberation { case }
            | Action::CastVote { case, .. }
            | Action::CloseCase { case } => Cite::Case { id: case.clone() },
        }
    }
}

/// HTML `/eval` and Discord modal fields share these names.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvalForm {
    pub action: String,
    #[serde(default)]
    pub case: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub hearing: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub target_case: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub enacts_policy: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub reason: String,
}

impl EvalForm {
    pub fn parse(self) -> Result<Action, String> {
        let mut fields = BTreeMap::new();
        fields.insert("case".into(), self.case);
        fields.insert("id".into(), self.id);
        fields.insert("kind".into(), self.kind);
        fields.insert("hearing".into(), self.hearing);
        fields.insert("brief".into(), self.brief);
        fields.insert("subject".into(), self.subject);
        fields.insert("target_case".into(), self.target_case);
        fields.insert("label".into(), self.label);
        fields.insert("body".into(), self.body);
        fields.insert("enacts_policy".into(), self.enacts_policy);
        fields.insert("outcome".into(), self.outcome);
        fields.insert("reason".into(), self.reason);
        parse_action(&self.action, &fields)
    }
}

pub fn parse_action(verb: &str, fields: &BTreeMap<String, String>) -> Result<Action, String> {
    let get = |k: &str| {
        fields
            .get(k)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let opt = |k: &str| {
        let s = get(k);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };
    match verb {
        "open_case" | "case" => Ok(Action::OpenCase {
            id: CaseId::parse(get("id")).map_err(|e| e.to_string())?,
            kind: get("kind")
                .parse::<DecisionKind>()
                .unwrap_or(DecisionKind::Record),
            hearing: get("hearing").parse::<Hearing>().unwrap_or(Hearing::None),
            brief: get("brief"),
            subject: opt("subject")
                .map(|s| PrincipalId::parse(s).map_err(|e| e.to_string()))
                .transpose()?,
            target_case: opt("target_case")
                .map(|s| CaseId::parse(s).map_err(|e| e.to_string()))
                .transpose()?,
        }),
        "file_evidence" | "evidence" => Ok(Action::FileEvidence {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
            id: EvidenceId::parse(get("id")).map_err(|e| e.to_string())?,
            label: get("label"),
            body: get("body"),
        }),
        "propose_outcome" | "outcome" => Ok(Action::ProposeOutcome {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
            id: OutcomeId::parse(get("id")).map_err(|e| e.to_string())?,
            body: get("body"),
            enacts_policy: opt("enacts_policy")
                .map(|s| PolicyId::parse(s).map_err(|e| e.to_string()))
                .transpose()?,
        }),
        "notify" => Ok(Action::NotifySubject {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
        }),
        "respond" => Ok(Action::FileResponse {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
            body: get("body"),
        }),
        "deliberate" => Ok(Action::OpenDeliberation {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
        }),
        "vote" => Ok(Action::CastVote {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
            outcome: OutcomeId::parse(get("outcome")).map_err(|e| e.to_string())?,
            reason: get("reason"),
        }),
        "close" => Ok(Action::CloseCase {
            case: CaseId::parse(get("case")).map_err(|e| e.to_string())?,
        }),
        other => Err(format!("unknown action {other}")),
    }
}

/// Discord `custom_id`. Instant acts are `go:verb:case`. Opening a modal is
/// `ask:verb` or `ask:verb:case`. Modal submit is `do:verb` or `do:verb:case`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wire {
    Go { verb: String, case: Option<String> },
    Ask { verb: String, case: Option<String> },
    Do { verb: String, case: Option<String> },
}

impl Wire {
    pub fn parse(custom_id: &str) -> Result<Self, String> {
        let mut parts = custom_id.splitn(3, ':');
        let kind = parts.next().unwrap_or("");
        let verb = parts.next().unwrap_or("").to_string();
        let case = parts.next().map(str::to_string).filter(|s| !s.is_empty());
        if verb.is_empty() {
            return Err("empty verb".into());
        }
        match kind {
            "go" => Ok(Wire::Go { verb, case }),
            "ask" => Ok(Wire::Ask { verb, case }),
            "do" => Ok(Wire::Do { verb, case }),
            _ => Err(format!("unknown wire {custom_id}")),
        }
    }

    pub fn custom_id(kind: &str, verb: &str, case: Option<&str>) -> String {
        match case {
            Some(c) if !c.is_empty() => format!("{kind}:{verb}:{c}"),
            _ => format!("{kind}:{verb}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_open_case() {
        let a = EvalForm {
            action: "open_case".into(),
            id: "case-cheat-1".into(),
            brief: "aimbot".into(),
            kind: "record".into(),
            ..EvalForm::default()
        }
        .parse()
        .unwrap();
        assert!(matches!(a, Action::OpenCase { .. }));
    }

    #[test]
    fn wire_round_trip() {
        let id = Wire::custom_id("go", "deliberate", Some("case-cheat-1"));
        assert_eq!(id, "go:deliberate:case-cheat-1");
        assert_eq!(
            Wire::parse(&id).unwrap(),
            Wire::Go {
                verb: "deliberate".into(),
                case: Some("case-cheat-1".into()),
            }
        );
    }
}
