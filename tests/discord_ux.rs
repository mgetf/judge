//! Discord is the UX: open a case in #docket, act in the case channel.

mod common;

use std::time::Duration;

use serde_json::json;

async fn wait_channel(stack: &common::Stack, name: &str) {
    for _ in 0..100 {
        if stack.mock.channel_names().iter().any(|n| n == name) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "missing channel {name}: {:?}",
        stack.mock.channel_names()
    );
}

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
    let docket_overwrites = stack
        .mock
        .channel_overwrites("docket")
        .expect("docket overwrites");
    let docket_rows = docket_overwrites.as_array().expect("overwrite list");
    assert!(
        docket_rows.iter().any(|r| r["id"] == "000000000000000000"
            && r["deny"] == judge::ticket::VIEW_CHANNEL.to_string()),
        "docket hides @everyone: {docket_overwrites}"
    );
    assert!(
        docket_rows.iter().any(|r| r["id"] == "111111111111111111"),
        "docket allows franchise roles: {docket_overwrites}"
    );

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
    wait_channel(&stack, "case-cheat-1").await;
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

    assert!(client
        .post(format!("{}/app/attach", stack.mock.base_url))
        .form(&[
            ("as_user", "100"),
            ("channel_id", case_ch.as_str()),
            ("filename", "demo.dem"),
            ("url", "https://mge.tf/demos/abc"),
        ])
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

    wait_channel(&stack, "closed-case-cheat-1").await;

    let mut archived = None;
    for _ in 0..100 {
        let resp = client
            .get(format!("{}/cases/case-cheat-1", stack.judge_url))
            .send()
            .await
            .unwrap();
        let html = resp.text().await.unwrap();
        if html.contains("data-testid=\"chat-archive\"") && html.contains("demo.dem") {
            archived = Some(html);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let html = archived.expect("closed case shows the archived channel");
    assert!(html.contains("https://mge.tf/demos/abc"), "{html}");
    assert!(!html.contains("data-testid=\"evidence-"), "{html}");

    let mut transcript = None;
    for _ in 0..100 {
        let resp = client
            .get(format!("{}/cases/case-cheat-1/transcript", stack.judge_url))
            .send()
            .await
            .unwrap();
        if resp.status() == 200 {
            transcript = Some(resp.text().await.unwrap());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let html = transcript.expect("transcript saved after close");
    assert!(html.contains("Transcript case-cheat-1"), "{html}");
    assert!(html.contains("demo.dem"), "{html}");

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
