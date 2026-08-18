mod common;

use judge::discord::DiscordClient;

#[tokio::test(flavor = "multi_thread")]
async fn mock_discord_oauth_and_roster() {
    let stack = common::start_stack().await.expect("stack");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let login = client
        .get(format!("{}/login", stack.judge_url))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), reqwest::StatusCode::SEE_OTHER);
    let loc = login
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        loc.starts_with(&stack.mock.authorize_url()),
        "login redirects at mock authorize: {loc}"
    );

    let approve = client
        .get(format!(
            "{}/oauth2/approve?user_id=100&redirect_uri={}/auth/discord/callback&state=x",
            stack.mock.base_url, stack.judge_url
        ))
        .send()
        .await
        .unwrap();
    let cb = approve
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cb.contains("code=code-100"), "{cb}");

    let discord = DiscordClient::new(judge::discord::DiscordEnv {
        client_id: "test-client".into(),
        client_secret: "test-secret".into(),
        redirect_uri: format!("{}/auth/discord/callback", stack.judge_url),
        api_base: stack.mock.api_base(),
        authorize_url: stack.mock.authorize_url(),
        bot_token: Some("mock-bot".into()),
        public_key: None,
        application_id: "test-client".into(),
    });
    let token = discord.exchange_code("code-100").await.unwrap();
    let me = discord.me(&token).await.unwrap();
    assert_eq!(me.username, "tommyy");
    let members = discord.list_members("000000000000000000").await.unwrap();
    assert!(members.iter().any(|m| m.user.username == "tommyy"));
    assert!(members.iter().any(|m| m.user.username == "abood"));

    let home = client
        .get(&stack.judge_url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home.contains("data-testid=\"bench\""));
    assert!(home.contains("tommyy"));
    assert!(home.contains("chief"));

    stack.abort();
}
