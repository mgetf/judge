//! In-process Discord: OAuth, REST, and a guild UI.
//!
//! Point the judge at it with:
//!   DISCORD_AUTHORIZE_URL={base}/oauth2/authorize
//!   DISCORD_API_BASE={base}/api/v10
//!
//! The guild page is the product UX. Chat is a channel. Attachments and
//! buttons are native. The judge's live `see` message is edited in place.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, patch, post, put};
use axum::{Form, Json, Router};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpListener;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MockDiscordConfig {
    pub client_id: String,
    pub client_secret: String,
    pub guild_id: String,
    pub bot_token: String,
    pub users: Vec<MockUser>,
}

impl Default for MockDiscordConfig {
    fn default() -> Self {
        Self {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            guild_id: "000000000000000000".into(),
            bot_token: "mock-bot".into(),
            users: vec![
                MockUser {
                    id: "100".into(),
                    username: "tommyy".into(),
                    global_name: Some("tommyy".into()),
                    roles: vec!["111111111111111111".into()],
                },
                MockUser {
                    id: "101".into(),
                    username: "neptune".into(),
                    global_name: Some("neptune".into()),
                    roles: vec!["222222222222222222".into()],
                },
                MockUser {
                    id: "102".into(),
                    username: "abood".into(),
                    global_name: Some("abood".into()),
                    roles: vec!["222222222222222222".into()],
                },
                MockUser {
                    id: "200".into(),
                    username: "rando".into(),
                    global_name: None,
                    roles: vec![],
                },
            ],
        }
    }
}

#[derive(Clone)]
struct Channel {
    id: String,
    name: String,
    #[allow(dead_code)]
    topic: String,
    parent_id: Option<String>,
    overwrites: Value,
}

#[derive(Clone)]
struct Message {
    id: String,
    #[allow(dead_code)]
    channel_id: String,
    author: String,
    content: String,
    embeds: Vec<Value>,
    components: Vec<Value>,
    #[allow(dead_code)]
    attachments: Vec<Value>,
}

struct Inner {
    cfg: MockDiscordConfig,
    codes: HashMap<String, String>,
    channels: Vec<Channel>,
    messages: HashMap<String, Vec<Message>>,
    next_id: u64,
    interactions_url: Option<String>,
    pending_modal: Option<Value>,
    flash: Option<String>,
}

#[derive(Clone)]
struct MockState {
    inner: Arc<Mutex<Inner>>,
}

pub struct MockDiscord {
    pub addr: SocketAddr,
    pub base_url: String,
    inner: Arc<Mutex<Inner>>,
    handle: tokio::task::JoinHandle<()>,
}

impl MockDiscord {
    pub fn authorize_url(&self) -> String {
        format!("{}/oauth2/authorize", self.base_url)
    }

    pub fn api_base(&self) -> String {
        format!("{}/api/v10", self.base_url)
    }

    pub fn app_url(&self) -> String {
        format!("{}/app", self.base_url)
    }

    pub fn set_interactions_url(&self, url: String) {
        self.inner.lock().unwrap().interactions_url = Some(url);
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .channels
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn channel_overwrites(&self, name: &str) -> Option<Value> {
        self.inner
            .lock()
            .unwrap()
            .channels
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.overwrites.clone())
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

pub async fn serve_mock_discord(cfg: MockDiscordConfig) -> std::io::Result<MockDiscord> {
    let inner = Arc::new(Mutex::new(Inner {
        cfg,
        codes: HashMap::new(),
        channels: Vec::new(),
        messages: HashMap::new(),
        next_id: 5000,
        interactions_url: None,
        pending_modal: None,
        flash: None,
    }));
    let state = MockState {
        inner: inner.clone(),
    };
    let app = Router::new()
        .route("/", get(landing))
        .route("/app", get(guild_app))
        .route("/app/click", post(app_click))
        .route("/app/modal", post(app_modal))
        .route("/app/slash", post(app_slash))
        .route("/app/attach", post(app_attach))
        .route("/app/chat", post(app_chat))
        .route("/oauth2/authorize", get(authorize))
        .route("/oauth2/approve", get(approve))
        .route("/api/v10/oauth2/token", post(token))
        .route("/api/v10/users/@me", get(me))
        .route(
            "/api/v10/users/@me/guilds/{guild_id}/member",
            get(my_member),
        )
        .route("/api/v10/guilds/{guild_id}/members", get(list_members))
        .route("/api/v10/guilds/{guild_id}/channels", post(create_channel))
        .route("/api/v10/channels/{channel_id}", patch(modify_channel))
        .route(
            "/api/v10/channels/{channel_id}/permissions/{target_id}",
            put(put_overwrite),
        )
        .route(
            "/api/v10/applications/{app_id}/guilds/{guild_id}/commands",
            put(put_commands),
        )
        .route(
            "/api/v10/channels/{channel_id}/messages",
            get(list_channel_messages).post(create_message),
        )
        .route(
            "/api/v10/channels/{channel_id}/messages/{message_id}",
            patch(edit_message),
        )
        .route(
            "/api/v10/channels/{channel_id}/pins/{message_id}",
            put(pin_message),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(MockDiscord {
        addr,
        base_url: format!("http://{addr}"),
        inner,
        handle,
    })
}

fn next_id(inner: &mut Inner) -> String {
    inner.next_id += 1;
    inner.next_id.to_string()
}

fn require_bot(st: &MockState, headers: &axum::http::HeaderMap) -> bool {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let tok = st.inner.lock().unwrap().cfg.bot_token.clone();
    auth == format!("Bot {tok}")
}

async fn create_channel(
    State(st): State<MockState>,
    Path(guild_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let mut inner = st.inner.lock().unwrap();
    if guild_id != inner.cfg.guild_id {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Guild"})),
        )
            .into_response();
    }
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("channel")
        .to_string();
    let topic = body
        .get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if let Some(existing) = inner.channels.iter().find(|c| c.name == name) {
        return Json(serde_json::json!({"id": existing.id, "name": existing.name, "type": 0}))
            .into_response();
    }
    let id = next_id(&mut inner);
    let parent_id = body
        .get("parent_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let overwrites = body
        .get("permission_overwrites")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    inner.channels.push(Channel {
        id: id.clone(),
        name: name.clone(),
        topic,
        parent_id,
        overwrites,
    });
    inner.messages.entry(id.clone()).or_default();
    Json(serde_json::json!({"id": id, "name": name, "type": 0})).into_response()
}

async fn modify_channel(
    State(st): State<MockState>,
    Path(channel_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let mut inner = st.inner.lock().unwrap();
    let Some(ch) = inner.channels.iter_mut().find(|c| c.id == channel_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Channel"})),
        )
            .into_response();
    };
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        ch.name = name.to_string();
    }
    if let Some(parent) = body.get("parent_id").and_then(|v| v.as_str()) {
        ch.parent_id = Some(parent.to_string());
    }
    if let Some(over) = body.get("permission_overwrites") {
        ch.overwrites = over.clone();
    }
    Json(serde_json::json!({"id": ch.id, "name": ch.name, "type": 0})).into_response()
}

async fn put_overwrite(
    State(st): State<MockState>,
    Path((channel_id, target_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let mut inner = st.inner.lock().unwrap();
    let Some(ch) = inner.channels.iter_mut().find(|c| c.id == channel_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Channel"})),
        )
            .into_response();
    };
    let mut row = body;
    row["id"] = Value::String(target_id);
    match ch.overwrites {
        Value::Array(ref mut rows) => rows.push(row),
        _ => ch.overwrites = Value::Array(vec![row]),
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn list_channel_messages(
    State(st): State<MockState>,
    Path(channel_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let inner = st.inner.lock().unwrap();
    let msgs = inner.messages.get(&channel_id).cloned().unwrap_or_default();
    let rows: Vec<Value> = msgs
        .iter()
        .rev()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "channel_id": m.channel_id,
                "content": m.content,
                "author": { "id": "bot", "username": m.author },
                "embeds": m.embeds,
            })
        })
        .collect();
    Json(rows).into_response()
}

async fn put_commands(
    State(st): State<MockState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    Json(body).into_response()
}

async fn create_message(
    State(st): State<MockState>,
    Path(channel_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let mut inner = st.inner.lock().unwrap();
    if !inner.channels.iter().any(|c| c.id == channel_id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Channel"})),
        )
            .into_response();
    }
    let id = next_id(&mut inner);
    let msg = Message {
        id: id.clone(),
        channel_id: channel_id.clone(),
        author: "bot".into(),
        content: body
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        embeds: body
            .get("embeds")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        components: body
            .get("components")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        attachments: Vec::new(),
    };
    inner
        .messages
        .entry(channel_id.clone())
        .or_default()
        .push(msg);
    Json(serde_json::json!({"id": id, "channel_id": channel_id})).into_response()
}

async fn edit_message(
    State(st): State<MockState>,
    Path((channel_id, message_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let mut inner = st.inner.lock().unwrap();
    let Some(list) = inner.messages.get_mut(&channel_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Channel"})),
        )
            .into_response();
    };
    let Some(msg) = list.iter_mut().find(|m| m.id == message_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Message"})),
        )
            .into_response();
    };
    if let Some(c) = body.get("content").and_then(|v| v.as_str()) {
        msg.content = c.into();
    }
    if let Some(e) = body.get("embeds").and_then(|v| v.as_array()) {
        msg.embeds = e.clone();
    }
    if let Some(c) = body.get("components").and_then(|v| v.as_array()) {
        msg.components = c.clone();
    }
    Json(serde_json::json!({"id": message_id, "channel_id": channel_id})).into_response()
}

async fn pin_message() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct AppQ {
    as_user: Option<String>,
    channel: Option<String>,
}

fn user_from_query_or_cookie(
    st: &MockState,
    headers: &axum::http::HeaderMap,
    as_user: Option<&str>,
) -> Option<MockUser> {
    let id = as_user.map(str::to_string).or_else(|| cookie_user(headers));
    let id = id?;
    st.inner
        .lock()
        .unwrap()
        .cfg
        .users
        .iter()
        .find(|u| u.id == id || u.username == id)
        .cloned()
}

fn cookie_user(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("mock_user=") {
            return Some(v.to_string());
        }
    }
    None
}

async fn landing(State(st): State<MockState>) -> Markup {
    let users = st.inner.lock().unwrap().cfg.users.clone();
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                style { (PreEscaped(GUILD_STYLES)) }
            }
            body {
                main.pick {
                    h1 data-testid="mock-discord" { "Mock Discord" }
                    p.sub { "Pick a guild member." }
                    @for u in &users {
                        p {
                            a data-testid=(format!("continue-{}", u.username)) href=(format!("/app?as_user={}", u.id)) {
                                "Continue as " (u.username)
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn guild_app(
    State(st): State<MockState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<AppQ>,
) -> Response {
    let Some(user) = user_from_query_or_cookie(&st, &headers, q.as_user.as_deref()) else {
        return Redirect::to("/").into_response();
    };
    let page = {
        let inner = st.inner.lock().unwrap();
        render_guild(&inner, &user, q.channel.as_deref())
    };
    let cookie = format!("mock_user={}; Path=/; SameSite=Lax", user.id);
    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
        Html(page.into_string()),
    )
        .into_response()
}

use axum::response::Response;

#[derive(Deserialize)]
struct ClickForm {
    channel_id: String,
    message_id: String,
    custom_id: String,
    as_user: String,
}

async fn app_click(State(st): State<MockState>, Form(form): Form<ClickForm>) -> impl IntoResponse {
    let payload = {
        let inner = st.inner.lock().unwrap();
        let Some(user) = inner.cfg.users.iter().find(|u| u.id == form.as_user) else {
            return Redirect::to("/").into_response();
        };
        interaction_base(&inner, user, &form.channel_id)
            .set("type", 3)
            .set(
                "data",
                serde_json::json!({"custom_id": form.custom_id, "component_type": 2}),
            )
            .set(
                "message",
                serde_json::json!({"id": form.message_id, "channel_id": form.channel_id}),
            )
            .build()
    };
    apply_interaction(&st, payload).await;
    Redirect::to(&format!(
        "/app?as_user={}&channel={}",
        form.as_user, form.channel_id
    ))
    .into_response()
}

#[derive(Deserialize)]
struct ModalForm {
    custom_id: String,
    as_user: String,
    channel_id: String,
    #[serde(flatten)]
    fields: HashMap<String, String>,
}

async fn app_modal(State(st): State<MockState>, Form(form): Form<ModalForm>) -> impl IntoResponse {
    let payload = {
        let mut inner = st.inner.lock().unwrap();
        inner.pending_modal = None;
        let Some(user) = inner
            .cfg
            .users
            .iter()
            .find(|u| u.id == form.as_user)
            .cloned()
        else {
            return Redirect::to("/").into_response();
        };
        let mut rows = Vec::new();
        for (k, v) in &form.fields {
            if matches!(k.as_str(), "custom_id" | "as_user" | "channel_id") {
                continue;
            }
            rows.push(serde_json::json!({
                "type": 1,
                "components": [{ "type": 4, "custom_id": k, "value": v }]
            }));
        }
        interaction_base(&inner, &user, &form.channel_id)
            .set("type", 5)
            .set(
                "data",
                serde_json::json!({"custom_id": form.custom_id, "components": rows}),
            )
            .build()
    };
    apply_interaction(&st, payload).await;
    Redirect::to(&format!(
        "/app?as_user={}&channel={}",
        form.as_user, form.channel_id
    ))
    .into_response()
}

#[derive(Deserialize)]
struct SlashForm {
    name: String,
    as_user: String,
    channel_id: String,
    #[serde(flatten)]
    fields: HashMap<String, String>,
}

async fn app_slash(State(st): State<MockState>, Form(form): Form<SlashForm>) -> impl IntoResponse {
    let payload = {
        let inner = st.inner.lock().unwrap();
        let Some(user) = inner.cfg.users.iter().find(|u| u.id == form.as_user) else {
            return Redirect::to("/").into_response();
        };
        let mut options = Vec::new();
        let mut resolved_attachments = serde_json::Map::new();
        for (k, v) in &form.fields {
            if matches!(k.as_str(), "name" | "as_user" | "channel_id") || v.is_empty() {
                continue;
            }
            if k == "file" {
                let att_id = format!("att-{}", v.len());
                options.push(serde_json::json!({"name": "file", "type": 11, "value": att_id}));
                resolved_attachments.insert(
                    att_id,
                    serde_json::json!({
                        "id": "att",
                        "filename": v,
                        "url": format!("https://cdn.discord.invalid/{}", v),
                    }),
                );
            } else {
                options.push(serde_json::json!({"name": k, "type": 3, "value": v}));
            }
        }
        let mut data = serde_json::json!({"name": form.name, "options": options});
        if !resolved_attachments.is_empty() {
            data["resolved"] = serde_json::json!({"attachments": resolved_attachments});
        }
        interaction_base(&inner, user, &form.channel_id)
            .set("type", 2)
            .set("data", data)
            .build()
    };
    apply_interaction(&st, payload).await;
    Redirect::to(&format!(
        "/app?as_user={}&channel={}",
        form.as_user, form.channel_id
    ))
    .into_response()
}

#[derive(Deserialize)]
struct AttachForm {
    as_user: String,
    channel_id: String,
    filename: String,
    url: String,
    label: String,
}

async fn app_attach(
    State(st): State<MockState>,
    Form(form): Form<AttachForm>,
) -> impl IntoResponse {
    let payload = {
        let inner = st.inner.lock().unwrap();
        let Some(user) = inner.cfg.users.iter().find(|u| u.id == form.as_user) else {
            return Redirect::to("/").into_response();
        };
        let att_id = "att-1";
        interaction_base(&inner, user, &form.channel_id)
            .set("type", 2)
            .set(
                "data",
                serde_json::json!({
                    "name": "evidence",
                    "options": [
                        { "name": "file", "type": 11, "value": att_id },
                        { "name": "label", "type": 3, "value": form.label }
                    ],
                    "resolved": {
                        "attachments": {
                            (att_id): {
                                "id": att_id,
                                "filename": form.filename,
                                "url": form.url
                            }
                        }
                    }
                }),
            )
            .build()
    };
    apply_interaction(&st, payload).await;
    Redirect::to(&format!(
        "/app?as_user={}&channel={}",
        form.as_user, form.channel_id
    ))
    .into_response()
}

#[derive(Deserialize)]
struct ChatForm {
    as_user: String,
    channel_id: String,
    content: String,
}

async fn app_chat(State(st): State<MockState>, Form(form): Form<ChatForm>) -> impl IntoResponse {
    {
        let mut inner = st.inner.lock().unwrap();
        let id = next_id(&mut inner);
        let author = inner
            .cfg
            .users
            .iter()
            .find(|u| u.id == form.as_user)
            .map(|u| u.username.clone())
            .unwrap_or_else(|| form.as_user.clone());
        inner
            .messages
            .entry(form.channel_id.clone())
            .or_default()
            .push(Message {
                id,
                channel_id: form.channel_id.clone(),
                author,
                content: form.content,
                embeds: Vec::new(),
                components: Vec::new(),
                attachments: Vec::new(),
            });
    }
    Redirect::to(&format!(
        "/app?as_user={}&channel={}",
        form.as_user, form.channel_id
    ))
}

struct Payload(Value);

impl Payload {
    fn set(mut self, k: &str, v: impl Into<Value>) -> Self {
        self.0[k] = v.into();
        self
    }
    fn build(self) -> Value {
        self.0
    }
}

fn interaction_base(inner: &Inner, user: &MockUser, channel_id: &str) -> Payload {
    Payload(serde_json::json!({
        "id": "int-1",
        "token": "int-tok",
        "application_id": inner.cfg.client_id,
        "guild_id": inner.cfg.guild_id,
        "channel_id": channel_id,
        "member": {
            "roles": user.roles,
            "user": {
                "id": user.id,
                "username": user.username,
                "global_name": user.global_name,
            }
        }
    }))
}

async fn apply_interaction(st: &MockState, payload: Value) {
    let url = st.inner.lock().unwrap().interactions_url.clone();
    let Some(url) = url else {
        st.inner.lock().unwrap().flash = Some("judge interactions URL not set".into());
        return;
    };
    let resp = match reqwest::Client::new()
        .post(&url)
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            st.inner.lock().unwrap().flash = Some(e.to_string());
            return;
        }
    };
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            st.inner.lock().unwrap().flash = Some(e.to_string());
            return;
        }
    };
    let mut inner = st.inner.lock().unwrap();
    let ty = body.get("type").and_then(|v| v.as_u64()).unwrap_or(0);
    match ty {
        9 => {
            inner.pending_modal = body.get("data").cloned();
        }
        7 | 4 => {
            inner.pending_modal = None;
            if let Some(data) = body.get("data") {
                let ch = payload
                    .get("channel_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if ty == 7 {
                    if let Some(list) = inner.messages.get_mut(ch) {
                        let mid = payload.pointer("/message/id").and_then(|v| v.as_str());
                        let msg = if let Some(mid) = mid {
                            list.iter_mut().find(|m| m.id == mid)
                        } else {
                            list.iter_mut().rev().find(|m| m.author == "bot")
                        };
                        if let Some(msg) = msg {
                            if let Some(e) = data.get("embeds").and_then(|v| v.as_array()) {
                                msg.embeds = e.clone();
                            }
                            if let Some(c) = data.get("components").and_then(|v| v.as_array()) {
                                msg.components = c.clone();
                            }
                            if let Some(c) = data.get("content").and_then(|v| v.as_str()) {
                                msg.content = c.into();
                            }
                        }
                    }
                } else if data.get("flags").and_then(|v| v.as_u64()) != Some(64) {
                    let id = next_id(&mut inner);
                    inner.messages.entry(ch.into()).or_default().push(Message {
                        id,
                        channel_id: ch.into(),
                        author: "bot".into(),
                        content: data
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .into(),
                        embeds: data
                            .get("embeds")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default(),
                        components: data
                            .get("components")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default(),
                        attachments: Vec::new(),
                    });
                }
                if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                    if data.get("flags").and_then(|v| v.as_u64()) == Some(64) && !content.is_empty()
                    {
                        inner.flash = Some(content.to_string());
                    }
                }
            }
        }
        _ => {
            inner.flash = Some(format!("interaction type {ty}"));
        }
    }
}

const GUILD_STYLES: &str = r#"
html,body { margin:0; height:100%; background:#313338; color:#f2f3f5;
  font: 15px/1.45 "gg sans", "Helvetica Neue", sans-serif; }
.pick { max-width: 22rem; margin: 12vh auto; padding: 1.5rem 1.4rem;
  background:#2b2d31; border-radius: 8px; }
.pick a { display:block; padding: 0.55rem 0.7rem; margin: 0.35rem 0; background:#5865f2;
  color:#fff; text-decoration:none; border-radius: 4px; text-align:center; }
.app { display:grid; grid-template-columns: 16rem 1fr; height:100vh; }
nav { background:#2b2d31; padding: 0.8rem 0.6rem; overflow:auto; }
nav h2 { font-size: 0.72rem; letter-spacing: 0.08em; text-transform:uppercase; color:#949ba4; margin: 0 0.4rem 0.5rem; }
nav a { display:block; color:#dbdee1; text-decoration:none; padding: 0.35rem 0.5rem; border-radius: 4px; }
nav a.on, nav a:hover { background:#404249; }
main.chat { display:flex; flex-direction:column; min-width:0; }
header.bar { padding: 0.7rem 1rem; border-bottom:1px solid #3f4147; font-weight:600; }
.msgs { flex:1; overflow:auto; padding: 1rem; }
.msg { margin: 0 0 1rem; }
.msg .who { color:#00a8fc; font-weight:600; }
.embed { background:#2b2d31; border-left: 4px solid #5865f2; padding: 0.7rem 0.85rem; border-radius: 4px; margin-top: 0.35rem; }
.embed h3 { margin:0 0 0.35rem; }
.embed .field { margin: 0.45rem 0; color:#dbdee1; }
.embed .field b { display:block; color:#b5bac1; font-size: 0.78rem; }
.btns { display:flex; flex-wrap:wrap; gap: 0.35rem; margin-top: 0.5rem; }
.btns button, .composer button { background:#5865f2; color:#fff; border:0; border-radius: 4px; padding: 0.35rem 0.7rem; font: inherit; cursor:pointer; }
.btns button.s2 { background:#4e5058; }
.btns button.s4 { background:#da373c; }
.composer { padding: 0.8rem 1rem 1rem; border-top:1px solid #3f4147; }
.composer form { display:flex; gap: 0.4rem; flex-wrap:wrap; align-items:end; }
.composer input, .composer textarea, dialog input, dialog textarea, dialog select {
  background:#383a40; color:#f2f3f5; border:1px solid #1e1f22; border-radius: 4px;
  padding: 0.4rem 0.5rem; font: inherit;
}
.flash { background:#3f2f1f; color:#f0d5b4; padding: 0.5rem 0.8rem; margin: 0.5rem 1rem; border-radius: 4px; }
dialog { border:0; border-radius: 8px; background:#313338; color:#f2f3f5; padding: 1rem 1.1rem; width: min(28rem, 92vw); }
dialog::backdrop { background: rgba(0,0,0,0.55); }
.sub { color:#b5bac1; }
"#;

fn render_guild(inner: &Inner, user: &MockUser, channel: Option<&str>) -> Markup {
    let current = channel
        .and_then(|id| inner.channels.iter().find(|c| c.id == id || c.name == id))
        .or_else(|| {
            inner
                .channels
                .iter()
                .find(|c| c.name == "docket")
                .or_else(|| inner.channels.first())
        });
    let msgs = current
        .and_then(|c| inner.messages.get(&c.id))
        .cloned()
        .unwrap_or_default();
    let flash = inner.flash.clone();
    let modal = inner.pending_modal.clone();
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Court — Discord" }
                style { (PreEscaped(GUILD_STYLES)) }
            }
            body {
                div.app data-testid="guild" {
                    nav {
                        h2 { "Court" }
                        @for c in &inner.channels {
                            @let on = current.is_some_and(|cur| cur.id == c.id);
                            a href=(format!("/app?as_user={}&channel={}", user.id, c.id))
                              class=@if on { "on" }
                              data-testid=(format!("channel-{}", c.name)) {
                                "#" (c.name)
                            }
                        }
                    }
                    main.chat {
                        header.bar data-testid="channel-title" {
                            @if let Some(c) = current { "#" (c.name) } @else { "No channels yet" }
                            " · "
                            (user.username)
                        }
                        @if let Some(f) = flash {
                            p.flash data-testid="flash" { (f) }
                        }
                        div.msgs data-testid="messages" {
                            @for m in &msgs {
                                (render_message(m, user, current.map(|c| c.id.as_str()).unwrap_or("")))
                            }
                        }
                        @if let Some(c) = current {
                            div.composer {
                                form method="post" action="/app/chat" data-testid="chat" {
                                    input type="hidden" name="as_user" value=(user.id);
                                    input type="hidden" name="channel_id" value=(c.id);
                                    input name="content" data-testid="chat-input" placeholder="Message #" aria-label="message";
                                    button type="submit" data-testid="chat-send" { "Send" }
                                }
                                form method="post" action="/app/attach" data-testid="attach" {
                                    input type="hidden" name="as_user" value=(user.id);
                                    input type="hidden" name="channel_id" value=(c.id);
                                    input name="filename" data-testid="attach-filename" placeholder="filename" value="demo.dem";
                                    input name="url" data-testid="attach-url" placeholder="url" value="https://mge.tf/demos/abc";
                                    input name="label" data-testid="attach-label" placeholder="label" value="STV demo";
                                    button type="submit" data-testid="attach-submit" { "Attach as evidence" }
                                }
                                form method="post" action="/app/slash" data-testid="slash" {
                                    input type="hidden" name="as_user" value=(user.id);
                                    input type="hidden" name="channel_id" value=(c.id);
                                    input name="name" data-testid="slash-name" placeholder="command" value="docket";
                                    button type="submit" data-testid="slash-submit" { "/" }
                                }
                            }
                        }
                    }
                }
                @if let Some(m) = modal {
                    (render_pending_modal(&m, user, current.map(|c| c.id.as_str()).unwrap_or("")))
                    script { (PreEscaped("document.querySelector('dialog').showModal();")) }
                }
            }
        }
    }
}

fn render_message(m: &Message, user: &MockUser, channel_id: &str) -> Markup {
    html! {
        div.msg data-testid=(format!("msg-{}", m.id)) {
            div.who { (m.author) }
            @if !m.content.is_empty() { p { (m.content) } }
            @for e in &m.embeds {
                (render_embed(e))
            }
            @if !m.components.is_empty() {
                div.btns {
                    @for row in &m.components {
                        @if let Some(cs) = row.get("components").and_then(|v| v.as_array()) {
                            @for c in cs {
                                @if c.get("type").and_then(|v| v.as_u64()) == Some(2) {
                                    @let cid = c.get("custom_id").and_then(|v| v.as_str()).unwrap_or("");
                                    @let label = c.get("label").and_then(|v| v.as_str()).unwrap_or("btn");
                                    @let style = c.get("style").and_then(|v| v.as_u64()).unwrap_or(1);
                                    @let testid = testid_for_custom_id(cid, label);
                                    form method="post" action="/app/click" style="display:inline" {
                                        input type="hidden" name="as_user" value=(user.id);
                                        input type="hidden" name="channel_id" value=(channel_id);
                                        input type="hidden" name="message_id" value=(m.id);
                                        input type="hidden" name="custom_id" value=(cid);
                                        button type="submit" class=(format!("s{style}")) data-testid=(testid) { (label) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn testid_for_custom_id(custom_id: &str, label: &str) -> String {
    match crate::action::Wire::parse(custom_id) {
        Ok(crate::action::Wire::Ask { verb, .. }) => match verb.as_str() {
            "case" | "open_case" => "open-case".into(),
            "evidence" | "file_evidence" => "file-evidence".into(),
            "outcome" | "propose_outcome" => "propose-outcome".into(),
            "vote" => "cast-vote".into(),
            "respond" => "respond".into(),
            _ => verb,
        },
        Ok(crate::action::Wire::Go { verb, .. }) => match verb.as_str() {
            "notify" => "notify-submit".into(),
            "deliberate" => "deliberate-submit".into(),
            "closerequest" => "close-request".into(),
            "close" => "close-submit".into(),
            "cancelclose" => "close-cancel".into(),
            _ => verb,
        },
        _ => label.to_ascii_lowercase().replace(' ', "-"),
    }
}

fn render_embed(e: &Value) -> Markup {
    let title = e.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let desc = e.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let fields = e
        .get("fields")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let is_case = !title.is_empty() && title != "Docket";
    html! {
        div.embed data-testid=@if title == "Docket" { "docket-view" } @else { "case-view" } {
            h3 data-testid=@if is_case { "case-title" } @else { "docket-title" } { (title) }
            @if !desc.is_empty() {
                p data-testid=@if is_case { "case-brief" } @else { "lede" } { (desc) }
            }
            @for f in &fields {
                @let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                @let value = f.get("value").and_then(|v| v.as_str()).unwrap_or("");
                @let testid = embed_field_testid(name, value);
                div.field data-testid=(if testid == "verdict" { "winner" } else { testid }) {
                    @if !name.trim().is_empty() { b { (name) } }
                    @if testid == "verdict" {
                        span data-testid="verdict" { (value) }
                    } @else {
                        (value)
                    }
                    @if testid == "evidence-list" {
                        @for line in value.lines() {
                            @if let Some(id) = line.split_whitespace().next().and_then(|s| s.strip_prefix("• ")) {
                                span data-testid=(format!("evidence-{id}")) { "" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn embed_field_testid(name: &str, value: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("verdict") || value.to_ascii_lowercase().contains("winner:") {
        "verdict"
    } else if n.contains("phase")
        || matches!(
            value,
            "intake" | "noticed" | "deliberation" | "closed" | "lapsed" | "vacated"
        )
    {
        "case-phase"
    } else if n.contains("evidence") {
        "evidence-list"
    } else if n.contains("outcome") {
        "outcome-list"
    } else if n.contains("ballot") {
        "ballot-list"
    } else if n.contains("bench") {
        "bench"
    } else if n.contains("case") {
        "docket"
    } else {
        "embed-field"
    }
}

fn render_pending_modal(data: &Value, user: &MockUser, channel_id: &str) -> Markup {
    let custom_id = data.get("custom_id").and_then(|v| v.as_str()).unwrap_or("");
    let verb = crate::action::Wire::parse(custom_id)
        .map(|w| match w {
            crate::action::Wire::Do { verb, .. }
            | crate::action::Wire::Ask { verb, .. }
            | crate::action::Wire::Go { verb, .. } => verb,
        })
        .unwrap_or_default();
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Modal");
    let rows = data
        .get("components")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    html! {
        dialog open data-testid="modal" {
            form method="post" action="/app/modal" {
                h3 { (title) }
                input type="hidden" name="custom_id" value=(custom_id);
                input type="hidden" name="as_user" value=(user.id);
                input type="hidden" name="channel_id" value=(channel_id);
                @for row in &rows {
                    @if let Some(cs) = row.get("components").and_then(|v| v.as_array()) {
                        @for c in cs {
                            @let fid = c.get("custom_id").and_then(|v| v.as_str()).unwrap_or("");
                            @let label = c.get("label").and_then(|v| v.as_str()).unwrap_or(fid);
                            @let para = c.get("style").and_then(|v| v.as_u64()) == Some(2);
                            @let testid = modal_field_testid(&verb, fid);
                            label {
                                (label)
                                @if para {
                                    textarea name=(fid) data-testid=(testid) {}
                                } @else {
                                    input name=(fid) data-testid=(testid);
                                }
                            }
                        }
                    }
                }
                button type="submit" data-testid=(modal_submit_testid(custom_id)) { "Submit" }
            }
        }
    }
}

fn modal_field_testid(verb: &str, fid: &str) -> &'static str {
    match (verb, fid) {
        ("case" | "open_case", "id") => "case-id",
        (_, "kind") => "case-kind",
        (_, "hearing") => "case-hearing",
        (_, "subject") => "case-subject",
        (_, "target_case") => "case-target",
        (_, "brief") => "case-brief",
        ("evidence" | "file_evidence", "id") => "evidence-id",
        (_, "label") => "evidence-label",
        ("evidence" | "file_evidence", "body") => "evidence-body",
        ("outcome" | "propose_outcome", "id") => "outcome-id",
        ("outcome" | "propose_outcome", "body") => "outcome-body",
        ("respond", "body") => "response-body",
        (_, "enacts_policy") => "outcome-policy",
        (_, "outcome") => "vote-outcome",
        (_, "reason") => "vote-reason",
        _ => "modal-field",
    }
}

fn modal_submit_testid(custom_id: &str) -> &'static str {
    match crate::action::Wire::parse(custom_id) {
        Ok(crate::action::Wire::Do { verb, .. }) => match verb.as_str() {
            "case" | "open_case" => "open-case-submit",
            "evidence" | "file_evidence" => "evidence-submit",
            "outcome" | "propose_outcome" => "outcome-submit",
            "vote" => "vote-submit",
            "respond" => "response-submit",
            _ => "modal-submit",
        },
        _ => "modal-submit",
    }
}

// --- OAuth (unchanged contract) ---

#[derive(Deserialize)]
struct AuthorizeQ {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
}

async fn authorize(State(st): State<MockState>, Query(q): Query<AuthorizeQ>) -> impl IntoResponse {
    let cfg = st.inner.lock().unwrap().cfg.clone();
    if q.client_id != cfg.client_id {
        return (
            StatusCode::BAD_REQUEST,
            Html::<String>("unknown client_id".into()),
        )
            .into_response();
    }
    authorize_page(&cfg.users, &q.redirect_uri, q.state.as_deref()).into_response()
}

fn authorize_page(users: &[MockUser], redirect_uri: &str, state: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                style { (PreEscaped(GUILD_STYLES)) }
            }
            body {
                main.pick {
                    h1 data-testid="mock-discord" { "Mock Discord" }
                    p.sub { "Pick a guild member to continue." }
                    @for u in users {
                        @let href = format!(
                            "/oauth2/approve?user_id={}&redirect_uri={}&state={}",
                            urlencoding::encode(&u.id),
                            urlencoding::encode(redirect_uri),
                            urlencoding::encode(state.unwrap_or("")),
                        );
                        p {
                            a data-testid=(format!("continue-{}", u.username)) href=(href) {
                                "Continue as " (u.username)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct ApproveQ {
    user_id: String,
    redirect_uri: String,
    state: Option<String>,
}

async fn approve(State(st): State<MockState>, Query(q): Query<ApproveQ>) -> impl IntoResponse {
    let ok = st
        .inner
        .lock()
        .unwrap()
        .cfg
        .users
        .iter()
        .any(|u| u.id == q.user_id);
    if !ok {
        return (StatusCode::BAD_REQUEST, "unknown user").into_response();
    }
    let code = format!("code-{}", q.user_id);
    st.inner
        .lock()
        .unwrap()
        .codes
        .insert(code.clone(), q.user_id);
    let mut loc = format!("{}?code={}", q.redirect_uri, urlencoding::encode(&code));
    if let Some(state) = q.state {
        if !state.is_empty() {
            loc.push_str("&state=");
            loc.push_str(&urlencoding::encode(&state));
        }
    }
    Redirect::to(&loc).into_response()
}

#[derive(Deserialize)]
struct TokenForm {
    client_id: String,
    client_secret: String,
    code: String,
}

async fn token(State(st): State<MockState>, Form(form): Form<TokenForm>) -> impl IntoResponse {
    let inner = st.inner.lock().unwrap();
    if form.client_id != inner.cfg.client_id || form.client_secret != inner.cfg.client_secret {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"invalid_client"})),
        )
            .into_response();
    }
    let user_id = inner.codes.get(&form.code).cloned();
    let Some(user_id) = user_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_grant"})),
        )
            .into_response();
    };
    Json(serde_json::json!({
        "access_token": format!("tok-{user_id}"),
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "identify guilds.members.read",
    }))
    .into_response()
}

fn user_from_bearer(st: &MockState, headers: &axum::http::HeaderMap) -> Option<MockUser> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    let id = token.strip_prefix("tok-")?;
    st.inner
        .lock()
        .unwrap()
        .cfg
        .users
        .iter()
        .find(|u| u.id == id)
        .cloned()
}

async fn me(State(st): State<MockState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    let Some(u) = user_from_bearer(&st, &headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    };
    Json(serde_json::json!({
        "id": u.id,
        "username": u.username,
        "global_name": u.global_name,
        "discriminator": "0",
    }))
    .into_response()
}

async fn my_member(
    State(st): State<MockState>,
    Path(guild_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let cfg_guild = st.inner.lock().unwrap().cfg.guild_id.clone();
    if guild_id != cfg_guild {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Guild"})),
        )
            .into_response();
    }
    let Some(u) = user_from_bearer(&st, &headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    };
    Json(member_json(&u)).into_response()
}

async fn list_members(
    State(st): State<MockState>,
    Path(guild_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !require_bot(&st, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let inner = st.inner.lock().unwrap();
    if guild_id != inner.cfg.guild_id {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Guild"})),
        )
            .into_response();
    }
    let rows: Vec<Value> = inner.cfg.users.iter().map(member_json).collect();
    Json(rows).into_response()
}

fn member_json(u: &MockUser) -> Value {
    serde_json::json!({
        "user": {
            "id": u.id,
            "username": u.username,
            "global_name": u.global_name,
            "discriminator": "0",
        },
        "roles": u.roles,
        "nick": null,
    })
}
