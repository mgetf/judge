//! Court pages are a live `see` plus buttons. Chat and attachments live in Discord.

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::action::Wire;
use crate::state::{Policy, Principal, Verdict};
use crate::types::Seat;
use crate::view::{modal_for, Btn, Modal, Target, View};

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
.actions { display: flex; flex-wrap: wrap; gap: 0.45rem; margin: 1.1rem 0; }
.record { list-style: none; padding: 0; margin: 0 0 0.6rem; }
.record li { padding: 0.45rem 0; border-bottom: 1px dotted var(--rule); }
.empty { color: var(--muted); font-style: italic; }
form.panel, .panel, dialog.panel {
  margin: 1.1rem 0;
  padding: 1rem 1.05rem 1.1rem;
  background: var(--sheet);
  border: 1px solid var(--rule);
  box-shadow: 2px 2px 0 rgba(28,24,20,0.04);
}
dialog.panel { max-width: 28rem; }
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
"#;

const LIVE_JS: &str = r#"
(function(){
  var box = document.querySelector('[data-live-root]');
  if (!box) return;
  var cite = box.getAttribute('data-cite') || 'docket';
  var es = new EventSource('/live');
  es.addEventListener('commit', function(){
    fetch('/see?cite=' + encodeURIComponent(cite), { headers: { 'Accept': 'text/html' }})
      .then(function(r){ return r.text(); })
      .then(function(html){
        var doc = new DOMParser().parseFromString(html, 'text/html');
        var next = doc.querySelector('[data-live-root]');
        if (next && box.parentNode) {
          box.replaceWith(next);
          box = document.querySelector('[data-live-root]');
        }
      }).catch(function(){});
  });
})();
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
                script { (PreEscaped(LIVE_JS)) }
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

fn cite_param(target: &Target) -> String {
    match target {
        Target::Docket => "docket".into(),
        Target::Cite(c) => match c {
            crate::links::Cite::Case { id } => format!("case:{id}"),
            crate::links::Cite::Principal { id } => format!("person:{id}"),
            crate::links::Cite::Policy { id } => format!("policy:{id}"),
            crate::links::Cite::Log { seq } => format!("log:{seq}"),
            crate::links::Cite::Evidence { case, id: _ } => format!("case:{case}"),
            crate::links::Cite::Note { case, .. } => format!("case:{case}"),
        },
    }
}

pub fn render_view(who: Option<&Principal>, view: &View) -> Markup {
    layout(&view.title, who, view_body(view))
}

fn view_body(view: &View) -> Markup {
    html! {
        div data-live-root data-cite=(cite_param(&view.target)) data-testid="live-root" {
            (flash(view.notice.as_deref()))
            h1 data-testid=(view.title_testid) { (view.title) }
            p.lede data-testid=@if matches!(view.target, Target::Cite(crate::links::Cite::Case { .. })) { "case-brief" } @else { "lede" } { (view.lede) }
            @if !view.chips.is_empty() {
                div.chips data-testid="case-meta" {
                    @for c in &view.chips {
                        span class={"chip " (c.class)} data-testid=@if c.class.starts_with("phase-") { "case-phase" } @else { "chip" } {
                            (c.label)
                        }
                    }
                }
            }
            @for m in &view.meta {
                p.meta { (m) }
            }
            @if let Some(url) = &view.channel_url {
                p.meta {
                    a href=(url) data-testid="discord-channel" { "Open Discord channel" }
                }
            }
            @for sec in &view.sections {
                h2 { (sec.heading) }
                ul.record data-testid=(sec.testid) {
                    @if sec.items.is_empty() {
                        li.empty { (sec.empty) }
                    } @else {
                        @for item in &sec.items {
                            li id=(format!("{}-{}", sec.testid, item.id)) data-testid=(item.testid) { (item.text) }
                        }
                    }
                }
            }
            @if let Some(v) = &view.verdict {
                (render_verdict(v))
            }
            (action_bar(&view.buttons, &view.target))
        }
    }
}

fn action_bar(buttons: &[Btn], target: &Target) -> Markup {
    let case = match target {
        Target::Cite(crate::links::Cite::Case { id }) => Some(id.as_str().to_string()),
        _ => None,
    };
    html! {
        div.actions data-testid="actions" {
            @for b in buttons {
                @if b.opens_modal {
                    button.quiet type="button" data-testid=(b.testid) onclick=(format!(
                        "document.getElementById('modal-{}').showModal()",
                        b.testid
                    )) { (b.label) }
                } @else {
                    form method="post" action="/eval" style="display:inline" {
                        input type="hidden" name="action" value=(verb_of(&b.custom_id));
                        @if let Some(c) = &case {
                            input type="hidden" name="case" value=(c);
                        }
                        button type="submit" data-testid=(b.testid) { (b.label) }
                    }
                }
            }
        }
        @for b in buttons {
            @if b.opens_modal {
                @if let Ok(modal) = modal_from_btn(b, case.as_deref()) {
                    (render_dialog(&modal, case.as_deref()))
                }
            }
        }
    }
}

fn verb_of(custom_id: &str) -> String {
    Wire::parse(custom_id)
        .map(|w| match w {
            Wire::Go { verb, .. } | Wire::Ask { verb, .. } | Wire::Do { verb, .. } => verb,
        })
        .unwrap_or_default()
}

fn modal_from_btn(b: &Btn, case: Option<&str>) -> Result<Modal, String> {
    let w = Wire::parse(&b.custom_id)?;
    match w {
        Wire::Ask { verb, case: c } => modal_for(&verb, c.as_deref().or(case)),
        _ => Err("not a modal".into()),
    }
}

fn render_dialog(modal: &Modal, case: Option<&str>) -> Markup {
    let verb = verb_of(&modal.custom_id);
    let dialog_id = match verb.as_str() {
        "case" | "open_case" => "modal-open-case",
        "evidence" | "file_evidence" => "modal-file-evidence",
        "outcome" | "propose_outcome" => "modal-propose-outcome",
        "vote" => "modal-cast-vote",
        "respond" => "modal-respond",
        other => other,
    };
    html! {
        dialog.panel id=(dialog_id) data-testid=(modal.testid) {
            form method="post" action="/eval" {
                h3 { (modal.title) }
                input type="hidden" name="action" value=(verb);
                @if let Some(c) = case {
                    input type="hidden" name="case" value=(c);
                }
                @for f in &modal.fields {
                    label.field {
                        span { (f.label) }
                        @if f.paragraph {
                            textarea name=(f.custom_id) data-testid=(f.testid) required[f.required] placeholder=(f.placeholder) {}
                        } @else {
                            input name=(f.custom_id) data-testid=(f.testid) required[f.required] placeholder=(f.placeholder);
                        }
                    }
                }
                button type="submit" data-testid=(format!("{}-submit", match verb.as_str() {
                    "case" | "open_case" => "open-case",
                    "evidence" | "file_evidence" => "evidence",
                    "outcome" | "propose_outcome" => "outcome",
                    "vote" => "vote",
                    "respond" => "response",
                    v => v,
                })) { "Submit" }
                button.quiet type="button" onclick="this.closest('dialog').close()" { "Cancel" }
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
                            a href=(crate::links::path(&crate::links::Cite::Case { id: v.enacted_by_case.clone() })) {
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

pub use crate::bot::see_path;

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
