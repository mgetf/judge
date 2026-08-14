use crate::events::{Event, RosterMember};
use crate::ids::{CaseId, EvidenceId, NoteId, OutcomeId, PolicyId, PrincipalId, Weight};
use crate::links::Cite;
use crate::reducer::GovState;
use crate::state::Fold;
use crate::types::{ClerkNoteKind, DecisionKind, Hearing, Phase, Reject, Seat, Ts};

fn pid(s: &str) -> PrincipalId {
    PrincipalId::parse(s).unwrap()
}
fn cid(s: &str) -> CaseId {
    CaseId::parse(s).unwrap()
}
fn oid(s: &str) -> OutcomeId {
    OutcomeId::parse(s).unwrap()
}
fn eid(s: &str) -> EvidenceId {
    EvidenceId::parse(s).unwrap()
}
fn nid(s: &str) -> NoteId {
    NoteId::parse(s).unwrap()
}
fn pol(s: &str) -> PolicyId {
    PolicyId::parse(s).unwrap()
}

fn member(id: &str, seat: Seat, weight: Weight) -> RosterMember {
    RosterMember {
        id: pid(id),
        display_name: id.to_string(),
        seat,
        weight,
        discord_role_ids: Vec::new(),
    }
}

fn bench3() -> Vec<RosterMember> {
    vec![
        member("tommyy", Seat::Chief, 3),
        member("neptune", Seat::Justice, 1),
        member("abood", Seat::Justice, 1),
        member(
            "clerk",
            Seat::Clerk {
                model: Some("claude".into()),
            },
            0,
        ),
    ]
}

fn sync(s: &mut GovState, members: Vec<RosterMember>) {
    s.apply(Event::RosterSynced { ts: 0, members }).unwrap();
}

fn open(
    s: &mut GovState,
    id: &str,
    kind: DecisionKind,
    hearing: Hearing,
    subject: Option<&str>,
    target: Option<&str>,
) {
    s.apply(Event::CaseOpened {
        ts: 1,
        id: cid(id),
        kind,
        hearing,
        opened_by: pid("tommyy"),
        brief: format!("brief for {id}"),
        subject: subject.map(pid),
        target_case: target.map(cid),
    })
    .unwrap();
}

fn propose(s: &mut GovState, case: &str, id: &str, body: &str, policy: Option<&str>) {
    s.apply(Event::OutcomeProposed {
        ts: 2,
        case: cid(case),
        by: pid("tommyy"),
        id: oid(id),
        body: body.into(),
        enacts_policy: policy.map(pol),
    })
    .unwrap();
}

fn evidence(s: &mut GovState, case: &str, id: &str, body: &str) {
    s.apply(Event::EvidenceFiled {
        ts: 2,
        case: cid(case),
        by: pid("abood"),
        id: eid(id),
        label: id.into(),
        body: body.into(),
        href: None,
        filename: None,
    })
    .unwrap();
}

fn deliberate(s: &mut GovState, case: &str, ts: Ts) {
    s.apply(Event::DeliberationOpened {
        ts,
        case: cid(case),
        by: pid("tommyy"),
    })
    .unwrap();
}

fn vote(s: &mut GovState, case: &str, voter: &str, outcome: &str, reason: &str) {
    s.apply(Event::VoteCast {
        ts: 10,
        case: cid(case),
        voter: pid(voter),
        outcome: oid(outcome),
        reason: reason.into(),
    })
    .unwrap();
}

fn close(s: &mut GovState, case: &str) -> Result<(), Reject> {
    s.apply(Event::CaseClosed {
        ts: 20,
        case: cid(case),
        by: pid("tommyy"),
    })
}

fn last_fold(s: &GovState) -> &Fold {
    &s.attempts.last().unwrap().fold
}

#[test]
fn record_cheat_case_skips_hearing_and_freezes_verdict() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-cheat",
        DecisionKind::Record,
        Hearing::None,
        Some("76561198000000000"),
        None,
    );
    assert!(s.principals.contains_key(&pid("76561198000000000")));
    evidence(
        &mut s,
        "case-cheat",
        "demo-stv",
        "https://mge.tf/demos/abc STV: aim snap at 3:12",
    );
    propose(
        &mut s,
        "case-cheat",
        "cheat-ban",
        "Permanent cheat ban",
        None,
    );
    propose(&mut s, "case-cheat", "no-action", "Not convinced", None);
    deliberate(&mut s, "case-cheat", 3);
    vote(
        &mut s,
        "case-cheat",
        "tommyy",
        "cheat-ban",
        "Demo is unambiguous.",
    );
    vote(
        &mut s,
        "case-cheat",
        "abood",
        "cheat-ban",
        "Same snap on the STV.",
    );
    close(&mut s, "case-cheat").unwrap();

    let c = &s.cases[&cid("case-cheat")];
    assert_eq!(c.phase, Phase::Closed);
    assert_eq!(c.hearing, Hearing::None);
    assert!(c.response.is_none());
    assert!(c.notified_ts.is_none());
    let v = c.verdict.as_ref().unwrap();
    assert_eq!(v.winner, oid("cheat-ban"));
    assert_eq!(v.hearing, Hearing::None);
    assert_eq!(v.cast_weight, 4);
    assert_eq!(v.sitting_weight, 5);
    assert_eq!(
        crate::links::path(&Cite::Case {
            id: cid("case-cheat")
        }),
        "/cases/case-cheat"
    );
    assert_eq!(
        crate::links::path(&Cite::Evidence {
            case: cid("case-cheat"),
            id: eid("demo-stv"),
        }),
        "/cases/case-cheat#evidence-demo-stv"
    );
}

#[test]
fn hearing_none_rejects_notice_and_response() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-x",
        DecisionKind::Record,
        Hearing::None,
        Some("cheater"),
        None,
    );
    let err = s
        .apply(Event::SubjectNotified {
            ts: 2,
            case: cid("case-x"),
            by: pid("tommyy"),
        })
        .unwrap_err();
    assert_eq!(err, Reject::HearingNotRequired);
    assert!(matches!(
        last_fold(&s),
        Fold::Rejected(Reject::HearingNotRequired)
    ));

    let err = s
        .apply(Event::ResponseFiled {
            ts: 3,
            case: cid("case-x"),
            by: pid("cheater"),
            body: "I didn't do it".into(),
        })
        .unwrap_err();
    assert_eq!(err, Reject::HearingNotRequired);
}

#[test]
fn hearing_required_blocks_deliberation_until_response_or_window() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-0007",
        DecisionKind::Personnel,
        Hearing::Required,
        Some("ricochet"),
        None,
    );
    propose(&mut s, "case-0007", "demote", "Remove roles", None);
    propose(&mut s, "case-0007", "warn", "Warning", None);

    let err = s
        .apply(Event::DeliberationOpened {
            ts: 2,
            case: cid("case-0007"),
            by: pid("tommyy"),
        })
        .unwrap_err();
    assert!(matches!(
        err,
        Reject::WrongPhase {
            need: "Noticed",
            ..
        }
    ));

    s.apply(Event::SubjectNotified {
        ts: 5,
        case: cid("case-0007"),
        by: pid("tommyy"),
    })
    .unwrap();

    let err = s
        .apply(Event::DeliberationOpened {
            ts: 6,
            case: cid("case-0007"),
            by: pid("tommyy"),
        })
        .unwrap_err();
    assert!(matches!(err, Reject::SubjectUnheard { .. }));

    s.apply(Event::ResponseFiled {
        ts: 7,
        case: cid("case-0007"),
        by: pid("ricochet"),
        body: "The logs were under the limit.".into(),
    })
    .unwrap();
    deliberate(&mut s, "case-0007", 9);
    assert_eq!(s.cases[&cid("case-0007")].phase, Phase::Deliberation);
}

#[test]
fn hearing_required_window_lapse_allows_deliberation() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-w",
        DecisionKind::Personnel,
        Hearing::Required,
        Some("ricochet"),
        None,
    );
    propose(&mut s, "case-w", "demote", "Remove roles", None);
    s.apply(Event::SubjectNotified {
        ts: 5,
        case: cid("case-w"),
        by: pid("tommyy"),
    })
    .unwrap();
    let after = 5 + 48 * 3_600_000;
    deliberate(&mut s, "case-w", after);
    assert!(s.cases[&cid("case-w")].response.is_none());
    assert_eq!(s.cases[&cid("case-w")].phase, Phase::Deliberation);
}

#[test]
fn hearing_required_without_subject_is_rejected() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    let err = s
        .apply(Event::CaseOpened {
            ts: 1,
            id: cid("case-bad"),
            kind: DecisionKind::Personnel,
            hearing: Hearing::Required,
            opened_by: pid("tommyy"),
            brief: "no one named".into(),
            subject: None,
            target_case: None,
        })
        .unwrap_err();
    assert_eq!(err, Reject::MissingSubject);
}

#[test]
fn personnel_hearing_full_lifecycle_weighted_votes() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-0007",
        DecisionKind::Personnel,
        Hearing::Required,
        Some("ricochet"),
        None,
    );
    evidence(&mut s, "case-0007", "audit-tm-move", "Site audit log link");
    evidence(
        &mut s,
        "case-0007",
        "msg-deletion",
        "Discord audit screenshot",
    );
    for (id, body) in [
        ("demote", "Remove all mod/admin roles"),
        ("warn", "Formal warning"),
        ("no-action", "Unsubstantiated"),
    ] {
        propose(&mut s, "case-0007", id, body, None);
    }
    s.apply(Event::SubjectNotified {
        ts: 5,
        case: cid("case-0007"),
        by: pid("tommyy"),
    })
    .unwrap();
    s.apply(Event::ResponseFiled {
        ts: 7,
        case: cid("case-0007"),
        by: pid("ricochet"),
        body: "TM's logs were under the limit.".into(),
    })
    .unwrap();
    s.apply(Event::ClerkNoteFiled {
        ts: 8,
        case: cid("case-0007"),
        clerk: pid("clerk"),
        id: nid("contradiction-1"),
        kind: ClerkNoteKind::Contradiction,
        body: "Response conflicts with audit-tm-move.".into(),
        cites: vec![Cite::Evidence {
            case: cid("case-0007"),
            id: eid("audit-tm-move"),
        }],
    })
    .unwrap();

    let err = s
        .apply(Event::VoteCast {
            ts: 8,
            case: cid("case-0007"),
            voter: pid("clerk"),
            outcome: oid("demote"),
            reason: "no".into(),
        })
        .unwrap_err();
    assert_eq!(err, Reject::ClerkCannotSovereign(pid("clerk")));

    deliberate(&mut s, "case-0007", 9);
    vote(
        &mut s,
        "case-0007",
        "abood",
        "demote",
        "Shifting stories against a clean audit trail.",
    );
    vote(
        &mut s,
        "case-0007",
        "neptune",
        "demote",
        "Deleting a founder's message crosses the line.",
    );
    vote(
        &mut s,
        "case-0007",
        "tommyy",
        "demote",
        "Precedent matters more than the person.",
    );
    close(&mut s, "case-0007").unwrap();

    let c = &s.cases[&cid("case-0007")];
    assert_eq!(c.phase, Phase::Closed);
    let v = c.verdict.as_ref().unwrap();
    assert_eq!(v.winner, oid("demote"));
    assert_eq!(v.cast_weight, 5);
    assert_eq!(v.sitting_weight, 5);
    assert_eq!(v.distinct_voters, 3);
    assert_eq!(
        c.clerk_notes[&nid("contradiction-1")].cites[0].path(),
        "/cases/case-0007#evidence-audit-tm-move"
    );

    let err = s
        .apply(Event::VoteCast {
            ts: 21,
            case: cid("case-0007"),
            voter: pid("abood"),
            outcome: oid("warn"),
            reason: "pile-on".into(),
        })
        .unwrap_err();
    assert!(matches!(err, Reject::WrongPhase { .. }));
    assert!(matches!(
        last_fold(&s),
        Fold::Rejected(Reject::WrongPhase { .. })
    ));
}

#[test]
fn last_ballot_replaces_and_uses_frozen_weight() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-r",
        DecisionKind::Record,
        Hearing::None,
        Some("x"),
        None,
    );
    propose(&mut s, "case-r", "ban", "ban", None);
    propose(&mut s, "case-r", "no", "no", None);
    deliberate(&mut s, "case-r", 3);
    vote(&mut s, "case-r", "tommyy", "no", "first thought");
    vote(&mut s, "case-r", "tommyy", "ban", "changed my mind");
    assert_eq!(s.cases[&cid("case-r")].ballots.len(), 1);
    assert_eq!(
        s.cases[&cid("case-r")].ballots[&pid("tommyy")].outcome,
        oid("ban")
    );
    assert_eq!(s.cases[&cid("case-r")].ballots[&pid("tommyy")].weight, 3);
}

#[test]
fn later_roster_sync_does_not_rewrite_a_frozen_bench() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-f",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-f", "yes", "yes", None);
    propose(&mut s, "case-f", "no", "no", None);
    deliberate(&mut s, "case-f", 3);
    let frozen = s.cases[&cid("case-f")].bench.clone().unwrap();
    assert_eq!(frozen.sitting_weight(), 5);

    sync(
        &mut s,
        vec![
            member("tommyy", Seat::Chief, 99),
            member("neptune", Seat::Justice, 1),
        ],
    );
    vote(&mut s, "case-f", "tommyy", "yes", "still the old weight");
    assert_eq!(s.cases[&cid("case-f")].ballots[&pid("tommyy")].weight, 3);
    assert_eq!(
        s.cases[&cid("case-f")]
            .bench
            .as_ref()
            .unwrap()
            .sitting_weight(),
        5
    );
    // Dropped from the live roster, still on the frozen bench: may vote.
    vote(
        &mut s,
        "case-f",
        "abood",
        "yes",
        "I was snapshotted onto this bench",
    );
    // Gained a live seat after the snapshot: cannot join this case.
    s.apply(Event::RosterSynced {
        ts: 4,
        members: vec![
            member("tommyy", Seat::Chief, 99),
            member("newbie", Seat::Justice, 1),
        ],
    })
    .unwrap();
    let err = s
        .apply(Event::VoteCast {
            ts: 12,
            case: cid("case-f"),
            voter: pid("newbie"),
            outcome: oid("yes"),
            reason: "I just got the role".into(),
        })
        .unwrap_err();
    assert_eq!(err, Reject::NotOnBench(pid("newbie")));
}

#[test]
fn recusal_drops_a_justice_from_the_snapshot() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-rec",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    s.apply(Event::Recused {
        ts: 2,
        case: cid("case-rec"),
        who: pid("abood"),
        by: pid("abood"),
    })
    .unwrap();
    propose(&mut s, "case-rec", "yes", "yes", None);
    deliberate(&mut s, "case-rec", 3);
    let bench = s.cases[&cid("case-rec")].bench.as_ref().unwrap();
    assert!(bench.seat(&pid("abood")).is_none());
    assert_eq!(bench.sitting_weight(), 4);
    let err = s
        .apply(Event::VoteCast {
            ts: 10,
            case: cid("case-rec"),
            voter: pid("abood"),
            outcome: oid("yes"),
            reason: "should fail".into(),
        })
        .unwrap_err();
    assert_eq!(err, Reject::Recused(pid("abood")));
}

#[test]
fn recusal_after_deliberation_is_rejected() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-late",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-late", "yes", "yes", None);
    deliberate(&mut s, "case-late", 3);
    let err = s
        .apply(Event::Recused {
            ts: 4,
            case: cid("case-late"),
            who: pid("abood"),
            by: pid("tommyy"),
        })
        .unwrap_err();
    assert!(matches!(err, Reject::WrongPhase { .. }));
}

#[test]
fn subject_is_excluded_from_the_bench_even_if_seated() {
    let mut s = GovState::new();
    let mut members = bench3();
    members.push(member("ricochet", Seat::Justice, 1));
    sync(&mut s, members);
    open(
        &mut s,
        "case-sub",
        DecisionKind::Personnel,
        Hearing::None,
        Some("ricochet"),
        None,
    );
    propose(&mut s, "case-sub", "demote", "demote", None);
    deliberate(&mut s, "case-sub", 3);
    let bench = s.cases[&cid("case-sub")].bench.as_ref().unwrap();
    assert!(bench.seat(&pid("ricochet")).is_none());
    let err = s
        .apply(Event::VoteCast {
            ts: 10,
            case: cid("case-sub"),
            voter: pid("ricochet"),
            outcome: oid("demote"),
            reason: "I vote to keep my job".into(),
        })
        .unwrap_err();
    assert_eq!(err, Reject::SubjectCannotAct(pid("ricochet")));
}

#[test]
fn empty_reason_and_unknown_outcome_are_rejected() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-v",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-v", "yes", "yes", None);
    deliberate(&mut s, "case-v", 3);
    assert_eq!(
        s.apply(Event::VoteCast {
            ts: 10,
            case: cid("case-v"),
            voter: pid("tommyy"),
            outcome: oid("yes"),
            reason: "   ".into(),
        })
        .unwrap_err(),
        Reject::EmptyReason
    );
    assert_eq!(
        s.apply(Event::VoteCast {
            ts: 10,
            case: cid("case-v"),
            voter: pid("tommyy"),
            outcome: oid("nope"),
            reason: "because".into(),
        })
        .unwrap_err(),
        Reject::UnknownOutcome(oid("nope"))
    );
}

#[test]
fn clerk_cannot_open_and_human_cannot_clerk() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    assert_eq!(
        s.apply(Event::CaseOpened {
            ts: 1,
            id: cid("c"),
            kind: DecisionKind::Record,
            hearing: Hearing::None,
            opened_by: pid("clerk"),
            brief: "no".into(),
            subject: None,
            target_case: None,
        })
        .unwrap_err(),
        Reject::ClerkCannotSovereign(pid("clerk"))
    );
    open(
        &mut s,
        "c2",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    assert_eq!(
        s.apply(Event::ClerkNoteFiled {
            ts: 2,
            case: cid("c2"),
            clerk: pid("tommyy"),
            id: nid("n"),
            kind: ClerkNoteKind::Summary,
            body: "no".into(),
            cites: vec![],
        })
        .unwrap_err(),
        Reject::HumanCannotClerk(pid("tommyy"))
    );
}

#[test]
fn unseated_principal_cannot_open() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    s.apply(Event::PrincipalSeen {
        ts: 1,
        id: pid("rando"),
        display_name: "rando".into(),
    })
    .unwrap();
    assert_eq!(
        s.apply(Event::CaseOpened {
            ts: 2,
            id: cid("c"),
            kind: DecisionKind::Record,
            hearing: Hearing::None,
            opened_by: pid("rando"),
            brief: "hi".into(),
            subject: None,
            target_case: None,
        })
        .unwrap_err(),
        Reject::NotSeated(pid("rando"))
    );
}

#[test]
fn close_without_quorum_lapses() {
    let mut s = GovState::new();
    sync(&mut s, bench3()); // sitting 5, half = 2.5 so need 3
    open(
        &mut s,
        "case-q",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-q", "yes", "yes", None);
    propose(&mut s, "case-q", "no", "no", None);
    deliberate(&mut s, "case-q", 3);
    vote(&mut s, "case-q", "abood", "yes", "only one justice"); // weight 1
    close(&mut s, "case-q").unwrap();
    assert_eq!(s.cases[&cid("case-q")].phase, Phase::Lapsed);
    assert!(s.cases[&cid("case-q")].verdict.is_none());
}

#[test]
fn exact_tie_lapses() {
    let mut s = GovState::new();
    sync(
        &mut s,
        vec![
            member("tommyy", Seat::Justice, 1),
            member("neptune", Seat::Justice, 1),
        ],
    );
    open(
        &mut s,
        "case-t",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-t", "yes", "yes", None);
    propose(&mut s, "case-t", "no", "no", None);
    deliberate(&mut s, "case-t", 3);
    vote(&mut s, "case-t", "tommyy", "yes", "yes");
    vote(&mut s, "case-t", "neptune", "no", "no");
    close(&mut s, "case-t").unwrap();
    assert_eq!(s.cases[&cid("case-t")].phase, Phase::Lapsed);
}

#[test]
fn personnel_margin_failure_lapses() {
    // sitting 4: chief 3 + justice 1. Both vote, 3 vs 1 = margin 3 >= 1.15 would pass.
    // Use two justices weight 2 and 2? 2 vs 2 is a tie.
    // Use 3 vs 3? Need margin < 1.15: 7 vs 6 = 1.166 >= 1.15.
    // 8 vs 7 = 1.14 < 1.15.
    let mut s = GovState::new();
    sync(
        &mut s,
        vec![
            member("tommyy", Seat::Chief, 8),
            member("neptune", Seat::Justice, 7),
        ],
    );
    open(
        &mut s,
        "case-m",
        DecisionKind::Personnel,
        Hearing::None,
        Some("x"),
        None,
    );
    propose(&mut s, "case-m", "demote", "d", None);
    propose(&mut s, "case-m", "warn", "w", None);
    deliberate(&mut s, "case-m", 3);
    vote(&mut s, "case-m", "tommyy", "demote", "yes");
    vote(&mut s, "case-m", "neptune", "warn", "no");
    close(&mut s, "case-m").unwrap();
    assert_eq!(s.cases[&cid("case-m")].phase, Phase::Lapsed);
}

#[test]
fn policy_case_enacts_precedent_and_appeal_can_vacate() {
    let mut s = GovState::new();
    sync(&mut s, bench3());

    open(
        &mut s,
        "case-0008",
        DecisionKind::Policy,
        Hearing::None,
        None,
        None,
    );
    propose(
        &mut s,
        "case-0008",
        "hard-mute-1wk",
        "Hard server mute, one week.",
        Some("moderation/slur-penalty"),
    );
    propose(
        &mut s,
        "case-0008",
        "perma",
        "Immediate permanent ban.",
        Some("moderation/slur-penalty"),
    );
    deliberate(&mut s, "case-0008", 3);
    vote(
        &mut s,
        "case-0008",
        "tommyy",
        "hard-mute-1wk",
        "Proportionality keeps players.",
    );
    vote(
        &mut s,
        "case-0008",
        "abood",
        "hard-mute-1wk",
        "Mute is enforceable.",
    );
    close(&mut s, "case-0008").unwrap();

    let p = &s.policies[&pol("moderation/slur-penalty")];
    assert_eq!(p.versions.len(), 1);
    assert_eq!(p.versions[0].enacted_by_case, cid("case-0008"));
    assert_eq!(
        crate::links::path(&Cite::Policy {
            id: pol("moderation/slur-penalty")
        }),
        "/policies/moderation/slur-penalty"
    );

    open(
        &mut s,
        "case-0009",
        DecisionKind::Routine,
        Hearing::None,
        None,
        None,
    );
    s.apply(Event::PolicyCited {
        ts: 6,
        case: cid("case-0009"),
        by: pid("abood"),
        policy: pol("moderation/slur-penalty"),
    })
    .unwrap();
    assert!(s.policies[&pol("moderation/slur-penalty")]
        .cited_in
        .contains(&cid("case-0009")));

    open(
        &mut s,
        "case-0010",
        DecisionKind::Appeal,
        Hearing::None,
        None,
        Some("case-0008"),
    );
    propose(&mut s, "case-0010", "vacate", "Vacate case-0008", None);
    propose(&mut s, "case-0010", "uphold", "Uphold", None);
    deliberate(&mut s, "case-0010", 8);
    vote(
        &mut s,
        "case-0010",
        "neptune",
        "vacate",
        "process was rushed",
    );
    vote(&mut s, "case-0010", "abood", "vacate", "process was rushed");
    vote(
        &mut s,
        "case-0010",
        "tommyy",
        "vacate",
        "process was rushed",
    );
    close(&mut s, "case-0010").unwrap();

    assert_eq!(s.cases[&cid("case-0008")].phase, Phase::Vacated);
    assert!(s.cases[&cid("case-0008")].verdict.is_some());
}

#[test]
fn appeal_target_must_be_closed() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "open-one",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    let err = s
        .apply(Event::CaseOpened {
            ts: 2,
            id: cid("appeal"),
            kind: DecisionKind::Appeal,
            hearing: Hearing::None,
            opened_by: pid("tommyy"),
            brief: "too soon".into(),
            subject: None,
            target_case: Some(cid("open-one")),
        })
        .unwrap_err();
    assert_eq!(err, Reject::AppealTargetNotClosed(cid("open-one")));
}

#[test]
fn duplicate_ids_are_rejected() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-d",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    assert!(matches!(
        s.apply(Event::CaseOpened {
            ts: 2,
            id: cid("case-d"),
            kind: DecisionKind::Record,
            hearing: Hearing::None,
            opened_by: pid("tommyy"),
            brief: "again".into(),
            subject: None,
            target_case: None,
        })
        .unwrap_err(),
        Reject::DuplicateCase(_)
    ));
    propose(&mut s, "case-d", "yes", "yes", None);
    assert!(matches!(
        s.apply(Event::OutcomeProposed {
            ts: 3,
            case: cid("case-d"),
            by: pid("tommyy"),
            id: oid("yes"),
            body: "again".into(),
            enacts_policy: None,
        })
        .unwrap_err(),
        Reject::DuplicateOutcome(_)
    ));
    evidence(&mut s, "case-d", "e1", "body");
    assert!(matches!(
        s.apply(Event::EvidenceFiled {
            ts: 4,
            case: cid("case-d"),
            by: pid("abood"),
            id: eid("e1"),
            label: "e1".into(),
            body: "again".into(),
            href: None,
            filename: None,
        })
        .unwrap_err(),
        Reject::DuplicateEvidence(_)
    ));
}

#[test]
fn evidence_may_be_an_exhibit_without_a_note() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-ex",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    s.apply(Event::EvidenceFiled {
        ts: 2,
        case: cid("case-ex"),
        by: pid("abood"),
        id: eid("snap"),
        label: "STV snap".into(),
        body: String::new(),
        href: Some("http://court.test/blobs/cases/case-ex/snap/stv-snap.png".into()),
        filename: Some("stv-snap.png".into()),
    })
    .unwrap();
    assert!(s.apply(Event::EvidenceFiled {
        ts: 3,
        case: cid("case-ex"),
        by: pid("abood"),
        id: eid("empty"),
        label: "nothing".into(),
        body: String::new(),
        href: None,
        filename: None,
    })
    .is_err());
}

#[test]
fn every_attempt_is_journaled_including_rejects() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    let n = s.attempts.len();
    let _ = s.apply(Event::CaseOpened {
        ts: 1,
        id: cid("z"),
        kind: DecisionKind::Record,
        hearing: Hearing::None,
        opened_by: pid("rando"),
        brief: "no".into(),
        subject: None,
        target_case: None,
    });
    assert_eq!(s.attempts.len(), n + 1);
    assert!(matches!(
        s.attempts.last().unwrap().fold,
        Fold::Rejected(Reject::UnknownPrincipal(_))
    ));
}

#[test]
fn roster_sync_clears_seats_of_people_not_listed() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    assert!(s.principals[&pid("abood")].is_voting_seat());
    sync(&mut s, vec![member("tommyy", Seat::Chief, 3)]);
    assert!(!s.principals[&pid("abood")].is_voting_seat());
    assert!(s.principals.contains_key(&pid("abood")));
    assert!(s.principals[&pid("tommyy")].is_voting_seat());
}

#[test]
fn empty_brief_rejected() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    assert_eq!(
        s.apply(Event::CaseOpened {
            ts: 1,
            id: cid("e"),
            kind: DecisionKind::Record,
            hearing: Hearing::None,
            opened_by: pid("tommyy"),
            brief: "  ".into(),
            subject: None,
            target_case: None,
        })
        .unwrap_err(),
        Reject::EmptyBrief
    );
}

#[test]
fn single_outcome_unanimous_closes() {
    let mut s = GovState::new();
    sync(&mut s, bench3());
    open(
        &mut s,
        "case-one",
        DecisionKind::Record,
        Hearing::None,
        None,
        None,
    );
    propose(&mut s, "case-one", "confirm", "confirm the record", None);
    deliberate(&mut s, "case-one", 3);
    vote(&mut s, "case-one", "tommyy", "confirm", "yes");
    vote(&mut s, "case-one", "neptune", "confirm", "yes");
    close(&mut s, "case-one").unwrap();
    let v = s.cases[&cid("case-one")].verdict.as_ref().unwrap();
    assert_eq!(v.winner, oid("confirm"));
    assert!(v.margin.is_infinite());
}
