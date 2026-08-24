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
    /// Discord user id → court position. Beats a weaker role mapping.
    #[serde(default)]
    pub principals: BTreeMap<String, RoleBinding>,
    /// Existing #docket channel. Created on boot when omitted.
    #[serde(default)]
    pub docket_channel_id: Option<String>,
    /// Category for open tickets (Ticket Tool: Category Created/Opened).
    #[serde(default)]
    pub court_category_id: Option<String>,
    /// Category tickets move to after close (Ticket Tool: Category Closed).
    #[serde(default)]
    pub closed_category_id: Option<String>,
    /// Channel that receives HTML transcripts on close.
    #[serde(default)]
    pub transcript_channel_id: Option<String>,
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

    /// Role mapping plus an optional per-person override. The stronger seat
    /// wins, so a named chief stays chief even if they also hold Admin.
    pub fn resolve_member(
        &self,
        principal_id: impl AsRef<str>,
        discord_role_ids: &[impl AsRef<str>],
    ) -> Option<(Seat, Weight)> {
        let from_roles = self.resolve(discord_role_ids);
        let from_person = self
            .principals
            .get(principal_id.as_ref())
            .map(|b| (Seat::from(b.seat), b.weight));
        match (from_roles, from_person) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => Some(pick_higher(a, b)),
        }
    }

    pub fn franchise_role_ids(&self) -> Vec<String> {
        self.roles.keys().cloned().collect()
    }

    pub fn franchise_principal_ids(&self) -> Vec<String> {
        self.principals.keys().cloned().collect()
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

    #[test]
    fn named_chief_beats_admin_role() {
        let cfg = CourtConfig::from_json(
            r#"{
              "guild_id": "g",
              "owner_discord_id": "owner",
              "roles": {
                "admin": { "seat": "justice", "weight": 2 },
                "mod":   { "seat": "justice", "weight": 1 }
              },
              "principals": {
                "tommy": { "seat": "chief", "weight": 3 }
              }
            }"#,
        )
        .unwrap();
        let (seat, w) = cfg.resolve_member("tommy", &["admin"]).unwrap();
        assert_eq!(seat, Seat::Chief);
        assert_eq!(w, 3);
        let (seat, w) = cfg.resolve_member("mod-only", &["mod"]).unwrap();
        assert_eq!(seat, Seat::Justice);
        assert_eq!(w, 1);
        assert_eq!(cfg.resolve_member("rando", &["unmapped"]), None);
    }
}
