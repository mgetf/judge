//! Ticket Tool-shaped Discord tickets. A case is a private channel with a
//! pinned ticket message, two-step close, closed category, and transcript.

use serde_json::Value;

use crate::action::Wire;

pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const SEND_MESSAGES: u64 = 1 << 11;
pub const EMBED_LINKS: u64 = 1 << 14;
pub const ATTACH_FILES: u64 = 1 << 15;
pub const READ_HISTORY: u64 = 1 << 16;

pub const TALK: u64 = VIEW_CHANNEL | SEND_MESSAGES | EMBED_LINKS | ATTACH_FILES | READ_HISTORY;
pub const SEE: u64 = VIEW_CHANNEL | READ_HISTORY;

/// `@everyone` deny view; bench roles and named users can talk.
pub fn opened_overwrites(guild_id: &str, support_roles: &[String], users: &[String]) -> Value {
    let mut rows = vec![serde_json::json!({
        "id": guild_id,
        "type": 0,
        "allow": "0",
        "deny": VIEW_CHANNEL.to_string(),
    })];
    for role in support_roles {
        rows.push(serde_json::json!({
            "id": role,
            "type": 0,
            "allow": TALK.to_string(),
            "deny": "0",
        }));
    }
    for user in users {
        rows.push(serde_json::json!({
            "id": user,
            "type": 1,
            "allow": TALK.to_string(),
            "deny": "0",
        }));
    }
    Value::Array(rows)
}

/// Same visibility, no send. Ticket Tool closed-state permissions.
pub fn closed_overwrites(guild_id: &str, support_roles: &[String], users: &[String]) -> Value {
    let mut rows = vec![serde_json::json!({
        "id": guild_id,
        "type": 0,
        "allow": "0",
        "deny": VIEW_CHANNEL.to_string(),
    })];
    for role in support_roles {
        rows.push(serde_json::json!({
            "id": role,
            "type": 0,
            "allow": SEE.to_string(),
            "deny": SEND_MESSAGES.to_string(),
        }));
    }
    for user in users {
        rows.push(serde_json::json!({
            "id": user,
            "type": 1,
            "allow": SEE.to_string(),
            "deny": SEND_MESSAGES.to_string(),
        }));
    }
    Value::Array(rows)
}

pub fn closed_channel_name(open_name: &str) -> String {
    if open_name.starts_with("closed-") {
        open_name.to_string()
    } else {
        let mut s = format!("closed-{open_name}");
        s.truncate(100);
        s
    }
}

pub fn close_ask(case: Option<&str>) -> Value {
    serde_json::json!({
        "type": 4,
        "data": {
            "content": "Are you sure you want to close this ticket?",
            "components": [{
                "type": 1,
                "components": [
                    {
                        "type": 2,
                        "style": 4,
                        "label": "Confirm Close",
                        "custom_id": Wire::custom_id("go", "close", case),
                    },
                    {
                        "type": 2,
                        "style": 2,
                        "label": "Cancel Close",
                        "custom_id": "go:cancelclose",
                    }
                ]
            }]
        }
    })
}

pub fn transcript_html(case_id: &str, messages: &[Value]) -> String {
    let mut body = String::new();
    for m in messages.iter().rev() {
        let author = m
            .pointer("/author/username")
            .and_then(|v| v.as_str())
            .or_else(|| m.get("author").and_then(|v| v.as_str()))
            .unwrap_or("unknown");
        let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
        body.push_str(&format!(
            "<div class=\"msg\" id=\"m-{}\"><span class=\"who\">{}</span><p>{}</p></div>\n",
            esc(id),
            esc(author),
            esc(content)
        ));
        if let Some(embeds) = m.get("embeds").and_then(|v| v.as_array()) {
            for e in embeds {
                if let Some(title) = e.get("title").and_then(|v| v.as_str()) {
                    body.push_str(&format!("<div class=\"embed\"><h3>{}</h3>", esc(title)));
                    if let Some(d) = e.get("description").and_then(|v| v.as_str()) {
                        body.push_str(&format!("<p>{}</p>", esc(d)));
                    }
                    body.push_str("</div>\n");
                }
            }
        }
    }
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Transcript {id}</title>\
<style>body{{font:16px/1.45 sans-serif;max-width:46rem;margin:2rem auto;color:#1c1814}}\
.who{{font-weight:600;color:#1a4d38}}.embed{{border-left:4px solid #5865f2;padding:0.4rem 0.7rem;background:#f3eee4;margin:0.4rem 0}}\
.msg{{margin:0 0 0.8rem}}</style></head><body>\
<h1>Transcript {id}</h1>{body}</body></html>",
        id = esc(case_id),
        body = body
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_name_prefixes() {
        assert_eq!(closed_channel_name("case-cheat-1"), "closed-case-cheat-1");
        assert_eq!(
            closed_channel_name("closed-case-cheat-1"),
            "closed-case-cheat-1"
        );
    }

    #[test]
    fn opened_ticket_hides_everyone() {
        let rows = opened_overwrites("guild", &["role-1".into()], &["100".into()]);
        let arr = rows.as_array().unwrap();
        assert_eq!(arr[0]["id"], "guild");
        assert_eq!(arr[0]["deny"], VIEW_CHANNEL.to_string());
        assert_eq!(arr[1]["id"], "role-1");
        assert_eq!(arr[1]["allow"], TALK.to_string());
        assert_eq!(arr[2]["id"], "100");
        assert_eq!(arr[2]["type"], 1);
    }

    #[test]
    fn close_ask_is_two_step() {
        let v = close_ask(Some("case-cheat-1"));
        assert_eq!(
            v["data"]["content"],
            "Are you sure you want to close this ticket?"
        );
        let btns = &v["data"]["components"][0]["components"];
        assert_eq!(btns[0]["label"], "Confirm Close");
        assert_eq!(btns[0]["custom_id"], "go:close:case-cheat-1");
        assert_eq!(btns[1]["label"], "Cancel Close");
    }
}
