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
    let overwrites = stack
        .mock
        .channel_overwrites("case-cheat-1")
        .expect("ticket overwrites");
    let rows = overwrites.as_array().expect("overwrite list");
    assert!(
        rows.iter().any(|r| r["id"] == "000000000000000000"
            && r["deny"] == judge::ticket::VIEW_CHANNEL.to_string()),
        "ticket hides @everyone: {overwrites}"
    );
    assert!(
        rows.iter().any(|r| r["id"] == "100" && r["type"] == 1),
        "opener can see the ticket: {overwrites}"
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

    for (id, body) in [
        ("cheat-ban", "Permanent cheat ban"),
        ("no-action", "Not convinced"),
    ] {
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

    let add = json!({
        "type": 2,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": {
            "name": "add",
            "options": [{ "name": "user", "type": 6, "value": "200" }]
        }
    });
    let added = client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&add)
        .send()
        .await
        .unwrap();
    assert!(
        added.status().is_success(),
        "{}",
        added.text().await.unwrap()
    );
    let after_add = stack
        .mock
        .channel_overwrites("case-cheat-1")
        .expect("overwrites after add");
    assert!(
        after_add
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "200"),
        "added user 200: {after_add}"
    );

    let ask = json!({
        "type": 3,
        "channel_id": case_ch,
        "guild_id": "000000000000000000",
        "member": {
            "roles": ["111111111111111111"],
            "user": { "id": "100", "username": "tommyy", "global_name": "tommyy" }
        },
        "data": { "custom_id": "go:closerequest:case-cheat-1", "component_type": 2 }
    });
    let asked = client
        .post(format!("{}/discord/interactions", stack.judge_url))
        .json(&ask)
        .send()
        .await
        .unwrap();
    let asked_body = asked.text().await.unwrap();
    assert!(
        asked_body.contains("Are you sure you want to close this ticket"),
        "{asked_body}"
    );
    assert!(asked_body.contains("Confirm Close"), "{asked_body}");
    assert!(asked_body.contains("Cancel Close"), "{asked_body}");

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
    assert!(
        closed.status().is_success(),
        "{}",
        closed.text().await.unwrap()
    );

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
    assert!(
        case_page.contains("data-testid=\"evidence-demo-stv\""),
        "{case_page}"
    );
    assert!(
        case_page.contains("https://mge.tf/demos/abc"),
        "{case_page}"
    );
    assert!(
        case_page.contains("/cases/case-cheat-1/transcript"),
        "{case_page}"
    );

    let transcript = client
        .get(format!("{}/cases/case-cheat-1/transcript", stack.judge_url))
        .send()
        .await
        .unwrap();
    assert_eq!(transcript.status(), 200);
    let html = transcript.text().await.unwrap();
    assert!(html.contains("Transcript case-cheat-1"), "{html}");

    let names = stack.mock.channel_names();
    assert!(
        names.iter().any(|n| n == "closed-case-cheat-1"),
        "closed ticket should be renamed: {names:?}"
    );

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
