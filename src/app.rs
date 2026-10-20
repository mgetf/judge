use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::action::Action;
use crate::bot::{self, DiscordBindings};
use crate::clock::now_ms;
use crate::config::CourtConfig;
use crate::discord::{roster_from_members, DiscordClient, DiscordEnv};
use crate::event_log::{EventLog, LogLine};
use crate::events::Event;
use crate::http::router;
use crate::ids::{unique_slug, CaseId, OutcomeId};
use crate::live::Live;
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
    pub live: Live,
    pub bindings: Arc<RwLock<DiscordBindings>>,
    pub bindings_path: PathBuf,
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
        let session_secret = std::env::var("JUDGE_SESSION_SECRET").unwrap_or_default();
        check_production_secrets(&public_url, &session_secret, &discord)?;
        let session_secret = if session_secret.is_empty() {
            tracing::warn!("JUDGE_SESSION_SECRET unset; using a dev secret (local only)");
            "dev-session-secret-change-me".to_string()
        } else {
            session_secret
        };
        Ok(Self {
            bind: std::env::var("JUDGE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            data_dir: std::env::var("JUDGE_DATA_DIR")
                .unwrap_or_else(|_| "data".to_string())
                .into(),
            config,
            session_secret,
            public_url,
            discord,
        })
    }
}

/// A public (https) deployment must not fall back to dev defaults: a known
/// session secret lets anyone forge a chief's cookie, and a missing Discord
/// public key lets anyone post forged interactions as any member.
pub fn check_production_secrets(
    public_url: &str,
    session_secret: &str,
    discord: &DiscordEnv,
) -> Result<(), String> {
    if !public_url.starts_with("https://") {
        return Ok(());
    }
    if session_secret.len() < 32 {
        return Err(
            "JUDGE_SESSION_SECRET must be set (32+ chars) when JUDGE_PUBLIC_URL is https".into(),
        );
    }
    if discord.bot_token.is_some() && discord.public_key.is_none() {
        return Err(
            "DISCORD_PUBLIC_KEY must be set when the bot is enabled on an https deployment".into(),
        );
    }
    Ok(())
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
    for line in events {
        match line {
            LogLine::Accepted(ev) => gov
                .replay_accepted(ev)
                .map_err(|e| format!("replay: {e}"))?,
            LogLine::Rejected { rejected, reason } => gov.replay_rejected(rejected, reason),
        }
    }
    let discord = DiscordClient::new(opts.discord.clone());
    let bindings_path = opts.data_dir.join("discord.json");
    let bindings = DiscordBindings::load(&bindings_path);
    let state = AppState {
        gov: Arc::new(RwLock::new(gov)),
        log: Arc::new(log),
        config: opts.config.clone(),
        discord: Arc::new(discord),
        session_secret: opts.session_secret.clone(),
        public_url: opts.public_url.clone(),
        live: Live::new(),
        bindings: Arc::new(RwLock::new(bindings)),
        bindings_path,
    };
    if let Err(e) = sync_roster(&state).await {
        tracing::warn!("roster sync skipped: {e}");
    }
    if let Err(e) = bot::ensure_ux(&state).await {
        tracing::warn!("discord ux skipped: {e}");
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

pub fn allocate_ids(gov: &GovState, action: Action) -> Result<Action, String> {
    match action {
        Action::OpenCase {
            id,
            kind,
            hearing,
            brief,
            subject,
            target_case,
        } => {
            let next = unique_slug(id.as_str(), |s| gov.cases.keys().any(|k| k.as_str() == s));
            Ok(Action::OpenCase {
                id: CaseId::parse(next).map_err(|e| e.to_string())?,
                kind,
                hearing,
                brief,
                subject,
                target_case,
            })
        }
        Action::ProposeOutcome {
            case,
            id,
            body,
            enacts_policy,
        } => {
            let next = if let Some(c) = gov.cases.get(&case) {
                unique_slug(id.as_str(), |s| c.outcomes.keys().any(|k| k.as_str() == s))
            } else {
                id.to_string()
            };
            Ok(Action::ProposeOutcome {
                case,
                id: OutcomeId::parse(next).map_err(|e| e.to_string())?,
                body,
                enacts_policy,
            })
        }
        other => Ok(other),
    }
}

pub async fn commit(state: &AppState, ev: Event) -> Result<(), AppError> {
    // Hold the write lock across the append so the file order is the fold
    // order; otherwise two concurrent commits can land out of order and the
    // log stops replaying.
    let (result, seq) = {
        let mut g = state.gov.write().await;
        let result = g.submit(ev.clone());
        let line = match &result {
            Ok(()) => LogLine::Accepted(ev.clone()),
            Err(r) => LogLine::Rejected {
                rejected: ev.clone(),
                reason: r.to_string(),
            },
        };
        if let Err(e) = state.log.append(&line).await {
            tracing::error!("event log append failed; memory is ahead of disk: {e}");
            return Err(e.into());
        }
        (result, g.attempts.last().map(|a| a.seq).unwrap_or(0))
    };
    state.live.publish(seq);
    result?;
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = bot::after_commit(&state, &ev).await {
            tracing::warn!("discord live view: {e}");
        }
    });
    Ok(())
}

pub async fn sync_roster(state: &AppState) -> Result<(), AppError> {
    let members = state
        .discord
        .list_members(&state.config.guild_id)
        .await
        .map_err(|e| AppError::Discord(e.to_string()))?;
    let roster = roster_from_members(&state.config, &members);
    if roster_unchanged(&*state.gov.read().await, &roster) {
        return Ok(());
    }
    commit(
        state,
        Event::RosterSynced {
            ts: now_ms(),
            members: roster,
        },
    )
    .await
}

/// True when folding this roster would change nothing, so logins and boots
/// don't append a full member list to the log every time.
fn roster_unchanged(gov: &GovState, roster: &[crate::events::RosterMember]) -> bool {
    let key = |id: &str, name: &str, seat: &crate::types::Seat, w: u64, roles: &[String]| {
        let mut roles = roles.to_vec();
        roles.sort();
        (
            id.to_string(),
            name.to_string(),
            format!("{seat:?}"),
            w,
            roles,
        )
    };
    let mut want: Vec<_> = roster
        .iter()
        .map(|m| {
            key(
                m.id.as_str(),
                &m.display_name,
                &m.seat,
                m.weight as u64,
                &m.discord_role_ids,
            )
        })
        .collect();
    let mut have: Vec<_> = gov
        .principals
        .values()
        .filter_map(|p| {
            let seat = p.seat.as_ref()?;
            Some(key(
                p.id.as_str(),
                &p.display_name,
                seat,
                p.weight as u64,
                &p.discord_role_ids,
            ))
        })
        .collect();
    want.sort();
    have.sort();
    want == have
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PrincipalId;
    use crate::state::Fold;

    fn env(bot: bool, key: bool) -> DiscordEnv {
        DiscordEnv {
            client_id: "c".into(),
            client_secret: "s".into(),
            redirect_uri: String::new(),
            api_base: "http://127.0.0.1:1/api/v10".into(),
            authorize_url: "http://127.0.0.1:1/oauth2/authorize".into(),
            bot_token: bot.then(|| "t".into()),
            public_key: key.then(|| "00".into()),
            application_id: "c".into(),
        }
    }

    #[test]
    fn https_refuses_dev_secrets() {
        let strong = "x".repeat(32);
        assert!(check_production_secrets("http://127.0.0.1:8080", "", &env(true, false)).is_ok());
        assert!(check_production_secrets("https://court.example", "", &env(false, false)).is_err());
        assert!(
            check_production_secrets("https://court.example", &strong, &env(true, false)).is_err()
        );
        assert!(
            check_production_secrets("https://court.example", &strong, &env(true, true)).is_ok()
        );
    }

    #[tokio::test]
    async fn rejected_attempts_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let opts = JudgeOptions {
            bind: "127.0.0.1:0".into(),
            data_dir: dir.path().into(),
            config: crate::testing::example_config(),
            session_secret: "s".into(),
            public_url: "http://127.0.0.1".into(),
            discord: env(false, false),
        };
        let state = boot_state(&opts).await.unwrap();
        let rando = PrincipalId::parse("rando").unwrap();
        commit(
            &state,
            Event::PrincipalSeen {
                ts: 1,
                id: rando.clone(),
                display_name: "rando".into(),
            },
        )
        .await
        .unwrap();
        let open = Action::CloseCase {
            case: CaseId::parse("case-x").unwrap(),
        };
        assert!(commit(&state, open.into_event(rando, 2)).await.is_err());

        let again = boot_state(&opts).await.unwrap();
        let g = again.gov.read().await;
        assert_eq!(g.attempts.len(), 2);
        assert!(matches!(g.attempts[0].fold, Fold::Accepted));
        match &g.attempts[1].fold {
            Fold::Rejected(r) => assert!(r.to_string().contains("not seated"), "{r}"),
            other => panic!("expected rejected, got {other:?}"),
        }
    }
}
