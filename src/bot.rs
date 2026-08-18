//! Discord is the court. Each case is a Ticket Tool-shaped ticket: private
//! channel, pinned live `see`, two-step close, closed category, transcript.
//! Chat and attachments stay in Discord.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::{parse_action, Action, Wire};
use crate::app::{commit, AppError, AppState};
use crate::clock::now_ms;
use crate::events::Event;
use crate::ids::{CaseId, PrincipalId};
use crate::state::Principal;
use crate::ticket::{
    close_ask, closed_channel_name, closed_overwrites, opened_overwrites, transcript_html, TALK,
    VIEW_CHANNEL,
};
use crate::view::{
    case_channel_name, discord_modal, discord_payload, modal_for, see_case, see_docket, Target,
    View,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscordBindings {
    pub docket_channel_id: Option<String>,
    pub docket_message_id: Option<String>,
    pub cases: BTreeMap<String, CaseBind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseBind {
    pub channel_id: String,
    pub view_message_id: String,
}

impl DiscordBindings {
    pub fn load(path: &PathBuf) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::BadRequest(format!("bindings dir: {e}")))?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::BadRequest(format!("bindings json: {e}")))?;
        std::fs::write(path, s)
            .map_err(|e| AppError::BadRequest(format!("bindings write: {e}")))?;
        Ok(())
    }

    pub fn case_for_channel(&self, channel_id: &str) -> Option<CaseId> {
        self.cases.iter().find_map(|(id, b)| {
            if b.channel_id == channel_id {
                CaseId::parse(id).ok()
            } else {
                None
            }
        })
    }
}

pub fn guild_commands() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "case",
            "description": "Open a case",
            "options": [
                { "name": "id", "description": "Case id", "type": 3, "required": true },
                { "name": "brief", "description": "Brief", "type": 3, "required": true },
                { "name": "kind", "description": "record/routine/personnel/policy/constitutional/appeal", "type": 3, "required": false },
                { "name": "hearing", "description": "none or required", "type": 3, "required": false },
                { "name": "subject", "description": "Subject id", "type": 3, "required": false },
                { "name": "target", "description": "Appeal target", "type": 3, "required": false }
            ]
        }),
        serde_json::json!({
            "name": "evidence",
            "description": "File an attachment as evidence in this case channel",
            "options": [
                { "name": "file", "description": "Attachment", "type": 11, "required": true },
                { "name": "label", "description": "Label", "type": 3, "required": true },
                { "name": "id", "description": "Evidence id (defaults to filename)", "type": 3, "required": false }
            ]
        }),
        serde_json::json!({
            "name": "outcome",
            "description": "Propose an outcome",
            "options": [
                { "name": "id", "description": "Outcome id", "type": 3, "required": true },
                { "name": "body", "description": "Body", "type": 3, "required": true },
                { "name": "policy", "description": "Enacts policy", "type": 3, "required": false }
            ]
        }),
        serde_json::json!({
            "name": "vote",
            "description": "Cast a ballot",
            "options": [
                { "name": "outcome", "description": "Outcome id", "type": 3, "required": true },
                { "name": "reason", "description": "Reason", "type": 3, "required": true }
            ]
        }),
        serde_json::json!({
            "name": "notify",
            "description": "Notify the subject (hearing required)"
        }),
        serde_json::json!({
            "name": "deliberate",
            "description": "Open deliberation"
        }),
        serde_json::json!({
            "name": "close",
            "description": "Ask to close this ticket (Ticket Tool two-step close)"
        }),
        serde_json::json!({
            "name": "add",
            "description": "Add a user to this ticket",
            "options": [
                { "name": "user", "description": "User to add", "type": 6, "required": true }
            ]
        }),
        serde_json::json!({
            "name": "remove",
            "description": "Remove a user from this ticket",
            "options": [
                { "name": "user", "description": "User to remove", "type": 6, "required": true }
            ]
        }),
        serde_json::json!({
            "name": "transcript",
            "description": "Save an HTML transcript of this ticket"
        }),
        serde_json::json!({
            "name": "docket",
            "description": "Show the live docket"
        }),
    ]
}

pub async fn ensure_ux(state: &AppState) -> Result<(), AppError> {
    if state.discord.env().bot_token.is_none() {
        return Ok(());
    }
    let guild = &state.config.guild_id;
    if let Err(e) = state
        .discord
        .overwrite_guild_commands(guild, &guild_commands())
        .await
    {
        tracing::warn!("register commands: {e}");
    }
    refresh_docket(state).await
}

pub async fn after_commit(state: &AppState, ev: &Event) -> Result<(), AppError> {
    if state.discord.env().bot_token.is_none() {
        return Ok(());
    }
    match ev {
        Event::CaseOpened { id, brief, .. } => {
            ensure_case_channel(state, id, brief).await?;
            refresh_case(state, id).await?;
            refresh_docket(state).await?;
        }
        Event::EvidenceFiled { case, .. }
        | Event::SubjectNotified { case, .. }
        | Event::ResponseFiled { case, .. }
        | Event::OutcomeProposed { case, .. }
        | Event::DeliberationOpened { case, .. }
        | Event::VoteCast { case, .. }
        | Event::ClerkNoteFiled { case, .. }
        | Event::PolicyCited { case, .. }
        | Event::Recused { case, .. }
        | Event::RecusalLifted { case, .. } => {
            refresh_case(state, case).await?;
            refresh_docket(state).await?;
        }
        Event::CaseClosed { case, .. } => {
            refresh_case(state, case).await?;
            refresh_docket(state).await?;
            archive_ticket(state, case).await?;
        }
        Event::RosterSynced { .. } | Event::PrincipalSeen { .. } => {
            refresh_docket(state).await?;
        }
    }
    Ok(())
}

async fn ensure_case_channel(state: &AppState, id: &CaseId, brief: &str) -> Result<(), AppError> {
    {
        let b = state.bindings.read().await;
        if b.cases.contains_key(id.as_str()) {
            return Ok(());
        }
    }
    let name = case_channel_name(id);
    let parent = state.config.court_category_id.as_deref();
    let (overwrites, ping) = {
        let g = state.gov.read().await;
        let case = g.cases.get(id).ok_or(AppError::NotFound)?;
        let mut users = vec![case.opened_by.to_string()];
        if let Some(s) = &case.subject {
            if s.as_str() != case.opened_by.as_str() {
                users.push(s.to_string());
            }
        }
        let roles: Vec<String> = state.config.roles.keys().cloned().collect();
        let overwrites = opened_overwrites(&state.config.guild_id, &roles, &users);
        let ping = format!(
            "<@{}> opened this ticket.{}",
            case.opened_by,
            case.subject
                .as_ref()
                .map(|s| format!(" Subject: <@{s}>"))
                .unwrap_or_default()
        );
        (overwrites, ping)
    };
    let ch = state
        .discord
        .create_guild_channel(
            &state.config.guild_id,
            &name,
            Some(brief),
            parent,
            Some(&overwrites),
        )
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let view = current_case_view(state, id, None).await?;
    let mut payload = discord_payload(&view);
    payload["content"] = serde_json::Value::String(ping);
    let msg = state
        .discord
        .create_message(&ch.id, &payload)
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let _ = state.discord.pin_message(&ch.id, &msg.id).await;
    let mut b = state.bindings.write().await;
    b.cases.insert(
        id.to_string(),
        CaseBind {
            channel_id: ch.id,
            view_message_id: msg.id,
        },
    );
    b.save(&state.bindings_path)?;
    Ok(())
}

async fn refresh_docket(state: &AppState) -> Result<(), AppError> {
    let view = current_docket_view(state, None).await;
    let payload = discord_payload(&view);
    let mut b = state.bindings.write().await;
    if let (Some(ch), Some(mid)) = (b.docket_channel_id.clone(), b.docket_message_id.clone()) {
        drop(b);
        if state
            .discord
            .edit_message(&ch, &mid, &payload)
            .await
            .is_ok()
        {
            return Ok(());
        }
        b = state.bindings.write().await;
    }
    let ch_id = if let Some(id) = b.docket_channel_id.clone() {
        id
    } else if let Some(id) = state.config.docket_channel_id.clone() {
        b.docket_channel_id = Some(id.clone());
        id
    } else {
        let ch = state
            .discord
            .create_guild_channel(
                &state.config.guild_id,
                "docket",
                Some("Live court docket"),
                state.config.court_category_id.as_deref(),
                None,
            )
            .await
            .map_err(|e| AppError::Discord(e.to_string()))?;
        b.docket_channel_id = Some(ch.id.clone());
        ch.id
    };
    let msg = state
        .discord
        .create_message(&ch_id, &payload)
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let _ = state.discord.pin_message(&ch_id, &msg.id).await;
    b.docket_message_id = Some(msg.id);
    b.save(&state.bindings_path)?;
    Ok(())
}

async fn refresh_case(state: &AppState, id: &CaseId) -> Result<(), AppError> {
    let view = current_case_view(state, id, None).await?;
    let payload = discord_payload(&view);
    let bind = {
        let b = state.bindings.read().await;
        b.cases.get(id.as_str()).cloned()
    };
    let Some(bind) = bind else {
        return Ok(());
    };
    if state
        .discord
        .edit_message(&bind.channel_id, &bind.view_message_id, &payload)
        .await
        .is_err()
    {
        let msg = state
            .discord
            .create_message(&bind.channel_id, &payload)
            .await
            .map_err(|e| AppError::Discord(e.to_string()))?;
        let mut b = state.bindings.write().await;
        if let Some(row) = b.cases.get_mut(id.as_str()) {
            row.view_message_id = msg.id;
        }
        b.save(&state.bindings_path)?;
    }
    Ok(())
}

pub async fn current_docket_view(state: &AppState, notice: Option<&str>) -> View {
    let g = state.gov.read().await;
    let mut cases: Vec<_> = g.cases.values().collect();
    cases.sort_by(|a, b| b.opened_ts.cmp(&a.opened_ts));
    let mut bench: Vec<_> = g
        .principals
        .values()
        .filter(|p| p.seat.is_some())
        .cloned()
        .collect();
    bench.sort_by(|a, b| a.id.cmp(&b.id));
    let channel_url = {
        let b = state.bindings.read().await;
        b.docket_channel_id
            .as_ref()
            .map(|ch| channel_link(&state.config.guild_id, ch))
    };
    see_docket(None, &cases, &bench, notice, channel_url)
}

pub async fn current_case_view(
    state: &AppState,
    id: &CaseId,
    notice: Option<&str>,
) -> Result<View, AppError> {
    let g = state.gov.read().await;
    let case = g.cases.get(id).ok_or(AppError::NotFound)?;
    let channel_url = {
        let b = state.bindings.read().await;
        b.cases
            .get(id.as_str())
            .map(|c| channel_link(&state.config.guild_id, &c.channel_id))
    };
    Ok(see_case(None, case, &g.principals, notice, channel_url))
}

pub fn channel_link(guild_id: &str, channel_id: &str) -> String {
    format!("https://discord.com/channels/{guild_id}/{channel_id}")
}

pub fn verify_signature(public_key_hex: &str, timestamp: &str, body: &[u8], sig_hex: &str) -> bool {
    let Ok(pk_bytes) = hex::decode(public_key_hex) else {
        return false;
    };
    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };
    let Ok(pk_arr) = <[u8; 32]>::try_from(pk_bytes.as_slice()) else {
        return false;
    };
    let Ok(sig_arr) = <[u8; 64]>::try_from(sig_bytes.as_slice()) else {
        return false;
    };
    let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(&pk_arr) else {
        return false;
    };
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
    let mut msg = timestamp.as_bytes().to_vec();
    msg.extend_from_slice(body);
    key.verify_strict(&msg, &sig).is_ok()
}

pub async fn handle_interaction(state: &AppState, body: &Value) -> Result<Value, AppError> {
    let ty = body.get("type").and_then(|v| v.as_u64()).unwrap_or(0);
    if ty == 1 {
        return Ok(serde_json::json!({ "type": 1 }));
    }
    let who = interaction_principal(state, body).await?;
    match ty {
        2 => handle_command(state, &who, body).await,
        3 => handle_component(state, &who, body).await,
        5 => handle_modal(state, &who, body).await,
        _ => Ok(ephemeral("unsupported interaction")),
    }
}

async fn interaction_principal(state: &AppState, body: &Value) -> Result<Principal, AppError> {
    let user = body
        .pointer("/member/user")
        .or_else(|| body.get("user"))
        .ok_or_else(|| AppError::BadRequest("interaction missing user".into()))?;
    let id = user
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("interaction missing user id".into()))?;
    let pid = PrincipalId::parse(id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let name = user
        .get("global_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| user.get("username").and_then(|v| v.as_str()))
        .unwrap_or(id)
        .to_string();
    commit(
        state,
        Event::PrincipalSeen {
            ts: now_ms(),
            id: pid.clone(),
            display_name: name,
        },
    )
    .await?;
    let g = state.gov.read().await;
    g.principals
        .get(&pid)
        .cloned()
        .ok_or(AppError::Unauthorized)
}

async fn handle_component(
    state: &AppState,
    who: &Principal,
    body: &Value,
) -> Result<Value, AppError> {
    let custom_id = body
        .pointer("/data/custom_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing custom_id".into()))?;
    match Wire::parse(custom_id).map_err(AppError::BadRequest)? {
        Wire::Ask { verb, case } => {
            let modal = modal_for(&verb, case.as_deref()).map_err(AppError::BadRequest)?;
            Ok(serde_json::json!({
                "type": 9,
                "data": discord_modal(&modal),
            }))
        }
        Wire::Go { verb, case } => {
            if verb == "closerequest" {
                return Ok(close_ask(case.as_deref()));
            }
            if verb == "cancelclose" {
                return Ok(serde_json::json!({
                    "type": 7,
                    "data": { "content": "Close cancelled.", "components": [] }
                }));
            }
            let action = action_from_verb(&verb, case.as_deref(), &BTreeMap::new())?;
            if verb == "close" {
                commit(state, action.into_event(who.id.clone(), now_ms())).await?;
                return Ok(serde_json::json!({
                    "type": 7,
                    "data": { "content": "Ticket closed.", "components": [] }
                }));
            }
            eval_and_ack(state, who, action, body, 7).await
        }
        Wire::Do { .. } => Ok(ephemeral("use the modal submit")),
    }
}

async fn handle_modal(state: &AppState, who: &Principal, body: &Value) -> Result<Value, AppError> {
    let custom_id = body
        .pointer("/data/custom_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing custom_id".into()))?;
    let wire = Wire::parse(custom_id).map_err(AppError::BadRequest)?;
    let (verb, case) = match wire {
        Wire::Do { verb, case } | Wire::Go { verb, case } | Wire::Ask { verb, case } => {
            (verb, case)
        }
    };
    let mut fields = modal_values(body);
    if let Some(c) = case {
        fields.entry("case".into()).or_insert(c);
    } else if let Some(c) = case_from_channel(state, body).await {
        fields.entry("case".into()).or_insert(c);
    }
    let action = parse_action(&verb, &fields).map_err(AppError::BadRequest)?;
    eval_and_ack(state, who, action, body, 7).await
}

async fn handle_command(
    state: &AppState,
    who: &Principal,
    body: &Value,
) -> Result<Value, AppError> {
    let name = body
        .pointer("/data/name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if name == "docket" {
        let view = current_docket_view(state, None).await;
        let mut data = discord_payload(&view);
        data["flags"] = serde_json::json!(64);
        return Ok(serde_json::json!({ "type": 4, "data": data }));
    }
    if name == "close" {
        let case = case_from_channel(state, body).await;
        return Ok(close_ask(case.as_deref()));
    }
    if name == "add" || name == "remove" {
        return ticket_member(state, body, name == "add").await;
    }
    if name == "transcript" {
        if let Some(id) = case_from_channel(state, body).await {
            if let Ok(cid) = CaseId::parse(&id) {
                let url = save_transcript(state, &cid).await?;
                return Ok(ephemeral(&format!("Transcript: {url}")));
            }
        }
        return Ok(ephemeral("run this in a ticket"));
    }
    let mut fields = command_values(body);
    if !fields.contains_key("case") {
        if let Some(c) = case_from_channel(state, body).await {
            fields.insert("case".into(), c);
        }
    }
    if name == "evidence" {
        if let Some((id, url, filename)) = resolved_attachment(body) {
            fields.entry("body".into()).or_insert(url);
            fields
                .entry("id".into())
                .or_insert_with(|| slug_id(&filename).or_else(|| slug_id(&id)).unwrap_or(id));
            if !fields.contains_key("label") {
                fields.insert("label".into(), filename);
            }
        }
    }
    if name == "case" {
        if let Some(t) = fields.remove("target") {
            fields.insert("target_case".into(), t);
        }
    }
    if name == "outcome" {
        if let Some(p) = fields.remove("policy") {
            fields.insert("enacts_policy".into(), p);
        }
    }
    let action = parse_action(name, &fields).map_err(AppError::BadRequest)?;
    eval_and_ack(state, who, action, body, 4).await
}

fn action_from_verb(
    verb: &str,
    case: Option<&str>,
    fields: &BTreeMap<String, String>,
) -> Result<Action, AppError> {
    let mut merged = fields.clone();
    if let Some(c) = case {
        merged.insert("case".into(), c.to_string());
    }
    parse_action(verb, &merged).map_err(AppError::BadRequest)
}

async fn eval_and_ack(
    state: &AppState,
    who: &Principal,
    action: Action,
    body: &Value,
    response_type: u64,
) -> Result<Value, AppError> {
    commit(state, action.into_event(who.id.clone(), now_ms())).await?;
    let view = view_for_interaction_channel(state, body).await;
    let mut data = discord_payload(&view);
    if response_type == 4 {
        data["flags"] = serde_json::json!(64);
    }
    Ok(serde_json::json!({ "type": response_type, "data": data }))
}

async fn view_for_interaction_channel(state: &AppState, body: &Value) -> View {
    let ch = body
        .get("channel_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let b = state.bindings.read().await;
    if b.docket_channel_id.as_deref() == Some(ch) {
        drop(b);
        return current_docket_view(state, None).await;
    }
    let case_id = b.case_for_channel(ch);
    drop(b);
    if let Some(id) = case_id {
        if let Ok(v) = current_case_view(state, &id, None).await {
            return v;
        }
    }
    current_docket_view(state, None).await
}

async fn case_from_channel(state: &AppState, body: &Value) -> Option<String> {
    let ch = body.get("channel_id").and_then(|v| v.as_str())?;
    let b = state.bindings.read().await;
    b.case_for_channel(ch).map(|c| c.to_string())
}

fn modal_values(body: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(rows) = body.pointer("/data/components").and_then(|v| v.as_array()) else {
        return out;
    };
    for row in rows {
        let Some(inner) = row.get("components").and_then(|v| v.as_array()) else {
            continue;
        };
        for c in inner {
            let Some(id) = c.get("custom_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let val = c.get("value").and_then(|v| v.as_str()).unwrap_or("");
            out.insert(id.to_string(), val.to_string());
        }
    }
    out
}

fn command_values(body: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(opts) = body.pointer("/data/options").and_then(|v| v.as_array()) else {
        return out;
    };
    for o in opts {
        let Some(name) = o.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(s) = o.get("value").and_then(|v| v.as_str()) {
            out.insert(name.to_string(), s.to_string());
        } else if let Some(n) = o.get("value").and_then(|v| v.as_i64()) {
            out.insert(name.to_string(), n.to_string());
        }
    }
    out
}

fn resolved_attachment(body: &Value) -> Option<(String, String, String)> {
    let file_id = body
        .pointer("/data/options")
        .and_then(|v| v.as_array())
        .and_then(|opts| {
            opts.iter().find_map(|o| {
                if o.get("name").and_then(|v| v.as_str()) == Some("file") {
                    o.get("value").and_then(|v| v.as_str()).map(str::to_string)
                } else {
                    None
                }
            })
        })?;
    let att = body.pointer(&format!("/data/resolved/attachments/{file_id}"))?;
    let url = att.get("url").and_then(|v| v.as_str())?.to_string();
    let filename = att
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("file")
        .to_string();
    Some((file_id, url, filename))
}

fn slug_id(s: &str) -> Option<String> {
    let slug: String = s
        .chars()
        .map(|c| match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            '.' => '-',
            _ => '-',
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        None
    } else {
        crate::ids::EvidenceId::parse(&slug)
            .ok()
            .map(|e| e.to_string())
    }
}

async fn archive_ticket(state: &AppState, id: &CaseId) -> Result<(), AppError> {
    let bind = {
        let b = state.bindings.read().await;
        b.cases.get(id.as_str()).cloned()
    };
    let Some(bind) = bind else {
        return Ok(());
    };
    let (users, roles) = ticket_people(state, id).await;
    let overwrites = closed_overwrites(&state.config.guild_id, &roles, &users);
    let new_name = closed_channel_name(&case_channel_name(id));
    if let Err(e) = state
        .discord
        .modify_channel(
            &bind.channel_id,
            Some(&new_name),
            state.config.closed_category_id.as_deref(),
            Some(&overwrites),
        )
        .await
    {
        tracing::warn!("archive ticket channel: {e}");
    }
    match save_transcript(state, id).await {
        Ok(url) => {
            let note = serde_json::json!({ "content": format!("Transcript: {url}") });
            if let Some(ch) = &state.config.transcript_channel_id {
                let _ = state.discord.create_message(ch, &note).await;
            }
            let _ = state.discord.create_message(&bind.channel_id, &note).await;
        }
        Err(e) => tracing::warn!("transcript: {e}"),
    }
    Ok(())
}

async fn ticket_people(state: &AppState, id: &CaseId) -> (Vec<String>, Vec<String>) {
    let g = state.gov.read().await;
    let mut users = Vec::new();
    if let Some(c) = g.cases.get(id) {
        users.push(c.opened_by.to_string());
        if let Some(s) = &c.subject {
            users.push(s.to_string());
        }
    }
    let roles: Vec<String> = state.config.roles.keys().cloned().collect();
    (users, roles)
}

pub fn transcript_file(state: &AppState, id: &CaseId) -> PathBuf {
    state
        .bindings_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("transcripts")
        .join(format!("{id}.html"))
}

pub async fn save_transcript(state: &AppState, id: &CaseId) -> Result<String, AppError> {
    let bind = {
        let b = state.bindings.read().await;
        b.cases.get(id.as_str()).cloned()
    }
    .ok_or(AppError::NotFound)?;
    let msgs = state
        .discord
        .list_messages(&bind.channel_id, 100)
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let html = transcript_html(id.as_str(), &msgs);
    let path = transcript_file(state, id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::BadRequest(format!("transcript dir: {e}")))?;
    }
    std::fs::write(&path, html)
        .map_err(|e| AppError::BadRequest(format!("transcript write: {e}")))?;
    Ok(format!(
        "{}/cases/{id}/transcript",
        state.public_url.trim_end_matches('/')
    ))
}

async fn ticket_member(state: &AppState, body: &Value, add: bool) -> Result<Value, AppError> {
    let ch = body
        .get("channel_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing channel".into()))?;
    let user_id = body
        .pointer("/data/options")
        .and_then(|v| v.as_array())
        .and_then(|opts| {
            opts.iter().find_map(|o| {
                if o.get("name").and_then(|v| v.as_str()) != Some("user") {
                    return None;
                }
                o.get("value")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .or_else(|| {
                        o.get("value")
                            .and_then(|v| v.as_u64())
                            .map(|n| n.to_string())
                    })
            })
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing user".into()))?;
    if add {
        state
            .discord
            .edit_overwrite(ch, &user_id, 1, TALK, 0)
            .await
            .map_err(|e| AppError::Discord(e.to_string()))?;
        Ok(ephemeral(&format!("Added <@{user_id}> to this ticket.")))
    } else {
        state
            .discord
            .edit_overwrite(ch, &user_id, 1, 0, VIEW_CHANNEL)
            .await
            .map_err(|e| AppError::Discord(e.to_string()))?;
        Ok(ephemeral(&format!(
            "Removed <@{user_id}> from this ticket."
        )))
    }
}

fn ephemeral(msg: &str) -> Value {
    serde_json::json!({
        "type": 4,
        "data": { "content": msg, "flags": 64 }
    })
}

pub fn see_path(target: &Target) -> String {
    match target {
        Target::Docket => "/".into(),
        Target::Cite(c) => c.path(),
    }
}
