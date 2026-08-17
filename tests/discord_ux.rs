//! Discord is the UX: open a case in #docket, act in the case channel.

mod common;

use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn case_channel_is_the_chat_and_view_live_updates() {
    let stack = common::start_stack().await.expect("stack");
    let client = reqwest::Client::new();

    let home = client
        .get(format!("{}/app?as_user=100", stack.mock.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home.contains("data-testid=\"guild\""), "{home}");
    assert!(home.contains("data-testid=\"channel-docket\""), "{home}");
    assert!(home.contains("data-testid=\"open-case\""), "{home}");

    let names = stack.mock.channel_names();
    assert!(names.iter().any(|n| n == "docket"), "{names:?}");

    let docket_html = client
        .get(format!("{}/see?cite=docket", stack.judge_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(docket_html.contains("data-testid=\"live-root\""));
    assert!(docket_html.contains("data-live-root"));

    let open = json!({
        "type": 5,
        "channel_id": channel_id_from_html(&home, "docket").expect("docket channel"),
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": {
            "custom_id": "do:case",
            "components": [
                row("id", "case-cheat-1"),
                row("kind", "record"),
                row("hearing", "none"),
                row("subject", "76561198000000000"),
                row("brief", "STV aimbot on mge_training")
            ]
        }
    });
    let resp = client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&open)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "{}", resp.text().await.unwrap());

    let names = stack.mock.channel_names();
    assert!(
        names.iter().any(|n| n == "case-cheat-1"),
        "case channel created: {names:?}"
    );

    let case_page = client
        .get(format!("{}/cases/case-cheat-1", stack.judge_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(case_page.contains("data-testid=\"case-title\""));
    assert!(case_page.contains("intake"));

    let guild = client
        .get(format!("{}/app?as_user=100", stack.mock.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let case_ch = channel_id_from_html(&guild, "case-cheat-1").expect("case channel id");

    let evidence = json!({
        "type": 2,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": {
            "name": "evidence",
            "options": [
                { "name": "file", "type": 11, "value": "att-1" },
                { "name": "label", "type": 3, "value": "STV demo" },
                { "name": "id", "type": 3, "value": "demo-stv" }
            ],
            "resolved": {
                "attachments": {
                    "att-1": {
                        "id": "att-1",
                        "filename": "demo.dem",
                        "url": "https://mge.tf/demos/abc"
                    }
                }
            }
        }
    });
    assert!(client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&evidence)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    for (id, body) in [("cheat-ban", "Permanent cheat ban"), ("no-action", "Not convinced")] {
        let outcome = json!({
            "type": 5,
            "channel_id": case_ch,
            "guild_id": "000000000000000000",
            "member": {
                "roles": ["111111111111111111"],
                "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
            },
            "data": {
                "custom_id": format!("do:outcome:case-cheat-1"),
                "components": [row("id", id), row("body", body)]
            }
        });
        assert!(client
            .post(format!("{}/discord/interactions", stack.judge_url))
            .json(&outcome)
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
    }

    let deliberate = json!({
        "type": 3,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": { "custom_id": "go:deliberate:case-cheat-1", "component_type": 2 }
    });
    assert!(client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&deliberate)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    let vote = json!({
        "type": 5,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": {
            "custom_id": "do:vote:case-cheat-1",
            "components": [row("outcome", "cheat-ban"), row("reason", "Demo is unambiguous.")]
        }
    });
    assert!(client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&vote)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    let close = json!({
        "type": 3,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": { "custom_id": "go:close:case-cheat-1", "component_type": 2 }
    });
    let closed = client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&close)
        .send()
        .await
        .unwrap();
    assert!(closed.status().is_success(), "{}", closed.text().await.unwrap());

    let case_page = client
        .get(format!("{}/cases/case-cheat-1", stack.judge_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(case_page.contains("data-testid=\"verdict\""), "{case_page}");
    assert!(case_page.contains("cheat-ban"), "{case_page}");
    assert!(case_page.contains("data-testid=\"evidence-demo-stv\""), "{case_page}");
    assert!(case_page.contains("https://mge.tf/demos/abc"), "{case_page}");

    stack.abort();
}

fn row(id: &str, value: &str) -> serde_json::Value {
    json!({
        "type": 1,
        "components": [{ "type": 4, "custom_id": id, "value": value }]
    })
}

fn channel_id_from_html(html: &str, name: &str) -> Option<String> {
    let needle = format!("data-testid=\"channel-{name}\"");
    let i = html.find(&needle)?;
    let before = &html[..i];
    let href_key = "channel=";
    let j = before.rfind(href_key)?;
    let rest = &before[j + href_key.len()..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
