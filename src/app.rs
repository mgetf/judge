use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::blob::BlobStore;
use crate::clock::now_ms;
use crate::config::CourtConfig;
use crate::discord::{roster_from_members, DiscordClient, DiscordEnv};
use crate::event_log::EventLog;
use crate::events::Event;
use crate::http::router;
use crate::reducer::GovState;
use crate::types::Reject;

#[derive(Clone)]
pub struct AppState {
    pub gov: Arc<RwLock<GovState>>,
    pub log: Arc<EventLog>,
    pub config: CourtConfig,
    pub discord: Arc<DiscordClient>,
    pub session_secret: String,
    pub public_url: String,
    pub blobs: BlobStore,
}

impl AppState {
    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }
}

#[derive(Debug, Clone)]
pub struct JudgeOptions {
    pub bind: String,
    pub data_dir: PathBuf,
    pub config: CourtConfig,
    pub session_secret: String,
    pub public_url: String,
    pub discord: DiscordEnv,
}

impl JudgeOptions {
    pub fn from_env() -> Result<Self, String> {
        let config_path =
            std::env::var("JUDGE_CONFIG").unwrap_or_else(|_| "config.json".to_string());
        let config =
            CourtConfig::from_path(&config_path).map_err(|e| format!("load {config_path}: {e}"))?;
        let public_url = std::env::var("JUDGE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let mut discord = DiscordEnv::from_env()?;
        if discord.redirect_uri.is_empty() {
            discord.redirect_uri =
                format!("{}/auth/discord/callback", public_url.trim_end_matches('/'));
        }
        Ok(Self {
            bind: std::env::var("JUDGE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            data_dir: std::env::var("JUDGE_DATA_DIR")
                .unwrap_or_else(|_| "data".to_string())
                .into(),
            config,
            session_secret: std::env::var("JUDGE_SESSION_SECRET")
                .unwrap_or_else(|_| "dev-session-secret-change-me".to_string()),
            public_url,
            discord,
        })
    }
}

pub struct JudgeServer {
    pub addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
}

impl JudgeServer {
    pub fn abort(&self) {
        self.handle.abort();
    }
}

pub async fn boot_state(opts: &JudgeOptions) -> Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&opts.data_dir)?;
    let log_path = opts.data_dir.join("events.jsonl");
    let log = EventLog::new(log_path);
    let (events, bad) = log.load_all().await?;
    if !bad.is_empty() {
        tracing::warn!(n = bad.len(), "skipped corrupt event log lines");
    }
    let mut gov = GovState::new();
    for ev in events {
        gov.replay_accepted(ev)
            .map_err(|e| format!("replay: {e}"))?;
    }
    let discord = DiscordClient::new(opts.discord.clone());
    let state = AppState {
        gov: Arc::new(RwLock::new(gov)),
        log: Arc::new(log),
        config: opts.config.clone(),
        discord: Arc::new(discord),
        session_secret: opts.session_secret.clone(),
        public_url: opts.public_url.clone(),
        blobs: BlobStore::from_env(&opts.data_dir, &opts.public_url),
    };
    if let Err(e) = sync_roster(&state).await {
        tracing::warn!("roster sync skipped: {e}");
    }
    Ok(state)
}

pub async fn serve_judge(opts: JudgeOptions) -> Result<JudgeServer, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(&opts.bind).await?;
    serve_on(listener, opts).await
}

pub async fn serve_on(
    listener: TcpListener,
    opts: JudgeOptions,
) -> Result<JudgeServer, Box<dyn std::error::Error>> {
    let addr = listener.local_addr()?;
    let state = boot_state(&opts).await?;
    let app = router(state);
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(JudgeServer { addr, handle })
}

pub async fn commit(state: &AppState, ev: Event) -> Result<(), AppError> {
    {
        let mut g = state.gov.write().await;
        g.submit(ev.clone())?;
    }
    state.log.append(&ev).await?;
    Ok(())
}

pub async fn sync_roster(state: &AppState) -> Result<(), AppError> {
    let members = state
        .discord
        .list_members(&state.config.guild_id)
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let roster = roster_from_members(&state.config, &members);
    commit(
        state,
        Event::RosterSynced {
            ts: now_ms(),
            members: roster,
        },
    )
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Reject(#[from] Reject),
    #[error("log: {0}")]
    Log(#[from] crate::event_log::EventLogError),
    #[error("{0}")]
    Discord(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            AppError::NotFound => axum::http::StatusCode::NOT_FOUND,
            AppError::Unauthorized => axum::http::StatusCode::UNAUTHORIZED,
            AppError::BadRequest(_) | AppError::Reject(_) => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}
