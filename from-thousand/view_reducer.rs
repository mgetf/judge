//! In-memory projection of `views.jsonl` (rebuilt on boot).

use std::collections::{BTreeMap, HashMap};

use crate::view_events::ViewRecorded;

/// Rolling window for weekly stats (7 days).
pub const VIEW_WEEK_SECS: i64 = 7 * 24 * 60 * 60;

/// UTC hour bucket: unix timestamp floored to hour.
pub fn hour_start_ts(ts_ms: i64) -> i64 {
    let ts = ts_ms / 1000;
    ts - (ts % 3600)
}

pub struct ViewReducerState {
    pub all_time: HashMap<String, u64>,
    /// Per key, per hour bucket → count (for charts and last-week per path).
    hourly_by_key: HashMap<String, BTreeMap<i64, u64>>,
    /// Sum of all keys per hour (global traffic series).
    hourly_total: BTreeMap<i64, u64>,
}

impl Default for ViewReducerState {
    fn default() -> Self {
        Self {
            all_time: HashMap::new(),
            hourly_by_key: HashMap::new(),
            hourly_total: BTreeMap::new(),
        }
    }
}

impl ViewReducerState {
    pub fn apply(&mut self, event: ViewRecorded) {
        let key = event.key();
        let hour = hour_start_ts(event.ts);

        *self.all_time.entry(key.clone()).or_insert(0) += 1;
        *self
            .hourly_by_key
            .entry(key)
            .or_default()
            .entry(hour)
            .or_insert(0) += 1;
        *self.hourly_total.entry(hour).or_insert(0) += 1;
    }

    pub fn count_all_time(&self, key: &str) -> u64 {
        self.all_time.get(key).copied().unwrap_or(0)
    }

    pub fn last_week_cutoff_ts(&self, now_ms: i64) -> i64 {
        now_ms / 1000 - VIEW_WEEK_SECS
    }

    pub fn last_week_for_key(&self, key: &str, now_ms: i64) -> u64 {
        let cutoff = self.last_week_cutoff_ts(now_ms);
        self.hourly_by_key
            .get(key)
            .map(|hours| {
                hours
                    .iter()
                    .filter(|(h, _)| **h >= cutoff)
                    .map(|(_, c)| c)
                    .sum()
            })
            .unwrap_or(0)
    }

    pub fn last_week_total(&self, now_ms: i64) -> u64 {
        let cutoff = self.last_week_cutoff_ts(now_ms);
        self.hourly_total
            .iter()
            .filter(|(h, _)| **h >= cutoff)
            .map(|(_, c)| c)
            .sum()
    }

    pub fn last_week_html_total(&self, now_ms: i64) -> u64 {
        self.sum_last_week_by_repr("html", now_ms)
    }

    pub fn last_week_json_total(&self, now_ms: i64) -> u64 {
        self.sum_last_week_by_repr("json", now_ms)
    }

    fn sum_last_week_by_repr(&self, repr: &str, now_ms: i64) -> u64 {
        let cutoff = self.last_week_cutoff_ts(now_ms);
        self.hourly_by_key
            .iter()
            .filter(|(k, _)| k.ends_with(&format!("|{repr}")))
            .flat_map(|(_, hours)| hours.iter())
            .filter(|(h, _)| **h >= cutoff)
            .map(|(_, c)| c)
            .sum()
    }

    pub fn all_time_total(&self) -> u64 {
        self.all_time.values().sum()
    }

    /// Hourly global series for charts: `(hour_start_ts, count)` sorted ascending.
    pub fn series_global(&self, now_ms: i64, days: u32) -> Vec<(i64, u64)> {
        let cutoff = now_ms / 1000 - i64::from(days) * 24 * 3600;
        self.hourly_total
            .iter()
            .filter(|(h, _)| **h >= cutoff)
            .map(|(h, c)| (*h, *c))
            .collect()
    }

    /// Per-key hourly series for charts.
    pub fn series_for_key(&self, key: &str, now_ms: i64, days: u32) -> Vec<(i64, u64)> {
        let cutoff = now_ms / 1000 - i64::from(days) * 24 * 3600;
        self.hourly_by_key
            .get(key)
            .map(|hours| {
                hours
                    .iter()
                    .filter(|(h, _)| **h >= cutoff)
                    .map(|(h, c)| (*h, *c))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_events::{view_key, ViewRecorded};

    #[test]
    fn apply_and_last_week() {
        let now = 1_700_000_000_000i64;
        let old_ts = (now / 1000 - VIEW_WEEK_SECS - 3600) * 1000;
        let recent_ts = (now / 1000 - 3600) * 1000;

        let mut st = ViewReducerState::default();
        let key = view_key("GET", "/", "html");
        st.apply(ViewRecorded {
            ts: old_ts,
            method: "GET".into(),
            path: "/".into(),
            repr: "html".into(),
        });
        st.apply(ViewRecorded {
            ts: recent_ts,
            method: "GET".into(),
            path: "/".into(),
            repr: "html".into(),
        });
        st.apply(ViewRecorded {
            ts: recent_ts,
            method: "GET".into(),
            path: "/".into(),
            repr: "html".into(),
        });

        assert_eq!(st.count_all_time(&key), 3);
        assert_eq!(st.last_week_for_key(&key, now), 2);
    }
}
