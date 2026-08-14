use serde::Deserialize;

use crate::config::CourtConfig;
use crate::events::RosterMember;
use crate::ids::PrincipalId;
use crate::types::Seat;

#[derive(Debug, Clone)]
pub struct DiscordEnv {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    /// Default `https://discord.com/api/v10`. Point at the mock in tests.
    pub api_base: String,
    /// Default `https://discord.com/oauth2/authorize`. Point at the mock in tests.
    pub authorize_url: String,
    pub bot_token: Option<String>,
}

impl DiscordEnv {
    pub fn from_env() -> Result<Self, String> {
        let client_id = std::env::var("DISCORD_CLIENT_ID")
            .map_err(|_| "DISCORD_CLIENT_ID is required".to_string())?;
        let client_secret = std::env::var("DISCORD_CLIENT_SECRET")
            .map_err(|_| "DISCORD_CLIENT_SECRET is required".to_string())?;
        let public_url = std::env::var("JUDGE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let redirect_uri = std::env::var("DISCORD_REDIRECT_URI").unwrap_or_else(|_| {
            format!("{}/auth/discord/callback", public_url.trim_end_matches('/'))
        });
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            api_base: std::env::var("DISCORD_API_BASE")
                .unwrap_or_else(|_| "https://discord.com/api/v10".to_string()),
            authorize_url: std::env::var("DISCORD_AUTHORIZE_URL")
                .unwrap_or_else(|_| "https://discord.com/oauth2/authorize".to_string()),
            bot_token: std::env::var("DISCORD_BOT_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }

    pub fn authorize_redirect(&self, state: &str) -> String {
        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.authorize_url,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode("identify guilds.members.read"),
            urlencoding::encode(state),
        )
    }
}

#[derive(Debug, Clone)]
pub struct DiscordClient {
    env: DiscordEnv,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
}

impl DiscordUser {
    pub fn display_name(&self) -> String {
        self.global_name
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.username.clone())
    }
}

#[derive(Debug, Clone)]
pub struct DiscordMember {
    pub user: DiscordUser,
    pub roles: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscordError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("discord: {0}")]
    Api(String),
    #[error("missing bot token")]
    NoBot,
}

impl DiscordClient {
    pub fn new(env: DiscordEnv) -> Self {
        Self {
            env,
            http: reqwest::Client::new(),
        }
    }

    pub fn env(&self) -> &DiscordEnv {
        &self.env
    }

    pub async fn exchange_code(&self, code: &str) -> Result<String, DiscordError> {
        let url = format!("{}/oauth2/token", self.env.api_base.trim_end_matches('/'));
        let resp = self
            .http
            .post(url)
            .form(&[
                ("client_id", self.env.client_id.as_str()),
                ("client_secret", self.env.client_secret.as_str()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.env.redirect_uri.as_str()),
            ])
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(DiscordError::Api(format!("token {status}: {body}")));
        }
        let tok: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| DiscordError::Api(format!("token json: {e}")))?;
        Ok(tok.access_token)
    }

    pub async fn me(&self, access_token: &str) -> Result<DiscordUser, DiscordError> {
        let url = format!("{}/users/@me", self.env.api_base.trim_end_matches('/'));
        let resp = self.http.get(url).bearer_auth(access_token).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(DiscordError::Api(format!("@me {status}: {body}")));
        }
        let u: UserJson =
            serde_json::from_str(&body).map_err(|e| DiscordError::Api(format!("@me json: {e}")))?;
        Ok(u.into())
    }

    pub async fn my_member(
        &self,
        access_token: &str,
        guild_id: &str,
    ) -> Result<DiscordMember, DiscordError> {
        let url = format!(
            "{}/users/@me/guilds/{guild_id}/member",
            self.env.api_base.trim_end_matches('/')
        );
        let resp = self.http.get(url).bearer_auth(access_token).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(DiscordError::Api(format!("member {status}: {body}")));
        }
        let m: MemberJson = serde_json::from_str(&body)
            .map_err(|e| DiscordError::Api(format!("member json: {e}")))?;
        Ok(m.into())
    }

    pub async fn list_members(&self, guild_id: &str) -> Result<Vec<DiscordMember>, DiscordError> {
        let token = self.env.bot_token.as_ref().ok_or(DiscordError::NoBot)?;
        let url = format!(
            "{}/guilds/{guild_id}/members?limit=1000",
            self.env.api_base.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(DiscordError::Api(format!("members {status}: {body}")));
        }
        let rows: Vec<MemberJson> = serde_json::from_str(&body)
            .map_err(|e| DiscordError::Api(format!("members json: {e}")))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

pub fn roster_from_members(cfg: &CourtConfig, members: &[DiscordMember]) -> Vec<RosterMember> {
    let mut out = Vec::new();
    for m in members {
        let Some((seat, weight)) = cfg.resolve(&m.roles) else {
            continue;
        };
        let Ok(id) = PrincipalId::parse(&m.user.id) else {
            continue;
        };
        let seat = match seat {
            Seat::Clerk { .. } => Seat::Clerk { model: None },
            other => other,
        };
        out.push(RosterMember {
            id,
            display_name: m.user.display_name(),
            seat,
            weight,
            discord_role_ids: m.roles.clone(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct UserJson {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
}

impl From<UserJson> for DiscordUser {
    fn from(u: UserJson) -> Self {
        Self {
            id: u.id,
            username: u.username,
            global_name: u.global_name,
        }
    }
}

#[derive(Deserialize)]
struct MemberJson {
    user: UserJson,
    #[serde(default)]
    roles: Vec<String>,
}

impl From<MemberJson> for DiscordMember {
    fn from(m: MemberJson) -> Self {
        Self {
            user: m.user.into(),
            roles: m.roles,
        }
    }
}
