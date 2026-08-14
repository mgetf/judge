//! Durable view telemetry (`views.jsonl`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One recorded `GET` (or other counted method) on a path.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ViewRecorded {
    pub ts: i64,
    pub method: String,
    pub path: String,
    /// `html` or `json` (from `Accept` on GET).
    pub repr: String,
}

impl ViewRecorded {
    pub fn key(&self) -> String {
        view_key(&self.method, &self.path, &self.repr)
    }
}

/// Stable counter key: `GET|/transactionlog|json`.
pub fn view_key(method: &str, path: &str, repr: &str) -> String {
    format!("{method}|{path}|{repr}")
}
