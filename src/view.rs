//! `see`: project court state into a view. HTML and Discord both render this.
//! Buttons are the only controls; chat and attachments stay in Discord.

use std::collections::HashMap;

use crate::action::Wire;
use crate::ids::{CaseId, PrincipalId};
use crate::links::Cite;
use crate::types::DecisionKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Docket,
    Cite(Cite),
}
use crate::state::{Case, Principal, Verdict};
use crate::ticket::ArchivedMessage;
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
    pub href: Option<String>,
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
pub struct SelectOpt {
    pub value: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum FieldControl {
    Line,
    Paragraph,
    Select { options: Vec<SelectOpt> },
}

#[derive(Debug, Clone)]
pub struct ModalField {
    pub custom_id: String,
    pub label: String,
    pub testid: String,
    pub required: bool,
    pub placeholder: String,
    pub control: FieldControl,
}

#[derive(Debug, Clone)]
pub struct Modal {
    pub custom_id: String,
    pub title: String,
    pub testid: String,
    pub fields: Vec<ModalField>,
}

#[derive(Debug, Clone, Default)]
pub struct ModalCtx {
    pub outcomes: Vec<SelectOpt>,
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
    pub modal_ctx: ModalCtx,
    /// Frozen Discord chat. Present after the ticket is archived.
    pub archive: Option<Vec<ArchivedMessage>>,
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
                href: None,
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
                href: Some(Cite::Case { id: c.id.clone() }.path()),
            })
            .collect(),
        empty: "No cases yet.",
    });
    let mut buttons = Vec::new();
    if who.is_none() || who.is_some_and(|p| p.is_voting_seat()) {
        buttons.push(Btn {
            custom_id: Wire::custom_id("ask", "case", None),
            label: "Create".into(),
            testid: "open-case".into(),
            style: 1,
            opens_modal: true,
        });
    }
    View {
        target: Target::Docket,
        title: "Docket".into(),
        title_testid: "docket-title".into(),
        lede: "Open a ticket. The bench will see it. Chat and attachments stay in the channel."
            .into(),
        chips: Vec::new(),
        meta: Vec::new(),
        sections,
        verdict: None,
        buttons,
        notice: notice.map(str::to_string),
        channel_url,
        modal_ctx: ModalCtx::default(),
        archive: None,
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

    let scored = crate::scoring::tally(&case.outcomes, &case.ballots);
    let weights: std::collections::HashMap<String, u64> = scored
        .ordering
        .iter()
        .map(|(id, w)| (id.to_string(), *w))
        .collect();
    let outcomes = Section {
        heading: "Outcomes".into(),
        testid: "outcome-list".into(),
        items: case
            .outcomes
            .values()
            .map(|o| {
                let w = weights.get(&o.id.to_string()).copied().unwrap_or(0);
                SectionItem {
                    id: o.id.to_string(),
                    testid: format!("outcome-{}", o.id),
                    text: format!("{} — {} · {w}", o.id, o.body),
                    href: None,
                }
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
                href: None,
            })
            .collect(),
        empty: "No ballots yet.",
    };

    View {
        target: Target::Cite(Cite::Case {
            id: case.id.clone(),
        }),
        title: if case.brief.is_empty() {
            case.id.to_string()
        } else {
            case.brief.clone()
        },
        title_testid: "case-title".into(),
        lede: format!("#{}", case.id),
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
        sections: vec![outcomes, ballots],
        verdict: case.verdict.clone(),
        buttons: case_buttons(case, seated, on_bench, is_subject),
        notice: notice.map(str::to_string),
        channel_url,
        modal_ctx: ModalCtx {
            outcomes: case
                .outcomes
                .values()
                .map(|o| SelectOpt {
                    value: o.id.to_string(),
                    label: if o.body.len() > 100 {
                        format!("{}...", &o.body[..97])
                    } else {
                        o.body.clone()
                    },
                    description: o.id.to_string(),
                })
                .collect(),
        },
        archive: None,
    }
}

fn case_buttons(case: &Case, seated: bool, on_bench: bool, is_subject: bool) -> Vec<Btn> {
    let proposing = matches!(case.phase, Phase::Intake | Phase::Noticed);
    let cid = case.id.as_str();
    let mut out = Vec::new();
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
            custom_id: Wire::custom_id("go", "closerequest", Some(cid)),
            label: "Close".into(),
            testid: "close-request".into(),
            style: 4,
            opens_modal: false,
        });
    }
    out
}

pub fn modal_for(verb: &str, case: Option<&str>, ctx: &ModalCtx) -> Result<Modal, String> {
    let do_id = Wire::custom_id("do", verb, case);
    match verb {
        "case" | "open_case" => Ok(Modal {
            custom_id: do_id,
            title: "Create a ticket".into(),
            testid: "open-case-modal".into(),
            fields: vec![
                field(
                    "title",
                    "Title",
                    "case-title-field",
                    true,
                    FieldControl::Line,
                    "STV aimbot on mge_training",
                ),
                select_field("kind", "Kind", "case-kind", true, kind_options()),
                select_field("hearing", "Hearing", "case-hearing", true, hearing_options()),
                field(
                    "subject",
                    "Subject",
                    "case-subject",
                    false,
                    FieldControl::Line,
                    "Discord user id, if someone is on the hook",
                ),
                field(
                    "brief",
                    "Brief",
                    "case-brief",
                    false,
                    FieldControl::Paragraph,
                    "Optional. Defaults to the title.",
                ),
            ],
        }),
        "outcome" | "propose_outcome" => Ok(Modal {
            custom_id: do_id,
            title: "Propose outcome".into(),
            testid: "propose-outcome-modal".into(),
            fields: vec![
                field(
                    "id",
                    "Outcome id",
                    "outcome-id",
                    true,
                    FieldControl::Line,
                    "yes-enforce",
                ),
                field(
                    "body",
                    "Outcome",
                    "outcome-body",
                    true,
                    FieldControl::Paragraph,
                    "Permanent cheat ban",
                ),
                field(
                    "enacts_policy",
                    "Enacts policy",
                    "outcome-policy",
                    false,
                    FieldControl::Line,
                    "optional policy id",
                ),
            ],
        }),
        "vote" => {
            if ctx.outcomes.is_empty() {
                return Err("propose an outcome first".into());
            }
            Ok(Modal {
                custom_id: do_id,
                title: "Cast ballot".into(),
                testid: "cast-vote-modal".into(),
                fields: vec![
                    select_field("outcome", "Outcome", "vote-outcome", true, ctx.outcomes.clone()),
                    field(
                        "reason",
                        "Reason",
                        "vote-reason",
                        true,
                        FieldControl::Paragraph,
                        "Why this outcome",
                    ),
                ],
            })
        }
        "respond" => Ok(Modal {
            custom_id: do_id,
            title: "Response".into(),
            testid: "respond-modal".into(),
            fields: vec![field(
                "body",
                "Body",
                "response-body",
                true,
                FieldControl::Paragraph,
                "",
            )],
        }),
        other => Err(format!("no modal for {other}")),
    }
}

fn kind_options() -> Vec<SelectOpt> {
    DecisionKind::all()
        .into_iter()
        .map(|k| SelectOpt {
            value: k.as_str().into(),
            label: k.menu_label().into(),
            description: k.menu_hint().into(),
        })
        .collect()
}

fn hearing_options() -> Vec<SelectOpt> {
    Hearing::all()
        .into_iter()
        .map(|h| SelectOpt {
            value: h.as_str().into(),
            label: h.menu_label().into(),
            description: h.menu_hint().into(),
        })
        .collect()
}

fn field(
    custom_id: &str,
    label: &str,
    testid: &str,
    required: bool,
    control: FieldControl,
    placeholder: &str,
) -> ModalField {
    ModalField {
        custom_id: custom_id.into(),
        label: label.into(),
        testid: testid.into(),
        required,
        placeholder: placeholder.into(),
        control,
    }
}

fn select_field(
    custom_id: &str,
    label: &str,
    testid: &str,
    required: bool,
    options: Vec<SelectOpt>,
) -> ModalField {
    field(custom_id, label, testid, required, FieldControl::Select { options }, "")
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
    // Discord caps: title 256, description 4096. A long case brief rides in
    // the title verbatim, and one overlong brief poisons every live refresh.
    // Truncate on chars, never split a UTF-8 boundary.
    fn clip(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            return s.to_string();
        }
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
    let embed = serde_json::json!({
        "title": clip(&view.title, 256),
        "description": clip(&desc, 4096),
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
        .filter_map(|f| discord_modal_field(f))
        .collect();
    serde_json::json!({
        "custom_id": modal.custom_id,
        "title": modal.title,
        "components": components,
    })
}

fn discord_modal_field(f: &ModalField) -> Option<serde_json::Value> {
    let (style, placeholder) = match &f.control {
        FieldControl::Line => (1, f.placeholder.clone()),
        FieldControl::Paragraph => (2, f.placeholder.clone()),
        FieldControl::Select { options } => {
            let hint = options
                .iter()
                .map(|o| o.value.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (1, if hint.is_empty() { f.placeholder.clone() } else { hint })
        }
    };
    Some(serde_json::json!({
        "type": 1,
        "components": [{
            "type": 4,
            "custom_id": f.custom_id,
            "label": f.label,
            "style": style,
            "required": f.required,
            "placeholder": placeholder,
        }],
    }))
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

    #[test]
    fn embed_title_clamped_to_discord_limit() {
        // A 300-char brief rides the title verbatim; Discord 400s past 256
        // and poisons every live refresh. The payload must clamp, never split UTF-8.
        let view = View {
            target: Target::Cite(Cite::Case { id: crate::ids::CaseId::parse("case-x").unwrap() }),
            title: "ж".repeat(300),
            title_testid: "case-title".into(),
            lede: "#case-x".into(),
            meta: vec!["y".repeat(5000)],
            chips: vec![],
            sections: vec![],
            buttons: vec![],
            verdict: None,
            notice: None,
            channel_url: None,
            modal_ctx: ModalCtx::default(),
            archive: None,
        };
        let payload = discord_payload(&view);
        let embed = &payload["embeds"][0];
        let title = embed["title"].as_str().unwrap();
        let desc = embed["description"].as_str().unwrap();
        assert!(title.chars().count() <= 256, "title {} chars", title.chars().count());
        assert!(title.ends_with('…'));
        assert!(desc.chars().count() <= 4096, "desc {} chars", desc.chars().count());
        // Short titles pass through untouched.
        let mut short = view.clone();
        short.title = "Short brief".into();
        short.meta.clear();
        let embed2 = &discord_payload(&short)["embeds"][0];
        assert_eq!(embed2["title"], "Short brief");
    }

    #[test]
    fn create_modal_fits_discord_limit() {
        let modal = modal_for("case", None, &ModalCtx::default()).unwrap();
        assert!(
            modal.fields.len() <= 5,
            "Discord modals allow 5 text inputs, got {}",
            modal.fields.len()
        );
        let payload = discord_modal(&modal);
        assert_eq!(payload["components"].as_array().unwrap().len(), modal.fields.len());
    }
}
