//! In-process judge + mock Discord. Used by examples and integration tests.

use std::path::PathBuf;

use tokio::net::TcpListener;

use crate::app::{serve_on, JudgeOptions, JudgeServer};
use crate::config::CourtConfig;
use crate::discord::DiscordEnv;
use crate::mock_discord::{serve_mock_discord, MockDiscord, MockDiscordConfig};

pub struct Stack {
    pub mock: MockDiscord,
    pub judge: JudgeServer,
    pub judge_url: String,
    _data: tempfile::TempDir,
}

impl Stack {
    pub fn abort(&self) {
        self.judge.abort();
        self.mock.abort();
    }
}

pub fn example_config() -> CourtConfig {
    CourtConfig::from_json(
        r#"{
          "guild_id": "000000000000000000",
          "owner_discord_id": "100",
          "roles": {
            "111111111111111111": { "seat": "chief", "weight": 3 },
            "222222222222222222": { "seat": "justice", "weight": 1 },
            "333333333333333333": { "seat": "clerk", "weight": 0 }
          }
        }"#,
    )
    .unwrap()
}

pub async fn start_stack() -> Result<Stack, Box<dyn std::error::Error>> {
    let mock = serve_mock_discord(MockDiscordConfig::default()).await?;
    let data = tempfile::tempdir()?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let public_url = format!("http://{addr}");
    let opts = JudgeOptions {
        bind: addr.to_string(),
        data_dir: PathBuf::from(data.path()),
        config: example_config(),
        session_secret: "test-session-secret".into(),
        public_url: public_url.clone(),
        discord: DiscordEnv {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_uri: format!("{public_url}/auth/discord/callback"),
            api_base: mock.api_base(),
            authorize_url: mock.authorize_url(),
            bot_token: Some("mock-bot".into()),
            public_key: None,
            application_id: "test-client".into(),
        },
    };
    let judge = serve_on(listener, opts).await?;
    mock.set_interactions_url(format!("{public_url}/discord/interactions"));
    Ok(Stack {
        mock,
        judge,
        judge_url: public_url,
        _data: data,
    })
}

/// Log `user_id` in through the real OAuth round trip against the mock and
/// return a client that carries the judge session cookie.
pub async fn login(
    stack: &Stack,
    user_id: &str,
) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
    use reqwest::header::{COOKIE, LOCATION, SET_COOKIE};
    let bare = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let first_cookie = |r: &reqwest::Response| -> Option<String> {
        let raw = r.headers().get(SET_COOKIE)?.to_str().ok()?;
        Some(raw.split(';').next()?.to_string())
    };
    let login = bare
        .get(format!("{}/login", stack.judge_url))
        .send()
        .await?;
    let state_cookie = first_cookie(&login).ok_or("no oauth state cookie")?;
    let loc = login
        .headers()
        .get(LOCATION)
        .ok_or("no login redirect")?
        .to_str()?;
    let state = url::Url::parse(loc)?
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .ok_or("no state param")?;
    let approve = bare
        .get(format!(
            "{}/oauth2/approve?user_id={user_id}&redirect_uri={}/auth/discord/callback&state={state}",
            stack.mock.base_url, stack.judge_url
        ))
        .send()
        .await?;
    let callback = approve
        .headers()
        .get(LOCATION)
        .ok_or("no approve redirect")?
        .to_str()?
        .to_string();
    let cb = bare
        .get(callback)
        .header(COOKIE, state_cookie)
        .send()
        .await?;
    let session = first_cookie(&cb).ok_or("no session cookie")?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(COOKIE, session.parse()?);
    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .build()?)
}
