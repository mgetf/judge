use std::collections::BTreeMap;

use crate::ids::{OutcomeId, PrincipalId};
use crate::state::{Ballot, Outcome};

/// Weighted plurality over a case's outcomes. Unvoted outcomes stay in the
/// ordering at weight 0 so the snapshot names every proposed option.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    /// Best-first, then outcome id for stability.
    pub ordering: Vec<(OutcomeId, u64)>,
    pub cast_weight: u64,
    pub distinct_voters: usize,
}

pub fn tally(
    outcomes: &BTreeMap<OutcomeId, Outcome>,
    ballots: &BTreeMap<PrincipalId, Ballot>,
) -> Tally {
    let mut weights: BTreeMap<OutcomeId, u64> = outcomes.keys().cloned().map(|k| (k, 0)).collect();
    let mut cast_weight = 0u64;
    for b in ballots.values() {
        cast_weight += b.weight as u64;
        *weights.entry(b.outcome.clone()).or_insert(0) += b.weight as u64;
    }
    let mut ordering: Vec<(OutcomeId, u64)> = weights.into_iter().collect();
    ordering.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Tally {
        ordering,
        cast_weight,
        distinct_voters: ballots.len(),
    }
}

/// `margin` is winner / runner-up. `tied` is true when the top two weights
/// are equal and positive. A lone voted outcome, or a runner-up of 0, is
/// not a tie; margin is `+∞`.
pub fn margin_of(ordering: &[(OutcomeId, u64)]) -> (f64, bool) {
    match ordering {
        [] => (0.0, false),
        [_one] => (f64::INFINITY, false),
        [(w_id, w), (r_id, r), ..] => {
            let _ = (w_id, r_id);
            if *w == 0 && *r == 0 {
                (0.0, false)
            } else if *w == *r {
                (1.0, true)
            } else if *r == 0 {
                (f64::INFINITY, false)
            } else {
                (*w as f64 / *r as f64, false)
            }
        }
    }
}

pub fn quorum_met(cast_weight: u64, sitting_weight: u64, frac: f64) -> bool {
    if sitting_weight == 0 || cast_weight == 0 {
        return false;
    }
    (cast_weight as f64) + 1e-12 >= (sitting_weight as f64) * frac
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::Weight;
    use crate::state::Ballot;

    fn oid(s: &str) -> OutcomeId {
        OutcomeId::parse(s).unwrap()
    }
    fn pid(s: &str) -> PrincipalId {
        PrincipalId::parse(s).unwrap()
    }
    fn outcome(id: &str) -> (OutcomeId, Outcome) {
        let id = oid(id);
        (
            id.clone(),
            Outcome {
                id,
                proposed_by: pid("x"),
                body: "b".into(),
                enacts_policy: None,
            },
        )
    }
    fn ballot(voter: &str, outcome: &str, weight: Weight) -> (PrincipalId, Ballot) {
        let voter = pid(voter);
        (
            voter.clone(),
            Ballot {
                ts: 1,
                voter,
                outcome: oid(outcome),
                reason: "because".into(),
                weight,
            },
        )
    }

    #[test]
    fn plurality_sums_weights_and_keeps_unvoted() {
        let outcomes: BTreeMap<_, _> = [outcome("ban"), outcome("no-action"), outcome("warn")]
            .into_iter()
            .collect();
        let ballots: BTreeMap<_, _> = [
            ballot("a", "ban", 3),
            ballot("b", "ban", 1),
            ballot("c", "warn", 1),
        ]
        .into_iter()
        .collect();
        let t = tally(&outcomes, &ballots);
        assert_eq!(
            t.ordering,
            vec![(oid("ban"), 4), (oid("warn"), 1), (oid("no-action"), 0),]
        );
        assert_eq!(t.cast_weight, 5);
        assert_eq!(t.distinct_voters, 3);
    }

    #[test]
    fn equal_top_is_a_tie() {
        let (m, tied) = margin_of(&[(oid("a"), 3), (oid("b"), 3)]);
        assert_eq!(m, 1.0);
        assert!(tied);
    }

    #[test]
    fn runner_up_zero_is_infinite_margin() {
        let (m, tied) = margin_of(&[(oid("a"), 3), (oid("b"), 0)]);
        assert!(m.is_infinite());
        assert!(!tied);
    }

    #[test]
    fn half_the_bench_meets_half_quorum() {
        assert!(quorum_met(2, 4, 0.5));
        assert!(!quorum_met(1, 4, 0.5));
        assert!(!quorum_met(0, 4, 0.5));
        assert!(!quorum_met(4, 0, 0.5));
    }
}
