//! Delta sync client against the Firefly sync service (push/pull, cursor,
//! UUID-keyed merge). See docs/API-CONTRACT.md §3.

use crate::error::{AppError, Result};
use crate::router::resolve_endpoint;
use crate::store::{ConvRow, MemRow, MsgRow};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct RegisterRequest {
    name: String,
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct RegisterResponse {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "userId")]
    user_id: String,
}

#[derive(Serialize)]
struct PushBody {
    conversations: Vec<ConvRow>,
    messages: Vec<MsgRow>,
}

#[derive(Deserialize)]
struct MemoriesSearchResponse {
    #[serde(default)]
    memories: Vec<MemRow>,
}

#[derive(Deserialize)]
pub struct PullResponse {
    #[serde(default)]
    pub conversations: Vec<ConvRow>,
    #[serde(default)]
    pub messages: Vec<MsgRow>,
    #[serde(default)]
    pub memories: Vec<MemRow>,
    pub cursor: String,
}

pub struct Registration {
    pub device_id: String,
    pub user_id: String,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(AppError::from)
}

fn base(endpoint: &str) -> String {
    resolve_endpoint(endpoint).trim_end_matches('/').to_string()
}

async fn ensure_ok(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        Ok(resp)
    } else {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        Err(AppError::Other(format!("sync HTTP {status}: {detail}")))
    }
}

pub async fn register_device(
    endpoint: &str,
    token: &str,
    name: &str,
    user_id: Option<&str>,
) -> Result<Registration> {
    let url = format!("{}/devices/register", base(endpoint));
    let resp = client()?
        .post(&url)
        .bearer_auth(token)
        .json(&RegisterRequest {
            name: name.to_string(),
            user_id: user_id.map(|s| s.to_string()),
        })
        .send()
        .await?;
    let resp = ensure_ok(resp).await?;
    let r: RegisterResponse = resp.json().await?;
    Ok(Registration {
        device_id: r.device_id,
        user_id: r.user_id,
    })
}

pub async fn push(
    endpoint: &str,
    token: &str,
    conversations: Vec<ConvRow>,
    messages: Vec<MsgRow>,
) -> Result<()> {
    let url = format!("{}/sync/push", base(endpoint));
    let resp = client()?
        .post(&url)
        .bearer_auth(token)
        .json(&PushBody {
            conversations,
            messages,
        })
        .send()
        .await?;
    ensure_ok(resp).await?;
    Ok(())
}

pub async fn pull(
    endpoint: &str,
    token: &str,
    since: &str,
    user_id: &str,
) -> Result<PullResponse> {
    let mut url = format!("{}/sync/pull", base(endpoint));
    let mut params: Vec<(&str, &str)> = Vec::new();
    if !since.is_empty() {
        params.push(("since", since));
    }
    if !user_id.is_empty() {
        params.push(("user", user_id));
    }
    if !params.is_empty() {
        let qs = params
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencoding(v)))
            .collect::<Vec<_>>()
            .join("&");
        url = format!("{url}?{qs}");
    }
    let resp = client()?.get(&url).bearer_auth(token).send().await?;
    let resp = ensure_ok(resp).await?;
    let body: PullResponse = resp.json().await?;
    Ok(body)
}

fn memories_search_url(base_url: &str, query: &str, user_id: &str, k: u32) -> String {
    let mut params = vec![format!("q={}", urlencoding(query)), format!("k={k}")];
    if !user_id.is_empty() {
        params.push(format!("user={}", urlencoding(user_id)));
    }
    format!("{base_url}/memories/search?{}", params.join("&"))
}

/// GET /memories/search — semantic search over the user's distilled memories,
/// best-first. See docs/API-CONTRACT.md §3.6. Best-effort at the call site:
/// callers should treat any error (incl. `501 not configured`) as "no memories".
pub async fn search_memories(
    endpoint: &str,
    token: &str,
    query: &str,
    user_id: &str,
    k: u32,
) -> Result<Vec<MemRow>> {
    let url = memories_search_url(&base(endpoint), query, user_id, k);
    let resp = client()?.get(&url).bearer_auth(token).send().await?;
    let resp = ensure_ok(resp).await?;
    let body: MemoriesSearchResponse = resp.json().await?;
    Ok(body.memories)
}

/// Minimal percent-encoding for the few query values we send (ISO timestamps,
/// UUIDs). Encodes everything that isn't an unreserved URL char.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ConvRow, MsgRow};

    #[test]
    fn memories_url_encodes_query_and_omits_empty_user() {
        let u = memories_search_url("http://firefly:8788", "what is my dog's name?", "", 8);
        assert_eq!(
            u,
            "http://firefly:8788/memories/search?q=what%20is%20my%20dog%27s%20name%3F&k=8"
        );
    }

    #[test]
    fn memories_url_includes_user_when_present() {
        let u = memories_search_url("http://firefly:8788", "hi", "user-1", 8);
        assert_eq!(u, "http://firefly:8788/memories/search?q=hi&k=8&user=user-1");
    }

    #[test]
    fn register_request_is_camel_case() {
        let body = serde_json::to_value(RegisterRequest {
            name: "macbook".into(),
            user_id: None,
        })
        .unwrap();
        assert_eq!(body["name"], "macbook");
        // omitted when None
        assert!(body.get("userId").is_none());
    }

    #[test]
    fn register_response_reads_camel_case() {
        let r: RegisterResponse =
            serde_json::from_str(r#"{"deviceId":"d-1","userId":"u-1"}"#).unwrap();
        assert_eq!(r.device_id, "d-1");
        assert_eq!(r.user_id, "u-1");
    }

    #[test]
    fn push_body_rows_are_snake_case() {
        let body = serde_json::to_value(PushBody {
            conversations: vec![ConvRow {
                id: "c1".into(),
                user_id: "u1".into(),
                title: Some("t".into()),
                created_at: "2026-01-01T00:00:00.000Z".into(),
                updated_at: "2026-01-01T00:00:00.000Z".into(),
            }],
            messages: vec![MsgRow {
                id: "m1".into(),
                conversation_id: "c1".into(),
                role: "user".into(),
                content: "hi".into(),
                model: None,
                created_at: "2026-01-01T00:00:00.000Z".into(),
            }],
        })
        .unwrap();
        assert_eq!(body["conversations"][0]["user_id"], "u1");
        assert_eq!(body["messages"][0]["conversation_id"], "c1");
    }
}
