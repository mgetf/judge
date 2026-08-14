use judge::mock_discord::{serve_mock_discord, MockDiscordConfig, MockUser};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = MockDiscordConfig::default();
    if let Ok(raw) = std::env::var("MOCK_DISCORD_USERS_JSON") {
        cfg.users = serde_json::from_str::<Vec<MockUser>>(&raw)?;
    }
    if let Ok(id) = std::env::var("MOCK_DISCORD_CLIENT_ID") {
        cfg.client_id = id;
    }
    if let Ok(secret) = std::env::var("MOCK_DISCORD_CLIENT_SECRET") {
        cfg.client_secret = secret;
    }
    if let Ok(guild) = std::env::var("MOCK_DISCORD_GUILD_ID") {
        cfg.guild_id = guild;
    }
    let mock = serve_mock_discord(cfg).await?;
    eprintln!("mock discord {}", mock.base_url);
    eprintln!("  DISCORD_AUTHORIZE_URL={}", mock.authorize_url());
    eprintln!("  DISCORD_API_BASE={}", mock.api_base());
    tokio::signal::ctrl_c().await?;
    mock.abort();
    Ok(())
}
