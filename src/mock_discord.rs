//! In-process Discord OAuth + API stand-in.
//!
//! Point the judge at it with:
//!   DISCORD_AUTHORIZE_URL={base}/oauth2/authorize
//!   DISCORD_API_BASE={base}/api/v10

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde::{Deserialize, Serialize};
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
struct MockState {
    cfg: MockDiscordConfig,
    codes: Arc<Mutex<HashMap<String, String>>>,
}

pub struct MockDiscord {
    pub addr: SocketAddr,
    pub base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl MockDiscord {
    pub fn authorize_url(&self) -> String {
        format!("{}/oauth2/authorize", self.base_url)
    }

    pub fn api_base(&self) -> String {
        format!("{}/api/v10", self.base_url)
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

pub async fn serve_mock_discord(cfg: MockDiscordConfig) -> std::io::Result<MockDiscord> {
    let state = MockState {
        cfg,
        codes: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/oauth2/authorize", get(authorize))
        .route("/oauth2/approve", get(approve))
        .route("/api/v10/oauth2/token", post(token))
        .route("/api/v10/users/@me", get(me))
        .route(
            "/api/v10/users/@me/guilds/{guild_id}/member",
            get(my_member),
        )
        .route("/api/v10/guilds/{guild_id}/members", get(list_members))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(MockDiscord {
        addr,
        base_url: format!("http://{addr}"),
        handle,
    })
}

#[derive(Deserialize)]
struct AuthorizeQ {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
}

async fn authorize(State(st): State<MockState>, Query(q): Query<AuthorizeQ>) -> impl IntoResponse {
    if q.client_id != st.cfg.client_id {
        return (
            StatusCode::BAD_REQUEST,
            Html::<String>("unknown client_id".into()),
        )
            .into_response();
    }
    authorize_page(&st.cfg.users, &q.redirect_uri, q.state.as_deref()).into_response()
}

const MOCK_STYLES: &str = r#"
html,body { margin:0; min-height:100%; background:#313338; color:#f2f3f5;
  font: 16px/1.45 "gg sans", "Helvetica Neue", sans-serif; }
main { max-width: 22rem; margin: 12vh auto; padding: 1.5rem 1.4rem 1.7rem;
  background:#2b2d31; border-radius: 8px; }
h1 { font-size: 1.2rem; margin: 0 0 0.35rem; }
.sub { color:#b5bac1; font-size: 0.9rem; margin: 0 0 1.1rem; }
a { display:block; padding: 0.55rem 0.7rem; margin: 0.35rem 0; background:#5865f2;
  color:#fff; text-decoration:none; border-radius: 4px; text-align:center; }
a:hover { background:#4752c4; }
"#;

fn authorize_page(users: &[MockUser], redirect_uri: &str, state: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                style { (PreEscaped(MOCK_STYLES)) }
            }
            body {
                main {
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
    if !st.cfg.users.iter().any(|u| u.id == q.user_id) {
        return (StatusCode::BAD_REQUEST, "unknown user").into_response();
    }
    let code = format!("code-{}", q.user_id);
    st.codes.lock().unwrap().insert(code.clone(), q.user_id);
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
    if form.client_id != st.cfg.client_id || form.client_secret != st.cfg.client_secret {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"invalid_client"})),
        )
            .into_response();
    }
    let user_id = st.codes.lock().unwrap().get(&form.code).cloned();
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
    st.cfg.users.iter().find(|u| u.id == id).cloned()
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
    if guild_id != st.cfg.guild_id {
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
    if guild_id != st.cfg.guild_id {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"message":"Unknown Guild"})),
        )
            .into_response();
    }
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bot {}", st.cfg.bot_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message":"401"})),
        )
            .into_response();
    }
    let rows: Vec<serde_json::Value> = st.cfg.users.iter().map(member_json).collect();
    Json(rows).into_response()
}

fn member_json(u: &MockUser) -> serde_json::Value {
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
