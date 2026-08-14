use std::path::PathBuf;

use judge::app::{serve_on, JudgeOptions, JudgeServer};
use judge::discord::DiscordEnv;
use judge::mock_discord::{serve_mock_discord, MockDiscord, MockDiscordConfig};
use judge::CourtConfig;
use tokio::net::TcpListener;

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
        },
    };
    let judge = serve_on(listener, opts).await?;
    Ok(Stack {
        mock,
        judge,
        judge_url: public_url,
        _data: data,
    })
}
