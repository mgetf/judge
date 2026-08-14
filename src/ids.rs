use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Ballot weight. Set on the Discord role in `config.json`, copied onto the
/// frozen bench, then onto each ballot. Never taken from the live roster at
/// close time.
pub type Weight = u32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} id {value:?}: {reason}")]
pub struct InvalidId {
    pub kind: &'static str,
    pub value: String,
    pub reason: &'static str,
}

/// URL-safe identifier: non-empty, ≤128 chars, `A-Za-z0-9._~/-`, no leading
/// or trailing `/`. Policy ids may contain `/` (`moderation/slur-penalty`).
pub fn is_link_id(s: &str) -> Result<(), &'static str> {
    if s.is_empty() {
        return Err("empty");
    }
    if s.len() > 128 {
        return Err("longer than 128 bytes");
    }
    if s.starts_with('/') || s.ends_with('/') {
        return Err("leading or trailing '/'");
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '~'))
    {
        return Err("contains a character outside A-Za-z0-9._~/-");
    }
    Ok(())
}

macro_rules! link_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(s: impl AsRef<str>) -> Result<Self, InvalidId> {
                let value = s.as_ref().to_string();
                match is_link_id(&value) {
                    Ok(()) => Ok(Self(value)),
                    Err(reason) => Err(InvalidId {
                        kind: $kind,
                        value,
                        reason,
                    }),
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }
    };
}

link_id!(PrincipalId, "principal");
link_id!(CaseId, "case");
link_id!(OutcomeId, "outcome");
link_id!(PolicyId, "policy");
link_id!(EvidenceId, "evidence");
link_id!(NoteId, "note");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs_snowflakes_and_policy_paths() {
        assert!(PrincipalId::parse("139512345678901234").is_ok());
        assert!(CaseId::parse("case-0007").is_ok());
        assert!(PolicyId::parse("moderation/slur-penalty").is_ok());
        assert!(EvidenceId::parse("audit-tm-move").is_ok());
    }

    #[test]
    fn rejects_empty_spaces_and_edge_slashes() {
        assert!(CaseId::parse("").is_err());
        assert!(CaseId::parse("has space").is_err());
        assert!(CaseId::parse("/leading").is_err());
        assert!(CaseId::parse("trailing/").is_err());
        assert!(CaseId::parse("bad?").is_err());
    }
}
