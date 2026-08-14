//! Court pages. Maud builds the markup; CSS is a paper docket, not a product.

use std::collections::HashMap;

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::ids::PrincipalId;
use crate::links::{path, Cite};
use crate::state::{Case, Policy, Principal, Verdict};
use crate::types::{Hearing, Phase, Seat};

fn name_of(people: &HashMap<PrincipalId, Principal>, id: &PrincipalId) -> String {
    people
        .get(id)
        .map(|p| p.display_name.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.to_string())
}

fn fmt_margin(m: f64) -> String {
    if !m.is_finite() {
        "unopposed".into()
    } else {
        format!("{m:.2}")
    }
}

const STYLES: &str = r#"
:root {
  --paper: #f3eee4;
  --sheet: #fbf7ef;
  --ink: #1c1814;
  --muted: #6a6258;
  --rule: #d4cbb8;
  --link: #1a4d38;
  --link-hover: #0d2e22;
  --stamp: #7a1f1f;
  --ok: #1d4a32;
  --ok-wash: #e4efe6;
  --warn-wash: #f6e6d8;
  --field: #fffdf8;
}
* { box-sizing: border-box; }
html { background: var(--paper); }
body {
  margin: 0 auto;
  max-width: 46rem;
  padding: 1.75rem 1.25rem 4rem;
  color: var(--ink);
  font: 16px/1.5 "Iowan Old Style", "Palatino Linotype", Palatino, "Times New Roman", serif;
}
header.mast {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 1rem;
  padding-bottom: 0.85rem;
  margin-bottom: 1.6rem;
  border-bottom: 2px solid var(--ink);
}
.brand {
  font-size: 1.35rem;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  text-decoration: none;
  color: var(--ink);
  font-weight: 700;
}
.who { display: flex; align-items: baseline; gap: 0.75rem; flex-wrap: wrap; font-size: 0.92rem; }
.whoami { font-weight: 600; }
.seat {
  color: var(--muted);
  font-variant: small-caps;
  letter-spacing: 0.04em;
}
a { color: var(--link); text-underline-offset: 0.15em; }
a:hover { color: var(--link-hover); }
h1 {
  font-size: 2rem;
  font-weight: 650;
  letter-spacing: -0.02em;
  margin: 0 0 0.35rem;
  line-height: 1.15;
}
h2 {
  font-size: 1.05rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  margin: 1.6rem 0 0.55rem;
  padding-bottom: 0.25rem;
  border-bottom: 1px solid var(--rule);
}
h3 { font-size: 1rem; margin: 0 0 0.7rem; }
.lede { font-size: 1.15rem; margin: 0.4rem 0 0.8rem; }
.meta { color: var(--muted); font-size: 0.92rem; }
.chips { display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.5rem 0 1rem; }
.chip {
  display: inline-block;
  padding: 0.12rem 0.5rem;
  border: 1px solid var(--rule);
  background: var(--sheet);
  font-size: 0.82rem;
  letter-spacing: 0.03em;
}
.chip.phase-intake { border-color: #b89a5a; }
.chip.phase-noticed { border-color: #8a6a3a; }
.chip.phase-deliberation { border-color: #3d6a8a; color: #1d3a4a; }
.chip.phase-closed { border-color: var(--ok); color: var(--ok); background: var(--ok-wash); }
.chip.phase-lapsed, .chip.phase-vacated { border-color: var(--stamp); color: var(--stamp); }
.bench { list-style: none; padding: 0; margin: 0 0 1.4rem; display: grid; gap: 0.45rem; }
.bench li {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.45rem 0.65rem;
  background: var(--sheet);
  border: 1px solid var(--rule);
}
.bench .wt { color: var(--muted); font-variant: small-caps; }
table { width: 100%; border-collapse: collapse; margin: 0.4rem 0 1.4rem; }
th {
  text-align: left;
  font-size: 0.78rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
  border-bottom: 1px solid var(--ink);
  padding: 0.35rem 0.4rem;
}
td { padding: 0.55rem 0.4rem; border-bottom: 1px solid var(--rule); vertical-align: top; }
tr:hover td { background: rgba(255,255,255,0.45); }
.empty { color: var(--muted); font-style: italic; }
form.panel, .panel {
  margin: 1.1rem 0;
  padding: 1rem 1.05rem 1.1rem;
  background: var(--sheet);
  border: 1px solid var(--rule);
  box-shadow: 2px 2px 0 rgba(28,24,20,0.04);
}
.field { display: block; margin: 0.55rem 0; }
.field span { display: block; font-size: 0.78rem; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); margin-bottom: 0.2rem; }
input, select, textarea {
  width: 100%;
  max-width: 28rem;
  padding: 0.4rem 0.5rem;
  border: 1px solid #b7ae9c;
  background: var(--field);
  color: var(--ink);
  font: inherit;
}
textarea { min-height: 5rem; max-width: 100%; }
button {
  font: inherit;
  font-size: 0.95rem;
  letter-spacing: 0.04em;
  padding: 0.4rem 0.9rem;
  border: 1px solid var(--ink);
  background: var(--ink);
  color: var(--sheet);
  cursor: pointer;
}
button:hover { background: #000; }
button.quiet { background: transparent; color: var(--ink); }
.flash { background: var(--warn-wash); border: 1px solid #c9a07a; padding: 0.6rem 0.8rem; }
.verdict {
  margin: 1.4rem 0;
  padding: 1.1rem 1.15rem;
  border: 2px solid var(--ok);
  background: var(--ok-wash);
}
.verdict h2 { border: 0; margin-top: 0; color: var(--ok); }
.verdict .winner { font-size: 1.35rem; margin: 0.2rem 0 0.5rem; }
.record { list-style: none; padding: 0; margin: 0 0 0.6rem; }
.record li { padding: 0.45rem 0; border-bottom: 1px dotted var(--rule); }
.record .by { color: var(--muted); }
.log { font-size: 0.82rem; }
.log code {
  display: block;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: "IBM Plex Mono", "Source Code Pro", ui-monospace, monospace;
  font-size: 0.75rem;
  line-height: 1.4;
  color: #2a241c;
}
.fold-accepted { color: var(--ok); }
.fold-rejected { color: var(--stamp); }
.person-id { font-family: ui-monospace, monospace; color: var(--muted); }
"#;

fn layout(title: &str, who: Option<&Principal>, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — Court" }
                style { (PreEscaped(STYLES)) }
            }
            body {
                header.mast {
                    a.brand href="/" data-testid="home" { "Court" }
                    (mast_who(who))
                }
                (body)
            }
        }
    }
}

fn mast_who(who: Option<&Principal>) -> Markup {
    html! {
        @if let Some(p) = who {
            @let seat = p
                .seat
                .as_ref()
                .map(|s| format!("{} · weight {}", s.label(), p.weight))
                .unwrap_or_else(|| "no seat".into());
            div.who {
                span.whoami data-testid="whoami" { (p.display_name) }
                span.seat data-testid="seat" { (seat) }
                a href="/logout" data-testid="logout" { "logout" }
            }
        } @else {
            div.who {
                a href="/login" data-testid="login" { "Log in with Discord" }
            }
        }
    }
}

fn flash(msg: Option<&str>) -> Markup {
    html! {
        @if let Some(m) = msg {
            p.flash data-testid="flash" { (m) }
        }
    }
}

pub fn login_required(msg: &str) -> Markup {
    layout(
        "Login",
        None,
        html! {
            p data-testid="need-login" { (msg) }
            p {
                a href="/login" data-testid="login" { "Log in with Discord" }
            }
        },
    )
}

pub fn flash_page(who: Option<&Principal>, msg: &str) -> Markup {
    layout(
        "Notice",
        who,
        html! {
            p.flash data-testid="flash" { (msg) }
        },
    )
}

pub fn docket(
    who: Option<&Principal>,
    cases: &[&Case],
    bench: &[Principal],
    notice: Option<&str>,
) -> Markup {
    layout(
        "Docket",
        who,
        html! {
            (flash(notice))
            h1 { "Docket" }
            p.meta {
                a href="/log" { "event log" }
            }
            h2 { "Bench" }
            ul.bench data-testid="bench" {
                @for p in bench {
                    li {
                        a href=(path(&Cite::Principal { id: p.id.clone() })) { (p.display_name) }
                        span.wt {
                            (p.seat.as_ref().map(Seat::label).unwrap_or("—"))
                            " · "
                            (p.weight)
                        }
                    }
                }
            }
            h2 { "Cases" }
            table data-testid="docket" {
                thead {
                    tr {
                        th { "Case" }
                        th { "Kind" }
                        th { "Phase" }
                        th { "Brief" }
                    }
                }
                tbody {
                    @if cases.is_empty() {
                        tr {
                            td colspan="4" class="empty" data-testid="empty-docket" { "No cases yet." }
                        }
                    } @else {
                        @for c in cases {
                            tr data-testid=(format!("case-row-{}", c.id)) {
                                td {
                                    a href=(path(&Cite::Case { id: c.id.clone() })) data-testid=(format!("case-link-{}", c.id)) {
                                        (c.id)
                                    }
                                }
                                td { (c.kind.as_str()) }
                                td data-testid=(format!("phase-{}", c.id)) {
                                    span class={"chip phase-" (c.phase.as_str())} { (c.phase.as_str()) }
                                }
                                td { (c.brief) }
                            }
                        }
                    }
                }
            }
            @if who.is_some_and(|p| p.is_voting_seat()) {
                (open_case_form())
            }
        },
    )
}

fn open_case_form() -> Markup {
    html! {
        form.panel method="post" action="/cases" data-testid="open-case" {
            h3 { "Open a case" }
            label.field {
                span { "Id" }
                input name="id" data-testid="case-id" required;
            }
            label.field {
                span { "Kind" }
                select name="kind" data-testid="case-kind" {
                    option value="record" selected { "record" }
                    option value="routine" { "routine" }
                    option value="personnel" { "personnel" }
                    option value="policy" { "policy" }
                    option value="constitutional" { "constitutional" }
                    option value="appeal" { "appeal" }
                }
            }
            label.field {
                span { "Hearing" }
                select name="hearing" data-testid="case-hearing" {
                    option value="none" selected { "none (record only)" }
                    option value="required" { "required" }
                }
            }
            label.field {
                span { "Subject" }
                input name="subject" data-testid="case-subject" placeholder="discord or steam id";
            }
            label.field {
                span { "Appeal target" }
                input name="target_case" data-testid="case-target";
            }
            label.field {
                span { "Brief" }
                textarea name="brief" data-testid="case-brief" required {}
            }
            button type="submit" data-testid="open-case-submit" { "Open" }
        }
    }
}

pub fn case_page(
    who: Option<&Principal>,
    case: &Case,
    notice: Option<&str>,
    people: &HashMap<PrincipalId, Principal>,
) -> Markup {
    let seated = who.is_some_and(|p| p.is_voting_seat());
    let on_bench = who.is_some_and(|p| case.bench.as_ref().and_then(|b| b.seat(&p.id)).is_some());
    let is_subject = who.is_some_and(|p| case.subject.as_ref() == Some(&p.id));
    let title = format!("Case {}", case.id);

    layout(
        &title,
        who,
        html! {
            (flash(notice))
            h1 data-testid="case-title" { (case.id) }
            p.lede data-testid="case-brief" { (case.brief) }
            div.chips data-testid="case-meta" {
                span.chip { (case.kind) }
                span.chip { "hearing " (case.hearing) }
                span class={"chip phase-" (case.phase.as_str())} data-testid="case-phase" {
                    (case.phase)
                }
            }
            p.meta {
                "Subject: "
                @if let Some(id) = &case.subject {
                    a href=(path(&Cite::Principal { id: id.clone() })) data-testid="subject" {
                        (name_of(people, id))
                    }
                } @else {
                    "—"
                }
            }
            h2 { "Evidence" }
            ul.record data-testid="evidence-list" {
                @if case.evidence.is_empty() {
                    li.empty { "None filed." }
                } @else {
                    @for e in case.evidence.values() {
                        li id=(format!("evidence-{}", e.id)) data-testid=(format!("evidence-{}", e.id)) {
                            a href=(path(&Cite::Evidence {
                                case: case.id.clone(),
                                id: e.id.clone(),
                            })) { (e.id) }
                            " — " (e.label) " — " (e.body) " "
                            span.by { "by " (name_of(people, &e.filed_by)) }
                        }
                    }
                }
            }
            h2 { "Outcomes" }
            ul.record data-testid="outcome-list" {
                @if case.outcomes.is_empty() {
                    li.empty { "None proposed." }
                } @else {
                    @for o in case.outcomes.values() {
                        li data-testid=(format!("outcome-{}", o.id)) {
                            strong { (o.id) }
                            " — " (o.body)
                        }
                    }
                }
            }
            h2 { "Ballots" }
            ul.record data-testid="ballot-list" {
                @if case.ballots.is_empty() {
                    li.empty { "No ballots yet." }
                } @else {
                    @for b in case.ballots.values() {
                        li data-testid=(format!("ballot-{}", b.voter)) {
                            strong { (name_of(people, &b.voter)) }
                            " → " (b.outcome) " "
                            span.by { "weight " (b.weight) }
                            " — " (b.reason)
                        }
                    }
                }
            }
            @if let Some(v) = &case.verdict {
                (render_verdict(v))
            }
            (case_actions(case, seated, on_bench, is_subject))
        },
    )
}

fn case_actions(case: &Case, seated: bool, on_bench: bool, is_subject: bool) -> Markup {
    let live = matches!(
        case.phase,
        Phase::Intake | Phase::Noticed | Phase::Deliberation
    );
    let proposing = matches!(case.phase, Phase::Intake | Phase::Noticed);
    html! {
        @if seated && live {
            form.panel method="post" action=(format!("/cases/{}/evidence", case.id)) data-testid="file-evidence" {
                h3 { "File evidence" }
                label.field {
                    span { "Id" }
                    input name="id" data-testid="evidence-id" required;
                }
                label.field {
                    span { "Label" }
                    input name="label" data-testid="evidence-label" required;
                }
                label.field {
                    span { "Body" }
                    textarea name="body" data-testid="evidence-body" required {}
                }
                button type="submit" data-testid="evidence-submit" { "File" }
            }
        }
        @if seated && proposing {
            form.panel method="post" action=(format!("/cases/{}/outcomes", case.id)) data-testid="propose-outcome" {
                h3 { "Propose outcome" }
                label.field {
                    span { "Id" }
                    input name="id" data-testid="outcome-id" required;
                }
                label.field {
                    span { "Body" }
                    textarea name="body" data-testid="outcome-body" required {}
                }
                label.field {
                    span { "Enacts policy" }
                    input name="enacts_policy" data-testid="outcome-policy";
                }
                button type="submit" data-testid="outcome-submit" { "Propose" }
            }
            @if case.hearing == Hearing::Required && case.phase == Phase::Intake {
                form.panel method="post" action=(format!("/cases/{}/notify", case.id)) data-testid="notify-subject" {
                    button type="submit" data-testid="notify-submit" { "Notify subject" }
                }
            }
            form.panel method="post" action=(format!("/cases/{}/deliberate", case.id)) data-testid="open-deliberation" {
                button type="submit" data-testid="deliberate-submit" { "Open deliberation" }
            }
        }
        @if is_subject && case.hearing == Hearing::Required && case.phase == Phase::Noticed {
            form.panel method="post" action=(format!("/cases/{}/respond", case.id)) data-testid="respond" {
                h3 { "Response" }
                label.field {
                    span { "Body" }
                    textarea name="body" data-testid="response-body" required {}
                }
                button type="submit" data-testid="response-submit" { "File response" }
            }
        }
        @if (on_bench || seated) && case.phase == Phase::Deliberation {
            form.panel method="post" action=(format!("/cases/{}/vote", case.id)) data-testid="cast-vote" {
                h3 { "Vote" }
                label.field {
                    span { "Outcome" }
                    select name="outcome" data-testid="vote-outcome" {
                        @for o in case.outcomes.keys() {
                            option value=(o) { (o) }
                        }
                    }
                }
                label.field {
                    span { "Reason" }
                    textarea name="reason" data-testid="vote-reason" required {}
                }
                button type="submit" data-testid="vote-submit" { "Cast ballot" }
            }
            form.panel method="post" action=(format!("/cases/{}/close", case.id)) data-testid="close-case" {
                button type="submit" data-testid="close-submit" { "Close case" }
            }
        }
    }
}

fn render_verdict(v: &Verdict) -> Markup {
    html! {
        section.verdict data-testid="verdict" {
            h2 { "Verdict" }
            p.winner data-testid="winner" {
                "Winner: "
                strong { (v.winner) }
            }
            p.meta {
                "cast " (v.cast_weight)
                " / sitting " (v.sitting_weight)
                " · margin " (fmt_margin(v.margin))
                " · hearing " (v.hearing)
            }
            ol data-testid="ordering" {
                @for (id, w) in &v.ordering {
                    li { (id) " — " (w) }
                }
            }
        }
    }
}

pub fn person_page(who: Option<&Principal>, person: &Principal) -> Markup {
    layout(
        &person.display_name,
        who,
        html! {
            h1 data-testid="person-name" { (person.display_name) }
            p.person-id data-testid="person-id" { (person.id) }
            p.chip {
                (person.seat.as_ref().map(Seat::label).unwrap_or("no seat"))
                " · weight "
                (person.weight)
            }
        },
    )
}

pub fn policy_page(who: Option<&Principal>, policy: &Policy) -> Markup {
    let title = format!("Policy {}", policy.id);
    layout(
        &title,
        who,
        html! {
            h1 data-testid="policy-id" { (policy.id) }
            @if policy.repealed {
                p.chip.phase-vacated { "repealed" }
            }
            p.meta { (policy.versions.len()) " versions" }
            h2 { "Versions" }
            ul.record data-testid="policy-versions" {
                @if policy.versions.is_empty() {
                    li.empty { "No versions." }
                } @else {
                    @for v in &policy.versions {
                        li {
                            a href=(path(&Cite::Case { id: v.enacted_by_case.clone() })) {
                                (v.enacted_by_case)
                            }
                            " — " (v.body)
                        }
                    }
                }
            }
        },
    )
}

pub fn log_page(who: Option<&Principal>, lines: &[(u64, String, String)]) -> Markup {
    layout(
        "Log",
        who,
        html! {
            h1 { "Event log" }
            table.log data-testid="log" {
                thead {
                    tr {
                        th { "seq" }
                        th { "fold" }
                        th { "event" }
                    }
                }
                tbody {
                    @for (seq, fold, ev) in lines {
                        @let fold_class = if fold.starts_with("rejected") {
                            "fold-rejected"
                        } else {
                            "fold-accepted"
                        };
                        tr id=(format!("log-{seq}")) data-testid=(format!("log-{seq}")) {
                            td { (seq) }
                            td class=(fold_class) { (fold) }
                            td { code { (ev) } }
                        }
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maud_escapes_flash_text() {
        let page = flash_page(None, "<script>alert(1)</script>");
        let s = page.into_string();
        assert!(s.contains("data-testid=\"flash\""));
        assert!(s.contains("&lt;script&gt;"));
        assert!(!s.contains("<script>alert"));
    }

    #[test]
    fn login_keeps_testids() {
        let s = login_required("Log in to act.").into_string();
        assert!(s.contains("data-testid=\"need-login\""));
        assert!(s.contains("data-testid=\"login\""));
        assert!(s.contains("Log in to act."));
    }
}
