//! `see`: project court state into a view. HTML and Discord both render this.
//! Buttons are the only controls; chat and attachments stay in Discord.

use std::collections::HashMap;

use crate::action::Wire;
use crate::ids::{CaseId, PrincipalId};
use crate::links::Cite;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Docket,
    Cite(Cite),
}
use crate::state::{Case, Principal, Verdict};
use crate::types::{Hearing, Phase, Seat};

#[derive(Debug, Clone)]
pub struct Chip {
    pub label: String,
    pub class: String,
}

#[derive(Debug, Clone)]
pub struct SectionItem {
    pub id: String,
    pub testid: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub heading: String,
    pub testid: String,
    pub items: Vec<SectionItem>,
    pub empty: &'static str,
}

#[derive(Debug, Clone)]
pub struct Btn {
    pub custom_id: String,
    pub label: String,
    pub testid: String,
    /// Discord button style: 1 primary, 2 secondary, 3 success, 4 danger.
    pub style: u8,
    pub opens_modal: bool,
}

#[derive(Debug, Clone)]
pub struct ModalField {
    pub custom_id: String,
    pub label: String,
    pub testid: String,
    pub paragraph: bool,
    pub required: bool,
    pub placeholder: String,
}

#[derive(Debug, Clone)]
pub struct Modal {
    pub custom_id: String,
    pub title: String,
    pub testid: String,
    pub fields: Vec<ModalField>,
}

#[derive(Debug, Clone)]
pub struct View {
    pub target: Target,
    pub title: String,
    pub title_testid: String,
    pub lede: String,
    pub chips: Vec<Chip>,
    pub meta: Vec<String>,
    pub sections: Vec<Section>,
    pub verdict: Option<Verdict>,
    pub buttons: Vec<Btn>,
    pub notice: Option<String>,
    pub channel_url: Option<String>,
}

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

pub fn see_docket(
    who: Option<&Principal>,
    cases: &[&Case],
    bench: &[Principal],
    notice: Option<&str>,
    channel_url: Option<String>,
) -> View {
    let mut sections = Vec::new();
    sections.push(Section {
        heading: "Bench".into(),
        testid: "bench".into(),
        items: bench
            .iter()
            .map(|p| SectionItem {
                id: p.id.to_string(),
                testid: format!("bench-{}", p.id),
                text: format!(
                    "{} · {} · {}",
                    p.display_name,
                    p.seat.as_ref().map(Seat::label).unwrap_or("—"),
                    p.weight
                ),
            })
            .collect(),
        empty: "No one seated.",
    });
    sections.push(Section {
        heading: "Cases".into(),
        testid: "docket".into(),
        items: cases
            .iter()
            .map(|c| SectionItem {
                id: c.id.to_string(),
                testid: format!("case-row-{}", c.id),
                text: format!("{} · {} · {}", c.id, c.kind, c.phase),
            })
            .collect(),
        empty: "No cases yet.",
    });
    let mut buttons = Vec::new();
    if who.is_none() || who.is_some_and(|p| p.is_voting_seat()) {
        buttons.push(Btn {
            custom_id: Wire::custom_id("ask", "case", None),
            label: "Open case".into(),
            testid: "open-case".into(),
            style: 1,
            opens_modal: true,
        });
    }
    View {
        target: Target::Docket,
        title: "Docket".into(),
        title_testid: "docket-title".into(),
        lede: "Court runs in Discord. This page is a live view.".into(),
        chips: Vec::new(),
        meta: Vec::new(),
        sections,
        verdict: None,
        buttons,
        notice: notice.map(str::to_string),
        channel_url,
    }
}

pub fn see_case(
    who: Option<&Principal>,
    case: &Case,
    people: &HashMap<PrincipalId, Principal>,
    notice: Option<&str>,
    channel_url: Option<String>,
) -> View {
    // Discord view is public; franchise is checked at eval. HTML still
    // hides buttons the viewer cannot press.
    let public = who.is_none();
    let seated = public || who.is_some_and(|p| p.is_voting_seat());
    let on_bench =
        public || who.is_some_and(|p| case.bench.as_ref().and_then(|b| b.seat(&p.id)).is_some());
    let is_subject = public || who.is_some_and(|p| case.subject.as_ref() == Some(&p.id));

    let subject = case
        .subject
        .as_ref()
        .map(|id| name_of(people, id))
        .unwrap_or_else(|| "—".into());

    let evidence = Section {
        heading: "Evidence".into(),
        testid: "evidence-list".into(),
        items: case
            .evidence
            .values()
            .map(|e| SectionItem {
                id: e.id.to_string(),
                testid: format!("evidence-{}", e.id),
                text: format!(
                    "{} — {} — {} · by {}",
                    e.id,
                    e.label,
                    e.body,
                    name_of(people, &e.filed_by)
                ),
            })
            .collect(),
        empty: "None filed. Drop an attachment in the case channel or use File evidence.",
    };
    let outcomes = Section {
        heading: "Outcomes".into(),
        testid: "outcome-list".into(),
        items: case
            .outcomes
            .values()
            .map(|o| SectionItem {
                id: o.id.to_string(),
                testid: format!("outcome-{}", o.id),
                text: format!("{} — {}", o.id, o.body),
            })
            .collect(),
        empty: "None proposed.",
    };
    let ballots = Section {
        heading: "Ballots".into(),
        testid: "ballot-list".into(),
        items: case
            .ballots
            .values()
            .map(|b| SectionItem {
                id: b.voter.to_string(),
                testid: format!("ballot-{}", b.voter),
                text: format!(
                    "{} → {} · weight {} — {}",
                    name_of(people, &b.voter),
                    b.outcome,
                    b.weight,
                    b.reason
                ),
            })
            .collect(),
        empty: "No ballots yet.",
    };

    View {
        target: Target::Cite(Cite::Case {
            id: case.id.clone(),
        }),
        title: case.id.to_string(),
        title_testid: "case-title".into(),
        lede: case.brief.clone(),
        chips: vec![
            Chip {
                label: case.kind.to_string(),
                class: String::new(),
            },
            Chip {
                label: format!("hearing {}", case.hearing),
                class: String::new(),
            },
            Chip {
                label: case.phase.to_string(),
                class: format!("phase-{}", case.phase.as_str()),
            },
        ],
        meta: vec![format!("Subject: {subject}")],
        sections: vec![evidence, outcomes, ballots],
        verdict: case.verdict.clone(),
        buttons: case_buttons(case, seated, on_bench, is_subject),
        notice: notice.map(str::to_string),
        channel_url,
    }
}

fn case_buttons(case: &Case, seated: bool, on_bench: bool, is_subject: bool) -> Vec<Btn> {
    let live = matches!(
        case.phase,
        Phase::Intake | Phase::Noticed | Phase::Deliberation
    );
    let proposing = matches!(case.phase, Phase::Intake | Phase::Noticed);
    let cid = case.id.as_str();
    let mut out = Vec::new();
    if seated && live {
        out.push(Btn {
            custom_id: Wire::custom_id("ask", "evidence", Some(cid)),
            label: "File evidence".into(),
            testid: "file-evidence".into(),
            style: 2,
            opens_modal: true,
        });
    }
    if seated && proposing {
        out.push(Btn {
            custom_id: Wire::custom_id("ask", "outcome", Some(cid)),
            label: "Propose outcome".into(),
            testid: "propose-outcome".into(),
            style: 2,
            opens_modal: true,
        });
        if case.hearing == Hearing::Required && case.phase == Phase::Intake {
            out.push(Btn {
                custom_id: Wire::custom_id("go", "notify", Some(cid)),
                label: "Notify subject".into(),
                testid: "notify-submit".into(),
                style: 2,
                opens_modal: false,
            });
        }
        out.push(Btn {
            custom_id: Wire::custom_id("go", "deliberate", Some(cid)),
            label: "Open deliberation".into(),
            testid: "deliberate-submit".into(),
            style: 1,
            opens_modal: false,
        });
    }
    if is_subject && case.hearing == Hearing::Required && case.phase == Phase::Noticed {
        out.push(Btn {
            custom_id: Wire::custom_id("ask", "respond", Some(cid)),
            label: "File response".into(),
            testid: "respond".into(),
            style: 1,
            opens_modal: true,
        });
    }
    if (on_bench || seated) && case.phase == Phase::Deliberation {
        out.push(Btn {
            custom_id: Wire::custom_id("ask", "vote", Some(cid)),
            label: "Cast ballot".into(),
            testid: "cast-vote".into(),
            style: 1,
            opens_modal: true,
        });
        out.push(Btn {
            custom_id: Wire::custom_id("go", "close", Some(cid)),
            label: "Close case".into(),
            testid: "close-submit".into(),
            style: 4,
            opens_modal: false,
        });
    }
    out
}

pub fn modal_for(verb: &str, case: Option<&str>) -> Result<Modal, String> {
    let do_id = Wire::custom_id("do", verb, case);
    match verb {
        "case" | "open_case" => Ok(Modal {
            custom_id: do_id,
            title: "Open a case".into(),
            testid: "open-case-modal".into(),
            fields: vec![
                field("id", "Id", "case-id", false, true, ""),
                field("kind", "Kind", "case-kind", false, false, "record"),
                field("hearing", "Hearing", "case-hearing", false, false, "none"),
                field(
                    "subject",
                    "Subject",
                    "case-subject",
                    false,
                    false,
                    "discord id",
                ),
                field(
                    "target_case",
                    "Appeal target",
                    "case-target",
                    false,
                    false,
                    "",
                ),
                field("brief", "Brief", "case-brief", true, true, ""),
            ],
        }),
        "evidence" | "file_evidence" => Ok(Modal {
            custom_id: do_id,
            title: "File evidence".into(),
            testid: "file-evidence-modal".into(),
            fields: vec![
                field("id", "Id", "evidence-id", false, true, ""),
                field("label", "Label", "evidence-label", false, true, ""),
                field(
                    "body",
                    "Attachment URL or body",
                    "evidence-body",
                    true,
                    true,
                    "Discord attachment URL",
                ),
            ],
        }),
        "outcome" | "propose_outcome" => Ok(Modal {
            custom_id: do_id,
            title: "Propose outcome".into(),
            testid: "propose-outcome-modal".into(),
            fields: vec![
                field("id", "Id", "outcome-id", false, true, ""),
                field("body", "Body", "outcome-body", true, true, ""),
                field(
                    "enacts_policy",
                    "Enacts policy",
                    "outcome-policy",
                    false,
                    false,
                    "",
                ),
            ],
        }),
        "vote" => Ok(Modal {
            custom_id: do_id,
            title: "Cast ballot".into(),
            testid: "cast-vote-modal".into(),
            fields: vec![
                field("outcome", "Outcome", "vote-outcome", false, true, ""),
                field("reason", "Reason", "vote-reason", true, true, ""),
            ],
        }),
        "respond" => Ok(Modal {
            custom_id: do_id,
            title: "Response".into(),
            testid: "respond-modal".into(),
            fields: vec![field("body", "Body", "response-body", true, true, "")],
        }),
        other => Err(format!("no modal for {other}")),
    }
}

fn field(
    custom_id: &str,
    label: &str,
    testid: &str,
    paragraph: bool,
    required: bool,
    placeholder: &str,
) -> ModalField {
    ModalField {
        custom_id: custom_id.into(),
        label: label.into(),
        testid: testid.into(),
        paragraph,
        required,
        placeholder: placeholder.into(),
    }
}

pub fn verdict_lines(v: &Verdict) -> Vec<String> {
    let mut lines = vec![
        format!("Winner: {}", v.winner),
        format!(
            "cast {} / sitting {} · margin {} · hearing {}",
            v.cast_weight,
            v.sitting_weight,
            fmt_margin(v.margin),
            v.hearing
        ),
    ];
    for (id, w) in &v.ordering {
        lines.push(format!("{id} — {w}"));
    }
    lines
}

pub fn case_channel_name(id: &CaseId) -> String {
    let mut s: String = id
        .as_str()
        .chars()
        .map(|c| match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' | '-' => c,
            '/' | '_' | '.' | '~' => '-',
            _ => '-',
        })
        .collect();
    if !s.starts_with("case-") && !s.starts_with("c-") {
        s = format!("case-{s}");
    }
    s.truncate(100);
    if s.is_empty() {
        "case".into()
    } else {
        s
    }
}

/// Discord message body for a view: one embed + button rows.
pub fn discord_payload(view: &View) -> serde_json::Value {
    let mut desc = view.lede.clone();
    if !view.meta.is_empty() {
        desc.push_str("\n");
        desc.push_str(&view.meta.join("\n"));
    }
    let mut fields = Vec::new();
    for chip in &view.chips {
        fields.push(serde_json::json!({
            "name": " ",
            "value": chip.label,
            "inline": true,
        }));
    }
    for sec in &view.sections {
        let value = if sec.items.is_empty() {
            sec.empty.to_string()
        } else {
            sec.items
                .iter()
                .map(|i| format!("• {}", i.text))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut value = value;
        if value.len() > 1024 {
            value.truncate(1021);
            value.push_str("...");
        }
        fields.push(serde_json::json!({
            "name": sec.heading,
            "value": if value.is_empty() { "—" } else { &value },
            "inline": false,
        }));
    }
    if let Some(v) = &view.verdict {
        fields.push(serde_json::json!({
            "name": "Verdict",
            "value": verdict_lines(v).join("\n"),
            "inline": false,
        }));
    }
    if fields.is_empty() {
        fields.push(serde_json::json!({
            "name": "Court",
            "value": "Live view.",
            "inline": false,
        }));
    }
    let color = if view.verdict.is_some() {
        0x1d4a32
    } else {
        0x5865f2
    };
    let embed = serde_json::json!({
        "title": view.title,
        "description": desc,
        "color": color,
        "fields": fields,
    });
    serde_json::json!({
        "content": "",
        "embeds": [embed],
        "components": component_rows(&view.buttons),
    })
}

pub fn component_rows(buttons: &[Btn]) -> Vec<serde_json::Value> {
    buttons
        .chunks(5)
        .map(|row| {
            serde_json::json!({
                "type": 1,
                "components": row.iter().map(|b| {
                    serde_json::json!({
                        "type": 2,
                        "style": b.style,
                        "label": b.label,
                        "custom_id": b.custom_id,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub fn discord_modal(modal: &Modal) -> serde_json::Value {
    let components: Vec<serde_json::Value> = modal
        .fields
        .iter()
        .map(|f| {
            serde_json::json!({
                "type": 1,
                "components": [{
                    "type": 4,
                    "custom_id": f.custom_id,
                    "label": f.label,
                    "style": if f.paragraph { 2 } else { 1 },
                    "required": f.required,
                    "placeholder": f.placeholder,
                }],
            })
        })
        .collect();
    serde_json::json!({
        "custom_id": modal.custom_id,
        "title": modal.title,
        "components": components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::CaseId;

    #[test]
    fn channel_name_sanitizes() {
        let id = CaseId::parse("case-cheat-1").unwrap();
        assert_eq!(case_channel_name(&id), "case-cheat-1");
        let id = CaseId::parse("Moderation/Slur").unwrap();
        assert_eq!(case_channel_name(&id), "case-moderation-slur");
    }
}
