use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::events::{Event, RosterMember};
use crate::ids::{CaseId, PrincipalId};
use crate::scoring::{margin_of, quorum_met, tally};
use crate::state::{
    Attempt, Ballot, BenchSeat, BenchSnapshot, Case, ClerkNote, Evidence, Fold, Outcome, Policy,
    PolicyVersion, Principal, Statement, Verdict,
};
use crate::types::{response_window_ms, rules_for, DecisionKind, Hearing, Phase, Reject, Seat, Ts};

/// Materialized court. Fold events through [`Self::submit`]; the attempt
/// journal is the history, including rejects.
#[derive(Debug, Default)]
pub struct GovState {
    pub principals: HashMap<PrincipalId, Principal>,
    pub cases: BTreeMap<CaseId, Case>,
    pub policies: BTreeMap<crate::ids::PolicyId, Policy>,
    pub attempts: Vec<Attempt>,
}

impl GovState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate-then-fold. Always appends an [`Attempt`]. Rejected events
    /// touch no other state.
    pub fn submit(&mut self, ev: Event) -> Result<(), Reject> {
        let recorded = ev.clone();
        match self.validate_and_fold(ev) {
            Ok(()) => {
                self.attempts.push(Attempt {
                    seq: self.attempts.len() as u64,
                    event: recorded,
                    fold: Fold::Accepted,
                });
                Ok(())
            }
            Err(r) => {
                self.attempts.push(Attempt {
                    seq: self.attempts.len() as u64,
                    event: recorded,
                    fold: Fold::Rejected(r.clone()),
                });
                Err(r)
            }
        }
    }

    /// Same as [`Self::submit`]. Name kept for the original TDSL call sites.
    pub fn apply(&mut self, ev: Event) -> Result<(), Reject> {
        self.submit(ev)
    }

    /// Boot replay: fold an already-accepted event and journal it as accepted.
    pub fn replay_accepted(&mut self, ev: Event) -> Result<(), Reject> {
        self.validate_and_fold(ev.clone())?;
        self.attempts.push(Attempt {
            seq: self.attempts.len() as u64,
            event: ev,
            fold: Fold::Accepted,
        });
        Ok(())
    }

    fn known(&self, p: &PrincipalId) -> Result<&Principal, Reject> {
        self.principals
            .get(p)
            .ok_or_else(|| Reject::UnknownPrincipal(p.clone()))
    }

    fn is_clerk(&self, p: &PrincipalId) -> Result<bool, Reject> {
        Ok(self.known(p)?.is_clerk())
    }

    fn sovereign(&self, p: &PrincipalId) -> Result<(), Reject> {
        let pr = self.known(p)?;
        if pr.is_clerk() {
            return Err(Reject::ClerkCannotSovereign(p.clone()));
        }
        if !pr.is_voting_seat() {
            return Err(Reject::NotSeated(p.clone()));
        }
        Ok(())
    }

    fn ensure_principal(&mut self, id: &PrincipalId, ts: Ts) {
        if self.principals.contains_key(id) {
            return;
        }
        self.principals.insert(
            id.clone(),
            Principal {
                id: id.clone(),
                display_name: id.to_string(),
                seat: None,
                weight: 0,
                discord_role_ids: Vec::new(),
                seen_ts: ts,
            },
        );
    }

    fn case_in(&mut self, id: &CaseId, phases: &[Phase]) -> Result<&mut Case, Reject> {
        let c = self
            .cases
            .get_mut(id)
            .ok_or_else(|| Reject::UnknownCase(id.clone()))?;
        if !phases.contains(&c.phase) {
            return Err(Reject::WrongPhase {
                case: id.clone(),
                have: c.phase,
                need: "see event preconditions",
            });
        }
        Ok(c)
    }

    fn snapshot_bench(&self, case: &Case, ts: Ts) -> Result<BenchSnapshot, Reject> {
        let mut seats: Vec<BenchSeat> = self
            .principals
            .values()
            .filter(|p| p.is_voting_seat())
            .filter(|p| !case.recused.contains(&p.id))
            .filter(|p| case.subject.as_ref() != Some(&p.id))
            .map(|p| BenchSeat {
                principal: p.id.clone(),
                seat: p.seat.clone().unwrap_or(Seat::Justice),
                weight: p.weight,
            })
            .collect();
        seats.sort_by(|a, b| a.principal.cmp(&b.principal));
        if seats.is_empty() {
            return Err(Reject::EmptyBench);
        }
        Ok(BenchSnapshot { ts, seats })
    }

    fn validate_and_fold(&mut self, ev: Event) -> Result<(), Reject> {
        match ev {
            Event::PrincipalSeen {
                ts,
                id,
                display_name,
            } => {
                match self.principals.get_mut(&id) {
                    Some(p) => {
                        p.display_name = display_name;
                        p.seen_ts = ts;
                    }
                    None => {
                        self.principals.insert(
                            id.clone(),
                            Principal {
                                id,
                                display_name,
                                seat: None,
                                weight: 0,
                                discord_role_ids: Vec::new(),
                                seen_ts: ts,
                            },
                        );
                    }
                }
                Ok(())
            }

            Event::RosterSynced { ts, members } => {
                for p in self.principals.values_mut() {
                    p.seat = None;
                    p.weight = 0;
                    p.discord_role_ids.clear();
                }
                for RosterMember {
                    id,
                    display_name,
                    seat,
                    weight,
                    discord_role_ids,
                } in members
                {
                    self.principals.insert(
                        id.clone(),
                        Principal {
                            id,
                            display_name,
                            seat: Some(seat),
                            weight,
                            discord_role_ids,
                            seen_ts: ts,
                        },
                    );
                }
                Ok(())
            }

            Event::CaseOpened {
                ts,
                id,
                kind,
                hearing,
                opened_by,
                brief,
                subject,
                target_case,
            } => {
                self.sovereign(&opened_by)?;
                if self.cases.contains_key(&id) {
                    return Err(Reject::DuplicateCase(id));
                }
                if brief.trim().is_empty() {
                    return Err(Reject::EmptyBrief);
                }
                if hearing == Hearing::Required && subject.is_none() {
                    return Err(Reject::MissingSubject);
                }
                if kind == DecisionKind::Appeal {
                    let t = target_case.as_ref().ok_or(Reject::MissingTarget)?;
                    let tc = self
                        .cases
                        .get(t)
                        .ok_or_else(|| Reject::UnknownCase(t.clone()))?;
                    if tc.phase != Phase::Closed {
                        return Err(Reject::AppealTargetNotClosed(t.clone()));
                    }
                }
                if let Some(ref sub) = subject {
                    self.ensure_principal(sub, ts);
                }
                self.cases.insert(
                    id.clone(),
                    Case {
                        id,
                        kind,
                        hearing,
                        phase: Phase::Intake,
                        opened_by,
                        opened_ts: ts,
                        brief,
                        subject,
                        target_case,
                        recused: BTreeSet::new(),
                        bench: None,
                        evidence: BTreeMap::new(),
                        notified_ts: None,
                        response: None,
                        outcomes: BTreeMap::new(),
                        ballots: BTreeMap::new(),
                        clerk_notes: BTreeMap::new(),
                        cited_policies: BTreeSet::new(),
                        verdict: None,
                    },
                );
                Ok(())
            }

            Event::Recused { case, who, by, .. } => {
                self.sovereign(&by)?;
                self.known(&who)?;
                if self.is_clerk(&who)? {
                    return Err(Reject::ClerkCannotSovereign(who));
                }
                let c = self.case_in(&case, &[Phase::Intake, Phase::Noticed])?;
                if c.subject.as_ref() == Some(&who) {
                    return Err(Reject::SubjectCannotAct(who));
                }
                c.recused.insert(who);
                Ok(())
            }

            Event::RecusalLifted { case, who, by, .. } => {
                self.sovereign(&by)?;
                let c = self.case_in(&case, &[Phase::Intake, Phase::Noticed])?;
                c.recused.remove(&who);
                Ok(())
            }

            Event::EvidenceFiled {
                ts,
                case,
                by,
                id,
                label,
                body,
            } => {
                if !self.principals.contains_key(&by) {
                    return Err(Reject::UnknownPrincipal(by));
                }
                if body.trim().is_empty() || label.trim().is_empty() {
                    return Err(Reject::EmptyBody);
                }
                let c =
                    self.case_in(&case, &[Phase::Intake, Phase::Noticed, Phase::Deliberation])?;
                if c.evidence.contains_key(&id) {
                    return Err(Reject::DuplicateEvidence(id));
                }
                c.evidence.insert(
                    id.clone(),
                    Evidence {
                        id,
                        ts,
                        filed_by: by,
                        label,
                        body,
                    },
                );
                Ok(())
            }

            Event::SubjectNotified { ts, case, by } => {
                self.sovereign(&by)?;
                let hearing = self
                    .cases
                    .get(&case)
                    .ok_or_else(|| Reject::UnknownCase(case.clone()))?
                    .hearing;
                if hearing != Hearing::Required {
                    return Err(Reject::HearingNotRequired);
                }
                let c = self.case_in(&case, &[Phase::Intake])?;
                if c.subject.is_none() {
                    return Err(Reject::MissingSubject);
                }
                c.notified_ts = Some(ts);
                c.phase = Phase::Noticed;
                Ok(())
            }

            Event::ResponseFiled { ts, case, by, body } => {
                let hearing = self
                    .cases
                    .get(&case)
                    .ok_or_else(|| Reject::UnknownCase(case.clone()))?
                    .hearing;
                if hearing != Hearing::Required {
                    return Err(Reject::HearingNotRequired);
                }
                if body.trim().is_empty() {
                    return Err(Reject::EmptyBody);
                }
                let c = self.case_in(&case, &[Phase::Noticed, Phase::Deliberation])?;
                if c.subject.as_ref() != Some(&by) {
                    return Err(Reject::SubjectCannotAct(by));
                }
                c.response = Some(Statement { ts, by, body });
                Ok(())
            }

            Event::OutcomeProposed {
                case,
                by,
                id,
                body,
                enacts_policy,
                ..
            } => {
                self.sovereign(&by)?;
                if body.trim().is_empty() {
                    return Err(Reject::EmptyBody);
                }
                let c = self.case_in(&case, &[Phase::Intake, Phase::Noticed])?;
                if c.outcomes.contains_key(&id) {
                    return Err(Reject::DuplicateOutcome(id));
                }
                c.outcomes.insert(
                    id.clone(),
                    Outcome {
                        id,
                        proposed_by: by,
                        body,
                        enacts_policy,
                    },
                );
                Ok(())
            }

            Event::PolicyCited {
                case, by, policy, ..
            } => {
                if !self.principals.contains_key(&by) {
                    return Err(Reject::UnknownPrincipal(by));
                }
                let case_id = case.clone();
                let c =
                    self.case_in(&case, &[Phase::Intake, Phase::Noticed, Phase::Deliberation])?;
                c.cited_policies.insert(policy.clone());
                if let Some(p) = self.policies.get_mut(&policy) {
                    p.cited_in.insert(case_id);
                }
                Ok(())
            }

            Event::DeliberationOpened { ts, case, by } => {
                self.sovereign(&by)?;
                let (hearing, kind, phase, notified_ts, has_response) = {
                    let c = self
                        .cases
                        .get(&case)
                        .ok_or_else(|| Reject::UnknownCase(case.clone()))?;
                    (
                        c.hearing,
                        c.kind,
                        c.phase,
                        c.notified_ts,
                        c.response.is_some(),
                    )
                };
                match hearing {
                    Hearing::None => {
                        if phase != Phase::Intake {
                            return Err(Reject::WrongPhase {
                                case: case.clone(),
                                have: phase,
                                need: "Intake",
                            });
                        }
                    }
                    Hearing::Required => {
                        if phase != Phase::Noticed {
                            return Err(Reject::WrongPhase {
                                case: case.clone(),
                                have: phase,
                                need: "Noticed",
                            });
                        }
                        if let Some(window) = response_window_ms(kind, hearing) {
                            let notified = notified_ts.ok_or(Reject::WrongPhase {
                                case: case.clone(),
                                have: phase,
                                need: "Noticed",
                            })?;
                            let ends = notified + window;
                            if !has_response && ts < ends {
                                return Err(Reject::SubjectUnheard { window_ends: ends });
                            }
                        }
                    }
                }
                let bench = {
                    let c = self.cases.get(&case).unwrap();
                    self.snapshot_bench(c, ts)?
                };
                let c = self.cases.get_mut(&case).unwrap();
                c.bench = Some(bench);
                c.phase = Phase::Deliberation;
                Ok(())
            }

            Event::VoteCast {
                ts,
                case,
                voter,
                outcome,
                reason,
            } => {
                // Franchise is the frozen bench, not the live Discord roster.
                self.known(&voter)?;
                if self.is_clerk(&voter)? {
                    return Err(Reject::ClerkCannotSovereign(voter));
                }
                if reason.trim().is_empty() {
                    return Err(Reject::EmptyReason);
                }
                let c = self.case_in(&case, &[Phase::Deliberation])?;
                if c.subject.as_ref() == Some(&voter) {
                    return Err(Reject::SubjectCannotAct(voter));
                }
                if c.recused.contains(&voter) {
                    return Err(Reject::Recused(voter));
                }
                let weight = c
                    .bench
                    .as_ref()
                    .and_then(|b| b.seat(&voter))
                    .map(|s| s.weight)
                    .ok_or_else(|| Reject::NotOnBench(voter.clone()))?;
                if !c.outcomes.contains_key(&outcome) {
                    return Err(Reject::UnknownOutcome(outcome));
                }
                c.ballots.insert(
                    voter.clone(),
                    Ballot {
                        ts,
                        voter,
                        outcome,
                        reason,
                        weight,
                    },
                );
                Ok(())
            }

            Event::ClerkNoteFiled {
                ts,
                case,
                clerk,
                id,
                kind,
                body,
                cites,
            } => {
                if !self.is_clerk(&clerk)? {
                    return Err(Reject::HumanCannotClerk(clerk));
                }
                if body.trim().is_empty() {
                    return Err(Reject::EmptyBody);
                }
                let c =
                    self.case_in(&case, &[Phase::Intake, Phase::Noticed, Phase::Deliberation])?;
                if c.clerk_notes.contains_key(&id) {
                    return Err(Reject::DuplicateNote(id));
                }
                c.clerk_notes.insert(
                    id.clone(),
                    ClerkNote {
                        id,
                        ts,
                        clerk,
                        kind,
                        body,
                        cites,
                    },
                );
                Ok(())
            }

            Event::CaseClosed { ts, case, by } => {
                // A justice frozen onto this bench may close even if they
                // later lost the live Discord role.
                let on_frozen_bench = self
                    .cases
                    .get(&case)
                    .and_then(|c| c.bench.as_ref())
                    .and_then(|b| b.seat(&by))
                    .is_some();
                if !on_frozen_bench {
                    self.sovereign(&by)?;
                }
                let (kind, verdict_opt) = {
                    let c = self.case_in(&case, &[Phase::Deliberation])?;
                    let r = rules_for(c.kind);
                    let sitting_weight = c.bench.as_ref().map(|b| b.sitting_weight()).unwrap_or(0);
                    let scored = tally(&c.outcomes, &c.ballots);
                    let (margin, tied) = margin_of(&scored.ordering);
                    let winner_weight = scored.ordering.first().map(|(_, w)| *w).unwrap_or(0);
                    let ok = quorum_met(scored.cast_weight, sitting_weight, r.quorum_weight_frac)
                        && !tied
                        && margin >= r.min_margin
                        && winner_weight > 0;
                    if ok {
                        let v = Verdict {
                            ts,
                            closed_by: by.clone(),
                            winner: scored.ordering[0].0.clone(),
                            ordering: scored.ordering,
                            cast_weight: scored.cast_weight,
                            sitting_weight,
                            distinct_voters: scored.distinct_voters,
                            margin,
                            hearing: c.hearing,
                            bench: c.bench.clone().expect("bench frozen at deliberation"),
                        };
                        c.verdict = Some(v.clone());
                        c.phase = Phase::Closed;
                        (c.kind, Some(v))
                    } else {
                        c.phase = Phase::Lapsed;
                        (c.kind, None)
                    }
                };

                if let Some(v) = verdict_opt {
                    if kind == DecisionKind::Policy {
                        let (pid, body, cid) = {
                            let c = &self.cases[&case];
                            let o = &c.outcomes[&v.winner];
                            (o.enacts_policy.clone(), o.body.clone(), c.id.clone())
                        };
                        if let Some(pid) = pid {
                            let p = self.policies.entry(pid.clone()).or_insert(Policy {
                                id: pid,
                                versions: Vec::new(),
                                cited_in: BTreeSet::new(),
                                repealed: false,
                            });
                            p.versions.push(PolicyVersion {
                                ts,
                                body,
                                enacted_by_case: cid,
                            });
                            p.repealed = false;
                        }
                    }
                    if kind == DecisionKind::Appeal && v.winner.as_str() == "vacate" {
                        let target = self.cases[&case].target_case.clone();
                        if let Some(t) = target {
                            if let Some(tc) = self.cases.get_mut(&t) {
                                tc.phase = Phase::Vacated;
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests;
