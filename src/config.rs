use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ids::Weight;
use crate::types::Seat;

/// Discord role id → court position. Loaded from `config.json`. Not an event;
/// the adapter resolves roles, then emits [`crate::events::Event::RosterSynced`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourtConfig {
    pub guild_id: String,
    pub owner_discord_id: String,
    pub roles: BTreeMap<String, RoleBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleBinding {
    pub seat: ConfigSeat,
    pub weight: Weight,
}

/// Flat seat tag in config (clerks do not name a model here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSeat {
    Chief,
    Justice,
    Clerk,
}

impl From<ConfigSeat> for Seat {
    fn from(s: ConfigSeat) -> Self {
        match s {
            ConfigSeat::Chief => Seat::Chief,
            ConfigSeat::Justice => Seat::Justice,
            ConfigSeat::Clerk => Seat::Clerk { model: None },
        }
    }
}

impl CourtConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_json(&raw)
    }

    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Highest weight among mapped roles the member holds. Ties go to the
    /// higher seat rank (chief > justice > clerk).
    pub fn resolve(&self, discord_role_ids: &[impl AsRef<str>]) -> Option<(Seat, Weight)> {
        let mut best: Option<(Seat, Weight)> = None;
        for raw in discord_role_ids {
            let Some(binding) = self.roles.get(raw.as_ref()) else {
                continue;
            };
            let seat = Seat::from(binding.seat);
            let cand = (seat, binding.weight);
            best = Some(match best {
                None => cand,
                Some(cur) => pick_higher(cur, cand),
            });
        }
        best
    }
}

fn pick_higher(a: (Seat, Weight), b: (Seat, Weight)) -> (Seat, Weight) {
    match a.1.cmp(&b.1) {
        std::cmp::Ordering::Less => b,
        std::cmp::Ordering::Greater => a,
        std::cmp::Ordering::Equal => {
            if b.0.rank() > a.0.rank() {
                b
            } else {
                a
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CourtConfig {
        CourtConfig::from_json(
            r#"{
              "guild_id": "g",
              "owner_discord_id": "owner",
              "roles": {
                "chief":  { "seat": "chief",   "weight": 3 },
                "just":   { "seat": "justice", "weight": 1 },
                "clerk":  { "seat": "clerk",   "weight": 0 },
                "heavy":  { "seat": "justice", "weight": 3 }
              }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn unmapped_roles_yield_no_seat() {
        assert_eq!(cfg().resolve(&["random"]), None);
    }

    #[test]
    fn highest_weight_wins() {
        let (seat, w) = cfg().resolve(&["just", "chief"]).unwrap();
        assert_eq!(seat, Seat::Chief);
        assert_eq!(w, 3);
    }

    #[test]
    fn equal_weight_prefers_chief() {
        let (seat, w) = cfg().resolve(&["chief", "heavy"]).unwrap();
        assert_eq!(seat, Seat::Chief);
        assert_eq!(w, 3);
    }

    #[test]
    fn example_file_parses() {
        let parsed = CourtConfig::from_path("config.example.json").unwrap();
        assert_eq!(parsed.roles.len(), 3);
        assert_eq!(parsed.resolve(&["111111111111111111"]).unwrap().1, 3);
    }
}
