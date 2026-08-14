use crate::links::{path, Cite};
use crate::state::{Case, Principal, Verdict};
use crate::types::{Hearing, Phase, Seat};

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn layout(title: &str, who: Option<&Principal>, body: &str) -> String {
    let nav = match who {
        Some(p) => {
            let seat = p
                .seat
                .as_ref()
                .map(|s| format!("{} · {}", s.label(), p.weight))
                .unwrap_or_else(|| "no seat".into());
            format!(
                "<span data-testid=\"whoami\">{}</span> \
                 <span data-testid=\"seat\">{seat}</span> \
                 <a href=\"/logout\" data-testid=\"logout\">logout</a>",
                esc(&p.display_name)
            )
        }
        None => "<a href=\"/login\" data-testid=\"login\">Log in with Discord</a>".into(),
    };
    format!(
        "<!doctype html><html lang=\"en\"><head>\
         <meta charset=\"utf-8\"/>\
         <title>{} — Court</title>\
         <style>
           body {{ font: 16px/1.45 system-ui, sans-serif; max-width: 52rem; margin: 2rem auto; padding: 0 1rem; color: #111; }}
           header {{ display:flex; gap:1rem; align-items:baseline; flex-wrap:wrap; border-bottom:1px solid #ccc; padding-bottom:.5rem; }}
           h1,h2 {{ font-weight:600; }}
           a {{ color:#0b4; }}
           form {{ margin:1rem 0; padding:1rem; border:1px solid #ddd; }}
           label {{ display:block; margin:.4rem 0; }}
           input,select,textarea {{ width:100%; max-width:32rem; }}
           .flash {{ background:#fee; padding:.5rem 1rem; }}
           .verdict {{ background:#efe; padding:1rem; }}
           .meta {{ color:#555; font-size:.9rem; }}
           table {{ border-collapse:collapse; width:100%; }}
           td,th {{ text-align:left; padding:.3rem .5rem; border-bottom:1px solid #eee; }}
         </style></head><body>\
         <header><a href=\"/\" data-testid=\"home\"><strong>Court</strong></a>{nav}</header>\
         {body}</body></html>",
        esc(title)
    )
}

pub fn login_required(msg: &str) -> String {
    layout(
        "Login",
        None,
        &format!(
            "<p data-testid=\"need-login\">{msg}</p>\
             <p><a href=\"/login\" data-testid=\"login\">Log in with Discord</a></p>"
        ),
    )
}

pub fn flash_page(who: Option<&Principal>, msg: &str) -> String {
    layout(
        "Notice",
        who,
        &format!("<p class=\"flash\" data-testid=\"flash\">{}</p>", esc(msg)),
    )
}

pub fn docket(
    who: Option<&Principal>,
    cases: &[&Case],
    bench: &[Principal],
    flash: Option<&str>,
) -> String {
    let mut rows = String::new();
    for c in cases {
        rows.push_str(&format!(
            "<tr data-testid=\"case-row-{id}\">\
               <td><a href=\"{href}\" data-testid=\"case-link-{id}\">{id}</a></td>\
               <td>{}</td><td data-testid=\"phase-{id}\">{}</td>\
               <td>{}</td></tr>",
            esc(c.kind.as_str()),
            esc(c.phase.as_str()),
            esc(&c.brief),
            id = c.id,
            href = path(&Cite::Case { id: c.id.clone() }),
        ));
    }
    if rows.is_empty() {
        rows = "<tr><td colspan=\"4\" data-testid=\"empty-docket\">No cases yet.</td></tr>".into();
    }
    let mut seats = String::new();
    for p in bench {
        seats.push_str(&format!(
            "<li><a href=\"{}\">{}</a> — {} · {}</li>",
            path(&Cite::Principal { id: p.id.clone() }),
            esc(&p.display_name),
            p.seat.as_ref().map(Seat::label).unwrap_or("—"),
            p.weight
        ));
    }
    let flash = flash
        .map(|m| format!("<p class=\"flash\" data-testid=\"flash\">{}</p>", esc(m)))
        .unwrap_or_default();
    let open_form = if who.is_some_and(|p| p.is_voting_seat()) {
        r#"<form method="post" action="/cases" data-testid="open-case">
          <h2>Open a case</h2>
          <label>Id <input name="id" data-testid="case-id" required/></label>
          <label>Kind <select name="kind" data-testid="case-kind">
            <option value="record" selected>record</option>
            <option value="routine">routine</option>
            <option value="personnel">personnel</option>
            <option value="policy">policy</option>
            <option value="constitutional">constitutional</option>
            <option value="appeal">appeal</option>
          </select></label>
          <label>Hearing <select name="hearing" data-testid="case-hearing">
            <option value="none" selected>none (record only)</option>
            <option value="required">required</option>
          </select></label>
          <label>Subject <input name="subject" data-testid="case-subject" placeholder="discord or steam id"/></label>
          <label>Appeal target <input name="target_case" data-testid="case-target"/></label>
          <label>Brief <textarea name="brief" data-testid="case-brief" required></textarea></label>
          <button type="submit" data-testid="open-case-submit">Open</button>
        </form>"#
            .to_string()
    } else {
        String::new()
    };
    layout(
        "Docket",
        who,
        &format!(
            "{flash}\
             <h1>Docket</h1>\
             <p class=\"meta\"><a href=\"/log\">event log</a></p>\
             <h2>Bench</h2><ul data-testid=\"bench\">{seats}</ul>\
             <table data-testid=\"docket\"><thead><tr><th>Case</th><th>Kind</th><th>Phase</th><th>Brief</th></tr></thead>\
             <tbody>{rows}</tbody></table>\
             {open_form}"
        ),
    )
}

pub fn case_page(who: Option<&Principal>, case: &Case, flash: Option<&str>) -> String {
    let flash = flash
        .map(|m| format!("<p class=\"flash\" data-testid=\"flash\">{}</p>", esc(m)))
        .unwrap_or_default();
    let subject = case
        .subject
        .as_ref()
        .map(|id| {
            format!(
                "<a href=\"{}\" data-testid=\"subject\">{}</a>",
                path(&Cite::Principal { id: id.clone() }),
                esc(&id.to_string())
            )
        })
        .unwrap_or_else(|| "—".into());

    let mut evidence = String::new();
    for e in case.evidence.values() {
        evidence.push_str(&format!(
            "<li id=\"evidence-{id}\" data-testid=\"evidence-{id}\">\
               <a href=\"{href}\">{id}</a> — {} — {} <span class=\"meta\">by {}</span></li>",
            esc(&e.label),
            esc(&e.body),
            esc(&e.filed_by.to_string()),
            id = e.id,
            href = path(&Cite::Evidence {
                case: case.id.clone(),
                id: e.id.clone()
            }),
        ));
    }
    let mut outcomes = String::new();
    for o in case.outcomes.values() {
        outcomes.push_str(&format!(
            "<li data-testid=\"outcome-{}\">{} — {}</li>",
            o.id,
            esc(&o.id.to_string()),
            esc(&o.body)
        ));
    }
    let mut ballots = String::new();
    for b in case.ballots.values() {
        ballots.push_str(&format!(
            "<li data-testid=\"ballot-{}\">{} → {} (weight {}) — {}</li>",
            b.voter,
            esc(&b.voter.to_string()),
            esc(&b.outcome.to_string()),
            b.weight,
            esc(&b.reason)
        ));
    }
    let verdict = case
        .verdict
        .as_ref()
        .map(render_verdict)
        .unwrap_or_default();

    let seated = who.is_some_and(|p| p.is_voting_seat());
    let on_bench = who.is_some_and(|p| case.bench.as_ref().and_then(|b| b.seat(&p.id)).is_some());
    let is_subject = who.is_some_and(|p| case.subject.as_ref() == Some(&p.id));

    let mut actions = String::new();
    if seated
        && matches!(
            case.phase,
            Phase::Intake | Phase::Noticed | Phase::Deliberation
        )
    {
        actions.push_str(&format!(
            r#"<form method="post" action="/cases/{id}/evidence" data-testid="file-evidence">
              <h3>File evidence</h3>
              <label>Id <input name="id" data-testid="evidence-id" required/></label>
              <label>Label <input name="label" data-testid="evidence-label" required/></label>
              <label>Body <textarea name="body" data-testid="evidence-body" required></textarea></label>
              <button type="submit" data-testid="evidence-submit">File</button>
            </form>"#,
            id = case.id
        ));
    }
    if seated && matches!(case.phase, Phase::Intake | Phase::Noticed) {
        actions.push_str(&format!(
            r#"<form method="post" action="/cases/{id}/outcomes" data-testid="propose-outcome">
              <h3>Propose outcome</h3>
              <label>Id <input name="id" data-testid="outcome-id" required/></label>
              <label>Body <textarea name="body" data-testid="outcome-body" required></textarea></label>
              <label>Enacts policy <input name="enacts_policy" data-testid="outcome-policy"/></label>
              <button type="submit" data-testid="outcome-submit">Propose</button>
            </form>"#,
            id = case.id
        ));
        if case.hearing == Hearing::Required && case.phase == Phase::Intake {
            actions.push_str(&format!(
                r#"<form method="post" action="/cases/{id}/notify" data-testid="notify-subject">
                  <button type="submit" data-testid="notify-submit">Notify subject</button>
                </form>"#,
                id = case.id
            ));
        }
        actions.push_str(&format!(
            r#"<form method="post" action="/cases/{id}/deliberate" data-testid="open-deliberation">
              <button type="submit" data-testid="deliberate-submit">Open deliberation</button>
            </form>"#,
            id = case.id
        ));
    }
    if is_subject && case.hearing == Hearing::Required && case.phase == Phase::Noticed {
        actions.push_str(&format!(
            r#"<form method="post" action="/cases/{id}/respond" data-testid="respond">
              <h3>Response</h3>
              <label>Body <textarea name="body" data-testid="response-body" required></textarea></label>
              <button type="submit" data-testid="response-submit">File response</button>
            </form>"#,
            id = case.id
        ));
    }
    if (on_bench || seated) && case.phase == Phase::Deliberation {
        let mut opts = String::new();
        for o in case.outcomes.keys() {
            opts.push_str(&format!("<option value=\"{}\">{}</option>", o, o));
        }
        actions.push_str(&format!(
            r#"<form method="post" action="/cases/{id}/vote" data-testid="cast-vote">
              <h3>Vote</h3>
              <label>Outcome <select name="outcome" data-testid="vote-outcome">{opts}</select></label>
              <label>Reason <textarea name="reason" data-testid="vote-reason" required></textarea></label>
              <button type="submit" data-testid="vote-submit">Cast ballot</button>
            </form>
            <form method="post" action="/cases/{id}/close" data-testid="close-case">
              <button type="submit" data-testid="close-submit">Close case</button>
            </form>"#,
            id = case.id
        ));
    }

    layout(
        &format!("Case {}", case.id),
        who,
        &format!(
            "{flash}\
             <h1 data-testid=\"case-title\">{}</h1>\
             <p class=\"meta\" data-testid=\"case-meta\">{} · hearing {} · phase <span data-testid=\"case-phase\">{}</span></p>\
             <p data-testid=\"case-brief\">{}</p>\
             <p>Subject: {subject}</p>\
             <h2>Evidence</h2><ul data-testid=\"evidence-list\">{evidence}</ul>\
             <h2>Outcomes</h2><ul data-testid=\"outcome-list\">{outcomes}</ul>\
             <h2>Ballots</h2><ul data-testid=\"ballot-list\">{ballots}</ul>\
             {verdict}{actions}",
            case.id,
            case.kind,
            case.hearing,
            case.phase,
            esc(&case.brief),
        ),
    )
}

fn render_verdict(v: &Verdict) -> String {
    let mut order = String::new();
    for (id, w) in &v.ordering {
        order.push_str(&format!("<li>{} — {w}</li>", esc(&id.to_string())));
    }
    format!(
        "<section class=\"verdict\" data-testid=\"verdict\">\
           <h2>Verdict</h2>\
           <p data-testid=\"winner\">Winner: <strong>{}</strong></p>\
           <p class=\"meta\">cast {} / sitting {} · margin {} · hearing {}</p>\
           <ol data-testid=\"ordering\">{order}</ol>\
         </section>",
        esc(&v.winner.to_string()),
        v.cast_weight,
        v.sitting_weight,
        v.margin,
        v.hearing
    )
}

pub fn person_page(who: Option<&Principal>, person: &Principal) -> String {
    layout(
        &person.display_name,
        who,
        &format!(
            "<h1 data-testid=\"person-name\">{}</h1>\
             <p data-testid=\"person-id\">{}</p>\
             <p>{} · weight {}</p>",
            esc(&person.display_name),
            person.id,
            person.seat.as_ref().map(Seat::label).unwrap_or("no seat"),
            person.weight
        ),
    )
}

pub fn log_page(who: Option<&Principal>, lines: &[(u64, String, String)]) -> String {
    let mut rows = String::new();
    for (seq, fold, ev) in lines {
        rows.push_str(&format!(
            "<tr id=\"log-{seq}\" data-testid=\"log-{seq}\"><td>{seq}</td><td>{}</td><td><code>{}</code></td></tr>",
            esc(fold),
            esc(ev)
        ));
    }
    layout(
        "Log",
        who,
        &format!(
            "<h1>Event log</h1>\
             <table data-testid=\"log\"><thead><tr><th>seq</th><th>fold</th><th>event</th></tr></thead>\
             <tbody>{rows}</tbody></table>"
        ),
    )
}
