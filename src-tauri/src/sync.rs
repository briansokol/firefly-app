//! Delta sync client against the Firefly sync service (push/pull, cursor,
//! UUID-keyed merge). See docs/API-CONTRACT.md §3.

use crate::error::{AppError, Result};
use crate::router::resolve_endpoint;
use crate::store::{ConvRow, MemRow, MsgRow};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---- Auth: signup ----
#[derive(Serialize)]
struct SignupRequest {
    username: String,
    password: String,
    #[serde(rename = "displayName")]
    display_name: String,
}

#[derive(Deserialize)]
struct SignupResponse {
    #[serde(rename = "userId")]
    user_id: String,
    #[allow(dead_code)]
    username: String,
    #[serde(rename = "displayName")]
    display_name: String,
    profile: String,
    #[serde(rename = "litellmKey")]
    litellm_key: String,
    #[serde(rename = "sessionToken")]
    session_token: String,
}

// ---- Auth: login ----
#[derive(Serialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginDeviceRow {
    id: String,
    name: String,
    #[serde(rename = "lastSync", default)]
    last_sync: Option<String>,
}

#[derive(Deserialize)]
struct LoginResponse {
    #[serde(rename = "userId")]
    user_id: String,
    username: String,
    profile: String,
    #[serde(rename = "litellmKey")]
    litellm_key: String,
    #[serde(rename = "sessionToken")]
    session_token: String,
    #[serde(default)]
    devices: Vec<LoginDeviceRow>,
}

// ---- Devices: register / claim (identical response shape) ----
#[derive(Serialize)]
struct DeviceRequest {
    name: String,
}

#[derive(Deserialize)]
struct DeviceResponse {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "deviceToken")]
    device_token: String,
    #[serde(rename = "litellmKey")]
    litellm_key: String,
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

// ---- Public types the commands consume ----
pub struct Auth {
    // The durable user_id + litellm_key are taken from the device-registration
    // response (see commit_device), not the auth response; these mirror the
    // contract's payload but are unused by the current flow.
    #[allow(dead_code)]
    pub user_id: String,
    pub display_name: String, // login has no displayName -> username
    pub profile: String,
    pub session_token: String,
    #[allow(dead_code)]
    pub litellm_key: String,
}

pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub last_sync: Option<String>,
}

pub struct LoginResult {
    pub auth: Auth,
    pub devices: Vec<DeviceInfo>,
}

pub struct DeviceCredentials {
    pub device_id: String,
    pub user_id: String,
    pub device_token: String,
    pub litellm_key: String,
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

/// Pull the server's `{"error":"..."}` message out of a failed-response body,
/// falling back to the raw body when it isn't JSON with an `error` field.
fn extract_error(status: reqwest::StatusCode, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| format!("sync HTTP {status}: {body}"))
}

/// Like `ensure_ok`, but surfaces the server's JSON `error` message verbatim —
/// used for auth/device routes where messages ("username taken",
/// "invalid credentials") drive onboarding UI.
async fn ensure_ok_json(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(AppError::Other(extract_error(status, &body)))
}

/// POST /auth/signup — create an account, returning a session token + per-user
/// LiteLLM key. No device token yet (call `register_device` next). §3.2.
pub async fn signup(
    endpoint: &str,
    username: &str,
    password: &str,
    display_name: &str,
) -> Result<Auth> {
    let url = format!("{}/auth/signup", base(endpoint));
    let resp = client()?
        .post(&url)
        .json(&SignupRequest {
            username: username.to_string(),
            password: password.to_string(),
            display_name: display_name.to_string(),
        })
        .send()
        .await?;
    let r: SignupResponse = ensure_ok_json(resp).await?.json().await?;
    Ok(Auth {
        user_id: r.user_id,
        display_name: r.display_name,
        profile: r.profile,
        session_token: r.session_token,
        litellm_key: r.litellm_key,
    })
}

/// POST /auth/login — verify credentials, returning a fresh session token and
/// the user's existing devices. The response carries no `displayName`, so the
/// username is used in its place. §3.2.
pub async fn login(endpoint: &str, username: &str, password: &str) -> Result<LoginResult> {
    let url = format!("{}/auth/login", base(endpoint));
    let resp = client()?
        .post(&url)
        .json(&LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?;
    let r: LoginResponse = ensure_ok_json(resp).await?.json().await?;
    let devices = r
        .devices
        .into_iter()
        .map(|d| DeviceInfo {
            id: d.id,
            name: d.name,
            last_sync: d.last_sync,
        })
        .collect();
    Ok(LoginResult {
        auth: Auth {
            user_id: r.user_id,
            display_name: r.username,
            profile: r.profile,
            session_token: r.session_token,
            litellm_key: r.litellm_key,
        },
        devices,
    })
}

/// POST /devices — register a brand-new device for the authenticated user.
/// Authorized by a session or device token. §3.2.
pub async fn register_device(endpoint: &str, token: &str, name: &str) -> Result<DeviceCredentials> {
    let url = format!("{}/devices", base(endpoint));
    let resp = client()?
        .post(&url)
        .bearer_auth(token)
        .json(&DeviceRequest {
            name: name.to_string(),
        })
        .send()
        .await?;
    let r: DeviceResponse = ensure_ok_json(resp).await?.json().await?;
    Ok(DeviceCredentials {
        device_id: r.device_id,
        user_id: r.user_id,
        device_token: r.device_token,
        litellm_key: r.litellm_key,
    })
}

/// POST /devices/:id/claim — rotate an existing device entry's token to this
/// install (e.g. after a reinstall). The old token stops working. §3.2.
pub async fn claim_device(
    endpoint: &str,
    token: &str,
    device_id: &str,
) -> Result<DeviceCredentials> {
    let url = format!("{}/devices/{}/claim", base(endpoint), urlencoding(device_id));
    let resp = client()?.post(&url).bearer_auth(token).send().await?;
    let r: DeviceResponse = ensure_ok_json(resp).await?.json().await?;
    Ok(DeviceCredentials {
        device_id: r.device_id,
        user_id: r.user_id,
        device_token: r.device_token,
        litellm_key: r.litellm_key,
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
    fn signup_request_serializes_camel_case() {
        let body = serde_json::to_value(SignupRequest {
            username: "ada".into(),
            password: "pw".into(),
            display_name: "Ada L".into(),
        })
        .unwrap();
        assert_eq!(body["username"], "ada");
        assert_eq!(body["password"], "pw");
        assert_eq!(body["displayName"], "Ada L");
    }

    #[test]
    fn signup_response_reads_fields() {
        let r: SignupResponse = serde_json::from_str(
            r#"{"userId":"u1","username":"ada","displayName":"Ada L","profile":"kid",
                "litellmKey":"lk","sessionToken":"st","sessionExpiresAt":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(r.user_id, "u1");
        assert_eq!(r.display_name, "Ada L");
        assert_eq!(r.profile, "kid");
        assert_eq!(r.litellm_key, "lk");
        assert_eq!(r.session_token, "st");
    }

    #[test]
    fn login_request_serializes() {
        let body = serde_json::to_value(LoginRequest {
            username: "ada".into(),
            password: "pw".into(),
        })
        .unwrap();
        assert_eq!(body["username"], "ada");
        assert_eq!(body["password"], "pw");
    }

    #[test]
    fn login_response_reads_devices_and_omits_display_name() {
        let r: LoginResponse = serde_json::from_str(
            r#"{"userId":"u1","username":"ada","profile":"adult","litellmKey":"lk",
                "sessionToken":"st","sessionExpiresAt":"2026-07-01T00:00:00Z",
                "devices":[{"id":"d1","name":"MacBook","lastSync":"2026-06-01T00:00:00Z"},
                           {"id":"d2","name":"iPad","lastSync":null}]}"#,
        )
        .unwrap();
        assert_eq!(r.user_id, "u1");
        assert_eq!(r.username, "ada");
        assert_eq!(r.devices.len(), 2);
        assert_eq!(r.devices[0].name, "MacBook");
        assert_eq!(r.devices[1].last_sync, None);
    }

    #[test]
    fn login_response_handles_zero_devices() {
        let r: LoginResponse = serde_json::from_str(
            r#"{"userId":"u1","username":"ada","profile":"kid","litellmKey":"lk",
                "sessionToken":"st","sessionExpiresAt":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert!(r.devices.is_empty());
    }

    #[test]
    fn device_request_serializes_name() {
        let body = serde_json::to_value(DeviceRequest { name: "laptop".into() }).unwrap();
        assert_eq!(body["name"], "laptop");
    }

    #[test]
    fn device_response_reads_credentials() {
        let r: DeviceResponse = serde_json::from_str(
            r#"{"deviceId":"d1","userId":"u1","deviceToken":"dt","litellmKey":"lk"}"#,
        )
        .unwrap();
        assert_eq!(r.device_id, "d1");
        assert_eq!(r.user_id, "u1");
        assert_eq!(r.device_token, "dt");
        assert_eq!(r.litellm_key, "lk");
    }

    #[test]
    fn extract_error_prefers_json_error_field() {
        let s = reqwest::StatusCode::CONFLICT;
        assert_eq!(extract_error(s, r#"{"error":"username taken"}"#), "username taken");
        assert_eq!(extract_error(s, "boom"), "sync HTTP 409 Conflict: boom");
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
                deleted_at: None,
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
