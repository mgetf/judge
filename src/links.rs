use serde::{Deserialize, Serialize};

use crate::ids::{CaseId, EvidenceId, NoteId, PolicyId, PrincipalId};

/// A typed pointer at something the docket can address. Clerk notes and
/// future UI cite these instead of free-text paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Cite {
    Case { id: CaseId },
    Evidence { case: CaseId, id: EvidenceId },
    Policy { id: PolicyId },
    Principal { id: PrincipalId },
    Note { case: CaseId, id: NoteId },
    Log { seq: u64 },
}

/// Stable path. Axum can mount these 1:1 later.
pub fn path(cite: &Cite) -> String {
    match cite {
        Cite::Case { id } => format!("/cases/{id}"),
        Cite::Evidence { case, id } => format!("/cases/{case}#evidence-{id}"),
        Cite::Policy { id } => format!("/policies/{id}"),
        Cite::Principal { id } => format!("/people/{id}"),
        Cite::Note { case, id } => format!("/cases/{case}#note-{id}"),
        Cite::Log { seq } => format!("/log/{seq}"),
    }
}

impl Cite {
    pub fn path(&self) -> String {
        path(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::*;

    fn cid(s: &str) -> CaseId {
        CaseId::parse(s).unwrap()
    }

    #[test]
    fn every_entity_has_a_stable_path() {
        assert_eq!(
            path(&Cite::Case {
                id: cid("case-0007")
            }),
            "/cases/case-0007"
        );
        assert_eq!(
            path(&Cite::Evidence {
                case: cid("case-0007"),
                id: EvidenceId::parse("demo-stv").unwrap(),
            }),
            "/cases/case-0007#evidence-demo-stv"
        );
        assert_eq!(
            path(&Cite::Policy {
                id: PolicyId::parse("moderation/slur-penalty").unwrap(),
            }),
            "/policies/moderation/slur-penalty"
        );
        assert_eq!(
            path(&Cite::Principal {
                id: PrincipalId::parse("1395").unwrap(),
            }),
            "/people/1395"
        );
        assert_eq!(
            path(&Cite::Note {
                case: cid("case-0007"),
                id: NoteId::parse("n1").unwrap(),
            }),
            "/cases/case-0007#note-n1"
        );
        assert_eq!(path(&Cite::Log { seq: 42 }), "/log/42");
    }
}
